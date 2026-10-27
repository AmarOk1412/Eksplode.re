import test from "node:test";
import assert from "node:assert/strict";
import { loadModule, fixtureText, commands, KIND, HELMETS, near } from "./wasm-helper.mjs";

const CANVAS = [800, 600];

test("exports the documented functions", async () => {
  const { ex } = await loadModule();
  for (const name of [
    "memory",
    "eksplode_re_alloc",
    "eksplode_re_free",
    "eksplode_re_load",
    "eksplode_re_frame",
    "eksplode_re_output_ptr",
    "eksplode_re_stride",
    "eksplode_re_pick_tile",
    "eksplode_re_portrait",
    "eksplode_re_portrait3d",
  ]) {
    assert.ok(name in ex, `missing export ${name}`);
  }
  assert.equal(ex.eksplode_re_stride(), 16);
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
  const head = ex.eksplode_re_portrait(1, 160, 226, 8);
  assert.ok(head > 10);
  const full = ex.eksplode_re_portrait(1, 160, 226, 0);
  assert.ok(full > head);
  assert.equal(ex.eksplode_re_portrait(0, 0, 10, 0), 0);
  assert.equal(ex.eksplode_re_portrait(0, NaN, 10, 0), 0);
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
  assert.equal(ex.eksplode_re_alloc(0), 0);
  assert.equal(ex.eksplode_re_alloc(2_000_000), 0);
  const p = ex.eksplode_re_alloc(16);
  assert.notEqual(p, 0);
  ex.eksplode_re_free(p, 16);
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
  assert.ok(m.ex.eksplode_re_pick_tile(400, 300) >= 0);
  assert.equal(m.ex.eksplode_re_pick_tile(-50, -50), -1);
});

test("f3_us2_as10_the_wasm_draws_every_power_as_a_pixel_icon_and_the_malus_pulse", async () => {
  const m = await loadModule();
  const obs = JSON.parse(fixtureText());
  const kinds = ["ExtraBomb", "Range", "Speed", "Punch", "Kick", "Slow", "HyperSpeed", "SmallBomb"];
  const bare = structuredClone(obs);
  bare.powerups = [];
  assert.equal(m.loadJson(JSON.stringify(bare)), 0);
  const before = commands(m.frame(...CANVAS, 1)).length;
  const count = [];
  for (const kind of kinds) {
    const one = structuredClone(bare);
    one.powerups = [{ pos: { x: 1, y: 1 }, kind }];
    assert.equal(m.loadJson(JSON.stringify(one)), 0);
    const list = commands(m.frame(...CANVAS, 1));
    assert.equal(list.filter((c) => c.kind === KIND.text).length, 0, `${kind} is a letter`);
    count.push(list.length - before);
  }
  assert.ok(count.every((n) => n >= 10), `each icon is several pixel runs: ${count}`);
  assert.equal(new Set(count).size > 3, true, "the pictures differ");
  obs.powerups = [];
  obs.players[0].malus = { kind: "Slow", ticks_left: 100 };
  assert.equal(m.loadJson(JSON.stringify(obs)), 0);
  const list = commands(m.frame(...CANVAS, 1));
  const pulse = list.filter((c) => c.kind === KIND.quad && c.color[3] > 0.1 && c.color[3] < 0.5 && near(c.color.slice(0, 3), [0.25, 0.5, 0.95]));
  assert.ok(pulse.length >= 1, "no malus pulse");
});

test("f3_us1_as8_the_wasm_draws_the_view_of_the_facing_and_a_step_frame_while_walking", async () => {
  const m = await loadModule();
  const base = JSON.parse(fixtureText());
  base.arena.tiles = base.arena.tiles.map((t) => (t === "Block" ? "Floor" : t));
  const place = (x, tick, facing) => {
    const o = structuredClone(base);
    o.tick = tick;
    o.players.forEach((p, i) => {
      p.pos = { x: x + 2 * i, y: 3 };
      p.facing = facing;
    });
    return o;
  };
  const eyes = [0x7a, 0x3a, 0x34].map((v) => v / 255); // eyes of the first character
  const eyeQuads = () => commands(m.frame(...CANVAS, 0)).filter((c) => c.kind === KIND.quad && c.color[3] === 1 && near(c.color.slice(0, 3), eyes)).length;
  const seen = {};
  for (const facing of ["Down", "Up", "Left", "Right"]) {
    assert.equal(m.loadJson(JSON.stringify(place(1, 10, facing))), 0);
    seen[facing] = eyeQuads();
  }
  assert.deepEqual(seen, { Down: 4, Up: 0, Left: 2, Right: 2 });
  // standing, then a step: the picture changes and goes back when the feet pass
  assert.equal(m.loadJson(JSON.stringify(place(1, 20, "Down"))), 0);
  const standing = m.frame(...CANVAS, 0).data.slice();
  assert.equal(m.loadJson(JSON.stringify(place(2, 21, "Down"))), 0);
  const stepping = m.frame(...CANVAS, 0).data.slice();
  assert.notDeepEqual(Array.from(stepping), Array.from(standing));
});
