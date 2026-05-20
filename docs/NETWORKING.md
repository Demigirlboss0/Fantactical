# Networking

Fantactical uses an authoritative WebSocket server with JSON-serialized
messages. The server and client share the same binary; the mode is selected
via `NetworkPlugin` configuration in `src/main.rs`.

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                     Bevy App (main thread)               │
│                                                          │
│  ┌──────────────────┐      ┌──────────────────────────┐ │
│  │ NetworkPlugin    │      │ process_network_incoming  │ │
│  │ (startup config) │      │ (Update system)           │ │
│  └──────────────────┘      └──────────────────────────┘ │
│         │                            ▲                   │
│         ▼                            │                   │
│  mpsc::UnboundedSender ──→  mpsc::UnboundedReceiver     │
│  (outgoing to network)       (incoming from network)     │
│         │                            │                   │
├─────────┼────────────────────────────┼───────────────────┤
│         ▼                            ▲                   │
│  ┌──────────────────────────────────────────┐           │
│  │         Tokio Runtime Task                │           │
│  │                                            │           │
│  │  Server mode:  TcpListener → accept loop   │           │
│  │  Client mode:  connect_async → read loop   │           │
│  └──────────────────────────────────────────┘           │
│         │                            │                   │
│         ▼                            ▼                   │
│  ┌──────────┐              ┌──────────────────┐         │
│  │  Client  │              │     Client       │         │
│  └──────────┘              └──────────────────┘         │
└──────────────────────────────────────────────────────────┘
```

## Protocol

### Client → Server: `ClientMessage`

`src/network/mod.rs:6`

| Variant | Fields | Purpose |
|---------|--------|---------|
| `Auth` | `token` | Session authentication |
| `DeclareManeuver` | `source_id`, `target_id`, `target_hex`, `maneuver`, `extra_efforts` | Declare a maneuver |
| `SelectDefense` | `defender_id`, `defense_type` | Choose active defense |
| `RollDice` | — | Request a dice roll |
| `AddModifier` | `label`, `value`, `actor_id` | Add a modifier |
| `RemoveModifier` | `index`, `actor_id` | Remove a modifier |
| `Rewind` | — | Rewind game state |
| `SetPainThreshold` | `actor_id`, `threshold` | Set pain threshold |
| `SetPosture` | `actor_id`, `posture` | Change actor posture |
| `MoveActor` | `actor_id`, `position` | Move token on battlemap |
| `ReorderTurnOrder` | `from_index`, `to_index` | Reorder turn |
| `ShockToggle` | `enabled` | Toggle shock rules |
| `ImportSheet` | `json_data` | Import GCS sheet |

### Server → Client: `ServerMessage`

`src/network/mod.rs:65`

| Variant | Fields | Purpose |
|---------|--------|---------|
| `AuthSuccess` | `client_id`, `is_gm` | Authentication succeeded |
| `AuthFailure` | `reason` | Authentication failed |
| `StateSnapshot` | `history` | Full `GameStateHistory` for sync |
| `RollResult` | `label`, `roll` | Dice roll result for display |
| `LogEntry` | `entry` | Event log entry |
| `ActorOwnership` | `actor_ids` | Which actors client controls |
| `Error` | `message` | Server error |

## Server (`src/server/mod.rs`)

### Spawning

`spawn_server(config)` returns `(outgoing_tx, incoming_rx)` — mpsc channels
for Bevy ↔ network communication.

The server task binds `TcpListener` and enters a `tokio::select!` loop:

```
loop {
    select! {
        accept → spawn handle_connection(stream, state, incoming)
        outgoing → broadcast or send-to-client
    }
}
```

### Connection Handler (`handle_connection()` at line 145)

1. Accept WebSocket upgrade
2. **Auth handshake** (added in audit):
   - Read first message → must be `ClientMessage::Auth`
   - Validate token against `ServerConfig.session_token`
   - On failure → send `AuthFailure`, close connection
   - On success → assign client ID, mark GM if first client
3. Send `AuthSuccess`
4. Enter message loop via `tokio::select!`:
   - Read client messages → forward to Bevy `incoming` channel
   - Read outgoing channel → send to client WebSocket
   - Handle Ping/Pong, Close
5. On disconnect → remove from client map, send `ClientDisconnected`

### Security

- Session token validated on first message
- First client to connect becomes GM automatically
- Subsequent clients are non-GM
- Invalid tokens receive `AuthFailure` and connection is closed

## Client (`src/client/mod.rs`)

### Spawning

`spawn_client()` returns `(outgoing_tx, incoming_rx)` channels.

### Connection Loop

1. Wait for `ClientOutgoing::Connect` message with host/port/token
2. Connect via `connect_async()`
3. Authenticate by sending `ClientMessage::Auth`
4. Enter read loop via `tokio::select!`:
   - Read server messages → forward to Bevy `incoming` channel
   - Read outgoing channel → send client messages to server
   - Handle disconnection
5. On disconnect → exponential backoff (1s → 2s → 4s → ... max 30s)
   then retry

### ClientIncoming Messages

| Variant | Purpose |
|---------|---------|
| `Connected` | Auth succeeded, connection established |
| `Disconnected` | Connection lost or failed |
| `Message(ServerMessage)` | Server message forwarded to Bevy |
| `Status(String)` | Status update for UI |

## Bevy Integration (`src/systems/network.rs`)

### NetworkPlugin

Configuration:

```rust
NetworkPlugin {
    mode: NetworkMode::Server,  // or Client, or Off
    session_token: "fantactical".into(),
    connect_host: Some("127.0.0.1".into()),
    connect_port: Some(9002),
}
```

Default mode: `NetworkMode::Off` (no networking).

### process_network_incoming System

Runs in `Update`. Polls both `ServerIncomingRx` and `ClientIncomingRx`
channels each frame:

- **Server mode**: Processes `ClientConnected`, `ClientDisconnected`,
  `Message` events from connected clients
- **Client mode**: Processes `Connected`, `Disconnected`, `Message`
  events from server; applies `StateSnapshot` to local `GameStateResource`;
  displays `RollResult` and `LogEntry` messages

### NetworkState Resource

```rust
pub struct NetworkState {
    pub connected: bool,
    pub client_id: Option<u64>,
    pub is_gm: bool,
    pub status: String,  // shown in UI
}
```

## Message Flow Example

### Client Declares a Maneuver (Server Mode)

```
Client                          Server                      Bevy
  │                               │                          │
  │── ClientMessage::DeclareManeuver ──→                      │
  │                               │                          │
  │                               │── ServerIncoming::Message ──→
  │                               │                          │ process_network_incoming
  │                               │                          │ validates, pushes state
  │                               │                          │
  │                               │←── ServerOutgoing::Broadcast ──
  │                               │                          │
  │←── ServerMessage::StateSnapshot ──                       │
  │                               │                          │
```

### Client Mode (Remote Player)

```
Server                          Client                      Bevy
  │                               │                          │
  │── ServerMessage::StateSnapshot ──→                        │
  │                               │                          │
  │                               │── ClientIncoming::Message ──→
  │                               │                          │ process_network_incoming
  │                               │                          │ state_res.history = new_history
  │                               │                          │
```

## Port Configuration

Default port: `9002` (`src/network/mod.rs:4`). Configurable via
`ServerConfig.port` or `ClientOutgoing::Connect.port`.
