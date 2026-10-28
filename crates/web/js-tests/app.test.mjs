import test from "node:test";
import assert from "node:assert/strict";
import { initialState, reduce, rejectText, lobbyOwner, hudRows, inputMessage, timerText, devPanel, resultRows, punchPowerActive } from "../assets/app.js";

const lobby = (...controllers) => ({
  slots: Array.from({ length: 4 }, (_, i) => ({
    controller: controllers[i] ?? "Empty",
    name: controllers[i] ? `p${i}` : "",
    ready: false,
  })),
});

const welcome = (slot, l = lobby("Remote")) => ({ type: "Welcome", slot, config: {}, lobby: l });

function run(events, start = initialState()) {
  return events.reduce(reduce, start);
}

test("the page starts idle and walks through the join flow", () => {
  let s = initialState();
  assert.equal(s.stage, "idle");
  s = reduce(s, { type: "connecting" });
  assert.equal(s.stage, "connecting");
  s = reduce(s, { type: "socket-open" });
  assert.equal(s.stage, "joining");
  s = reduce(s, welcome(0));
  assert.equal(s.stage, "lobby");
  assert.equal(s.slot, 0);
  s = reduce(s, { type: "Start", seed: 1, config: {} });
  assert.equal(s.stage, "playing");
  s = reduce(s, { type: "Snapshot", tick: 5, state: { tick: 5 }, events: [] });
  assert.equal(s.tick, 5);
  assert.deepEqual(s.observation, { tick: 5 });
});

test("us1_as2_refusals_are_explained_in_plain_language", () => {
  for (const [reason, word] of [
    ["Full", "full"],
    ["BadName", "name"],
    ["InMatch", "running"],
    ["BadVersion", "version"],
  ]) {
    const s = run([{ type: "connecting" }, { type: "socket-open" }, { type: "Rejected", reason }]);
    assert.equal(s.stage, "closed", reason);
    assert.ok(s.notice.toLowerCase().includes(word), `${reason}: ${s.notice}`);
    assert.equal(s.notice, rejectText(reason));
  }
});

test("us1_as6_a_closed_socket_shows_connection_lost_and_retry_starts_over", () => {
  let s = run([{ type: "connecting" }, { type: "socket-open" }, welcome(1), { type: "socket-closed" }]);
  assert.equal(s.stage, "closed");
  assert.match(s.notice, /connection lost/i);
  s = reduce(s, { type: "retry" });
  assert.equal(s.stage, "idle");
  assert.equal(s.notice, "");
  assert.equal(s.slot, null);
});

test("us1_as6_host_left_says_the_server_is_gone and keeps that message when the socket closes", () => {
  let s = run([welcome(0), { type: "Error", code: "HostLeft", message: "the host closed the match" }]);
  assert.equal(s.stage, "closed");
  assert.match(s.notice, /server (stopped|is gone)/i);
  const before = s.notice;
  s = reduce(s, { type: "socket-closed" });
  assert.equal(s.notice, before);
});

test("other server errors are shown but do not end the session", () => {
  const s = run([welcome(1), { type: "Error", code: "NotOwner", message: "Only Ann can start the match" }]);
  assert.equal(s.stage, "lobby");
  assert.equal(s.notice, "Only Ann can start the match");
});

test("us2_as3_a_takeover_is_announced_and_play_continues", () => {
  let s = run([welcome(0), { type: "Start", seed: 1, config: {} }]);
  s = reduce(s, { type: "PlayerLeft", slot: 2, replaced_by_bot: true });
  assert.equal(s.notice, "Player 3 left, a bot took over");
  assert.equal(s.stage, "playing");
  s = reduce(s, { type: "Snapshot", tick: 9, state: { tick: 9 }, events: [] });
  assert.equal(s.tick, 9);
});

test("the result is announced and waits for an explicit lobby return", () => {
  let s = run([welcome(1), { type: "Start", seed: 1, config: {} }]);
  assert.equal(reduce(s, { type: "Ended", outcome: { Winner: 1 } }).notice, "You won!");
  assert.equal(reduce(s, { type: "Ended", outcome: { Winner: 3 } }).notice, "Player 4 won");
  const end = reduce(s, { type: "Ended", outcome: "Draw" });
  assert.equal(end.notice, "Draw");
  assert.equal(end.stage, "results");
  assert.equal(reduce(end, { type: "return-to-lobby" }).stage, "lobby");
});

test("us2_as9_punch_control_is_visible_only_while_the_local_player_has_punch", () => {
  const state = { slot: 1, observation: { players: [{ has_punch: true }, { has_punch: false }] } };
  assert.equal(punchPowerActive(state), false);
  state.observation.players[1].has_punch = true;
  assert.equal(punchPowerActive(state), true);
  state.observation = null;
  assert.equal(punchPowerActive(state), false);
});

