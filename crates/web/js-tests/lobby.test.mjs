import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { lobbyCards, Animator, initialState, reduce } from "../assets/app.js";
import { defaultRoom, loadRoom, roomControls, saveRoom } from "../assets/room.js";

const slot = (controller, name = "", ready = false) => ({ controller, name, ready });

test("f3_us1_as3_each_occupied_slot_is_a_card_with_banner_and_name", () => {
  const lobby = { slots: [slot("Local", "Ann", true), slot("Remote", "Bob"), slot("Bot"), slot("Empty")] };
  const cards = lobbyCards(lobby, 1);
  assert.equal(cards.length, 4, "one card per slot");
  assert.deepEqual(
    cards.map((c) => c.banner),
    ["PLAYER 1", "PLAYER 2", "PLAYER 3", "EMPTY"],
  );
  assert.equal(cards[0].caption, "Ann (owner, ready)");
  assert.equal(cards[1].caption, "Bob (you)");
  assert.equal(cards[2].name, "bot", "bots are labelled bot");
  assert.equal(cards[2].caption, "bot");
  assert.equal(cards[3].empty, true);
  assert.equal(cards[3].caption, "empty");
  assert.deepEqual(cards[0].marks, ["owner", "ready"]);
  assert.deepEqual(cards[1].marks, ["you"]);
  assert.deepEqual(cards.map((c) => c.empty), [false, false, false, true]);
});

test("a card without a name falls back to the player number and an empty lobby has no cards", () => {
  assert.equal(lobbyCards({ slots: [slot("Remote", "")] }, 0)[0].name, "player 1");
  assert.deepEqual(lobbyCards(null, 0), []);
  assert.deepEqual(lobbyCards({ slots: [] }, null), []);
});

function fakeFrames() {
  const pending = new Map();
  let id = 0;
  return {
    raf: (fn) => {
      id += 1;
      pending.set(id, fn);
      return id;
    },
    caf: (n) => pending.delete(n),
    pending,
    tick(ms) {
      const [key, fn] = [...pending][0];
      pending.delete(key);
      fn(ms);
    },
  };
}

test("f3_us1_as3_the_animation_runs_only_while_the_lobby_is_visible", () => {
  const frames = fakeFrames();
  const seen = [];
  const a = new Animator(frames.raf, frames.caf, (seconds) => seen.push(seconds));
  assert.equal(frames.pending.size, 0, "nothing runs before it is started");
  a.setActive(true);
  a.setActive(true);
  assert.equal(frames.pending.size, 1, "starting twice schedules one frame");
  frames.tick(1000);
  frames.tick(1500);
  assert.deepEqual(seen, [0, 0.5], "seconds count from the first frame");
  assert.equal(frames.pending.size, 1);
  a.setActive(false);
  assert.equal(frames.pending.size, 0, "stopping cancels the next frame");
  a.setActive(false);
  a.setActive(true);
  frames.tick(9000);
  assert.deepEqual(seen.slice(2), [0], "a new run starts from zero");
});

test("f4_us3_as1_in_team_mode_each_card_shows_its_team", () => {
  const lobby = {
    slots: [slot("Local", "Ann"), slot("Bot"), slot("Remote", "Cy"), slot("Bot")],
    settings: { name: "Friday", mode: "team", size: "small", minutes: 3, friendly_fire: true },
  };
  const cards = lobbyCards(lobby, 2);
  assert.deepEqual(cards.map((c) => c.team), ["A", "A", "B", "B"], "slots 1 and 2 against 3 and 4");
  assert.ok(cards[0].marks.includes("team A") && cards[3].marks.includes("team B"));
  assert.equal(cards[2].caption, "Cy (you, team B)");
  // outside Team mode there are no teams
  const normal = lobbyCards({ ...lobby, settings: { ...lobby.settings, mode: "normal" } }, 2);
  assert.ok(normal.every((c) => c.team === null && !c.marks.some((m) => m.startsWith("team"))));
  assert.deepEqual(lobbyCards({ slots: lobby.slots }, 0).map((c) => c.team), [null, null, null, null], "an old host sends no settings");
});

test("us2_as2_room_password_inputs_are_masked_and_passwords_never_enter_room_defaults", () => {
  const html = readFileSync(new URL("../assets/index.html", import.meta.url), "utf8");
  assert.match(html, /<input[^>]*id="room-password"[^>]*type="password"/);
  assert.match(html, /<input[^>]*id="join-password"[^>]*type="password"/);
  assert.match(html, /<select[^>]*id="lobby-capacity"/);
  assert.match(html, /<input[^>]*id="lobby-password"[^>]*type="password"/);
  assert.match(html, /id="lobby-password-set"/);
  assert.match(html, /id="lobby-password-remove"/);
  const values = new Map();
  const storage = { getItem: (key) => values.get(key) ?? null, setItem: (key, value) => values.set(key, value) };
  saveRoom(storage, { ...defaultRoom(), password: "password-canary" });
  assert.equal(loadRoom(storage).password, undefined);
  assert.ok(![...values.values()].some((value) => value.includes("password-canary")));
});

test("us2_as6_room_updates_refresh_explicit_owner_capacity_and_protection_controls", () => {
  const lobby = (ownerSlot, name) => ({
    owner_slot: ownerSlot,
    slots: [slot("Remote", "Ann"), slot("Remote", "Bob"), slot("Empty"), slot("Empty")],
    settings: { name, mode: "normal", size: "small", minutes: 3, friendly_fire: true },
  });
  let state = reduce(initialState(), {
    type: "Welcome", slot: 0, owner_slot: 0, capacity: 2, password_required: true,
    lobby: lobby(0, "Protected"), looks: [],
  });
  let controls = roomControls(state);
  assert.equal(controls.enabled, true);
  assert.equal(controls.capacity, 2);
  assert.equal(controls.passwordRequired, true);

  state = reduce(state, {
    type: "RoomUpdated", owner_slot: 1, capacity: 3, password_required: false,
    lobby: lobby(1, "Transferred"), looks: [],
  });
  controls = roomControls(state);
  assert.equal(controls.enabled, false, "the former owner loses controls immediately");
  assert.equal(controls.capacity, 3);
  assert.equal(controls.passwordRequired, false);
  assert.equal(roomControls({ ...state, slot: 1 }).enabled, true, "the authoritative new owner gains controls");
  assert.equal(roomControls({ ...state, slot: 1, stage: "playing" }).enabled, false);
  assert.equal(reduce(state, { type: "Error", code: "NotOwner", message: "Only Bob can manage this room" }).notice,
    "Only Bob can manage this room");
});
