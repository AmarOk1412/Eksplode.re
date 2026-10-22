// A fake Web Audio implementation for node tests: it records what the page builds and schedules.

export function fakeAudio() {
  const log = { contexts: [], nodes: [], oscillators: [], noises: [], resumed: 0, suspended: 0 };

  class Param {
    constructor(value) {
      this.value = value;
      this.events = [];
    }
    setValueAtTime(v, t) {
      this.value = v;
      this.events.push(["set", v, t]);
      return this;
    }
    linearRampToValueAtTime(v, t) {
      this.events.push(["linear", v, t]);
      this.value = v;
      return this;
    }
    exponentialRampToValueAtTime(v, t) {
      this.events.push(["exp", v, t]);
      this.value = v;
      return this;
    }
    setTargetAtTime(v, t, k) {
      this.events.push(["target", v, t, k]);
      this.value = v;
      return this;
    }
    cancelScheduledValues() {
      this.events.push(["cancel"]);
      return this;
    }
  }

  class Node {
    constructor(kind) {
      this.kind = kind;
      this.outputs = [];
      log.nodes.push(this);
    }
    connect(other) {
      this.outputs.push(other);
      return other;
    }
    disconnect() {
      this.outputs = [];
    }
  }

  class Gain extends Node {
    constructor() {
      super("gain");
      this.gain = new Param(1);
    }
  }

  class Osc extends Node {
    constructor() {
      super("osc");
      this.type = "sine";
      this.frequency = new Param(440);
      this.detune = new Param(0);
      this.startedAt = null;
      this.stoppedAt = null;
      log.oscillators.push(this);
    }
    start(t = 0) {
      this.startedAt = t;
    }
    stop(t = 0) {
      this.stoppedAt = t;
    }
  }

  class Filter extends Node {
    constructor() {
      super("filter");
      this.type = "lowpass";
      this.frequency = new Param(350);
      this.Q = new Param(1);
    }
  }

  class BufferSource extends Node {
    constructor() {
      super("noise");
      this.buffer = null;
      this.loop = false;
      this.startedAt = null;
      this.stoppedAt = null;
      log.noises.push(this);
    }
    start(t = 0) {
      this.startedAt = t;
    }
    stop(t = 0) {
      this.stoppedAt = t;
    }
  }

  class Context {
    constructor() {
      this.currentTime = 0;
      this.state = "suspended";
      this.sampleRate = 44100;
      this.destination = new Node("destination");
      log.contexts.push(this);
    }
    createGain() {
      return new Gain();
    }
    createOscillator() {
      return new Osc();
    }
    createBiquadFilter() {
      return new Filter();
    }
    createBufferSource() {
      return new BufferSource();
    }
    createBuffer(channels, length, rate) {
      const data = new Float32Array(length);
      return { numberOfChannels: channels, length, sampleRate: rate, getChannelData: () => data };
    }
    resume() {
      this.state = "running";
      log.resumed += 1;
      return Promise.resolve();
    }
    suspend() {
      this.state = "suspended";
      log.suspended += 1;
      return Promise.resolve();
    }
    advance(seconds) {
      this.currentTime += seconds;
    }
  }

  return { AudioContext: Context, log };
}

/** A tiny in-memory `localStorage` for tests; `broken` makes every call throw. */
export function fakeStorage({ broken = false } = {}) {
  const map = new Map();
  const guard = () => {
    if (broken) throw new Error("storage unavailable");
  };
  return {
    getItem(k) {
      guard();
      return map.has(k) ? map.get(k) : null;
    },
    setItem(k, v) {
      guard();
      map.set(k, String(v));
    },
    removeItem(k) {
      guard();
      map.delete(k);
    },
    dump: () => Object.fromEntries(map),
  };
}