test("us3_as1_round_result_keeps_final_standings_until_return_to_lobby", () => {
  const players = [0, 1].map((i) => ({
    alive: i === 1,
    bombs_active: 0,
    bombs_max: 1 + i,
    range: 2 + i,
    move_cooldown: 4,
    has_punch: i === 1,
    has_kick: false,
    malus: null,
  }));
  const slots = lobby("Remote", "Remote");
  slots.slots[0].name = "Ann";
  slots.slots[1].name = "Bea";
  const started = run([
    welcome(0, slots),
    { type: "Snapshot", tick: 40, state: { tick: 40, config: { base_move_cooldown: 4, tick_hz: 20 }, players }, events: [] },
    { type: "Ended", outcome: { Winner: 1 } },
  ]);
  assert.equal(started.stage, "results");
  assert.equal(started.notice, "Player 2 won");
  assert.deepEqual(resultRows(started).map(({ label, standing }) => [label, standing]), [
    ["Ann", "Eliminated"],
    ["Bea", "Winner"],
  ]);
  assert.equal(resultRows(started)[1].range, 3);
  assert.deepEqual(resultRows(started)[1].effects, ["punch"]);

  const updated = reduce(started, { type: "Lobby", lobby: slots });
  assert.equal(updated.stage, "results", "the server lobby update must not hide the result");
  assert.equal(reduce(updated, { type: "return-to-lobby" }).stage, "lobby");
  assert.equal(reduce(updated, { type: "return-to-lobby" }).result, null);
});

test("us3_as2_results_describe_draw_team_and_chaser_outcomes", () => {
  const slots = lobby("Remote", "Remote", "Remote", "Remote");
  const players = [
    { alive: true, team: 0, role: "Normal" },
    { alive: false, team: 0, role: "Normal" },
    { alive: true, team: 1, role: "Normal" },
    { alive: false, team: 1, role: "Normal" },
  ];
  const start = { ...initialState(), stage: "playing", slot: 0, lobby: slots, observation: { players, config: {} } };
  assert.deepEqual(resultRows(reduce(start, { type: "Ended", outcome: "Draw" })).map((row) => row.standing), ["Draw", "Draw", "Draw", "Draw"]);
  assert.deepEqual(resultRows(reduce(start, { type: "Ended", outcome: { TeamWin: 1 } })).map((row) => row.standing), ["Survived", "Eliminated", "Winner", "Winner"]);
  const chaserPlayers = players.map((player, index) => ({ ...player, role: index === 0 ? "Chaser" : "Runner" }));
  const chaserStart = { ...start, observation: { players: chaserPlayers, config: {} } };
  assert.equal(resultRows(reduce(chaserStart, { type: "Ended", outcome: "ChaserWins" }))[0].standing, "Winner");
  assert.equal(resultRows(reduce(chaserStart, { type: "Ended", outcome: "RunnersWin" }))[2].standing, "Winner");
});

test("a lobby message during play does not leave the game", () => {
  let s = run([welcome(0), { type: "Start", seed: 1, config: {} }]);
  s = reduce(s, { type: "Lobby", lobby: lobby("Remote", "Bot") });
  assert.equal(s.stage, "playing");
  assert.equal(s.lobby.slots[1].controller, "Bot");
});

test("the owner is the lowest human slot", () => {
  assert.equal(lobbyOwner(lobby("Bot", "Remote", "Local")), 1);
  assert.equal(lobbyOwner(lobby("Bot")), null);
  assert.equal(lobbyOwner(null), null);
  const s = run([welcome(1, lobby("Remote", "Remote"))]);
  assert.equal(s.slot === lobbyOwner(s.lobby), false);
});

test("us2_as4_owner_name_is_available_for_not_owner_hints", () => {
  const l = lobby("Remote", "Remote");
  l.slots[0].name = "Ann";
  assert.equal(l.slots[lobbyOwner(l)].name, "Ann");
});

test("the hud lists the participants with their stats", () => {
  const players = [0, 1, 2, 3].map((i) => ({
    alive: i !== 2,
    bombs_active: 0,
    bombs_max: 1 + i,
    range: 2,
    move_cooldown: 4,
  }));
  const s = run([welcome(1, lobby("Remote", "Remote", "Bot")), { type: "Snapshot", tick: 1, state: { players, config: { base_move_cooldown: 4 } }, events: [] }]);
  const rows = hudRows(s);
  assert.equal(rows.length, 3, "empty slots are not listed");
  assert.deepEqual(rows[1], { slot: 1, kind: "you", label: "You", alive: true, bombs: "0/2", range: 2, speed: 1, effects: [], team: null, role: null, hearts: 0, shield: false, you: true });
  assert.equal(rows[2].label, "Bot");
  assert.equal(rows[2].kind, "bot");
  assert.equal(rows[2].alive, false);
});

