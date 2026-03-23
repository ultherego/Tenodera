# Tenodera — Instalacja

Tenodera składa się z dwóch niezależnych komponentów:

- **Panel** — centralny serwer webowy + UI (instalowany na **jednym** hoście)
- **Agent** — daemon zarządzania systemem (instalowany na **każdym** zarządzanym hoście)

## Architektura połączeń

Panel łączy się z agentami na dwa sposoby:

### Tryb SSH (domyślny) — model Cockpit

Agent nasłuchuje **tylko na localhost** (`127.0.0.1:9091`). Panel otwiera tunel SSH
na zarządzany host za pomocą `sshpass`, a następnie łączy się z agentem przez ten tunel.
Hasło logowania użytkownika jest przechowywane w sesji i używane do uwierzytelniania SSH
(ten sam model co Cockpit). Wymaga `PasswordAuthentication yes` w sshd zdalnych hostów.
Nie wymaga kluczy API ani otwierania portów agenta na zewnątrz.

```
┌────────────────┐      SSH tunnel       ┌──────────────────────────────┐
│                │ ═══════════════════►   │  Host A                     │
│  Panel :9090   │   ssh -N -L ...       │  Agent 127.0.0.1:9091       │
│  (gateway+UI)  │ ═══════════════════►   │  Host B                     │
│                │      port 22          │  Agent 127.0.0.1:9091       │
└────────────────┘                       └──────────────────────────────┘
     1 serwer                              N hostów (port 9091 zamknięty)
```

### Tryb Direct (alternatywny) — agent dostępny z sieci

Agent nasłuchuje na `0.0.0.0:9091` z kluczem API i opcjonalnie TLS.
Panel łączy się bezpośrednio przez WebSocket. Wymaga otwarcia portu 9091.

```
┌────────────────┐     WebSocket (TLS)    ┌───────────────────────────┐
│  Panel :9090   │ ─────────────────────► │  Agent 0.0.0.0:9091 (API) │
└────────────────┘                        └───────────────────────────┘
```

---

## Wymagania

### Systemowe

| Komponent | Wymagania |
|-----------|-----------|
| Panel | Linux (x86_64/aarch64), systemd, PAM, `make`, `sshpass` |
| Agent | Linux (x86_64/aarch64), systemd, `make` |

### Sprzętowe (minimum)

| Komponent | CPU | RAM | Dysk |
|-----------|-----|-----|------|
| Panel | 1 vCPU | 512 MB | 200 MB |
| Agent | 1 vCPU | 128 MB | 50 MB |
| Budowanie Panelu | 2 vCPU | 4 GB | 2 GB |
| Budowanie Agenta | 2 vCPU | 2 GB | 1 GB |

> Wymogi dyskowe dotyczą zainstalowanego komponentu. Budowanie wymaga więcej miejsca tymczasowo
> (kompilator Rust + zależności), które można zwolnić po instalacji (`make clean`).

> Zależności budowania (Rust, Node.js, biblioteki systemowe) są instalowane automatycznie
> przez `make deps`. Na czystym Debianie trzeba najpierw zainstalować `make`:
> `sudo apt-get update && sudo apt-get install -y make`

---

## 1. Szybka instalacja (Makefile)

Każdy komponent ma własny `Makefile` z pełną automatyzacją: zależności → budowanie → instalacja.

### 1.1. Klonowanie repozytorium

```bash
git clone <repo-url> tenodera
cd tenodera
```

### 1.2. Instalacja Panelu (jeden serwer)

```bash
cd "Tenodera Panel"
make all
```

`make all` automatycznie:
1. Instaluje zależności systemowe (`build-essential`, `pkg-config`, `libssl-dev`, `libpam0g-dev`, `sshpass`)
2. Instaluje Rust (jeśli brak)
3. Instaluje Node.js 22 (jeśli brak)
4. Buduje backend (`tenodera-gateway`, `tenodera-bridge`) i frontend (React UI)
5. Kopiuje binarki do `/usr/local/bin/`, UI do `/usr/share/tenodera/ui/`
6. Instaluje usługę systemd z `TENODERA_BIND=0.0.0.0:9090`
7. Uruchamia `tenodera-gateway`

Panel dostępny pod: `http://<adres-serwera>:9090`

Logowanie przez PAM — użyj loginu i hasła systemowego.

Dostępne targety Makefile:

