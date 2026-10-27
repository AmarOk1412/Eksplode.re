import test from "node:test";
import assert from "node:assert/strict";
import {
  COLOUR_NAMES,
  GENDERS,
  HEADS,
  FACES,
  LOOK_KEY,
  defaultLook,
  sanitizeLook,
  editorReducer,
  loadLook,
  saveLook,
  previewCommands,
  looksJson,
  lookMessage,
} from "../assets/look.js";
import { initialState, reduce } from "../assets/app.js";
import { fakeStorage } from "./audio-helper.mjs";

const lobby = (n) => ({
  slots: Array.from({ length: 4 }, (_, i) => ({
    controller: i < n ? "Remote" : "Empty",
    name: i < n ? `p${i}` : "",
    ready: false,
  })),
});

test("the options are exactly the documented ones", () => {
  assert.deepEqual(COLOUR_NAMES, ["Blue", "Black", "Red", "Purple", "Green", "Orange", "Yellow", "Pink", "Teal", "White"]);
  assert.deepEqual(GENDERS, ["Male", "Female"]);
  assert.deepEqual(HEADS, ["None", "Cap", "TopHat", "Crown", "Headphones"]);
  assert.deepEqual(FACES, ["None", "Glasses", "Sunglasses", "Moustache", "Mask"]);
});

test("defaults follow the slot like the host's", () => {
  assert.deepEqual(defaultLook(0), { colour: "Blue", gender: "Male", head: "None", face: "None" });
  assert.deepEqual(defaultLook(1), { colour: "Black", gender: "Female", head: "None", face: "None" });
  assert.deepEqual(defaultLook(2), { colour: "Red", gender: "Male", head: "None", face: "None" });
  assert.deepEqual(defaultLook(3), { colour: "Purple", gender: "Female", head: "None", face: "None" });
  assert.deepEqual(defaultLook(4), defaultLook(0));
});

test("sanitizing replaces bad fields one by one and never throws", () => {
  assert.deepEqual(sanitizeLook({ colour: "Green" }, 1), { colour: "Green", gender: "Female", head: "None", face: "None" });
  assert.deepEqual(sanitizeLook({ colour: 7, gender: null, head: [], face: {} }, 2), defaultLook(2));
  assert.deepEqual(sanitizeLook({ colour: "green" }, 3), defaultLook(3));
  for (const bad of [null, undefined, 5, "Blue", [1], true]) assert.deepEqual(sanitizeLook(bad, 1), defaultLook(1));
});

test("f3_us1_as1_the_editor_changes_one_choice_at_a_time", () => {
  const start = defaultLook(0);
  let s = editorReducer(start, { type: "colour", value: "Teal" });
  assert.equal(s.colour, "Teal");
  s = editorReducer(s, { type: "gender", value: "Female" });
  s = editorReducer(s, { type: "head", value: "Crown" });
  s = editorReducer(s, { type: "face", value: "Sunglasses" });
  assert.deepEqual(s, { colour: "Teal", gender: "Female", head: "Crown", face: "Sunglasses" });
  assert.deepEqual(start, defaultLook(0), "the old state is not touched");
  // unknown values are ignored
  assert.deepEqual(editorReducer(s, { type: "colour", value: "Mauve" }), s);
  assert.deepEqual(editorReducer(s, { type: "nothing" }), s);
  // random picks valid options from the given generator
  const r = editorReducer(s, { type: "random", rng: () => 0.99 });
  assert.equal(r.colour, "White");
  assert.equal(r.gender, "Female");
  assert.equal(r.head, "Headphones");
  assert.equal(r.face, "Mask");
});

test("f3_us1_as2_the_look_is_remembered_after_a_reload", () => {
  const storage = fakeStorage();
  const look = { colour: "Pink", gender: "Female", head: "Cap", face: "Glasses" };
  saveLook(storage, look);
  assert.ok(LOOK_KEY.endsWith(".v1"));
  assert.deepEqual(JSON.parse(storage.dump()[LOOK_KEY]), look);
  assert.deepEqual(loadLook(storage, 0), look, "a new page load finds it again");
});

test("looks migrate from the Bomberust storage key", () => {
  const storage = fakeStorage();
  const look = { colour: "Pink", gender: "Female", head: "Cap", face: "Glasses" };
  storage.setItem("bomberust.look.v1", JSON.stringify(look));
  assert.deepEqual(loadLook(storage), look);
});

