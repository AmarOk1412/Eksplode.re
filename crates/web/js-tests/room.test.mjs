import test from "node:test";
import assert from "node:assert/strict";
import {
  MAX_NAME,
  MINUTES,
  MODES,
  PLAYABLE_MODES,
  ROOM_KEY,
  SIZES,
  defaultRoom,
  loadRoom,
  roomControls,
  roomLabel,
  roomPayload,
  roomReducer,
  sanitizeRoom,
  saveRoom,
  setRoomMessage,
} from "../assets/room.js";
import { initialState, reduce } from "../assets/app.js";

const memory = () => {
  const data = new Map();
  return {
    data,
    getItem: (k) => (data.has(k) ? data.get(k) : null),
    setItem: (k, v) => data.set(k, v),
  };
};

test("the defaults are the game as it was: Normal, small board, 3 minutes, friendly fire on", () => {
  assert.deepEqual(defaultRoom(), { name: "", mode: "normal", size: "small", minutes: 3, friendly_fire: true });
  assert.deepEqual(MODES, ["normal", "team", "chaser"]);
  assert.deepEqual(SIZES, ["small", "medium", "large"]);
  assert.deepEqual(MINUTES, [2, 3, 5, 10]);
  assert.ok(PLAYABLE_MODES.includes("normal"));
});

test("sanitizeRoom turns anything into valid settings, one field at a time", () => {
  for (const v of [null, undefined, 5, "team", [1], true]) assert.deepEqual(sanitizeRoom(v), defaultRoom());
  const s = sanitizeRoom({ name: 5, mode: 3, size: ["large"], minutes: "5", friendly_fire: "no", extra: {} });
  assert.deepEqual(s, defaultRoom());
  const t = sanitizeRoom({ name: "Friday", size: "large", minutes: 10, friendly_fire: false, mode: "battle" });
  assert.deepEqual(t, { name: "Friday", mode: "normal", size: "large", minutes: 10, friendly_fire: false });
  assert.equal(sanitizeRoom({ minutes: 4 }).minutes, 3);
  assert.equal(sanitizeRoom({ name: "\u0007Hi\nthere " }).name, "Hithere");
  assert.equal([...sanitizeRoom({ name: "é".repeat(80) }).name].length, MAX_NAME);
});

test("the reducer changes one value at a time and ignores bad ones", () => {
  let r = defaultRoom();
  r = roomReducer(r, { type: "name", value: "Friday room" });
  r = roomReducer(r, { type: "size", value: "medium" });
  r = roomReducer(r, { type: "minutes", value: "5" });
  r = roomReducer(r, { type: "friendly_fire", value: false });
  assert.deepEqual(r, { name: "Friday room", mode: "normal", size: "medium", minutes: 5, friendly_fire: false });
  assert.equal(roomReducer(r, { type: "size", value: "huge" }), r);
  assert.equal(roomReducer(r, { type: "minutes", value: 4 }), r);
  assert.equal(roomReducer(r, { type: "mode", value: "battle" }), r);
  assert.equal(roomReducer(r, { type: "nothing" }), r);
  assert.equal(roomReducer(r, { type: "name", value: "x".repeat(60) }).name.length, MAX_NAME);
  assert.equal(roomReducer(r, { type: "mode", value: "normal" }).mode, "normal");
});

test("the settings are remembered on this device and bad storage gives the defaults", () => {
  const s = memory();
  saveRoom(s, { name: "Friday", size: "large", minutes: 10, mode: "normal", friendly_fire: true });
  assert.ok(s.data.has(ROOM_KEY));
  assert.equal(loadRoom(s).size, "large");
  assert.equal(loadRoom(s).name, "Friday");
  s.data.set(ROOM_KEY, "{broken");
  assert.deepEqual(loadRoom(s), defaultRoom());
  s.data.set(ROOM_KEY, JSON.stringify({ size: ["x"], minutes: 99 }));
  assert.deepEqual(loadRoom(s), defaultRoom());
  assert.deepEqual(loadRoom(undefined), defaultRoom());
  const throwing = {
    getItem() {
      throw new Error("blocked");
    },
    setItem() {
      throw new Error("full");
    },
  };
  assert.deepEqual(loadRoom(throwing), defaultRoom());
  assert.doesNotThrow(() => saveRoom(throwing, defaultRoom()));
  assert.equal(loadRoom(null, { size: "medium" }).size, "medium");
});

