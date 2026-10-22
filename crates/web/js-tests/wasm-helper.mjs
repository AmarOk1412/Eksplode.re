// Loads the committed WebAssembly module in Node, the same way the page does.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));

export const FIXTURE = path.join(here, "../../web-render/tests/fixtures/observation.json");
export const WASM = path.join(here, "../assets/bomberust_render.wasm");

export async function loadModule() {
  const { instance } = await WebAssembly.instantiate(readFileSync(WASM), {});
  const ex = instance.exports;

  function loadJson(text) {
    const bytes = typeof text === "string" ? new TextEncoder().encode(text) : text;
    const size = Math.max(bytes.length, 1);
    const ptr = ex.bomberust_alloc(size);
    if (ptr === 0) return -1;
    new Uint8Array(ex.memory.buffer, ptr, bytes.length).set(bytes);
    const code = ex.bomberust_load(ptr, bytes.length);
    ex.bomberust_free(ptr, size);
    return code;
  }

  function frame(width, height, alpha) {
    const count = ex.bomberust_frame(width, height, alpha);
    const stride = ex.bomberust_stride();
    const view = new Float32Array(ex.memory.buffer, ex.bomberust_output_ptr(), count * stride);
    return { count, stride, data: Float32Array.from(view) };
  }

  return { ex, loadJson, frame };
}

export function fixtureText() {
  return readFileSync(FIXTURE, "utf8");
}

export const KIND = { quad: 0, circle: 1, text: 2, triangle: 3 };

/** Helmet colour of each character as the Rust palette defines it. */
export const HELMETS = [
  [0xd9, 0xd9, 0xde],
  [0x0b, 0x0b, 0x0b],
  [0xb0, 0x34, 0x34],
  [0xc7, 0x7a, 0xe6],
].map((c) => c.map((v) => v / 255));

export function near(a, b) {
  return a.every((v, i) => Math.abs(v - b[i]) < 1e-4);
}

export function commands({ count, stride, data }) {
  return Array.from({ length: count }, (_, i) => {
    const o = i * stride;
    return {
      kind: data[o],
      color: [data[o + 1], data[o + 2], data[o + 3], data[o + 4]],
      size: data[o + 13],
      text: data[o + 14],
      layer: data[o + 15],
    };
  });
}
