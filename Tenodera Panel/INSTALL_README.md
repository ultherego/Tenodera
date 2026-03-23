# Tenodera — Installation

Tenodera consists of two independent components:

- **Panel** — central web server + UI (installed on **one** host)
- **Agent** — system management daemon (installed on **each** managed host)

## Connection architecture

The Panel connects to agents in two ways:

### SSH mode (default) — Cockpit model

The agent listens **only on localhost** (`127.0.0.1:9091`). The panel opens an SSH tunnel
to the managed host using `sshpass`, then connects to the agent through that tunnel.
The user's login password is stored in the session and used for SSH authentication
(same model as Cockpit). Requires `PasswordAuthentication yes` in sshd on remote hosts.
Does not require API keys or opening agent ports externally.

```
┌────────────────┐      SSH tunnel       ┌──────────────────────────────┐
│                │ ═══════════════════►   │  Host A                     │
│  Panel :9090   │   ssh -N -L ...       │  Agent 127.0.0.1:9091       │
│  (gateway+UI)  │ ═══════════════════►   │  Host B                     │
│                │      port 22          │  Agent 127.0.0.1:9091       │
└────────────────┘                       └──────────────────────────────┘
     1 server                              N hosts (port 9091 closed)
```

### Direct mode (alternative) — network-accessible agent

The agent listens on `0.0.0.0:9091` with an API key and optional TLS.
The panel connects directly via WebSocket. Requires opening port 9091.

```
┌────────────────┐     WebSocket (TLS)    ┌───────────────────────────┐
│  Panel :9090   │ ─────────────────────► │  Agent 0.0.0.0:9091 (API) │
└────────────────┘                        └───────────────────────────┘
```

---

## Requirements

### System

| Component | Requirements |
|-----------|-------------|
| Panel | Linux (x86_64/aarch64), systemd, PAM, `make`, `sshpass` |
| Agent | Linux (x86_64/aarch64), systemd, `make` |

### Hardware (minimum)

| Component | CPU | RAM | Disk |
|-----------|-----|-----|------|
| Panel | 1 vCPU | 512 MB | 200 MB |
| Agent | 1 vCPU | 128 MB | 50 MB |
| Building Panel | 2 vCPU | 4 GB | 2 GB |
| Building Agent | 2 vCPU | 2 GB | 1 GB |

> Disk requirements refer to the installed component. Building requires more space temporarily
> (Rust compiler + dependencies), which can be freed after installation (`make clean`).

> Build dependencies (Rust, Node.js, system libraries) are installed automatically
> by `make deps`. On a clean Debian, you need to install `make` first:
> `sudo apt-get update && sudo apt-get install -y make`

---

## 1. Quick installation (Makefile)

Each component has its own `Makefile` with full automation: dependencies → build → install.

### 1.1. Clone the repository

```bash
git clone <repo-url> tenodera
cd tenodera
```

### 1.2. Install the Panel (one server)

```bash
cd "Tenodera Panel"
make all
```

`make all` automatically:
1. Installs system dependencies (`build-essential`, `pkg-config`, `libssl-dev`, `libpam0g-dev`, `sshpass`)
2. Installs Rust (if missing)
3. Installs Node.js 22 (if missing)
4. Builds backend (`tenodera-gateway`, `tenodera-bridge`) and frontend (React UI)
5. Copies binaries to `/usr/local/bin/`, UI to `/usr/share/tenodera/ui/`
6. Installs systemd service with `TENODERA_BIND=0.0.0.0:9090`
7. Starts `tenodera-gateway`

Panel available at: `http://<server-address>:9090`

Login via PAM — use system username and password.

Available Makefile targets:

| Target | Description |
|--------|-------------|
| `make all` | Full installation (deps + build + install) |
| `make deps` | Dependencies only (system + Rust + Node.js) |
| `make build` | Build only (backend + frontend) |
| `make install` | Install only (binaries + UI + systemd) |
| `make uninstall` | Remove (binaries + services; config is kept) |
| `make clean` | Remove build artifacts |

### 1.3. Install the Agent (each managed host)

