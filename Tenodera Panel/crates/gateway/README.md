# tenodera-gateway

Serwer HTTP/WebSocket z uwierzytelnianiem PAM, zarządzaniem sesjami, obsługą TLS i orkiestracją połączeń do agentów. Punkt wejścia do systemu Tenodera.

## Rola w architekturze

Gateway to centralny serwer dostępny z przeglądarki. Obsługuje:
1. **Logowanie** — uwierzytelnianie PAM przez `unix_chkpwd`
2. **WebSocket** — upgrade po auth → mostek do bridge/agenta
3. **Serwowanie UI** — pliki statyczne React
4. **TLS** — opcjonalne szyfrowanie (rustls)
5. **Multi-host** — routing kanałów do lokalnego bridge lub zdalnych agentów (SSH tunnel + sshpass)

```
Przeglądarka → HTTPS/WSS → Gateway (:9090) → stdin/stdout → Bridge (local)
                                             → SSH tunnel (sshpass) → Agent (remote)
```

## Moduły

### `main.rs` — Serwer Axum

Definiuje routing HTTP:

| Endpoint | Metoda | Opis |
|----------|--------|------|
| `/api/auth/login` | POST | Logowanie (user/password) |
| `/api/ws` | GET | WebSocket upgrade |
| `/api/health` | GET | Health check (`{ status: "ok" }`) |
| `/*` | GET | Serwowanie plików UI (fallback na `index.html`) |

State aplikacji (współdzielony między handlerem):
- `SessionStore` — w pamięci, chroniony `Arc<RwLock<HashMap>>`
- `GatewayConfig` — konfiguracja z env vars
- `HostsConfig` — konfiguracja hostów zdalnych

**TLS:** Jeżeli jest skonfigurowany cert/key, gateway uruchamia serwer z `TlsAcceptor` (tokio-rustls). W trybie non-TLS, startuje normalny `axum::serve`.

### `auth.rs` — Endpoint logowania

```
POST /api/auth/login
Content-Type: application/json

{ "user": "...", "password": "..." }
```

Przepływ:
1. Walidacja obecności pól `user` i `password`
2. Wywołanie `pam::authenticate(user, password)`
3. Przy sukcesie: utworzenie sesji w `SessionStore`, zwrot session_id
4. Przy błędzie: HTTP 401 z komunikatem

Odpowiedź sukcesu:
```json
{ "session_id": "uuid-...", "user": "...", "message": "Authentication successful" }
```

### `ws.rs` — WebSocket handler

```
GET /api/ws?session_id=uuid-...
```

Przepływ WebSocket:
1. Walidacja `session_id` w query params
2. Odczyt sesji ze store (user, superuser_password)
3. Spawn lokalnego bridge (`BridgeProcess::spawn()`)
4. Pętla eventów (`tokio::select!`):
   - **Msg z WebSocket** → JSON parse → routing:
     - Jeśli `Open` z hostem != `localhost` → `connect_remote_bridge()`
     - Jeśli `Data`/`Close` na kanale z mapowaniem → przekieruj do remote bridge
     - Inaczej → wyślij do lokalnego bridge stdin
   - **Msg z lokalnego bridge stdout** → wyślij na WebSocket
   - **Msg z remote bridge stdout** → wyślij na WebSocket

**Multi-host routing:**
- `channel_to_bridge: HashMap<String, usize>` — mapowanie kanał → indeks bridge
- `remote_bridges: Vec<BridgeProcess>` — wektor zdalnych bridge
- Gdy klient otwiera kanał z `host != "localhost"`, gateway:
  1. Szuka definicji hosta w `hosts_config`
  2. Spawnuje zdalny bridge przez SSH
  3. Rejestruje mapowanie kanał → bridge index

### `session.rs` — Zarządzanie sesjami

- **Store:** `Arc<RwLock<HashMap<String, Session>>>`
- **Session:**
  - `id: String` (UUID v4)
  - `user: String`
  - `password: String` — hasło sesji przechowywane w pamięci (model Cockpit, używane do SSH)
  - `created_at: Instant`
- **Debug:** Custom impl ukrywający hasło (`***`)
- **Idle timeout:** 15 minut (900s) — sesje wygasają automatycznie
- **Operacje:** `create(user, password)`, `validate(session_id)` (aktualizuje `last_active`), `remove(session_id)`

### `bridge_transport.rs` — Zarządzanie połączeniami

**BridgeProcess — Spawn lokalny:**
```rust
BridgeProcess::spawn(bridge_bin) -> BridgeProcess
```
- Uruchamia `bridge_bin` jako subprocess
- Tworzy kanały `mpsc` (256 buf) do stdin/stdout
- Spawnuje 2 tokio tasks: reader (stdout→mpsc) i writer (mpsc→stdin)

