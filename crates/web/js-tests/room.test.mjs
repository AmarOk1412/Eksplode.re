import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
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
  roomProtocolMessage,
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

test("v2 presentation messages scope actions without falling back to legacy Join", () => {
  const state = { room_id: "room", revision: 3, capacity: 4 };
  assert.deepEqual(roomProtocolMessage(state, { type: "Start" }, "start"), { type: "Start", room_id: "room", request_id: "start" });
  assert.deepEqual(roomProtocolMessage(state, { type: "Input", tick: 2, mv: null, bomb: false, punch: false }, "ignored"),
    { type: "Input", room_id: "room", tick: 2, mv: null, bomb: false, punch: false });
  assert.deepEqual(roomProtocolMessage({}, { type: "Hello", version: 2 }, "ignored"), { type: "Hello", version: 2 });
  assert.equal(roomProtocolMessage({}, { type: "Start" }, "start"), null);
  assert.equal(roomProtocolMessage(state, { type: "Join", name: "Ann" }, "join"), null);
  const update = roomProtocolMessage(state, setRoomMessage(defaultRoom()), "settings");
  assert.equal(update.expected_revision, 3);
  assert.equal(update.capacity, 4);
  assert.deepEqual(roomProtocolMessage({}, { type:"CreateRoom", name:"Owner", settings:defaultRoom(), capacity:2,
    password:"create-canary" }, "create"), { type:"CreateRoom", name:"Owner", settings:defaultRoom(),
    capacity:2, password:"create-canary", request_id:"create" });
  assert.deepEqual(roomProtocolMessage({}, { type:"JoinRoom", name:"Member", room_id:"r", server_epoch:"e",
    password:"join-canary" }, "join"), { type:"JoinRoom", name:"Member", room_id:"r", server_epoch:"e",
    password:"join-canary", request_id:"join" });
  assert.deepEqual(roomProtocolMessage(state, { type:"SetRoom", settings:defaultRoom(), capacity:2,
    password:"replace-canary" }, "replace"), { type:"SetRoom", room_id:"room", request_id:"replace",
    settings:defaultRoom(), capacity:2, password:"replace-canary", expected_revision:3 });
});

test("the nested join panel retains horizontal centering independently of the app grid", () => {
  const css = readFileSync(new URL("../assets/style.css", import.meta.url), "utf8");
  const rule = /^#join-panel\s*\{([^}]*)\}/m.exec(css)?.[1] ?? "";
  assert.match(rule, /margin-inline:\s*auto\s*;/);
  const app = /^#app\s*\{([^}]*)\}/m.exec(css)?.[1] ?? "";
  assert.match(app, /grid-template-columns:\s*minmax\(0,\s*1fr\)\s*;/);
});

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