> **Important:** The agent must be built on a system with the same (or older) glibc version as the target hosts.
> If you build on a newer system (e.g. Arch/Fedora) and deploy to an older one (e.g. Debian 12),
> **build the agent directly on the target host**.

Copy the `Tenodera Agent` directory to the target host and run:

```bash
cd "Tenodera Agent"
make all
```

`make all` automatically:
1. Installs system dependencies (`build-essential`, `pkg-config`, `libssl-dev`)
2. Installs Rust (if missing)
3. Builds `tenodera-agent` in release mode
4. Copies the binary to `/usr/local/bin/`
5. Creates default configuration `/etc/tenodera/agent.toml` (localhost, no API key)
6. Installs and starts the systemd service

Verification:

```bash
curl http://127.0.0.1:9091/health
# Expected output: ok
```

Available Makefile targets:

| Target | Description |
|--------|-------------|
| `make all` | Full installation (deps + build + install) |
| `make deps` | Dependencies only (system + Rust) |
| `make build` | Build only |
| `make install` | Install only (binary + config + systemd) |
| `make uninstall` | Remove (binary + service; config is kept) |
| `make clean` | Remove build artifacts |

---

## 2. Manual installation (alternative)

If you prefer to install without the Makefile.

### 2.1. Building the Panel

```bash
cd "Tenodera Panel"

# Backend (gateway + bridge)
cargo build --release

# Frontend (React UI)
cd ui
npm install
npm run build
cd ..
```

Resulting binaries:
- `target/release/tenodera-gateway` — HTTP/WebSocket server
- `target/release/tenodera-bridge` — local bridge (needed on the panel server)
- `ui/dist/` — built frontend

### 2.2. Installing the Panel

```bash
# Binaries
sudo install -m 755 target/release/tenodera-gateway /usr/local/bin/
sudo install -m 755 target/release/tenodera-bridge  /usr/local/bin/

# Frontend
sudo mkdir -p /usr/share/tenodera/ui
sudo cp -r ui/dist/* /usr/share/tenodera/ui/

# Configuration directories
sudo mkdir -p /etc/tenodera/tls

# systemd service
sudo cp systemd/tenodera-gateway.service /etc/systemd/system/

# Override — listen on all interfaces
sudo mkdir -p /etc/systemd/system/tenodera-gateway.service.d
printf '[Service]\nEnvironment=TENODERA_BIND=0.0.0.0:9090\n' \
  | sudo tee /etc/systemd/system/tenodera-gateway.service.d/bind.conf > /dev/null

sudo systemctl daemon-reload
sudo systemctl enable --now tenodera-gateway
```

### 2.3. Building the Agent

```bash
cd "Tenodera Agent"
cargo build --release
```

Resulting binary: `target/release/tenodera-agent`

### 2.4. Installing the Agent

```bash
sudo install -m 755 target/release/tenodera-agent /usr/local/bin/

# Default config (if not exists)
sudo mkdir -p /etc/tenodera
if [ ! -f /etc/tenodera/agent.toml ]; then
  printf 'bind = "127.0.0.1:9091"\napi_key = ""\nallow_unencrypted = true\n' \
    | sudo tee /etc/tenodera/agent.toml > /dev/null
fi

# systemd service
cat << 'EOF' | sudo tee /etc/systemd/system/tenodera-agent.service
[Unit]
Description=Tenodera Agent
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/tenodera-agent
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable --now tenodera-agent
```

---

## 3. Agent configuration

### SSH mode (default — agent on localhost)

Default configuration created by `make install`:

```toml
# /etc/tenodera/agent.toml
bind = "127.0.0.1:9091"
api_key = ""
allow_unencrypted = true
```

The agent listens only on localhost — not accessible from the network.
Empty `api_key` means no authorization (safe, since access is only through SSH).

### Direct mode (network-accessible agent)

Edit `/etc/tenodera/agent.toml`:

```toml
bind = "0.0.0.0:9091"
api_key = "<API_KEY>"
allow_unencrypted = false
tls_cert = "/etc/tenodera/cert.pem"
tls_key = "/etc/tenodera/key.pem"
```

Generate an API key and TLS certificate:

