# Eksplode.re

[![CI](https://github.com/AmarOk1412/Eksplode.re/actions/workflows/ci.yml/badge.svg)](https://github.com/AmarOk1412/Eksplode.re/actions/workflows/ci.yml)

Note: This is a "Toy project" fully sloppy generated.

Four-player Bomberman in Rust. Play in a terminal or in a web page, add bots, and let browser and
terminal players share the same match. The rules, the bots, the network protocol and the
pixel-art view are all small, tested crates.

## Features

- Four players per match: any mix of people (terminal or browser) and bots.
- **Rooms and modes:** the owner of a room chooses the mode (Normal, Team, Chaser), the board size (small,
  medium, large), the round time (2, 3, 5 or 10 minutes) and, for Team, friendly fire, on a creation
  menu (page) or setup screen (terminal). In Normal and Team rounds the board closes in during the
  last minute until one player or team is left; see [docs/GAMEPLAY.md](docs/GAMEPLAY.md).
- **Web page** with a top-down pixel-art view: four characters (blue, black, red, purple),
  grey pillars, red-brick blocks, bombs and flames, a round timer and tall lobby cards with
  animated isometric 3D characters, all drawn by a Rust module compiled to WebAssembly. The page
  opens on a main menu (Create new game, Join a game, Options); Options has Customize, Sound and
  Controls tabs, and Customize picks colour, head, face and costume (or randomizes them with the
  icon on the preview). Empty lobby seats fill with bots: the owner removes a bot or kicks a
  player with the cross on a seat, anybody puts a bot back with "+", and room settings and the
  password live behind the gear icon.
- **Terminal version** for local play on one keyboard, with a lobby screen.
- **Network play:** an authoritative host, `join` by address, or a server that hosts the match
  and serves the web page.
- Bots that flee flames, collect power-ups, hunt opponents and dig through blocks.
- Deterministic rules: the same seed and inputs always give the same match.
- Safe defaults: loopback bind, same-origin WebSockets, hard limits on all external input.

## Quick start

You need [Rust](https://rustup.rs/). The WebAssembly module is committed, so nothing else is
required.

```sh
cargo build --workspace                                   # 1. build everything
cargo run -p eksplode-re-web -- --bind 127.0.0.1 --bots 1  # 2. start a local server
# 3. open http://127.0.0.1:8080/, create a room, enter your name, then Start
cargo run -p eksplode-re-cli -- join 127.0.0.1              # 4. select the room in the terminal directory
cargo test --workspace                                    # 5. run the tests
```

The creator owns their room and starts its match (Enter or the Start button). Other players
select that room from the directory; each room manages its own four participants and settings.
Prefer to host from the terminal? `cargo run -p eksplode-re-cli -- host --bots 1 --web-port 8080`
does the same and lets browsers join too.

Screenshots are not committed yet; to see the game, run the steps above (or
`node scripts/browser-smoke.mjs http://0.0.0.0:8080/ /tmp/shots` with a server running to
capture PNGs).

## Modes

| Mode | Command | What it is |
|------|---------|-----------|
| Terminal host | `cargo run -p eksplode-re-cli` | the default: host a match on port 7878 and play in it |
| Terminal host (explicit) | `cargo run -p eksplode-re-cli -- host --bots 1` | same as above, named explicitly, with options |
| Local | `cargo run -p eksplode-re-cli -- local --players 2 --bots 2` | up to four people share one keyboard, bots fill the rest; shows the lobby screen first |
| Local, instant start | `cargo run -p eksplode-re-cli -- local --instant` | skip the lobby screen and start right away |
| Terminal host with the page | `cargo run -p eksplode-re-cli -- host --web-port 8080` | the same match, joinable from browsers |
| Server | `cargo run -p eksplode-re-web -- --bots 1` | headless host plus the web page; nobody has to sit at it |
| Join | `cargo run -p eksplode-re-cli -- join 192.168.1.20` | a terminal player joins a host or server |

### Terminal options

`eksplode-re-cli [OPTIONS]` with no subcommand defaults to `host` (see below) so a bare run is
ready for others to join. `eksplode-re-cli local [OPTIONS]` plays on this machine only, no network:

- `--players <N>` local human players, 0 to 4 (default 1);
- `--bots <N>` bots, 0 to 4 (default: fill up to 4 participants);
- `--seed <U64>` fixes the random generator;
- `--lobby` shows the lobby screen first (Up/Down select, `h` human, `b` bot, Enter start) —
  this is the default; `--instant` skips it and starts the match immediately instead;
- `--mode <normal|team|chaser>`, `--size <small|medium|large>`, `--time <2|3|5|10>` (minutes) and
  `--friendly-fire <on|off>` set the room settings of the match (defaults: normal, small, 3, on);
- `--p1-keys`, `--p2-keys`, `--p3-keys` and `--p4-keys` configure each local player's six keys in
  `up,down,left,right,bomb,punch` order; for example, `--p1-keys "w,s,a,d,e,z"`. Values accept
  single characters or `Up`, `Down`, `Left`, `Right`, `Space`. Omitted slots keep their defaults;
  duplicate keys and `q` (quit), `p` (pause) or `r` (rematch) are rejected;
- `-h`, `--help` and `-V`, `--version`.

`eksplode-re-cli host [--port N] [--bots N] [--name NAME] [--seed N] [--web-port N] [--bind IP] [--dev]
[--room-name TEXT] [--mode M] [--size S] [--time MIN] [--friendly-fire on|off] [--no-setup]`
(the `host` keyword is optional — this is also what a bare `eksplode-re-cli` runs):

- `--port <N>` game port (default 7878), `--bots <N>` bots from 0 to 3, `--name <NAME>` your
  name (1 to 16 printable characters), `--seed <N>` fixed seed;
- `--web-port <N>` also serve the web page on this port;
- `--server-id <ID>` stable ASCII host identifier, 1 to 64 characters; `--max-rooms <N>` limits
  simultaneous rooms to 1-128 (default 32), and `--max-connections <N>` sets the shared host
  connection cap to 4-1024 (default 256);
- `--room-name <TEXT>` names the room (at most 24 characters), and `--mode`, `--size`, `--time` and
  `--friendly-fire` set it as above; without any of them the host first shows a room setup screen
  (Up/Down select, Left/Right change, type the name, Enter on "Create room"), which `--no-setup`
  skips. In the lobby the owner changes the settings with `m` mode, `s` size, `t` time and `f`
  friendly fire; everybody sees them;
- `--dev` developer mode (testing only): the owner can pause bots and set drop chances from the
  web page; see [docs/WEB.md](docs/WEB.md);
- `--bind <IP>` listening address. By default the game port listens on every interface and the
  page only on `0.0.0.0`; a non-loopback address prints a `WARNING:`. See
  [docs/SECURITY.md](docs/SECURITY.md).

`eksplode-re-cli join <ADDR[:PORT]> [--name NAME] [--keys KEYS] [--room-id ID]` opens a host's
directory without joining a room. Select a room or press `c` to create one; `--room-id` explicitly
selects a known 32-character lowercase hexadecimal room ID on that host. `--keys` uses the
same six-key order for the joining player only. `host`, `host:port` and web addresses such as
`http://host:8080/` work; for a web address only the host is kept. A refused or full match ends
with a one-line explanation and exit status 1.

The terminal must be at least 40x20; a medium board needs 40x22 and a large one 52x26 (the game says so when the terminal is too small). Wrong usage exits with status 2.

### Server options

`eksplode-re-server` (`cargo run -p eksplode-re-web --`) takes `--bind`, `--http-port`,
`--game-port`, `--bots`, `--seed`, `--allow-origin`, `--allow-any-origin`,
`--max-connections`, `--max-rooms`, `--server-id`, `--dev` and the room defaults of the creation menu (`--room-name`, `--mode`,
`--size`, `--time`, `--friendly-fire`); the table and examples are in [docs/WEB.md](docs/WEB.md).

## Controls

| Where | Move | Bomb | Punch |
|-------|------|------|-------|
| Browser keyboard | arrows or WASD | Space or E | Z or Q |
| Mobile browser (portrait) | bottom-left joystick | bottom-right Bomb | bottom-right Punch, when powered |
| Mobile browser (landscape) | existing horizontal layout | existing action layout | Punch only when powered |
| Xbox-compatible controller | left stick or D-pad | A | B |

On a touch-capable phone, including Firefox for Android, portrait play uses a joystick at the bottom left and bomb/power actions at the bottom right. Punch appears only while your player has the Punch power. If you rotate to landscape, the existing horizontal control layout remains in use. The game does not force an orientation change.

After a round, the scoreboard ranks the players by survival (the winner or last survivor first, then the others in reverse order of elimination; ties by kills, then slot) and shows only Player, Kills and Bombs placed. It remains until Return to Lobby is selected.

Every browser screen is also keyboard/controller navigable: Tab and Shift+Tab move focus, arrows move between choices, Enter or Space activates, and Escape opens or backs out of the menu hub.
| Terminal P1 | arrow keys | Space | Z |
| Terminal P2 | W A S D | E | C |
| Terminal P3 | I J K L | O | U |
| Terminal P4 | T F G H | Y | N |

In the lobby: Enter starts (owner only), `b` adds a bot, `x` removes one, `r` toggles ready, `q`
quits. In a local match `p` pauses and `r` starts a rematch after a round. The terminal reports
no key releases, so a direction key moves one tile per tap and keeps moving while the key
repeats. In the browser the game keeps moving while a key is held.

Open the keyboard icon in the browser header to reassign any action, see the active bindings or
restore defaults. A reassignment replaces that action's aliases and is saved on the same device.
Conflicting and reserved keys are refused. For a controller, focus the visible game page and press
a button or move a stick if it is not listed yet; choose among listed controllers in the panel.
Movement uses the left stick or D-pad, A places a bomb and B punches a bomb. Keyboard input remains
available when a controller is disconnected or unsupported. See [docs/WEB.md](docs/WEB.md) for
browser setup details.

Every browser screen is also keyboard/controller navigable: Tab and Shift+Tab move focus, arrows
move between choices, Enter or Space activates, and Escape opens or backs out of the menu hub.
Controller D-pad or left stick moves focus, A activates, B goes back, and Start opens or closes the
hub. Activate a focused text field with A to enter text through the on-screen keyboard; its B or
Cancel action restores the original text. The hub provides Sound, Controls, Resume and, in developer
mode, Developer. Navigation panels suppress match input until they close. See
[docs/WEB.md](docs/WEB.md) for screen-by-screen details.

## Rules in short

Bombs explode after 3 s in a cross, stop at walls and destroy the first block, trigger other
bombs they reach, and eliminate every player on a burning tile. Blocks may hide power-ups:
extra bomb, roller (faster), flame (longer range), a rare punch and a kick that pushes bombs
until they hit something, but also maluses (slow, hyper speed, small bomb) that last 10 s. Every
power has a pixel-art icon and players turn and walk as they move. See [docs/GAMEPLAY.md](docs/GAMEPLAY.md). The web page has generated sound effects and music with a
volume panel (no audio files; see [docs/AUDIO.md](docs/AUDIO.md)). The last player alive wins; a round
without a survivor, or lasting more than 3 minutes, is a draw. With a single player the round
is a practice round that ends when that player dies.

## Bots

Bots take any slot that is not a person. They see exactly what a human sees (the public
observation) and act through the same input type. Check them with:

```sh
cargo test -p eksplode-re-bots
cargo test -p eksplode-re-bots --test soak -- --nocapture   # 100 four-bot matches + escape rate
```

## Network play and safety

The host is authoritative: it runs the simulation, validates every message and sends each
player its own view 20 times per second. If a player disconnects or stays silent for 10 s, a
bot takes the slot and the match goes on; if the host leaves, everybody is told.

**There is no encryption and no authentication.** The server listens on `0.0.0.0` by default;
only expose it on a network you trust, or behind a firewall, an SSH tunnel or a TLS reverse
proxy. WebSocket connections from other web origins are refused unless you opt in with
`--allow-origin` (or the unsafe `--allow-any-origin`). Messages over 64 KiB are refused and a
connection that sends ten bad messages is closed. The details are in
[docs/SECURITY.md](docs/SECURITY.md) and the wire format in [docs/PROTOCOL.md](docs/PROTOCOL.md).

## Repository map

| Path | Contents |
|------|----------|
| `crates/core` | pure rules: arena, bombs, step, lobby, observation |
| `crates/bots` | the default bot, danger map and path finding |
| `crates/net` | wire protocol, authoritative host, client |
| `crates/cli` | the terminal game, `host` and `join` |
| `crates/web` | `eksplode-re-server`, the HTTP and WebSocket server, and the page (`assets/`) |
| `crates/web-render` | the pixel-art view, compiled to WebAssembly |
| `docs/` | the guides below |
| `scripts/` | `build-wasm.sh`, `pty-enter-check.py`, `browser-smoke.mjs`, `check-scenarios.sh` |

## Documentation

- [Architecture](docs/ARCHITECTURE.md): how the parts fit together and how a key press travels.
- [Protocol](docs/PROTOCOL.md): every message, error code and limit.
- [Web server and page](docs/WEB.md): options, routes, origin policy, the WebAssembly module.
- [Security](docs/SECURITY.md): what is and is not protected, and how to host safely.
- [Development](docs/DEVELOPMENT.md): build, test, gates, commits and how to extend the game.

## Contributing

Enable the commit hook once per clone: `git config core.hooksPath .githooks`. Commit subjects
follow `part: short desc` (at most 80 characters) followed by a body that explains the change.
Parts: `core`, `bots`, `net`, `cli`, `web`, `render`, `build`, `docs`, `test`. Before merging run
`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` and
`cargo test --workspace`; see [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for the full list.