| Target | Opis |
|--------|------|
| `make all` | Pełna instalacja (deps + build + install) |
| `make deps` | Tylko zależności (system + Rust + Node.js) |
| `make build` | Tylko budowanie (backend + frontend) |
| `make install` | Tylko instalacja (binarki + UI + systemd) |
| `make uninstall` | Usunięcie (binarki + usługi; konfiguracja zostaje) |
| `make clean` | Usunięcie artefaktów budowania |

### 1.3. Instalacja Agenta (każdy zarządzany host)

> **Ważne:** Agent musi być zbudowany na systemie z tą samą (lub starszą) wersją glibc co docelowe hosty.
> Jeśli budujesz na nowszym systemie (np. Arch/Fedora) i wdrażasz na starszy (np. Debian 12),
> **zbuduj agenta bezpośrednio na docelowym hoście**.

Skopiuj katalog `Tenodera Agent` na docelowy host i uruchom:

```bash
cd "Tenodera Agent"
make all
```

`make all` automatycznie:
1. Instaluje zależności systemowe (`build-essential`, `pkg-config`, `libssl-dev`)
2. Instaluje Rust (jeśli brak)
3. Buduje `tenodera-agent` w trybie release
4. Kopiuje binarkę do `/usr/local/bin/`
5. Tworzy domyślną konfigurację `/etc/tenodera/agent.toml` (localhost, bez API key)
6. Instaluje i uruchamia usługę systemd

Weryfikacja:

```bash
curl http://127.0.0.1:9091/health
# Oczekiwany wynik: ok
```

Dostępne targety Makefile:

| Target | Opis |
|--------|------|
| `make all` | Pełna instalacja (deps + build + install) |
| `make deps` | Tylko zależności (system + Rust) |
| `make build` | Tylko budowanie |
| `make install` | Tylko instalacja (binarka + config + systemd) |
| `make uninstall` | Usunięcie (binarka + usługa; konfiguracja zostaje) |
| `make clean` | Usunięcie artefaktów budowania |

---

## 2. Instalacja ręczna (alternatywna)

Jeśli wolisz zainstalować bez Makefile.

### 2.1. Budowanie Panelu

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

Wynikowe binarki:
- `target/release/tenodera-gateway` — serwer HTTP/WebSocket
- `target/release/tenodera-bridge` — lokalny bridge (potrzebny na serwerze panelu)
- `ui/dist/` — zbudowany frontend

### 2.2. Instalacja Panelu

```bash
# Binarki
sudo install -m 755 target/release/tenodera-gateway /usr/local/bin/
sudo install -m 755 target/release/tenodera-bridge  /usr/local/bin/

# Frontend
sudo mkdir -p /usr/share/tenodera/ui
sudo cp -r ui/dist/* /usr/share/tenodera/ui/

# Katalogi konfiguracyjne
sudo mkdir -p /etc/tenodera/tls

# Usługa systemd
sudo cp systemd/tenodera-gateway.service /etc/systemd/system/

# Override — nasłuch na wszystkich interfejsach
sudo mkdir -p /etc/systemd/system/tenodera-gateway.service.d
printf '[Service]\nEnvironment=TENODERA_BIND=0.0.0.0:9090\n' \
  | sudo tee /etc/systemd/system/tenodera-gateway.service.d/bind.conf > /dev/null

sudo systemctl daemon-reload
sudo systemctl enable --now tenodera-gateway
```

### 2.3. Budowanie Agenta

```bash
cd "Tenodera Agent"
cargo build --release
```

Wynikowa binarka: `target/release/tenodera-agent`

### 2.4. Instalacja Agenta

```bash
sudo install -m 755 target/release/tenodera-agent /usr/local/bin/

# Domyślna konfiguracja (jeśli nie istnieje)
sudo mkdir -p /etc/tenodera
if [ ! -f /etc/tenodera/agent.toml ]; then
  printf 'bind = "127.0.0.1:9091"\napi_key = ""\nallow_unencrypted = true\n' \
    | sudo tee /etc/tenodera/agent.toml > /dev/null
fi

# Usługa systemd
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

## 3. Konfiguracja Agenta

### Tryb SSH (domyślny — agent na localhost)

Domyślna konfiguracja tworzona przez `make install`:

```toml
# /etc/tenodera/agent.toml
bind = "127.0.0.1:9091"
api_key = ""
allow_unencrypted = true
```

Agent nasłuchuje tylko na localhost — nie jest dostępny z sieci.
Pusty `api_key` oznacza brak autoryzacji (bezpieczne, bo dostęp tylko przez SSH).

### Tryb Direct (agent dostępny z sieci)

Edytuj `/etc/tenodera/agent.toml`:

```toml
bind = "0.0.0.0:9091"
api_key = "<KLUCZ_API>"
allow_unencrypted = false
tls_cert = "/etc/tenodera/cert.pem"
tls_key = "/etc/tenodera/key.pem"
```

Wygeneruj klucz API i certyfikat TLS:

```bash
# Klucz API
openssl rand -hex 32

