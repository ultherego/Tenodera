# tenodera-protocol

Shared type library defining the channel-based communication protocol between all Tenodera system components.

## Role in architecture

`tenodera-protocol` is a **library** crate — it does not produce any binary executable. It is a dependency for all other crates in the workspace (`gateway`, `bridge`). It defines the common "language" for message exchange: enums, structs, and error types that guarantee consistent JSON serialization/deserialization between the frontend, gateway, and bridge.

## Modules

### `message.rs` — Protocol message enums

The main `Message` type is a serde-tagged enum representing frames sent over WebSocket:

| Variant | Direction | Description |
|---------|-----------|-------------|
| `Open` | Client → Bridge | Opens a new channel. Contains `channel` (ID) and `ChannelOpenOptions` (payload type + extra options) |
| `Ready` | Bridge → Client | Channel readiness confirmation |
| `Data` | Bidirectional | Payload data on an open channel (`serde_json::Value`) |
| `Control` | Bidirectional | Control signal on a channel (command + extra fields) |
| `Close` | Bidirectional | Channel close. `problem: None` = clean close, `Some(reason)` = error |
| `Auth` | Client → Gateway | Authentication (credentials) |
| `AuthResult` | Gateway → Client | Authentication result (success/failure + user) |
| `Ping` | Bidirectional | Heartbeat |
| `Pong` | Bidirectional | Heartbeat response |

The `AuthCredentials` type supports two schemes:
- **Basic** — `user` + `password`
- **Token** — bearer `token`

### `channel.rs` — Channel types

- `ChannelId` — alias for `String`, unique channel identifier within a session
- `ChannelState` — channel state machine: `Opening` → `Ready` → `Closing` → `Closed`
- `ChannelOpenOptions` — options sent when opening a channel:
  - `payload: String` — required, points to a handler in the bridge (e.g. `"metrics.stream"`)
  - `superuser: Option<SuperuserMode>` — optional privilege mode (`Require` or `Try`)
  - `extra: serde_json::Map` — additional payload-specific options (flatten)
- `SuperuserMode` — enum: `Require` (force root) or `Try` (attempt, don't fail)

### `payload.rs` — Payload type registry

The `Payload` enum maps payload type strings to variants:

| String | Variant | Description |
|--------|---------|-------------|
| `system.info` | `SystemInfo` | Host information |
| `systemd.units` | `SystemdUnits` | systemd service list |
| `systemd.unit.action` | `SystemdUnitAction` | Service actions |
| `journal.query` | `JournalQuery` | journald queries |
| `journal.follow` | `JournalFollow` | journald following (streaming) |
| `file.read` | `FileRead` | File read |
| `file.write` | `FileWrite` | File write |
| `file.list` | `FileList` | Directory listing |
| `process.exec` | `ProcessExec` | Process execution |
| `process.stream` | `ProcessStream` | Process streaming |
| `terminal.pty` | `TerminalPty` | PTY terminal |
| `network.interfaces` | `NetworkInterfaces` | Network interfaces |
| `firewall.rules` | `FirewallRules` | Firewall rules |
| `metrics.stream` | `MetricsStream` | Metrics streaming |
| `ssh.remote` | `SshRemote` | Remote SSH |
| `package.updates` | `PackageUpdates` | Package updates |
| `container.list` | `ContainerList` | Container list |
| `Custom(String)` | — | Escape hatch for unknown/future payloads |

Methods: `from_str()` / `as_str()` for string ↔ enum conversion, `Display` trait.

### `error.rs` — Error types

`ProtocolError` (via `thiserror`) defines variants:

| Variant | Description |
|---------|-------------|
| `InvalidMessage` | Invalid message format |
| `UnknownPayload` | Unknown payload type |
| `ChannelNotFound` | Channel does not exist |
| `ChannelAlreadyExists` | Channel already exists |
| `AuthFailed` | Authentication failure |
| `PermissionDenied` | Access denied |
| `Serialization` | JSON serialization error (from `serde_json::Error`) |
| `Transport` | Transport layer error |

## Dependencies

- `serde` + `serde_json` — JSON serialization/deserialization
- `thiserror` — declarative error types
- `uuid` — ID generation (available, though not used directly in this crate)
- `chrono` — time types
- `bytes` — byte buffers

## Usage example

```rust
use tenodera_protocol::message::Message;
use tenodera_protocol::channel::ChannelOpenOptions;

// Deserialize a message from JSON
let json = r#"{"type":"open","channel":"ch1","payload":"metrics.stream","interval":1000}"#;
let msg: Message = serde_json::from_str(json).unwrap();

// Serialize a response
let resp = Message::Ready { channel: "ch1".to_string() };
let json_out = serde_json::to_string(&resp).unwrap();
```

## Wire format

Each WebSocket frame is a single JSON object with a `type` field:

```jsonc
// Open
{ "type": "open", "channel": "ch1", "payload": "system.info" }

// Ready
{ "type": "ready", "channel": "ch1" }

// Data
{ "type": "data", "channel": "ch1", "data": { "hostname": "server1", ... } }

// Close (clean)
{ "type": "close", "channel": "ch1" }

// Close (with error)
{ "type": "close", "channel": "ch1", "problem": "access-denied" }

// Ping/Pong
{ "type": "ping" }
{ "type": "pong" }
```
