# Tenodera Agent — Makefile
# Usage:
#   sudo apt-get install make  (Debian) / sudo dnf install make (Fedora) / already included (Arch)
#   make all       — deps + build + install (full setup)
#   make deps      — install build dependencies (Rust + system libs)
#   make build     — compile agent in release mode
#   make install   — install binary, default config, systemd service
#   make uninstall — remove binary, service (keeps config)
#   make clean     — remove build artifacts

PREFIX      ?= /usr/local
BINDIR      ?= $(PREFIX)/bin
CONFDIR     ?= /etc/tenodera
SYSTEMDDIR  ?= /etc/systemd/system
BINARY      := target/release/tenodera-agent

.PHONY: all deps build install uninstall clean

all: deps build install

# ── Dependencies ──────────────────────────────────────────

deps:
	@echo "==> Installing build dependencies..."
	@if command -v apt-get >/dev/null 2>&1; then \
		sudo apt-get update && sudo apt-get install -y build-essential pkg-config libssl-dev curl; \
	elif command -v dnf >/dev/null 2>&1; then \
		sudo dnf install -y gcc make pkg-config openssl-devel curl; \
	elif command -v pacman >/dev/null 2>&1; then \
		sudo pacman -Sy --needed --noconfirm base-devel openssl pkg-config curl; \
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
		printf 'bind = "127.0.0.1:9091"\napi_key = ""\nallow_unencrypted = true\n' \
			| sudo tee $(CONFDIR)/agent.toml > /dev/null; \
		echo "    Created $(CONFDIR)/agent.toml (localhost-only, no API key)"; \
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
	@echo ""
	@echo "✓ tenodera-agent installed and running"
	@echo "  Config:  $(CONFDIR)/agent.toml"
	@echo "  Service: systemctl status tenodera-agent"
	@echo "  Health:  curl http://127.0.0.1:9091/health"

$(BINARY):
	$(MAKE) build

# ── Uninstall ─────────────────────────────────────────────

uninstall:
	@echo "==> Stopping and removing tenodera-agent..."
	-sudo systemctl stop tenodera-agent
	-sudo systemctl disable tenodera-agent
	-sudo rm -f $(SYSTEMDDIR)/tenodera-agent.service
	-sudo systemctl daemon-reload
	-sudo rm -f $(BINDIR)/tenodera-agent
	-sudo rm /etc/tenodera/agent.toml
	@echo "✓ Removed (config in $(CONFDIR) kept)"

# ── Clean ─────────────────────────────────────────────────

clean:
	cargo clean