# Certyfikat TLS (self-signed)
sudo openssl req -x509 -newkey rsa:4096 \
  -keyout /etc/tenodera/key.pem \
  -out /etc/tenodera/cert.pem \
  -days 365 -nodes -subj "/CN=$(hostname)"
sudo chmod 600 /etc/tenodera/key.pem

sudo systemctl restart tenodera-agent
```

> **Zapamiętaj klucz API** — ten sam musisz podać przy dodawaniu hosta w panelu.

---

## 4. Konfiguracja TLS Panelu (produkcja)

W produkcji panel powinien działać z TLS:

```bash
# Self-signed (do testów)
sudo openssl req -x509 -newkey rsa:4096 \
  -keyout /etc/tenodera/tls/key.pem \
  -out /etc/tenodera/tls/cert.pem \
  -days 365 -nodes -subj "/CN=tenodera-panel"

# Let's Encrypt (produkcja) — użyj certbot, skopiuj cert i klucz do /etc/tenodera/tls/
```

Włącz TLS w usłudze systemd:

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

Panel dostępny pod: `https://<adres-serwera>:9090`

---

## 5. Uruchomienie deweloperskie (bez instalacji)

```bash
cd "Tenodera Panel"
make build

RUST_LOG=info TENODERA_BRIDGE_BIN=target/release/tenodera-bridge \
  target/release/tenodera-gateway
```

Panel dostępny pod: `http://127.0.0.1:9090`

---

## 6. Dodanie hosta w panelu

Hosty dodaje się przez interfejs webowy panelu: **Hosts → Dodaj host**.

### Tryb SSH (domyślny)

| Pole | Wartość |
|------|---------|
| ID | unikalna nazwa (np. `web-1`) |
| Address | IP lub hostname hosta |
| Transport | SSH Tunnel |
| SSH User | użytkownik SSH (domyślnie: użytkownik sesji panelu) |
| SSH Port | `22` |
| Agent Port | `9091` |

> **Uwaga:** Hasło logowania użytkownika jest używane do uwierzytelniania SSH na zdalne hosty
> (model Cockpit — `sshpass`). Zdalny host musi mieć włączone `PasswordAuthentication yes` w sshd.
> Panel otwiera tunel SSH w imieniu zalogowanego użytkownika.

### Tryb Direct

| Pole | Wartość |
|------|---------|
| ID | unikalna nazwa |
| Address | IP lub hostname hosta |
| Transport | Direct (API key) |
| Agent Port | `9091` |
| API Key | klucz z `agent.toml` hosta |
| TLS | zaznacz jeśli agent ma certyfikat |

Hosty można też dodawać ręcznie w pliku `~/.config/tenodera/hosts.json`:

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

| Pole | Opis | Domyślnie |
|------|------|-----------|
| `id` | Unikalna nazwa hosta | (wymagane) |
| `address` | IP lub hostname | (wymagane) |
| `transport` | `"ssh"` (tunel SSH) lub `"agent"` (bezpośrednio) | `"ssh"` |
| `user` | Użytkownik SSH | (użytkownik sesji) |
| `ssh_port` | Port SSH | `22` |
| `agent_port` | Port agenta na hoście | `9091` |
| `api_key` | Klucz API (tylko tryb Direct) | `""` |
| `agent_tls` | TLS agenta (tylko tryb Direct) | `false` |

---

## 7. Firewall

### Na serwerze panelu

```bash
# Port panelu (UI + API)
sudo ufw allow 9090/tcp    # lub: firewall-cmd --permanent --add-port=9090/tcp
```

### Na hostach z agentem

**Tryb SSH** — wystarczy otwarty port SSH (22). Agent na localhost — port 9091 **nie** musi być otwarty.

