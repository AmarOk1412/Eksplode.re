import test from "node:test";
import assert from "node:assert/strict";
import { AudioEngine, MAX_SOUNDS_PER_SNAPSHOT, SOUND_IDS, soundsForEvents } from "../assets/audio.js";
import { fakeAudio, fakeStorage, fakeTimer } from "./audio-helper.mjs";

const P = { x: 2, y: 3 };
const ev = {
  placed: { BombPlaced: { player: 0, pos: P } },
  exploded: { BombExploded: { owner: 0, pos: P } },
  punched: { BombPunched: { player: 0, from: P, dir: "Right" } },
  landed: { BombLanded: { pos: P } },
  kicked: { BombKicked: { player: 0, from: P, dir: "Right" } },
  out: { PlayerEliminated: { player: 2 } },
  bonus: { PowerUpCollected: { player: 0, pos: P, kind: "Punch" } },
  malus: { PowerUpCollected: { player: 0, pos: P, kind: "Slow" } },
  win: { RoundEnded: { Winner: 1 } },
  draw: { RoundEnded: "Draw" },
};

const ids = (events) => soundsForEvents(events).map((s) => s.id);

test("f3_us3_as2_every_listed_event_has_its_own_sound", () => {
  assert.deepEqual(ids([ev.placed]), ["bombPlace"]);
  assert.deepEqual(ids([ev.exploded]), ["explosion"]);
  assert.deepEqual(ids([ev.punched]), ["punch"]);
  assert.deepEqual(ids([ev.landed]), ["land"]);
  assert.deepEqual(ids([ev.kicked]), ["kick"]);
  assert.deepEqual(ids([ev.out]), ["eliminated"]);
  assert.deepEqual(ids([ev.win]), ["win"]);
  assert.deepEqual(ids([ev.draw]), ["draw"]);
  for (const kind of ["ExtraBomb", "Range", "Speed", "Punch", "Kick"]) {
    assert.deepEqual(ids([{ PowerUpCollected: { player: 0, pos: P, kind } }]), ["bonus"], kind);
  }
  for (const kind of ["Slow", "HyperSpeed", "SmallBomb"]) {
    assert.deepEqual(ids([{ PowerUpCollected: { player: 0, pos: P, kind } }]), ["malus"], kind);
  }
  assert.equal(new Set(SOUND_IDS).size, SOUND_IDS.length);
  for (const id of ["click", "bombPlace", "explosion", "bonus", "malus", "punch", "kick", "land", "eliminated", "win", "draw"]) {
    assert.ok(SOUND_IDS.includes(id), id);
  }
});

test("events without a sound, odd shapes and non-arrays are ignored", () => {
  const quiet = [
    { Moved: { player: 0, from: P, to: P } },
    { BlockDestroyed: { pos: P } },
    { PowerUpSpawned: { pos: P, kind: "Speed" } },
    { BombHopped: { pos: P } },
    { MalusStarted: { player: 0, kind: "Slow" } },
    "BombPlaced",
    null,
    42,
    {},
  ];
  assert.deepEqual(soundsForEvents(quiet), []);
  assert.deepEqual(soundsForEvents(undefined), []);
  assert.deepEqual(soundsForEvents("nope"), []);
});

test("at most one sound per id per snapshot, quieter explosions when several go off", () => {
  const many = soundsForEvents(Array.from({ length: 9 }, () => ev.exploded));
  assert.equal(many.length, 1);
  assert.ok(many[0].volume < 1 && many[0].volume > 0);
  assert.equal(soundsForEvents([ev.exploded])[0].volume, 1);
  const two = soundsForEvents([ev.exploded, ev.exploded])[0].volume;
  assert.ok(many[0].volume < two && two < 1, "more explosions are quieter each");
  assert.equal(soundsForEvents([ev.placed, ev.placed, ev.placed]).length, 1);
});

test("at most eight sounds start per snapshot", () => {
  const all = Object.values(ev);
  const list = soundsForEvents(all);
  assert.ok(list.length <= MAX_SOUNDS_PER_SNAPSHOT);
  assert.equal(MAX_SOUNDS_PER_SNAPSHOT, 8);
  assert.equal(new Set(list.map((s) => s.id)).size, list.length);
});

// --- recipes -------------------------------------------------------------------------------

function play(id, volume) {
  const fake = fakeAudio();
  const engine = new AudioEngine({ AudioContext: fake.AudioContext, storage: fakeStorage(), timer: fakeTimer() });
  engine.unlock();
  fake.log.contexts[0].currentTime = 10;
  const baseO = fake.log.oscillators.length;
  const baseN = fake.log.noises.length;
  engine.play(id, volume);
  const osc = fake.log.oscillators.slice(baseO).map((o) => {
    const set = o.frequency.events.find((e) => e[0] === "set");
    const ramp = o.frequency.events.find((e) => e[0] === "exp" || e[0] === "linear");
    return {
      type: o.type,
      f0: set ? set[1] : o.frequency.value,
      f1: ramp ? ramp[1] : null,
      start: o.startedAt - 10,
      dur: o.stoppedAt - o.startedAt,
    };
  });
  const noise = fake.log.noises.slice(baseN).map((n) => ({ start: n.startedAt - 10, dur: n.stoppedAt - n.startedAt }));
  return { osc, noise, fake };
}

const near = (a, b, eps = 1e-6) => Math.abs(a - b) <= eps;