test("room settings migrate from the Bomberust storage key", () => {
  const storage = memory();
  const room = { ...defaultRoom(), name: "Old room", size: "large" };
  storage.data.set("bomberust.room.v1", JSON.stringify(room));
  assert.deepEqual(loadRoom(storage), room);
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
  assert.equal(s.menu, "main", "the main menu is the fallback until the host answers");
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
  assert.equal(s.menu, "main");
  const late = reduce({ ...s, stage: "lobby" }, { type: "RoomInfo", open: false, settings: {}, humans: 0 });
  assert.equal(late.stage, "lobby", "an answer that arrives after joining changes nothing");
  assert.equal(late.menu, "main");
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

// ---- 011 US1: Join a game opens the room list ----
import { screens, MAIN_MENU_ACTIONS } from "../assets/app.js";
import { listRoomsMessage } from "../assets/room.js";

const helloV2 = { type: "Hello", version: 2, server_id: "alpha", server_epoch: "epoch", capabilities: ["rooms", "directory"] };

test("f11_us1_as3_join_a_game_shows_the_room_list_with_refresh_and_new_room", () => {
  const join = MAIN_MENU_ACTIONS.find((a) => a.label === "Join a game");
  const state = reduce(reduce(initialState(), helloV2), join.event);
  const visible = screens(state);
  assert.equal(visible.directoryPanel, true);
  assert.equal(visible.mainMenu, false);
  assert.equal(visible.roomPanel, false);
  const html = readFileSync(new URL("../assets/index.html", import.meta.url), "utf8");
  const directory = /<section[^>]*id="directory-panel"[^>]*>([\s\S]*?)<\/section>/.exec(html)?.[1] ?? "";
  assert.match(directory, /id="directory-refresh"[^>]*>Refresh</);
  assert.match(directory, /id="directory-create"[^>]*>New room</);
  const created = reduce(state, { type: "new-room" });
  assert.equal(screens(created).roomPanel, true, "New room opens the creation form");
  assert.equal(screens(reduce(state, { type: "show-main" })).mainMenu, true);
});

test("f11_us1_as4_refresh_requeries_the_rooms_without_leaving_the_list", () => {
  assert.deepEqual(listRoomsMessage(), { type: "ListRooms", scope: "local" });
  assert.deepEqual(roomProtocolMessage({}, listRoomsMessage(), "list-1"), { type: "ListRooms", scope: "local", request_id: "list-1" });
  let state = reduce(reduce(initialState(), helloV2), { type: "show-directory" });
  state = reduce(state, { type: "Rooms", rooms: [], sources: [] });
  assert.equal(state.menu, "directory");
  assert.equal(screens(state).directoryPanel, true, "an empty answer keeps the list (empty state) on screen");
  const room = { server_id: "alpha", server_epoch: "epoch", room_id: "r", name: "Friday", mode: "normal", occupied: 1, capacity: 4, phase: "lobby" };
  state = reduce(state, { type: "Rooms", rooms: [room], sources: [] });
  assert.equal(screens(state).directoryPanel, true);
  assert.equal(state.rooms.length, 1);
});

// ---- 011 US2: the configuration panel and the leave icon ----
import { lobbyHeader } from "../assets/app.js";
import { roomConfigDraft, roomConfigReducer, roomConfigMessage } from "../assets/room.js";

const configLobby = (ownerSlot) => ({
  owner_slot: ownerSlot,
  slots: [{ controller: "Remote", name: "Ann" }, { controller: "Remote", name: "Bob" }, { controller: "Bot" }, { controller: "Empty" }],
  settings: { name: "Friday", mode: "normal", size: "small", minutes: 3, friendly_fire: true },
});
const lobbySection = () => {
  const html = readFileSync(new URL("../assets/index.html", import.meta.url), "utf8");
  return {
    html,
    lobby: /<section[^>]*id="lobby-panel"[^>]*>([\s\S]*?)<\/section>/.exec(html)?.[1] ?? "",
    config: /<section[^>]*id="room-config"[^>]*>([\s\S]*?)<\/section>/.exec(html)?.[1] ?? "",
  };
};

test("f11_us2_as5_as6_the_configuration_icon_is_for_the_owner_only", () => {
  const owner = { stage: "lobby", slot: 0, lobby: configLobby(0), capacity: 4 };
  assert.deepEqual(lobbyHeader(owner), { config: true, leave: true });
  assert.deepEqual(lobbyHeader({ ...owner, slot: 1 }), { config: false, leave: true });
  assert.deepEqual(lobbyHeader({ ...owner, stage: "playing" }).config, false);
  const { lobby, config } = lobbySection();
  const header = /<div class="lobby-header"[^>]*>([\s\S]*?)<\/div>/.exec(lobby)?.[1] ?? "";
  assert.match(header, /id="room-config-open"[^>]*aria-label="Room settings"/);
  assert.match(header, /id="leave-room"[^>]*aria-label="Leave room"/, "FR-013: leave sits beside the configuration icon");
  for (const id of ["lobby-room-name", "lobby-capacity", "lobby-mode", "lobby-size", "lobby-minutes", "lobby-ff", "lobby-password"]) {
    assert.match(config, new RegExp(`id="${id}"`), `${id} lives in the configuration panel`);
    assert.doesNotMatch(lobby, new RegExp(`id="${id}"`), `${id} is not in the lobby body`);
  }
  assert.match(config, /id="room-config-save"/);
  assert.match(config, /id="room-config-cancel"/);
});

test("f11_us2_as5_saving_the_configuration_sends_one_set_room", () => {
  const state = { stage: "lobby", slot: 0, lobby: configLobby(0), capacity: 4, password_required: false };
  let draft = roomConfigDraft(state);
  assert.deepEqual(draft, { settings: { name: "Friday", mode: "normal", size: "small", minutes: 3, friendly_fire: true },
    capacity: 4, password: "", removePassword: false });
  draft = roomConfigReducer(draft, { type: "name", value: "Saturday" });
  draft = roomConfigReducer(draft, { type: "size", value: "large" });
  draft = roomConfigReducer(draft, { type: "capacity", value: "3" });
  draft = roomConfigReducer(draft, { type: "password", value: "hunter22" });
  assert.deepEqual(roomConfigMessage(draft), { type: "SetRoom",
    settings: { name: "Saturday", mode: "normal", size: "large", minutes: 3, friendly_fire: true }, capacity: 3, password: "hunter22" });
  const scoped = roomProtocolMessage({ room_id: "r", revision: 7, capacity: 4 }, roomConfigMessage(draft), "save");
  assert.equal(scoped.expected_revision, 7);
  assert.equal(scoped.capacity, 3);
  // a team room needs four seats; no new password and no removal sends no password field
  let team = roomConfigReducer(roomConfigReducer(roomConfigDraft(state), { type: "capacity", value: 2 }), { type: "mode", value: "team" });
  assert.equal(team.capacity, 4);
  assert.equal(Object.hasOwn(roomConfigMessage(team), "password"), false);
  // removing protection sends an empty password
  team = roomConfigReducer(team, { type: "removePassword", value: true });
  assert.equal(roomConfigMessage(team).password, "");
  // cancel: the draft is dropped and nothing is sent
  assert.equal(roomConfigReducer(team, { type: "cancel" }), null);
  assert.equal(roomConfigMessage(null), null);
});

test("f11_us2_edge_a_bot_filled_room_stays_joinable_until_four_humans_are_in", async () => {
  const { roomJoinable } = await import("../assets/room.js");
  const room = { phase: "lobby", capacity: 4, occupied: 4, bots: 3, connected_humans: 1, reserved_humans: 0 };
  assert.equal(roomJoinable(room), true, "bots give their seat to a joining human");
  assert.equal(roomJoinable({ ...room, bots: 0, connected_humans: 4 }), false);
  assert.equal(roomJoinable({ ...room, bots: 1, connected_humans: 2, reserved_humans: 1 }), true);
  assert.equal(roomJoinable({ ...room, phase: "running" }), false);
  assert.equal(roomJoinable({ phase: "lobby", capacity: 2, occupied: 1 }), true, "an old host without bot counts");
  assert.equal(roomJoinable({ phase: "lobby", capacity: 2, occupied: 2 }), false);
});
