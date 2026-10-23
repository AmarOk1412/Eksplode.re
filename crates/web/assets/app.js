// Bomberust page: lobby, keyboard, WebSocket and canvas painting of the commands returned by the
// WebAssembly graphics module. The page never decides game rules: it sends key states as `Input`
// messages and draws the `Snapshot`s the server sends.
//
// The helpers before `start()` are pure and exported for the node tests (crates/web/js-tests).

import { AudioEngine, tuneForStage } from "./audio.js";
import {
  COLOUR_CSS,
  COLOUR_NAMES,
  FACES,
  GENDERS,
  HEADS,
  defaultLook,
  editorReducer,
  loadLook,
  looksJson,
  lookMessage,
  previewCommands,
  sanitizeLook,
  saveLook,
} from "./look.js";

const DIRECTIONS = {
  ArrowUp: "Up",
  KeyW: "Up",
  ArrowDown: "Down",
  KeyS: "Down",
  ArrowLeft: "Left",
  KeyA: "Left",
  ArrowRight: "Right",
  KeyD: "Right",
};
const BOMB_KEYS = new Set(["Space", "KeyE"]);
const PUNCH_KEYS = new Set(["KeyZ", "KeyQ"]);
const TICK_MS = 50;
const PROTOCOL_VERSION = 1;

/** Keeps the held keys and turns them into one input per tick. */
export class KeyTracker {
  constructor() {
    this.held = [];
    this.bomb = false;
    this.punch = false;
  }

  press(code) {
    if (code in DIRECTIONS) {
      if (!this.held.includes(code)) this.held.push(code);
    } else if (BOMB_KEYS.has(code)) {
      this.bomb = true;
    } else if (PUNCH_KEYS.has(code)) {
      this.punch = true;
    }
  }

  release(code) {
    this.held = this.held.filter((c) => c !== code);
  }

  clear() {
    this.held = [];
    this.bomb = false;
    this.punch = false;
  }

  /** The latest pressed direction that is still held; bomb and punch presses fire once. */
  take() {
    const last = this.held[this.held.length - 1];
    const input = { mv: last ? DIRECTIONS[last] : null, bomb: this.bomb, punch: this.punch };
    this.bomb = false;
    this.punch = false;
    return input;
  }
}

export function initialState() {
  return { stage: "idle", slot: null, lobby: null, notice: "", observation: null, tick: 0, looks: defaultLooks() };
}

function defaultLooks() {
  return [0, 1, 2, 3].map((i) => defaultLook(i));
}

/** The four looks of a message; a host that sends none leaves what is known (or the defaults). */
function looksOf(event, previous) {
  if (!Array.isArray(event.looks) || event.looks.length === 0) return previous ?? defaultLooks();
  return [0, 1, 2, 3].map((i) => sanitizeLook(event.looks[i], i));
}

export function rejectText(reason) {
  switch (reason) {
    case "Full":
      return "The match is full (4 players). Try again when someone leaves.";
    case "BadName":
      return "Your name is not valid: use 1 to 16 printable characters.";
    case "InMatch":
      return "A match is running. Wait for it to finish, then join.";
    case "BadVersion":
      return "This page and the server use incompatible versions. Reload the page.";
    default:
      return "The server refused the join.";
  }
}

function outcomeText(outcome, slot) {
  if (outcome === "Draw") return "Draw";
  const winner = outcome?.Winner;
  if (winner === slot) return "You won!";
  return `Player ${winner + 1} won`;
}

