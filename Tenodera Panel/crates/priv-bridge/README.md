# tenodera-priv-bridge

Privileged helper running as root with a restrictive operation allowlist. Designed for secure privilege escalation for a limited set of system commands.

## Role in architecture

`tenodera-priv-bridge` is an optional component running as root (via systemd socket activation or directly). It accepts JSON messages from stdin and responds on stdout — identically to the regular bridge — but with root privileges. Only strictly defined operations are permitted (allowlist).

```
Gateway → stdin/stdout → priv-bridge (root)   ← only allowed operations
```

## Security — allowlist model

Priv-bridge implements a **payload type whitelist** as the primary security mechanism:

```rust
const ALLOWED_PAYLOADS: &[&str] = &[
    "systemd.unit.action",
    "package.updates",
];
```

Every `Open` message is validated:
1. Check if `payload_type` is on the `ALLOWED_PAYLOADS` list
2. If not → immediate rejection with `Close` + error message
3. If yes → dispatch to the appropriate handler

## Implementation (`main.rs`)

### Main loop

Synchronous stdin/stdout loop (not async):

```
loop {
    1. Read a line from stdin
    2. Deserialize JSON → Message
    3. Match on message type:
       - Open → check allowlist → dispatch or reject
       - Data → reject (no active bidirectional channels)
       - Close → ignore
       - Ping → return Pong
    4. Serialize responses → stdout
}
```

### Operation dispatch

Currently a stub — after allowlist validation returns:
- `Ready` (channel open confirmation)
- `Close` (immediate close)

Target handlers should implement actual logic (e.g. `systemctl` directly as root without sudo).

### Rejection of disallowed operations

```json
// Input:
{"type": "open", "channel": "ch1", "payload": "terminal.pty"}

// Response:
{"type": "close", "channel": "ch1", "problem": "not-authorized", "message": "Payload 'terminal.pty' is not allowed in privileged bridge"}
```

## Implementation status

| Element | Status |
|---------|--------|
| stdin/stdout loop | ✅ Implemented |
| Allowlist validation | ✅ Implemented |
| Ping/Pong | ✅ Implemented |
| `systemd.unit.action` handler | ⚠️ Stub (Ready+Close) |
| `package.updates` handler | ⚠️ Stub (Ready+Close) |

## systemd configuration

File `systemd/tenodera-priv-bridge.service`:

```ini
[Service]
Type=simple
ExecStart=/usr/local/bin/tenodera-priv-bridge
StandardInput=socket
User=root

# Hardening
NoNewPrivileges=false          # Requires escalation
ProtectSystem=strict
ProtectHome=read-only
PrivateTmp=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectControlGroups=yes
RestrictSUIDSGID=yes
```

## Dependencies

- `tenodera-protocol` — shared message types
- `tokio` — runtime (choć pętla główna jest synchroniczna)
- `serde` + `serde_json` — JSON
- `nix 0.29` — operacje systemowe
- `tracing` — logowanie

## Planowany rozwój

Docelowa architektura priv-bridge:
1. **Allowlist rozszerzalny** — dodawanie nowych dozwolonych operacji
2. **Właściwe handlery** — bezpośrednie wywołania systemowe jako root (bez sudo)
3. **Auditing** — logowanie każdej operacji z timestampem i danymi
4. **Rate limiting** — ograniczenie częstotliwości operacji
