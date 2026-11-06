import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRenderer, paintCommands, LobbySceneController } from "../assets/app.js";
import { WASM, fixtureText } from "./wasm-helper.mjs";

async function exportsOfModule() {
  const { instance } = await WebAssembly.instantiate(readFileSync(WASM), {});
  return instance.exports;
}

test("f14_us2_as5_decorative_failures_leave_application_state_and_gameplay_exports_alone", async () => {
  const { graphicsFailure } = await import("../assets/app.js");
  assert.equal(typeof graphicsFailure, "function");
  const state = { stage: "directory", protocol: 2, menu: "main", notice: "", room_id: null };
  assert.deepEqual(graphicsFailure(state, "offline"), { ...state, graphicsError: "Graphics unavailable: offline" });
  assert.equal(state.graphicsError, undefined);
  for (const available of [false, true]) {
    let stopped = 0;
    const scene = new LobbySceneController({ raf: () => 1, caf: () => {}, paint: () => false, still: () => { stopped += 1; } });
    scene.setRenderer({ lobbyAvailable: () => available, resetLobby: () => 0 });
    scene.sync({ menuVisible: true });
    assert.equal(scene.failed, true);
    assert.equal(scene.animator.active, false);
    assert.ok(stopped > 0);
  }
});

test("f14_foundation_scene_wrapper_detects_optional_exports_and_validates_inputs", () => {
  const memory = new WebAssembly.Memory({ initial: 1 });
  const calls = [];
  const ex = {
    memory, eksplode_re_stride: () => 16, eksplode_re_output_ptr: () => 16,
    eksplode_re_lobby_reset: (seed) => { calls.push(seed); return 0; },
    eksplode_re_lobby_advance: (ticks) => { calls.push(ticks); return 0; },
    eksplode_re_lobby_frame: (_width, _height, alpha) => {
      assert.ok(Math.fround(alpha) < 1, "interpolation stays valid after the WASM f32 conversion");
      memory.grow(1);
      new Float32Array(memory.buffer, 16, 16).fill(0.5);
      return 1;
    },
  };
  const renderer = createRenderer(ex);
  assert.equal(renderer.lobbyAvailable(), true);
  assert.equal(renderer.resetLobby(0xffffffff), 0);
  assert.equal(renderer.advanceLobby(15), 0);
  assert.notEqual(renderer.resetLobby(-1), 0);
  assert.notEqual(renderer.resetLobby(0x100000000), 0);
  assert.notEqual(renderer.advanceLobby(16), 0);
  assert.notEqual(renderer.advanceLobby(0.5), 0);
  assert.deepEqual(calls, [0xffffffff, 15]);
  const first = renderer.lobbyFrame(640, 200, 0);
  assert.equal(first.data.length, 16);
  const copy = first.data.slice();
  const second = renderer.lobbyFrame(640, 200, 0.5);
  assert.equal(second.data.buffer, memory.buffer);
  assert.deepEqual(second.data, copy);
  assert.equal(renderer.lobbyFrame(640, 200, 1 - Number.EPSILON).count, 1);
  const old = createRenderer({ memory, eksplode_re_stride: () => 16 });
  assert.equal(old.lobbyAvailable(), false);
  assert.notEqual(old.resetLobby(1), 0);
  assert.equal(old.lobbyFrame(640, 200, 0).count, 0);
});

class FakeCtx {
  constructor() {
    this.ops = [];
    this.fillStyle = "";
    this.font = "";
    this.textAlign = "";
    this.textBaseline = "";
  }
  beginPath() { this.ops.push(["beginPath"]); }
  moveTo(x, y) { this.ops.push(["moveTo", x, y]); }
  lineTo(x, y) { this.ops.push(["lineTo", x, y]); }
  closePath() { this.ops.push(["closePath"]); }
  arc(x, y, r) { this.ops.push(["arc", x, y, r]); }
  fill() { this.ops.push(["fill", this.fillStyle]); }
  fillText(t, x, y) { this.ops.push(["fillText", t, x, y, this.font]); }
  clearRect() { this.ops.push(["clearRect"]); }
}

