import test from "node:test";
import assert from "node:assert/strict";
import {
  DROP_KINDS,
  chancesFromPercents,
  devMessages,
  devPanel,
  initialState,
  percentsFromChances,
  reduce,
} from "../assets/app.js";

const slot = (controller, name = "") => ({ controller, name, ready: false });
const lobby = { slots: [slot("Remote", "Ann"), slot("Remote", "Bob"), slot("Empty"), slot("Empty")] };
const welcome = (mySlot, dev) => ({ type: "Welcome", slot: mySlot, config: {}, lobby, dev });
const DEFAULTS = [0.057, 0.066, 0.066, 0.009, 0.03, 0.024, 0.033, 0.015];

test("the reducer keeps dev and the dev state", () => {
  assert.equal(initialState().dev, false);
  assert.equal(initialState().devState, null);
  let s = reduce(initialState(), welcome(0, true));
  assert.equal(s.dev, true);
  s = reduce(s, { type: "DevState", paused: true, drops: DEFAULTS });
  assert.deepEqual(s.devState, { paused: true, drops: DEFAULTS });
  s = reduce(s, { type: "DevState", paused: false, drops: [1, 0, 0, 0, 0, 0, 0] });
  assert.deepEqual(s.devState, { paused: false, drops: [1, 0, 0, 0, 0, 0, 0] });
  assert.equal(reduce(initialState(), welcome(0, undefined)).dev, false, "an old host means no dev");
  assert.equal(reduce(initialState(), welcome(0, false)).dev, false);
  assert.equal(reduce(s, { type: "retry" }).dev, false, "a new session forgets it");
});

test("f3_us4_as1_the_panel_is_offered_only_in_dev_mode", () => {
  assert.equal(devPanel(reduce(initialState(), welcome(0, false))), null);
  assert.equal(devPanel(initialState()), null);
  const panel = devPanel(reduce(initialState(), welcome(0, true)));
  assert.ok(panel);
  assert.equal(panel.marker, "DEV MODE");
});

test("the panel offers pause or resume, seven sliders in power order and a reset", () => {
  let s = reduce(initialState(), welcome(0, true));
  s = reduce(s, { type: "DevState", paused: false, drops: DEFAULTS });
  let panel = devPanel(s);
  assert.equal(panel.pause.label, "Pause bots");
  assert.equal(panel.pause.paused, false);
  assert.deepEqual(
    panel.sliders.map((x) => x.kind),
    ["ExtraBomb", "Range", "Speed", "Punch", "Slow", "HyperSpeed", "SmallBomb", "Kick"],
  );
  assert.deepEqual(DROP_KINDS, panel.sliders.map((x) => x.kind));
  assert.deepEqual(
    panel.sliders.map((x) => x.percent),
    [6, 7, 7, 1, 3, 2, 3, 2],
  );
  assert.ok(panel.sliders.every((x) => typeof x.label === "string" && x.label.length > 0));
  assert.deepEqual(
    panel.sliders.map((x) => x.label),
    ["Extra bomb", "Flame", "Roller", "Punch", "Slow", "Hyper speed", "Small bomb", "Kick"],
  );
  assert.equal(panel.reset.label, "Reset drops");
  s = reduce(s, { type: "DevState", paused: true, drops: DEFAULTS });
  panel = devPanel(s);
  assert.equal(panel.pause.label, "Resume bots");
  assert.equal(panel.pause.paused, true);
});

test("a panel without a dev state yet shows zeroes and still works", () => {
  const panel = devPanel(reduce(initialState(), welcome(0, true)));
  assert.equal(panel.sliders.length, 8);
  assert.ok(panel.sliders.every((x) => x.percent === 0));
});

test("slider values map to chances between 0.0 and 1.0", () => {
  assert.deepEqual(chancesFromPercents([0, 50, 100, 25, 10, 1, 75]), [0, 0.5, 1, 0.25, 0.1, 0.01, 0.75, 0]);
  assert.deepEqual(chancesFromPercents([-5, 150, Number.NaN, "x", undefined, null, 33.4]).slice(0, 6), [0, 1, 0, 0, 0, 0]);
  assert.equal(chancesFromPercents([0, 0, 0, 0, 0, 0, 0, 100]).length, 8);
  assert.deepEqual(percentsFromChances([0, 0.5, 1, 0.25, 0.1, 0.01, 0.756]), [0, 50, 100, 25, 10, 1, 76, 0]);
  assert.deepEqual(percentsFromChances(undefined), [0, 0, 0, 0, 0, 0, 0, 0]);
  assert.deepEqual(percentsFromChances([2, -1, Number.NaN]).slice(0, 3), [100, 0, 0]);
});

test("only the owner can use the panel", () => {
  const asOwner = devPanel(reduce(initialState(), welcome(0, true)));
  const asGuest = devPanel(reduce(initialState(), welcome(1, true)));
  assert.equal(asOwner.enabled, true);
  assert.equal(asGuest.enabled, false, "a guest sees the panel but cannot use it");
  assert.match(asGuest.hint, /owner/i);
});

test("the messages follow the protocol", () => {
  const m = devMessages();
  assert.deepEqual(m.pause(true), { type: "DevPauseBots", paused: true });
  assert.deepEqual(m.pause(false), { type: "DevPauseBots", paused: false });
  assert.deepEqual(m.reset(), { type: "DevResetDrops" });
  assert.deepEqual(m.drops([0, 0, 0, 100, 0, 0, 0, 0]), { type: "DevSetDrops", chances: [0, 0, 0, 1, 0, 0, 0, 0] });
});