**AgentConnection — Połączenie ze zdalnym agentem (SSH tunnel + sshpass):**
```rust
AgentConnection::connect_via_ssh_tunnel(ssh_user, password, address, ssh_port, agent_port)
  -> (AgentConnection, Child)
```
- Otwiera tunel SSH: `sshpass -e ssh -N -L <local_port>:127.0.0.1:<agent_port> <ssh_user>@<address>`
- Hasło przekazywane przez zmienną środowiskową `SSHPASS` (model Cockpit)
- `StrictHostKeyChecking=accept-new` — TOFU
- Po nawiązaniu tunelu łączy się z agentem przez WebSocket na `127.0.0.1:<local_port>`
- Zwraca `AgentConnection` (kanały mpsc) + `Child` (proces SSH do kill on drop)

**AgentConnection — Połączenie bezpośrednie (tryb Direct):**
```rust
AgentConnection::connect(address, port, api_key, use_tls) -> AgentConnection
```
- WebSocket do agenta z opcjonalnym API key i TLS

### `pam.rs` — Uwierzytelnianie PAM

Implementacja przez `unix_chkpwd` — setuid helper z pam_unix:

```bash
unix_chkpwd {user} nullok   # hasło na stdin (NUL-terminated)
```

Parser wyniku:
- Exit code 0 → sukces
- Exit code != 0 → błąd autentykacji
- Błąd spawnu → ogólny błąd auth

Walidacja wejścia: sprawdzenie czy username nie zawiera `\0` lub `\n` (ochrona przed injection).

### `config.rs` — Konfiguracja

Wszystkie ustawienia z zmiennych środowiskowych:

| Zmienna | Domyślna | Opis |
|---------|----------|------|
| `TENODERA_BIND_ADDR` | `127.0.0.1` | Adres nasłuchiwania |
| `TENODERA_BIND_PORT` | `9090` | Port nasłuchiwania |
| `TENODERA_BRIDGE_BIN` | `./target/debug/tenodera-bridge` | Ścieżka do binary bridge |
| `TENODERA_UI_DIR` | `./ui/dist` | Katalog z build UI |
| `TENODERA_TLS_CERT` | `""` | Ścieżka do certyfikatu PEM |
| `TENODERA_TLS_KEY` | `""` | Ścieżka do klucza prywatnego PEM |
| `TENODERA_ALLOW_UNENCRYPTED` | `false` | Pozwól na HTTP bez TLS |
| `TENODERA_IDLE_TIMEOUT` | `900` | Timeout nieaktywności sesji (sekundy) |
| `TENODERA_MAX_STARTUPS` | `20` | Maks. równoczesnych połączeń |
| `RUST_LOG` | — | Filtr logów |

### `tls.rs` — Obsługa TLS

- Budowanie `TlsAcceptor` z plików PEM (cert chain + klucz prywatny)
- Akceptor na `rustls::ServerConfig` z domyślnym crypto provider
- Połączenie z axum przez `hyper_util` + `TokioIo` wrapper
- Graceful shutdown: `tokio::signal::ctrl_c()`

### `hosts_config.rs` — Konfiguracja hostów zdalnych

- Plik: `~/.config/tenodera/hosts.json`
- Struktura:
```json
[
  {
    "id": "uuid-...",
    "name": "Debian VM",
    "address": "192.168.56.10",
    "user": "",
    "ssh_port": 22,
    "agent_port": 9091,
    "transport": "ssh",
    "api_key": "",
    "agent_tls": false,
    "added_at": "2026-03-22T10:00:00Z"
  }
]
```
- `load()` — wczytuje plik, zwraca domyślny pusty config przy braku pliku
- `find_host(id)` — wyszukuje host po ID
- `effective_user(session_user)` — jeśli `user` pusty, używa `session_user`
- `Transport`: `Ssh` (tunel sshpass) lub `Agent` (bezpośredni WebSocket)

## Przepływ uwierzytelniania (end-to-end)

```
1. POST /api/auth/login { user, password }
2. pam::authenticate() → unix_chkpwd user nullok
3. SessionStore::create(user, password)
4. → 200 { session_id, user }
5. GET /api/ws?session_id=uuid-...
6. SessionStore::validate(id) — sprawdź istnienie + timeout
7. BridgeProcess::spawn(bridge_bin) — lokalny bridge
8. ↔ WebSocket ↔ Bridge stdin/stdout ↔ System
9. Dla remote hostów: AgentConnection::connect_via_ssh_tunnel(ssh_user, password, ...)
```

## Zależności

- `axum 0.8` — framework HTTP + WebSocket
- `tokio` — async runtime
- `tokio-rustls 0.26` + `rustls 0.23` — TLS
- `tower-http 0.6` — ServeDir, CORS
- `hyper-util` — listener TCP dla TLS
- `serde` + `serde_json` — JSON
- `uuid` — generowanie session ID
- `tracing` — logowanie
- `tenodera-protocol` — współdzielone typy

## Bezpieczeństwo

- Sesje w pamięci (nie na dysku)
- Timeout sesji 15 min
- Bridge per użytkownik (izolacja uprawnień)
- SSH z `StrictHostKeyChecking=accept-new` (TOFU)
- Hasło sesji używane do SSH przez `sshpass` (model Cockpit)
- TLS opcjonalne ale rekomendowane
- CORS: `AllowOrigin::any()` (do zastąpienia w produkcji)
