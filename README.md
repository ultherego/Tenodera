# Tenodera

A self-hosted Linux server administration panel with real-time monitoring, terminal access, and multi-host management — all from a single web interface.

```
Browser ──WSS──▶ Gateway (:9090) ──spawn──▶ Bridge (per-user)
                                            ──SSH──▶ Agent (:9091) on remote host
```

## Components

| Component | Description |
|-----------|-------------|
| **Tenodera Panel** | Central server: gateway (auth + WebSocket routing), per-user bridge processes, and React web UI |
| **Tenodera Agent** | Lightweight daemon installed on each managed host — exposes system data over WebSocket |

### What you get

- **Dashboard** — CPU, RAM, disk, network in real-time (streaming metrics)
- **Terminal** — Full PTY shell in the browser (xterm.js)
- **Services** — systemd unit management (start/stop/restart/enable/disable)
- **Packages** — View installed packages, check for updates
- **Storage** — Disk usage, mount points, block devices
- **Networking** — Interfaces, addresses, connections, firewall
- **Containers** — Docker/Podman container listing
- **Files** — Remote file browser
- **Logs** — Live journald log viewer with filtering
- **Log Files** — Browse and search `/var/log` files with keyword search, context lines, and date+time range filtering
- **Kernel Dump** — View kdump status, crash kernel config, and browse crash dumps with dmesg output
- **Multi-host** — Manage multiple servers from one panel

## Requirements

- **OS:** Linux (Debian/Ubuntu, Fedora/RHEL, Arch)
- **Rust:** Installed automatically via `make deps`
- **Node.js 18+:** Installed automatically via `make deps` (Panel only)
- **System libs:** `build-essential`, `pkg-config`, `libssl-dev`, `libpam0g-dev`, `sshpass`
- **Make:** `sudo apt install make` / `sudo dnf install make` / included on Arch

## Installation

### Panel (central server)

```bash
cd "Tenodera Panel"
sudo make all
```

This runs `deps` → `build` → `install` in one step:
- Installs Rust, Node.js, and system dependencies
- Compiles gateway + bridge (Rust) and UI (React/Vite)
- Installs binaries to `/usr/local/bin/`
- Deploys UI to `/usr/share/tenodera/ui/`
- Creates config dir `/etc/tenodera/tls/`
- Enables and starts `tenodera-gateway.service` on port **9090**

Individual steps if preferred:

```bash
make deps      # install build dependencies
make build     # compile backend + frontend
sudo make install   # install and start service
```

### Agent (on each managed host)

```bash
cd "Tenodera Agent"
sudo make all
```

This installs and starts `tenodera-agent.service` on port **9091** (localhost-only by default).

Individual steps:

```bash
make deps      # install Rust + system libs
make build     # compile agent
sudo make install   # install binary, config, start service
```

### Uninstall

```bash
# Panel
cd "Tenodera Panel"
sudo make uninstall

# Agent
cd "Tenodera Agent"
sudo make uninstall
```

## Configuration

### Agent — `/etc/tenodera/agent.toml`

```toml
# Address to listen on
bind = "0.0.0.0:9091"

# API key for panel authentication (empty = allow all — NOT for production)
api_key = ""

# TLS (optional, recommended for production)
# tls_cert = "/etc/tenodera/cert.pem"
# tls_key  = "/etc/tenodera/key.pem"

# Allow running without TLS
allow_unencrypted = true
```

### Panel — Environment variables

Set via `systemctl edit tenodera-gateway`:

| Variable | Description | Default |
|----------|-------------|---------|
| `TENODERA_BIND` | Listen address | `0.0.0.0:9090` |
| `TENODERA_BRIDGE_BIN` | Path to bridge binary | `/usr/local/bin/tenodera-bridge` |
| `TENODERA_UI_DIR` | Path to built UI | `/usr/share/tenodera/ui` |
| `TENODERA_TLS_CERT` | TLS certificate path | *(unset — plain HTTP)* |
| `TENODERA_TLS_KEY` | TLS private key path | *(unset — plain HTTP)* |

### TLS (production)

```bash
sudo systemctl edit tenodera-gateway
```

```ini
[Service]
Environment=TENODERA_TLS_CERT=/etc/tenodera/tls/cert.pem
Environment=TENODERA_TLS_KEY=/etc/tenodera/tls/key.pem
```

```bash
sudo systemctl restart tenodera-gateway
```

## Authentication & Permissions

### Login

The panel authenticates against **PAM** — you log in with any valid Linux system user credentials on the gateway host.

### Required user permissions

| Action | Permission needed |
|--------|-------------------|
| View dashboard, metrics, logs | Any authenticated user |
| Terminal (PTY) | SSH access to the remote host |
| Start/stop/restart systemd services | `sudo` on the remote host |
| Install/update packages | `sudo` on the remote host |
| Browse files | Read permissions on the remote host |

