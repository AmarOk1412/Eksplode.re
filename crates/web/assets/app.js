// Bomberust page: lobby, keyboard, WebSocket and canvas painting of the commands returned by the
// WebAssembly graphics module. The page never decides game rules: it sends key states as `Input`
// messages and draws the `Snapshot`s the server sends.
//
// The helpers before `start()` are pure and exported for the node tests (crates/web/js-tests).

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
const TICK_MS = 50;
const PROTOCOL_VERSION = 1;

/** Keeps the held keys and turns them into one input per tick. */
export class KeyTracker {
  constructor() {
    this.held = [];
    this.bomb = false;
  }

  press(code) {
    if (code in DIRECTIONS) {
      if (!this.held.includes(code)) this.held.push(code);
    } else if (BOMB_KEYS.has(code)) {
      this.bomb = true;
    }
  }

  release(code) {
    this.held = this.held.filter((c) => c !== code);
  }

  clear() {
    this.held = [];
    this.bomb = false;
  }

  /** The latest pressed direction that is still held; a bomb press fires once. */
  take() {
    const last = this.held[this.held.length - 1];
    const input = { mv: last ? DIRECTIONS[last] : null, bomb: this.bomb };
    this.bomb = false;
    return input;
  }
}

export function initialState() {
  return { stage: "idle", slot: null, lobby: null, notice: "", observation: null, tick: 0 };
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
      return { ...state, stage: "lobby", slot: event.slot, lobby: event.lobby, notice: "" };
    case "Rejected":
      return { ...state, stage: "closed", notice: rejectText(event.reason) };
    case "Lobby": {
      const stage = state.stage === "playing" || state.stage === "closed" ? state.stage : "lobby";
      return { ...state, stage, lobby: event.lobby };
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

/** One row per participant for the status list next to the canvas. */
export function hudRows(state) {
  const players = state.observation?.players;
  const slots = state.lobby?.slots;
  if (!players || !slots) return [];
  const base = state.observation.config?.base_move_cooldown ?? 4;
  const rows = [];
  slots.forEach((slot, i) => {
    if (slot.controller === "Empty" || !players[i]) return;
    const p = players[i];
    const you = i === state.slot;
    const label = you ? "You" : slot.controller === "Bot" ? "Bot" : slot.name || `Player ${i + 1}`;
    rows.push({
      slot: i,
      label,
      alive: p.alive,
      bombs: `${p.bombs_active}/${p.bombs_max}`,
      range: p.range,
      speed: base + 1 - p.move_cooldown,
      you,
    });
  });
  return rows;
}

export function inputMessage(state, keys) {
  return { type: "Input", tick: state.tick, mv: keys.mv, bomb: keys.bomb };
}

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
      ctx.font = `bold ${Math.max(8, Math.round(data[o + 13]))}px system-ui, sans-serif`;
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
  };
  const ctx = el.canvas.getContext("2d");
  const keys = new KeyTracker();
  let state = initialState();
  let socket = null;
  let renderer = null;
  let lastSnapshotAt = 0;

  el.name.value = `Player${1 + Math.floor(Math.random() * 99)}`;

  function send(msg) {
    if (socket && socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(msg));
  }

  function dispatch(event) {
    state = reduce(state, event);
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
    el.join.hidden = state.stage === "closed";
    el.retry.hidden = state.stage !== "closed";
    el.join.disabled = state.stage === "connecting" || state.stage === "joining";
    el.name.disabled = el.join.disabled;

    const owner = lobbyOwner(state.lobby);
    const isOwner = owner !== null && owner === state.slot;
    el.start.disabled = !isOwner;
    el.addBot.disabled = !isOwner;
    el.removeBot.disabled = !isOwner;
    el.slots.replaceChildren(
      ...(state.lobby?.slots ?? []).map((slot, i) => {
        const li = document.createElement("li");
        const chip = document.createElement("span");
        chip.className = `chip slot-${i}`;
        chip.textContent = String(i + 1);
        const text = document.createElement("span");
        const who =
          slot.controller === "Empty"
            ? "empty"
            : slot.controller === "Bot"
              ? "bot"
              : slot.name || `player ${i + 1}`;
        const tags = [i === state.slot ? "you" : "", i === owner ? "owner" : "", slot.ready ? "ready" : ""]
          .filter(Boolean)
          .join(", ");
        text.textContent = tags ? `${who} (${tags})` : who;
        li.append(chip, text);
        return li;
      }),
    );
    el.hud.replaceChildren(
      ...hudRows(state).map((row) => {
        const li = document.createElement("li");
        li.className = row.alive ? "" : "dead";
        const chip = document.createElement("span");
        chip.className = `chip slot-${row.slot}`;
        chip.textContent = String(row.slot + 1);
        const text = document.createElement("span");
        text.textContent = row.alive
          ? `${row.label}: bombs ${row.bombs}, range ${row.range}, speed ${row.speed}`
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
      send({ type: "Join", version: PROTOCOL_VERSION, name });
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

  window.addEventListener("keydown", (e) => {
    if (state.stage === "playing") {
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
    } catch (e) {
      state = { ...state, stage: "closed", notice: `Graphics module failed: ${e.message}` };
    }
    updateUi();
  }

  updateUi();
  loadGraphics();
  requestAnimationFrame(draw);
}

if (typeof document !== "undefined") start();
