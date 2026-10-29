# Changelog

All notable changes to this project are documented in this file.
## v0.0.1-test - 2026-10-29

### Bots

- Bots: add danger map, pathing and default bot (643728a)

- Bots: dodge flying bombs and pick bonuses over maluses (31f0cb5)

- Bots: treat sliding bombs as dangerous (d16ef84)

- Bots: avoid announced squares and move inward (4c05fc3)

- Bots: play for a team (9002f26)

- Bots: play the chaser and runner roles (3aa02fc)


### Build

- Build: initialise workspace, lint gates and commit-msg hook (a059a78)

- Build: add web crates, commit parts and CI steps for the web client (47c9193)

- Build: check scenario ids of features 002 and 003 (9f1798d)

- Build: run the full scenario check in CI (cc16a66)

- Build: check the scenario ids of the amendment (9ed2a3f)

- Build: add CI workflow to validate every push to main (f192465)

- Build: add release workflow with git-cliff changelog (1d6be1f)

- Build: run the project's full test suite in both workflows (5dbaaa8)

- Build: fix empty release notes and missing CHANGELOG.md commit (90bbc77)


### CLI

- Cli: add terminal game for local players (c25311f)

- Cli: add lobby screen and local bot slots (0ad6ee7)

- Cli: add host and join modes for network play (d7a77fe)

- Cli: normalise Enter keys, let the owner start and show refusals (22ba386)

- Cli: host with the web page, bind option and clearer join errors (af482fa)

- Cli: punch keys, power letters and effect status (b4d0c6f)

- Cli: add host --dev (49ab580)

- Cli: show the kick, sliding bombs and the kick in the status line (f3cb1e1)

- Cli: add room options, the setup screen and lobby keys (ab05edf)

- Cli: show the too-small message on short lines (9fe7563)

- Cli: show protected room ownership controls (09bc1a3)

- Cli: add masked room password prompt (48806f7)

- Cli: add terminal room capacity and password controls (405722e)


### Core

- Core: add rng, config and basic action types (fef4843)

- Core: implement arena, movement, bombs, chains and round end (9a9932f)

- Core: add lobby slot model and public blast calculation (f8ae0f9)

- Core: add lobby owner and last bot helpers (7936931)

- Core: add Arena::is_valid for untrusted data (84e5fd7)

- Core: add maluses, the punch power and a drop table (b331f45)

- Core: add the kick bonus (af7188a)

- Core: add room settings, mode types and flame owners (7c19396)

- Core: add the end pattern (c178e7d)

- Core: add Team mode and friendly fire (afc3d91)

- Core: add Chaser mode (e650c16)

- Core: rename project Eksplode.re (504f331)

- Core: game controls now support xbox controller (84d31f7)

- Core: remove old files (b51ff30)

- Core: handle mobile players (dc0acb9)

- Core: first room management implem (1d16fea)


### Documentation

- Docs: document CLI controls and rules (4881ad8)

- Docs: rewrite README with quick start and repository map (b7ab0a5)

- Docs: add architecture, protocol, web, security and development guides (7c677df)

- Docs: describe the top-down pixel-art view (7b6a797)

- Docs: describe powers, maluses, the punch and the new keys (875cf2d)

- Docs: describe developer mode (7f3e4df)

- Docs: add how to add a power, a look option and a sound (cf2a029)

- Docs: describe the kick, pixel icons and walking sprites (c019807)

- Docs: complete the guides for rooms and modes (823838b)

- Docs: describe mouse-free game navigation (63ffdf8)

- Docs: add CI status badge to README (2de253e)


### Networking

- Net: add protocol, codec, authoritative host and client (9df8d14)

- Net: add owner commands and in-process connect (034241e)

- Net: add HostHandle for attaching in-process clients (eb0ecc2)

- Net: add looks with colour resolution and SetLook (84cd96d)

- Net: carry the punch input (010c3cb)

- Net: add developer mode with bot pause and live drop table (f91b9cf)

- Net: eight drop chances and the kick on the wire (533abd9)

- Net: add room query, settings and SetRoom (d190581)

- Net: validate room name length after trimming (cf32500)

- Net: reject empty room names in settings updates (a2edb62)

- Net: add private Argon2id password verifier (62e0201)

- Net: bound password work and completion queues (6a51b52)

- Net: protect room admission with bounded password workers (7d697e1)

- Net: support atomic owner password updates (27620d1)

- Net: test stale owner password completions (f122191)

- Net: implement owner RemovePlayer and bounded password rate limiting (82225e8)

- Net: implement bounded JSONL event logging and wire it into real mutations (1575f4e)

- Net: silence fetch_update deprecation to keep MSRV 1.75 (ce3591d)


### Other

- First commit (010aaa0)