```bash
# API key
openssl rand -hex 32

# TLS certificate (self-signed)
sudo openssl req -x509 -newkey rsa:4096 \
  -keyout /etc/tenodera/key.pem \
  -out /etc/tenodera/cert.pem \
  -days 365 -nodes -subj "/CN=$(hostname)"
sudo chmod 600 /etc/tenodera/key.pem

sudo systemctl restart tenodera-agent
```

> **Remember the API key** — you need to provide the same one when adding the host in the panel.

---

## 4. Panel TLS configuration (production)

In production, the panel should run with TLS:

```bash
# Self-signed (for testing)
sudo openssl req -x509 -newkey rsa:4096 \
  -keyout /etc/tenodera/tls/key.pem \
  -out /etc/tenodera/tls/cert.pem \
  -days 365 -nodes -subj "/CN=tenodera-panel"

# Let's Encrypt (production) — use certbot, copy cert and key to /etc/tenodera/tls/
```

Enable TLS in the systemd service:

```bash
sudo systemctl edit tenodera-gateway
```

```ini
[Service]
Environment=TENODERA_TLS_CERT=/etc/tenodera/tls/cert.pem
Environment=TENODERA_TLS_KEY=/etc/tenodera/tls/key.pem
Environment=TENODERA_ALLOW_UNENCRYPTED=0
```

```bash
sudo systemctl restart tenodera-gateway
```

Panel available at: `https://<server-address>:9090`

---

## 5. Development run (without installation)

```bash
cd "Tenodera Panel"
make build

RUST_LOG=info TENODERA_BRIDGE_BIN=target/release/tenodera-bridge \
  target/release/tenodera-gateway
```

Panel available at: `http://127.0.0.1:9090`

---

## 6. Adding a host in the panel

Hosts are added through the panel web interface: **Hosts → Add host**.

### SSH mode (default)

| Field | Value |
|-------|-------|
| ID | unique name (e.g. `web-1`) |
| Address | host IP or hostname |
| Transport | SSH Tunnel |
| SSH User | SSH user (default: panel session user) |
| SSH Port | `22` |
| Agent Port | `9091` |

> **Note:** The user's login password is used for SSH authentication to remote hosts
> (Cockpit model — `sshpass`). The remote host must have `PasswordAuthentication yes` enabled in sshd.
> The panel opens an SSH tunnel on behalf of the logged-in user.

### Direct mode

| Field | Value |
|-------|-------|
| ID | unique name |
| Address | host IP or hostname |
| Transport | Direct (API key) |
| Agent Port | `9091` |
| API Key | key from the host's `agent.toml` |
| TLS | check if agent has a certificate |

Hosts can also be added manually in the `~/.config/tenodera/hosts.json` file:

```json
{
  "hosts": [
    {
      "id": "web-1",
      "address": "192.168.1.10",
      "transport": "ssh",
      "user": "",
      "ssh_port": 22,
      "agent_port": 9091
    },
    {
      "id": "db-1",
      "address": "192.168.1.20",
      "transport": "agent",
      "agent_port": 9091,
      "api_key": "a3f8c2e1d4b5...",
      "agent_tls": true
    }
  ]
}
```

| Field | Description | Default |
|-------|-------------|---------|
| `id` | Unique host name | (required) |
| `address` | IP or hostname | (required) |
| `transport` | `"ssh"` (SSH tunnel) or `"agent"` (direct) | `"ssh"` |
| `user` | SSH user | (session user) |
| `ssh_port` | SSH port | `22` |
| `agent_port` | Agent port on host | `9091` |
| `api_key` | API key (Direct mode only) | `""` |
| `agent_tls` | Agent TLS (Direct mode only) | `false` |

---

## 7. Firewall

### On the panel server

```bash
# Panel port (UI + API)
sudo ufw allow 9090/tcp    # or: firewall-cmd --permanent --add-port=9090/tcp
```

### On agent hosts

**SSH mode** — only SSH port (22) needs to be open. Agent is on localhost — port 9091 does **not** need to be open.

**Direct mode** — port 9091 must be open:
```bash
sudo ufw allow 9091/tcp    # or: firewall-cmd --permanent --add-port=9091/tcp
```

---

