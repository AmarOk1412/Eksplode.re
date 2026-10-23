import test from "node:test";
import assert from "node:assert/strict";
import { AudioEngine, Sequencer, TUNES, buildTune, tuneForStage } from "../assets/audio.js";
import { fakeAudio, fakeStorage, fakeTimer } from "./audio-helper.mjs";

const pitchClass = (hz) => ((Math.round(69 + 12 * Math.log2(hz / 440)) % 12) + 12) % 12;
const C_PENTATONIC = new Set([0, 2, 4, 7, 9]);
const A_MINOR = new Set([9, 11, 0, 2, 4, 5, 7]);

test("the tunes are built from fixed seeds and are the same on every run", () => {
  for (const name of ["lobby", "match"]) {
    const a = buildTune(name);
    const b = buildTune(name);
    assert.deepEqual(a, b, name);
    assert.ok(a.notes.length > 8);
  }
  assert.notDeepEqual(buildTune("lobby").notes, buildTune("match").notes);
  assert.equal(buildTune("nope"), null);
  assert.deepEqual(Object.keys(TUNES).sort(), ["lobby", "match"]);
});

test("the lobby tune is a 96 BPM C major pentatonic arpeggio over a 4-bar bass", () => {
  const t = buildTune("lobby");
  assert.equal(t.bpm, 96);
  assert.equal(t.loopSteps, 32, "four bars of eight eighth-notes");
  const lead = t.notes.filter((n) => n.voice === "lead");
  const bass = t.notes.filter((n) => n.voice === "bass");
  const tick = t.notes.filter((n) => n.voice === "tick");
  assert.ok(lead.length >= 16, "an arpeggio");
  assert.equal(bass.length, 4, "one bass note per bar");
  assert.ok(tick.length >= 8, "a light noise tick");
  for (const n of [...lead, ...bass]) assert.ok(C_PENTATONIC.has(pitchClass(n.freq)), `${n.voice} ${n.freq}`);
  assert.ok(lead.every((n) => n.freq > 200 && n.freq < 1400));
  assert.ok(bass.every((n) => n.freq < 150));
  assert.deepEqual(bass.map((n) => n.step), [0, 8, 16, 24]);
});

test("the match tune is a 144 BPM A natural minor tune with a driving 8th-note bass", () => {
  const t = buildTune("match");
  assert.equal(t.bpm, 144);
  assert.equal(t.loopSteps, 16, "a two-bar loop");
  const lead = t.notes.filter((n) => n.voice === "lead");
  const bass = t.notes.filter((n) => n.voice === "bass");
  const hat = t.notes.filter((n) => n.voice === "hat");
  assert.equal(bass.length, 16, "a bass note on every eighth");
  assert.ok(lead.length >= 6 && lead.length <= 16);
  assert.ok(hat.length >= 8, "a noise hi-hat");
  for (const n of [...lead, ...bass]) assert.ok(A_MINOR.has(pitchClass(n.freq)), `${n.voice} ${n.freq}`);
  assert.equal(pitchClass(bass[0].freq), 9, "it starts on A");
});

test("the notes stay within the loop", () => {
  for (const name of ["lobby", "match"]) {
    const t = buildTune(name);
    for (const n of t.notes) {
      assert.ok(Number.isInteger(n.step) && n.step >= 0 && n.step < t.loopSteps);
      assert.ok(n.steps >= 1 && n.gain > 0 && n.gain <= 1);
    }
  }
});

function startSequencer(name, startAt = 0) {
  const fake = fakeAudio();
  const timer = fakeTimer();
  const ctx = new fake.AudioContext();
  ctx.currentTime = startAt;
  const out = ctx.createGain();
  const seq = new Sequencer(ctx, out, buildTune(name), timer);
  return { fake, timer, ctx, seq, out };
}

