use crate::protocol::channel::ChannelOpenOptions;
use crate::protocol::message::Message;

use crate::handler::ChannelHandler;

pub struct KdumpInfoHandler;

#[async_trait::async_trait]
impl ChannelHandler for KdumpInfoHandler {
    fn payload_type(&self) -> &str {
        "kdump.info"
    }

    async fn open(&self, channel: &str, _options: &ChannelOpenOptions) -> Vec<Message> {
        let status = collect_kdump_status().await;
        let dumps = list_crash_dumps().await;
        let config = read_kdump_config().await;
        let crashkernel = read_crashkernel_param();

        let info = serde_json::json!({
            "status": status,
            "crashkernel": crashkernel,
            "config": config,
            "dumps": dumps,
        });

        vec![
            Message::Ready { channel: channel.to_string() },
            Message::Data { channel: channel.to_string(), data: info },
            Message::Close { channel: channel.to_string(), problem: None },
        ]
    }

    async fn data(&self, channel: &str, data: &serde_json::Value) -> Vec<Message> {
        let action = data.get("action").and_then(|a| a.as_str()).unwrap_or("");

        let result = match action {
            "read_dump" => {
                let path = data.get("path").and_then(|p| p.as_str()).unwrap_or("");
                read_dump_details(path).await
            }
            "read_dmesg" => {
                let path = data.get("path").and_then(|p| p.as_str()).unwrap_or("");
                read_dmesg_log(path).await
            }
            _ => serde_json::json!({ "ok": false, "error": format!("unknown action: {action}") }),
        };

        vec![Message::Data {
            channel: channel.to_string(),
            data: serde_json::json!({ "type": "response", "action": action, "data": result }),
        }]
    }
}

/// Check kdump service status and whether kernel crash dumping is active.
async fn collect_kdump_status() -> serde_json::Value {
    // Check if kdump-tools or kexec-tools is installed
    let kdump_tools_installed = tokio::fs::metadata("/usr/sbin/kdump-config").await.is_ok();
    let kexec_tools_installed = tokio::fs::metadata("/usr/sbin/makedumpfile").await.is_ok()
        || tokio::fs::metadata("/usr/bin/makedumpfile").await.is_ok();

    // Check kdump service status (Debian: kdump-tools, RHEL: kdump)
    let service_name = if kdump_tools_installed { "kdump-tools" } else { "kdump" };
    let service_active = check_service_active(service_name).await;
    let service_enabled = check_service_enabled(service_name).await;

    // Check if crash kernel is loaded via /sys/kernel/kexec_crash_loaded
    let crash_loaded = std::fs::read_to_string("/sys/kernel/kexec_crash_loaded")
        .ok()
        .map(|s| s.trim() == "1")
        .unwrap_or(false);

    // Check /sys/kernel/kexec_crash_size
    let crash_size = std::fs::read_to_string("/sys/kernel/kexec_crash_size")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);

    // Current kernel version
    let kernel_version = std::fs::read_to_string("/proc/version")
        .ok()
        .and_then(|s| s.split_whitespace().nth(2).map(String::from))
        .unwrap_or_default();

    serde_json::json!({
        "installed": kdump_tools_installed || kexec_tools_installed,
        "service_name": service_name,
        "service_active": service_active,
        "service_enabled": service_enabled,
        "crash_kernel_loaded": crash_loaded,
        "crash_kernel_reserved_bytes": crash_size,
        "kernel_version": kernel_version,
        "kdump_tools": kdump_tools_installed,
        "kexec_tools": kexec_tools_installed,
    })
}

async fn check_service_active(name: &str) -> String {
    tokio::process::Command::new("systemctl")
        .args(["is-active", name])
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".into())
}

async fn check_service_enabled(name: &str) -> String {
    tokio::process::Command::new("systemctl")
        .args(["is-enabled", name])
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".into())
}

/// Read crashkernel= parameter from /proc/cmdline.
fn read_crashkernel_param() -> serde_json::Value {
    let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
    let param = cmdline
        .split_whitespace()
        .find(|s| s.starts_with("crashkernel="))
        .map(|s| s.strip_prefix("crashkernel=").unwrap_or(s))
        .unwrap_or("");

    serde_json::json!({
        "param": param,
        "configured": !param.is_empty(),
    })
}

/// Read kdump configuration file.
async fn read_kdump_config() -> serde_json::Value {
    // Try Debian path first, then RHEL
    let paths = [
        "/etc/default/kdump-tools",
        "/etc/kdump.conf",
        "/etc/kdump/kdump.conf",
    ];

    for path in &paths {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            return serde_json::json!({
                "path": path,
                "content": content,
            });
        }
    }

    serde_json::json!({
        "path": null,
        "content": null,
    })
}

