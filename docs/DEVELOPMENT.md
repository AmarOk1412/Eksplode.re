# Development

## Prerequisites

| Tool | Needed for |
|------|-----------|
| Rust (stable or nightly, edition 2021) with `rustfmt` and `clippy` | everything |
| `rustup target add wasm32-unknown-unknown` | rebuilding the WebAssembly module only |
| Node.js 20 or newer | `node --test` for the page, and the browser smoke script |
| Python 3 | `scripts/pty-enter-check.py` (the pseudo-terminal check of Enter) |
| A Chromium-based browser | the optional smoke script `scripts/browser-smoke.mjs` |

Playing the game needs only Rust: the compiled `eksplode_re_render.wasm` is committed.

## Build and run

```sh
cargo build --workspace
cargo run -p eksplode-re-cli -- --players 1 --bots 3     # local game in the terminal
cargo run -p eksplode-re-web -- --bots 1                 # server and web page
cargo run -p eksplode-re-cli -- join 127.0.0.1           # a terminal player on that server
```

## Test

```sh
cargo test --workspace                       # all Rust tests, including the docs tests
cd crates/web/js-tests && node --test        # page helpers and the compiled wasm module
python3 scripts/pty-enter-check.py           # Enter in a real pseudo-terminal (add --attempts 20)
node scripts/browser-smoke.mjs http://127.0.0.1:8080/ /tmp/smoke   # a server must be running
scripts/check-scenarios.sh                   # every acceptance scenario has a test or a manual check
```

Run `node --test` from inside `crates/web/js-tests`; passing the directory from the repository
root does not work with Node 22. Net and web tests join their clients one after another because
slots are handed out in arrival order.

Acceptance scenarios are named in test names: `us1_as3_...` for the web client and `f3_us1_as3_...`
for customization, powers, sound and developer mode (the `f3_` prefix keeps the two numberings
apart). Scenarios that need eyes or ears are ticked in a manual checklist next to the specs
(`- [x] f3_us1_as3 ...`); `F3_ONLY="f3_us1 f3_us2" scripts/check-scenarios.sh` checks only the stages
that are already built.

Game modes and room settings use the prefix `f4_` (`f4_us2_as3_...`), the manual checklist
`manual-004.md` next to the specs, and `F4_ONLY="f4_us1 f4_us2_as1"` for the same purpose. The
terminal host asks for the room settings on a setup screen unless a settings option or
`--no-setup` is given, so `scripts/pty-enter-check.py` and other scripts pass `--no-setup`.

## Gates

Everything below must pass before a change is merged (CI runs the same):

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/build-wasm.sh
(cd crates/web/js-tests && node --test)
python3 scripts/pty-enter-check.py
scripts/check-scenarios.sh
```

Library code must not panic on external input (network bytes, HTTP heads, WebAssembly lengths);
`unwrap` and `expect` are warnings in library code and allowed in tests. `unsafe` is forbidden
except in `crates/web-render/src/ffi.rs`.

## Commits

Enable the hook once per clone:

```sh
git config core.hooksPath .githooks
```

A subject is `part: short desc` (at most 80 characters) followed by a body that explains the
change:

```text
web: add http server, origin policy and websocket bridge