test("f3_us2_as9_the_hud_lists_active_effects_with_seconds_left", () => {
  const players = [0, 1, 2].map(() => ({ alive: true, bombs_active: 0, bombs_max: 1, range: 2, move_cooldown: 4 }));
  players[1].has_punch = true;
  players[1].malus = { kind: "Slow", ticks_left: 7 * 20 - 3 };
  players[2].malus = { kind: "HyperSpeed", ticks_left: 80 };
  const s = run([
    welcome(1, lobby("Remote", "Remote", "Bot")),
    { type: "Snapshot", tick: 1, state: { players, config: { base_move_cooldown: 4, tick_hz: 20 } }, events: [] },
  ]);
  const rows = hudRows(s);
  assert.deepEqual(rows[0].effects, []);
  assert.deepEqual(rows[1].effects, ["punch", "slow 7s"]);
  assert.deepEqual(rows[2].effects, ["hyper 4s"]);
  const small = { ...s, observation: { ...s.observation, players: players.map((p, i) => (i === 2 ? { ...p, malus: { kind: "SmallBomb", ticks_left: 161 } } : p)) } };
  assert.deepEqual(hudRows(small)[2].effects, ["small 9s"]);
});

test("us2_as3_other_human_players_are_listed_by_name_and_kind", () => {
  const players = [0, 1].map(() => ({ alive: true, bombs_active: 0, bombs_max: 1, range: 2, move_cooldown: 4 }));
  const s = run([welcome(1, lobby("Remote", "Remote")), { type: "Snapshot", tick: 1, state: { players }, events: [] }]);
  const rows = hudRows(s);
  assert.equal(rows[0].kind, "player");
  assert.equal(rows[0].label, "p0 (player)");
});

test("us2_as4_a_full_match_is_explained_and_not_owner_names_the_owner", () => {
  const s = run([{ type: "connecting" }, { type: "socket-open" }, { type: "Rejected", reason: "Full" }]);
  assert.match(s.notice, /full/i);
  const l = lobby("Remote", "Remote");
  l.slots[0].name = "Ann";
  const msg = `Only ${l.slots[lobbyOwner(l)].name} can start the match`;
  assert.equal(run([welcome(1, l), { type: "Error", code: "NotOwner", message: msg }]).notice, msg);
});

test("input messages carry the latest tick and the key state", () => {
  const s = { tick: 12 };
  assert.deepEqual(inputMessage(s, { mv: "Left", bomb: true, punch: false }), { type: "Input", tick: 12, mv: "Left", bomb: true, punch: false });
  assert.deepEqual(inputMessage(s, { mv: null, bomb: false, punch: true }), { type: "Input", tick: 12, mv: null, bomb: false, punch: true });
  assert.equal(inputMessage(s, { mv: null, bomb: false }).punch, false, "a missing punch means false");
});

test("the timer shows the time left as minutes and seconds", () => {
  const obs = (tick) => ({ tick, config: { round_limit_ticks: 3600, tick_hz: 20 } });
  assert.equal(timerText(obs(0)), "03:00");
  assert.equal(timerText(obs(60)), "02:57");
  assert.equal(timerText(obs(3599)), "00:01");
  assert.equal(timerText(obs(3600)), "00:00");
  assert.equal(timerText(obs(9999)), "00:00");
});

test("the timer is empty without a snapshot or a limit", () => {
  assert.equal(timerText(null), "");
  assert.equal(timerText({ tick: 1, config: {} }), "");
  assert.equal(timerText({ config: { round_limit_ticks: 10, tick_hz: 20 } }), "");
});

import { patternBanner } from "../assets/app.js";

test("f4_us2_as6_the_banner_appears_when_the_pattern_starts_and_the_clock_says_closing", () => {
  assert.equal(patternBanner(["PatternStarted"]), "The walls close in!");
  assert.equal(patternBanner([{ Moved: {} }, "PatternStarted"]), "The walls close in!");
  assert.equal(patternBanner([{ BlockWarned: { pos: { x: 1, y: 1 } } }]), "");
  assert.equal(patternBanner(undefined), "");
  assert.equal(patternBanner("PatternStarted"), "");

  const config = { round_limit_ticks: 3600, closing_ticks: 1200, tick_hz: 20, mode: "normal" };
  assert.equal(timerText({ tick: 2399, config }), "01:01");
  assert.equal(timerText({ tick: 2400, config }), "CLOSING");
  assert.equal(timerText({ tick: 5000, config }), "CLOSING", "past the limit while two live");
  assert.equal(timerText({ tick: 2400, config: { ...config, closing_ticks: 0 } }), "01:00", "no pattern, no word");
  assert.equal(timerText({ tick: 2400, config: { ...config, mode: "chaser" } }), "01:00");
});