/// List crash dumps from standard directories.
async fn list_crash_dumps() -> serde_json::Value {
    let search_dirs = [
        "/var/crash",
        "/var/lib/kdump",
    ];

    let mut dumps = Vec::new();

    for dir in &search_dirs {
        if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                if let Ok(meta) = entry.metadata().await {
                    if meta.is_dir() {
                        // Crash dump directory (e.g. /var/crash/202603221430/)
                        let dump = scan_crash_dir(&path).await;
                        dumps.push(serde_json::json!({
                            "name": name,
                            "path": path.to_string_lossy(),
                            "type": "directory",
                            "size_bytes": dump.total_size,
                            "files": dump.files,
                            "has_vmcore": dump.has_vmcore,
                            "has_dmesg": dump.has_dmesg,
                            "timestamp": file_timestamp(&meta),
                        }));
                    } else if name.ends_with(".crash")
                        || name.starts_with("vmcore")
                        || name.starts_with("dump.")
                        || name.ends_with(".dmesg")
                    {
                        dumps.push(serde_json::json!({
                            "name": name,
                            "path": path.to_string_lossy(),
                            "type": "file",
                            "size_bytes": meta.len(),
                            "has_vmcore": name.starts_with("vmcore"),
                            "has_dmesg": name.ends_with(".dmesg") || name.ends_with(".txt"),
                            "timestamp": file_timestamp(&meta),
                        }));
                    }
                }
            }
        }
    }

    // Sort by timestamp descending (newest first)
    dumps.sort_by(|a, b| {
        let ta = a.get("timestamp").and_then(|t| t.as_str()).unwrap_or("");
        let tb = b.get("timestamp").and_then(|t| t.as_str()).unwrap_or("");
        tb.cmp(ta)
    });

    serde_json::json!(dumps)
}

struct CrashDirInfo {
    files: Vec<serde_json::Value>,
    total_size: u64,
    has_vmcore: bool,
    has_dmesg: bool,
}

async fn scan_crash_dir(dir: &std::path::Path) -> CrashDirInfo {
    let mut info = CrashDirInfo {
        files: Vec::new(),
        total_size: 0,
        has_vmcore: false,
        has_dmesg: false,
    };

    if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Ok(meta) = entry.metadata().await {
                let size = meta.len();
                info.total_size += size;

                if name.starts_with("vmcore") || name.ends_with(".core") {
                    info.has_vmcore = true;
                }
                if name.contains("dmesg") || name.ends_with(".txt") || name.ends_with(".log") {
                    info.has_dmesg = true;
                }

                info.files.push(serde_json::json!({
                    "name": name,
                    "path": entry.path().to_string_lossy(),
                    "size_bytes": size,
                    "timestamp": file_timestamp(&meta),
                }));
            }
        }
    }

    info
}

fn file_timestamp(meta: &std::fs::Metadata) -> String {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| {
            let secs = d.as_secs();
            // Format as ISO-ish via chrono if available, otherwise epoch
            chrono::DateTime::from_timestamp(secs as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| secs.to_string())
        })
        .unwrap_or_default()
}

/// Read details of a specific dump (first 4KB of dmesg/stack trace).
async fn read_dump_details(path: &str) -> serde_json::Value {
    if !is_valid_dump_path(path) {
        return serde_json::json!({ "ok": false, "error": "invalid path" });
    }

    match tokio::fs::metadata(path).await {
        Ok(meta) => {
            let size = meta.len();
            // For vmcore files, run crash --minimal or just report size
            if path.contains("vmcore") {
                serde_json::json!({
                    "ok": true,
                    "path": path,
                    "size_bytes": size,
                    "type": "vmcore",
                    "note": "Use 'crash' tool for full analysis",
                })
            } else {
                // Read text file (dmesg, log) — limit to 64KB
                let max_read = 65536;
                match tokio::fs::read(path).await {
                    Ok(data) => {
                        let content = if data.len() > max_read {
                            let slice = &data[..max_read];
                            format!("{}\n\n[... truncated at 64KB, total {} bytes ...]",
                                String::from_utf8_lossy(slice), data.len())
                        } else {
                            String::from_utf8_lossy(&data).to_string()
                        };
                        serde_json::json!({
                            "ok": true,
                            "path": path,
                            "size_bytes": size,
                            "type": "text",
                            "content": content,
                        })
                    }
                    Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
                }
            }
        }
        Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
    }
}

/// Read dmesg log associated with a crash dump directory.
async fn read_dmesg_log(dir_path: &str) -> serde_json::Value {
    if !is_valid_dump_path(dir_path) {
        return serde_json::json!({ "ok": false, "error": "invalid path" });
    }

    let dir = std::path::Path::new(dir_path);

    // Look for dmesg files in the directory
    let candidates = ["dmesg.txt", "dmesg.log", "vmcore-dmesg.txt"];
    for name in &candidates {
        let path = dir.join(name);
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            let truncated = if content.len() > 65536 {
                format!("{}\n\n[... truncated at 64KB ...]", &content[..65536])
            } else {
                content
            };
            return serde_json::json!({
                "ok": true,
                "path": path.to_string_lossy(),
                "content": truncated,
            });
        }
    }

    // Try to find any .txt or .log file
    if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".txt") || name.ends_with(".log") || name.contains("dmesg") {
                if let Ok(content) = tokio::fs::read_to_string(entry.path()).await {
                    let truncated = if content.len() > 65536 {
                        format!("{}\n\n[... truncated at 64KB ...]", &content[..65536])
                    } else {
                        content
                    };
                    return serde_json::json!({
                        "ok": true,
                        "path": entry.path().to_string_lossy(),
                        "content": truncated,
                    });
                }
            }
        }
    }

    serde_json::json!({ "ok": false, "error": "no dmesg log found in dump directory" })
}

/// Validate dump path to prevent path traversal.
fn is_valid_dump_path(path: &str) -> bool {
    let p = std::path::Path::new(path);
    // Must be absolute, under allowed dirs, no ..
    p.is_absolute()
        && !path.contains("..")
        && (path.starts_with("/var/crash") || path.starts_with("/var/lib/kdump"))
}