test("the glue loads the fixture and reads commands back", async () => {
  const r = createRenderer(await exportsOfModule());
  assert.equal(r.load(fixtureText()), 0);
  const f = r.frame(800, 600, 1);
  assert.ok(f.count > 100);
  assert.equal(f.stride, 16);
  assert.equal(f.data.length, f.count * 16);
});

test("the glue refuses a module with another stride", async () => {
  const ex = await exportsOfModule();
  assert.throws(() => createRenderer({ ...ex, eksplode_re_stride: () => 12 }), /out of sync/i);
});

test("the glue reports load errors with words", async () => {
  const r = createRenderer(await exportsOfModule());
  assert.notEqual(r.load("{}"), 0);
  assert.equal(r.describe(1), "invalid text");
  assert.equal(r.describe(2), "invalid game data");
  assert.equal(r.describe(3), "arena too large");
});

test("painting draws every command kind", async () => {
  const r = createRenderer(await exportsOfModule());
  assert.equal(r.load(fixtureText()), 0);
  const f = r.frame(800, 600, 1);
  const ctx = new FakeCtx();
  paintCommands(ctx, f);
  const count = (name) => ctx.ops.filter((o) => o[0] === name).length;
  assert.equal(count("fill"), f.count - count("fillText"), "every non-text command is filled");
  const texts = Array.from({ length: f.count }, (_, i) => f.data[i * f.stride]).filter((k) => k === 2).length;
  assert.equal(count("fillText"), texts, "text only for power-up letters");
  assert.equal(count("closePath"), f.count - count("fillText") - count("arc"));
  assert.ok(ctx.ops.filter((o) => o[0] === "fill").every((o) => /^rgba\(/.test(o[1])));
});

test("painting uses the colours of the commands", () => {
  const ctx = new FakeCtx();
  const data = new Float32Array(16);
  data.set([0, 1, 0, 0, 1, 0, 0, 10, 0, 10, 10, 0, 10, 0, 0, 0]);
  paintCommands(ctx, { count: 1, stride: 16, data });
  assert.deepEqual(ctx.ops.find((o) => o[0] === "fill"), ["fill", "rgba(255,0,0,1)"]);
});

test("a circle command is an arc and a text command is centred text", () => {
  const ctx = new FakeCtx();
  const circle = new Float32Array(16);
  circle.set([1, 0, 0, 0, 1, 50, 60, 0, 0, 0, 0, 0, 0, 7, 0, 0]);
  const text = new Float32Array(16);
  text.set([2, 1, 1, 1, 1, 20, 30, 0, 0, 0, 0, 0, 0, 14, 66, 0]);
  paintCommands(ctx, { count: 2, stride: 16, data: Float32Array.from([...circle, ...text]) });
  assert.deepEqual(ctx.ops.find((o) => o[0] === "arc"), ["arc", 50, 60, 7]);
  const t = ctx.ops.find((o) => o[0] === "fillText");
  assert.equal(t[1], "B");
  assert.equal(ctx.textAlign, "center");
});

test("the glue paints character portraits", async () => {
  const r = createRenderer(await exportsOfModule());
  const p = r.portrait(2, 160, 226, 8);
  assert.ok(p.count > 10);
  assert.equal(p.data.length, p.count * 16);
  const ctx = new FakeCtx();
  paintCommands(ctx, p);
  assert.equal(ctx.ops.filter((o) => o[0] === "fill").length, p.count);
});

test("the glue paints 3d character portraits", async () => {
  const r = createRenderer(await exportsOfModule());
  const p = r.portrait3d(1, 160, 226, 0.5);
  assert.ok(p.count > 20);
  assert.equal(p.data.length, p.count * 16);
  assert.equal(r.portrait3d(0, 0, 226, 0).count, 0);
  const ctx = new FakeCtx();
  paintCommands(ctx, p);
  assert.equal(ctx.ops.filter((o) => o[0] === "fill").length, p.count);
});