/** Pure state machine of the page: `event` is a server message or a local event (lower case). */
export function reduce(state, event) {
  switch (event.type) {
    case "connecting":
      return { ...state, stage: "connecting", notice: "" };
    case "socket-open":
      return { ...state, stage: "joining" };
    case "socket-closed":
      if (state.stage === "closed") return state;
      return { ...state, stage: "closed", notice: "Connection lost. The server may have stopped." };
    case "retry":
      return initialState();
    case "Welcome":
      return { ...state, stage: "lobby", slot: event.slot, lobby: event.lobby, notice: "", looks: looksOf(event, null) };
    case "Rejected":
      return { ...state, stage: "closed", notice: rejectText(event.reason) };
    case "Lobby": {
      const stage = state.stage === "playing" || state.stage === "closed" ? state.stage : "lobby";
      return { ...state, stage, lobby: event.lobby, looks: looksOf(event, state.looks) };
    }
    case "Start":
      return { ...state, stage: "playing", notice: "" };
    case "Snapshot":
      return { ...state, stage: "playing", observation: event.state, tick: event.tick };
    case "PlayerLeft":
      return { ...state, notice: `Player ${event.slot + 1} left, a bot took over` };
    case "Ended":
      return { ...state, stage: "lobby", notice: outcomeText(event.outcome, state.slot) };
    case "Error":
      if (event.code === "HostLeft") {
        return { ...state, stage: "closed", notice: "The server stopped (server gone)." };
      }
      return { ...state, notice: event.message };
    default:
      return state;
  }
}

/** Lowest slot held by a human: the only one that may start the match. */
export function lobbyOwner(lobby) {
  const i = lobby?.slots.findIndex((s) => s.controller === "Local" || s.controller === "Remote");
  return i === undefined || i < 0 ? null : i;
}

/** The lobby as cards, one per slot: banner, name caption and the owner/you/ready marks. */
export function lobbyCards(lobby, you) {
  const owner = lobbyOwner(lobby);
  return (lobby?.slots ?? []).map((slot, i) => {
    const empty = slot.controller === "Empty";
    const name = empty ? "empty" : slot.controller === "Bot" ? "bot" : slot.name || `player ${i + 1}`;
    const marks = [i === you ? "you" : "", i === owner ? "owner" : "", slot.ready ? "ready" : ""].filter(Boolean);
    return {
      slot: i,
      empty,
      banner: empty ? "EMPTY" : `PLAYER ${i + 1}`,
      name,
      marks,
      caption: marks.length ? `${name} (${marks.join(", ")})` : name,
    };
  });
}

/** Calls `paint(seconds)` on every animation frame while it is active, and not a frame longer. */
export class Animator {
  constructor(raf, caf, paint) {
    this.raf = raf;
    this.caf = caf;
    this.paint = paint;
    this.active = false;
    this.handle = null;
    this.origin = null;
  }

  setActive(on) {
    if (on === this.active) return;
    this.active = on;
    if (on) {
      this.origin = null;
      this.handle = this.raf((t) => this.frame(t));
    } else {
      if (this.handle !== null) this.caf(this.handle);
      this.handle = null;
    }
  }

  frame(t) {
    this.handle = null;
    if (!this.active) return;
    this.origin ??= t;
    this.paint((t - this.origin) / 1000);
    if (this.active) this.handle = this.raf((next) => this.frame(next));
  }
}

/** One row per participant for the status list next to the canvas. */
export function hudRows(state) {
  const players = state.observation?.players;
  const slots = state.lobby?.slots;
  if (!players || !slots) return [];
  const base = state.observation.config?.base_move_cooldown ?? 4;
  const hz = state.observation.config?.tick_hz > 0 ? state.observation.config.tick_hz : 20;
  const rows = [];
  slots.forEach((slot, i) => {
    if (slot.controller === "Empty" || !players[i]) return;
    const p = players[i];
    const you = i === state.slot;
    const kind = you ? "you" : slot.controller === "Bot" ? "bot" : "player";
    const label = you ? "You" : kind === "bot" ? "Bot" : `${slot.name || `Player ${i + 1}`} (player)`;
    rows.push({
      slot: i,
      kind,
      label,
      alive: p.alive,
      bombs: `${p.bombs_active}/${p.bombs_max}`,
      range: p.range,
      speed: base + 1 - p.move_cooldown,
      effects: effectsOf(p, hz),
      you,
    });
  });
  return rows;
}

