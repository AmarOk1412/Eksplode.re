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
cargo run -p bomberust-cli -- --players 1 --seed 42
cargo run -p bomberust-cli -- --help
```

The terminal must be at least 40x20. Options: `--players N` (local humans, 0-4, default 1),
`--bots N` (0-4, default 0), `--seed N`. `players + bots` must be 1 to 4; wrong usage exits with
code 2.

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

(added with network play)

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
