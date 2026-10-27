// Eksplode.re page: lobby, keyboard, WebSocket and canvas painting of the commands returned by the
// WebAssembly graphics module. The page never decides game rules: it sends key states as `Input`
// messages and draws the `Snapshot`s the server sends.
//
// The helpers before `start()` are pure and exported for the node tests (crates/web/js-tests).

import { AudioEngine, tuneForStage } from "./audio.js";
import {
  DEFAULT_BINDINGS,
  GamepadTracker,
  assignBinding,
  compatibleGamepads,
  loadBindings,
  sanitizeBindings,
  saveBindings,
} from "./controls.js";
import {
  NavigationLayers,
  NavTracker,
  activateControl,
  adjustControl,
  focusKey,
  focusTargets,
  moveFocus,
  moveFormFocus,
  moveGroupFocus,
  navigationMode,
  OSK_ROWS,
  createOskState,
  reduceOsk,
  restoreFocus,
} from "./nav.js";
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

import {
  MODES,
  MODE_HINTS,
  MODE_LABELS,
  PLAYABLE_MODES,
  SIZES,
  SIZE_DIMENSIONS,
  SIZE_LABELS,
  MINUTES,
  loadRoom,
  roomControls,
  roomPayload,
  roomReducer,
  sanitizeRoom,
  saveRoom,
  setRoomMessage,
} from "./room.js";

const DIRECTION_ACTIONS = { up: "Up", down: "Down", left: "Left", right: "Right" };
const TICK_MS = 50;
const PROTOCOL_VERSION = 1;

/** Keeps the held keys and turns them into one input per tick. */
export class KeyTracker {
  constructor(bindings = DEFAULT_BINDINGS) {
    this.bindings = sanitizeBindings(bindings);
    this.held = [];
    this.bomb = false;
    this.punch = false;
  }

  setBindings(bindings) {
    this.bindings = sanitizeBindings(bindings);
    this.clear();
  }

  actionFor(code) {
    return Object.keys(this.bindings).find((action) => this.bindings[action].includes(code)) ?? null;
  }

  isBound(code) {
    return this.actionFor(code) !== null;
  }