### How it works

1. User logs in via PAM on the gateway
2. Gateway spawns a **per-user bridge process** (privilege isolation)
3. Bridge connects to the remote agent via SSH using the session credentials (`sshpass`)
4. Privileged operations (systemd actions, package updates) go through `tenodera-priv-bridge` — a **root-level helper with a strict allowlist**

### priv-bridge allowlist

The privileged bridge only allows:
- `systemd.unit.action` — start/stop/restart/enable/disable units
- `package.updates` — check and apply updates

All other requests are rejected.

## Service management

```bash
# Panel
sudo systemctl status tenodera-gateway
sudo systemctl restart tenodera-gateway
journalctl -u tenodera-gateway -f

# Agent
sudo systemctl status tenodera-agent
sudo systemctl restart tenodera-agent
journalctl -u tenodera-agent -f
```

## Development (Vagrant)

A Vagrantfile is included for local testing with 6 Debian VMs:

```bash
cd "Tenodera Panel"
export TENODERA_USER="tenodera"
export TENODERA_PASS="your-password"
vagrant up
```

Each VM gets: Rust toolchain, SSH with password auth, sudo access.

| VM | IP |
|----|----|
| tenodera-remote-1 | 192.168.56.10 |
| tenodera-remote-2 | 192.168.56.11 |
| tenodera-remote-3 | 192.168.56.12 |
| tenodera-remote-4 | 192.168.56.13 |
| tenodera-remote-5 | 192.168.56.14 |
| tenodera-remote-6 | 192.168.56.15 |

## Optional: Kernel Dump (kdump) setup

The **Kernel Dump** page in the panel shows crash dump status from managed hosts. It works out of the box (showing "not installed" if kdump is absent), but to get full functionality you need to install and configure kdump on the target hosts.

### Debian / Ubuntu

```bash
sudo apt install -y kdump-tools crash kexec-tools makedumpfile

# Add crashkernel parameter to GRUB
sudo sed -i 's/GRUB_CMDLINE_LINUX_DEFAULT="\(.*\)"/GRUB_CMDLINE_LINUX_DEFAULT="\1 crashkernel=256M"/' /etc/default/grub
sudo update-grub

# Enable the service and reboot
sudo systemctl enable kdump-tools
sudo reboot
```

### Fedora / RHEL / CentOS

```bash
sudo dnf install -y kexec-tools crash

# Set crashkernel (Fedora 35+)
sudo kdumpctl reset-crashkernel
# Or manually:
# sudo grubby --args="crashkernel=256M" --update-kernel=ALL

# Enable the service and reboot
sudo systemctl enable kdump
sudo reboot
```

### Arch Linux

kdump on Arch requires AUR packages and manual kernel parameter setup:

```bash
# Install from AUR (using yay or paru)
yay -S simple-kdump   # or: yay -S kdumpst

# Install crash analysis tool (official repo)
sudo pacman -S crash

# Add crashkernel parameter (512M recommended on Arch)
# Edit your boot loader config and append: crashkernel=512M
# For GRUB:
sudo sed -i 's/GRUB_CMDLINE_LINUX_DEFAULT="\(.*\)"/GRUB_CMDLINE_LINUX_DEFAULT="\1 crashkernel=512M"/' /etc/default/grub
sudo grub-mkconfig -o /boot/grub/grub.cfg

# Enable the service (simple-kdump) and reboot
sudo systemctl enable simple-kdump-setup
sudo reboot
```

### Verify

After reboot, confirm kdump is working:

```bash
# Should return "1"
cat /sys/kernel/kexec_crash_loaded

# Should show reserved memory (> 0)
cat /sys/kernel/kexec_crash_size

# Check service status
systemctl status kdump-tools   # Debian/Ubuntu
systemctl status kdump         # Fedora/RHEL
```

Crash dumps are saved to `/var/crash/` (all distros) or `/var/lib/kdump/`.

## Project structure

```
Tenodera Agent/          Remote host agent
├── src/                 Rust source (handlers, protocol, TLS, auth)
├── systemd/             Service file
├── config.example.toml  Example configuration
└── Makefile             Build & install

Tenodera Panel/          Central server
├── crates/
│   ├── gateway/         HTTP/WebSocket gateway + PAM auth
│   ├── bridge/          Per-user bridge with 20 handler types
│   ├── priv-bridge/     Privileged root helper (strict allowlist)
│   └── protocol/        Shared message types & payloads
├── ui/                  React + TypeScript SPA (Vite)
├── systemd/             Service files
├── Vagrantfile          Dev environment (6 Debian VMs)
└── Makefile             Build & install
```

## License

This project is licensed under the [MIT License](LICENSE).
