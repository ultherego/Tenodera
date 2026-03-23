# Tenodera — Complete Project Documentation

> A Rust replacement for Cockpit — a web-based Linux server administration panel with multi-host support.

---

## Table of Contents

1. [Introduction](#introduction)
2. [System Architecture](#system-architecture)
3. [Communication Protocol](#communication-protocol)
4. [Components](#components)
   - [tenodera-protocol](#tenodera-protocol)
   - [tenodera-gateway](#tenodera-gateway)
   - [tenodera-bridge](#tenodera-bridge)
   - [tenodera-priv-bridge](#tenodera-priv-bridge)
   - [tenodera-ui](#tenodera-ui)
   - [systemd Files](#systemd-files)
5. [Data Flow](#data-flow)
6. [Multi-Host Management](#multi-host-management)
7. [Configuration](#configuration)
8. [Building and Running](#building-and-running)
9. [Security](#security)
10. [Developer Tools](#developer-tools)

---

## Introduction

Tenodera is a reimplementation of Cockpit in Rust with a React frontend. The project implements a multi-process architecture: a central gateway handles authentication and WebSocket connections, spawning an isolated bridge process per session that runs with the logged-in user's privileges. It supports managing multiple remote hosts via SSH.

### Key Features

- **Real-time metrics** — CPU (per-core), RAM, swap, load, disk I/O, network I/O
- **systemd service management** — start/stop/restart/enable/disable/reload
- **Docker/Podman containers** — list, create, logs, image management
- **Networking** — interfaces, firewall (ufw/firewalld/nftables/iptables), bridges, VLAN, VPN
- **Packages** — pacman/apt/dnf with auto-detection, repositories
- **Terminal** — full PTY emulator (xterm.js)
- **File browser** — filesystem navigation with sudo fallback
- **System logs** — journald with filters
- **Multi-host** — manage multiple servers from a single interface
- **TLS** — optional encryption via rustls
- **systemd hardening** — service sandboxing

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Backend runtime | Rust 1.94, edition 2024, tokio async |
| HTTP/WS | axum 0.8 |
| TLS | rustls 0.23 + tokio-rustls 0.26 |
| System | nix 0.29, libc (PTY, fork, ioctl, statvfs) |
| D-Bus | zbus 5 |
| Frontend | React 19, TypeScript 5.7, Vite 6 |
| Charts | Recharts 3.8 |
| Terminal | @xterm/xterm 5.5 |
| Routing | react-router-dom 7 |
| State | @tanstack/react-query 5 |

---

## System Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                            Browser                              │
│  React SPA (tenodera-ui)                                        │
│  ├── WebSocket transport (multiplexed channels)                 │
│  ├── 12 pages (Dashboard, Services, Terminal, ...)              │
│  └── HostTransportContext (local/remote routing)                │
└──────────────────────────┬──────────────────────────────────────┘
                           │ WSS/WS
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│  tenodera-gateway                    :9090                        │
│  ├── POST /api/auth/login  → PAM (unix_chkpwd) → SessionStore    │
│  ├── GET  /api/ws          → WebSocket handler                   │
│  ├── GET  /api/health      → health check                       │
│  └── GET  /*               → ServeDir (UI files)                │
│                                                                  │
│  Per session:                                                    │
│  ├── BridgeProcess::spawn()        → local bridge                │
│  └── AgentConnection::connect_via_ssh_tunnel() → SSH → agent      │
└──────┬───────────────────────────────────────┬──────────────────┘
       │ stdin/stdout                          │ SSH
       ▼                                       ▼
┌─────────────────────┐              ┌─────────────────────┐
│  tenodera-bridge     │              │  tenodera-bridge     │
│  (user: alice)      │              │  (remote host)      │
│  ├── Router         │              │  ├── Router         │
│  ├── 18 handlers    │              │  ├── 18 handlers    │
│  └── PTY, systemctl │              │  └── PTY, systemctl │
└─────────────────────┘              └─────────────────────┘

┌─────────────────────┐
│  tenodera-priv-bridge│
│  (root, allowlist)  │
│  ├── systemd.unit   │
│  └── package.updates│
└─────────────────────┘
```

### Cargo Workspace

```toml
[workspace]
members = [
    "crates/protocol",    # Library: shared protocol types
    "crates/gateway",     # Binary: tenodera-gateway
    "crates/bridge",      # Binary + lib: tenodera-bridge
    "crates/priv-bridge", # Binary: tenodera-priv-bridge
]
```

---

## Communication Protocol

The system uses a channel-based protocol built on JSON, transmitted over WebSocket (browser ↔ gateway) and stdin/stdout (gateway ↔ bridge).

### Message Types

| Type | Direction | Description |
|------|-----------|-------------|
| `Open` | client → server | Open a channel (payload, options) |
| `Ready` | server → client | Channel open confirmation |
| `Data` | ↔ bidirectional | Channel data (JSON) |
| `Control` | ↔ bidirectional | Control commands |
| `Close` | ↔ bidirectional | Close channel (optional problem) |
| `Ping` | client → server | Keepalive |
| `Pong` | server → client | Keepalive response |

### Payload Types (18 built-in + Custom)

| Payload | Handler Type | Description |
|---------|-------------|-------------|
| `system.info` | One-shot | System information |
| `hardware.info` | One-shot | CPU, kernel, temperatures |
| `top.processes` | One-shot | Top 15 processes |
| `disk.usage` | One-shot | Partition usage |
| `network.stats` | One-shot | Network interfaces |
| `journal.query` | One-shot | journald logs |
| `file.list` | One-shot | Directory listing |
| `superuser.verify` | One-shot | Sudo password verification |
| `systemd.units` | One-shot | systemd unit list |
| `metrics.stream` | Streaming | Real-time CPU/RAM/IO metrics |
| `storage.stream` | Streaming | Disk I/O + block devices |
| `networking.stream` | Streaming | TX/RX rates per interface |
| `terminal.pty` | Streaming+Bidi | Interactive PTY terminal |
| `systemd.manage` | Bidirectional | systemd service management |
| `container.manage` | Bidirectional | Docker/Podman |
| `networking.manage` | Bidirectional | Firewall, bridges, VLAN, VPN |
| `packages.manage` | Bidirectional | System packages |
| `hosts.manage` | Bidirectional | Remote host CRUD |

### Handler Modes

- **One-shot:** Open → Ready + Data + Close (single data fetch)
- **Streaming:** Open → Ready → [Data, Data, ...] (continuous stream, Close from client stops it)
- **Bidirectional:** Open → Ready → [Data↔Data] (client sends commands, server responds)

### Message Exchange Example

```json
// Client → Open channel
{"type":"open","channel":"ch1","payload":"system.info"}

// Server → Ready
{"type":"ready","channel":"ch1"}

// Server → Data
{"type":"data","channel":"ch1","data":{"hostname":"srv1","os":"Arch Linux","uptime":86400}}

// Server → Close
{"type":"close","channel":"ch1"}
```

---

## Components

### tenodera-protocol

**Location:** `crates/protocol/` | **Type:** Rust library | **Details:** [crates/protocol/README.md](crates/protocol/README.md)

Shared library defining protocol types used by all other crates:

- **`message.rs`** — `Message` enum with 8 variants (Open/Ready/Data/Control/Close/Auth/AuthResult/Ping/Pong) + `AuthCredentials`
- **`channel.rs`** — `ChannelId`, `ChannelState`, `ChannelOpenOptions` (payload, superuser, extra), `SuperuserMode`
- **`payload.rs`** — `Payload` enum with 17 variants + Custom, string↔enum conversions, Display
- **`error.rs`** — `ProtocolError` with 8 variants via thiserror

---

### tenodera-gateway

**Location:** `crates/gateway/` | **Binary:** `tenodera-gateway` | **Port:** 9090 | **Details:** [crates/gateway/README.md](crates/gateway/README.md)

Central HTTP/WebSocket server:

| Module | Description |
|--------|-------------|
| `main.rs` | Axum server with HTTP routing |
| `auth.rs` | Login endpoint (POST /api/auth/login) |
| `ws.rs` | WebSocket handler with multi-host routing |
| `session.rs` | In-memory SessionStore (UUID, configurable timeout 900s) |
| `bridge_transport.rs` | Bridge spawn: local (subprocess) or remote (SSH) |
| `pam.rs` | PAM authentication via `unix_chkpwd` |
| `config.rs` | Configuration from env vars |
| `tls.rs` | TLS via rustls (optional) |
| `hosts_config.rs` | Reads ~/.config/tenodera/hosts.json, `effective_user()` (empty user = session user) |

**Endpoints:**

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/auth/login` | POST | Login |
| `/api/ws` | GET | WebSocket upgrade |
| `/api/health` | GET | Health check |
| `/*` | GET | UI files (fallback index.html) |

---

### tenodera-bridge

**Location:** `crates/bridge/` | **Binary:** `tenodera-bridge` | **Details:** [crates/bridge/README.md](crates/bridge/README.md)

Per-session message router with 18 handlers (from 16 modules):

| Module | Description |
|--------|-------------|
| `main.rs` | Async loop: stdin → Router → stdout (JSON lines) |
| `handler.rs` | `ChannelHandler` trait (payload_type, is_streaming, open, stream, data) |
| `router.rs` | Message dispatch by payload type, channel management, 18 handler registration |
| `handlers/` | 16 modules exporting 18 handlers (systemd_units and networking each export 2) |

**Handlers:**

| Handler | Payload | Type | Module |
|---------|---------|------|--------|
| SystemInfoHandler | `system.info` | One-shot | system_info |
| HardwareInfoHandler | `hardware.info` | One-shot | hardware_info |
| TopProcessesHandler | `top.processes` | One-shot | top_processes |
| DiskUsageHandler | `disk.usage` | One-shot | disk_usage |
| NetworkStatsHandler | `network.stats` | One-shot | network_stats |
| JournalQueryHandler | `journal.query` | One-shot | journal_query |
| FileListHandler | `file.list` | One-shot | file_list |
| SuperuserVerifyHandler | `superuser.verify` | One-shot | superuser_verify |
| SystemdUnitsHandler | `systemd.units` | One-shot | systemd_units |
| MetricsStreamHandler | `metrics.stream` | Streaming | metrics_stream |
| StorageStreamHandler | `storage.stream` | Streaming | storage |
| NetworkStreamHandler | `networking.stream` | Streaming | networking |
| TerminalPtyHandler | `terminal.pty` | Streaming+Bidi | terminal_pty |
| SystemdManageHandler | `systemd.manage` | Bidirectional | systemd_units |
| ContainersHandler | `container.manage` | Bidirectional | containers |
| NetworkManageHandler | `networking.manage` | Bidirectional | networking |
| PackagesHandler | `packages.manage` | Bidirectional | packages |
| HostsManageHandler | `hosts.manage` | Bidirectional | hosts |

**Data Sources:**
- `/proc/stat`, `/proc/meminfo`, `/proc/loadavg`, `/proc/diskstats`, `/proc/net/dev` — metrics
- `/proc/cpuinfo`, `/proc/uptime`, `/proc/mounts` — system info
- `/etc/os-release`, `/etc/passwd` — system configuration
- `/sys/class/hwmon/`, `/sys/class/net/` — hardware
- `systemctl`, `journalctl`, `lsblk`, `ip`, `ps` — system tools
- `ufw`, `firewall-cmd`, `nft`, `iptables` — firewall
- `podman`, `docker` — containers
- `pacman`, `apt`, `dnf` — packages
- `nmcli` — VPN
- `openpty()`, `fork()` — PTY terminal

---

### tenodera-priv-bridge

**Location:** `crates/priv-bridge/` | **Binary:** `tenodera-priv-bridge` | **Details:** [crates/priv-bridge/README.md](crates/priv-bridge/README.md)

Privileged helper (root) with an allowlist:

- **Allowed operations:** `systemd.unit.action`, `package.updates`
- **All others:** rejected with `not-authorized`
- **Status:** stub — validation works, handlers to be implemented
- Synchronous stdin/stdout loop (not async)

---

### tenodera-ui

**Location:** `ui/` | **Dev port:** 3000 | **Details:** [ui/README.md](ui/README.md)

Frontend React SPA:

| Page | Payload Types | Description |
|------|--------------|-------------|
| Login | — | PAM login form |
| Shell | `hosts.manage`, `system.info` | Container: sidebar, top bar, routing, host selector, superuser |
| Dashboard | `system.info`, `metrics.stream`, `hardware.info`, `disk.usage`, `network.stats`, `top.processes` | Real-time CPU/RAM/IO charts, processes, hardware |
| Services | `systemd.units`, `systemd.manage` | systemd service management |
| Containers | `container.manage` | Docker/Podman GUI |
| Storage | `storage.stream` | Disk I/O and block devices |
| Networking | `networking.stream`, `networking.manage` | Network, firewall (multi-backend), VPN |
| Packages | `packages.manage` | System packages (pacman/apt/dnf) |
| Logs | `journal.query` | journald logs with filters |
| Terminal | `terminal.pty` | Terminal emulator (xterm.js) |
| Files | `file.list` | File browser with sudo fallback |
| Hosts | `hosts.manage` | Remote host CRUD |

**Transport layer:**
- `transport.ts` — singleton WebSocket with channel multiplexing (`connect()`, `openChannel()`, `request()`)
- `auth.ts` — login client (`login(user, password)` → POST /api/auth/login)
- `HostTransportContext.tsx` — React Context for local/remote routing: `useTransport()` hook wraps `openChannel()` and `request()` adding `{host: hostId}` when activeHost is set

---

### systemd Files

**Location:** `systemd/` | **Details:** [systemd/README.md](systemd/README.md)

| Service | Description |
|---------|-------------|
| `tenodera-gateway.service` | Main HTTP/WS service (with security hardening) |
| `tenodera-priv-bridge.service` | Root helper (socket activation) |

Both services have hardening: `ProtectSystem=full`, `PrivateTmp`, `ProtectKernelTunables`, `ProtectControlGroups`, `LockPersonality`.

---

## Data Flow

### Login and Session Establishment

```
1. User → POST /api/auth/login { user, password }
2. Gateway → pam::authenticate() → unix_chkpwd (PAM helper)
3. Gateway → SessionStore::create(user, password) → UUID
4. Gateway → 200 { session_id, user }
5. Browser → sessionStorage.setItem('session_id', ...)
6. Browser → GET /api/ws?session_id=uuid
7. Gateway → validate_session() → spawn BridgeProcess
8. ↔ WebSocket ↔ Bridge (JSON lines stdin/stdout)
```

### One-shot Request (e.g. system info)

```
Client: {"type":"open","channel":"1","payload":"system.info"}
  → Gateway → Bridge stdin
  → Router → SystemInfoHandler::open()
  → Bridge stdout → Gateway → WebSocket
Client: {"type":"ready","channel":"1"}
         {"type":"data","channel":"1","data":{...}}
         {"type":"close","channel":"1"}
```

### Streaming (e.g. metrics)

```
Client: {"type":"open","channel":"2","payload":"metrics.stream","interval":1000}
  → Router → spawn tokio task → MetricsStreamHandler::stream()
  → every 1s: {"type":"data","channel":"2","data":{cpu:...,memory:...}}
  → every 1s: {"type":"data","channel":"2","data":{cpu:...,memory:...}}
  → ...
Client: {"type":"close","channel":"2"}  ← stops the stream
```

### Bidirectional (e.g. systemd manage)

```
Client: {"type":"open","channel":"3","payload":"systemd.manage"}
Server: {"type":"ready","channel":"3"}

Client: {"type":"data","channel":"3","data":{"action":"list"}}
Server: {"type":"data","channel":"3","data":[{unit:"nginx.service",...}]}

Client: {"type":"data","channel":"3","data":{"action":"restart","unit":"nginx.service"}}
Server: {"type":"data","channel":"3","data":{"ok":true}}

Client: {"type":"close","channel":"3"}
```

---

## Multi-Host Management

### Multi-Host Architecture

Tenodera supports managing multiple servers from a single interface:

1. **Host registration** — via the Hosts page (payload `hosts.manage`)
2. **Persistence** — `~/.config/tenodera/hosts.json`
3. **Connection** — gateway spawns a remote bridge via SSH
4. **Transparent routing** — frontend adds `host: hostId` to the Open message

### Host Configuration File

```json
[
  {
    "id": "9ca1fc19-...",
    "name": "Debian VM",
    "address": "192.168.56.10",
    "user": "",
    "ssh_port": 22,
    "added_at": "2026-03-22T10:00:00Z"
  }
]
```

### Remote Flow

```
1. Frontend: openChannel("system.info", { host: "uuid-..." })
2. Gateway: detects "host" field in Open message
3. Gateway: find_host(id) → address, effective_user(session_user), ssh_port
4. Gateway: AgentConnection::connect_via_ssh_tunnel()
   → sshpass -e ssh -N -o StrictHostKeyChecking=accept-new
          -p 22 -L <local_port>:127.0.0.1:9091 <user>@<host>
   → WebSocket to agent through the tunnel (127.0.0.1:<local_port>)
5. Gateway: registers channel → remote agent mapping
6. Agent on remote host: identical protocol
7. Responses: remote agent WS → gateway → WebSocket → frontend
```

**Host `user` field:**
- Empty (`""`) — SSH logs in as the session's logged-in user (FreeIPA/enterprise model)
- Filled — SSH logs in as the specified user (per-host override)

### Frontend: HostTransportContext

```tsx
// Shell.tsx wraps routes in HostTransportProvider with the active host
<HostTransportProvider value={activeHost?.id ?? null}>
  <Routes>
    <Route path="/" element={<Dashboard />} />
    <Route path="/services" element={<Services />} />
    {/* ... same components for local and remote */}
  </Routes>
</HostTransportProvider>

// Inside Dashboard (or any page):
const { request, openChannel } = useTransport();
const data = await request('system.info');
// → if activeHost is set, automatically adds { host: "uuid-..." }
// → if null (local host), sends without the host field
```

---

## Configuration

### Gateway Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `TENODERA_BIND_ADDR` | `127.0.0.1` | Listen address |
| `TENODERA_BIND_PORT` | `9090` | Listen port |
| `TENODERA_BRIDGE_BIN` | `./target/debug/tenodera-bridge` | Path to bridge binary |
| `TENODERA_UI_DIR` | `./ui/dist` | UI directory |
| `TENODERA_TLS_CERT` | `""` | TLS certificate (PEM) |
| `TENODERA_TLS_KEY` | `""` | TLS private key (PEM) |
| `TENODERA_ALLOW_UNENCRYPTED` | `true` | Allow HTTP without TLS |
| `TENODERA_IDLE_TIMEOUT` | `900` | Session timeout (seconds) |
| `TENODERA_MAX_STARTUPS` | `20` | Max concurrent bridge processes |
| `RUST_LOG` | — | Log filter |

---

## Building and Running

### Requirements

- Rust 1.94+ (stable)
- Node.js 18+ and npm
- Linux (requires /proc, systemd, PTY)

### Building

```bash
# Backend — all 4 binaries
cargo build

# Frontend
cd ui && npm install && npm run build && cd ..
```

### Output Binaries

| Binary | Location | Description |
|--------|----------|-------------|
| `tenodera-gateway` | `target/debug/tenodera-gateway` | Main server |
| `tenodera-bridge` | `target/debug/tenodera-bridge` | Per-session router |
| `tenodera-priv-bridge` | `target/debug/tenodera-priv-bridge` | Root helper |

### Development Run

```bash
# Terminal 1 — gateway
RUST_LOG=info cargo run --bin tenodera-gateway

# Terminal 2 — frontend dev server (proxy to :9090)
cd ui && npm run dev

# Open browser at http://localhost:3000
```

### Production Run

```bash
cargo build --release

# Installation
sudo cp target/release/tenodera-gateway /usr/local/bin/
sudo cp target/release/tenodera-bridge /usr/local/bin/
sudo mkdir -p /usr/share/tenodera/ui
sudo cp -r ui/dist/* /usr/share/tenodera/ui/

# systemd
sudo cp systemd/tenodera-gateway.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now tenodera-gateway
```

### Test VMs (Vagrant)

```bash
vagrant up        # 2x Debian bookworm: 192.168.56.10 (Panel), 192.168.56.11 (Agent)
vagrant ssh tenodera-remote-1   # Enter the Panel VM
vagrant ssh tenodera-remote-2   # Enter the Agent VM
```

---

## Security

### Process Isolation

- Each session = a separate bridge process with the logged-in user's privileges
- Bridge has no access to other users' sessions
- Gateway does not directly perform system operations

### Authentication

- PAM via `unix_chkpwd` (setuid helper from pam_unix)
- In-memory sessions with UUID v4
- 15-minute inactivity timeout (configurable via `TENODERA_IDLE_TIMEOUT`, default 900s)
- Session password stored in gateway memory (Cockpit model) — used for SSH to remote hosts

### Privilege Escalation

- Bridge verifies password via `unix_chkpwd` (`superuser.verify` handler)
- Privileged operations (systemctl) executed directly (bridge runs as root)
- Priv-bridge runs as root with a restrictive allowlist

### systemd Hardening

- `ProtectSystem=full` — read-only /usr, /boot, /efi
- `PrivateTmp` — isolated /tmp
- `ProtectKernelTunables` — blocks /proc/sys writes
- `ProtectControlGroups` — blocks /sys/fs/cgroup writes
- `LockPersonality` — blocks execution domain changes

### Input Validation

- File list: `canonicalize()` for path traversal protection
- Priv-bridge: payload type allowlist
- Gateway: session_id validation before WebSocket upgrade

### SSH (remote hosts) — Cockpit Model

- The user's login password is stored in the gateway session
- SSH tunnels opened via `sshpass -e ssh` with the session password (`SSHPASS` env var)
- `StrictHostKeyChecking=accept-new` — TOFU (Trust On First Use)
- Requires `PasswordAuthentication yes` in sshd on remote hosts
- Host `user` field: empty = session user (enterprise/FreeIPA model), filled = override
- System dependency: `sshpass` package

---

## Developer Tools

### Diagnostics

```bash
# Test bridge via SSH
python3 test_ssh_bridge.py
node test_ssh_bridge.js
```

### Project Structure

```
Tenodera/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── protocol/           # Protocol library
│   │   └── src/            # message, channel, payload, error
│   ├── gateway/            # HTTP/WS server
│   │   └── src/            # auth, ws, session, bridge_transport, pam, config, tls, hosts_config
│   ├── bridge/             # Router + 18 handlers
│   │   └── src/
│   │       ├── handlers/   # 16 modules: system_info, hardware_info, top_processes,
│   │       │               #   metrics_stream, systemd_units, journal_query,
│   │       │               #   terminal_pty, file_list, disk_usage, storage,
│   │       │               #   network_stats, networking, containers, packages,
│   │       │               #   superuser_verify, hosts
│   │       ├── handler.rs  # ChannelHandler trait
│   │       └── router.rs   # Router dispatch (18 handlers from 16 modules)
│   └── priv-bridge/        # Root helper (allowlist)
│       └── src/main.rs
├── systemd/                # systemd service files
├── ui/                     # React/TypeScript frontend
│   ├── src/
│   │   ├── api/            # transport, auth, HostTransportContext
│   │   └── pages/          # 12 pages (Login, Shell, Dashboard, Services, Containers,
│   │                       #   Storage, Networking, Packages, Logs, Terminal, Files, Hosts)
│   ├── package.json
│   └── vite.config.ts
├── Vagrantfile             # Test VMs (Debian bookworm: 192.168.56.10, 192.168.56.11)
├── README.md               # README (English)
├── GENERAL_README.md       # Cockpit architecture analysis (reference)
├── MASTER_README.md        # This file — complete project documentation
└── README-HOSTS.md         # Host management analysis
```
