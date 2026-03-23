# tenodera-gateway

HTTP/WebSocket server with PAM authentication, session management, TLS support, and agent connection orchestration. Entry point to the Tenodera system.

## Role in architecture

The gateway is the central server accessible from the browser. It handles:
1. **Login** — PAM authentication via `unix_chkpwd`
2. **WebSocket** — upgrade after auth → bridge to bridge/agent
3. **UI serving** — static React files
4. **TLS** — optional encryption (rustls)
5. **Multi-host** — channel routing to local bridge or remote agents (SSH tunnel + sshpass)

```
Browser → HTTPS/WSS → Gateway (:9090) → stdin/stdout → Bridge (local)
                                       → SSH tunnel (sshpass) → Agent (remote)
```

## Modules

### `main.rs` — Axum server

Defines HTTP routing:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/auth/login` | POST | Login (user/password) |
| `/api/ws` | GET | WebSocket upgrade |
| `/api/health` | GET | Health check (`{ status: "ok" }`) |
| `/*` | GET | UI file serving (fallback to `index.html`) |

Application state (shared between handlers):
- `SessionStore` — in-memory, protected by `Arc<RwLock<HashMap>>`
- `GatewayConfig` — configuration from env vars
- `HostsConfig` — remote host configuration

**TLS:** If cert/key is configured, the gateway starts the server with `TlsAcceptor` (tokio-rustls). In non-TLS mode, it starts a regular `axum::serve`.

### `auth.rs` — Login endpoint

```
POST /api/auth/login
Content-Type: application/json

{ "user": "...", "password": "..." }
```

Flow:
1. Validate presence of `user` and `password` fields
2. Call `pam::authenticate(user, password)`
3. On success: create session in `SessionStore`, return session_id
4. On failure: HTTP 401 with message

Success response:
```json
{ "session_id": "uuid-...", "user": "...", "message": "Authentication successful" }
```

### `ws.rs` — WebSocket handler

```
GET /api/ws?session_id=uuid-...
```

WebSocket flow:
1. Validate `session_id` in query params
2. Read session from store (user, superuser_password)
3. Spawn local bridge (`BridgeProcess::spawn()`)
4. Event loop (`tokio::select!`):
   - **Msg from WebSocket** → JSON parse → routing:
     - If `Open` with host != `localhost` → `connect_remote_bridge()`
     - If `Data`/`Close` on channel with mapping → redirect to remote bridge
     - Otherwise → send to local bridge stdin
   - **Msg from local bridge stdout** → send to WebSocket
   - **Msg from remote bridge stdout** → send to WebSocket

**Multi-host routing:**
- `channel_to_bridge: HashMap<String, usize>` — channel → bridge index mapping
- `remote_bridges: Vec<BridgeProcess>` — vector of remote bridges
- When client opens channel with `host != "localhost"`, gateway:
  1. Looks up host definition in `hosts_config`
  2. Spawns remote bridge via SSH
  3. Registers channel → bridge index mapping

### `session.rs` — Session management

- **Store:** `Arc<RwLock<HashMap<String, Session>>>`
- **Session:**
  - `id: String` (UUID v4)
  - `user: String`
  - `password: String` — session password stored in memory (Cockpit model, used for SSH)
  - `created_at: Instant`
- **Debug:** Custom impl hiding password (`***`)
- **Idle timeout:** 15 minutes (900s) — sessions expire automatically
- **Operations:** `create(user, password)`, `validate(session_id)` (updates `last_active`), `remove(session_id)`

### `bridge_transport.rs` — Connection management

**BridgeProcess — Local spawn:**
```rust
BridgeProcess::spawn(bridge_bin) -> BridgeProcess
```
- Launches `bridge_bin` as a subprocess
- Creates `mpsc` channels (256 buf) for stdin/stdout
- Spawns 2 tokio tasks: reader (stdout→mpsc) and writer (mpsc→stdin)

**AgentConnection — Remote agent connection (SSH tunnel + sshpass):**
```rust
AgentConnection::connect_via_ssh_tunnel(ssh_user, password, address, ssh_port, agent_port)
  -> (AgentConnection, Child)
```
- Opens SSH tunnel: `sshpass -e ssh -N -L <local_port>:127.0.0.1:<agent_port> <ssh_user>@<address>`
- Password passed via `SSHPASS` environment variable (Cockpit model)
- `StrictHostKeyChecking=accept-new` — TOFU
- After establishing the tunnel, connects to the agent via WebSocket on `127.0.0.1:<local_port>`
- Returns `AgentConnection` (mpsc channels) + `Child` (SSH process to kill on drop)

**AgentConnection — Direct connection (Direct mode):**
```rust
AgentConnection::connect(address, port, api_key, use_tls) -> AgentConnection
```
- WebSocket to agent with optional API key and TLS

### `pam.rs` — PAM authentication

Implementation via `unix_chkpwd` — setuid helper from pam_unix:

```bash
unix_chkpwd {user} nullok   # password on stdin (NUL-terminated)
```

Result parser:
- Exit code 0 → success
- Exit code != 0 → authentication error
- Spawn error → general auth error

Input validation: checks that username does not contain `\0` or `\n` (injection protection).

### `config.rs` — Configuration

All settings from environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `TENODERA_BIND_ADDR` | `127.0.0.1` | Listen address |
| `TENODERA_BIND_PORT` | `9090` | Listen port |
| `TENODERA_BRIDGE_BIN` | `./target/debug/tenodera-bridge` | Path to bridge binary |
| `TENODERA_UI_DIR` | `./ui/dist` | Directory with built UI |
| `TENODERA_TLS_CERT` | `""` | Path to PEM certificate |
| `TENODERA_TLS_KEY` | `""` | Path to PEM private key |
| `TENODERA_ALLOW_UNENCRYPTED` | `false` | Allow HTTP without TLS |
| `TENODERA_IDLE_TIMEOUT` | `900` | Session inactivity timeout (seconds) |
| `TENODERA_MAX_STARTUPS` | `20` | Max concurrent connections |
| `RUST_LOG` | — | Log filter |

### `tls.rs` — TLS handling

- Building `TlsAcceptor` from PEM files (cert chain + private key)
- Acceptor on `rustls::ServerConfig` with default crypto provider
- Integration with axum via `hyper_util` + `TokioIo` wrapper
- Graceful shutdown: `tokio::signal::ctrl_c()`

### `hosts_config.rs` — Remote host configuration

- File: `~/.config/tenodera/hosts.json`
- Structure:
```json
[
  {
    "id": "uuid-...",
    "name": "Debian VM",
    "address": "192.168.56.10",
    "user": "",
    "ssh_port": 22,
    "agent_port": 9091,
    "transport": "ssh",
    "api_key": "",
    "agent_tls": false,
    "added_at": "2026-03-22T10:00:00Z"
  }
]
```
- `load()` — reads file, returns default empty config if file is missing
- `find_host(id)` — looks up host by ID
- `effective_user(session_user)` — if `user` is empty, uses `session_user`
- `Transport`: `Ssh` (sshpass tunnel) or `Agent` (direct WebSocket)

## Authentication flow (end-to-end)

```
1. POST /api/auth/login { user, password }
2. pam::authenticate() → unix_chkpwd user nullok
3. SessionStore::create(user, password)
4. → 200 { session_id, user }
5. GET /api/ws?session_id=uuid-...
6. SessionStore::validate(id) — check existence + timeout
7. BridgeProcess::spawn(bridge_bin) — local bridge
8. ↔ WebSocket ↔ Bridge stdin/stdout ↔ System
9. For remote hosts: AgentConnection::connect_via_ssh_tunnel(ssh_user, password, ...)
```

## Dependencies

- `axum 0.8` — HTTP + WebSocket framework
- `tokio` — async runtime
- `tokio-rustls 0.26` + `rustls 0.23` — TLS
- `tower-http 0.6` — ServeDir, CORS
- `hyper-util` — TCP listener for TLS
- `serde` + `serde_json` — JSON
- `uuid` — session ID generation
- `tracing` — logging
- `tenodera-protocol` — shared types

## Security

- Sessions in memory (not on disk)
- 15 min session timeout
- Per-user bridge (privilege isolation)
- SSH with `StrictHostKeyChecking=accept-new` (TOFU)
- Session password used for SSH via `sshpass` (Cockpit model)
- TLS optional but recommended
- CORS: `AllowOrigin::any()` (to be replaced in production)
