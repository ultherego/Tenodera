# Pliki systemd

Usługi systemd do uruchamiania komponentów Tenodera jako demonów systemu.

## tenodera-gateway.service

Główna usługa — serwer HTTP/WebSocket na porcie 9090. Spawnuje `tenodera-bridge` per sesja.

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

# Zmienne środowiskowe
Environment=RUST_LOG=tenodera_gateway=info
Environment=TENODERA_BRIDGE_BIN=/usr/local/bin/tenodera-bridge
Environment=TENODERA_UI_DIR=/usr/share/tenodera/ui
# TLS — uncomment and set paths for production:
# Environment=TENODERA_TLS_CERT=/etc/tenodera/tls/cert.pem
# Environment=TENODERA_TLS_KEY=/etc/tenodera/tls/key.pem
# Environment=TENODERA_ALLOW_UNENCRYPTED=0

# Hardening bezpieczeństwa
ProtectSystem=full
PrivateTmp=yes
ProtectKernelTunables=yes
ProtectControlGroups=yes
LockPersonality=yes

[Install]
WantedBy=multi-user.target
```

### Hardening — wyjaśnienie dyrektyw

| Dyrektywa | Opis |
|-----------|------|
| `ProtectSystem=full` | /usr, /boot, /efi read-only (ale /etc zapisywalny — potrzebny dla hosts.json) |
| `PrivateTmp=yes` | Izolowany /tmp |
| `ProtectKernelTunables=yes` | Blokada /proc/sys, /sys zapisu |
| `ProtectControlGroups=yes` | Blokada zapisu do /sys/fs/cgroup |
| `LockPersonality=yes` | Blokada zmiany domeny egzekucji |

## tenodera-priv-bridge.service

Uprzywilejowany helper działający jako root z socket activation:

```ini
[Unit]
Description=Tenodera Privileged Bridge (Rust)

[Service]
Type=simple
ExecStart=/usr/local/bin/tenodera-priv-bridge
StandardInput=socket
User=root

# Hardening (luźniejszy — wymaga root)
NoNewPrivileges=false          # Root musi eskalować
ProtectSystem=strict
ProtectHome=read-only
PrivateTmp=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectControlGroups=yes
RestrictSUIDSGID=yes
```

## Instalacja

```bash
# Kopiowanie binariów
sudo cp target/release/tenodera-gateway /usr/local/bin/
sudo cp target/release/tenodera-bridge /usr/local/bin/
sudo cp target/release/tenodera-priv-bridge /usr/local/bin/

# Kopiowanie plików usług
sudo cp systemd/*.service /etc/systemd/system/

# Instalacja UI
sudo mkdir -p /usr/share/tenodera/ui
sudo cp -r ui/dist/* /usr/share/tenodera/ui/

# Uruchomienie
sudo systemctl daemon-reload
sudo systemctl enable --now tenodera-gateway
```

## Konfiguracja

Wszystkie ustawienia przez zmienne środowiskowe w pliku usługi:

| Zmienna | Domyślna | Opis |
|---------|----------|------|
| `TENODERA_BIND_ADDR` | `127.0.0.1` | Adres nasłuchiwania |
| `TENODERA_BIND_PORT` | `9090` | Port nasłuchiwania |
| `TENODERA_BRIDGE_BIN` | `tenodera-bridge` | Ścieżka do binary bridge |
| `TENODERA_UI_DIR` | `ui/dist` | Ścieżka do zbudowanego frontendu |
| `TENODERA_TLS_CERT` | (brak) | Ścieżka do certyfikatu PEM |
| `TENODERA_TLS_KEY` | (brak) | Ścieżka do klucza prywatnego PEM |
| `TENODERA_ALLOW_UNENCRYPTED` | `true` | Pozwól na HTTP bez TLS |
| `TENODERA_IDLE_TIMEOUT` | `900` | Timeout sesji (sekundy) |
| `TENODERA_MAX_STARTUPS` | `20` | Maks. równoczesnych połączeń |
| `RUST_LOG` | (brak) | Filtr logów (np. `info`, `tenodera_gateway=debug`) |

### Nadpisywanie zmiennych

```bash
sudo systemctl edit tenodera-gateway
# Dodaj:
# [Service]
# Environment=TENODERA_TLS_CERT=/etc/tenodera/cert.pem
# Environment=TENODERA_TLS_KEY=/etc/tenodera/key.pem
# Environment=TENODERA_ALLOW_UNENCRYPTED=false
```

## Logi

```bash
journalctl -u tenodera-gateway -f     # live logi gateway
journalctl -u tenodera-priv-bridge -f # live logi priv-bridge
```
