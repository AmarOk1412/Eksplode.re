import test from "node:test";
import assert from "node:assert/strict";
import {
  CONTROL_STORAGE_KEY,
  DEFAULT_BINDINGS,
  GamepadTracker,
  assignBinding,
  compatibleGamepads,
  loadBindings,
  sanitizeBindings,
  saveBindings,
} from "../assets/controls.js";

const memory = () => {
  const data = new Map();
  return {
    data,
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => data.set(key, value),
  };
};

const gamepad = ({ index = 0, mapping = "standard", buttons = [], axes = [0, 0] } = {}) => ({
  index,
  id: `Xbox-compatible ${index}`,
  connected: true,
  mapping,
  buttons: Array.from({ length: 16 }, (_, button) => ({ pressed: buttons.includes(button) })),
  axes,
});

test("defaults preserve all existing keyboard aliases", () => {
  assert.deepEqual(DEFAULT_BINDINGS, {
    up: ["ArrowUp", "KeyW"],
    down: ["ArrowDown", "KeyS"],
    left: ["ArrowLeft", "KeyA"],
    right: ["ArrowRight", "KeyD"],
    bomb: ["Space", "KeyE"],
    punch: ["KeyZ", "KeyQ"],
  });
});

test("rebind replaces aliases and conflicts or reserved keys preserve the old mapping", () => {
  const original = structuredClone(DEFAULT_BINDINGS);
  const changed = assignBinding(original, "up", "KeyI");
  assert.equal(changed.ok, true);
  assert.deepEqual(changed.bindings.up, ["KeyI"]);

  const conflict = assignBinding(changed.bindings, "down", "KeyI");
  assert.equal(conflict.ok, false);
  assert.equal(conflict.reason, "conflict");
  assert.deepEqual(conflict.bindings, changed.bindings);

  const reserved = assignBinding(changed.bindings, "down", "Escape");
  assert.equal(reserved.ok, false);
  assert.equal(reserved.reason, "reserved");
  assert.deepEqual(reserved.bindings, changed.bindings);
});

test("invalid saved bindings fall back by action and remove cross-action conflicts", () => {
  const bindings = sanitizeBindings({
    up: ["KeyI"],
    down: ["KeyI"],
    left: ["not-a-key"],
  });
  assert.deepEqual(bindings.up, ["KeyI"]);
  assert.deepEqual(bindings.down, DEFAULT_BINDINGS.down);
  assert.deepEqual(bindings.left, DEFAULT_BINDINGS.left);
  assert.deepEqual(sanitizeBindings(null), DEFAULT_BINDINGS);
});

test("malformed saved bindings cannot consume another actions default keys", () => {
  const bindings = sanitizeBindings({
    up: ["ArrowDown", "KeyS"],
    down: ["invalid"],
  });
  assert.deepEqual(bindings.up, DEFAULT_BINDINGS.up);
  assert.deepEqual(bindings.down, DEFAULT_BINDINGS.down);
});

test("versioned preferences load, save, and reset through the provided storage", () => {
  const storage = memory();
  const changed = assignBinding(DEFAULT_BINDINGS, "up", "KeyI").bindings;
  saveBindings(storage, changed);
  assert.equal(CONTROL_STORAGE_KEY, "eksplode.re.controls.v1");
  assert.deepEqual(JSON.parse(storage.data.get(CONTROL_STORAGE_KEY)), {
    version: 1,
    bindings: changed,
  });
  assert.deepEqual(loadBindings(storage), changed);
  storage.data.set(CONTROL_STORAGE_KEY, JSON.stringify({ version: 2, bindings: changed }));
  assert.deepEqual(loadBindings(storage), DEFAULT_BINDINGS);
  storage.data.set(CONTROL_STORAGE_KEY, "{broken");
  assert.deepEqual(loadBindings(storage), DEFAULT_BINDINGS);
});

test("blocked storage never prevents default keyboard play", () => {
  const blocked = {
    getItem() {
      throw new Error("blocked");
    },
    setItem() {
      throw new Error("full");
    },
  };
  assert.deepEqual(loadBindings(blocked), DEFAULT_BINDINGS);
  assert.doesNotThrow(() => saveBindings(blocked, DEFAULT_BINDINGS));
});

test("only connected standard-mapped controllers are selectable", () => {
  const pads = [gamepad(), gamepad({ index: 1, mapping: "" }), null, gamepad({ index: 3 })];
  assert.deepEqual(compatibleGamepads(pads).map(({ index }) => index), [0, 3]);
});

test("D-pad and left stick map to movement with a drift dead zone", () => {
  const tracker = new GamepadTracker();
  const dpad = gamepad({ buttons: [12] });
  assert.deepEqual(tracker.sample([dpad]), { mv: "Up", bomb: false, punch: false });
  assert.deepEqual(tracker.sample([gamepad({ axes: [-0.8, 0] })]), { mv: "Left", bomb: false, punch: false });
  assert.deepEqual(tracker.sample([gamepad({ axes: [0.2, 0] })]), { mv: null, bomb: false, punch: false });
  assert.deepEqual(tracker.sample([gamepad({ axes: [0.26, 0] })]), { mv: "Right", bomb: false, punch: false });
});

test("Xbox A and B trigger bomb and punch once per press edge", () => {
  const tracker = new GamepadTracker();
  assert.deepEqual(tracker.sample([gamepad({ buttons: [0, 1] })]), { mv: null, bomb: true, punch: true });
  assert.deepEqual(tracker.sample([gamepad({ buttons: [0, 1] })]), { mv: null, bomb: false, punch: false });
  tracker.sample([gamepad()]);
  assert.deepEqual(tracker.sample([gamepad({ buttons: [0] })]), { mv: null, bomb: true, punch: false });
});

test("disconnect and controller selection changes clear held actions", () => {
  const tracker = new GamepadTracker();
  assert.equal(tracker.sample([gamepad({ buttons: [0] })]).bomb, true);
  assert.deepEqual(tracker.sample([null]), { mv: null, bomb: false, punch: false });
  const twoPads = [gamepad({ index: 0 }), gamepad({ index: 1, buttons: [0] })];
  tracker.sample(twoPads);
  tracker.select(1);
  assert.deepEqual(tracker.sample(twoPads), { mv: null, bomb: false, punch: false });
  assert.deepEqual(tracker.sample([twoPads[1]]), { mv: null, bomb: false, punch: false });
});

test("a held action across disconnect and reconnect does not fire until a fresh press", () => {
  const tracker = new GamepadTracker();
  tracker.sample([gamepad()]);
  assert.deepEqual(tracker.sample([null]), { mv: null, bomb: false, punch: false });
  assert.deepEqual(tracker.sample([gamepad({ buttons: [0] })]), { mv: null, bomb: false, punch: false });
  assert.deepEqual(tracker.sample([gamepad()]), { mv: null, bomb: false, punch: false });
  assert.deepEqual(tracker.sample([gamepad({ buttons: [0] })]), { mv: null, bomb: true, punch: false });
});