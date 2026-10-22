# Bomberust

Four-player Bomberman in Rust with bots, a terminal version, network play and an isometric
graphical version.

## Build

```sh
cargo build --workspace
cargo test --workspace
```

## Play in the terminal

```sh
cargo run -p bomberust-cli -- --players 1 --bots 3 --seed 42
cargo run -p bomberust-cli -- --lobby
cargo run -p bomberust-cli -- --help
```

The terminal must be at least 40x20. Options: `--players N` (local humans, 0-4, default 1),
`--bots N` (0-4, default: fill up to 4 participants), `--seed N`, `--lobby` (pick the slots on a
lobby screen first: Up/Down select, `h` human, `b` bot, Enter start). `players + bots` must be 1
to 4; wrong usage exits with code 2.

## Bots

Bots take any slot that is not a human. They see exactly what a human sees (the public
observation) and act through the same input type. A bot flees fire (including chain reactions),
picks up power-ups, hunts opponents and digs through blocks, and only drops a bomb when it has a
way to escape. Check them with:

```sh
cargo test -p bomberust-bots
cargo test -p bomberust-bots --test soak -- --nocapture   # 100 four-bot matches + escape rate
```

Controls (up to four players share one keyboard; the terminal reports no key releases, so a
direction key moves one tile per tap and keeps moving while the key repeats):

| Player | Move | Bomb |
|--------|------|------|
| P1 | arrow keys | Space |
| P2 | W A S D | E |
| P3 | I J K L | O |
| P4 | T F G H | Y |

`p` pauses, `q` quits, `r` starts a rematch after a round ended.

Rules in short: bombs explode after 3 s in a cross, stop at walls and destroy the first block,
trigger other bombs they reach, and eliminate every player on a burning tile. Blocks may hide
power-ups: `+B` extra bomb, `+R` longer range, `+S` faster movement. The last player alive wins;
a round without a survivor, or lasting more than 3 minutes, is a draw. With a single player the
round is a practice round that ends when that player dies.

## Host / join

Up to four participants (humans and bots) can play over a network. One person hosts, the others
join by address. The host is authoritative: it runs the simulation, validates every message and
sends each client its own view 20 times per second.

```sh
cargo run -p bomberust-cli -- host --bots 1          # listens on port 7878
cargo run -p bomberust-cli -- join 192.168.1.20      # default port 7878
cargo run -p bomberust-cli -- join 192.168.1.20:9000 --name Ann
```

Host options: `--port N`, `--bots N` (0-3), `--name NAME`, `--seed N`. Names are 1 to 16 printable
characters. In the lobby the host uses `b` (add bot), `x` (remove a bot) and `Enter` (start);
clients toggle `r` (ready). In a match the keys are the P1 keys (arrows + Space) or P2 keys
(WASD + E). If a player disconnects or stays silent for 10 s, a bot takes the slot and the match
goes on; if the host leaves, everybody is told. After a round the lobby returns for a rematch.

The wire protocol (newline-delimited JSON, port 7878, version 1) is described in
`specs/001-bomberman-multiplayer/contracts/network-protocol.md` of the planning workspace.
There is no encryption or authentication: play on a trusted network. Malformed or oversized
messages are rejected and a connection that sends ten bad messages is closed.

```sh
cargo test -p bomberust-net      # codec, lobby rules, loopback matches, latency and agreement
```

## Contributing

Enable the commit hook once per clone:

```sh
git config core.hooksPath .githooks
```

Commit subjects follow `part: short desc` (at most 80 characters) followed by a body:

```text
part: short desc (80 chars)

long description to explain the feature
on multiple lines
```

Parts: `core`, `bots`, `net`, `cli`, `gui`, `build`, `docs`, `test`.

Before merging: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`.