test("f3_us3_as1_the_sequencer_schedules_notes_with_a_25_ms_timer_and_120_ms_lookahead", () => {
  const { fake, timer, ctx, seq } = startSequencer("lobby");
  const tune = buildTune("lobby");
  seq.start();
  assert.equal(timer.intervals.size, 1);
  assert.equal([...timer.intervals.values()][0].ms, 25);
  const step = 60 / tune.bpm / 2;
  const first = 0.05;
  const expected = [];
  const total = tune.loopSteps * 2 + 3; // a little more than two loops
  for (let s = 0; s < total; s += 1) {
    for (const n of tune.notes.filter((x) => x.step === s % tune.loopSteps)) {
      expected.push({ time: first + s * step, voice: n.voice, freq: n.freq });
    }
  }
  const end = first + total * step;
  let t = 0;
  while (t < end - 0.2) {
    ctx.currentTime = t;
    seq.tick();
    for (const o of fake.log.oscillators) {
      assert.ok(o.startedAt <= t + 0.12 + 1e-9, `scheduled ${o.startedAt} at ${t}`);
    }
    t += 0.025;
  }
  // every expected tonal note was scheduled exactly once, at the right time
  const tonal = expected.filter((e) => e.voice !== "tick" && e.voice !== "hat");
  const scheduled = fake.log.oscillators.map((o) => ({
    time: o.startedAt,
    freq: o.frequency.events.find((e) => e[0] === "set")?.[1] ?? o.frequency.value,
  }));
  for (const e of tonal.filter((x) => x.time < end - 0.5)) {
    const hits = scheduled.filter((s) => Math.abs(s.time - e.time) < 1e-6 && Math.abs(s.freq - e.freq) < 1e-6);
    assert.equal(hits.length, 1, `${e.voice} ${e.freq} at ${e.time}`);
  }
  // and the tune loops: the second pass is the first one, later
  const loopLength = tune.loopSteps * step;
  const firstPass = scheduled.filter((s) => s.time < first + loopLength - 1e-9);
  for (const s of firstPass) {
    assert.ok(
      scheduled.some((x) => Math.abs(x.time - (s.time + loopLength)) < 1e-6 && Math.abs(x.freq - s.freq) < 1e-6),
      `no repeat of ${s.freq} at ${s.time}`,
    );
  }
});

test("stopping the sequencer clears its timer and schedules nothing more", () => {
  const { fake, timer, ctx, seq } = startSequencer("match");
  seq.start();
  seq.stop();
  assert.equal(timer.intervals.size, 0);
  const n = fake.log.oscillators.length;
  ctx.currentTime = 5;
  seq.tick();
  assert.equal(fake.log.oscillators.length, n);
});

function engineWithTimer() {
  const fake = fakeAudio();
  const timer = fakeTimer();
  const engine = new AudioEngine({ AudioContext: fake.AudioContext, storage: fakeStorage(), timer });
  return { engine, fake, timer };
}

test("switching tunes fades the old one out and the new one in over 300 ms", () => {
  const { engine, fake, timer } = engineWithTimer();
  engine.unlock();
  const ctx = fake.log.contexts[0];
  ctx.currentTime = 10;
  engine.setTune("lobby");
  const lobby = engine.sequencers.get("lobby");
  assert.deepEqual(lobby.gain.gain.events.at(-1), ["linear", 1, 10 + 0.3], "fades in");
  assert.equal(engine.tune, "lobby");
  engine.setTune("lobby");
  assert.equal(engine.sequencers.size, 1, "the same tune again changes nothing");
  ctx.currentTime = 20;
  engine.setTune("match");
  const match = engine.sequencers.get("match");
  assert.deepEqual(lobby.gain.gain.events.at(-1), ["linear", 0, 20 + 0.3], "the old fades out");
  assert.deepEqual(match.gain.gain.events.at(-1), ["linear", 1, 20 + 0.3], "the new fades in");
  assert.equal(engine.sequencers.size, 2, "both play while they cross-fade");
  const delays = [...timer.timeouts.values()].map((t) => t.ms);
  assert.ok(delays.some((ms) => ms >= 300 && ms <= 350), `a stop is scheduled after the fade: ${delays}`);
  timer.runTimeouts();
  assert.equal(engine.sequencers.has("lobby"), false, "the old tune is gone after the fade");
  assert.equal(engine.sequencers.has("match"), true);
  engine.setTune(null);
  timer.runTimeouts();
  assert.equal(engine.sequencers.size, 0);
  assert.equal(engine.tune, null);
});

test("a tune asked for before the first gesture starts with the context", () => {
  const { engine, fake } = engineWithTimer();
  engine.setTune("lobby");
  assert.equal(fake.log.contexts.length, 0);
  engine.unlock();
  assert.equal(engine.sequencers.has("lobby"), true);
  assert.equal(engine.tune, "lobby");
});

test("a hidden page keeps the music out of the way and a shown page brings it back", () => {
  const { engine, fake } = engineWithTimer();
  engine.unlock();
  engine.setTune("match");
  const ctx = fake.log.contexts[0];
  engine.setHidden(true);
  assert.equal(ctx.state, "suspended");
  assert.equal(engine.sequencers.has("match"), true, "the tune is remembered");
  engine.setHidden(false);
  assert.equal(ctx.state, "running");
});

test("f3_us3_as1_the_lobby_tune_plays_in_the_lobby_and_on_the_join_screen_the_match_tune_in_a_match", () => {
  for (const stage of ["idle", "connecting", "joining", "lobby"]) assert.equal(tuneForStage(stage), "lobby", stage);
  assert.equal(tuneForStage("playing"), "match");
  assert.equal(tuneForStage("closed"), null);
  assert.equal(tuneForStage(undefined), "lobby");
});