  press(code) {
    const action = this.actionFor(code);
    if (action in DIRECTION_ACTIONS) {
      if (!this.held.includes(code)) this.held.push(code);
    } else if (action === "bomb") {
      this.bomb = true;
    } else if (action === "punch") {
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
    const action = last ? this.actionFor(last) : null;
    const input = { mv: action ? DIRECTION_ACTIONS[action] : null, bomb: this.bomb, punch: this.punch };
    this.bomb = false;
    this.punch = false;
    return input;
  }
}

export function initialState() {
  return {
    stage: "idle",
    // "room" shows the room creation menu, "join" the name and look screen
    menu: "join",
    room: null,
    slot: null,
    lobby: null,
    notice: "",
    observation: null,
    tick: 0,
    looks: defaultLooks(),
    dev: false,
    devState: null,
  };
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

const TEAM_LETTERS = ["A", "B"];

/** The name shown for a slot in messages. */
function slotName(lobby, i) {
  const s = lobby?.slots?.[i];
  if (s?.controller === "Bot") return `Bot ${i + 1}`;
  return s?.name || `Player ${i + 1}`;
}

function outcomeText(outcome, slot, lobby, observation) {
  if (outcome === "Draw") return "Draw";
  if (outcome === "ChaserWins") {
    const chaser = observation?.players?.findIndex((p) => p.role === "Chaser") ?? -1;
    if (chaser >= 0 && chaser === slot) return "You won! The chaser wins";
    return chaser >= 0 ? `The chaser wins (${slotName(lobby, chaser)})` : "The chaser wins";
  }
  if (outcome === "RunnersWin") {
    const chaser = observation?.players?.findIndex((p) => p.role === "Chaser") ?? -1;
    return chaser >= 0 && chaser !== slot ? "You survived! The runners win" : "The runners win";
  }
  if (outcome !== null && typeof outcome === "object" && "TeamWin" in outcome) {
    const team = outcome.TeamWin === 1 ? 1 : 0;
    const members = [0, 1, 2, 3].filter((i) => i >> 1 === team).map((i) => slotName(lobby, i));
    const mine = slot !== null && slot >> 1 === team ? " - your team!" : "";
    return `Team ${TEAM_LETTERS[team]} wins (${members.join(", ")})${mine}`;
  }
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
    case "RoomInfo": {
      const room = {
        open: event.open === true,
        settings: sanitizeRoom(event.settings),
        humans: Number.isInteger(event.humans) ? event.humans : 0,
      };
      if (state.stage !== "idle") return { ...state, room };
      return { ...state, room, menu: room.open ? "join" : "room" };
    }
    case "room-confirm":
      return { ...state, menu: "join" };
    case "Welcome":
      return { ...state, stage: "lobby", slot: event.slot, lobby: event.lobby, notice: "", looks: looksOf(event, null), dev: event.dev === true, devState: null };
    case "DevState":
      return { ...state, devState: { paused: event.paused === true, drops: event.drops } };
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
      return { ...state, stage: "lobby", notice: outcomeText(event.outcome, state.slot, state.lobby, state.observation) };
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

// ---- developer mode ----

/** The seven powers in the order of `DevSetDrops.chances`. */
export const DROP_KINDS = ["ExtraBomb", "Range", "Speed", "Punch", "Slow", "HyperSpeed", "SmallBomb", "Kick"];
export const DROP_LABELS = ["Extra bomb", "Flame", "Roller", "Punch", "Slow", "Hyper speed", "Small bomb", "Kick"];

const unit = (v) => (typeof v === "number" && Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 0);

/** Slider percentages (0-100) as drop chances (0.0-1.0), one per power. */
export function chancesFromPercents(percents) {
  return DROP_KINDS.map((_, i) => unit(percents?.[i] / 100));
}

/** Drop chances as whole slider percentages. */
export function percentsFromChances(chances) {
  return DROP_KINDS.map((_, i) => Math.round(unit(chances?.[i]) * 100));
}

/** What the developer panel shows, or `null` when the host is not in developer mode. */
export function devPanel(state) {
  if (!state.dev) return null;
  const owner = lobbyOwner(state.lobby);
  const enabled = owner !== null && owner === state.slot;
  const paused = state.devState?.paused === true;
  const percents = percentsFromChances(state.devState?.drops);
  // Chaser mode has its own bonuses: blocks hide nothing, so the drop chances do nothing
  const mode = state.observation?.config?.mode ?? state.lobby?.settings?.mode;
  const noDropEffect = mode === "chaser";
  const base = enabled ? "Developer mode: testing only." : "Only the lobby owner can use the developer panel.";
  return {
    marker: "DEV MODE",
    enabled,
    noDropEffect,
    hint: noDropEffect ? `${base} Chaser mode: bonuses appear on their own, the drop sliders have no effect.` : base,
    pause: { paused, label: paused ? "Resume bots" : "Pause bots" },
    sliders: DROP_KINDS.map((kind, i) => ({ kind, label: DROP_LABELS[i], percent: percents[i] })),
    reset: { label: "Reset drops" },
  };
}

/** The messages the developer panel sends. */
export function devMessages() {
  return {
    pause: (paused) => ({ type: "DevPauseBots", paused: paused === true }),
    drops: (percents) => ({ type: "DevSetDrops", chances: chancesFromPercents(percents) }),
    reset: () => ({ type: "DevResetDrops" }),
  };
}

/** The lobby as cards, one per slot: banner, name caption and the owner/you/ready marks. */
export function lobbyCards(lobby, you) {
  const owner = lobbyOwner(lobby);
  return (lobby?.slots ?? []).map((slot, i) => {
    const empty = slot.controller === "Empty";
    const name = empty ? "empty" : slot.controller === "Bot" ? "bot" : slot.name || `player ${i + 1}`;
    const team = lobby?.settings?.mode === "team" ? TEAM_LETTERS[i >> 1] : null;
    const marks = [i === you ? "you" : "", i === owner ? "owner" : "", slot.ready ? "ready" : "", team ? `team ${team}` : ""].filter(Boolean);
    return {
      slot: i,
      team,
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
      team: typeof p.team === "number" ? TEAM_LETTERS[p.team] ?? null : null,
      role: p.role === "Chaser" ? "chaser" : p.role === "Runner" ? "runner" : null,
      hearts: p.role === "Chaser" ? p.hearts : 0,
      shield: p.role === "Chaser" && p.shield_left > 0,
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

/** The banner shown for two seconds when the end pattern starts; empty for any other events. */
export function patternBanner(events) {
  return Array.isArray(events) && events.includes("PatternStarted") ? "The walls close in!" : "";
}

/** Time left in the round as `MM:SS`, `CLOSING` while the end pattern runs, or an empty string. */
export function timerText(obs) {
  const limit = obs?.config?.round_limit_ticks;
  const hz = obs?.config?.tick_hz;
  if (!(limit > 0) || !(hz > 0) || typeof obs.tick !== "number") return "";
  const closing = obs.config.closing_ticks;
  const mode = obs.config.mode;
  const pattern = (mode === undefined || mode === "normal" || mode === "team") && closing > 0 && closing < limit;
  if (pattern && obs.tick >= limit - closing) return "CLOSING";
  const left = Math.max(0, Math.ceil((limit - obs.tick) / hz));
  const two = (n) => String(n).padStart(2, "0");
  return `${two(Math.floor(left / 60))}:${two(left % 60)}`;
}

/** Rows of a character shown on its card: head and shoulders. */
export const CARD_ROWS = 7;

const LOAD_ERRORS = { 0: "ok", 1: "invalid text", 2: "invalid game data", 3: "arena too large", 4: "out of memory" };

/** Wraps the exports of the WebAssembly module (see contracts/wasm-render-api.md). */
export function createRenderer(ex) {
  const stride = ex.eksplode_re_stride();
  if (stride !== 16) {
    throw new Error("The graphics module and the page are out of sync (stride " + stride + ").");
  }
  const encoder = new TextEncoder();
  return {
    load(text) {
      const bytes = encoder.encode(text);
      const size = Math.max(bytes.length, 1);
      const ptr = ex.eksplode_re_alloc(size);
      if (ptr === 0) return 4;
      new Uint8Array(ex.memory.buffer, ptr, bytes.length).set(bytes);
      const code = ex.eksplode_re_load(ptr, bytes.length);
      ex.eksplode_re_free(ptr, size);
      return code;
    },
    describe(code) {
      return LOAD_ERRORS[code] ?? "unknown error";
    },
    frame(width, height, alpha) {
      const count = ex.eksplode_re_frame(width, height, alpha);
      const data = new Float32Array(ex.memory.buffer, ex.eksplode_re_output_ptr(), count * stride);
      return { count, stride, data };
    },
    pick(x, y) {
      return ex.eksplode_re_pick_tile(x, y);
    },
    /** Gives the module the looks of the four slots (plain objects). */
    setLooks(looks) {
      if (!ex.eksplode_re_set_looks) return -1;
      const bytes = encoder.encode(looksJson(looks));
      const size = Math.max(bytes.length, 1);
      const ptr = ex.eksplode_re_alloc(size);
      if (ptr === 0) return 4;
      new Uint8Array(ex.memory.buffer, ptr, bytes.length).set(bytes);
      const code = ex.eksplode_re_set_looks(ptr, bytes.length);
      ex.eksplode_re_free(ptr, size);
      return code;
    },
    /** The semi-3D character of a slot, `phase` seconds into its animation. */
    portrait3d(slot, width, height, phase) {
      const count = ex.eksplode_re_portrait3d ? ex.eksplode_re_portrait3d(slot, width, height, phase) : 0;
      const data = new Float32Array(ex.memory.buffer, count ? ex.eksplode_re_output_ptr() : 0, count * stride);
      return { count, stride, data };
    },
    /** Commands of a character fitted into width x height pixels; `rows` 0 is the whole figure. */
    portrait(slot, width, height, rows) {
      const count = ex.eksplode_re_portrait(slot, width, height, rows);
      const data = new Float32Array(ex.memory.buffer, ex.eksplode_re_output_ptr(), count * stride);
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
    roomPanel: $("room-panel"),
    roomName: $("room-name"),
    roomModes: $("room-modes"),
    roomModeHint: $("room-mode-hint"),
    roomSize: $("room-size"),
    roomMinutes: $("room-minutes"),
    roomFfLabel: $("room-ff-label"),
    roomFf: $("room-ff"),
    roomCreate: $("room-create"),
    joinRoom: $("join-room"),
    roomSummary: $("room-summary"),
    roomNote: $("room-note"),
    lobbyName: $("lobby-room-name"),
    lobbyMode: $("lobby-mode"),
    lobbySize: $("lobby-size"),
    lobbyMinutes: $("lobby-minutes"),
    lobbyFfLabel: $("lobby-ff-label"),
    lobbyFf: $("lobby-ff"),
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
    banner: $("banner"),
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
    hubSound: $("hub-sound"),
    hubControls: $("hub-controls"),
    hubDev: $("hub-dev"),
    hubResume: $("hub-resume"),
    controlsButton: $("controls-button"),
    controlsPanel: $("controls-panel"),
    controlsStatus: $("controls-status"),
    controlBindings: [...document.querySelectorAll("[data-bind-action]")],
    controlsReset: $("controls-reset"),
    controlsClose: $("controls-close"),
    controllerSelect: $("controller-select"),
    controllerStatus: $("controller-status"),
    musicVolume: $("music-volume"),
    effectsVolume: $("effects-volume"),
    mute: $("mute"),
    testSound: $("test-sound"),
    closeSound: $("close-sound"),
    devPanel: $("dev-panel"),
    devHint: $("dev-hint"),
    devPause: $("dev-pause"),
    devReset: $("dev-reset"),
    devSliders: $("dev-sliders"),
    app: $("app"),
    hub: $("menu-hub"),
    osk: $("osk"),
    oskTitle: $("osk-title"),
    oskPreview: $("osk-preview"),
    oskKeys: $("osk-keys"),
  };
  const storage = (() => {
    try {
      return window.localStorage;
    } catch {
      return undefined;
    }
  })();
  const ctx = el.canvas.getContext("2d");
  let keyboardBindings = loadBindings(storage);
  const keys = new KeyTracker(keyboardBindings);
  const gamepads = new GamepadTracker();
  const navTracker = new NavTracker();
  const navigationLayers = new NavigationLayers();
  let state = initialState();
  let socket = null;
  let renderer = null;
  let pageFocused = true;
  let lastSnapshotAt = 0;
  let logoPainted = false;
  let lobbyCanvases = [];
  let bannerTimer = null;
  let lastFocusRoot = null;
  let lastFocusStage = null;
  let oskState = null;
  let oskTarget = null;
  let oskAccepted = false;
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
    if (event.type === "Snapshot") {
      audio.handleEvents(event.events);
      const text = patternBanner(event.events);
      if (text) {
        el.banner.textContent = text;
        el.banner.hidden = false;
        clearTimeout(bannerTimer);
        bannerTimer = setTimeout(() => {
          el.banner.hidden = true;
        }, 2000);
      }
    }
    audio.setTune(tuneForStage(state.stage));
    if ((event.type === "Welcome" || event.type === "Lobby") && renderer) renderer.setLooks(state.looks);
    if (event.type === "Snapshot" && renderer) {
      const code = renderer.load(JSON.stringify(event.state));
      if (code !== 0) state = { ...state, notice: `Graphics error: ${renderer.describe(code)}` };
      lastSnapshotAt = performance.now();
    }
    updateUi();
  }

  // ---- developer mode: the panel exists only when the host said `dev` ----
  const devSend = devMessages();
  const devRows = DROP_KINDS.map((kind, i) => {
    const li = document.createElement("li");
    const label = document.createElement("label");
    label.textContent = DROP_LABELS[i];
    const input = document.createElement("input");
    input.type = "range";
    input.min = "0";
    input.max = "100";
    input.step = "1";
    input.id = `dev-drop-${kind}`;
    label.htmlFor = input.id;
    const value = document.createElement("output");
    input.addEventListener("input", () => {
      value.textContent = `${input.value}%`;
    });
    input.addEventListener("change", () => send(devSend.drops(devRows.map((r) => Number(r.input.value)))));
    li.append(label, input, value);
    el.devSliders.append(li);
    return { input, value };
  });
  el.devPause.addEventListener("click", () => {
    const panel = devPanel(state);
    if (panel) send(devSend.pause(!panel.pause.paused));
  });
  el.devReset.addEventListener("click", () => send(devSend.reset()));

  function renderDev() {
    const panel = devPanel(state);
    el.devPanel.hidden = !panel || !(state.stage === "lobby" || state.stage === "playing");
    if (!panel) return;
    el.devHint.textContent = panel.hint;
    el.devPause.textContent = panel.pause.label;
    el.devPause.disabled = !panel.enabled;
    el.devReset.disabled = !panel.enabled;
    panel.sliders.forEach((s, i) => {
      const row = devRows[i];
      row.input.disabled = !panel.enabled;
      if (document.activeElement !== row.input) row.input.value = String(s.percent);
      row.value.textContent = `${row.input.value}%`;
    });
  }

  function navigationRoot() {
    if (navigationLayers.top) return navigationLayers.top.root;
    if (!el.controlsPanel.hidden) return el.controlsPanel;
    if (!el.soundPanel.hidden) return el.soundPanel;
    for (const root of [el.roomPanel, el.joinPanel, el.editor, el.lobbyPanel, el.gamePanel, el.devPanel]) {
      if (root && !root.hidden) return root;
    }
    return null;
  }

  function hasNavigationLayer() {
    return navigationLayers.top !== null || !el.soundPanel.hidden || !el.controlsPanel.hidden ||
      Boolean(el.hub && !el.hub.hidden) || Boolean(el.osk && !el.osk.hidden);
  }

  function syncLayerInert() {
    const root = navigationLayers.top?.root ??
      (!el.controlsPanel.hidden ? el.controlsPanel : !el.soundPanel.hidden ? el.soundPanel : null);
    if (!el.app) return;
    for (const child of el.app.children) child.inert = Boolean(root && child !== root);
  }

  function focusFirst(root = navigationRoot()) {
    const target = focusTargets(root)[0] ?? null;
    target?.focus();
    return target;
  }

  function renderOsk() {
    el.oskPreview.textContent = oskState.buffer;
    el.oskKeys.replaceChildren(...OSK_ROWS.map((row, rowIndex) => {
      const rowElement = document.createElement("div");
      rowElement.className = "osk-row";
      rowElement.setAttribute("role", "row");
      rowElement.append(...row.map((key, columnIndex) => {
        const button = document.createElement("button");
        button.type = "button";
        button.className = "osk-key";
        button.id = key.type === "character" ? `osk-key-${rowIndex}-${columnIndex}` : `osk-${key.type}`;
        button.dataset.oskRow = String(rowIndex);
        button.dataset.oskColumn = String(columnIndex);
        button.dataset.oskType = key.type;
        if (key.type === "character") {
          button.dataset.oskValue = key.value;
          button.textContent = oskState.shift ? key.value.toUpperCase() : key.value;
        } else {
          button.textContent = key.type === "backspace" ? "Backspace"
            : key.type === "space" ? "Space"
            : key.type === "shift" ? "Shift"
            : key.type[0].toUpperCase() + key.type.slice(1);
        }
        button.setAttribute("aria-label", button.textContent);
        if (rowIndex === oskState.cursor.row && columnIndex === oskState.cursor.column) button.setAttribute("aria-current", "true");
        button.addEventListener("click", () => applyOskAction(key, button));
        return button;
      }));
      return rowElement;
    }));
  }

  function openOsk(target) {
    if (!el.osk || !target) return false;
    oskTarget = target;
    oskState = createOskState(target.id, target.value, target.maxLength > 0 ? target.maxLength : 24);
    const label = target.labels?.[0]?.textContent?.trim();
    el.oskTitle.textContent = label ? `Enter ${label.toLowerCase()}` : "Enter text";
    renderOsk();
    if (!openNavigationLayer("osk", el.osk, target)) return false;
    focusFirst(el.osk);
    return true;
  }

  function applyOskAction(action, button = null) {
    if (!oskState) return;
    if (button) {
      oskState.cursor = { row: Number(button.dataset.oskRow), column: Number(button.dataset.oskColumn) };
    }
    oskState = reduceOsk(oskState, action);
    if (oskState.closed) {
      oskTarget.value = oskState.result;
      oskTarget.dispatchEvent(new Event("input", { bubbles: true }));
      oskTarget.dispatchEvent(new Event("change", { bubbles: true }));
      oskAccepted = true;
      closeNavigationLayer();
      return;
    }
    renderOsk();
    const buttonToFocus = el.oskKeys.querySelector(`[data-osk-row="${oskState.cursor.row}"][data-osk-column="${oskState.cursor.column}"]`);
    buttonToFocus?.focus();
  }

  function openNavigationLayer(id, root, opener = document.activeElement) {
    if (!root || !navigationLayers.open(id, root, opener)) return false;
    root.hidden = false;
    syncLayerInert();
    navTracker.contextChanged();
    keys.clear();
    gamepads.clear();
    focusFirst(root);
    return true;
  }

  function openMenuHub(opener = document.activeElement) {
    el.hubDev.hidden = !state.dev;
    return openNavigationLayer("hub", el.hub, opener);
  }

  function closeNavigationLayer() {
    if (!navigationLayers.top) return false;
    const closed = navigationLayers.close();
    const opener = closed.closed.opener;
    if (closed.closed.id === "osk") {
      if (!oskAccepted && oskTarget && oskState) oskTarget.value = oskState.original;
      oskAccepted = false;
      oskState = null;
      oskTarget = null;
    }
    if (closed.closed.id !== "dev") closed.closed.root.hidden = true;
    else el.devPanel.classList.remove("navigation-layer");
    syncLayerInert();
    navTracker.contextChanged();
    keys.clear();
    gamepads.clear();
    const visibleOpener = focusTargets(document).includes(opener) ? opener : closed.focus;
    if (navigationLayers.top) {
      (visibleOpener ?? focusFirst(navigationLayers.top.root))?.focus?.();
    } else if (visibleOpener && focusTargets(document).includes(visibleOpener)) {
      visibleOpener.focus();
    } else if (state.stage !== "playing") {
      focusFirst();
    }
    el.soundButton.setAttribute("aria-expanded", String(navigationLayers.has("sound")));
    el.controlsButton.setAttribute("aria-expanded", String(navigationLayers.has("controls")));
    return true;
  }

  function handleNavigationInput(input) {
    if (input.menu) {
      if (navigationLayers.top?.id === "hub") closeNavigationLayer();
      else if (!navigationLayers.top) openMenuHub();
      return;
    }
    if (input.back) {
      if (navigationLayers.top) closeNavigationLayer();
      else if (state.stage === "lobby" && editing) el.doneLook.click();
      else if (!el.controlsPanel.hidden) el.controlsClose.click();
      else if (!el.soundPanel.hidden) el.closeSound.click();
      else openMenuHub();
      return;
    }
    const target = document.activeElement;
    if (input.direction) {
      const candidates = focusTargets(navigationRoot());
      if (moveGroupFocus(target, input.direction, candidates)) return;
      if ((input.direction === "left" || input.direction === "right") && target.matches?.(".swatch, #room-modes button")) {
        moveFocus(target, input.direction, candidates)?.click();
        return;
      }
      if ((input.direction === "left" || input.direction === "right") && adjustControl(target, input.direction)) return;
      const next = moveFormFocus(target, input.direction, candidates) ?? moveFocus(target, input.direction, candidates);
      if (oskState && next?.dataset?.oskRow !== undefined) {
        oskState.cursor = { row: Number(next.dataset.oskRow), column: Number(next.dataset.oskColumn) };
      }
    }
    if (input.confirm) {
      if (activateControl(target) === "osk" && el.osk) openOsk(target);
    }
  }

  function updateUi() {
    const previousKey = focusKey(document.activeElement);
    renderDev();
    el.notice.textContent = state.notice;
    const playing = state.stage === "playing";
    const inLobby = state.stage === "lobby";
    const roomMenu = state.stage === "idle" && state.menu === "room";
    el.roomPanel.hidden = !roomMenu;
    el.joinPanel.hidden = inLobby || playing || roomMenu;
    el.lobbyPanel.hidden = !inLobby;
    el.gamePanel.hidden = !playing;
    if (!playing) el.banner.hidden = true;
    el.editor.hidden = roomMenu || (!(!inLobby && !playing) && !(inLobby && editing));
    renderRoom();
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
        card.className = `card slot-${c.slot}${c.empty ? " empty" : ""}${c.team ? ` team-${c.team}` : ""}`;
        const portrait = document.createElement("canvas");
        portrait.className = "portrait";
        card.append(portrait);
        if (c.marks.length > 0) {
          const marks = document.createElement("div");
          marks.className = "marks";
          for (const m of c.marks) {
            const mark = document.createElement("span");
            mark.className = `mark mark-${m.replace(" ", "-")}`;
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
        const roleText = row.role === "chaser" ? ` [chaser ${"\u2665".repeat(row.hearts)}${row.shield ? " protected" : ""}]` : row.role === "runner" ? " [runner]" : "";
        text.textContent = row.alive
          ? `${row.label}${row.team ? ` [team ${row.team}]` : ""}${roleText}: bombs ${row.bombs}, range ${row.range}, speed ${row.speed}${row.effects.map((e) => `, ${e}`).join("")}`
          : `${row.label}${row.team ? ` [team ${row.team}]` : ""}${roleText}: eliminated`;
        if (row.role) li.classList.add(`role-${row.role}`);
        if (row.team) li.classList.add(`team-${row.team}`);
        li.append(chip, text);
        return li;
      }),
    );
    syncLayerInert();
    const root = navigationRoot();
    if (state.stage !== lastFocusStage || root !== lastFocusRoot) {
      navTracker.contextChanged();
      keys.clear();
      gamepads.clear();
      if (state.stage === "playing" && !navigationLayers.top) focusFirst(root);
      else if (state.stage === "closed") el.retry.focus();
      else focusFirst(root);
    } else if (previousKey) {
      restoreFocus(root, previousKey);
    }
    lastFocusRoot = root;
    lastFocusStage = state.stage;
  }

  function connect() {
    const name = el.name.value.trim();
    if (name.length < 1 || [...name].length > 16) {
      dispatch({ type: "Rejected", reason: "BadName" });
      state = { ...state, stage: "idle" };
      updateUi();
      return;
    }
    // the settings of the creation menu are used only when this join creates the room
    const creating = state.room !== null && state.room.open === false;
    if (creating) saveRoom(storage, roomDraft);
    dispatch({ type: "connecting" });
    const scheme = location.protocol === "https:" ? "wss" : "ws";
    socket = new WebSocket(`${scheme}://${location.host}/ws`);
    socket.onopen = () => {
      dispatch({ type: "socket-open" });
      const join = lookMessage("Join", draft, { version: PROTOCOL_VERSION, name });
      send(creating ? { ...join, room: roomPayload(roomDraft) } : join);
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

  // ---- the room: the creation menu before joining and the settings in the lobby ----
  const modeLabel = (m) => MODE_LABELS[m] ?? m;
  const sizeText = (s) => `${SIZE_LABELS[s]} (${SIZE_DIMENSIONS[s][0]} x ${SIZE_DIMENSIONS[s][1]})`;
  function fillOptions(select, values, text) {
    select.replaceChildren(
      ...values.map((v) => {
        const o = document.createElement("option");
        o.value = String(v);
        o.textContent = text(v);
        return o;
      }),
    );
  }
  for (const [size, minutes, mode] of [
    [el.roomSize, el.roomMinutes, null],
    [el.lobbySize, el.lobbyMinutes, el.lobbyMode],
  ]) {
    fillOptions(size, SIZES, sizeText);
    fillOptions(minutes, MINUTES, (m) => `${m} minutes`);
    if (mode) fillOptions(mode, PLAYABLE_MODES, modeLabel);
  }
  let roomDraft = loadRoom(storage);

  function renderRoomMenu() {
    el.roomName.value = roomDraft.name;
    el.roomSize.value = roomDraft.size;
    el.roomMinutes.value = String(roomDraft.minutes);
    el.roomFf.checked = roomDraft.friendly_fire;
    el.roomFfLabel.hidden = roomDraft.mode !== "team";
    el.roomModeHint.textContent = MODE_HINTS[roomDraft.mode] ?? "";
    el.roomModes.replaceChildren(
      ...PLAYABLE_MODES.map((mode) => {
        const b = document.createElement("button");
        b.type = "button";
        b.textContent = modeLabel(mode);
        b.setAttribute("role", "radio");
        b.dataset.navKey = `room-mode-${mode}`;
        b.setAttribute("aria-checked", String(mode === roomDraft.mode));
        b.addEventListener("click", () => editRoom({ type: "mode", value: mode }));
        return b;
      }),
    );
  }

  function editRoom(action) {
    const previousKey = focusKey(document.activeElement);
    roomDraft = roomReducer(roomDraft, action);
    renderRoomMenu();
    restoreFocus(el.roomModes, previousKey);
  }

  /** Settings shown in the lobby; the owner's controls follow the lobby message. */
  function renderRoom() {
    const controls = roomControls(state);
    el.roomSummary.textContent = state.stage === "lobby" ? controls.summary : "";
    el.roomNote.textContent = state.stage === "lobby" ? controls.note : "";
    const s = controls.settings;
    if (document.activeElement !== el.lobbyName) el.lobbyName.value = s.name;
    el.lobbyMode.value = s.mode;
    el.lobbySize.value = s.size;
    el.lobbyMinutes.value = String(s.minutes);
    el.lobbyFf.checked = s.friendly_fire;
    el.lobbyFfLabel.hidden = !controls.showFriendlyFire;
    for (const control of [el.lobbyName, el.lobbyMode, el.lobbySize, el.lobbyMinutes, el.lobbyFf]) {
      control.disabled = !controls.enabled;
    }
    const room = state.room;
    el.joinRoom.hidden = !(room && room.open);
    el.joinRoom.textContent = room && room.open ? `Joining: ${roomControlsSummary(room.settings)}` : "";
    if (state.stage === "idle" && state.menu === "room") renderRoomMenu();
  }

  const roomControlsSummary = (settings) => roomControls({ stage: "lobby", slot: null, lobby: { slots: [], settings } }).summary;

  function changeRoom(field, value) {
    if (state.stage !== "lobby") return;
    const settings = { ...sanitizeRoom(state.lobby?.settings), [field]: value };
    send(setRoomMessage(settings));
  }
  el.roomName.addEventListener("input", () => {
    roomDraft = roomReducer(roomDraft, { type: "name", value: el.roomName.value });
  });
  el.roomName.addEventListener("keydown", (e) => {
    if (e.key === "Enter") dispatch({ type: "room-confirm" });
  });
  el.roomSize.addEventListener("change", () => editRoom({ type: "size", value: el.roomSize.value }));
  el.roomMinutes.addEventListener("change", () => editRoom({ type: "minutes", value: el.roomMinutes.value }));
  el.roomFf.addEventListener("change", () => editRoom({ type: "friendly_fire", value: el.roomFf.checked }));
  el.roomCreate.addEventListener("click", () => dispatch({ type: "room-confirm" }));
  el.lobbyName.addEventListener("change", () => changeRoom("name", el.lobbyName.value));
  el.lobbyMode.addEventListener("change", () => changeRoom("mode", el.lobbyMode.value));
  el.lobbySize.addEventListener("change", () => changeRoom("size", el.lobbySize.value));
  el.lobbyMinutes.addEventListener("change", () => changeRoom("minutes", Number(el.lobbyMinutes.value)));
  el.lobbyFf.addEventListener("change", () => changeRoom("friendly_fire", el.lobbyFf.checked));

  /** Asks the server whether a room is open on a short-lived socket; no answer means an old server. */
  function queryRoom() {
    if (typeof WebSocket === "undefined") return;
    const scheme = location.protocol === "https:" ? "wss" : "ws";
    let probe;
    try {
      probe = new WebSocket(`${scheme}://${location.host}/ws`);
    } catch {
      return;
    }
    const timer = setTimeout(() => probe.close(), 1000);
    probe.onopen = () => probe.send(JSON.stringify({ type: "RoomQuery" }));
    probe.onmessage = (ev) => {
      try {
        const msg = JSON.parse(ev.data);
        if (msg.type !== "RoomInfo") return;
        clearTimeout(timer);
        roomDraft = loadRoom(storage, sanitizeRoom(msg.settings));
        dispatch(msg);
      } catch {
        // not a protocol message
      }
      probe.close();
    };
    probe.onerror = () => clearTimeout(timer);
  }

  el.join.addEventListener("click", connect);
  el.name.addEventListener("keydown", (e) => {
    if (e.key === "Enter") connect();
  });
  el.retry.addEventListener("click", () => {
    if (socket) socket.close();
    dispatch({ type: "retry" });
    queryRoom();
  });
  el.start.addEventListener("click", () => send({ type: "Start" }));
  el.addBot.addEventListener("click", () => send({ type: "AddBot" }));
  el.removeBot.addEventListener("click", () => send({ type: "RemoveBot" }));
  el.ready.addEventListener("click", () => {
    const mine = state.lobby?.slots[state.slot]?.ready ?? false;
    send({ type: "SetReady", ready: !mine });
  });

  // ---- the character editor: the draft is saved on this device and used by the next Join ----

  // ---- sound: generated effects and music, started by the first key press or click ----
  const audio = new AudioEngine({ storage });
  audio.attach(window);
  audio.setTune(tuneForStage(state.stage));
  el.musicVolume.value = String(Math.round(audio.settings.music * 100));
  el.effectsVolume.value = String(Math.round(audio.settings.effects * 100));
  el.mute.checked = audio.settings.muted;
  el.soundButton.addEventListener("click", () => {
    if (navigationLayers.top?.id === "sound") closeNavigationLayer();
    else if (!navigationLayers.has("sound")) {
      openNavigationLayer("sound", el.soundPanel, el.soundButton);
      el.soundButton.setAttribute("aria-expanded", "true");
    }
  });
  el.closeSound.addEventListener("click", () => {
    if (navigationLayers.top?.id === "sound") closeNavigationLayer();
    else {
      el.soundPanel.hidden = true;
      el.soundButton.setAttribute("aria-expanded", "false");
      syncLayerInert();
    }
  });
  el.musicVolume.addEventListener("input", () => audio.setSettings({ music: Number(el.musicVolume.value) / 100 }));
  el.effectsVolume.addEventListener("input", () => audio.setSettings({ effects: Number(el.effectsVolume.value) / 100 }));
  el.mute.addEventListener("change", () => audio.setSettings({ muted: el.mute.checked }));
  el.testSound.addEventListener("click", () => audio.testSound());

  let capturingAction = null;
  const actionLabels = {
    up: "Move up",
    down: "Move down",
    left: "Move left",
    right: "Move right",
    bomb: "Place bomb",
    punch: "Punch bomb",
  };
  const keyLabel = (code) =>
    code === "Space" ? "Space" : code.startsWith("Arrow") ? code.slice(5) : code.startsWith("Key") ? code.slice(3) : code;
  let resetArmed = false;
  let resetTimer = null;
  let spaceActivateTarget = null;

  function cancelReset(message = "") {
    clearTimeout(resetTimer);
    resetTimer = null;
    if (!resetArmed) return;
    resetArmed = false;
    el.controlsReset.textContent = "Restore defaults";
    if (message) el.controlsStatus.textContent = message;
  }

  function renderControlBindings() {
    for (const button of el.controlBindings) {
      const action = button.dataset.bindAction;
      button.textContent = `${actionLabels[action]}: ${keyboardBindings[action].map(keyLabel).join(" / ")}`;
    }
  }

  function readGamepads() {
    if (typeof navigator.getGamepads !== "function") return { state: "unsupported", gamepads: [] };
    try {
      return { state: "available", gamepads: Array.from(navigator.getGamepads() ?? []) };
    } catch {
      return { state: "blocked", gamepads: [] };
    }
  }

  function renderControllerPanel() {
    const snapshot = readGamepads();
    const devices = compatibleGamepads(snapshot.gamepads);
    el.controllerSelect.replaceChildren(
      ...devices.map((device) => {
        const option = document.createElement("option");
        option.value = String(device.index);
        option.textContent = device.id;
        return option;
      }),
    );
    el.controllerSelect.disabled = devices.length < 2;
    const selected = devices.find((device) => device.index === gamepads.selectedIndex) ?? devices[0];
    if (selected) {
      el.controllerSelect.value = String(selected.index);
      el.controllerStatus.textContent = `${selected.id} is connected and selected.`;
    } else if (snapshot.state === "unsupported") {
      el.controllerStatus.textContent = "Controller input is unavailable in this browser. Keyboard controls remain available.";
    } else if (snapshot.state === "blocked") {
      el.controllerStatus.textContent = "Controller access is blocked by this browser or page. Keyboard controls remain available.";
    } else if (snapshot.gamepads.some((gamepad) => gamepad && gamepad.connected !== false)) {
      el.controllerStatus.textContent = "A controller is connected but does not have a supported standard mapping. Keyboard controls remain available.";
    } else {
      el.controllerStatus.textContent = "No standard controller is visible. Focus this page and press a button or move a stick.";
    }
  }

  function closeControls() {
    capturingAction = null;
    cancelReset();
    for (const button of el.controlBindings) button.removeAttribute("aria-pressed");
    if (navigationLayers.top?.id === "controls") {
      closeNavigationLayer();
      return;
    }
    el.controlsPanel.hidden = true;
    el.controlsButton.setAttribute("aria-expanded", "false");
    syncLayerInert();
  }

  function setKeyboardBindings(bindings) {
    keyboardBindings = sanitizeBindings(bindings);
    keys.setBindings(keyboardBindings);
    saveBindings(storage, keyboardBindings);
    renderControlBindings();
  }

  renderControlBindings();
  el.controlsStatus.textContent = "Select an action to change its key.";
  el.controlsButton.addEventListener("click", () => {
    if (navigationLayers.top?.id === "controls") closeControls();
    else if (!navigationLayers.has("controls")) {
      openNavigationLayer("controls", el.controlsPanel, el.controlsButton);
      el.controlsButton.setAttribute("aria-expanded", "true");
      renderControllerPanel();
    }
  });
  el.hubSound.addEventListener("click", () => openNavigationLayer("sound", el.soundPanel, el.hubSound));
  el.hubControls.addEventListener("click", () => {
    if (openNavigationLayer("controls", el.controlsPanel, el.hubControls)) {
      el.controlsButton.setAttribute("aria-expanded", "true");
      renderControllerPanel();
    }
  });
  el.hubDev.addEventListener("click", () => {
    if (openNavigationLayer("dev", el.devPanel, el.hubDev)) el.devPanel.classList.add("navigation-layer");
  });
  el.hubResume.addEventListener("click", closeNavigationLayer);
  el.controlsClose.addEventListener("click", closeControls);
  el.controllerSelect.addEventListener("change", () => {
    const index = Number(el.controllerSelect.value);
    gamepads.select(index);
    keys.clear();
    renderControllerPanel();
  });
  window.addEventListener("gamepadconnected", renderControllerPanel);
  window.addEventListener("gamepaddisconnected", () => {
    gamepads.clear();
    renderControllerPanel();
  });
  for (const button of el.controlBindings) {
    button.addEventListener("click", () => {
      cancelReset();
      capturingAction = button.dataset.bindAction;
      el.controlsStatus.textContent = `Press a key for ${actionLabels[capturingAction].toLowerCase()}.`;
      for (const item of el.controlBindings) item.setAttribute("aria-pressed", String(item === button));
      button.focus();
    });
  }
  el.controlsReset.addEventListener("click", () => {
    if (!resetArmed) {
      resetArmed = true;
      el.controlsReset.textContent = "Confirm reset";
      el.controlsStatus.textContent = "Select Confirm reset within four seconds to restore defaults.";
      resetTimer = setTimeout(() => cancelReset("Reset cancelled."), 4000);
      return;
    }
    cancelReset();
    setKeyboardBindings(DEFAULT_BINDINGS);
    el.controlsStatus.textContent = "Default keyboard controls restored.";
  });

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

  function focusEditor() {
    (el.swatches.querySelector(".swatch.selected") ?? focusTargets(el.editor)[0])?.focus();
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
    if (editing) focusEditor();
  });
  el.doneLook.addEventListener("click", () => {
    editing = false;
    updateUi();
    el.changeLook.focus();
  });

  window.addEventListener("keydown", (e) => {
    if (capturingAction) {
      e.preventDefault();
      e.stopPropagation();
      if (e.repeat) return;
      if (e.key === "Escape") {
        capturingAction = null;
        el.controlsStatus.textContent = "Key capture cancelled.";
        for (const button of el.controlBindings) button.setAttribute("aria-pressed", "false");
        return;
      }
      const result = assignBinding(keyboardBindings, capturingAction, e.code);
      if (result.ok) {
        setKeyboardBindings(result.bindings);
        el.controlsStatus.textContent = `${actionLabels[capturingAction]} updated.`;
      } else {
        el.controlsStatus.textContent = result.reason === "conflict"
          ? "That key is already assigned to another action."
          : "That key is reserved and cannot be assigned.";
      }
      capturingAction = null;
      for (const button of el.controlBindings) button.setAttribute("aria-pressed", "false");
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      if (navigationLayers.top) closeNavigationLayer();
      else if (state.stage === "lobby" && editing) el.doneLook.click();
      else if (!el.controlsPanel.hidden) el.controlsClose.click();
      else if (!el.soundPanel.hidden) el.closeSound.click();
      else openMenuHub();
      return;
    }
    if (e.target.matches?.("button") && e.key === "Enter") {
      e.preventDefault();
      if (!e.repeat) e.target.click();
      return;
    }
    if (e.target.matches?.("button") && e.code === "Space") {
      e.preventDefault();
      if (!e.repeat) spaceActivateTarget = e.target;
      return;
    }
    if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key) && e.target.matches?.("button, [role='radio']")) {
      e.preventDefault();
      const direction = e.key.slice(5).toLowerCase();
      const candidates = focusTargets(navigationRoot());
      const next = moveGroupFocus(e.target, direction, candidates) ?? moveFocus(e.target, direction, candidates);
      if ((direction === "left" || direction === "right") && e.target.matches(".swatch, #room-modes button")) next?.click();
      return;
    }
    if (navigationLayers.top || !el.controlsPanel.hidden || !el.soundPanel.hidden) {
      if (keys.isBound(e.code) && !e.target.matches?.("input, select, textarea")) e.preventDefault();
      return;
    }
    if (state.stage === "playing") {
      // the sliders of the sound panel keep their arrow keys
      if (e.target instanceof HTMLInputElement) return;
      if (keys.isBound(e.code)) e.preventDefault();
      keys.press(e.code);
    } else if (state.stage === "lobby" && e.key === "Enter") {
      // Enter in a text field or a list belongs to that control
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
      send({ type: "Start" });
    }
  });
  window.addEventListener("keyup", (e) => {
    keys.release(e.code);
    if (e.code !== "Space" || !spaceActivateTarget) return;
    const target = spaceActivateTarget;
    spaceActivateTarget = null;
    if (target === document.activeElement && !target.disabled) target.click();
  });
  window.addEventListener("blur", () => {
    pageFocused = false;
    keys.clear();
    gamepads.clear();
    navTracker.clear();
  });
  window.addEventListener("focus", () => {
    pageFocused = true;
    keys.clear();
    gamepads.clear();
    navTracker.contextChanged();
  });
  document.addEventListener("visibilitychange", () => {
    keys.clear();
    gamepads.clear();
    if (document.hidden) navTracker.clear();
    else navTracker.contextChanged();
  });

  setInterval(() => {
    if (!pageFocused || document.hidden) {
      keys.clear();
      gamepads.clear();
      navTracker.clear();
      if (state.stage === "playing") send(inputMessage(state, { mv: null, bomb: false, punch: false }));
      return;
    }
    const pads = readGamepads().gamepads;
    const devices = compatibleGamepads(pads);
    const selected = devices.find((device) => device.index === gamepads.selectedIndex) ?? devices[0];
    const navPad = selected ? pads.find((pad) => pad?.index === selected.index) : null;
    const navInput = navTracker.sample(navPad, performance.now());
    if (navigationMode(state.stage, hasNavigationLayer()) === "navigation" || navInput.menu) {
      keys.clear();
      gamepads.clear();
      handleNavigationInput(navInput);
      if (state.stage === "playing") send(inputMessage(state, { mv: null, bomb: false, punch: false }));
      return;
    }
    if (state.stage !== "playing") return;
    const controllerInput = gamepads.sample(readGamepads().gamepads);
    const keyboardInput = keys.take();
    send(inputMessage(state, {
      mv: controllerInput.mv ?? keyboardInput.mv,
      bomb: controllerInput.bomb || keyboardInput.bomb,
      punch: controllerInput.punch || keyboardInput.punch,
    }));
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
      const response = await fetch("/eksplode_re_render.wasm");
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
  queryRoom();
  requestAnimationFrame(draw);
}

if (typeof document !== "undefined") start();
