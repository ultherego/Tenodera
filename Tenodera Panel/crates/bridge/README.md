# tenodera-bridge

Per-sesyjny router wiadomości kanałowych z pluginowymi handlerami systemowymi.

## Rola w architekturze

`tenodera-bridge` to główny silnik backendu Tenodera. Dla każdej zalogowanej sesji użytkownika, gateway spawnuje *osobny* proces `tenodera-bridge` działający z uprawnieniami tego użytkownika. Bridge komunikuje się z gateway przez stdin/stdout (JSON lines), a gateway mostkuje je do/z WebSocketa w przeglądarce.

```
Przeglądarka ←→ WebSocket ←→ Gateway ←→ stdin/stdout ←→ Bridge (per user)
```

## Architektura wewnętrzna

### `main.rs` — Pętla główna

1. Inicjalizacja loggera (`tracing` → stderr)
2. Utworzenie kanału `mpsc` (256 elementów) na wiadomości wychodzące
3. Rejestracja domyślnych handlerów w `Router`
4. Spawn zadania pisarza stdout — odbiera wiadomości z kanału i serializuje je jako JSON lines
5. Pętla główna — czyta stdin linia po linii, deserializuje `Message`, przekazuje do `Router::handle()`, wysyła odpowiedzi

### `handler.rs` — Trait `ChannelHandler`

Definiuje interfejs dla wszystkich handlerów:

```rust
pub trait ChannelHandler: Send + Sync {
    fn payload_type(&self) -> &str;        // np. "system.info"
    fn is_streaming(&self) -> bool;         // domyślnie false
    async fn open(&self, channel: &str, options: &ChannelOpenOptions) -> Vec<Message>;
    async fn stream(&self, channel: &str, options: &ChannelOpenOptions,
                    tx: mpsc::Sender<Message>, shutdown: watch::Receiver<bool>);
    async fn data(&self, channel: &str, data: &Value) -> Vec<Message>;
}
```

**Typy handlerów:**
- **One-shot** — `open()` zwraca Ready + Data + Close od razu
- **Streaming** — `is_streaming()=true`, `stream()` wysyła dane przez `tx` do momentu `shutdown`
- **Bidirectional** — `open()` zwraca Ready (bez Close), a po otwarciu przyjmuje `data()` z komendami

### `router.rs` — Router wiadomości

`Router` zarządza:
- `handlers: HashMap<String, Arc<dyn ChannelHandler>>` — rejestr handlerów po payload type
- `active_channels: HashMap<String, ActiveChannel>` — aktywne kanały streamingowe (z `shutdown_tx`)
- `channel_handlers: HashMap<String, Arc<dyn ChannelHandler>>` — mapowanie kanału → handler (one-shot/bidirectional)
- `out_tx: mpsc::Sender<Message>` — kanał wyjściowy do stdout

Logika routingu w `handle()`:
- **Open** → szuka handlera po `options.payload`
  - Streaming: spawnuje tokio task, wysyła Ready, uruchamia `stream()`
  - One-shot/Bidirectional: woła `open()`, zwraca odpowiedzi
- **Data** → szuka handlera po channel ID (najpierw active_channels, potem channel_handlers), woła `data()`
- **Close** → zamyka kanał streamingowy (wysyła shutdown), usuwa tracking
- **Ping** → zwraca Pong

## Handlery (17 zarejestrowanych)

### One-shot handlery

| Handler | Payload type | Źródło danych | Opis |
|---------|-------------|---------------|------|
| `SystemInfoHandler` | `system.info` | `/proc/uptime`, `/etc/os-release`, `gethostname()` | Hostname, OS, uptime, czas startu |
| `HardwareInfoHandler` | `hardware.info` | `/proc/cpuinfo`, `uname()`, `/sys/class/hwmon/` | Model CPU, rdzenie/wątki, MHz, architektura, kernel, sensory temperatury |
| `TopProcessesHandler` | `top.processes` | `ps --sort=-%cpu` | Top 15 procesów (PID, user, CPU%, MEM%, RSS, command) |
| `DiskUsageHandler` | `disk.usage` | `/proc/mounts` + `statvfs()` | Użycie partycji (device, mount, fstype, total/used/free/avail, %) |
| `NetworkStatsHandler` | `network.stats` | `/proc/net/dev`, `/sys/class/net/`, `ip -j addr show` | Interfejsy sieciowe z RX/TX, MAC, speed, state, IPv4/IPv6 |
| `JournalQueryHandler` | `journal.query` | `journalctl --output=json` | Wpisy journald z filtrami (unit, priority, lines) |
| `FileListHandler` | `file.list` | `read_dir()` / `sudo ls -laH` | Listing katalogu z typem, rozmiarem. Sudo fallback dla ograniczonych katalogów. Walidacja ścieżek przez `canonicalize()` |
| `SuperuserVerifyHandler` | `superuser.verify` | `unix_chkpwd` | Weryfikacja hasła użytkownika. Zwraca `{ ok: true/false }` |

### Streaming handlery

