// Generated sound effects, music and volume settings (presentation only, no audio files).
//
// Everything you hear is made at run time from oscillators and noise with the Web Audio API;
// no sample, recording or third-party melody is used, fetched or embedded. See docs/AUDIO.md.
export const MODULE = "audio";

// --- settings -------------------------------------------------------------------------------

export const STORAGE_KEY = "bomberust.audio.v1";
export const DEFAULT_SETTINGS = Object.freeze({ music: 0.35, effects: 0.7, muted: false });
const RAMP_SECONDS = 0.05;
const FADE_SECONDS = 0.3;

const level = (value, fallback) =>
  typeof value === "number" && Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : fallback;

/** A complete, clamped settings object from anything (loose JSON, nothing, garbage). */
export function sanitizeSettings(raw) {
  const o = raw !== null && typeof raw === "object" && !Array.isArray(raw) ? raw : {};
  return {
    music: level(o.music, DEFAULT_SETTINGS.music),
    effects: level(o.effects, DEFAULT_SETTINGS.effects),
    muted: o.muted === true,
  };
}

export function loadSettings(storage) {
  try {
    const text = storage?.getItem(STORAGE_KEY);
    return text ? sanitizeSettings(JSON.parse(text)) : sanitizeSettings(undefined);
  } catch {
    return sanitizeSettings(undefined);
  }
}

export function saveSettings(storage, settings) {
  try {
    storage?.setItem(STORAGE_KEY, JSON.stringify(sanitizeSettings(settings)));
  } catch {
    /* private mode or full storage: the settings just do not persist */
  }
}

/** The gain a bus gets: nothing while muted, otherwise the level. */
export function effectiveGain(value, muted) {
  return muted ? 0 : level(value, 0);
}

// --- sound effects --------------------------------------------------------------------------

export const SOUND_IDS = ["click", "bombPlace", "explosion", "bonus", "malus", "punch", "land", "eliminated", "win", "draw"];
export const MAX_SOUNDS_PER_SNAPSHOT = 8;
const MALUS_KINDS = new Set(["Slow", "HyperSpeed", "SmallBomb"]);

/**
 * The sounds a snapshot's events ask for: at most one per id, at most eight in all; several
 * explosions at once become one quieter sound.
 */
export function soundsForEvents(events) {
  if (!Array.isArray(events)) return [];
  const counts = new Map();
  const add = (id) => counts.set(id, (counts.get(id) ?? 0) + 1);
  for (const e of events) {
    if (e === null || typeof e !== "object" || Array.isArray(e)) continue;
    if ("BombPlaced" in e) add("bombPlace");
    else if ("BombExploded" in e) add("explosion");
    else if ("BombPunched" in e) add("punch");
    else if ("BombLanded" in e) add("land");
    else if ("PlayerEliminated" in e) add("eliminated");
    else if ("PowerUpCollected" in e) add(MALUS_KINDS.has(e.PowerUpCollected?.kind) ? "malus" : "bonus");
    else if ("RoundEnded" in e) {
      const outcome = e.RoundEnded;
      add(outcome !== null && typeof outcome === "object" && "Winner" in outcome ? "win" : "draw");
    }
  }
  return [...counts]
    .slice(0, MAX_SOUNDS_PER_SNAPSHOT)
    .map(([id, n]) => ({ id, volume: id === "explosion" ? 1 / Math.sqrt(n) : 1 }));
}

const hz = (midi) => 440 * 2 ** ((midi - 69) / 12);

/** One enveloped oscillator note; `f1` makes its pitch glide. */
function tone(ctx, out, { type, f0, f1, start, dur, gain, attack = 0 }) {
  const osc = ctx.createOscillator();
  osc.type = type;
  osc.frequency.setValueAtTime(f0, start);
  if (f1 !== undefined && f1 !== f0) osc.frequency.exponentialRampToValueAtTime(f1, start + dur);
  const g = ctx.createGain();
  if (attack > 0) {
    g.gain.setValueAtTime(0.0001, start);
    g.gain.linearRampToValueAtTime(gain, start + attack);
  } else {
    g.gain.setValueAtTime(gain, start);
  }
  g.gain.exponentialRampToValueAtTime(0.0001, start + dur);
  osc.connect(g);
  g.connect(out);
  osc.start(start);
  osc.stop(start + dur);
}