## 8. Mass agent deployment (SSH mode)

Script for quick deployment to multiple hosts using the Makefile:

```bash
#!/bin/bash
# deploy-agent.sh
HOSTS="192.168.1.10 192.168.1.11 192.168.1.12"
AGENT_SRC="Tenodera Agent"

for HOST in $HOSTS; do
    echo "==> $HOST"

    # Copy sources + Makefile to host
    ssh root@"$HOST" "mkdir -p /tmp/tenodera-agent-src"
    scp -r "$AGENT_SRC/src" "$AGENT_SRC/Cargo.toml" "$AGENT_SRC/Makefile" \
        root@"$HOST":/tmp/tenodera-agent-src/

    # Install (deps + build + install) in one command
    ssh root@"$HOST" "cd /tmp/tenodera-agent-src && make all && rm -rf /tmp/tenodera-agent-src"

    echo "    OK — add host in panel: id=$HOST, address=$HOST, transport=ssh"
done
```

---

## 9. Environment variables

### Panel (gateway)

| Variable | Description | Default |
|----------|-------------|---------|
| `TENODERA_BIND` | Listen address | `127.0.0.1:9090` |
| `TENODERA_TLS_CERT` | Path to PEM certificate | (none) |
| `TENODERA_TLS_KEY` | Path to PEM private key | (none) |
| `TENODERA_ALLOW_UNENCRYPTED` | `1`/`true` — allow without TLS | `true` (dev) |
| `TENODERA_BRIDGE_BIN` | Path to bridge binary | `tenodera-bridge` |
| `TENODERA_UI_DIR` | Built UI directory | `ui/dist` |
| `RUST_LOG` | Log level | `info` |

### Agent

| Variable | Description | Default |
|----------|-------------|---------|
| `TENODERA_AGENT_CONFIG` | Path to config file | `/etc/tenodera/agent.toml` |
| `TENODERA_AGENT_BIND` | Listen address (overrides config) | `127.0.0.1:9091` |
| `TENODERA_AGENT_API_KEY` | API key (overrides config) | `""` |
| `TENODERA_AGENT_TLS_CERT` | TLS certificate (overrides config) | (none) |
| `TENODERA_AGENT_TLS_KEY` | TLS key (overrides config) | (none) |
| `TENODERA_AGENT_ALLOW_UNENCRYPTED` | `1`/`true` (overrides config) | `true` |
| `RUST_LOG` | Log level | `info` |

---

## 10. Diagnostics

```bash
# Panel logs
journalctl -u tenodera-gateway -f

# Agent logs (on the remote host)
journalctl -u tenodera-agent -f

# Agent health check (on the agent host)
curl http://127.0.0.1:9091/health

# SSH tunnel test (from the panel server)
ssh -N -L 19091:127.0.0.1:9091 user@HOST &
curl http://127.0.0.1:19091/health
kill %1

# Direct test (Direct mode, from the panel server)
curl -k -H "Authorization: Bearer <API_KEY>" https://HOST:9091/health
```

---

## 11. Updating

### Agent (with Makefile)

```bash
cd "Tenodera Agent"
git pull
make build
sudo make install
```

### Agent (manual)

```bash
cd "Tenodera Agent"
git pull && cargo build --release
sudo cp target/release/tenodera-agent /usr/local/bin/
sudo systemctl restart tenodera-agent
```

### Panel (with Makefile)

```bash
cd "Tenodera Panel"
git pull
make build
sudo make install
```

### Panel (manual)

```bash
cd "Tenodera Panel"
git pull
cargo build --release
cd ui && npm run build && cd ..

sudo cp target/release/tenodera-gateway /usr/local/bin/
sudo cp target/release/tenodera-bridge /usr/local/bin/
sudo cp -r ui/dist/* /usr/share/tenodera/ui/
sudo systemctl restart tenodera-gateway
```

---

## 12. Uninstalling

### Agent

```bash
cd "Tenodera Agent"
make uninstall
```

### Panel

```bash
cd "Tenodera Panel"
make uninstall
```

Configuration in `/etc/tenodera/` is not removed automatically. To remove completely:

```bash
sudo rm -rf /etc/tenodera
```
