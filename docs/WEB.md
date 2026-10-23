# The web server and the page

`bomberust-server` (crate `bomberust-web`) runs one match host and a small web server in a single
process. The page it serves draws the game in a canvas with a WebAssembly module and talks to the
host over a WebSocket.

```sh
cargo run -p bomberust-web -- --bots 1
# Web page:        http://127.0.0.1:8080/
# Terminal client: bomberust-cli join 127.0.0.1:7878
```

Open the address in a browser, type a name and press Join. The first person to join is the lobby
owner and can press Enter (or the Start button) to begin; the Add bot and Remove bot buttons fill
the other slots.

The lobby shows one tall card per slot: a cubic isometric character that bobs and turns every
few seconds (`bomberust_portrait3d` in the WebAssembly module), a coloured `PLAYER n` banner, the
player name below and small marks for you, the owner and ready. Empty slots are plain cards. The
animation runs only while the lobby is on screen and the tab is visible, and it is a still image
when the system asks for reduced motion.

Sound effects and music are generated in the page; the speaker button opens the volume settings.
See [AUDIO.md](AUDIO.md).

## Developer mode

`--dev` (also `bomberust-cli host --dev`) turns on developer mode for testing. The page then shows
a "DEV MODE" marker and a panel for the lobby owner: pause or resume every bot (placed bombs keep
burning, people are not affected), seven sliders from 0 to 100 % for the drop chance of each power
(extra bomb, flame, roller, punch, slow, hyper speed, small bomb) and a reset button. The sliders
change what blocks destroyed from then on drop, also in the middle of a match; for example punch
at 100 % and the rest at 0 % makes every block drop a punch. Without `--dev` the server refuses
every developer command (`DevDisabled`) and the page shows no panel. Developer mode is for testing
on your own machine: it prints a `WARNING:` at start-up and must not be exposed to an untrusted
network, see [SECURITY.md](SECURITY.md).

## Options

<!-- options:begin -->
| Option | Meaning |
|--------|---------|
| `--bind <IP>` | address to listen on (default 127.0.0.1) |
| `--http-port <N>` | web page and WebSocket port (default 8080) |
| `--game-port <N>` | terminal client port (default 7878) |
| `--bots <N>` | bots to add to the lobby, 0-3 (default 0) |
| `--seed <U64>` | fixed RNG seed for every match (default: random) |
| `--allow-origin <ORIGIN>` | accept WebSocket connections from this web origin (repeatable) |
| `--allow-any-origin` | accept WebSocket connections from any web origin (unsafe) |
| `--max-connections <N>` | open HTTP/WebSocket connections (default 64, minimum 4) |
| `--dev` | developer mode: owner-only commands to pause bots and set drop chances (testing only) |
| `-h, --help` | show this help |
| `-V, --version` | show the version |
<!-- options:end -->

The table above is checked against the program's `--help` text by `cargo test -p bomberust-web
--test docs`; if you change an option, the test prints the new table to paste here.

Examples:

```sh
# only this machine, one bot waiting in the lobby
bomberust-server --bots 1

# a LAN party: reachable by everybody on the network (prints a WARNING, see SECURITY.md)
bomberust-server --bind 0.0.0.0 --http-port 8080 --game-port 7878

# the page is served from another origin (CORS): allow exactly that origin
bomberust-server --allow-origin http://localhost:3000

