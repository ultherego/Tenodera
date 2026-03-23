# tenodera-ui

Frontendowa aplikacja React (SPA) dla panelu administracyjnego Tenodera. Komunikuje się z gateway przez WebSocket używając protokołu kanałowego.

## Stack technologiczny

| Technologia | Wersja | Rola |
|-------------|--------|------|
| React | 19 | Framework UI |
| TypeScript | 5.7 | Typowanie |
| Vite | 6 | Bundler + dev server |
| React Router DOM | 7 | Routing SPA |
| @tanstack/react-query | 5 | Cache i zarządzanie stanem async |
| Recharts | 3.8 | Wykresy (Dashboard, metryki) |
| @xterm/xterm | 5.5 | Emulator terminala |
| @xterm/addon-fit | 0.10 | Auto-resize terminala |

## Architektura

### Warstwa transportowa (`src/api/`)

#### `transport.ts` — WebSocket kanałowy

Singleton WebSocket z multiplexingiem kanałów:

- **`connect()`** — nawiązuje WS do `/api/ws?session_id=...` (auto wss:/ws: wg protokołu strony)
- **`disconnect()`** — zamyka WS i czyści listenery
- **`openChannel(payload, options)`** — otwiera kanał, zwraca obiekt z:
  - `channel` — ID kanału
  - `onMessage(cb)` — rejestracja callbacka
  - `send(data)` — wysyłanie danych
  - `close()` — zamknięcie kanału
- **`request(payload, options)`** — one-shot: otwiera kanał, zbiera wszystkie Data, resolve na Close

Wiadomości przychodzące są routowane do listenerów po channel ID. Ping od serwera jest automatycznie odpowiadany Pong.

#### `auth.ts` — Klient logowania

```typescript
login(user, password): Promise<{ session_id, user }>
// POST /api/auth/login
```

#### `HostTransportContext.tsx` — Routing multi-host

React Context + hook `useTransport()` do transparentnego routingu:

- Bez `hostId` → kanały idą do lokalnego bridge
- Z `hostId` → dodaje `{ host: hostId }` do opcji Open → gateway routuje przez SSH do zdalnego bridge

```tsx
<HostTransportProvider value="remote-host-id">
  <Dashboard />  {/* automatycznie odpytuje zdalny host */}
</HostTransportProvider>
```

### Routing (`App.tsx`)

| Ścieżka | Komponent | Opis |
|----------|-----------|------|
| `/login` | `Login` | Formularz logowania |
| `/*` | `Shell` | Layout z nawigacją (zagnieżdżone route) |

`Shell` zawiera sidebar nawigacyjny i zagnieżdżony routing do podstron.

### Strony (`src/pages/`)

| Strona | Payload types | Opis |
|--------|--------------|------|
| `Dashboard.tsx` | `system.info`, `metrics.stream` | Informacje o systemie + wykresy CPU/RAM/swap/load/IO w czasie rzeczywistym |
| `Services.tsx` | `systemd.manage` | Lista usług systemd z akcjami start/stop/restart/enable/disable |
| `Containers.tsx` | `container.manage` | Docker/Podman: kontenery, obrazy, tworzenie, logi |
| `Storage.tsx` | `storage.stream`, `disk.usage` | Drzewo urządzeń blokowych + wykresy I/O + użycie partycji |
| `Networking.tsx` | `networking.stream`, `networking.manage` | Interfejsy, ruch sieciowy, firewall, mosty, VLAN, VPN |
| `Packages.tsx` | `packages.manage` | Pakiety systemowe: lista, wyszukiwanie, instalacja, repozytoria |
| `Logs.tsx` | `journal.query` | Wpisy journald z filtrami |
| `Terminal.tsx` | `terminal.pty` | Pełny emulator terminala (xterm.js) z resize |
| `Files.tsx` | `file.list` | Przeglądarka plików z nawigacją |
| `Hosts.tsx` | `hosts.manage` | Zarządzanie hostami zdalnymi (CRUD) |
| `RemoteDashboard.tsx` | jak `Dashboard` | Dashboard zdalnego hosta (via `HostTransportProvider`) |
| `RemoteShell.tsx` | — | Layout dla zdalnego hosta ze zagnieżdżonymi stronami |
| `Login.tsx` | — | Formularz logowania, zapisuje session_id w sessionStorage |
| `Shell.tsx` | — | Główny layout z sidebar, routing wewnętrzny |

### Zarządzanie sesją

- Po zalogowaniu: `session_id` i `user` w `sessionStorage`
- WebSocket łączy się z `session_id` w query param
- Przy zamknięciu WS: redirect do `/login`
- Brak session → redirect do `/login`

## Konfiguracja deweloperska

### Vite dev server

```typescript
// vite.config.ts
server: {
  port: 3000,
  proxy: {
    '/api': {
      target: 'http://127.0.0.1:9090',
      ws: true,              // proxy WebSocket
      changeOrigin: true,
    },
  },
}
```

### Komendy

```bash
npm install      # instalacja zależności
npm run dev      # dev server na :3000 z HMR
npm run build    # produkcyjny build do dist/
npm run preview  # podgląd builda
```

### Produkcyjny build

Build trafia do `ui/dist/`. Gateway serwuje te pliki z katalogu `TENODERA_UI_DIR` (domyślnie `./ui/dist`).

## Struktura plików

```
ui/
├── index.html          # Entry point HTML
├── package.json        # Zależności i skrypty
├── tsconfig.json       # Konfiguracja TypeScript
├── vite.config.ts      # Konfiguracja Vite + proxy
├── public/             # Pliki statyczne
└── src/
    ├── main.tsx        # React root (StrictMode + QueryClientProvider)
    ├── App.tsx         # Router główny
    ├── index.css       # Style globalne
    ├── vite-env.d.ts   # Typy Vite
    ├── api/
    │   ├── transport.ts            # WebSocket transport kanałowy
    │   ├── auth.ts                 # Klient logowania
    │   └── HostTransportContext.tsx # Context multi-host
    └── pages/
        ├── Login.tsx
        ├── Shell.tsx
        ├── Dashboard.tsx
        ├── Services.tsx
        ├── Containers.tsx
        ├── Storage.tsx
        ├── Networking.tsx
        ├── Packages.tsx
        ├── Logs.tsx
        ├── Terminal.tsx
        ├── Files.tsx
        ├── Hosts.tsx
        ├── RemoteDashboard.tsx
        └── RemoteShell.tsx
```