test("a missing, corrupt or unavailable store gives the default", () => {
  assert.deepEqual(loadLook(fakeStorage(), 1), defaultLook(1));
  const corrupt = fakeStorage();
  corrupt.setItem(LOOK_KEY, "{not json");
  assert.deepEqual(loadLook(corrupt, 2), defaultLook(2));
  const odd = fakeStorage();
  odd.setItem(LOOK_KEY, JSON.stringify({ colour: "Mauve", head: "Cap" }));
  assert.deepEqual(loadLook(odd, 0), { ...defaultLook(0), head: "Cap" });
  const broken = fakeStorage({ broken: true });
  assert.deepEqual(loadLook(broken, 3), defaultLook(3));
  assert.doesNotThrow(() => saveLook(broken, defaultLook(0)));
  assert.deepEqual(loadLook(undefined, 0), defaultLook(0));
});

test("f3_us1_as1_the_preview_sets_the_look_then_draws_and_restores_the_others", () => {
  const calls = [];
  const renderer = {
    setLooks: (looks) => calls.push(["setLooks", looks]),
    portrait: (slot, w, h, rows) => {
      calls.push(["portrait", slot, w, h, rows]);
      return { count: 3, stride: 16, data: new Float32Array(48) };
    },
  };
  const draft = { colour: "Green", gender: "Male", head: "Crown", face: "None" };
  const others = [defaultLook(0), defaultLook(1)];
  const commands = previewCommands(renderer, draft, 160, 226, 7, others);
  assert.equal(commands.count, 3);
  assert.deepEqual(calls[0], ["setLooks", [draft]]);
  assert.deepEqual(calls[1], ["portrait", 0, 160, 226, 7]);
  assert.deepEqual(calls[2], ["setLooks", others]);
  assert.equal(calls.length, 3);
});

test("looks are sent to the module as a JSON array of four entries", () => {
  const looks = [defaultLook(0), { colour: "Teal" }, null];
  const parsed = JSON.parse(looksJson(looks));
  assert.equal(parsed.length, 4);
  assert.deepEqual(parsed[0], defaultLook(0));
  assert.deepEqual(parsed[1], sanitizeLook({ colour: "Teal" }, 1));
  assert.deepEqual(parsed[2], defaultLook(2), "bad entries become defaults");
  assert.deepEqual(parsed[3], defaultLook(3));
});

test("the join and set-look messages carry the sanitized look", () => {
  const look = { colour: "Orange", gender: "Male", head: "None", face: "Moustache" };
  assert.deepEqual(lookMessage("Join", look, { name: "Ann", version: 1 }), {
    type: "Join",
    version: 1,
    name: "Ann",
    look,
  });
  assert.deepEqual(lookMessage("SetLook", { colour: 1 }, {}), { type: "SetLook", look: defaultLook(0) });
});

test("f3_us1_as4_the_reducer_keeps_the_looks_and_shows_the_adjusted_colour_notice", () => {
  const looks = [0, 1, 2, 3].map((i) => defaultLook(i));
  let s = reduce(initialState(), { type: "Welcome", slot: 0, config: {}, lobby: lobby(1), looks });
  assert.deepEqual(s.looks, looks);
  const changed = [defaultLook(0), { ...defaultLook(1), head: "Cap" }, defaultLook(2), defaultLook(3)];
  s = reduce(s, { type: "Lobby", lobby: lobby(2), looks: changed });
  assert.deepEqual(s.looks, changed);
  s = reduce(s, { type: "Lobby", lobby: lobby(2) });
  assert.deepEqual(s.looks, changed, "a lobby without looks (old host) keeps what is known");
  s = reduce(s, { type: "Error", code: "LookAdjusted", message: "That colour is taken: you are Orange instead" });
  assert.match(s.notice, /Orange/);
  assert.equal(s.stage, "lobby");
});

test("a welcome from an old host without looks gives default looks", () => {
  const s = reduce(initialState(), { type: "Welcome", slot: 0, config: {}, lobby: lobby(1) });
  assert.deepEqual(s.looks, [0, 1, 2, 3].map((i) => defaultLook(i)));
});