/** Short names of what is active on a player: the punch and the malus with the seconds left. */
export function effectsOf(player, hz) {
  const effects = [];
  if (player.has_punch) effects.push("punch");
  const m = player.malus;
  if (m && typeof m.ticks_left === "number") {
    const name = { Slow: "slow", HyperSpeed: "hyper", SmallBomb: "small" }[m.kind] ?? "malus";
    effects.push(`${name} ${Math.ceil(m.ticks_left / hz)}s`);
  }
  return effects;
}

export function inputMessage(state, keys) {
  return { type: "Input", tick: state.tick, mv: keys.mv, bomb: keys.bomb, punch: keys.punch === true };
}

/** Time left in the round as `MM:SS`, or an empty string when the snapshot has no limit. */
export function timerText(obs) {
  const limit = obs?.config?.round_limit_ticks;
  const hz = obs?.config?.tick_hz;
  if (!(limit > 0) || !(hz > 0) || typeof obs.tick !== "number") return "";
  const left = Math.max(0, Math.ceil((limit - obs.tick) / hz));
  const two = (n) => String(n).padStart(2, "0");
  return `${two(Math.floor(left / 60))}:${two(left % 60)}`;
}

/** Rows of a character shown on its card: head and shoulders. */
export const CARD_ROWS = 7;

const LOAD_ERRORS = { 0: "ok", 1: "invalid text", 2: "invalid game data", 3: "arena too large", 4: "out of memory" };

/** Wraps the exports of the WebAssembly module (see contracts/wasm-render-api.md). */
export function createRenderer(ex) {
  const stride = ex.bomberust_stride();
  if (stride !== 16) {
    throw new Error("The graphics module and the page are out of sync (stride " + stride + ").");
  }
  const encoder = new TextEncoder();
  return {
    load(text) {
      const bytes = encoder.encode(text);
      const size = Math.max(bytes.length, 1);
      const ptr = ex.bomberust_alloc(size);
      if (ptr === 0) return 4;
      new Uint8Array(ex.memory.buffer, ptr, bytes.length).set(bytes);
      const code = ex.bomberust_load(ptr, bytes.length);
      ex.bomberust_free(ptr, size);
      return code;
    },
    describe(code) {
      return LOAD_ERRORS[code] ?? "unknown error";
    },
    frame(width, height, alpha) {
      const count = ex.bomberust_frame(width, height, alpha);
      const data = new Float32Array(ex.memory.buffer, ex.bomberust_output_ptr(), count * stride);
      return { count, stride, data };
    },
    pick(x, y) {
      return ex.bomberust_pick_tile(x, y);
    },
    /** Gives the module the looks of the four slots (plain objects). */
    setLooks(looks) {
      if (!ex.bomberust_set_looks) return -1;
      const bytes = encoder.encode(looksJson(looks));
      const size = Math.max(bytes.length, 1);
      const ptr = ex.bomberust_alloc(size);
      if (ptr === 0) return 4;
      new Uint8Array(ex.memory.buffer, ptr, bytes.length).set(bytes);
      const code = ex.bomberust_set_looks(ptr, bytes.length);
      ex.bomberust_free(ptr, size);
      return code;
    },
    /** The semi-3D character of a slot, `phase` seconds into its animation. */
    portrait3d(slot, width, height, phase) {
      const count = ex.bomberust_portrait3d ? ex.bomberust_portrait3d(slot, width, height, phase) : 0;
      const data = new Float32Array(ex.memory.buffer, count ? ex.bomberust_output_ptr() : 0, count * stride);
      return { count, stride, data };
    },
    /** Commands of a character fitted into width x height pixels; `rows` 0 is the whole figure. */
    portrait(slot, width, height, rows) {
      const count = ex.bomberust_portrait(slot, width, height, rows);
      const data = new Float32Array(ex.memory.buffer, ex.bomberust_output_ptr(), count * stride);
      return { count, stride, data };
    },
  };
}

function rgba(data, o) {
  const c = (v) => Math.round(Math.min(1, Math.max(0, v)) * 255);
  return `rgba(${c(data[o + 1])},${c(data[o + 2])},${c(data[o + 3])},${+data[o + 4].toFixed(3)})`;
}

