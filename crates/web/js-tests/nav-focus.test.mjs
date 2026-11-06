import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { focusKey, focusTargets, moveFocus, moveFormFocus, moveGroupFocus, restoreFocus } from "../assets/nav.js";

test("f14_us2_as4_scene_layout_reserves_a_clipped_stable_area", () => {
  const css = readFileSync(new URL("../assets/style.css", import.meta.url), "utf8");
  const scene = /^#lobby-scene\s*\{([^}]*)\}/m.exec(css)?.[1] ?? "";
  assert.match(scene, /width:\s*100%/);
  assert.match(scene, /aspect-ratio:\s*16\s*\/\s*5/);
  assert.match(scene, /overflow:\s*hidden/);
  assert.match(css, /#lobby-scene > canvas, #lobby-scene > img\s*\{[^}]*pointer-events:\s*none/s);
});

test("f14_us2_as1_scene_art_never_takes_focus_or_changes_the_primary_actions", () => {
  const html = readFileSync(new URL("../assets/index.html", import.meta.url), "utf8");
  const menu = /<section id="main-menu"[^>]*>([\s\S]*?)<\/section>/.exec(html)[1];
  const actions = /<div class="menu-actions"[^>]*>([\s\S]*?)<\/div>/.exec(menu)[1];
  assert.equal((actions.match(/<button /g) ?? []).length, 4);
  assert.doesNotMatch(actions, /lobby-scene-pause/);
  assert.doesNotMatch(menu, /id="lobby-scene-pause"/);
  assert.match(menu, /<canvas id="lobby-scene-canvas"[^>]*aria-hidden="true"/);
  assert.doesNotMatch(menu, /<canvas[^>]*tabindex/);
});

test("f14_us2_as3_the_scene_adds_no_pause_button_or_focus_target", () => {
  const html = readFileSync(new URL("../assets/index.html", import.meta.url), "utf8");
  assert.doesNotMatch(html, /id="lobby-scene-pause"|scene-pause-icon|scene-play-icon/);
  const name = element("name", 0, 0);
  const root = { querySelectorAll: () => [name] };
  assert.deepEqual(focusTargets(root), [name]);
  assert.equal(restoreFocus(root, focusKey(name)), name);
});

function element(id, x, y, options = {}) {
  return {
    id,
    disabled: options.disabled ?? false,
    hidden: options.hidden ?? false,
    inert: options.inert ?? false,
    dataset: options.dataset ?? {},
    rect: { left: x, top: y, right: x + 20, bottom: y + 20, width: options.width ?? 20, height: options.height ?? 20 },
    getBoundingClientRect() { return this.rect; },
    focus() { this.focused = true; },
  };
}

test("focus movement chooses the nearest enabled target in the pressed direction", () => {
  const current = element("current", 0, 0);
  const near = element("near", 30, 2);
  const far = element("far", 90, 0);
  assert.equal(moveFocus(current, "right", [current, far, near]), near);
  assert.equal(near.focused, true);
});

test("focus movement falls back to DOM order when no target lies in that direction", () => {
  const first = element("first", 0, 0);
  const second = element("second", 30, 0);
  assert.equal(moveFocus(second, "right", [first, second]), first);
});

test("spatial movement uses the full label row for checkbox targets", () => {
  const slider = element("effects", 0, 0);
  slider.rect = { left: 0, top: 0, right: 100, bottom: 20, width: 100, height: 20 };
  const mute = element("mute", 0, 30);
  mute.type = "checkbox";
  mute.closest = () => ({ getBoundingClientRect: () => ({ left: 0, top: 30, right: 100, bottom: 50, width: 100, height: 20 }) });
  const testButton = element("test", 40, 60);
  assert.equal(moveFocus(slider, "down", [slider, mute, testButton]), mute);
});

test("vertical navigation walks text and select controls in DOM order", () => {
  const name = element("name", 0, 0);
  name.tagName = "INPUT";
  name.type = "text";
  const mode = element("mode", 100, 0);
  const teamMode = element("team-mode", 130, 0);
  const modeGroup = { contains: (target) => [mode, teamMode].includes(target) };
  for (const radio of [mode, teamMode]) {
    radio.tagName = "BUTTON";
    radio.getAttribute = (attribute) => attribute === "role" ? "radio" : null;
    radio.closest = (selector) => selector === '[role="radiogroup"]' ? modeGroup : null;
  }
  const size = element("size", 200, 0);
  size.tagName = "SELECT";
  const start = element("start", 0, 50);
  start.tagName = "BUTTON";
  const controls = [name, mode, teamMode, size, start];
  assert.equal(moveFormFocus(name, "down", controls), mode);
  assert.equal(moveFormFocus(teamMode, "down", controls), size);
  assert.equal(moveFormFocus(size, "down", controls), start);
  assert.equal(moveFormFocus(start, "up", controls), size);
  const oskKey = element("osk-key", 0, 0);
  oskKey.tagName = "BUTTON";
  oskKey.matches = (selector) => selector === ".osk-key";
  assert.equal(moveFormFocus(oskKey, "down", [oskKey, start]), null);
});

test("horizontal movement within an action group follows button order", () => {
  const first = element("first", 0, 0);
  const second = element("second", 10, 10);
  const third = element("third", 20, 20);
  const group = { contains: (target) => [first, second, third].includes(target) };
  for (const button of [first, second, third]) {
    button.tagName = "BUTTON";
    button.closest = (selector) => selector === ".buttons" ? group : null;
  }
  assert.equal(moveGroupFocus(third, "left", [first, second, third]), second);
  assert.equal(moveGroupFocus(first, "left", [first, second, third]), null);
});

test("focus targets omit hidden, disabled, inert and zero-size elements", () => {
  const visible = element("visible", 0, 0);
  const disabled = element("disabled", 0, 0, { disabled: true });
  const hidden = element("hidden", 0, 0, { hidden: true });
  const inert = element("inert", 0, 0, { inert: true });
  const empty = element("empty", 0, 0, { width: 0, height: 0 });
  const root = { querySelectorAll: () => [visible, disabled, hidden, inert, empty] };
  assert.deepEqual(focusTargets(root), [visible]);
});

test("stable focus keys restore after an element is rebuilt, and missing keys are safe", () => {
  const original = element("", 0, 0, { dataset: { navKey: "room-mode-team" } });
  const root = { querySelectorAll: () => [element("other", 0, 0), element("", 0, 0, { dataset: { navKey: "room-mode-team" } })] };
  const key = focusKey(original);
  assert.equal(key, "room-mode-team");
  assert.equal(restoreFocus(root, key).dataset.navKey, key);
  assert.equal(restoreFocus(root, "missing"), null);
  assert.equal(moveFocus(null, "left", []), null);
});

