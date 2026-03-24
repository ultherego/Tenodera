# systemd files

systemd service files for running Tenodera components as system daemons.

## tenodera-gateway.service

Main service — HTTP/WebSocket server on port 9090. Spawns `tenodera-bridge` per session.

```ini
[Unit]
Description=Tenodera Web Console Gateway
Documentation=https://github.com/tenodera
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/tenodera-gateway
Restart=on-failure
RestartSec=5

# Environment variables
Environment=RUST_LOG=tenodera_gateway=info
Environment=TENODERA_BRIDGE_BIN=/usr/local/bin/tenodera-bridge
Environment=TENODERA_UI_DIR=/usr/share/tenodera/ui
# TLS — uncomment and set paths for production:
# Environment=TENODERA_TLS_CERT=/etc/tenodera/tls/cert.pem
# Environment=TENODERA_TLS_KEY=/etc/tenodera/tls/key.pem
# Environment=TENODERA_ALLOW_UNENCRYPTED=0

# Security hardening
ProtectSystem=full
ReadWritePaths=/etc/tenodera /var/log
PrivateTmp=yes
ProtectKernelTunables=yes
ProtectControlGroups=yes
LockPersonality=yes

[Install]
WantedBy=multi-user.target
```

### Hardening — directive explanations

| Directive | Description |
|-----------|-------------|
| `ProtectSystem=full` | /usr, /boot, /efi, /etc read-only (except paths listed in ReadWritePaths) |
| `ReadWritePaths=/etc/tenodera /var/log` | Allow writes to hosts.json config and audit log files |
| `PrivateTmp=yes` | Isolated /tmp |
| `ProtectKernelTunables=yes` | Block /proc/sys, /sys writes |
| `ProtectControlGroups=yes` | Block writes to /sys/fs/cgroup |
| `LockPersonality=yes` | Block execution domain changes |

## Installation

```bash
# Copy binaries
sudo cp target/release/tenodera-gateway /usr/local/bin/
sudo cp target/release/tenodera-bridge /usr/local/bin/

# Copy service files
sudo cp systemd/tenodera-gateway.service /etc/systemd/system/

# Install UI
sudo mkdir -p /usr/share/tenodera/ui
sudo cp -r ui/dist/* /usr/share/tenodera/ui/

# Start
sudo systemctl daemon-reload
sudo systemctl enable --now tenodera-gateway
```

## Configuration

All settings via environment variables in the service file:

| Variable | Default | Description |
|----------|---------|-------------|
| `TENODERA_BIND_ADDR` | `127.0.0.1` | Listen address |
| `TENODERA_BIND_PORT` | `9090` | Listen port |
| `TENODERA_BRIDGE_BIN` | `tenodera-bridge` | Path to bridge binary |
| `TENODERA_UI_DIR` | `ui/dist` | Path to built frontend |
| `TENODERA_TLS_CERT` | (none) | Path to PEM certificate |
| `TENODERA_TLS_KEY` | (none) | Path to PEM private key |
| `TENODERA_ALLOW_UNENCRYPTED` | `true` | Allow HTTP without TLS |
| `TENODERA_IDLE_TIMEOUT` | `900` | Session timeout (seconds) |
| `TENODERA_MAX_STARTUPS` | `20` | Max concurrent connections |
| `RUST_LOG` | (none) | Log filter (e.g. `info`, `tenodera_gateway=debug`) |

### Overriding variables

```bash
sudo systemctl edit tenodera-gateway
# Add:
# [Service]
# Environment=TENODERA_TLS_CERT=/etc/tenodera/cert.pem
# Environment=TENODERA_TLS_KEY=/etc/tenodera/key.pem
# Environment=TENODERA_ALLOW_UNENCRYPTED=false
```

## Logs

```bash
journalctl -u tenodera-gateway -f     # live gateway logs
```