test("the payloads for Join and SetRoom carry sanitized settings", () => {
  assert.deepEqual(roomPayload({ size: "large", minutes: 99 }), { ...defaultRoom(), size: "large" });
  assert.deepEqual(setRoomMessage({ name: "A", minutes: 5 }), {
    type: "SetRoom",
    settings: { name: "A", mode: "normal", size: "small", minutes: 5, friendly_fire: true },
  });
});

test("roomLabel matches the one line of the terminal lobby", () => {
  assert.equal(roomLabel({ name: "Lab", size: "medium", minutes: 5 }), "Lab - Normal - 19 x 15 - 5 min");
  assert.equal(roomLabel(undefined), "Normal - 15 x 13 - 3 min");
});

test("f4_us1_as1_a_closed_room_shows_the_creation_menu_and_an_open_room_the_join_screen", () => {
  let s = initialState();
  assert.equal(s.menu, "join", "the join screen is the fallback until the host answers");
  const closed = reduce(s, { type: "RoomInfo", open: false, settings: { name: "", size: "small" }, humans: 0 });
  assert.equal(closed.stage, "idle");
  assert.equal(closed.menu, "room");
  assert.equal(closed.room.open, false);
  const confirmed = reduce(closed, { type: "room-confirm" });
  assert.equal(confirmed.menu, "join");
  const open = reduce(s, { type: "RoomInfo", open: true, settings: { name: "Friday" }, humans: 1 });
  assert.equal(open.menu, "join");
  assert.equal(open.room.settings.name, "Friday");
  s = reduce(closed, { type: "retry" });
  assert.equal(s.menu, "join");
  const late = reduce({ ...s, stage: "lobby" }, { type: "RoomInfo", open: false, settings: {}, humans: 0 });
  assert.equal(late.stage, "lobby", "an answer that arrives after joining changes nothing");
  assert.equal(late.menu, "join");
  assert.equal(reduce(s, { type: "RoomInfo", open: "yes", settings: 5 }).room.open, false);
});

test("f4_us1_as2_the_lobby_shows_the_settings_to_everybody_but_only_the_owner_can_change_them", () => {
  const lobby = (settings) => ({
    slots: [
      { controller: "Remote", name: "Ann", ready: false },
      { controller: "Remote", name: "Bob", ready: false },
      { controller: "Empty", name: "", ready: false },
      { controller: "Empty", name: "", ready: false },
    ],
    settings,
  });
  let s = reduce(initialState(), { type: "Welcome", slot: 1, lobby: lobby({ name: "Friday", size: "medium", minutes: 5 }), looks: [] });
  assert.equal(s.stage, "lobby");
  const bob = roomControls(s);
  assert.equal(bob.summary, "Friday - Normal - 19 x 15 - 5 min");
  assert.equal(bob.enabled, false, "the second player cannot change the room");
  s = reduce({ ...s, slot: 0 }, { type: "Lobby", lobby: lobby({ name: "Renamed", size: "large", minutes: 10 }), looks: [] });
  const ann = roomControls(s);
  assert.equal(ann.enabled, true);
  assert.equal(ann.summary, "Renamed - Normal - 25 x 19 - 10 min", "a lobby message updates the summary at once");
  assert.equal(roomControls({ ...s, stage: "playing" }).enabled, false, "nobody can change it during a match");
  assert.equal(roomControls({ stage: "lobby", lobby: null, slot: null }).enabled, false);
});

test("f4_us1_as3_the_refusals_of_the_host_are_shown_as_notices", () => {
  let s = reduce(initialState(), { type: "Error", code: "NotOwner", message: "Only Ann can change the room settings" });
  assert.equal(s.notice, "Only Ann can change the room settings");
  s = reduce(s, { type: "Error", code: "WrongPhase", message: "the room can only change in the lobby" });
  assert.equal(s.notice, "the room can only change in the lobby");
  s = reduce(s, { type: "Error", code: "BadRoom", message: "the room settings must be an object" });
  assert.equal(s.notice, "the room settings must be an object");
});
