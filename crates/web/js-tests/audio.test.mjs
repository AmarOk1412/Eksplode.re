import test from "node:test";
import assert from "node:assert/strict";
import {
  AudioEngine,
  DEFAULT_SETTINGS,
  STORAGE_KEY,
  effectiveGain,
  loadSettings,
  sanitizeSettings,
  saveSettings,
} from "../assets/audio.js";
import { fakeAudio, fakeStorage, fakeTarget, fakeTimer } from "./audio-helper.mjs";

function make(options = {}) {
  const fake = fakeAudio({ resumeFailures: options.resumeFailures });
  const storage = options.storage ?? fakeStorage();
  const timer = fakeTimer();
  const engine = new AudioEngine({ AudioContext: fake.AudioContext, storage, timer, ...options });
  return { engine, fake, storage, timer, log: fake.log };
}

test("f3_us3_as5_no_audio_context_exists_before_the_first_gesture", () => {
  for (const gesture of ["keydown", "pointerdown", "touchstart"]) {
    const { engine, log } = make();
    const target = fakeTarget();
    engine.attach(target);
    assert.equal(log.contexts.length, 0, "nothing is created when the page loads");
    engine.play("click");
    engine.setTune("lobby");
    engine.handleEvents([{ BombExploded: { owner: 0, pos: { x: 1, y: 1 } } }]);
    assert.equal(log.contexts.length, 0, "playing before a gesture is silent and harmless");
    target.fire(gesture);
    assert.equal(log.contexts.length, 1, `${gesture} creates the context`);
    target.fire("keydown");
    assert.equal(log.contexts.length, 1, "only one context");
    assert.equal(target.count(), 3, "later gestures can recover a suspended context");
  }
});

test("a later user gesture retries a rejected AudioContext resume", () => {
  const { engine, log } = make({ resumeFailures: 1 });
  const target = fakeTarget();
  engine.attach(target);
  target.fire("pointerdown");
  assert.equal(log.contexts.length, 1);
  assert.equal(log.contexts[0].state, "suspended");
  target.fire("keydown");
  assert.equal(log.contexts.length, 1, "retry reuses the same context");
  assert.equal(log.resumeAttempts, 2);
  assert.equal(log.contexts[0].state, "running");
});

test("a missing or throwing AudioContext makes every call a no-op", () => {
  class Throwing {
    constructor() {
      throw new Error("blocked");
    }
  }
  for (const AudioContext of [undefined, Throwing]) {
    const engine = new AudioEngine({ AudioContext, storage: fakeStorage(), timer: fakeTimer() });
    const target = fakeTarget();
    engine.attach(target);
    assert.doesNotThrow(() => {
      target.fire("keydown");
      engine.play("explosion");
      engine.handleEvents([{ RoundEnded: "Draw" }]);
      engine.setTune("match");
      engine.setSettings({ muted: true });
      engine.setHidden(true);
      engine.setHidden(false);
      engine.testSound();
    });
    assert.equal(engine.started, false);
  }
});

test("there are three buses: master to the destination, music and effects into master", () => {
  const { engine, log } = make();
  engine.unlock();
  const { master, music, effects } = engine.buses;
  const ctx = log.contexts[0];
  assert.ok(master.outputs.includes(ctx.destination));
  assert.ok(music.outputs.includes(master));
  assert.ok(effects.outputs.includes(master));
  assert.equal(music.gain.value, DEFAULT_SETTINGS.music);
  assert.equal(effects.gain.value, DEFAULT_SETTINGS.effects);
});

test("f3_us3_as3_volume_changes_apply_at_once", () => {
  const { engine, log } = make();
  engine.unlock();
  const ctx = log.contexts[0];
  ctx.currentTime = 4;
  engine.setSettings({ music: 0.5 });
  const music = engine.buses.music.gain;
  assert.equal(music.value, 0.5);
  assert.deepEqual(music.events.at(-1), ["linear", 0.5, 4 + 0.05], "a 50 ms ramp");
  engine.setSettings({ effects: 0.1 });
  assert.equal(engine.buses.effects.gain.value, 0.1);
  assert.equal(engine.buses.master.gain.value, 1, "master stays open");
  engine.setSettings({ muted: true });
  assert.equal(engine.buses.music.gain.value, 0);
  assert.equal(engine.buses.effects.gain.value, 0);
  assert.equal(engine.settings.music, 0.5, "the levels are remembered while muted");
  engine.setSettings({ muted: false });
  assert.equal(engine.buses.music.gain.value, 0.5);
  assert.equal(engine.buses.effects.gain.value, 0.1);
});

