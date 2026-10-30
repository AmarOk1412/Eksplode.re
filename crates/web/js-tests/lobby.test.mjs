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
  // 011: the password is set or removed with the configuration panel's single Save
  assert.match(html, /id="room-config-save"/);
  assert.match(html, /<input[^>]*id="lobby-password-remove"[^>]*type="checkbox"/);
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

// ---- 011 US2: per-seat controls and the two-button action row ----
import { lobbyActions } from "../assets/app.js";

const lobbyHtml = () => {
  const html = readFileSync(new URL("../assets/index.html", import.meta.url), "utf8");
  return /<section[^>]*id="lobby-panel"[^>]*>([\s\S]*?)<\/section>/.exec(html)?.[1] ?? "";
};
const owned = (slots) => ({ owner_slot: 0, slots, settings: { mode: "normal" } });
const members = [{ membership_id: "m-ann", slot: 0 }, { membership_id: "m-bob", slot: 1 }];

test("f11_us2_as2_the_owner_sees_a_cross_on_every_other_occupied_seat", () => {
  const lobby = owned([slot("Remote", "Ann"), slot("Remote", "Bob"), slot("Bot"), slot("Empty")]);
  const cards = lobbyCards(lobby, 0, { capacity: 4, memberships: members });
  assert.equal(cards[0].remove, null, "the owner does not kick themselves");
  assert.deepEqual(cards[1].remove, { type: "RemovePlayer", membership_id: "m-bob" });
  assert.deepEqual(cards[2].remove, { type: "RemoveBot", slot: 2 });
  assert.equal(cards[3].remove, null, "an empty seat has no cross");
});

test("f11_us2_as3_a_non_owner_sees_no_cross_at_all", () => {
  const lobby = owned([slot("Remote", "Ann"), slot("Remote", "Bob"), slot("Bot"), slot("Bot")]);
  const cards = lobbyCards(lobby, 1, { capacity: 4, memberships: members });
  assert.ok(cards.every((c) => c.remove === null));
});

test("f11_us2_as4_every_empty_seat_shows_plus_for_any_session", () => {
  const lobby = owned([slot("Remote", "Ann"), slot("Empty"), slot("Bot"), slot("Empty")]);
  for (const you of [0, 1, null]) {
    const cards = lobbyCards(lobby, you, { capacity: 4, memberships: members });
    assert.deepEqual(cards.map((c) => c.add), [null, { type: "AddBot", slot: 1 }, null, { type: "AddBot", slot: 3 }]);
  }
  const small = lobbyCards(owned([slot("Remote", "Ann"), slot("Empty"), slot("Empty"), slot("Empty")]), 0, { capacity: 2 });
  assert.deepEqual(small.map((c) => c.add), [null, { type: "AddBot", slot: 1 }, null, null], "seats past the capacity are closed");
  assert.deepEqual(small.map((c) => c.closed), [false, false, true, true]);
});

test("f11_us2_as8_the_action_row_is_exactly_ready_then_start_for_the_owner", () => {
  const lobby = owned([slot("Remote", "Ann"), slot("Remote", "Bob", true), slot("Bot"), slot("Bot")]);
  const owner = lobbyActions({ stage: "lobby", slot: 0, lobby });
  assert.deepEqual(owner.map((a) => [a.id, a.label]), [["ready", "Ready"], ["start", "Start"]]);
  assert.deepEqual(owner.map((a) => a.visible), [true, true]);
  const other = lobbyActions({ stage: "lobby", slot: 1, lobby });
  assert.deepEqual(other.map((a) => [a.id, a.visible]), [["ready", true], ["start", false]]);
  assert.equal(other[0].pressed, true, "Ready shows the sender's own state");
  assert.deepEqual(owner[0].message, { type: "SetReady", ready: true });
  assert.deepEqual(other[0].message, { type: "SetReady", ready: false });

  const panel = lobbyHtml();
  const row = /<div class="buttons lobby-actions"[^>]*>([\s\S]*?)<\/div>/.exec(panel)?.[1] ?? "";
  assert.deepEqual([...row.matchAll(/<button[^>]*id="([^"]+)"/g)].map((m) => m[1]), ["ready", "start"]);
});

test("f11_us2_as9_add_bot_remove_bot_and_change_look_buttons_are_gone", () => {
  const panel = lobbyHtml();
  for (const id of ["add-bot", "remove-bot", "change-look"]) assert.doesNotMatch(panel, new RegExp(`id="${id}"`), id);
  assert.doesNotMatch(panel, />\s*(Add bot|Remove bot|Change look)\s*</);
});
