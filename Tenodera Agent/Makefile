# Tenodera Agent — Makefile
# Usage:
#   sudo apt-get install make  (Debian) / sudo dnf install make (Fedora) / already included (Arch)
#   make all       — deps + build + install (full setup)
#   make deps      — install build dependencies (Rust + system libs)
#   make build     — compile agent in release mode
#   make install   — install binary, default config, systemd service
#   make register  — register this agent with Tenodera Panel
#   make uninstall — remove binary, service (keeps config)
#   make clean     — remove build artifacts
#
# Auto-registration (optional — adds agent to panel during install):
#   make install PANEL_HOST=192.168.56.10
#
# Registration parameters:
#   PANEL_HOST   — Panel IP/hostname (required for registration)
#   PANEL_USER   — SSH user for panel host (default: current user)
#   AGENT_NAME   — Friendly name (default: hostname)
#   AGENT_IP     — This agent's IP as seen by panel (default: auto-detect)
#   AGENT_PORT   — Agent listen port (default: 9091)
#   TRANSPORT    — "agent" (WebSocket+API key) or "ssh" (default: agent)

PREFIX      ?= /usr/local
BINDIR      ?= $(PREFIX)/bin
CONFDIR     ?= /etc/tenodera
SYSTEMDDIR  ?= /etc/systemd/system
BINARY      := target/release/tenodera-agent

# Registration defaults
PANEL_HOST  ?=
PANEL_USER  ?= $(or $(SUDO_USER),$(shell whoami))
AGENT_NAME  ?= $(shell hostname -s)
AGENT_IP    ?= $(if $(PANEL_HOST),$(shell ip route get $(PANEL_HOST) 2>/dev/null | sed -n 's/.*src \([^ ]*\).*/\1/p'),$(shell hostname -I 2>/dev/null | awk '{print $$1}'))
AGENT_PORT  ?= 9091
TRANSPORT   ?= agent

.PHONY: all deps build install register uninstall clean

all: deps build install

# ── Dependencies ──────────────────────────────────────────

deps:
	@echo "==> Installing build dependencies..."
	@if command -v apt-get >/dev/null 2>&1; then \
		sudo apt-get update && sudo apt-get install -y build-essential pkg-config libssl-dev curl jq; \
	elif command -v dnf >/dev/null 2>&1; then \
		sudo dnf install -y gcc make pkg-config openssl-devel curl jq; \
	elif command -v pacman >/dev/null 2>&1; then \
		sudo pacman -Sy --needed --noconfirm base-devel openssl pkg-config curl jq; \
	else \
		echo "ERROR: Unsupported package manager. Install manually: gcc, make, pkg-config, openssl-dev, curl"; \
		exit 1; \
	fi
	@if ! command -v cargo >/dev/null 2>&1; then \
		echo "==> Installing Rust..."; \
		curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y; \
	else \
		echo "==> Rust already installed: $$(rustc --version)"; \
	fi

# ── Build ─────────────────────────────────────────────────

build:
	@echo "==> Building tenodera-agent (release)..."
	. "$$HOME/.cargo/env" 2>/dev/null || true; cargo build --release

# ── Install ───────────────────────────────────────────────

install: $(BINARY)
	@echo "==> Installing tenodera-agent..."
	sudo install -m 755 $(BINARY) $(BINDIR)/tenodera-agent
	@echo "==> Creating default config (if not exists)..."
	sudo mkdir -p $(CONFDIR)
	@if [ ! -f $(CONFDIR)/agent.toml ]; then \
		printf 'bind = "0.0.0.0:9091"\napi_key = ""\nallow_unencrypted = true\n' \
			| sudo tee $(CONFDIR)/agent.toml > /dev/null; \
		echo "    Created $(CONFDIR)/agent.toml (all interfaces, no API key)"; \
	else \
		echo "    $(CONFDIR)/agent.toml already exists — skipping"; \
	fi
	@echo "==> Installing systemd service..."
	@printf '[Unit]\n\
Description=Tenodera Agent\n\
After=network.target\n\
\n\
[Service]\n\
Type=simple\n\
ExecStart=$(BINDIR)/tenodera-agent\n\
Restart=on-failure\n\
RestartSec=5\n\
\n\
[Install]\n\
WantedBy=multi-user.target\n' | sudo tee $(SYSTEMDDIR)/tenodera-agent.service > /dev/null
	sudo systemctl daemon-reload
	sudo systemctl enable --now tenodera-agent
	@echo "==> Installing logrotate config..."
	sudo install -m 644 logrotate/tenodera /etc/logrotate.d/tenodera
	@echo ""
	@echo "✓ tenodera-agent installed and running"
	@echo "  Config:  $(CONFDIR)/agent.toml"
	@echo "  Service: systemctl status tenodera-agent"
	@echo "  Health:  curl http://127.0.0.1:9091/health"
	@if [ -n "$(PANEL_HOST)" ]; then \
		$(MAKE) register; \
	else \
		echo ""; \
		echo "  To register with a panel: make register PANEL_HOST=<ip>"; \
	fi

