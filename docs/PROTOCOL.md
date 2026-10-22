# Wire protocol (version 1)

Bomberust has one protocol and two transports. A terminal client and a browser tab exchange
exactly the same messages with the match host; only the framing differs.

| Transport | Used by | Framing |
|-----------|---------|---------|
| TCP, default port 7878 | `bomberust-cli join` | one JSON object per line (`\n` terminated, UTF-8) |
| WebSocket `/ws` on the web port (default 8080) | the browser page | one JSON object per **text** frame |

Every message is an object `{"type": "<Variant>", ...fields}`. The Rust definitions live in
[`crates/net/src/protocol.rs`](../crates/net/src/protocol.rs); the codec (line reader and size
limit) in [`crates/net/src/codec.rs`](../crates/net/src/codec.rs). The host is authoritative: it
runs the simulation and validates every message, whatever the transport.

## Limits

| Limit | Value | What happens |
|-------|-------|--------------|
| Message size | 64 KiB | TCP: the connection is closed. WebSocket: close code `1009` |
| Malformed messages per connection | 10 | each one gets `Error{Malformed}`; the tenth closes the connection (WebSocket close code `1008`) |
| Silence during a match | 10 s | the player is replaced by a bot (`Error{Timeout}`, then `PlayerLeft`) |
| Participants | 4 | a fifth `Join` gets `Rejected{Full}` |
| Name | 1 to 16 printable characters | otherwise `Rejected{BadName}` |

## Client to host (`ClientMsg`)

| type | Fields | When |
|------|--------|------|
| `Join` | `version: u32`, `name: string` | must be the first message |
| `SetReady` | `ready: bool` | in the lobby |
| `Input` | `tick: u32`, `mv: "Up"/"Down"/"Left"/"Right"/null`, `bomb: bool` | during a match, at most one per tick; `tick` is the latest snapshot the client has seen |
| `Leave` | none | graceful exit |
| `Ping` | `nonce: u64` | optional keep-alive, answered with `Pong` |
| `Start` | none | lobby owner only: start the match |
| `AddBot` | none | lobby owner only: put a bot into the first free slot |
| `RemoveBot` | none | lobby owner only: free the highest bot slot |

Examples:

```json
{"type":"Join","version":1,"name":"Ann"}
{"type":"Input","tick":42,"mv":"Left","bomb":false}
{"type":"Start"}
```

## Host to client (`ServerMsg`)

| type | Fields | When |
|------|--------|------|
| `Welcome` | `slot: 0..3`, `config`, `lobby` | the join was accepted |
| `Rejected` | `reason: "Full" / "BadVersion" / "BadName" / "InMatch"` | the join was refused; the connection is closed |
| `Lobby` | `lobby: {slots: [{controller, name, ready}; 4]}` | any lobby change |
| `Start` | `seed: u64`, `config` | the owner started a match |
| `Snapshot` | `tick: u32`, `state` (an `Observation` for this slot), `events: [Event]` | every tick of a match (20 Hz) |
| `PlayerLeft` | `slot: u8`, `replaced_by_bot: bool` | a player disconnected or went silent |
| `Ended` | `outcome: {"Winner": slot} / "Draw"` | the round is over; the lobby follows |
| `Error` | `code: string`, `message: string` | a message was refused; see below |
| `Pong` | `nonce: u64` | answer to `Ping` |

`controller` in a lobby slot is `Local` (a person at the host), `Remote` (a person on the
network, terminal or browser), `Bot` or `Empty`.

Examples:

```json
{"type":"Welcome","slot":0,"config":{"...":"..."},"lobby":{"slots":["..."]}}
{"type":"Snapshot","tick":43,"state":{"me":0,"tick":43,"arena":{"...":"..."}},"events":[]}
{"type":"Error","code":"NotOwner","message":"Only Ann can start the match"}
{"type":"Ended","outcome":{"Winner":1}}
```

An `Observation` is what any participant, human or bot, may see: the arena, the four players,
bombs, flames and the power-ups that are not hidden under blocks. It never contains the random
generator.

## Error codes

| code | Meaning |
|------|---------|
| `Malformed` | bad JSON, unknown `type`, binary WebSocket frame, or an oversized message |
| `NotJoined` | the first message was not `Join` |
| `AlreadyJoined` | a second `Join` on the same connection |
| `NotOwner` | `Start`, `AddBot` or `RemoveBot` from somebody who is not the lobby owner; the message names the owner |
| `CannotStart` | `Start` while a match is already running |
| `WrongPhase` | `AddBot`, `RemoveBot` or `SetReady` during a match, or `Input` in the lobby |
| `NoSlot` | `AddBot` with all slots taken |
| `BadTick` | an `Input` from the future |
| `Timeout` | silent for too long; the slot goes to a bot |
| `HostLeft` | the host is shutting down; every client gets it before the connection closes |

An `Error` never ends a match for the others: the bad message is dropped.

## Rules

- The first message must be `Join` with `version: 1`; a different version gets
  `Rejected{BadVersion}`.
- **Lobby owner:** the lowest slot held by a person (`Local` or `Remote`). With the headless
  server nobody sits at the host, so the first browser or terminal player is the owner. When the
  owner leaves, the next person becomes owner.
- An `Input` is applied at the next host tick if it arrives before it; late inputs are dropped; a
  missing input means "no action". Clients send their current key state every tick.
- If a player disconnects or stays silent for 10 s during a match, a bot takes the slot and
  everybody receives `PlayerLeft{replaced_by_bot: true}`. In the lobby the slot is simply freed.
- After `Ended`, the host sends a `Lobby`; the owner can `Start` again (a rematch).
- When the host leaves, every client gets `Error{HostLeft}` and the connection is closed
  (WebSocket close code `1001`).

## WebSocket specifics

- The handshake must carry an acceptable `Origin`; see [WEB.md](WEB.md) and
  [SECURITY.md](SECURITY.md).
- Ping and pong control frames are answered by the server. Binary frames are answered with
  `Error{Malformed}`.
- The browser connects with `new WebSocket("ws://<host>:<web-port>/ws")`.

## Versioning

New message types and new fields are additive: a client that does not know a message ignores it,
and the protocol version stays `1`. A change that breaks an existing message bumps
`PROTOCOL_VERSION` and old clients are refused with `BadVersion`.

## Tests

`cargo test -p bomberust-net` covers the codec, lobby rules, loopback matches, latency and
agreement of all clients on the result. `cargo test -p bomberust-web` repeats the join, start,
play and result flows over WebSocket and mixes browser and terminal players in one match.