/** Paints the commands in order: 0 quad, 1 circle, 2 text, 3 triangle. */
export function paintCommands(ctx, { count, stride, data }) {
  for (let i = 0; i < count; i += 1) {
    const o = i * stride;
    const kind = data[o];
    ctx.fillStyle = rgba(data, o);
    if (kind === 0 || kind === 3) {
      const points = kind === 0 ? 4 : 3;
      ctx.beginPath();
      ctx.moveTo(data[o + 5], data[o + 6]);
      for (let p = 1; p < points; p += 1) ctx.lineTo(data[o + 5 + 2 * p], data[o + 6 + 2 * p]);
      ctx.closePath();
      ctx.fill();
    } else if (kind === 1) {
      ctx.beginPath();
      ctx.arc(data[o + 5], data[o + 6], data[o + 13], 0, Math.PI * 2);
      ctx.fill();
    } else if (kind === 2) {
      ctx.font = `bold ${Math.max(8, Math.round(data[o + 13]))}px "Courier New", monospace`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(String.fromCharCode(data[o + 14]), data[o + 5], data[o + 6]);
    }
  }
}

// ---------------------------------------------------------------------------------------------
// Browser part

function start() {
  const $ = (id) => document.getElementById(id);
  const el = {
    notice: $("notice"),
    joinPanel: $("join-panel"),
    lobbyPanel: $("lobby-panel"),
    gamePanel: $("game-panel"),
    name: $("name"),
    join: $("join"),
    retry: $("retry"),
    slots: $("slots"),
    start: $("start"),
    addBot: $("add-bot"),
    removeBot: $("remove-bot"),
    ready: $("ready"),
    canvas: $("view"),
    hud: $("hud"),
    timer: $("timer"),
    logo: $("logo"),
    editor: $("editor"),
    preview: $("preview"),
    swatches: $("swatches"),
    gender: $("gender"),
    head: $("head"),
    face: $("face"),
    randomLook: $("random-look"),
    doneLook: $("done-look"),
    changeLook: $("change-look"),
    soundButton: $("sound-button"),
    soundPanel: $("sound-panel"),
    musicVolume: $("music-volume"),
    effectsVolume: $("effects-volume"),
    mute: $("mute"),
    testSound: $("test-sound"),
    closeSound: $("close-sound"),
  };
  const ctx = el.canvas.getContext("2d");
  const keys = new KeyTracker();
  let state = initialState();
  let socket = null;
  let renderer = null;
  let lastSnapshotAt = 0;
  let logoPainted = false;
  let lobbyCanvases = [];
  const reducedMotion = () => window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;

  /** Paints the semi-3D characters of the lobby cards `seconds` into their animation. */
  function paintLobby(seconds) {
    if (!renderer) return;
    const dpr = window.devicePixelRatio || 1;
    for (const [canvas, slot] of lobbyCanvases) {
      if (!canvas.clientWidth || !canvas.clientHeight) continue;
      const w = Math.max(16, Math.round(canvas.clientWidth * dpr));
      const h = Math.max(16, Math.round(canvas.clientHeight * dpr));
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
      const g = canvas.getContext("2d");
      g.clearRect(0, 0, w, h);
      paintCommands(g, renderer.portrait3d(slot, w, h, seconds));
    }
  }
  const animator = new Animator(
    (fn) => window.requestAnimationFrame(fn),
    (id) => window.cancelAnimationFrame(id),
    paintLobby,
  );
  document.addEventListener("visibilitychange", () => animator.setActive(state.stage === "lobby" && !document.hidden && renderer !== null && !reducedMotion()));

  /** Paints a character into a canvas whose CSS size is set by the stylesheet. */
  function paintPortrait(canvas, slot, rows) {
    if (!renderer) return;
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    const w = Math.max(16, Math.round((rect.width || canvas.width) * dpr));
    const h = Math.max(16, Math.round((rect.height || canvas.height) * dpr));
    canvas.width = w;
    canvas.height = h;
    paintCommands(canvas.getContext("2d"), renderer.portrait(slot, w, h, rows));
  }

  el.name.value = `Player${1 + Math.floor(Math.random() * 99)}`;

  function send(msg) {
    if (socket && socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(msg));
  }

  function dispatch(event) {
    state = reduce(state, event);
    if (event.type === "Snapshot") audio.handleEvents(event.events);
    audio.setTune(tuneForStage(state.stage));
    if ((event.type === "Welcome" || event.type === "Lobby") && renderer) renderer.setLooks(state.looks);
    if (event.type === "Snapshot" && renderer) {
      const code = renderer.load(JSON.stringify(event.state));
      if (code !== 0) state = { ...state, notice: `Graphics error: ${renderer.describe(code)}` };
      lastSnapshotAt = performance.now();
    }
    updateUi();
  }

  function updateUi() {
    el.notice.textContent = state.notice;
    const playing = state.stage === "playing";
    const inLobby = state.stage === "lobby";
    el.joinPanel.hidden = inLobby || playing;
    el.lobbyPanel.hidden = !inLobby;
    el.gamePanel.hidden = !playing;
    el.editor.hidden = !(!inLobby && !playing) && !(inLobby && editing);
    el.doneLook.hidden = !inLobby;
    if (!el.editor.hidden) renderEditor();
    el.join.hidden = state.stage === "closed";
    el.retry.hidden = state.stage !== "closed";
    el.join.disabled = state.stage === "connecting" || state.stage === "joining";
    el.name.disabled = el.join.disabled;

    const owner = lobbyOwner(state.lobby);
    const isOwner = owner !== null && owner === state.slot;
    el.start.disabled = !isOwner;
    el.addBot.disabled = !isOwner;
    el.removeBot.disabled = !isOwner;
    lobbyCanvases = [];
    el.slots.replaceChildren(
      ...lobbyCards(state.lobby, state.slot).map((c) => {
        const li = document.createElement("li");
        li.className = "slot";
        const card = document.createElement("div");
        card.className = `card slot-${c.slot}${c.empty ? " empty" : ""}`;
        const portrait = document.createElement("canvas");
        portrait.className = "portrait";
        card.append(portrait);
        if (c.marks.length > 0) {
          const marks = document.createElement("div");
          marks.className = "marks";
          for (const m of c.marks) {
            const mark = document.createElement("span");
            mark.className = `mark mark-${m}`;
            mark.textContent = m;
            marks.append(mark);
          }
          card.append(marks);
        }
        const banner = document.createElement("div");
        banner.className = "banner";
        banner.textContent = c.banner;
        card.append(banner);
        li.append(card);
        if (!c.empty) lobbyCanvases.push([portrait, c.slot]);
        const caption = document.createElement("span");
        caption.className = "caption";
        caption.textContent = c.caption;
        caption.title = c.caption;
        li.append(caption);
        return li;
      }),
    );
    // canvases need a layout before their size is known, so paint once they are in the page
    paintLobby(0);
    animator.setActive(state.stage === "lobby" && !document.hidden && renderer !== null && !reducedMotion());
    el.timer.textContent = timerText(state.observation);
    if (renderer && !logoPainted && !el.joinPanel.hidden) {
      paintPortrait(el.logo, 0, 0);
      logoPainted = true;
    }
    el.hud.replaceChildren(
      ...hudRows(state).map((row) => {
        const li = document.createElement("li");
        li.className = row.alive ? "" : "dead";
        const chip = document.createElement("span");
        chip.className = `chip slot-${row.slot}`;
        chip.style.backgroundColor = COLOUR_CSS[state.looks[row.slot]?.colour] ?? "";
        chip.textContent = String(row.slot + 1);
        const text = document.createElement("span");
        text.textContent = row.alive
          ? `${row.label}: bombs ${row.bombs}, range ${row.range}, speed ${row.speed}${row.effects.map((e) => `, ${e}`).join("")}`
          : `${row.label}: eliminated`;
        li.append(chip, text);
        return li;
      }),
    );
  }

  function connect() {
    const name = el.name.value.trim();
    if (name.length < 1 || [...name].length > 16) {
      dispatch({ type: "Rejected", reason: "BadName" });
      state = { ...state, stage: "idle" };
      updateUi();
      return;
    }
    dispatch({ type: "connecting" });
    const scheme = location.protocol === "https:" ? "wss" : "ws";
    socket = new WebSocket(`${scheme}://${location.host}/ws`);
    socket.onopen = () => {
      dispatch({ type: "socket-open" });
      send(lookMessage("Join", draft, { version: PROTOCOL_VERSION, name }));
    };
    socket.onmessage = (ev) => {
      try {
        dispatch(JSON.parse(ev.data));
      } catch {
        // ignore anything that is not a protocol message
      }
    };
    socket.onclose = () => dispatch({ type: "socket-closed" });
  }

  el.join.addEventListener("click", connect);
  el.name.addEventListener("keydown", (e) => {
    if (e.key === "Enter") connect();
  });
  el.retry.addEventListener("click", () => {
    if (socket) socket.close();
    dispatch({ type: "retry" });
  });
  el.start.addEventListener("click", () => send({ type: "Start" }));
  el.addBot.addEventListener("click", () => send({ type: "AddBot" }));
  el.removeBot.addEventListener("click", () => send({ type: "RemoveBot" }));
  el.ready.addEventListener("click", () => {
    const mine = state.lobby?.slots[state.slot]?.ready ?? false;
    send({ type: "SetReady", ready: !mine });
  });

  // ---- the character editor: the draft is saved on this device and used by the next Join ----
  const storage = (() => {
    try {
      return window.localStorage;
    } catch {
      return undefined;
    }
  })();

  // ---- sound: generated effects and music, started by the first key press or click ----
  const audio = new AudioEngine({ storage });
  audio.attach(window);
  audio.setTune(tuneForStage(state.stage));
  el.musicVolume.value = String(Math.round(audio.settings.music * 100));
  el.effectsVolume.value = String(Math.round(audio.settings.effects * 100));
  el.mute.checked = audio.settings.muted;
  el.soundButton.addEventListener("click", () => {
    el.soundPanel.hidden = !el.soundPanel.hidden;
    el.soundButton.setAttribute("aria-expanded", String(!el.soundPanel.hidden));
  });
  el.closeSound.addEventListener("click", () => {
    el.soundPanel.hidden = true;
    el.soundButton.setAttribute("aria-expanded", "false");
  });
  el.musicVolume.addEventListener("input", () => audio.setSettings({ music: Number(el.musicVolume.value) / 100 }));
  el.effectsVolume.addEventListener("input", () => audio.setSettings({ effects: Number(el.effectsVolume.value) / 100 }));
  el.mute.addEventListener("change", () => audio.setSettings({ muted: el.mute.checked }));
  el.testSound.addEventListener("click", () => audio.testSound());
  // every button of the menu and lobby clicks (the test button plays its own sound)
  document.addEventListener("click", (e) => {
    const button = e.target instanceof Element ? e.target.closest("button") : null;
    if (button && button !== el.testSound && state.stage !== "playing") audio.play("click");
  });
  document.addEventListener("visibilitychange", () => audio.setHidden(document.hidden));

  let draft = loadLook(storage, 0);
  let editing = false;

  function fillSelect(select, options) {
    select.replaceChildren(
      ...options.map((name) => {
        const o = document.createElement("option");
        o.value = name;
        o.textContent = name;
        return o;
      }),
    );
  }
  fillSelect(el.gender, GENDERS);
  fillSelect(el.head, HEADS);
  fillSelect(el.face, FACES);
  el.swatches.replaceChildren(
    ...COLOUR_NAMES.map((name) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "swatch";
      b.dataset.colour = name;
      b.title = name;
      b.setAttribute("aria-label", name);
      b.style.backgroundColor = COLOUR_CSS[name];
      b.addEventListener("click", () => editLook({ type: "colour", value: name }));
      return b;
    }),
  );

  function editLook(action) {
    draft = editorReducer(draft, action);
    saveLook(storage, draft);
    renderEditor();
    if (state.stage === "lobby") send(lookMessage("SetLook", draft));
  }

  function renderEditor() {
    el.gender.value = draft.gender;
    el.head.value = draft.head;
    el.face.value = draft.face;
    for (const b of el.swatches.children) b.classList.toggle("selected", b.dataset.colour === draft.colour);
    paintPreview();
  }

  function paintPreview() {
    if (!renderer || el.editor.hidden) return;
    const dpr = window.devicePixelRatio || 1;
    const rect = el.preview.getBoundingClientRect();
    const w = Math.max(16, Math.round((rect.width || 160) * dpr));
    const h = Math.max(16, Math.round((rect.height || 226) * dpr));
    el.preview.width = w;
    el.preview.height = h;
    paintCommands(el.preview.getContext("2d"), previewCommands(renderer, draft, w, h, 0, state.looks));
  }

  el.gender.addEventListener("change", () => editLook({ type: "gender", value: el.gender.value }));
  el.head.addEventListener("change", () => editLook({ type: "head", value: el.head.value }));
  el.face.addEventListener("change", () => editLook({ type: "face", value: el.face.value }));
  el.randomLook.addEventListener("click", () => editLook({ type: "random" }));
  el.changeLook.addEventListener("click", () => {
    editing = !editing;
    updateUi();
  });
  el.doneLook.addEventListener("click", () => {
    editing = false;
    updateUi();
  });

  window.addEventListener("keydown", (e) => {
    if (state.stage === "playing") {
      // the sliders of the sound panel keep their arrow keys
      if (e.target instanceof HTMLInputElement) return;
      if (e.code in DIRECTIONS || BOMB_KEYS.has(e.code)) e.preventDefault();
      keys.press(e.code);
    } else if (state.stage === "lobby" && e.key === "Enter") {
      send({ type: "Start" });
    }
  });
  window.addEventListener("keyup", (e) => keys.release(e.code));
  window.addEventListener("blur", () => keys.clear());
  document.addEventListener("visibilitychange", () => keys.clear());

  setInterval(() => {
    if (state.stage === "playing") send(inputMessage(state, keys.take()));
  }, TICK_MS);
  setInterval(() => {
    if (document.hidden) send({ type: "Ping", nonce: Date.now() });
  }, 2000);

  function resize() {
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(200, Math.floor(el.canvas.clientWidth * dpr));
    const h = Math.max(150, Math.floor(el.canvas.clientHeight * dpr));
    if (el.canvas.width !== w || el.canvas.height !== h) {
      el.canvas.width = w;
      el.canvas.height = h;
    }
  }
  window.addEventListener("resize", resize);

  function draw(now) {
    requestAnimationFrame(draw);
    if (state.stage !== "playing" || !renderer || !state.observation) return;
    resize();
    const alpha = Math.min(1, Math.max(0, (now - lastSnapshotAt) / TICK_MS));
    const commands = renderer.frame(el.canvas.width, el.canvas.height, alpha);
    ctx.clearRect(0, 0, el.canvas.width, el.canvas.height);
    paintCommands(ctx, commands);
  }

  async function loadGraphics() {
    if (typeof WebAssembly === "undefined" || typeof WebSocket === "undefined") {
      state = { ...state, stage: "closed", notice: "This browser needs WebAssembly and WebSocket support." };
      updateUi();
      return;
    }
    try {
      const response = await fetch("/bomberust_render.wasm");
      const bytes = await response.arrayBuffer();
      const { instance } = await WebAssembly.instantiate(bytes, {});
      renderer = createRenderer(instance.exports);
      renderer.setLooks(state.looks);
    } catch (e) {
      state = { ...state, stage: "closed", notice: `Graphics module failed: ${e.message}` };
    }
    updateUi();
    renderEditor();
  }

  updateUi();
  loadGraphics();
  requestAnimationFrame(draw);
}

if (typeof document !== "undefined") start();
