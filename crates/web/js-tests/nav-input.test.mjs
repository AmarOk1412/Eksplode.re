import test from "node:test";
import assert from "node:assert/strict";
import { NavTracker, navigationMode } from "../assets/nav.js";

function pad({ buttons = [], axes = [0, 0] } = {}) {
  return {
    connected: true,
    mapping: "standard",
    buttons: Array.from({ length: 16 }, (_, index) => ({ pressed: buttons.includes(index) })),
    axes,
  };
}

test("D-pad and left stick directions repeat at 400 ms and then every 120 ms", () => {
  const tracker = new NavTracker();
  assert.equal(tracker.sample(pad({ buttons: [15] }), 0).direction, "right");
  assert.equal(tracker.sample(pad({ buttons: [15] }), 399).direction, null);
  assert.equal(tracker.sample(pad({ buttons: [15] }), 400).direction, "right");
  assert.equal(tracker.sample(pad({ buttons: [15] }), 519).direction, null);
  assert.equal(tracker.sample(pad({ buttons: [15] }), 520).direction, "right");
  tracker.sample(pad(), 530);
  assert.equal(tracker.sample(pad({ axes: [-0.5, 0] }), 540).direction, "left");
});

test("stick hysteresis releases at 0.3 and buttons are one-shot", () => {
  const tracker = new NavTracker();
  assert.equal(tracker.sample(pad({ axes: [0.5, 0] }), 0).direction, "right");
  assert.equal(tracker.sample(pad({ axes: [0.4, 0] }), 10).direction, null);
  assert.equal(tracker.sample(pad({ axes: [0.29, 0] }), 20).direction, null);
  assert.deepEqual(tracker.sample(pad({ buttons: [0, 1, 9] }), 30), { direction: null, confirm: true, back: true, menu: true });
  assert.deepEqual(tracker.sample(pad({ buttons: [0, 1, 9] }), 40), { direction: null, confirm: false, back: false, menu: false });
});

test("context changes suppress held buttons until released; absent and short pads are neutral", () => {
  const tracker = new NavTracker();
  tracker.contextChanged();
  assert.equal(tracker.sample(pad({ buttons: [0, 12] }), 0).confirm, false);
  assert.deepEqual(tracker.sample(pad({ buttons: [0, 12] }), 10), { direction: null, confirm: false, back: false, menu: false });
  tracker.sample(pad(), 20);
  assert.equal(tracker.sample(pad({ buttons: [0] }), 30).confirm, true);
  assert.deepEqual(tracker.sample(null, 40), { direction: null, confirm: false, back: false, menu: false });
  assert.deepEqual(tracker.sample({ connected: true, mapping: "standard", buttons: [], axes: [] }, 50), { direction: null, confirm: false, back: false, menu: false });
});

test("navigation takes precedence except in active play without an open layer", () => {
  assert.equal(navigationMode("playing", false), "gameplay");
  for (const stage of ["idle", "joining", "lobby", "closed"]) assert.equal(navigationMode(stage, false), "navigation");
  assert.equal(navigationMode("playing", true), "navigation");
});