| Handler | Payload type | Interwał | Źródło danych | Opis |
|---------|-------------|----------|---------------|------|
| `MetricsStreamHandler` | `metrics.stream` | Konfigurowalny (domyślnie 1s) | `/proc/stat`, `/proc/meminfo`, `/proc/loadavg`, `/proc/diskstats`, `/proc/net/dev` | CPU (zagregowany + per-core), pamięć, swap, load avg, disk I/O, net I/O |
| `StorageStreamHandler` | `storage.stream` | Konfigurowalny (domyślnie 2s) | `/proc/diskstats` + `lsblk -J` + `statvfs()` | Szybkości I/O dyskowego + drzewo urządzeń blokowych z użyciem FS |
| `NetworkStreamHandler` | `networking.stream` | Konfigurowalny (domyślnie 1s) | `/proc/net/dev` | Szybkości TX/RX per interfejs (bytes/sec) |
| `TerminalPtyHandler` | `terminal.pty` | — (event-driven) | `openpty()` + `fork()` + `execvp()` | Interaktywny terminal PTY. Dwukierunkowy I/O + resize (TIOCSWINSZ) |

### Bidirectional handlery (open + data)

| Handler | Payload type | Akcje | Opis |
|---------|-------------|-------|------|
| `SystemdManageHandler` | `systemd.manage` | `start`, `stop`, `restart`, `reload`, `enable`, `disable`, `status`, `list` | Zarządzanie usługami systemd. Weryfikuje hasło przez `unix_chkpwd`, wykonuje `systemctl` bezpośrednio (bridge działa jako root) |
| `ContainersHandler` | `container.manage` | `list_containers`, `list_images`, `inspect`, `start`, `stop`, `restart`, `remove`, `remove_image`, `pull`, `create`, `logs`, `service_status`, `service_start/stop/restart` | Docker/Podman. Auto-detekcja runtime (podman → docker) |
| `NetworkManageHandler` | `networking.manage` | `list_interfaces`, `firewall_status/rules/enable/disable/add_rule/remove_rule`, `add_bridge`, `add_vlan`, `remove_interface`, `iface_up/down`, `vpn_list/connect/disconnect`, `network_logs` | Zarządzanie siecią. Multi-backend firewall (ufw/firewalld/nftables/iptables) |
| `PackagesHandler` | `packages.manage` | `detect`, `list_installed`, `search`, `package_info`, `install`, `remove`, `check_updates`, `update_system`, `list_repos`, `add_repo`, `remove_repo`, `refresh_repos` | Pakiety systemowe. Auto-detekcja (pacman/apt/dnf) |
| `HostsManageHandler` | `hosts.manage` | `list`, `add`, `remove` | CRUD hostów zdalnych. Persistence w `~/.config/tenodera/hosts.json` |



## Szczegóły implementacyjne

### Terminal PTY (`terminal_pty.rs`)
- Otwieranie PTY: `nix::pty::openpty()` z konfigurowalnymi wymiarami
- Fork: `nix::unistd::fork()` → dziecko execvp shell
- Rodzic: `AsyncFd` na master FD (non-blocking) do odczytu, dup'd FD do zapisu
- Detekcja shella: parsowanie `/etc/passwd` po UID
- Resize: `TIOCSWINSZ` ioctl na FD przy wiadomości `{ "resize": { "cols": N, "rows": N } }`
- Input klienta: pisanie do master FD przy wiadomości `{ "input": "..." }`

### Firewall multi-backend (`networking.rs`)
- Detekcja: sprawdza `which` dla ufw → firewalld → nftables → iptables
- Status/rules: odpytuje wszystkie wykryte backendy naraz
- Smart filtering: ukrywa wewnętrzne łańcuchy ufw/docker/firewalld z nftables/iptables
- Add/remove: różna logika per backend (porty vs serwisy)

### Container management (`containers.rs`)
- Auto-detekcja: podman (preferowany) → docker
- JSON parsing: obsługa zarówno JSON array (docker) jak i JSON-per-line (podman)
- Create: obsługa nazw, portów, env vars, volumów, restart policy, custom commands
- Service: zarządzanie `docker.service` / `podman.socket` przez systemctl

### Package management (`packages.rs`)
- Auto-detekcja: pacman → apt → dnf
- Dedykowane parsery per distro (pacman -Q, dpkg-query, rpm -qa)
- Obsługa repozytoriów specyficzna per menedżer

## Zależności

- `tenodera-protocol` — współdzielone typy
- `tokio` — async runtime (full features)
- `serde` + `serde_json` — serializacja JSON
- `nix 0.29` — PTY, fork, setsid, dup2, ioctl, hostname
- `libc` — surowe syscalle (read, write, ioctl, fcntl, statvfs)
- `async-trait` — async trait methods
- `zbus 5` — D-Bus (dostępne, nieużywane bezpośrednio)
- `chrono` — timestampy
- `tracing` — logowanie
- `uuid` — generowanie ID kanałów

## Uruchomienie

Bridge jest normalnie uruchamiany przez gateway (nie bezpośrednio):

```bash
# Ręczne testowanie — pipe JSON na stdin:
echo '{"type":"open","channel":"ch1","payload":"system.info"}' | ./tenodera-bridge
```

Zmienne środowiskowe:
- `RUST_LOG` — filtr logów, np. `tenodera_bridge=debug`
