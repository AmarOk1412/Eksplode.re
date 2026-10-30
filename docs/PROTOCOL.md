# Wire Protocol

Eksplode.re has one protocol and two transports. A terminal client and a browser tab exchange
exactly the same messages with the match host; only the framing differs.

| Transport | Used by | Framing |
|-----------|---------|---------|
| TCP, default port 7878 | `eksplode-re-cli join` | one JSON object per line (`\n` terminated, UTF-8) |
| WebSocket `/ws` on the web port (default 8080) | the browser page | one JSON object per **text** frame |

Every message is an object `{"type": "<Variant>", ...fields}`. The Rust definitions live in
[`crates/net/src/protocol.rs`](../crates/net/src/protocol.rs); the codec (line reader and size
limit) in [`crates/net/src/codec.rs`](../crates/net/src/codec.rs). The host is authoritative: it
runs the simulation and validates every message, whatever the transport.

## Explicit Rooms (Version 2)

First-party browser and terminal clients send `Hello` with `version: 2` before admission. The
reply includes server identity, its launch epoch, capabilities and default room settings. A
directory connection owns no slot. `ListRooms` returns at most 32 public summaries per page;
opaque cursors expire after five seconds and room mutations invalidate the current generation.
HTTP `GET /rooms?scope=local` exposes the same read-only directory without creating a player.

`RoomV2` is the Rust `ClientMsg`/`ServerMsg` wrapper for v2 requests/replies, not an extra wire
tag: JSON still uses the request or reply's own `type`. Requests carry a printable ASCII
`request_id` (1-64 characters); admitted operations also identify the bound `room_id`. Room
references include `server_id` and `server_epoch`, so a room on another host or launch cannot
accidentally be selected. Summaries contain occupancy, settings, capacity, phase, revision and
trusted endpoint metadata, never credentials.

- `CreateRoom` explicitly creates a room and admits its creator as owner; `JoinRoom` selects
  an existing lobby. `Welcome` privately returns membership and room state. Capacity is 1-4;
  Team and Chaser require four. Full rooms and running matches reject fresh admission.
- `LeaveRoom` revokes the sender's membership; `LeftRoom` returns that connection to selection.
  Each room independently publishes `RoomUpdated`, `Start`, `Snapshot`, `Ended` and developer
  state. Explicit owner identity, not lowest occupied slot, controls administration.
- Scoped ready/look/input, bot/start/developer commands and revision-checked `SetRoom` retain
  the gameplay payloads described below. An error keeps its typed `code`; owner permission
  errors name the current owner. Out-of-room operations do not mutate either room.
- `RequestStatus` and `CancelRequest` reconcile the original admission ID. The five-second
  deadline includes queue time; timeout means outcome unknown, not permission to retry under
  a new ID. Up to eight private outcomes per connection are retained for 60 seconds.

### Lobby seats and bots

Before a match every empty seat within the room's capacity holds a bot: `CreateRoom` fills them
right after seating the creator, a joining human takes the first empty seat (otherwise the lowest
bot seat; the bot leaves), and a seat a human leaves (`LeaveRoom` or a disconnect) gets a bot
again. A seat the owner empties on purpose stays empty until a player joins or somebody uses "+".
`RoomSummary.occupied` counts bots; clients decide whether a room can be joined from
`connected_humans + reserved_humans`.

| Request | Who | Effect |
|---------|-----|--------|
| `AddBot {request_id, room_id, slot?}` | any member | a bot into the empty seat `slot`; without `slot`, the first free seat |
| `RemoveBot {request_id, room_id, slot?}` | owner only | removes the bot in seat `slot`, which stays empty; without `slot`, the highest bot seat |

`slot` is the 0-based seat index; the web page always sends it, the terminal client (`b`/`x`) does
not. Refusals use typed codes: `BadSlot` (index outside the four seats), `NotABot` (`RemoveBot` on a
human or empty seat), `SlotTaken` (`AddBot` on a taken seat), `Full` (the seat is past the capacity,
or the room is at capacity), `NotOwner`, `InMatch`. `SetRoom` capacity changes keep humans
(`CapacityConflict` counts only humans): bots past the new capacity leave, and newly opened seats get
bots.

### Match statistics

