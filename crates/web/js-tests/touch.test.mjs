import test from "node:test";
import assert from "node:assert/strict";
import { TouchInputTracker, directionFromVector, isTouchCapable } from "../assets/touch.js";

test("us2_as1_joystick maps a thumb vector to one cardinal direction", () => {
  assert.equal(directionFromVector(0, -40), "Up");
  assert.equal(directionFromVector(0, 40), "Down");
  assert.equal(directionFromVector(-40, 0), "Left");
  assert.equal(directionFromVector(40, 0), "Right");
  assert.equal(directionFromVector(10, 4), null, "the central dead zone stays neutral");
});

test("us2_as1_only the joystick owner can steer or release movement", () => {
  const tracker = new TouchInputTracker();
  assert.equal(tracker.startJoystick(1, 100, 100), true);
  assert.deepEqual(tracker.take(), { mv: null, bomb: false, punch: false });
  assert.equal(tracker.startJoystick(2, 200, 200), false, "a second finger cannot steal the stick");
  assert.equal(tracker.moveJoystick(2, 240, 200), false);
  assert.equal(tracker.moveJoystick(1, 145, 100), true);
  assert.equal(tracker.take().mv, "Right");
  assert.equal(tracker.endJoystick(2), false, "another finger cannot release movement");
  assert.equal(tracker.take().mv, "Right");
  assert.equal(tracker.endJoystick(1), true);
  assert.equal(tracker.take().mv, null);
});

test("us2_as2_bomb_and_punch_are_one_shot_actions", () => {
  const tracker = new TouchInputTracker();
  assert.equal(tracker.pressAction(2, "bomb"), true);
  assert.equal(tracker.pressAction(2, "bomb"), false, "a held finger does not repeat");
  assert.equal(tracker.pressAction(3, "punch"), true);
  assert.deepEqual(tracker.take(), { mv: null, bomb: true, punch: true });
  assert.deepEqual(tracker.take(), { mv: null, bomb: false, punch: false });
  assert.equal(tracker.releaseAction(2), true);
  assert.equal(tracker.pressAction(2, "bomb"), true, "a fresh press can act again");
  assert.equal(tracker.take().bomb, true);
});

test("us2_as3_simultaneous_joystick_and_action_pointers combine in one input", () => {
  const tracker = new TouchInputTracker();
  tracker.startJoystick(1, 100, 100);
  tracker.moveJoystick(1, 150, 100);
  tracker.pressAction(2, "bomb");
  assert.deepEqual(tracker.take(), { mv: "Right", bomb: true, punch: false });
  assert.deepEqual(tracker.take(), { mv: "Right", bomb: false, punch: false });
});

test("us2_as5_pointer cancellation clears held movement and pending actions", () => {
  const tracker = new TouchInputTracker();
  tracker.startJoystick(1, 100, 100);
  tracker.moveJoystick(1, 150, 100);
  tracker.pressAction(2, "punch");
  tracker.clear();
  assert.deepEqual(tracker.take(), { mv: null, bomb: false, punch: false });
  assert.equal(tracker.moveJoystick(1, 180, 100), false);
});

test("us2_as10_firefox_android_touch_points_enable_mobile_controls_without_a_coarse_primary_pointer", () => {
  assert.equal(isTouchCapable({ primaryCoarse: false, anyCoarse: false, maxTouchPoints: 5 }), true);
  assert.equal(isTouchCapable({ hasTouchEvent: true }), true, "Firefox Android can expose touch events without pointer media data");
  assert.equal(isTouchCapable({ primaryCoarse: false, anyCoarse: true, maxTouchPoints: 0 }), true);
  assert.equal(isTouchCapable({ primaryCoarse: true, anyCoarse: false, maxTouchPoints: 0 }), true);
  assert.equal(isTouchCapable({ primaryCoarse: false, anyCoarse: false, maxTouchPoints: 0 }), false);
});