# reproducible matches, for tests and bug reports
bomberust-server --seed 42
```

Exit codes: `0` after Ctrl+C (every client is told the host left), `1` on a runtime error such as a
port in use (the message names the port), `2` for a usage error.

The terminal host can serve the same page: `bomberust-cli host --web-port 8080` shares its match
with browsers (`--bind` sets the address; see the options of `bomberust-cli --help`):

```sh
bomberust-cli host --bots 1 --web-port 8080
bomberust-cli join http://192.168.1.20:8080/     # web addresses work too; only the host is kept
```

## Routes and headers

| Request | Answer |
|---------|--------|
| `GET /`, `GET /index.html` | `200 text/html` - the page |
| `GET /app.js` | `200 text/javascript` - the page logic (ES module) |
| `GET /look.js` | `200 text/javascript` - the character looks: options, editor state and storage |
| `GET /audio.js` | `200 text/javascript` - generated sound effects, music and volume settings |
| `GET /style.css` | `200 text/css` |
| `GET /bomberust_render.wasm` | `200 application/wasm` - the graphics module |
| `GET /ws` with `Upgrade: websocket` | `101` - see [PROTOCOL.md](PROTOCOL.md) |
| `HEAD` of the routes above | the same headers, no body |
| any other path | `404` |
| any other method | `405` with `Allow: GET, HEAD` |
| a request head over 8 KiB | `431`, connection closed |
| more than `--max-connections` open | `503` |

Every static response carries `Cache-Control: no-cache`, `X-Content-Type-Options: nosniff`,
`Referrer-Policy: no-referrer` and a Content Security Policy that allows only the page's own
files, WebAssembly and WebSocket connections (`default-src 'self'; script-src 'self'
'wasm-unsafe-eval'; connect-src 'self' ws: wss:`). The assets are compiled into the binary, so the
server needs no files at run time.

## Origin and CORS policy

A browser sends an `Origin` header with a WebSocket handshake. The server accepts the connection
only when the origin is the server's own, one given with `--allow-origin`, or any origin with
`--allow-any-origin`. A different origin gets `403` and never reaches the match. Both options print
a CORS warning when the server starts. This is what keeps a hostile web page from driving a game
running on your machine. Details, and what the game does not protect (there is no encryption and
no authentication), are in [SECURITY.md](SECURITY.md).

When the page and the server share an origin (the normal case) there is nothing to configure.

## The page

| File | Role |
|------|------|
| [`index.html`](../crates/web/assets/index.html) | structure: join form, lobby list, canvas, HUD, notices; no inline script |
| [`look.js`](../crates/web/assets/look.js), [`audio.js`](../crates/web/assets/audio.js) | ES modules imported by `app.js`: the look options, editor state and storage; the generated sound and settings |
| [`app.js`](../crates/web/assets/app.js) | an ES module: the WebSocket client and its state machine (`reduce`), keyboard tracking, DOM updates, and painting of the draw commands on the canvas |
| [`style.css`](../crates/web/assets/style.css) | layout, the grid background, the menu card and the four character colours |
| [`bomberust_render.wasm`](../crates/web/assets/bomberust_render.wasm) | the top-down pixel-art view, built from `crates/web-render` |

**Character editor.** On the join screen (and from "Change look" in the lobby) the player picks a
colour, a gender look, a head accessory and a face accessory with a live preview drawn by the
WebAssembly module. The choice is saved on the device (`localStorage`, key `bomberust.look.v1`),
sent with `Join`, and changed in the lobby with `SetLook`; the server settles colour conflicts (see
[PROTOCOL.md](PROTOCOL.md#looks)). During a match the editor is locked.

The page never evaluates game rules. It sends the keys currently held as `Input` messages every
50 ms and draws the `Snapshot`s it receives. Keys: arrows or WASD to move, Space or E for a bomb, Z or Q to punch;
all keys are released when the window loses focus. Messages from the server show up as notices
(`Only Ann can start the match`, `Player 3 left, a bot took over`, `You won!`), and when the
connection drops the page says so and offers a Retry button.

## The WebAssembly module

The module has no imports and no JavaScript glue. `app.js` copies the latest `Observation` (as
JSON) into its memory, asks for a frame and paints what comes back:

| Export | Meaning |
|--------|---------|
| `memory` | linear memory shared with the page |
| `bomberust_alloc(len) -> ptr` | reserve `len` bytes (at most 1 MiB; 0 on failure) |
| `bomberust_free(ptr, len)` | release a block |
| `bomberust_load(ptr, len) -> code` | parse an `Observation`; `0` ok, `1` not UTF-8, `2` invalid JSON, `3` arena too large |
| `bomberust_frame(width, height, alpha) -> count` | build the draw commands for a canvas; `alpha` interpolates between the last two ticks |
| `bomberust_output_ptr() -> ptr` | where the commands are |
| `bomberust_stride() -> 16` | floats per command |
| `bomberust_pick_tile(x, y) -> index` | the tile under a canvas point, or `-1` |
| `bomberust_portrait(slot, width, height, rows) -> count` | commands of one character for the lobby cards and the menu: the whole figure for `rows` 0, otherwise its first `rows` rows zoomed |

A command is 16 `f32`: `[kind, r, g, b, a, x0, y0, x1, y1, x2, y2, x3, y3, size, text, layer]`
with kind `0` quad, `1` circle, `2` one character of text, `3` triangle. Commands arrive sorted
back to front; the page paints them in order. The page checks `bomberust_stride() === 16` at
start-up and reports a mismatch instead of drawing garbage.

### Rebuilding the module

The compiled module is committed, so running the game needs no WebAssembly tools. After you
change `crates/web-render`:

```sh
rustup target add wasm32-unknown-unknown   # once
./scripts/build-wasm.sh                     # builds and copies crates/web/assets/bomberust_render.wasm
cargo build -p bomberust-web                # the server embeds the new module
```

Commit the rebuilt `.wasm` together with the source change. CI rebuilds it to make sure the
artifact still builds.

## Debugging in the browser

- Open the developer tools (F12). The console must stay clean while you play; the smoke script
  `node scripts/browser-smoke.mjs <url> <out-dir>` drives a headless Chromium through join, start
  and a few key presses, saves screenshots and prints the console output.
- The Network tab, filtered by `WS`, shows every message in both directions.
- Start the server with a fixed `--seed` and `--bots 1` to get the same arena every time.
- If the canvas stays empty, check for a failed `bomberust_render.wasm` request or a stride error
  message in the page.

## Tests

```sh
cargo test -p bomberust-web            # routes, origin policy, WebSocket, mixed matches, docs
cargo test -p bomberust-web-render     # view fitting, sprites, draw order, commands, FFI
cd crates/web/js-tests && node --test  # helpers of app.js and the compiled wasm module
```