**Tryb Direct** — port 9091 musi być otwarty:
```bash
sudo ufw allow 9091/tcp    # lub: firewall-cmd --permanent --add-port=9091/tcp
```

---

## 8. Masowa instalacja agenta (tryb SSH)

Skrypt do szybkiego wdrożenia na wielu hostach za pomocą Makefile:

```bash
#!/bin/bash
# deploy-agent.sh
HOSTS="192.168.1.10 192.168.1.11 192.168.1.12"
AGENT_SRC="Tenodera Agent"

for HOST in $HOSTS; do
    echo "==> $HOST"

    # Skopiuj źródła + Makefile na host
    ssh root@"$HOST" "mkdir -p /tmp/tenodera-agent-src"
    scp -r "$AGENT_SRC/src" "$AGENT_SRC/Cargo.toml" "$AGENT_SRC/Makefile" \
        root@"$HOST":/tmp/tenodera-agent-src/

    # Zainstaluj (deps + build + install) jednym poleceniem
    ssh root@"$HOST" "cd /tmp/tenodera-agent-src && make all && rm -rf /tmp/tenodera-agent-src"

    echo "    OK — dodaj host w panelu: id=$HOST, address=$HOST, transport=ssh"
done
```

---

## 9. Zmienne środowiskowe

### Panel (gateway)

| Zmienna | Opis | Domyślnie |
|---------|------|-----------|
| `TENODERA_BIND` | Adres nasłuchu | `127.0.0.1:9090` |
| `TENODERA_TLS_CERT` | Ścieżka do certyfikatu PEM | (brak) |
| `TENODERA_TLS_KEY` | Ścieżka do klucza prywatnego PEM | (brak) |
| `TENODERA_ALLOW_UNENCRYPTED` | `1`/`true` — pozwól bez TLS | `true` (dev) |
| `TENODERA_BRIDGE_BIN` | Ścieżka do binarki bridge | `tenodera-bridge` |
| `TENODERA_UI_DIR` | Katalog zbudowanego UI | `ui/dist` |
| `RUST_LOG` | Poziom logowania | `info` |

### Agent

| Zmienna | Opis | Domyślnie |
|---------|------|-----------|
| `TENODERA_AGENT_CONFIG` | Ścieżka do pliku konfiguracyjnego | `/etc/tenodera/agent.toml` |
| `TENODERA_AGENT_BIND` | Adres nasłuchu (nadpisuje config) | `127.0.0.1:9091` |
| `TENODERA_AGENT_API_KEY` | Klucz API (nadpisuje config) | `""` |
| `TENODERA_AGENT_TLS_CERT` | Certyfikat TLS (nadpisuje config) | (brak) |
| `TENODERA_AGENT_TLS_KEY` | Klucz TLS (nadpisuje config) | (brak) |
| `TENODERA_AGENT_ALLOW_UNENCRYPTED` | `1`/`true` (nadpisuje config) | `true` |
| `RUST_LOG` | Poziom logowania | `info` |

---

## 10. Diagnostyka

```bash
# Logi panelu
journalctl -u tenodera-gateway -f

# Logi agenta (na zdalnym hoście)
journalctl -u tenodera-agent -f

# Health check agenta (na hoście agenta)
curl http://127.0.0.1:9091/health

# Test tunelu SSH (z serwera panelu)
ssh -N -L 19091:127.0.0.1:9091 user@HOST &
curl http://127.0.0.1:19091/health
kill %1

# Test bezpośredni (tryb Direct, z serwera panelu)
curl -k -H "Authorization: Bearer <API_KEY>" https://HOST:9091/health
```

---

## 11. Aktualizacja

### Agent (z Makefile)

```bash
cd "Tenodera Agent"
git pull
make build
sudo make install
```

### Agent (ręcznie)

```bash
cd "Tenodera Agent"
git pull && cargo build --release
sudo cp target/release/tenodera-agent /usr/local/bin/
sudo systemctl restart tenodera-agent
```

### Panel (z Makefile)

```bash
cd "Tenodera Panel"
git pull
make build
sudo make install
```

### Panel (ręcznie)

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

## 12. Odinstalowanie

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

Konfiguracja w `/etc/tenodera/` nie jest usuwana automatycznie. Aby usunąć całkowicie:

```bash
sudo rm -rf /etc/tenodera
```
