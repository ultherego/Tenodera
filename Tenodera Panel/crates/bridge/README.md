# tenodera-bridge

Per-session channel message router with pluggable system handlers.

## Role in architecture

`tenodera-bridge` is the main backend engine of Tenodera. For each logged-in user session, the gateway spawns a *separate* `tenodera-bridge` process running with that user's privileges. The bridge communicates with the gateway via stdin/stdout (JSON lines), and the gateway bridges them to/from the WebSocket in the browser.

```
Browser ←→ WebSocket ←→ Gateway ←→ stdin/stdout ←→ Bridge (per user)
```

## Internal architecture

### `main.rs` — Main loop

1. Logger initialization (`tracing` → stderr)
2. Create `mpsc` channel (256 elements) for outgoing messages
3. Register default handlers in `Router`
4. Spawn stdout writer task — receives messages from channel and serializes them as JSON lines
5. Main loop — reads stdin line by line, deserializes `Message`, passes to `Router::handle()`, sends responses

### `handler.rs` — `ChannelHandler` trait

Defines the interface for all handlers:

```rust
pub trait ChannelHandler: Send + Sync {
    fn payload_type(&self) -> &str;        // e.g. "system.info"
    fn is_streaming(&self) -> bool;         // default false
    async fn open(&self, channel: &str, options: &ChannelOpenOptions) -> Vec<Message>;
    async fn stream(&self, channel: &str, options: &ChannelOpenOptions,
                    tx: mpsc::Sender<Message>, shutdown: watch::Receiver<bool>);
    async fn data(&self, channel: &str, data: &Value) -> Vec<Message>;
}
```

**Handler types:**
- **One-shot** — `open()` returns Ready + Data + Close immediately
- **Streaming** — `is_streaming()=true`, `stream()` sends data via `tx` until `shutdown`
- **Bidirectional** — `open()` returns Ready (without Close), then accepts `data()` with commands

### `router.rs` — Message router

`Router` manages:
- `handlers: HashMap<String, Arc<dyn ChannelHandler>>` — handler registry by payload type
- `active_channels: HashMap<String, ActiveChannel>` — active streaming channels (with `shutdown_tx`)
- `channel_handlers: HashMap<String, Arc<dyn ChannelHandler>>` — channel → handler mapping (one-shot/bidirectional)
- `out_tx: mpsc::Sender<Message>` — output channel to stdout

Routing logic in `handle()`:
- **Open** → looks up handler by `options.payload`
  - Streaming: spawns tokio task, sends Ready, runs `stream()`
  - One-shot/Bidirectional: calls `open()`, returns responses
- **Data** → looks up handler by channel ID (first active_channels, then channel_handlers), calls `data()`
- **Close** → closes streaming channel (sends shutdown), removes tracking
- **Ping** → returns Pong

## Handlers (17 registered)

### One-shot handlers

| Handler | Payload type | Data source | Description |
|---------|-------------|-------------|-------------|
| `SystemInfoHandler` | `system.info` | `/proc/uptime`, `/etc/os-release`, `gethostname()` | Hostname, OS, uptime, boot time |
| `HardwareInfoHandler` | `hardware.info` | `/proc/cpuinfo`, `uname()`, `/sys/class/hwmon/` | CPU model, cores/threads, MHz, arch, kernel, temperature sensors |
| `TopProcessesHandler` | `top.processes` | `ps --sort=-%cpu` | Top 15 processes (PID, user, CPU%, MEM%, RSS, command) |
| `DiskUsageHandler` | `disk.usage` | `/proc/mounts` + `statvfs()` | Partition usage (device, mount, fstype, total/used/free/avail, %) |
| `NetworkStatsHandler` | `network.stats` | `/proc/net/dev`, `/sys/class/net/`, `ip -j addr show` | Network interfaces with RX/TX, MAC, speed, state, IPv4/IPv6 |
| `JournalQueryHandler` | `journal.query` | `journalctl --output=json` | journald entries with filters (unit, priority, lines) |
| `FileListHandler` | `file.list` | `read_dir()` / `sudo ls -laH` | Directory listing with type, size. Sudo fallback for restricted directories. Path validation via `canonicalize()` |
| `SuperuserVerifyHandler` | `superuser.verify` | `unix_chkpwd` | User password verification. Returns `{ ok: true/false }` |

### Streaming handlers

