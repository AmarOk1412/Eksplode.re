import test from "node:test";
import assert from "node:assert/strict";
import { KeyTracker } from "../assets/app.js";
import { DEFAULT_BINDINGS } from "../assets/controls.js";

test("f13_us1_as3_kart_keys_keep_horizontal_intent_and_cancel_opposites", () => {
  const keys = new KeyTracker();
  keys.press("ArrowLeft");
  keys.press("ArrowRight");
  assert.equal(keys.take(true).mv, null);
  keys.release("ArrowLeft");
  keys.press("ArrowUp");
  assert.equal(keys.take(true).mv, "Right");
  keys.clear();
  assert.equal(keys.take(true).mv, null);
});

test("f13_us1_as1_kart_is_not_only_a_dead_hud_row", async () => {
  const { hudRows } = await import("../assets/app.js");
  const rows = hudRows({ slot: 0, lobby: { slots: [{controller: "Local"}] }, observation: {
    config: { tick_hz: 20 }, players: [{alive: false, kart: {index: 0}, bombs_active: 0, bombs_max: 1, range: 2, move_cooldown: 4}]
  }});
  assert.equal(rows[0].kart, true);
});

test("f13_us2_as3_stun_is_visible_without_replacing_other_effects", async () => {
  const { hudRows } = await import("../assets/app.js");
  const rows = hudRows({slot: 0, lobby: {slots: [{controller: "Local"}]}, observation: {
    config: {tick_hz: 20}, players: [{alive: true, immobilized_left: 20, has_punch: true, bombs_active: 0, bombs_max: 1, range: 2, move_cooldown: 4}]
  }});
  assert.ok(rows[0].effects.includes("stunned 1s"));
  assert.ok(rows[0].effects.includes("punch"));
});

test("f13_us1_as3_opposing_devices_are_neutral_before_wire_reduction", async () => {
  const { mergeGameplayInputs } = await import("../assets/app.js");
  assert.equal(mergeGameplayInputs([{mv: "Left"}, {mv: "Right", bomb: true}], true).mv, null);
  assert.equal(mergeGameplayInputs([{mv: "Left"}, {mv: "Right", bomb: true}], true).bomb, true);
  assert.equal(mergeGameplayInputs([{mv: "Left"}, {mv: "Right"}], false).mv, "Left");
});

test("f13_us4_as5_a_human_kart_is_still_participating_in_the_match", async () => {
  const { hubActions } = await import("../assets/app.js");
  const state = {stage: "playing", lobby: {slots: [{controller: "Remote"}, {controller: "Bot"}]}, observation: {players: [{alive: false, kart: {index: 0}}, {alive: true}]}};
  assert.equal(hubActions(state).lobby.enabled, false);
  state.observation.players[0].kart = null;
  assert.equal(hubActions(state).lobby.enabled, true);
});

test("us1_as4_arrow_keys_and_space", () => {
  const k = new KeyTracker();
  k.press("ArrowLeft");
  assert.deepEqual(k.take(), { mv: "Left", bomb: false, punch: false });
  k.press("Space");
  assert.deepEqual(k.take(), { mv: "Left", bomb: true, punch: false });
  k.release("ArrowLeft");
  assert.deepEqual(k.take(), { mv: null, bomb: false, punch: false });
});

test("us1_as4_wasd_and_e", () => {
  const k = new KeyTracker();
  for (const [code, dir] of [
    ["KeyW", "Up"],
    ["KeyS", "Down"],
    ["KeyA", "Left"],
    ["KeyD", "Right"],
  ]) {
    k.press(code);
    assert.equal(k.take().mv, dir);
    k.release(code);
  }
  k.press("KeyE");
  assert.equal(k.take().bomb, true);
});

test("us1_as4_the_last_pressed_direction_wins_and_falls_back_on_release", () => {
  const k = new KeyTracker();
  k.press("ArrowUp");
  k.press("ArrowRight");
  assert.equal(k.take().mv, "Right");
  k.release("ArrowRight");
  assert.equal(k.take().mv, "Up", "the still held key takes over");
  k.release("ArrowUp");
  assert.equal(k.take().mv, null);
});

test("us1_as4_a_bomb_press_is_one_shot_even_if_released_before_the_next_tick", () => {
  const k = new KeyTracker();
  k.press("Space");
  k.release("Space");
  assert.equal(k.take().bomb, true);
  assert.equal(k.take().bomb, false);
  k.press("Space");
  k.press("Space"); // key repeat does not queue a second bomb
  assert.equal(k.take().bomb, true);
  assert.equal(k.take().bomb, false);
});

test("us1_as4_blur_clears_every_key", () => {
  const k = new KeyTracker();
  k.press("ArrowDown");
  k.press("Space");
  k.clear();
  assert.deepEqual(k.take(), { mv: null, bomb: false, punch: false });
});

test("unknown keys are ignored and releasing an unpressed key is harmless", () => {
  const k = new KeyTracker();
  k.press("KeyX");
  k.press("Enter");
  k.release("ArrowUp");
  assert.deepEqual(k.take(), { mv: null, bomb: false, punch: false });
});

test("key repeat of a held direction keeps it", () => {
  const k = new KeyTracker();
  k.press("ArrowDown");
  k.press("ArrowDown");
  k.press("ArrowDown");
  assert.equal(k.take().mv, "Down");
  k.release("ArrowDown");
  assert.equal(k.take().mv, null);
});

test("custom bindings replace the default direction while preserving one-shot actions", () => {
  const bindings = { ...DEFAULT_BINDINGS, up: ["KeyI"], bomb: ["KeyB"] };
  const tracker = new KeyTracker(bindings);
  tracker.press("KeyI");
  tracker.press("KeyB");
  assert.deepEqual(tracker.take(), { mv: "Up", bomb: true, punch: false });
  assert.deepEqual(tracker.take(), { mv: "Up", bomb: false, punch: false });
  tracker.release("KeyI");
  assert.equal(tracker.take().mv, null);
});

test("f3_us2_as9_punch_keys_fire_once_and_clear_on_release_and_blur", () => {
  for (const code of ["KeyZ", "KeyQ"]) {
    const k = new KeyTracker();
    k.press(code);
    assert.equal(k.take().punch, true, code);
    assert.equal(k.take().punch, false, "a punch fires once");
    k.press(code);
    k.release(code);
    assert.equal(k.take().punch, true, "a quick tap is not lost");
    k.press(code);
    k.clear();
    assert.equal(k.take().punch, false, "blur clears it");
  }
  const k = new KeyTracker();
  k.press("KeyZ");
  assert.deepEqual(k.take(), { mv: null, bomb: false, punch: true });
});
