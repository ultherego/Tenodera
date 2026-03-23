# tenodera-ui

React frontend application (SPA) for the Tenodera administration panel. Communicates with the gateway via WebSocket using the channel protocol.

## Tech stack

| Technology | Version | Role |
|------------|---------|------|
| React | 19 | UI framework |
| TypeScript | 5.7 | Type safety |
| Vite | 6 | Bundler + dev server |
| React Router DOM | 7 | SPA routing |
| @tanstack/react-query | 5 | Async state management & caching |
| Recharts | 3.8 | Charts (Dashboard, metrics) |
| @xterm/xterm | 5.5 | Terminal emulator |
| @xterm/addon-fit | 0.10 | Terminal auto-resize |

## Architecture

### Transport layer (`src/api/`)

#### `transport.ts` — Channel WebSocket

Singleton WebSocket with channel multiplexing:

- **`connect()`** — establishes WS to `/api/ws?session_id=...` (auto wss:/ws: based on page protocol)
- **`disconnect()`** — closes WS and clears listeners
- **`openChannel(payload, options)`** — opens a channel, returns object with:
  - `channel` — channel ID
  - `onMessage(cb)` — register callback
  - `send(data)` — send data
  - `close()` — close the channel
- **`request(payload, options)`** — one-shot: opens channel, collects all Data, resolves on Close

Incoming messages are routed to listeners by channel ID. Server Ping is automatically answered with Pong.

#### `auth.ts` — Login client

```typescript
login(user, password): Promise<{ session_id, user }>
// POST /api/auth/login
```

#### `HostTransportContext.tsx` — Multi-host routing

React Context + `useTransport()` hook for transparent routing:

- Without `hostId` → channels go to local bridge
- With `hostId` → adds `{ host: hostId }` to Open options → gateway routes via SSH to remote bridge

```tsx
<HostTransportProvider value="remote-host-id">
  <Dashboard />  {/* automatically queries the remote host */}
</HostTransportProvider>
```

### Routing (`App.tsx`)

| Path | Component | Description |
|------|-----------|-------------|
| `/login` | `Login` | Login form |
| `/*` | `Shell` | Layout with navigation (nested routes) |

`Shell` contains a sidebar navigation and nested routing to sub-pages.

### Pages (`src/pages/`)

| Page | Payload types | Description |
|------|--------------|-------------|
| `Dashboard.tsx` | `system.info`, `metrics.stream` | System info + real-time CPU/RAM/swap/load/IO charts |
| `Services.tsx` | `systemd.manage` | systemd service list with start/stop/restart/enable/disable actions |
| `Containers.tsx` | `container.manage` | Docker/Podman: containers, images, creation, logs |
| `Storage.tsx` | `storage.stream`, `disk.usage` | Block device tree + I/O charts + partition usage |
| `Networking.tsx` | `networking.stream`, `networking.manage` | Interfaces, network traffic, firewall, bridges, VLAN, VPN |
| `Packages.tsx` | `packages.manage` | System packages: list, search, install, repositories |
| `Logs.tsx` | `journal.query` | journald entries with filters |
| `Terminal.tsx` | `terminal.pty` | Full terminal emulator (xterm.js) with resize |
| `Files.tsx` | `file.list` | File browser with navigation |
| `Hosts.tsx` | `hosts.manage` | Remote host management (CRUD) |
| `RemoteDashboard.tsx` | same as `Dashboard` | Remote host dashboard (via `HostTransportProvider`) |
| `RemoteShell.tsx` | — | Remote host layout with nested pages |
| `Login.tsx` | — | Login form, saves session_id to sessionStorage |
| `Shell.tsx` | — | Main layout with sidebar, internal routing |

### Session management

- After login: `session_id` and `user` in `sessionStorage`
- WebSocket connects with `session_id` in query param
- On WS close: redirect to `/login`
- No session → redirect to `/login`

## Development setup

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

### Commands

```bash
npm install      # install dependencies
npm run dev      # dev server on :3000 with HMR
npm run build    # production build to dist/
npm run preview  # preview the build
```

### Production build

Build output goes to `ui/dist/`. The gateway serves these files from the `TENODERA_UI_DIR` directory (default `./ui/dist`).

## File structure

```
ui/
├── index.html          # HTML entry point
├── package.json        # Dependencies and scripts
├── tsconfig.json       # TypeScript configuration
├── vite.config.ts      # Vite configuration + proxy
├── public/             # Static files
└── src/
    ├── main.tsx        # React root (StrictMode + QueryClientProvider)
    ├── App.tsx         # Main router
    ├── index.css       # Global styles
    ├── vite-env.d.ts   # Vite types
    ├── api/
    │   ├── transport.ts            # Channel WebSocket transport
    │   ├── auth.ts                 # Login client
    │   └── HostTransportContext.tsx # Multi-host context
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
