import test from "node:test";
import assert from "node:assert/strict";
import { NavigationLayers } from "../assets/nav.js";

function control(id, disabled = false) {
  return {
    id,
    disabled,
    getBoundingClientRect: () => ({ left: 0, top: 0, right: 20, bottom: 20, width: 20, height: 20 }),
    focus() { this.focused = true; },
  };
}
function root(...controls) {
  return { querySelectorAll: () => controls };
}

test("layers are unique, top-only, and close restores focus to the opener", () => {
  const layers = new NavigationLayers();
  const hubButton = control("hub-sound");
  const soundClose = control("sound-close");
  layers.open("hub", root(hubButton), control("menu-opener"));
  layers.open("sound", root(soundClose), hubButton);
  assert.equal(layers.top.id, "sound");
  assert.equal(layers.open("sound", root(soundClose)), false);
  assert.equal(layers.close().focus, hubButton);
  assert.equal(layers.top.id, "hub");
});

test("closing falls back to first enabled control below and back closes only the top", () => {
  const layers = new NavigationLayers();
  const first = control("first");
  const opener = control("opener", true);
  const panel = control("panel");
  layers.open("hub", root(first), null);
  layers.open("controls", root(panel), opener);
  assert.equal(layers.back().focus, first);
  assert.equal(layers.top.id, "hub");
  assert.equal(layers.close().focus, null);
  assert.equal(layers.top, null);
});

test("the on-screen keyboard layer can only occur once", () => {
  const layers = new NavigationLayers();
  layers.open("osk", root(control("key")));
  assert.equal(layers.open("osk", root(control("key-2"))), false);
  assert.equal(layers.has("osk"), true);
});