test("f4_us3_as5_the_result_names_the_winning_team_and_its_members", () => {
  const lobby = {
    slots: [
      { controller: "Remote", name: "Ann" },
      { controller: "Bot", name: "" },
      { controller: "Remote", name: "Cy" },
      { controller: "Remote", name: "Di" },
    ],
    settings: { mode: "team" },
  };
  const base = { ...initialState(), stage: "playing", slot: 2, lobby };
  const a = reduce(base, { type: "Ended", outcome: { TeamWin: 0 } });
  assert.equal(a.stage, "results");
  assert.equal(a.notice, "Team A wins (Ann, Bot 2)");
  const b = reduce(base, { type: "Ended", outcome: { TeamWin: 1 } });
  assert.equal(b.notice, "Team B wins (Cy, Di) - your team!");
  assert.equal(reduce(base, { type: "Ended", outcome: "Draw" }).notice, "Draw");
  assert.equal(reduce(base, { type: "Ended", outcome: { Winner: 2 } }).notice, "You won!");
});

test("f4_us3_as5_status_rows_carry_the_team", () => {
  const players = [0, 1, 2, 3].map((i) => ({ alive: true, bombs_active: 0, bombs_max: 1, range: 2, move_cooldown: 4, team: i < 2 ? 0 : 1 }));
  const state = {
    ...initialState(),
    slot: 0,
    lobby: { slots: [0, 1, 2, 3].map((i) => ({ controller: i === 0 ? "Remote" : "Bot", name: "Ann" })) },
    observation: { players, config: { base_move_cooldown: 4, tick_hz: 20 } },
  };
  const rows = hudRows(state);
  assert.deepEqual(rows.map((r) => r.team), ["A", "A", "B", "B"]);
  const none = hudRows({ ...state, observation: { ...state.observation, players: players.map((p) => ({ ...p, team: undefined })) } });
  assert.ok(none.every((r) => r.team === null));
});

test("f4_us4_as9_the_hud_shows_roles_and_hearts_and_the_result_says_who_won", () => {
  const players = [0, 1, 2, 3].map((i) => ({
    alive: true,
    bombs_active: 0,
    bombs_max: i === 2 ? 2 : 0,
    range: 2,
    move_cooldown: 4,
    role: i === 2 ? "Chaser" : "Runner",
    hearts: i === 2 ? 2 : 1,
    shield_left: i === 2 ? 10 : 0,
  }));
  const lobby = {
    slots: [0, 1, 2, 3].map((i) => ({ controller: i === 0 ? "Remote" : "Bot", name: "Ann" })),
    settings: { mode: "chaser" },
  };
  const state = { ...initialState(), slot: 0, lobby, observation: { players, config: { base_move_cooldown: 4, tick_hz: 20, mode: "chaser" } } };
  const rows = hudRows(state);
  assert.deepEqual(rows.map((r) => r.role), ["runner", "runner", "chaser", "runner"]);
  assert.equal(rows[2].hearts, 2);
  assert.equal(rows[2].shield, true);
  assert.equal(rows[0].hearts, 0, "runners show no hearts");

  const playing = { ...state, stage: "playing" };
  assert.equal(reduce(playing, { type: "Ended", outcome: "ChaserWins" }).notice, "The chaser wins (Bot 3)");
  assert.equal(reduce(playing, { type: "Ended", outcome: "RunnersWin" }).notice, "You survived! The runners win");
  assert.equal(reduce({ ...playing, slot: 2 }, { type: "Ended", outcome: "RunnersWin" }).notice, "The runners win");
  const chaserMe = { ...playing, slot: 2 };
  assert.equal(reduce(chaserMe, { type: "Ended", outcome: "ChaserWins" }).notice, "You won! The chaser wins");
});

test("f4_us4_dev_panel_says_the_drop_sliders_have_no_effect_in_chaser_mode", () => {
  const lobby = { slots: [{ controller: "Remote", name: "Ann" }, { controller: "Empty" }, { controller: "Empty" }, { controller: "Empty" }], settings: { mode: "chaser" } };
  const state = { ...initialState(), dev: true, slot: 0, lobby, devState: { paused: false, drops: [0.1, 0, 0, 0, 0, 0, 0, 0] } };
  const panel = devPanel(state);
  assert.equal(panel.noDropEffect, true);
  assert.match(panel.hint, /no effect/i);
  assert.equal(panel.enabled, true, "pausing bots still works");
  const normal = devPanel({ ...state, lobby: { ...lobby, settings: { mode: "normal" } } });
  assert.equal(normal.noDropEffect, false);
  assert.doesNotMatch(normal.hint, /no effect/i);
});