- Bomberman: add base player and movements (beda4b3)

- Bomberman: improve bomb behavior (1082de8)

- Bomb: add explode function (c17a821)

- Items: add item images (bc15aaf)

- Game: implement raycasting for bomb explosions (eff0130)

- Item: apply some effects (902d35f)

- Bomb: add animation (7b5e532)

- Item: add some maluses (180d0c0)

- Items: add repel bombs (0a6178c)

- Misc: clean (7375fec)

- Bomberman: add sounds (af5ae23)

- Player: add red sprites (f6fd8e4)

- Item: fix repelBombs (8733f47)

- Item: implement push bombs (5d3cec8)

- Player: add dark player (30cd6a8)

- Player: add player 4 (ddad2fd)

- Player: add blew animation (761ff7c)

- Network: add lobby (b0755cc)

- Lobby: add error window (9455835)

- Lobby: connect to connection_succeeded (29a51a6)

- Lobby: connect leave button (ed86351)

- Player: manage movement via network (e8bc1e5)

- Bomberman: drop bombs via network (691d0a1)

- Settings: camera (6e514a7)

- Lobby: select player (e0a5f78)

- Game: add close button (df83343)

- Network: generate same map everywhere (79216ff)

- Game: end anim + rename player (03e3dba)

- Lobby: improve design (994e47b)

- Lobby: improve (c5bc34d)

- Lobby: start game only if every player are showing lobby (ddc86c3)

- Network: handle server disconnection (37d6825)

- Misc: fix sound in game + bomb sprite offset (0f60d97)

- End: manage objects via an y-sort node (310d3e3)

- Game: fix background (79a86a8)

- Misc: rename Main.tscn->Game.tscn (2a456e2)

- Network: item are now managed by the server (cbc1ef9)

- Sound: play random track when starting a game (c75809c)

- Misc: rename to Eksplode.re (06ad58f)

- Game: spawn items under the game node (ec503a0)

- Game: add multiple end animations (7c39169)

- Game: fix height for first line (1b9d999)

- Game: reset puppet_pos when starting a new game (085a1ee)

- Game: player can't move when game is finished (692c42b)

- Misc: clean collisions with players (3e8ef9d)

- Misc: mv files (98714c1)

- Icon: update (3afa06f)

- Release: add basis for appimage (ef91b6d)

- Misc: improve README.md (68e6ca7)

- Network: manage explosions server side (dd49453)

- Misc: add link to github and credits (ce56800)

- Game: explode through players and items (206d4df)

- Android: add controls (362f8cf)

- Joystick: add support for joysticks (3a0c3e1)

- Players: remove collision between players (5dfe585)


### Rendering

- Render: add isometric projection, depth order, cubes and wasm exports (b3f0396)

- Render: lower walls and blocks so player cubes stay visible (d882c61)

- Render: switch to a top-down pixel-art view with character sprites (ad487ea)

- Render: draw players from their look (cd60d92)

- Render: draw the seven power icons, flights and malus pulse (0c26c93)

- Render: add the semi-3D character for lobby cards (b9de20c)

- Render: draw pixel-art power icons and directional walking sprites (d6c3ba8)

- Render: draw the end pattern in the page and the terminal (51fbaa2)


### Testing

- Test: check scenarios, fuzz inputs, latency and page size (c32f5f2)

- Test: fuzz the new messages and exports, add a mixed-match power test (cfafcb4)

- Test: cover all board sizes and round times (e712e76)

- Test: fuzz, compatibility and determinism for rooms and modes (6955872)


### Web

- Web: add http server, origin policy and websocket bridge (2aec697)

- Web: add the browser page, js tests and smoke script (0c0320b)

- Web: cover mixed browser and terminal matches (fd2995d)

- Web: show other players and takeovers in the page (a83e53a)

- Web: restyle the page with a menu card, character cards and a timer (91a8650)

- Web: serve the look and audio modules (62a94ff)

- Web: add the character editor and remember the look (58d1887)

- Web: punch keys and active effects in the player list (d2a4798)

- Web: show the lobby as 3D character cards (672988e)

- Web: add generated sound effects and music (35b40b2)

- Web: add the sound settings panel (07f9862)

- Web: add --dev to the server (8b80086)

- Web: add the developer panel (53346f3)

- Web: eighth dev slider and the kick sound (ac9b04e)

- Web: add the room creation menu and lobby settings (263f50f)

- Web: show teams in the lobby, the HUD and the match (60fbeef)

- Web: show the chaser, hearts and roles (cc7521c)

- Web: add mouse-free keyboard and controller navigation (bdd6343)

- Web: hide hub when opening settings panels (b27634b)

- Web: hide map focus ring during play (256bb49)

- Web: add protected room controls (fda79d0)


