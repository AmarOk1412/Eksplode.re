import test from "node:test";
import assert from "node:assert/strict";
import { activateControl, adjustControl } from "../assets/nav.js";

function control(properties = {}) {
  return {
    type: "button",
    value: "",
    checked: false,
    events: [],
    options: [],
    dispatchEvent(event) { this.events.push(event.type); },
    click() { this.clicked = true; },
    ...properties,
  };
}

test("left and right cycle selects and dispatch change", () => {
  const select = control({ tagName: "SELECT", options: [{ value: "a" }, { value: "b" }, { value: "c" }], selectedIndex: 0, value: "a" });
  assert.equal(adjustControl(select, "right"), true);
  assert.equal(select.value, "b");
  assert.deepEqual(select.events, ["change"]);
  adjustControl(select, "left");
  assert.equal(select.value, "a");
});

test("range steps clamp and dispatch input before change", () => {
  const range = control({ type: "range", min: "0", max: "5", step: "2", value: "4" });
  adjustControl(range, "right");
  assert.equal(range.value, "5");
  assert.deepEqual(range.events, ["input", "change"]);
});

test("radio groups move selection, and vertical directions leave adjustment alone", () => {
  const first = control({ type: "radio", checked: true, name: "mode" });
  const second = control({ type: "radio", checked: false, name: "mode" });
  first.form = second.form = { querySelectorAll: () => [first, second] };
  assert.equal(adjustControl(first, "right"), true);
  assert.equal(second.checked, true);
  assert.deepEqual(second.events, ["change"]);
  assert.equal(adjustControl(second, "down"), false);
});

test("confirm activates buttons and checkboxes, and requests OSK for text fields", () => {
  const button = control({ tagName: "BUTTON" });
  assert.equal(activateControl(button), "activated");
  assert.equal(button.clicked, true);
  const checkbox = control({ type: "checkbox" });
  activateControl(checkbox);
  assert.equal(checkbox.clicked, true);
  const text = control({ tagName: "INPUT", type: "text" });
  assert.equal(activateControl(text), "osk");
});
