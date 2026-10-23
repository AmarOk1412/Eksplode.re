# Bomberust

Four-player Bomberman in Rust. Play in a terminal or in a web page, add bots, and let browser and
terminal players share the same match. The rules, the bots, the network protocol and the
pixel-art view are all small, tested crates.

## Features

- Four players per match: any mix of people (terminal or browser) and bots.
- **Web page** with a top-down pixel-art view: four characters (blue, black, red, purple),
  grey pillars, red-brick blocks, bombs and flames, a round timer and character cards in the
  lobby, all drawn by a Rust module compiled to WebAssembly.
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
cargo run -p bomberust-web -- --bots 1                    # 2. start the server with one bot
# 3. open http://127.0.0.1:8080/ in a browser, type a name, press Join, then Start
cargo run -p bomberust-cli -- join 127.0.0.1              # 4. optional: a terminal player in the same match
cargo test --workspace                                    # 5. run the tests
```

The first person to join is the lobby owner and starts the match (Enter or the Start button).
Prefer to host from the terminal? `cargo run -p bomberust-cli -- host --bots 1 --web-port 8080`
does the same and lets browsers join too.

Screenshots are not committed yet; to see the game, run the steps above (or
`node scripts/browser-smoke.mjs http://127.0.0.1:8080/ /tmp/shots` with a server running to
capture PNGs).

## Modes

| Mode | Command | What it is |
|------|---------|-----------|
| Local | `cargo run -p bomberust-cli -- --players 2 --bots 2` | up to four people share one keyboard, bots fill the rest |
| Local with a lobby | `cargo run -p bomberust-cli -- --lobby` | pick the slots on a lobby screen first |
| Terminal host | `cargo run -p bomberust-cli -- host --bots 1` | host a match on port 7878 and play in it |
| Terminal host with the page | `cargo run -p bomberust-cli -- host --web-port 8080` | the same match, joinable from browsers |
| Server | `cargo run -p bomberust-web -- --bots 1` | headless host plus the web page; nobody has to sit at it |
| Join | `cargo run -p bomberust-cli -- join 192.168.1.20` | a terminal player joins a host or server |

### Terminal options

`bomberust-cli [OPTIONS]` plays on this machine:

- `--players <N>` local human players, 0 to 4 (default 1);
- `--bots <N>` bots, 0 to 4 (default: fill up to 4 participants);
- `--seed <U64>` fixes the random generator;
- `--lobby` shows the lobby screen first (Up/Down select, `h` human, `b` bot, Enter start);
- `-h`, `--help` and `-V`, `--version`.

`bomberust-cli host [--port N] [--bots N] [--name NAME] [--seed N] [--web-port N] [--bind IP]`:

- `--port <N>` game port (default 7878), `--bots <N>` bots from 0 to 3, `--name <NAME>` your
  name (1 to 16 printable characters), `--seed <N>` fixed seed;
- `--web-port <N>` also serve the web page on this port;
- `--bind <IP>` listening address. By default the game port listens on every interface and the
  page only on `127.0.0.1`; a non-loopback address prints a `WARNING:`. See
  [docs/SECURITY.md](docs/SECURITY.md).

`bomberust-cli join <ADDR[:PORT]> [--name NAME]` joins a host. `host`, `host:port` and web
addresses such as `http://host:8080/` work; for a web address only the host is kept. A refused
or full match ends with a one-line explanation and exit status 1.

The terminal must be at least 40x20. Wrong usage exits with status 2.

### Server options

`bomberust-server` (`cargo run -p bomberust-web --`) takes `--bind`, `--http-port`,
`--game-port`, `--bots`, `--seed`, `--allow-origin`, `--allow-any-origin` and
`--max-connections`; the table and examples are in [docs/WEB.md](docs/WEB.md).

## Controls

| Where | Move | Bomb | Punch |
|-------|------|------|-------|
| Browser | arrows or WASD | Space or E | Z or Q |
| Terminal P1 | arrow keys | Space | Z |
| Terminal P2 | W A S D | E | C |
| Terminal P3 | I J K L | O | U |
| Terminal P4 | T F G H | Y | N |

In the lobby: Enter starts (owner only), `b` adds a bot, `x` removes one, `r` toggles ready, `q`
quits. In a local match `p` pauses and `r` starts a rematch after a round. The terminal reports
no key releases, so a direction key moves one tile per tap and keeps moving while the key
repeats. In the browser the game keeps moving while a key is held.

## Rules in short

Bombs explode after 3 s in a cross, stop at walls and destroy the first block, trigger other
bombs they reach, and eliminate every player on a burning tile. Blocks may hide power-ups:
extra bomb, roller (faster), flame (longer range) and a rare punch for bombs, but also maluses
(slow, hyper speed, small bomb) that last 10 s. See [docs/GAMEPLAY.md](docs/GAMEPLAY.md). The last player alive wins; a round
without a survivor, or lasting more than 3 minutes, is a draw. With a single player the round
is a practice round that ends when that player dies.

## Bots

Bots take any slot that is not a person. They see exactly what a human sees (the public
observation) and act through the same input type. Check them with:

```sh
cargo test -p bomberust-bots
cargo test -p bomberust-bots --test soak -- --nocapture   # 100 four-bot matches + escape rate
```

## Network play and safety

The host is authoritative: it runs the simulation, validates every message and sends each
player its own view 20 times per second. If a player disconnects or stays silent for 10 s, a
bot takes the slot and the match goes on; if the host leaves, everybody is told.

**There is no encryption and no authentication.** The server listens on `127.0.0.1` by default;
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
| `crates/web` | `bomberust-server`, the HTTP and WebSocket server, and the page (`assets/`) |
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
