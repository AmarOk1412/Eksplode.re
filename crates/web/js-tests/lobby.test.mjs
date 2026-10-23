import test from "node:test";
import assert from "node:assert/strict";
import { lobbyCards, Animator } from "../assets/app.js";

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