test("click is a 40 ms square tick at 1.2 kHz", () => {
  const { osc, noise } = play("click");
  assert.equal(osc.length, 1);
  assert.equal(noise.length, 0);
  assert.equal(osc[0].type, "square");
  assert.equal(osc[0].f0, 1200);
  assert.ok(near(osc[0].dur, 0.04));
});

test("bombPlace is an 80 ms triangle blip falling from 520 to 330 Hz", () => {
  const { osc } = play("bombPlace");
  assert.equal(osc.length, 1);
  assert.deepEqual([osc[0].type, osc[0].f0, osc[0].f1], ["triangle", 520, 330]);
  assert.ok(near(osc[0].dur, 0.08));
});

test("explosion is 600 ms of low-passed noise falling from 3000 to 120 Hz plus a 60 Hz thump", () => {
  const { osc, noise, fake } = play("explosion");
  assert.equal(noise.length, 1);
  assert.ok(near(noise[0].dur, 0.6));
  const filter = fake.log.nodes.find((n) => n.kind === "filter" && n.type === "lowpass");
  assert.ok(filter, "a low-pass filter");
  const sweep = filter.frequency.events.filter((e) => e[0] !== "cancel").map((e) => e[1]);
  assert.equal(sweep[0], 3000);
  assert.equal(sweep.at(-1), 120);
  assert.equal(osc.length, 1);
  assert.deepEqual([osc[0].type, osc[0].f0], ["sine", 60]);
});

test("bonus is a rising three-note arpeggio C5 E5 G5 in 60 ms notes", () => {
  const { osc } = play("bonus");
  assert.equal(osc.length, 3);
  const hz = osc.map((o) => Math.round(o.f0 * 100) / 100);
  assert.deepEqual(hz, [523.25, 659.25, 783.99]);
  osc.forEach((o, i) => {
    assert.ok(near(o.dur, 0.06), `note ${i}`);
    assert.ok(near(o.start, 0.06 * i + osc[0].start), `start ${i}`);
  });
});

test("malus is a 250 ms falling sawtooth buzz from 400 to 120 Hz", () => {
  const { osc } = play("malus");
  assert.equal(osc.length, 1);
  assert.deepEqual([osc[0].type, osc[0].f0, osc[0].f1], ["sawtooth", 400, 120]);
  assert.ok(near(osc[0].dur, 0.25));
});

test("punch is a 120 ms noise burst with a pitch drop from 700 to 200 Hz", () => {
  const { osc, noise } = play("punch");
  assert.equal(noise.length, 1);
  assert.ok(near(noise[0].dur, 0.12));
  assert.equal(osc.length, 1);
  assert.deepEqual([osc[0].f0, osc[0].f1], [700, 200]);
  assert.ok(near(osc[0].dur, 0.12));
});

test("kick is a 110 ms triangle thud falling from 260 to 90 Hz", () => {
  const { osc, noise } = play("kick");
  assert.equal(noise.length, 0);
  assert.equal(osc.length, 1);
  assert.deepEqual([osc[0].type, osc[0].f0, osc[0].f1], ["triangle", 260, 90]);
  assert.ok(near(osc[0].dur, 0.11));
});

test("land is a 90 ms sine thud at 140 Hz", () => {
  const { osc } = play("land");
  assert.equal(osc.length, 1);
  assert.deepEqual([osc[0].type, osc[0].f0], ["sine", 140]);
  assert.ok(near(osc[0].dur, 0.09));
});

test("eliminated is four descending notes of 100 ms", () => {
  const { osc } = play("eliminated");
  assert.equal(osc.length, 4);
  for (let i = 1; i < 4; i += 1) {
    assert.ok(osc[i].f0 < osc[i - 1].f0, "descending");
    assert.ok(near(osc[i].start - osc[i - 1].start, 0.1), "every 100 ms");
  }
  assert.ok(osc.every((o) => near(o.dur, 0.1)));
});

test("win is a rising major fanfare and draw is two low tones", () => {
  const win = play("win").osc;
  assert.ok(win.length >= 4);
  for (let i = 1; i < win.length; i += 1) assert.ok(win[i].f0 > win[i - 1].f0, "rising");
  const draw = play("draw").osc;
  assert.equal(draw.length, 2);
  assert.ok(draw.every((o) => o.f0 < 300), "low");
});

test("every sound id has its own recipe", () => {
  const seen = new Set();
  for (const id of SOUND_IDS) {
    const { osc, noise } = play(id);
    assert.ok(osc.length + noise.length > 0, `${id} makes a sound`);
    seen.add(JSON.stringify({ osc, noise }));
  }
  assert.equal(seen.size, SOUND_IDS.length);
});

test("an unknown id and a bad volume are harmless", () => {
  const { osc, noise } = play("nope");
  assert.equal(osc.length + noise.length, 0);
  assert.doesNotThrow(() => play("click", Number.NaN));
  assert.doesNotThrow(() => play("click", -3));
});

test("the engine plays the sounds of a snapshot's events, capped and without repeats", () => {
  const fake = fakeAudio();
  const engine = new AudioEngine({ AudioContext: fake.AudioContext, storage: fakeStorage(), timer: fakeTimer() });
  engine.unlock();
  const before = fake.log.noises.length;
  engine.handleEvents(Array.from({ length: 12 }, () => ev.exploded));
  assert.equal(fake.log.noises.length - before, 1, "twelve explosions are one sound");
  engine.handleEvents([ev.placed, ev.landed, ev.punched]);
  engine.handleEvents(undefined);
  assert.doesNotThrow(() => engine.handleEvents([null, 3]));
});
