# Bomberust

Four-player Bomberman in Rust with bots, a terminal version, network play and an isometric
graphical version.

## Build

```sh
cargo build --workspace
cargo test --workspace
```

## Controls

(added with the CLI)

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