test("the effective gain is zero when muted and the clamped level otherwise", () => {
  assert.equal(effectiveGain(0.4, true), 0);
  assert.equal(effectiveGain(0.4, false), 0.4);
  assert.equal(effectiveGain(7, false), 1);
  assert.equal(effectiveGain(-2, false), 0);
  assert.equal(effectiveGain(Number.NaN, false), 0);
});

test("settings are clamped, with defaults 0.35 / 0.7 / not muted", () => {
  assert.deepEqual(DEFAULT_SETTINGS, { music: 0.35, effects: 0.7, muted: false });
  assert.deepEqual(sanitizeSettings(undefined), DEFAULT_SETTINGS);
  assert.deepEqual(sanitizeSettings("loud"), DEFAULT_SETTINGS);
  assert.deepEqual(sanitizeSettings({ music: 5, effects: -1, muted: true }), { music: 1, effects: 0, muted: true });
  assert.deepEqual(sanitizeSettings({ music: "x", effects: Number.NaN, muted: "yes" }), DEFAULT_SETTINGS);
  assert.deepEqual(sanitizeSettings({ music: 0.2 }), { music: 0.2, effects: 0.7, muted: false });
});

test("f3_us3_as4_settings_survive_a_reload", () => {
  const storage = fakeStorage();
  const first = make({ storage });
  first.engine.setSettings({ music: 0.8, effects: 0.2, muted: true });
  assert.equal(STORAGE_KEY, "eksplode.re.audio.v1");
  assert.deepEqual(JSON.parse(storage.getItem(STORAGE_KEY)), { music: 0.8, effects: 0.2, muted: true });
  const second = make({ storage });
  assert.deepEqual(second.engine.settings, { music: 0.8, effects: 0.2, muted: true });
  second.engine.unlock();
  assert.equal(second.engine.buses.music.gain.value, 0, "muted survives too");
});

test("audio settings migrate from the Bomberust storage key", () => {
  const storage = fakeStorage();
  storage.setItem("bomberust.audio.v1", JSON.stringify({ music: 0.6, effects: 0.4, muted: false }));
  assert.deepEqual(loadSettings(storage), { music: 0.6, effects: 0.4, muted: false });
});

test("corrupt or unavailable storage gives the defaults and saving never throws", () => {
  const corrupt = fakeStorage();
  corrupt.setItem(STORAGE_KEY, "{not json");
  assert.deepEqual(loadSettings(corrupt), DEFAULT_SETTINGS);
  const odd = fakeStorage();
  odd.setItem(STORAGE_KEY, JSON.stringify({ music: 3, effects: "a" }));
  assert.deepEqual(loadSettings(odd), { music: 1, effects: 0.7, muted: false });
  const broken = fakeStorage({ broken: true });
  assert.deepEqual(loadSettings(broken), DEFAULT_SETTINGS);
  assert.doesNotThrow(() => saveSettings(broken, DEFAULT_SETTINGS));
  assert.deepEqual(loadSettings(undefined), DEFAULT_SETTINGS);
  const { engine } = make({ storage: broken });
  assert.doesNotThrow(() => engine.setSettings({ music: 0.1 }));
  assert.equal(engine.settings.music, 0.1);
});

test("f3_us3_as6_a_hidden_page_suspends_the_audio_and_a_visible_one_resumes_it", () => {
  const { engine, log } = make();
  assert.doesNotThrow(() => engine.setHidden(true), "before the context exists");
  engine.setHidden(false);
  engine.unlock();
  const ctx = log.contexts[0];
  assert.equal(ctx.state, "running");
  const suspended = log.suspended;
  engine.setHidden(true);
  assert.equal(ctx.state, "suspended");
  assert.equal(log.suspended, suspended + 1);
  const resumed = log.resumed;
  engine.setHidden(false);
  assert.equal(ctx.state, "running");
  assert.equal(log.resumed, resumed + 1);
});

test("a page that is hidden when the first gesture arrives stays suspended until it is shown", () => {
  const { engine, log } = make();
  engine.setHidden(true);
  engine.unlock();
  assert.equal(log.contexts[0].state, "suspended");
  engine.setHidden(false);
  assert.equal(log.contexts[0].state, "running");
});
