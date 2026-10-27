import test from "node:test";
import assert from "node:assert/strict";
import { createOskState, reduceOsk } from "../assets/nav.js";

test("on-screen keyboard respects max length, shift, space and backspace", () => {
  let state = createOskState("Name", "Ab", 4);
  state = reduceOsk(state, { type: "shift" });
  state = reduceOsk(state, { type: "character", value: "c" });
  assert.equal(state.buffer, "AbC");
  state = reduceOsk(state, { type: "space" });
  state = reduceOsk(state, { type: "character", value: "D" });
  assert.equal(state.buffer, "AbC ");
  state = reduceOsk(state, { type: "backspace" });
  assert.equal(state.buffer, "AbC");
  state = reduceOsk(state, { type: "character", value: "\n" });
  assert.equal(state.buffer, "AbC");
  state = reduceOsk(state, { type: "character", value: "\u0085" });
  assert.equal(state.buffer, "AbC");
});

test("confirm returns the buffer, cancel restores the original and cursor clamps", () => {
  let state = createOskState("Name", "Old", 16, { row: 99, column: 99 });
  state = reduceOsk(state, { type: "character", value: "x" });
  assert.deepEqual(reduceOsk(state, { type: "confirm" }), { ...state, result: "Oldx", closed: true });
  assert.deepEqual(reduceOsk(state, { type: "cancel" }), { ...state, result: "Old", closed: true });
  assert.ok(state.cursor.row >= 0 && state.cursor.column >= 0);
});

test("empty player names remain representable within their field limit", () => {
  const state = createOskState("Player", "", 16);
  assert.equal(state.buffer, "");
  assert.equal(reduceOsk(state, { type: "confirm" }).result, "");
});
