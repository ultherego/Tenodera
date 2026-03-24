use std::fs::OpenOptions;
use std::io::Write;

const AUDIT_LOG: &str = "/var/log/tenodera_audit.log";

/// Write a single audit line to the audit log file.
/// Format: [ISO8601] user=<user> action=<action> target=<target> result=<ok|fail> details=<...>
/// Silently ignores write errors (logging should never crash the process).
pub fn log(user: &str, action: &str, target: &str, ok: bool, details: &str) {
    let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let result = if ok { "ok" } else { "fail" };
    let line = format!("[{ts}] user={user} action={action} target={target} result={result} details={details}\n");

    let _ = OpenOptions::new()
        .create(true)
        .append(true)
        .open(AUDIT_LOG)
        .and_then(|mut f| f.write_all(line.as_bytes()));
}