$(BINARY):
	$(MAKE) build

# ── Register with Panel ───────────────────────────────────

register:
	@if [ -z "$(PANEL_HOST)" ]; then \
		echo "ERROR: PANEL_HOST is required. Usage: make register PANEL_HOST=<ip>"; \
		exit 1; \
	fi
	@echo "==> Registering agent with panel at $(PANEL_HOST)..."
	@HOSTS_FILE=/etc/tenodera/hosts.json; \
	AGENT_ID=$$(cat /proc/sys/kernel/random/uuid 2>/dev/null || uuidgen); \
	TIMESTAMP=$$(date -u +%Y-%m-%dT%H:%M:%SZ); \
	echo "    Agent: $(AGENT_NAME) @ $(AGENT_IP):$(AGENT_PORT)"; \
	echo "    Transport: $(TRANSPORT)"; \
	echo "    Panel: $(PANEL_USER)@$(PANEL_HOST)"; \
	if ! command -v ssh >/dev/null 2>&1; then \
		echo "ERROR: ssh is required for registration"; exit 1; \
	fi; \
	if [ -n "$(SUDO_USER)" ]; then \
		SSH="sudo -u $(SUDO_USER) ssh"; \
	else \
		SSH="ssh"; \
	fi; \
	API_KEY=""; \
	if [ "$(TRANSPORT)" = "agent" ]; then \
		API_KEY=$$(openssl rand -hex 32); \
		echo "    API key: $$(printf '%.8s' $$API_KEY)...(truncated)"; \
		sudo sed -i 's/^api_key = .*/api_key = "'$$API_KEY'"/' $(CONFDIR)/agent.toml; \
		echo "    Updated $(CONFDIR)/agent.toml with API key"; \
		sudo systemctl restart tenodera-agent; \
		echo "    Restarted tenodera-agent"; \
	fi; \
	CURRENT=$$($$SSH $(PANEL_USER)@$(PANEL_HOST) "sudo cat $$HOSTS_FILE 2>/dev/null"); \
	if [ -z "$$CURRENT" ] || ! echo "$$CURRENT" | jq empty 2>/dev/null; then \
		CURRENT='{"hosts":[]}'; \
	fi; \
	if echo "$$CURRENT" | jq -e ".hosts[] | select(.address==\"$(AGENT_IP)\" and .agent_port==$(AGENT_PORT))" >/dev/null 2>&1; then \
		echo "    Agent $(AGENT_IP):$(AGENT_PORT) already registered — skipping"; \
	else \
		UPDATED=$$(echo "$$CURRENT" | jq --arg id "$$AGENT_ID" --arg name "$(AGENT_NAME)" --arg addr "$(AGENT_IP)" --arg ts "$$TIMESTAMP" --arg key "$$API_KEY" --arg transport "$(TRANSPORT)" \
			'.hosts += [{id:$$id,name:$$name,address:$$addr,user:"",ssh_port:22,added_at:$$ts,transport:$$transport,agent_port:$(AGENT_PORT),api_key:$$key,agent_tls:false}]'); \
		if [ -z "$$UPDATED" ]; then \
			echo "ERROR: Failed to build hosts JSON — is jq installed locally?"; exit 1; \
		fi; \
		echo "$$UPDATED" | $$SSH $(PANEL_USER)@$(PANEL_HOST) "sudo tee $$HOSTS_FILE > /dev/null"; \
		echo "✓ Agent registered with panel"; \
	fi

# ── Uninstall ─────────────────────────────────────────────

uninstall:
	@echo "==> Stopping and removing tenodera-agent..."
	-sudo systemctl stop tenodera-agent
	-sudo systemctl disable tenodera-agent
	-sudo rm -f $(SYSTEMDDIR)/tenodera-agent.service
	-sudo systemctl daemon-reload
	-sudo rm -f $(BINDIR)/tenodera-agent
	-sudo rm -rf $(CONFDIR)
	-sudo rm -f /etc/logrotate.d/tenodera
	-sudo rm -f /var/log/tenodera*
	@echo "✓ tenodera-agent fully removed"

# ── Clean ─────────────────────────────────────────────────

clean:
	cargo clean