| Handler | Payload type | Interval | Data source | Description |
|---------|-------------|----------|-------------|-------------|
| `MetricsStreamHandler` | `metrics.stream` | Configurable (default 1s) | `/proc/stat`, `/proc/meminfo`, `/proc/loadavg`, `/proc/diskstats`, `/proc/net/dev` | CPU (aggregated + per-core), memory, swap, load avg, disk I/O, net I/O |
| `StorageStreamHandler` | `storage.stream` | Configurable (default 2s) | `/proc/diskstats` + `lsblk -J` + `statvfs()` | Disk I/O rates + block device tree with FS usage |
| `NetworkStreamHandler` | `networking.stream` | Configurable (default 1s) | `/proc/net/dev` | TX/RX rates per interface (bytes/sec) |
| `TerminalPtyHandler` | `terminal.pty` | — (event-driven) | `openpty()` + `fork()` + `execvp()` | Interactive PTY terminal. Bidirectional I/O + resize (TIOCSWINSZ) |

### Bidirectional handlers (open + data)

| Handler | Payload type | Actions | Description |
|---------|-------------|---------|-------------|
| `SystemdManageHandler` | `systemd.manage` | `start`, `stop`, `restart`, `reload`, `enable`, `disable`, `status`, `list` | systemd service management. Verifies password via `unix_chkpwd`, executes `systemctl` directly (bridge runs as root) |
| `ContainersHandler` | `container.manage` | `list_containers`, `list_images`, `inspect`, `start`, `stop`, `restart`, `remove`, `remove_image`, `pull`, `create`, `logs`, `service_status`, `service_start/stop/restart` | Docker/Podman. Auto-detection of runtime (podman → docker) |
| `NetworkManageHandler` | `networking.manage` | `list_interfaces`, `firewall_status/rules/enable/disable/add_rule/remove_rule`, `add_bridge`, `add_vlan`, `remove_interface`, `iface_up/down`, `vpn_list/connect/disconnect`, `network_logs` | Network management. Multi-backend firewall (ufw/firewalld/nftables/iptables) |
| `PackagesHandler` | `packages.manage` | `detect`, `list_installed`, `search`, `package_info`, `install`, `remove`, `check_updates`, `update_system`, `list_repos`, `add_repo`, `remove_repo`, `refresh_repos` | System packages. Auto-detection (pacman/apt/dnf) |
| `HostsManageHandler` | `hosts.manage` | `list`, `add`, `remove` | Remote host CRUD. Persistence in `~/.config/tenodera/hosts.json` |



## Implementation details

### Terminal PTY (`terminal_pty.rs`)
- PTY opening: `nix::pty::openpty()` with configurable dimensions
- Fork: `nix::unistd::fork()` → child execvp shell
- Parent: `AsyncFd` on master FD (non-blocking) for reading, dup'd FD for writing
- Shell detection: parsing `/etc/passwd` by UID
- Resize: `TIOCSWINSZ` ioctl on FD upon `{ "resize": { "cols": N, "rows": N } }` message
- Client input: writing to master FD upon `{ "input": "..." }` message

### Multi-backend firewall (`networking.rs`)
- Detection: checks `which` for ufw → firewalld → nftables → iptables
- Status/rules: queries all detected backends simultaneously
- Smart filtering: hides internal ufw/docker/firewalld chains from nftables/iptables
- Add/remove: different logic per backend (ports vs services)

### Container management (`containers.rs`)
- Auto-detection: podman (preferred) → docker
- JSON parsing: handles both JSON array (docker) and JSON-per-line (podman)
- Create: supports names, ports, env vars, volumes, restart policy, custom commands
- Service: manages `docker.service` / `podman.socket` via systemctl

### Package management (`packages.rs`)
- Auto-detection: pacman → apt → dnf
- Dedicated parsers per distro (pacman -Q, dpkg-query, rpm -qa)
- Repository management specific to each package manager

## Dependencies

- `tenodera-protocol` — shared types
- `tokio` — async runtime (full features)
- `serde` + `serde_json` — JSON serialization
- `nix 0.29` — PTY, fork, setsid, dup2, ioctl, hostname
- `libc` — raw syscalls (read, write, ioctl, fcntl, statvfs)
- `async-trait` — async trait methods
- `zbus 5` — D-Bus (available, not used directly)
- `chrono` — timestamps
- `tracing` — logging
- `uuid` — channel ID generation

## Running

The bridge is normally launched by the gateway (not directly):

```bash
# Manual testing — pipe JSON to stdin:
echo '{"type":"open","channel":"ch1","payload":"system.info"}' | ./tenodera-bridge
```

Environment variables:
- `RUST_LOG` — log filter, e.g. `tenodera_bridge=debug`