Each player of an `Observation` also carries `kills` (other players eliminated by a flame one of its
bombs lit; eliminating yourself counts for nobody, a landing block is nobody's kill),
`bombs_placed` (every successful placement, whatever became of the bomb) and `eliminated_order`
(`1` for the first elimination of the match, the next number for the next one, shared by players
eliminated in the same tick; `null` while alive). All three are optional for older peers (`0`, `0`,
`null`). The web scoreboard ranks players with them.

Password protection, `RemovePlayer` and `ResumeRoom`/`AckResume` are reserved in the v2 schema
but not implemented yet; they are not advertised as available. Known-server aggregation also
remains future work. Version-1 `Join` is rejected with `Rejected { reason: "BadVersion" }`
followed by closure, without creating a room or taking a seat. There is no implicit-room fallback.

The Rust SDK's `Client::connect(&str)` and `connect_unjoined(&str)` both negotiate v2 without
admission; explicitly call `create_room` or `join_room` with the selected reference. The old
two-argument connect-and-join API and unscoped host local-client/bot/start helpers are removed.
In-process clients use `Host::connect(local)`, send Hello and explicitly admit; operator controls
use `HostHandle::add_room_bot`, `remove_room_bot` or `start_room` with a complete room reference.

## Legacy Version 1 (Reference Only)

The remaining sections describe historical payloads retained for decoding and gameplay DTO
projection, not an accepted single-room admission path. First-party clients use v2 exclusively.

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
| `Join` | `version: u32`, `name: string`, `look` (optional, see [Looks](#looks)), `room` (optional, see [Rooms](#rooms)) | must be the first message that joins; `room` is used only when this join creates the room |
| `RoomQuery` | none | any time, also before `Join`: answered with `RoomInfo`; takes no slot |
| `SetRoom` | `settings` (see [Rooms](#rooms)) | lobby owner only: replace the room settings; refused during a match with `WrongPhase`, a `settings` that is not an object gives `BadRoom` |
| `SetLook` | `look` | lobby only: change the sender's look; refused during a match with `WrongPhase` |
| `SetReady` | `ready: bool` | in the lobby |
| `Input` | `tick: u32`, `mv: "Up"/"Down"/"Left"/"Right"/null`, `bomb: bool`, `punch: bool` (optional, false when missing) | during a match, at most one per tick; `tick` is the latest snapshot the client has seen |
| `Leave` | none | graceful exit |
| `Ping` | `nonce: u64` | optional keep-alive, answered with `Pong` |
| `Start` | none | lobby owner only: start the match |
| `AddBot` | none | lobby owner only: put a bot into the first free slot |
| `RemoveBot` | none | lobby owner only: free the highest bot slot |
| `DevPauseBots` | `paused: bool` | developer mode and owner only: stop or resume the bots |
| `DevSetDrops` | `chances: [f32; 8]` in the order extra bomb, flame, roller, punch, slow, hyper speed, small bomb, kick | developer mode and owner only; each value finite and within 0 to 1, otherwise `BadValue` |
| `DevResetDrops` | none | developer mode and owner only: back to the default drop table |

Examples:

```json
{"type":"Join","version":1,"name":"Ann"}
{"type":"Input","tick":42,"mv":"Left","bomb":false,"punch":false}
{"type":"Start"}
```

## Host to client (`ServerMsg`)

| type | Fields | When |
|------|--------|------|
| `Welcome` | `slot: 0..3`, `config`, `lobby`, `looks: [Look; 4]`, `dev: bool` (optional, false when missing) | the join was accepted |
| `Rejected` | `reason: "Full" / "BadVersion" / "BadName" / "InMatch"` | the join was refused; the connection is closed |
| `RoomInfo` | `open: bool`, `settings`, `humans: u8` | answer to `RoomQuery`: whether a person is in the room, its settings (the start-up defaults while it is closed) and how many humans are in it |
| `Lobby` | `lobby: {slots: [{controller, name, ready}; 4], settings}`, `looks: [Look; 4]` | any lobby change, including a change of the settings |
| `Start` | `seed: u64`, `config` | the owner started a match |
| `Snapshot` | `tick: u32`, `state` (an `Observation` for this slot), `events: [Event]` | every tick of a match (20 Hz) |
| `PlayerLeft` | `slot: u8`, `replaced_by_bot: bool` | a player disconnected or went silent |
| `Ended` | `outcome: {"Winner": slot} / "Draw" / {"TeamWin": 0 or 1}` | the round is over; the lobby follows |
| `Error` | `code: string`, `message: string` | a message was refused; see below |
| `Pong` | `nonce: u64` | answer to `Ping` |
| `DevState` | `paused: bool`, `drops: [f32; 8]` | developer mode: sent to a joiner and to everybody when the pause or the drop table changes |

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
bombs, flames and the power-ups lying on the board. It never contains the random generator or
the drop table. Players carry `has_punch`, `has_kick` and `malus` (`{"kind":"Slow","ticks_left":120}` or
null); a bomb in the air carries `flight` (`{"dir":"Right","hop_left":1,"origin":{"x":3,"y":1}}`).
The config adds `malus_ticks` and `punch_hop_ticks`; all new fields are optional for older
peers. Events gain `BombPunched`, `BombHopped`, `BombLanded`, `BombKicked`, `BombSlid`, `BombStopped`,
`MalusStarted` and `MalusEnded`.
Rooms add to the observation: `config.mode` (`"normal"`, `"team"`, `"chaser"`), `config.friendly_fire`
and the end pattern values (`closing_ticks`, `closing_lead_ticks`), `falling` (the announced squares,
`[{"pos":{"x":1,"y":1},"ticks_left":12}]`), and per player `team` (`0` or `1` in Team mode, else
null), `role`, `hearts` and `shield_left`; a flame carries `owners`, a bit mask of the slots whose
bombs lit it. Events gain `PatternStarted`, `BlockWarned`, `BlockLanded`, `HeartLost` and
`RoleAssigned`. All of them are optional for older peers.
Rules: [GAMEPLAY.md](GAMEPLAY.md).

## Rooms

A room is the lobby plus its settings, one object that a later version can publish on the network:

```json
{"name":"Friday room","mode":"normal","size":"medium","minutes":5,"friendly_fire":true}
```

| Field | Values | Default |
|-------|--------|---------|
| `name` | text, control characters removed, at most 24 characters; empty becomes "<owner>'s room" | empty |
| `mode` | `normal`, `team`, `chaser` (a mode whose rules do not exist yet becomes `normal`) | `normal` |
| `size` | `small` 15 x 13, `medium` 19 x 15, `large` 25 x 19 | `small` |
| `minutes` | `2`, `3`, `5`, `10` | `3` |
| `friendly_fire` | `true` or `false`, used by Team mode | `true` |

The host reads the settings leniently, like looks: a missing, wrongly typed or unknown field
becomes its default and a value that is not an object gives the defaults (only `SetRoom` refuses
that with `BadRoom`), so a room never causes `Malformed`. The first `Join` into an empty room
creates it and its sender becomes the owner; a `room` in any later `Join` is ignored. The room
closes when the last person leaves it, and the next person creates a new one. `Welcome` and `Lobby`
carry the settings in `lobby.settings`; `Start.config` and `Welcome.config` are derived from them.
Old clients never send `RoomQuery` or `SetRoom` and ignore the new fields.

## Looks

A look is a JSON object `{"colour":"Blue","head":"Cap","face":"None","costume":"Scarf"}`. The host
reads it leniently: a missing, wrongly typed or unknown field becomes the default of the sender's
slot for that field, and a value that is not an object gives the full default, so a look never
causes `Malformed`. The four participants of a match always have different colours: when the
requested colour is taken, the sender gets the next free colour in palette order (wrapping) and an
`Error{LookAdjusted}` notice. A bot or a terminal player gets the default look of its slot (blue,
black, red and purple characters). `Welcome` and `Lobby` carry the look of every slot; old hosts send
none and old clients ignore the field. There is no gender choice any more: the `gender` key that
older pages send is read and ignored (never an error), and the graphics module picks the body
silhouette from the slot. A look without `costume` (an older page or host) means `None`.

| Field | Values |
|-------|--------|
| `colour` | `Blue`, `Black`, `Red`, `Purple`, `Green`, `Orange`, `Yellow`, `Pink`, `Teal`, `White` |
| `head` | `None`, `Cap`, `TopHat`, `Crown`, `Headphones` |
| `face` | `None`, `Glasses`, `Sunglasses`, `Moustache`, `Mask` |
| `costume` | `None`, `Scarf`, `Cape`, `BowTie`, `Medal` |

## Error codes

| code | Meaning |
|------|---------|
| `Malformed` | bad JSON, unknown `type`, binary WebSocket frame, or an oversized message |
| `NotJoined` | the first message was not `Join` |
| `AlreadyJoined` | a second `Join` on the same connection |
| `NotOwner` | `Start`, `AddBot`, `RemoveBot`, `SetRoom` or a `Dev*` message from somebody who is not the lobby owner; the message names the owner |
| `CannotStart` | `Start` while a match is already running, or with the wrong number of participants for the mode (the message says how many are missing) |
| `BadRoom` | `SetRoom` whose `settings` is not an object |
| `WrongPhase` | `AddBot`, `RemoveBot` or `SetReady` during a match, or `Input` in the lobby |
| `NoSlot` | `AddBot` with all slots taken |
| `BadTick` | an `Input` from the future |
| `DevDisabled` | a `Dev*` message on a host that was not started with `--dev` |
| `BadValue` | `DevSetDrops` with a value outside 0 to 1, not finite or not eight values |
| `Timeout` | silent for too long; the slot goes to a bot |
| `HostLeft` | the host is shutting down; every client gets it before the connection closes |
| `LookAdjusted` | the requested colour was taken; the message names the colour you got (a notice, not a refusal) |

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

`cargo test -p eksplode-re-net` covers the codec, lobby rules, loopback matches, latency and
agreement of all clients on the result. `cargo test -p eksplode-re-web` repeats the join, start,
play and result flows over WebSocket and mixes browser and terminal players in one match.