long description to explain the feature
on multiple lines
```

Parts: `core`, `bots`, `net`, `cli`, `web`, `render`, `build`, `docs`, `test`. Commit small, working
steps; tests go in the same commit as the code they cover (or the commit before it).

## Repository layout

```text
Cargo.toml          workspace and the wasm-release profile
README.md           what it is and how to start
docs/               this documentation
scripts/            build-wasm.sh, pty-enter-check.py, browser-smoke.mjs, check-scenarios.sh
.githooks/          the commit message check
crates/core         rules
crates/bots         computer players
crates/net          protocol, host, client
crates/cli          terminal front end
crates/web          server, page (assets/), JS tests (js-tests/)
crates/web-render   pixel-art view compiled to WebAssembly
```

## How to add things

**A rule.** Write a failing test in `crates/core/tests` that shows the behaviour with a
hand-built state, then change `core` until it passes. Keep `step` deterministic: no clocks and
no random numbers except through the seeded generator in the state. If the rule changes what a
participant must see, add it to `Observation` and check bots, the terminal view and the render
module.

**A bot behaviour.** Add a scenario to `crates/bots/tests/behaviour.rs`, then extend
`DefaultBot` (or implement the `Bot` trait). A bot receives only an `Observation`. Run
`cargo test -p eksplode-re-bots --test soak -- --nocapture` to see that matches still finish and
that bots survive bombs they drop.

**A protocol message.** Add the variant to `crates/net/src/protocol.rs`, handle it in
`Host`, add loopback tests in `crates/net/tests` and WebSocket tests in `crates/web/tests`, and
describe it in [PROTOCOL.md](PROTOCOL.md) (a test fails until every variant is documented).

**A page feature.** Add the state change to `reduce` in `crates/web/assets/app.js` and a test in
`crates/web/js-tests/app.test.mjs`, then render it in the DOM or the canvas. Keep the page free of
inline scripts (the Content Security Policy forbids them) and free of game rules. Check the
result in a browser or with the smoke script, and record manual checks in a checklist.

**A power.** Add the kind to `PowerUpKind` (and `ALL`, `is_malus`), its pickup effect in
`crates/core/src/step.rs`, its share in `drops::DEFAULT_SHARES`, then core tests (`powerups`,
`malus`, `drops`). Bots score it in `crates/bots`, the terminal needs a letter in `view.rs`, the
render module a 10x10 pixel icon in `sprites.rs` (`icon`) and a letter in `commands.rs`, the
page a label in `DROP_LABELS` and the guide [GAMEPLAY.md](GAMEPLAY.md) a table row; the drop table
(`DEFAULT_SHARES`, `DevSetDrops`, the dev panel) grows by one entry at the end. Rebuild the wasm.

**A sprite pose.** Views and frames of the characters live in `look::grid_view` (front, back,
left; right is mirrored) and the choice of view and frame in `Interp::pose` and `depth.rs`; test
them in `crates/web-render/tests/sprites3.rs`.

**A look option.** Add the name to `crates/net/src/look.rs` and `crates/web-render/src/look.rs`
(the wire names must match), the pixels or boxes in `look.rs` and `iso3d.rs`, the option in
`crates/web/assets/look.js`, and tests in `crates/net/tests/look.rs` and
`crates/web-render/tests/look.rs`.

**A sound.** Add the id to `SOUND_IDS` and its recipe to `RECIPES` in
`crates/web/assets/audio.js`, map the event in `soundsForEvents`, test it in
`crates/web/js-tests/sounds.test.mjs` and add a row with its recipe to [AUDIO.md](AUDIO.md) (the
docs test checks it). Never add an audio file.

**A server option.** Add it to `crates/web/src/args.rs` (and its `USAGE` text), test it in
`crates/web/tests/args.rs`, and paste the table that `cargo test -p eksplode-re-web --test docs`
prints into [WEB.md](WEB.md).

## Release checklist

1. `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`, `node --test` in `crates/web/js-tests`.
2. If `crates/web-render` changed: `./scripts/build-wasm.sh`, and commit the rebuilt
   `crates/web/assets/eksplode_re_render.wasm` (it must stay under 300 KB).
3. `cargo build -p eksplode-re-web`, start the server and play a round in a browser and a terminal.
4. `python3 scripts/pty-enter-check.py --attempts 20`.
5. Update the README and the guides for anything user-visible; the docs tests must pass.
6. Check the history: `git log --format=%s | awk 'length>80'` prints nothing.

## Measured results

| Criterion | Result | Where it is checked |
|-----------|--------|---------------------|
| Key press to the snapshot that shows it, p95, two browsers and two terminals at 20 Hz | about 100 ms (limit 200 ms) | `cargo test -p eksplode-re-web --test mixed sc003 -- --nocapture` |
| WebAssembly module size | 213 KB (limit 300 KB) | `scripts/build-wasm.sh` fails above the limit; `sc005_the_whole_page_stays_small` |
| Whole page (HTML, JS, CSS, wasm) | 293 KB (limit 500 KB) | `sc005_the_whole_page_stays_small` in `crates/web/tests/http.rs` |
| Cross-origin WebSocket upgrades refused by default | 100 of 100 | `sc007_cross_origin_upgrades_are_refused_by_default` |
| Enter starts a match in a real pseudo-terminal | 20 of 20 for each Enter variant | `python3 scripts/pty-enter-check.py --attempts 20` |

On the large 25 x 19 board with a full room the same kind of test gives a p95 of about 65 ms
(`f4_sc010_key_to_snapshot_latency_on_the_large_board`, limit 200 ms).

With the powers of feature 003 the same latency test gives a p95 of about 100 ms. Sixty frames per
second in the animated lobby and in a match is checked by eye (SC-010).

Smooth drawing is not machine-measured: the page paints once per `requestAnimationFrame` and
interpolates between the 20 Hz snapshots; check it by eye with a running server.
