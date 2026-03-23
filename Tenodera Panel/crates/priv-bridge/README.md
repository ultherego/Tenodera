# tenodera-priv-bridge

Uprzywilejowany helper działający jako root z restrykcyjnym allowlistem operacji. Zaprojektowany do bezpiecznej eskalacji uprawnień dla ograniczonego zbioru komend systemowych.

## Rola w architekturze

`tenodera-priv-bridge` jest opcjonalnym komponentem uruchamianym jako root (przez systemd socket activation lub bezpośrednio). Przyjmuje wiadomości JSON ze stdin i odpowiada na stdout — identycznie jak zwykły bridge — ale z uprawnieniami root. Tylko ściśle zdefiniowane operacje są dopuszczone (allowlist).

```
Gateway → stdin/stdout → priv-bridge (root)   ← tylko dozwolone operacje
```

## Bezpieczeństwo — model allowlist

Priv-bridge implementuje **whitelistę payload types** jako główny mechanizm bezpieczeństwa:

```rust
const ALLOWED_PAYLOADS: &[&str] = &[
    "systemd.unit.action",
    "package.updates",
];
```

Każda wiadomość `Open` jest walidowana:
1. Sprawdzenie czy `payload_type` jest na liście `ALLOWED_PAYLOADS`
2. Jeśli nie → natychmiastowe odrzucenie z `Close` + komunikatem błędu
3. Jeśli tak → dispatch do odpowiedniego handlera

## Implementacja (`main.rs`)

### Pętla główna

Synchroniczna pętla stdin/stdout (nie async):

```
loop {
    1. Czytaj linię ze stdin
    2. Deserializuj JSON → Message
    3. Match na typ wiadomości:
       - Open → sprawdź allowlist → dispatch lub odrzuć
       - Data → odrzuć (brak aktywnych kanałów bidirectional)
       - Close → ignoruj
       - Ping → zwróć Pong
    4. Serializuj odpowiedzi → stdout
}
```

### Dispatch operacji

Aktualnie stub — po walidacji allowlist zwraca:
- `Ready` (potwierdzenie otwarcia kanału)
- `Close` (natychmiastowe zamknięcie)

Docelowo handlery powinny implementować właściwą logikę (np. `systemctl` bezpośrednio jako root bez sudo).

### Odrzucenie niedozwolonej operacji

```json
// Wejście:
{"type": "open", "channel": "ch1", "payload": "terminal.pty"}

// Odpowiedź:
{"type": "close", "channel": "ch1", "problem": "not-authorized", "message": "Payload 'terminal.pty' is not allowed in privileged bridge"}
```

## Status implementacji

| Element | Status |
|---------|--------|
| Pętla stdin/stdout | ✅ Zaimplementowana |
| Allowlist walidacja | ✅ Zaimplementowana |
| Ping/Pong | ✅ Zaimplementowane |
| `systemd.unit.action` handler | ⚠️ Stub (Ready+Close) |
| `package.updates` handler | ⚠️ Stub (Ready+Close) |

## Konfiguracja systemd

Plik `systemd/tenodera-priv-bridge.service`:

```ini
[Service]
Type=simple
ExecStart=/usr/local/bin/tenodera-priv-bridge
StandardInput=socket
User=root

# Hardening
NoNewPrivileges=false          # Wymaga eskalacji
ProtectSystem=strict
ProtectHome=read-only
PrivateTmp=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectControlGroups=yes
RestrictSUIDSGID=yes
```

## Zależności

- `tenodera-protocol` — współdzielone typy wiadomości
- `tokio` — runtime (choć pętla główna jest synchroniczna)
- `serde` + `serde_json` — JSON
- `nix 0.29` — operacje systemowe
- `tracing` — logowanie

## Planowany rozwój

Docelowa architektura priv-bridge:
1. **Allowlist rozszerzalny** — dodawanie nowych dozwolonych operacji
2. **Właściwe handlery** — bezpośrednie wywołania systemowe jako root (bez sudo)
3. **Auditing** — logowanie każdej operacji z timestampem i danymi
4. **Rate limiting** — ograniczenie częstotliwości operacji
