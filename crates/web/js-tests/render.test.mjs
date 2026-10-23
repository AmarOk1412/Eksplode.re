import test from "node:test";
import assert from "node:assert/strict";
import { loadModule, fixtureText, commands, KIND, HELMETS, near } from "./wasm-helper.mjs";

const CANVAS = [800, 600];

test("exports the documented functions", async () => {
  const { ex } = await loadModule();
  for (const name of [
    "memory",
    "bomberust_alloc",
    "bomberust_free",
    "bomberust_load",
    "bomberust_frame",
    "bomberust_output_ptr",
    "bomberust_stride",
    "bomberust_pick_tile",
    "bomberust_portrait",
    "bomberust_portrait3d",
  ]) {
    assert.ok(name in ex, `missing export ${name}`);
  }
  assert.equal(ex.bomberust_stride(), 16);
});

test("us1_as3_four_characters_in_their_own_colours", async () => {
  const m = await loadModule();
  assert.equal(m.loadJson(fixtureText()), 0);
  const list = commands(m.frame(...CANVAS, 1));
  const quads = list.filter((c) => c.kind === KIND.quad && c.color[3] === 1);
  HELMETS.forEach((helmet, slot) => {
    const n = quads.filter((c) => near(c.color.slice(0, 3), helmet)).length;
    assert.ok(n >= 1, `slot ${slot} has no helmet pixels`);
  });
  assert.equal(new Set(HELMETS.map((h) => h.join())).size, 4);
});

test("a portrait fills the command buffer without a loaded game", async () => {
  const { ex } = await loadModule();
  const head = ex.bomberust_portrait(1, 160, 226, 8);
  assert.ok(head > 10);
  const full = ex.bomberust_portrait(1, 160, 226, 0);
  assert.ok(full > head);
  assert.equal(ex.bomberust_portrait(0, 0, 10, 0), 0);
  assert.equal(ex.bomberust_portrait(0, NaN, 10, 0), 0);
});

test("us1_as3_commands are ordered back to front", async () => {
  const m = await loadModule();
  assert.equal(m.loadJson(fixtureText()), 0);
  const layers = commands(m.frame(...CANVAS, 1)).map((c) => c.layer);
  for (let i = 1; i < layers.length; i += 1) {
    assert.ok(layers[i - 1] <= layers[i], `command ${i} is out of order`);
  }
});

test("the same input gives the same buffer", async () => {
  const m = await loadModule();
  assert.equal(m.loadJson(fixtureText()), 0);
  const a = m.frame(...CANVAS, 0.5);
  const b = m.frame(...CANVAS, 0.5);
  assert.deepEqual(Array.from(a.data), Array.from(b.data));
  assert.ok(a.count > 100);
});

test("nothing is drawn before a load and odd canvases are handled", async () => {
  const m = await loadModule();
  assert.equal(m.frame(800, 600, 0.5).count, 0);
  assert.equal(m.loadJson(fixtureText()), 0);
  assert.equal(m.frame(0, 0, 0.5).count, 0);
  assert.equal(m.frame(NaN, 600, 0.5).count, 0);
  assert.ok(m.frame(800, 600, NaN).count > 0);
});

test("load error codes", async () => {
  const m = await loadModule();
  assert.equal(m.loadJson(new Uint8Array([0xff, 0xfe, 0xfd])), 1);
  assert.equal(m.loadJson("{}"), 2);
  assert.equal(m.loadJson("not json"), 2);
  assert.equal(m.loadJson(""), 2);

  const big = JSON.parse(fixtureText());
  big.arena.width = 65;
  big.arena.height = 65;
  big.arena.tiles = Array.from({ length: 65 * 65 }, () => "Floor");
  assert.equal(m.loadJson(JSON.stringify(big)), 3);

  const short = JSON.parse(fixtureText());
  short.arena.tiles = ["Floor"];
  assert.equal(m.loadJson(JSON.stringify(short)), 2);
});

test("allocation limits", async () => {
  const { ex } = await loadModule();
  assert.equal(ex.bomberust_alloc(0), 0);
  assert.equal(ex.bomberust_alloc(2_000_000), 0);
  const p = ex.bomberust_alloc(16);
  assert.notEqual(p, 0);
  ex.bomberust_free(p, 16);
});

test("random bytes never trap the module", async () => {
  const m = await loadModule();
  let x = 0x1234abcd;
  for (let len = 0; len < 200; len += 1) {
    const bytes = new Uint8Array(len);
    for (let i = 0; i < len; i += 1) {
      x = (Math.imul(x, 1664525) + 1013904223) >>> 0;
      bytes[i] = x >>> 24;
    }
    const code = m.loadJson(bytes);
    assert.ok(code === 1 || code === 2, `len ${len} gave ${code}`);
  }
});

test("a loaded game can be picked by canvas position", async () => {
  const m = await loadModule();
  assert.equal(m.loadJson(fixtureText()), 0);
  m.frame(...CANVAS, 1);
  assert.ok(m.ex.bomberust_pick_tile(400, 300) >= 0);
  assert.equal(m.ex.bomberust_pick_tile(-50, -50), -1);
});

test("f3_us2_as9_the_wasm_draws_every_power_icon_and_the_malus_pulse", async () => {
  const m = await loadModule();
  const obs = JSON.parse(fixtureText());
  const kinds = ["ExtraBomb", "Range", "Speed", "Punch", "Slow", "HyperSpeed", "SmallBomb"];
  obs.powerups = kinds.map((kind, i) => ({ pos: { x: 1 + i, y: 1 }, kind }));
  obs.players[0].malus = { kind: "Slow", ticks_left: 100 };
  assert.equal(m.loadJson(JSON.stringify(obs)), 0);
  const list = commands(m.frame(...CANVAS, 1));
  const letters = list.filter((c) => c.kind === KIND.text).map((c) => String.fromCharCode(c.text));
  for (const letter of ["B", "R", "F", "P", "S", "H", "s"]) {
    assert.ok(letters.includes(letter), `missing icon ${letter}`);
  }
  const pulse = list.filter((c) => c.kind === KIND.quad && c.color[3] > 0.1 && c.color[3] < 0.5 && near(c.color.slice(0, 3), [0.25, 0.5, 0.95]));
  assert.ok(pulse.length >= 1, "no malus pulse");
});