const noiseBuffers = new WeakMap();

/** One second of white noise from a fixed generator, shared by every burst of a context. */
function noiseBuffer(ctx) {
  let buffer = noiseBuffers.get(ctx);
  if (!buffer) {
    const rate = ctx.sampleRate || 44100;
    buffer = ctx.createBuffer(1, rate, rate);
    const data = buffer.getChannelData(0);
    const next = mulberry32(0x5eed);
    for (let i = 0; i < data.length; i += 1) data[i] = next() * 2 - 1;
    noiseBuffers.set(ctx, buffer);
  }
  return buffer;
}

/** A filtered noise burst whose filter may sweep from `f0` to `f1`. */
function noise(ctx, out, { type, f0, f1, start, dur, gain }) {
  const src = ctx.createBufferSource();
  src.buffer = noiseBuffer(ctx);
  const filter = ctx.createBiquadFilter();
  filter.type = type;
  filter.frequency.setValueAtTime(f0, start);
  if (f1 !== undefined && f1 !== f0) filter.frequency.exponentialRampToValueAtTime(f1, start + dur);
  const g = ctx.createGain();
  g.gain.setValueAtTime(gain, start);
  g.gain.exponentialRampToValueAtTime(0.0001, start + dur);
  src.connect(filter);
  filter.connect(g);
  g.connect(out);
  src.start(start);
  src.stop(start + dur);
}

/** Schedules a sound effect on `out`, `v` (0..1) is its volume. */
const RECIPES = {
  click: (c, o, t, v) => tone(c, o, { type: "square", f0: 1200, start: t, dur: 0.04, gain: 0.25 * v }),
  bombPlace: (c, o, t, v) => tone(c, o, { type: "triangle", f0: 520, f1: 330, start: t, dur: 0.08, gain: 0.5 * v }),
  explosion: (c, o, t, v) => {
    noise(c, o, { type: "lowpass", f0: 3000, f1: 120, start: t, dur: 0.6, gain: 0.9 * v });
    tone(c, o, { type: "sine", f0: 60, start: t, dur: 0.6, gain: 0.9 * v });
  },
  bonus: (c, o, t, v) => {
    [523.25, 659.25, 783.99].forEach((f0, i) => tone(c, o, { type: "triangle", f0, start: t + 0.06 * i, dur: 0.06, gain: 0.4 * v }));
  },
  malus: (c, o, t, v) => tone(c, o, { type: "sawtooth", f0: 400, f1: 120, start: t, dur: 0.25, gain: 0.25 * v }),
  punch: (c, o, t, v) => {
    noise(c, o, { type: "highpass", f0: 1500, start: t, dur: 0.12, gain: 0.6 * v });
    tone(c, o, { type: "triangle", f0: 700, f1: 200, start: t, dur: 0.12, gain: 0.5 * v });
  },
  land: (c, o, t, v) => tone(c, o, { type: "sine", f0: 140, start: t, dur: 0.09, gain: 0.7 * v }),
  eliminated: (c, o, t, v) => {
    [392, 329.63, 261.63, 196].forEach((f0, i) => tone(c, o, { type: "triangle", f0, start: t + 0.1 * i, dur: 0.1, gain: 0.45 * v }));
  },
  win: (c, o, t, v) => {
    [523.25, 659.25, 783.99, 1046.5].forEach((f0, i) =>
      tone(c, o, { type: "triangle", f0, start: t + 0.1 * i, dur: i === 3 ? 0.35 : 0.1, gain: 0.45 * v }),
    );
  },
  draw: (c, o, t, v) => {
    tone(c, o, { type: "triangle", f0: 196, start: t, dur: 0.2, gain: 0.45 * v });
    tone(c, o, { type: "triangle", f0: 146.83, start: t + 0.22, dur: 0.35, gain: 0.45 * v });
  },
};

