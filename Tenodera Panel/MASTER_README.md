# Tenodera — Kompletna dokumentacja projektu

> Rustowy zamiennik Cockpit — webowy panel administracji serwerów Linux z obsługą wielu hostów.

---

## Spis treści

1. [Wprowadzenie](#wprowadzenie)
2. [Architektura systemu](#architektura-systemu)
3. [Protokół komunikacji](#protokół-komunikacji)
4. [Komponenty](#komponenty)
   - [tenodera-protocol](#tenodera-protocol)
   - [tenodera-gateway](#tenodera-gateway)
   - [tenodera-bridge](#tenodera-bridge)
   - [tenodera-priv-bridge](#tenodera-priv-bridge)
   - [tenodera-ui](#tenodera-ui)
   - [Pliki systemd](#pliki-systemd)
5. [Przepływ danych](#przepływ-danych)
6. [Zarządzanie wieloma hostami](#zarządzanie-wieloma-hostami)
7. [Konfiguracja](#konfiguracja)
8. [Budowanie i uruchomienie](#budowanie-i-uruchomienie)
9. [Bezpieczeństwo](#bezpieczeństwo)
10. [Narzędzia deweloperskie](#narzędzia-deweloperskie)

---

## Wprowadzenie

Tenodera to reimplementacja Cockpit w Rust z frontendem React. Projekt realizuje architekturę wieloprocesową: centralny gateway obsługuje uwierzytelnianie i WebSocket, a dla każdej sesji spawnuje izolowany proces bridge działający z uprawnieniami zalogowanego użytkownika. Obsługuje zarządzanie wieloma zdalnymi hostami przez SSH.

### Główne cechy

- **Metryki w czasie rzeczywistym** — CPU (per-core), RAM, swap, load, disk I/O, network I/O
- **Zarządzanie usługami systemd** — start/stop/restart/enable/disable/reload
- **Kontenery Docker/Podman** — lista, tworzenie, logi, zarządzanie obrazami
- **Sieć** — interfejsy, firewall (ufw/firewalld/nftables/iptables), mosty, VLAN, VPN
- **Pakiety** — pacman/apt/dnf z auto-detekcją, repozytoria
- **Terminal** — pełny emulator PTY (xterm.js)
- **Przeglądarka plików** — nawigacja po systemie plików z sudo fallback
- **Logi systemowe** — journald z filtrami
- **Multi-host** — zarządzanie wieloma serwerami z jednego interfejsu
- **TLS** — opcjonalne szyfrowanie przez rustls
- **Hardening systemd** — sandboxing usług

### Stack technologiczny

| Warstwa | Technologia |
|---------|-------------|
| Backend runtime | Rust 1.94, edition 2024, tokio async |
| HTTP/WS | axum 0.8 |
| TLS | rustls 0.23 + tokio-rustls 0.26 |
| System | nix 0.29, libc (PTY, fork, ioctl, statvfs) |
| D-Bus | zbus 5 |
| Frontend | React 19, TypeScript 5.7, Vite 6 |
| Wykresy | Recharts 3.8 |
| Terminal | @xterm/xterm 5.5 |
| Routing | react-router-dom 7 |
| State | @tanstack/react-query 5 |

---

## Architektura systemu

```
┌─────────────────────────────────────────────────────────────────┐
│                          Przeglądarka                           │
│  React SPA (tenodera-ui)                                        │
│  ├── WebSocket transport (kanały multiplexowane)                │
│  ├── 12 stron (Dashboard, Services, Terminal, ...)              │
│  └── HostTransportContext (routing local/remote)                │
└──────────────────────────┬──────────────────────────────────────┘
                           │ WSS/WS
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│  tenodera-gateway                    :9090                        │
│  ├── POST /api/auth/login  → PAM (unix_chkpwd) → SessionStore    │
│  ├── GET  /api/ws          → WebSocket handler                   │
│  ├── GET  /api/health      → health check                       │
│  └── GET  /*               → ServeDir (UI pliki)                │
│                                                                  │
│  Per sesja:                                                      │
│  ├── BridgeProcess::spawn()        → lokalny bridge              │
│  └── AgentConnection::connect_via_ssh_tunnel() → SSH → agent      │
└──────┬───────────────────────────────────────┬──────────────────┘
       │ stdin/stdout                          │ SSH
       ▼                                       ▼
┌─────────────────────┐              ┌─────────────────────┐
│  tenodera-bridge     │              │  tenodera-bridge     │
│  (user: alice)      │              │  (remote host)      │
│  ├── Router         │              │  ├── Router         │
│  ├── 18 handlerów   │              │  ├── 18 handlerów   │
│  └── PTY, systemctl │              │  └── PTY, systemctl │
└─────────────────────┘              └─────────────────────┘

┌─────────────────────┐
│  tenodera-priv-bridge│
│  (root, allowlist)  │
│  ├── systemd.unit   │
│  └── package.updates│
└─────────────────────┘
```

### Workspace Cargo

```toml
[workspace]
members = [
    "crates/protocol",    # Biblioteka: współdzielone typy protokołu
    "crates/gateway",     # Binary: tenodera-gateway
    "crates/bridge",      # Binary + lib: tenodera-bridge
    "crates/priv-bridge", # Binary: tenodera-priv-bridge
]
```

---

## Protokół komunikacji

System używa protokołu kanałowego opartego na JSON przesyłanym przez WebSocket (przeglądarka ↔ gateway) i stdin/stdout (gateway ↔ bridge).

### Typy wiadomości

| Typ | Kierunek | Opis |
|-----|----------|------|
| `Open` | klient → serwer | Otwieranie kanału (payload, opcje) |
| `Ready` | serwer → klient | Potwierdzenie otwarcia |
| `Data` | ↔ dwukierunkowy | Dane kanału (JSON) |
| `Control` | ↔ dwukierunkowy | Komendy sterujące |
| `Close` | ↔ dwukierunkowy | Zamknięcie kanału (opcjonalny problem) |
| `Ping` | klient → serwer | Keepalive |
| `Pong` | serwer → klient | Odpowiedź keepalive |

### Payload types (18 wbudowanych + Custom)

| Payload | Typ handlera | Opis |
|---------|-------------|------|
| `system.info` | One-shot | Informacje o systemie |
| `hardware.info` | One-shot | CPU, kernel, temperatury |
| `top.processes` | One-shot | Top 15 procesów |
| `disk.usage` | One-shot | Użycie partycji |
| `network.stats` | One-shot | Interfejsy sieciowe |
| `journal.query` | One-shot | Logi journald |
| `file.list` | One-shot | Listing katalogu |
| `superuser.verify` | One-shot | Weryfikacja hasła sudo |
| `systemd.units` | One-shot | Lista jednostek systemd |
| `metrics.stream` | Streaming | Metryki CPU/RAM/IO w czasie rzeczywistym |
| `storage.stream` | Streaming | Disk I/O + urządzenia blokowe |
| `networking.stream` | Streaming | TX/RX rates per interfejs |
| `terminal.pty` | Streaming+Bidi | Interaktywny terminal PTY |
| `systemd.manage` | Bidirectional | Zarządzanie usługami systemd |
| `container.manage` | Bidirectional | Docker/Podman |
| `networking.manage` | Bidirectional | Firewall, mosty, VLAN, VPN |
| `packages.manage` | Bidirectional | Pakiety systemowe |
| `hosts.manage` | Bidirectional | CRUD hostów zdalnych |

### Tryby handlerów

- **One-shot:** Open → Ready + Data + Close (jednokrotne pobranie danych)
- **Streaming:** Open → Ready → [Data, Data, ...] (ciągły strumień, Close z klienta zatrzymuje)
- **Bidirectional:** Open → Ready → [Data↔Data] (klient wysyła komendy, serwer odpowiada)

### Przykład wymiany wiadomości

```json
// Klient → Open kanał
{"type":"open","channel":"ch1","payload":"system.info"}

// Serwer → Ready
{"type":"ready","channel":"ch1"}

// Serwer → Data
{"type":"data","channel":"ch1","data":{"hostname":"srv1","os":"Arch Linux","uptime":86400}}

// Serwer → Close
{"type":"close","channel":"ch1"}
```

---

## Komponenty

### tenodera-protocol

**Lokalizacja:** `crates/protocol/` | **Typ:** Biblioteka Rust | **Szczegóły:** [crates/protocol/README.md](crates/protocol/README.md)

Współdzielona biblioteka definiująca typy protokołu używane przez wszystkie inne crate'y:

- **`message.rs`** — Enum `Message` z 8 wariantami (Open/Ready/Data/Control/Close/Auth/AuthResult/Ping/Pong) + `AuthCredentials`
- **`channel.rs`** — `ChannelId`, `ChannelState`, `ChannelOpenOptions` (payload, superuser, extra), `SuperuserMode`
- **`payload.rs`** — Enum `Payload` z 17 wariantami + Custom, konwersje string↔enum, Display
- **`error.rs`** — `ProtocolError` z 8 wariantami przez thiserror

---

### tenodera-gateway

**Lokalizacja:** `crates/gateway/` | **Binary:** `tenodera-gateway` | **Port:** 9090 | **Szczegóły:** [crates/gateway/README.md](crates/gateway/README.md)

Centralny serwer HTTP/WebSocket:

| Moduł | Opis |
|-------|------|
| `main.rs` | Serwer Axum z routingiem HTTP |
| `auth.rs` | Endpoint logowania (POST /api/auth/login) |
| `ws.rs` | WebSocket handler z routingiem multi-host |
| `session.rs` | In-memory SessionStore (UUID, konfigurowalny timeout 900s) |
| `bridge_transport.rs` | Spawn bridge: lokalny (subprocess) lub zdalny (SSH) |
| `pam.rs` | Uwierzytelnianie PAM przez `unix_chkpwd` |
| `config.rs` | Konfiguracja z env vars |
| `tls.rs` | TLS przez rustls (opcjonalne) |
| `hosts_config.rs` | Odczyt ~/.config/tenodera/hosts.json, `effective_user()` (pusty user = sesja) |

**Endpointy:**

| Endpoint | Metoda | Opis |
|----------|--------|------|
| `/api/auth/login` | POST | Logowanie |
| `/api/ws` | GET | WebSocket upgrade |
| `/api/health` | GET | Health check |
| `/*` | GET | Pliki UI (fallback index.html) |

---

### tenodera-bridge

**Lokalizacja:** `crates/bridge/` | **Binary:** `tenodera-bridge` | **Szczegóły:** [crates/bridge/README.md](crates/bridge/README.md)

Per-sesyjny router wiadomości z 18 handlerami (z 16 modułów):

| Moduł | Opis |
|-------|------|
| `main.rs` | Pętla async: stdin → Router → stdout (JSON lines) |
| `handler.rs` | Trait `ChannelHandler` (payload_type, is_streaming, open, stream, data) |
| `router.rs` | Dispatch wiadomości po payload type, zarządzanie kanałami, rejestracja 18 handlerów |
| `handlers/` | 16 modułów eksportujących 18 handlerów (systemd_units i networking eksportują po 2) |

**Handlery:**

| Handler | Payload | Typ | Moduł |
|---------|---------|-----|-------|
| SystemInfoHandler | `system.info` | One-shot | system_info |
| HardwareInfoHandler | `hardware.info` | One-shot | hardware_info |
| TopProcessesHandler | `top.processes` | One-shot | top_processes |
| DiskUsageHandler | `disk.usage` | One-shot | disk_usage |
| NetworkStatsHandler | `network.stats` | One-shot | network_stats |
| JournalQueryHandler | `journal.query` | One-shot | journal_query |
| FileListHandler | `file.list` | One-shot | file_list |
| SuperuserVerifyHandler | `superuser.verify` | One-shot | superuser_verify |
| SystemdUnitsHandler | `systemd.units` | One-shot | systemd_units |
| MetricsStreamHandler | `metrics.stream` | Streaming | metrics_stream |
| StorageStreamHandler | `storage.stream` | Streaming | storage |
| NetworkStreamHandler | `networking.stream` | Streaming | networking |
| TerminalPtyHandler | `terminal.pty` | Streaming+Bidi | terminal_pty |
| SystemdManageHandler | `systemd.manage` | Bidirectional | systemd_units |
| ContainersHandler | `container.manage` | Bidirectional | containers |
| NetworkManageHandler | `networking.manage` | Bidirectional | networking |
| PackagesHandler | `packages.manage` | Bidirectional | packages |
| HostsManageHandler | `hosts.manage` | Bidirectional | hosts |


**Źródła danych:**
- `/proc/stat`, `/proc/meminfo`, `/proc/loadavg`, `/proc/diskstats`, `/proc/net/dev` — metryki
- `/proc/cpuinfo`, `/proc/uptime`, `/proc/mounts` — info systemowe
- `/etc/os-release`, `/etc/passwd` — konfiguracja systemu
- `/sys/class/hwmon/`, `/sys/class/net/` — hardware
- `systemctl`, `journalctl`, `lsblk`, `ip`, `ps` — narzędzia systemowe
- `ufw`, `firewall-cmd`, `nft`, `iptables` — firewall
- `podman`, `docker` — kontenery
- `pacman`, `apt`, `dnf` — pakiety
- `nmcli` — VPN
- `openpty()`, `fork()` — terminal PTY

---

### tenodera-priv-bridge

**Lokalizacja:** `crates/priv-bridge/` | **Binary:** `tenodera-priv-bridge` | **Szczegóły:** [crates/priv-bridge/README.md](crates/priv-bridge/README.md)

Uprzywilejowany helper (root) z allowlistem:

- **Dozwolone operacje:** `systemd.unit.action`, `package.updates`
- **Wszystkie inne:** odrzucane z `not-authorized`
- **Status:** stub — walidacja działa, handlery do implementacji
- Synchroniczna pętla stdin/stdout (nie async)

---

### tenodera-ui

**Lokalizacja:** `ui/` | **Dev port:** 3000 | **Szczegóły:** [ui/README.md](ui/README.md)

Frontendowa aplikacja React SPA:

| Strona | Payload types | Opis |
|--------|--------------|------|
| Login | — | Formularz logowania PAM |
| Shell | `hosts.manage`, `system.info` | Kontener: sidebar, top bar, routing, host selector, superuser |
| Dashboard | `system.info`, `metrics.stream`, `hardware.info`, `disk.usage`, `network.stats`, `top.processes` | Wykresy CPU/RAM/IO real-time, procesy, hardware |
| Services | `systemd.units`, `systemd.manage` | Zarządzanie usługami systemd |
| Containers | `container.manage` | Docker/Podman GUI |
| Storage | `storage.stream` | Dyski I/O i urządzenia blokowe |
| Networking | `networking.stream`, `networking.manage` | Sieć, firewall (multi-backend), VPN |
| Packages | `packages.manage` | Pakiety systemowe (pacman/apt/dnf) |
| Logs | `journal.query` | Logi journald z filtrami |
| Terminal | `terminal.pty` | Emulator terminala (xterm.js) |
| Files | `file.list` | Przeglądarka plików z sudo fallback |
| Hosts | `hosts.manage` | CRUD hostów zdalnych |

**Warstwa transportowa:**
- `transport.ts` — singleton WebSocket z multiplexingiem kanałów (`connect()`, `openChannel()`, `request()`)
- `auth.ts` — klient logowania (`login(user, password)` → POST /api/auth/login)
- `HostTransportContext.tsx` — React Context routing local/remote: hook `useTransport()` wrappuje `openChannel()` i `request()` dodając `{host: hostId}` gdy activeHost jest ustawiony

---

### Pliki systemd

**Lokalizacja:** `systemd/` | **Szczegóły:** [systemd/README.md](systemd/README.md)

| Usługa | Opis |
|--------|------|
| `tenodera-gateway.service` | Główna usługa HTTP/WS (z security hardening) |
| `tenodera-priv-bridge.service` | Helper root (socket activation) |

Obie usługi mają hardening: `ProtectSystem=full`, `PrivateTmp`, `ProtectKernelTunables`, `ProtectControlGroups`, `LockPersonality`.

---

## Przepływ danych

### Logowanie i nawiązanie sesji

```
1. Użytkownik → POST /api/auth/login { user, password }
2. Gateway → pam::authenticate() → unix_chkpwd (PAM helper)
3. Gateway → SessionStore::create(user, password) → UUID
4. Gateway → 200 { session_id, user }
5. Przeglądarka → sessionStorage.setItem('session_id', ...)
6. Przeglądarka → GET /api/ws?session_id=uuid
7. Gateway → validate_session() → spawn BridgeProcess
8. ↔ WebSocket ↔ Bridge (JSON lines stdin/stdout)
```

### One-shot request (np. system info)

```
Klient: {"type":"open","channel":"1","payload":"system.info"}
  → Gateway → Bridge stdin
  → Router → SystemInfoHandler::open()
  → Bridge stdout → Gateway → WebSocket
Klient: {"type":"ready","channel":"1"}
         {"type":"data","channel":"1","data":{...}}
         {"type":"close","channel":"1"}
```

### Streaming (np. metryki)

```
Klient: {"type":"open","channel":"2","payload":"metrics.stream","interval":1000}
  → Router → spawn tokio task → MetricsStreamHandler::stream()
  → co 1s: {"type":"data","channel":"2","data":{cpu:...,memory:...}}
  → co 1s: {"type":"data","channel":"2","data":{cpu:...,memory:...}}
  → ...
Klient: {"type":"close","channel":"2"}  ← zatrzymuje stream
```

### Bidirectional (np. systemd manage)

```
Klient: {"type":"open","channel":"3","payload":"systemd.manage"}
Serwer: {"type":"ready","channel":"3"}

Klient: {"type":"data","channel":"3","data":{"action":"list"}}
Serwer: {"type":"data","channel":"3","data":[{unit:"nginx.service",...}]}

Klient: {"type":"data","channel":"3","data":{"action":"restart","unit":"nginx.service"}}
Serwer: {"type":"data","channel":"3","data":{"ok":true}}

Klient: {"type":"close","channel":"3"}
```

---

## Zarządzanie wieloma hostami

### Architektura multi-host

Tenodera obsługuje zarządzanie wieloma serwerami z jednego interfejsu:

1. **Rejestracja hosta** — przez stronę Hosts (payload `hosts.manage`)
2. **Persistence** — `~/.config/tenodera/hosts.json`
3. **Połączenie** — gateway spawnuje remote bridge przez SSH
4. **Transparent routing** — frontend dodaje `host: hostId` do Open message

### Plik konfiguracyjny hostów

```json
[
  {
    "id": "9ca1fc19-...",
    "name": "Debian VM",
    "address": "192.168.56.10",
    "user": "",
    "ssh_port": 22,
    "added_at": "2026-03-22T10:00:00Z"
  }
]
```

### Przepływ remote

```
1. Frontend: openChannel("system.info", { host: "uuid-..." })
2. Gateway: wykrywa pole "host" w Open message
3. Gateway: find_host(id) → address, effective_user(session_user), ssh_port
4. Gateway: AgentConnection::connect_via_ssh_tunnel()
   → sshpass -e ssh -N -o StrictHostKeyChecking=accept-new
          -p 22 -L <local_port>:127.0.0.1:9091 <user>@<host>
   → WebSocket do agenta przez tunel (127.0.0.1:<local_port>)
5. Gateway: rejestruje channel → remote agent mapping
6. Agent na zdalnym hoście: identyczny protokół
7. Odpowiedzi: remote agent WS → gateway → WebSocket → frontend
```

**Pole `user` w konfiguracji hosta:**
- Puste (`""`) — SSH loguje się jako zalogowany użytkownik sesji (model FreeIPA/enterprise)
- Wypełnione — SSH loguje się jako wskazany użytkownik (override per host)

### Frontend: HostTransportContext

```tsx
// Shell.tsx owija trasy w HostTransportProvider z aktywnym hostem
<HostTransportProvider value={activeHost?.id ?? null}>
  <Routes>
    <Route path="/" element={<Dashboard />} />
    <Route path="/services" element={<Services />} />
    {/* ... te same komponenty dla local i remote */}
  </Routes>
</HostTransportProvider>

// Wewnątrz Dashboard (lub dowolnej strony):
const { request, openChannel } = useTransport();
const data = await request('system.info');
// → jeśli activeHost jest ustawiony, automatycznie dodaje { host: "uuid-..." }
// → jeśli null (host lokalny), wysyła bez pola host
```

---

## Konfiguracja

### Zmienne środowiskowe gateway

| Zmienna | Domyślna | Opis |
|---------|----------|------|
| `TENODERA_BIND_ADDR` | `127.0.0.1` | Adres nasłuchiwania |
| `TENODERA_BIND_PORT` | `9090` | Port nasłuchiwania |
| `TENODERA_BRIDGE_BIN` | `./target/debug/tenodera-bridge` | Ścieżka do bridge binary |
| `TENODERA_UI_DIR` | `./ui/dist` | Katalog z UI |
| `TENODERA_TLS_CERT` | `""` | Certyfikat TLS (PEM) |
| `TENODERA_TLS_KEY` | `""` | Klucz prywatny TLS (PEM) |
| `TENODERA_ALLOW_UNENCRYPTED` | `true` | Pozwól na HTTP bez TLS |
| `TENODERA_IDLE_TIMEOUT` | `900` | Timeout sesji (sekundy) |
| `TENODERA_MAX_STARTUPS` | `20` | Maks. równoczesnych procesów bridge |
| `RUST_LOG` | — | Filtr logów |

---

## Budowanie i uruchomienie

### Wymagania

- Rust 1.94+ (stable)
- Node.js 18+ i npm
- Linux (wymagane /proc, systemd, PTY)

### Budowanie

```bash
# Backend — wszystkie 4 binary
cargo build

# Frontend
cd ui && npm install && npm run build && cd ..
```

### Binaria wynikowe

| Binary | Lokalizacja | Opis |
|--------|------------|------|
| `tenodera-gateway` | `target/debug/tenodera-gateway` | Serwer główny |
| `tenodera-bridge` | `target/debug/tenodera-bridge` | Router per-sesja |
| `tenodera-priv-bridge` | `target/debug/tenodera-priv-bridge` | Helper root |

### Uruchomienie deweloperskie

```bash
# Terminal 1 — gateway
RUST_LOG=info cargo run --bin tenodera-gateway

# Terminal 2 — frontend dev server (z proxy na :9090)
cd ui && npm run dev

# Otwórz przeglądarkę na http://localhost:3000
```

### Uruchomienie produkcyjne

```bash
cargo build --release

# Instalacja
sudo cp target/release/tenodera-gateway /usr/local/bin/
sudo cp target/release/tenodera-bridge /usr/local/bin/
sudo mkdir -p /usr/share/tenodera/ui
sudo cp -r ui/dist/* /usr/share/tenodera/ui/

# Systemd
sudo cp systemd/tenodera-gateway.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now tenodera-gateway
```

### VM testowe (Vagrant)

```bash
vagrant up        # 2x Debian bookworm: 192.168.56.10 (Panel), 192.168.56.11 (Agent)
vagrant ssh tenodera-remote-1   # Wejście do VM z panelem
vagrant ssh tenodera-remote-2   # Wejście do VM z agentem
```

---

## Bezpieczeństwo

### Izolacja procesów

- Każda sesja = osobny proces bridge z uprawnieniami zalogowanego użytkownika
- Bridge nie ma dostępu do sesji innych użytkowników
- Gateway nie wykonuje bezpośrednio operacji systemowych

### Uwierzytelnianie

- PAM przez `unix_chkpwd` (setuid helper z pam_unix)
- Sesje in-memory z UUID v4
- 15-minutowy timeout nieaktywności (konfigurowalny `TENODERA_IDLE_TIMEOUT`, domyślnie 900s)
- Hasło sesji przechowywane w pamięci gateway (model Cockpit) — używane do SSH na zdalne hosty

### Eskalacja uprawnień

- Bridge weryfikuje hasło przez `unix_chkpwd` (handler `superuser.verify`)
- Operacje uprzywilejowane (systemctl) wykonywane bezpośrednio (bridge działa jako root)
- Priv-bridge działa jako root z restrykcyjną allowlistą

### Hardening systemd

- `ProtectSystem=full` — read-only /usr, /boot, /efi
- `PrivateTmp` — izolowany /tmp
- `ProtectKernelTunables` — blokada /proc/sys zapisu
- `ProtectControlGroups` — blokada zapisu do /sys/fs/cgroup
- `LockPersonality` — blokada zmiany domeny egzekucji

### Walidacja wejścia

- File list: `canonicalize()` jako ochrona przed path traversal
- Priv-bridge: allowlist payload types
- Gateway: walidacja session_id przed WebSocket upgrade

### SSH (remote hosts) — model Cockpit

- Hasło logowania użytkownika jest przechowywane w sesji gateway
- Tunele SSH otwierane przez `sshpass -e ssh` z hasłem sesji (zmienna `SSHPASS`)
- `StrictHostKeyChecking=accept-new` — TOFU (Trust On First Use)
- Wymagane `PasswordAuthentication yes` w sshd zdalnych hostów
- Pole `user` w konfiguracji hosta: puste = użytkownik sesji (model enterprise/FreeIPA), wypełnione = override
- Zależność systemowa: pakiet `sshpass`

---

## Narzędzia deweloperskie

### Diagnostyka

```bash
# Test bridge przez SSH
python3 test_ssh_bridge.py
node test_ssh_bridge.js
```

### Struktura projektu

```
Tenodera/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── protocol/           # Biblioteka protokołu
│   │   └── src/            # message, channel, payload, error
│   ├── gateway/            # Serwer HTTP/WS
│   │   └── src/            # auth, ws, session, bridge_transport, pam, config, tls, hosts_config
│   ├── bridge/             # Router + 18 handlerów
│   │   └── src/
│   │       ├── handlers/   # 16 modułów: system_info, hardware_info, top_processes,
│   │       │               #   metrics_stream, systemd_units, journal_query,
│   │       │               #   terminal_pty, file_list, disk_usage, storage,
│   │       │               #   network_stats, networking, containers, packages,
│   │       │               #   superuser_verify, hosts
│   │       ├── handler.rs  # Trait ChannelHandler
│   │       └── router.rs   # Router dispatch (18 handlerów z 16 modułów)
│   └── priv-bridge/        # Helper root (allowlist)
│       └── src/main.rs
├── systemd/                # Pliki usług systemd
├── ui/                     # Frontend React/TypeScript
│   ├── src/
│   │   ├── api/            # transport, auth, HostTransportContext
│   │   └── pages/          # 12 stron (Login, Shell, Dashboard, Services, Containers,
│   │                       #   Storage, Networking, Packages, Logs, Terminal, Files, Hosts)
│   ├── package.json
│   └── vite.config.ts
├── Vagrantfile             # VM testowe (Debian bookworm: 192.168.56.10, 192.168.56.11)
├── README.md               # README (angielski)
├── GENERAL_README.md       # Analiza architektury Cockpit (referencja)
├── MASTER_README.md        # Ten plik — kompletna dokumentacja projektu
└── README-HOSTS.md         # Analiza zarządzania hostami
```
