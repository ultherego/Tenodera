# tenodera-protocol

Biblioteka współdzielonych typów definiująca protokół komunikacji kanałowej pomiędzy wszystkimi komponentami systemu Tenodera.

## Rola w architekturze

`tenodera-protocol` to crate typu **library** — nie produkuje żadnego binarnego pliku wykonywalnego. Jest zależnością dla wszystkich pozostałych crate'ów w workspace (`gateway`, `bridge`, `priv-bridge`). Definiuje wspólny "język" wymiany wiadomości: enumy, struktury i typy błędów, które gwarantują spójność serializacji/deserializacji JSON pomiędzy frontendem, gateway i bridge'em.

## Moduły

### `message.rs` — Enumy wiadomości protokołu

Główny typ `Message` to enum z tagiem `type` (serde tag), reprezentujący ramki przesyłane przez WebSocket:

| Wariant | Kierunek | Opis |
|---------|----------|------|
| `Open` | Klient → Bridge | Otwiera nowy kanał. Zawiera `channel` (ID) i `ChannelOpenOptions` (payload type + extra opcje) |
| `Ready` | Bridge → Klient | Potwierdzenie gotowości kanału |
| `Data` | Dwukierunkowy | Dane payload na otwartym kanale (`serde_json::Value`) |
| `Control` | Dwukierunkowy | Sygnał sterujący na kanale (komenda + dodatkowe pola) |
| `Close` | Dwukierunkowy | Zamknięcie kanału. `problem: None` = czyste zamknięcie, `Some(reason)` = błąd |
| `Auth` | Klient → Gateway | Uwierzytelnianie (credentials) |
| `AuthResult` | Gateway → Klient | Wynik uwierzytelniania (success/failure + user) |
| `Ping` | Dwukierunkowy | Heartbeat |
| `Pong` | Dwukierunkowy | Odpowiedź na heartbeat |

Typ `AuthCredentials` obsługuje dwa schematy:
- **Basic** — `user` + `password`
- **Token** — bearer `token`

### `channel.rs` — Typy kanałów

- `ChannelId` — alias na `String`, unikalny identyfikator kanału w sesji
- `ChannelState` — maszyna stanów kanału: `Opening` → `Ready` → `Closing` → `Closed`
- `ChannelOpenOptions` — opcje wysyłane przy otwieraniu kanału:
  - `payload: String` — wymagany, wskazuje handler w bridge (np. `"metrics.stream"`)
  - `superuser: Option<SuperuserMode>` — opcjonalny tryb uprawnień (`Require` lub `Try`)
  - `extra: serde_json::Map` — dodatkowe opcje specyficzne dla payloadu (flatten)
- `SuperuserMode` — enum: `Require` (wymuś root) lub `Try` (spróbuj, nie failuj)

### `payload.rs` — Rejestr typów payload

Enum `Payload` mapuje stringi payload type na warianty:

| String | Wariant | Opis |
|--------|---------|------|
| `system.info` | `SystemInfo` | Informacje o hoście |
| `systemd.units` | `SystemdUnits` | Lista usług systemd |
| `systemd.unit.action` | `SystemdUnitAction` | Akcje na usługach |
| `journal.query` | `JournalQuery` | Zapytania do journald |
| `journal.follow` | `JournalFollow` | Śledzenie journald (streaming) |
| `file.read` | `FileRead` | Odczyt pliku |
| `file.write` | `FileWrite` | Zapis pliku |
| `file.list` | `FileList` | Listing katalogu |
| `process.exec` | `ProcessExec` | Wykonanie procesu |
| `process.stream` | `ProcessStream` | Streaming procesu |
| `terminal.pty` | `TerminalPty` | Terminal PTY |
| `network.interfaces` | `NetworkInterfaces` | Interfejsy sieciowe |
| `firewall.rules` | `FirewallRules` | Reguły firewalla |
| `metrics.stream` | `MetricsStream` | Streaming metryk |
| `ssh.remote` | `SshRemote` | Zdalne SSH |
| `package.updates` | `PackageUpdates` | Aktualizacje pakietów |
| `container.list` | `ContainerList` | Lista kontenerów |
| `Custom(String)` | — | Escape hatch dla nieznanych/przyszłych payloadów |

Metody: `from_str()` / `as_str()` do konwersji string ↔ enum, `Display` trait.

### `error.rs` — Typy błędów

`ProtocolError` (z `thiserror`) definiuje warianty:

| Wariant | Opis |
|---------|------|
| `InvalidMessage` | Nieprawidłowy format wiadomości |
| `UnknownPayload` | Nieznany typ payload |
| `ChannelNotFound` | Kanał nie istnieje |
| `ChannelAlreadyExists` | Kanał już istnieje |
| `AuthFailed` | Błąd uwierzytelniania |
| `PermissionDenied` | Odmowa dostępu |
| `Serialization` | Błąd serializacji JSON (from `serde_json::Error`) |
| `Transport` | Błąd warstwy transportowej |

## Zależności

- `serde` + `serde_json` — serializacja/deserializacja JSON
- `thiserror` — deklaratywne typy błędów
- `uuid` — generowanie ID (dostępny, choć nieużywany bezpośrednio w tym crate)
- `chrono` — typy czasowe
- `bytes` — bufory bajtowe

## Przykład użycia

```rust
use tenodera_protocol::message::Message;
use tenodera_protocol::channel::ChannelOpenOptions;

// Deserializacja wiadomości z JSON
let json = r#"{"type":"open","channel":"ch1","payload":"metrics.stream","interval":1000}"#;
let msg: Message = serde_json::from_str(json).unwrap();

// Serializacja odpowiedzi
let resp = Message::Ready { channel: "ch1".to_string() };
let json_out = serde_json::to_string(&resp).unwrap();
```

## Format na drucie

Każda ramka WebSocket to pojedynczy obiekt JSON z polem `type`:

```jsonc
// Open
{ "type": "open", "channel": "ch1", "payload": "system.info" }

// Ready
{ "type": "ready", "channel": "ch1" }

// Data
{ "type": "data", "channel": "ch1", "data": { "hostname": "server1", ... } }

// Close (czyste)
{ "type": "close", "channel": "ch1" }

// Close (z błędem)
{ "type": "close", "channel": "ch1", "problem": "access-denied" }

// Ping/Pong
{ "type": "ping" }
{ "type": "pong" }
```