// --- music ----------------------------------------------------------------------------------

/** A small seeded generator so that the tunes are the same on every run. */
export function mulberry32(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export const TUNE_NAMES = ["lobby", "match"];
export const TUNES = {
  lobby: { bpm: 96, seed: 0x10bb1, loopSteps: 32 },
  match: { bpm: 144, seed: 0x3a7c4, loopSteps: 16 },
};

const LOBBY_BASS = [36, 33, 31, 40]; // C2 A1 G1 E2: the C major pentatonic notes
const PENTATONIC = [60, 62, 64, 67, 69, 72, 74, 76]; // C major pentatonic from C4
const A_MINOR_SCALE = [69, 71, 72, 74, 76, 77, 79, 81]; // A natural minor from A4
const MATCH_BASS = [45, 41]; // A2 then F2
const MATCH_BASS_PATTERN = [0, 0, 12, 0, 0, 12, 0, 7];

/** The notes of a tune: `{ step, voice, freq, steps, gain }` on a grid of eighth-notes. */
export function buildTune(name) {
  const spec = TUNES[name];
  if (!spec) return null;
  const next = mulberry32(spec.seed);
  const notes = [];
  if (name === "lobby") {
    let at = Math.floor(next() * 4);
    for (let bar = 0; bar < 4; bar += 1) {
      notes.push({ step: bar * 8, voice: "bass", freq: hz(LOBBY_BASS[bar]), steps: 7, gain: 0.55 });
      for (let i = 0; i < 8; i += 1) {
        at = Math.min(PENTATONIC.length - 1, Math.max(0, at + [-2, -1, 1, 2][Math.floor(next() * 4)]));
        if (next() < 0.15) continue;
        notes.push({ step: bar * 8 + i, voice: "lead", freq: hz(PENTATONIC[at]), steps: 1, gain: 0.5 });
      }
    }
    for (let s = 0; s < spec.loopSteps; s += 2) notes.push({ step: s, voice: "tick", freq: 0, steps: 1, gain: 0.25 });
  } else {
    for (let s = 0; s < spec.loopSteps; s += 1) {
      const root = MATCH_BASS[Math.floor(s / 8)];
      notes.push({ step: s, voice: "bass", freq: hz(root + MATCH_BASS_PATTERN[s % 8]), steps: 1, gain: 0.5 });
      notes.push({ step: s, voice: "hat", freq: 0, steps: 1, gain: s % 2 === 1 ? 0.3 : 0.15 });
    }
    let at = 3;
    for (let s = 0; s < spec.loopSteps; s += 1) {
      at = Math.min(A_MINOR_SCALE.length - 1, Math.max(0, at + [-2, -1, 1, 2][Math.floor(next() * 4)]));
      if (next() < 0.3) continue;
      notes.push({ step: s, voice: "lead", freq: hz(A_MINOR_SCALE[at]), steps: next() < 0.3 ? 2 : 1, gain: 0.45 });
    }
  }
  notes.sort((a, b) => a.step - b.step);
  return { name, bpm: spec.bpm, loopSteps: spec.loopSteps, notes };
}

/** The tune that belongs to a page stage: the match tune in a match, silence when closed. */
export function tuneForStage(stage) {
  if (stage === "playing") return "match";
  if (stage === "closed") return null;
  return "lobby";
}

const TIMER_MS = 25;
const LOOKAHEAD = 0.12;

/** A lookahead step sequencer: a 25 ms timer schedules notes up to 120 ms ahead on the audio clock. */
export class Sequencer {
  constructor(ctx, output, tune, timer) {
    this.ctx = ctx;
    this.tune = tune;
    this.timer = timer;
    this.gain = ctx.createGain();
    this.gain.connect(output);
    this.timerId = null;
    this.stopped = false;
    this.fading = false;
    this.fadeTimer = null;
    this.step = 0;
    this.next = 0;
    this.byStep = Array.from({ length: tune.loopSteps }, (_, s) => tune.notes.filter((n) => n.step === s));
  }

  start() {
    this.stopped = false;
    this.step = 0;
    this.next = this.ctx.currentTime + 0.05;
    this.timerId = this.timer.setInterval(() => this.tick(), TIMER_MS);
    this.tick();
  }

  stop() {
    this.stopped = true;
    if (this.timerId !== null) this.timer.clearInterval(this.timerId);
    this.timerId = null;
  }

  tick() {
    if (this.stopped) return;
    const horizon = this.ctx.currentTime + LOOKAHEAD;
    const stepSeconds = 60 / this.tune.bpm / 2;
    while (this.next < horizon) {
      for (const n of this.byStep[this.step]) this.play(n, this.next, stepSeconds);
      this.next += stepSeconds;
      this.step = (this.step + 1) % this.tune.loopSteps;
    }
  }

  play(n, time, stepSeconds) {
    const lobby = this.tune.name === "lobby";
    const dur = Math.max(0.03, n.steps * stepSeconds * 0.95);
    if (n.voice === "lead") {
      tone(this.ctx, this.gain, { type: lobby ? "triangle" : "square", f0: n.freq, start: time, dur, gain: n.gain * (lobby ? 0.5 : 0.18), attack: 0.01 });
    } else if (n.voice === "bass") {
      tone(this.ctx, this.gain, { type: lobby ? "triangle" : "sawtooth", f0: n.freq, start: time, dur, gain: n.gain * (lobby ? 0.6 : 0.22), attack: 0.01 });
    } else {
      noise(this.ctx, this.gain, { type: "highpass", f0: 7000, start: time, dur: n.voice === "hat" ? 0.04 : 0.03, gain: n.gain * 0.5 });
    }
  }
}

// --- the engine -----------------------------------------------------------------------------

const GESTURES = ["keydown", "pointerdown", "touchstart"];

/**
 * Owns the audio context (created by the first user gesture), the three buses, the settings, the
 * sound effects and the music. Without Web Audio every method does nothing.
 */
export class AudioEngine {
  constructor({ AudioContext = globalThis.AudioContext ?? globalThis.webkitAudioContext, storage, timer } = {}) {
    this.AudioContextClass = AudioContext;
    this.storage = storage;
    this.timer = timer ?? {
      setInterval: (fn, ms) => globalThis.setInterval(fn, ms),
      clearInterval: (id) => globalThis.clearInterval(id),
      setTimeout: (fn, ms) => globalThis.setTimeout(fn, ms),
      clearTimeout: (id) => globalThis.clearTimeout(id),
    };
    this.settings = loadSettings(storage);
    this.ctx = null;
    this.buses = null;
    this.failed = false;
    this.hidden = false;
    this.tune = null;
    this.sequencers = new Map();
  }

  get started() {
    return this.ctx !== null;
  }

  /** Creates the context on the first keydown, pointerdown or touchstart of `target`. */
  attach(target) {
    const handler = () => {
      for (const type of GESTURES) target.removeEventListener(type, handler);
      this.unlock();
    };
    for (const type of GESTURES) target.addEventListener(type, handler);
  }

  /** Creates the context and the buses; does nothing when Web Audio is missing or refuses. */
  unlock() {
    if (this.ctx || this.failed) return;
    if (typeof this.AudioContextClass !== "function") {
      this.failed = true;
      return;
    }
    try {
      const ctx = new this.AudioContextClass();
      const master = ctx.createGain();
      master.connect(ctx.destination);
      const music = ctx.createGain();
      music.connect(master);
      const effects = ctx.createGain();
      effects.connect(master);
      this.ctx = ctx;
      this.buses = { master, music, effects };
      this.applyGains(true);
      if (!this.hidden) this.quietly(ctx.resume());
      this.applyTune();
    } catch {
      this.ctx = null;
      this.buses = null;
      this.failed = true;
      this.sequencers.clear();
    }
  }

  quietly(promise) {
    if (promise && typeof promise.catch === "function") promise.catch(() => {});
  }

  applyGains(immediate) {
    if (!this.ctx) return;
    const now = this.ctx.currentTime;
    const set = (param, value) => {
      if (immediate) {
        param.setValueAtTime(value, now);
      } else {
        param.cancelScheduledValues(now);
        param.setValueAtTime(param.value, now);
        param.linearRampToValueAtTime(value, now + RAMP_SECONDS);
      }
    };
    set(this.buses.music.gain, effectiveGain(this.settings.music, this.settings.muted));
    set(this.buses.effects.gain, effectiveGain(this.settings.effects, this.settings.muted));
  }

  /** Changes some settings: they apply within 50 ms and are stored. */
  setSettings(patch) {
    this.settings = sanitizeSettings({ ...this.settings, ...patch });
    saveSettings(this.storage, this.settings);
    try {
      this.applyGains(false);
    } catch {
      /* a broken context must not break the page */
    }
  }

  /** Suspends the audio while the page is hidden. */
  setHidden(hidden) {
    this.hidden = hidden === true;
    if (!this.ctx) return;
    try {
      this.quietly(this.hidden ? this.ctx.suspend() : this.ctx.resume());
    } catch {
      /* ignored */
    }
  }

  /** Plays one sound effect now. */
  play(id, volume = 1) {
    if (!this.ctx || !(id in RECIPES)) return;
    const v = Number.isFinite(volume) ? Math.min(1, Math.max(0, volume)) : 1;
    try {
      RECIPES[id](this.ctx, this.buses.effects, this.ctx.currentTime + 0.005, v);
    } catch {
      /* a failing node must not break the game loop */
    }
  }

  /** Plays what a snapshot's events sound like. */
  handleEvents(events) {
    if (!this.ctx) return;
    for (const s of soundsForEvents(events)) this.play(s.id, s.volume);
  }

  /** The sound of the "test sound" button. */
  testSound() {
    this.unlock();
    this.play("bonus");
  }

  /** Makes `name` the tune (`null` for none); the old one fades out as the new one fades in. */
  setTune(name) {
    const want = name !== null && name !== undefined && TUNES[name] ? name : null;
    if (want === this.tune) return;
    this.tune = want;
    if (this.ctx) this.applyTune();
  }

  applyTune() {
    if (!this.ctx) return;
    const now = this.ctx.currentTime;
    for (const [name, seq] of this.sequencers) {
      if (name !== this.tune && !seq.fading) this.fadeOut(name, seq, now);
    }
    if (this.tune === null) return;
    let seq = this.sequencers.get(this.tune);
    if (!seq) {
      seq = new Sequencer(this.ctx, this.buses.music, buildTune(this.tune), this.timer);
      this.sequencers.set(this.tune, seq);
      seq.gain.gain.setValueAtTime(0, now);
      seq.gain.gain.linearRampToValueAtTime(1, now + FADE_SECONDS);
      seq.start();
    } else if (seq.fading) {
      // asked for again while it was fading out: turn it back up
      this.timer.clearTimeout(seq.fadeTimer);
      seq.fading = false;
      seq.fadeTimer = null;
      seq.gain.gain.cancelScheduledValues(now);
      seq.gain.gain.setValueAtTime(seq.gain.gain.value, now);
      seq.gain.gain.linearRampToValueAtTime(1, now + FADE_SECONDS);
    }
  }

  fadeOut(name, seq, now) {
    seq.fading = true;
    seq.gain.gain.cancelScheduledValues(now);
    seq.gain.gain.setValueAtTime(seq.gain.gain.value, now);
    seq.gain.gain.linearRampToValueAtTime(0, now + FADE_SECONDS);
    seq.fadeTimer = this.timer.setTimeout(() => {
      seq.stop();
      seq.gain.disconnect();
      if (this.sequencers.get(name) === seq) this.sequencers.delete(name);
    }, FADE_SECONDS * 1000 + 50);
  }
}
