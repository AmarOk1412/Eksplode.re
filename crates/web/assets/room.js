// Room settings: the options, the menu state and the storage. Presentation only: the host reads
// every value leniently and decides what applies; the rules live in the server.

/** Every mode, by wire name, and the modes whose rules exist (the menu offers only these). */
export const MODES = ["normal", "team", "chaser"];
export const PLAYABLE_MODES = ["normal", "team", "chaser"];
export const MODE_LABELS = { normal: "Normal", team: "Team", chaser: "Chaser" };
export const MODE_HINTS = {
  normal: "Everybody against everybody. The board closes in during the last minute.",
  team: "2 against 2. Needs 4 players.",
  chaser: "One chaser with bombs and hearts against three runners. Needs 4 players.",
};

export const SIZES = ["small", "medium", "large"];
export const SIZE_LABELS = { small: "Small", medium: "Medium", large: "Large" };
export const SIZE_DIMENSIONS = { small: [15, 13], medium: [19, 15], large: [25, 19] };
export const MINUTES = [2, 3, 5, 10];
export const MAX_NAME = 24;
/** How well the room's bots play; the owner chooses, Medium by default. */
export const BOT_DIFFICULTIES = ["easy", "medium", "hard"];
export const BOT_DIFFICULTY_LABELS = { easy: "Easy", medium: "Medium", hard: "Hard" };

export const ROOM_KEY = "eksplode.re.room.v1";
const LEGACY_ROOM_KEY = "bomberust.room.v1";

/** The room of the game as it was before rooms: Normal, small board, 3 minutes, Medium bots. */
export function defaultRoom() {
  return { name: "", mode: "normal", size: "small", minutes: 3, friendly_fire: true, bot_difficulty: "medium" };
}

function cleanName(value) {
  if (typeof value !== "string") return "";
  const text = [...value].filter((c) => !/\p{Cc}/u.test(c)).join("").trim();
  return [...text].slice(0, MAX_NAME).join("").trim();
}

/** Valid settings from anything: bad fields fall back to their defaults one by one. */
export function sanitizeRoom(value) {
  const d = defaultRoom();
  if (value === null || typeof value !== "object" || Array.isArray(value)) return d;
  return {
    name: cleanName(value.name),
    mode: PLAYABLE_MODES.includes(value.mode) ? value.mode : d.mode,
    size: SIZES.includes(value.size) ? value.size : d.size,
    minutes: MINUTES.includes(value.minutes) ? value.minutes : d.minutes,
    friendly_fire: typeof value.friendly_fire === "boolean" ? value.friendly_fire : d.friendly_fire,
    bot_difficulty: BOT_DIFFICULTIES.includes(value.bot_difficulty) ? value.bot_difficulty : d.bot_difficulty,
  };
}

/** State of the menu: the settings being edited; actions change one value at a time. */
export function roomReducer(room, action) {
  switch (action.type) {
    case "name":
      return { ...room, name: cleanName(action.value) };
    case "mode":
      return PLAYABLE_MODES.includes(action.value) ? { ...room, mode: action.value } : room;
    case "size":
      return SIZES.includes(action.value) ? { ...room, size: action.value } : room;
    case "minutes": {
      const minutes = Number(action.value);
      return MINUTES.includes(minutes) ? { ...room, minutes } : room;
    }
    case "friendly_fire":
      return { ...room, friendly_fire: action.value === true };
    case "bot_difficulty":
      return BOT_DIFFICULTIES.includes(action.value) ? { ...room, bot_difficulty: action.value } : room;
    default:
      return room;
  }
}

/** One line for the lobby: `name - Normal - 19 x 15 - 5 min - Medium bots`, with friendly fire
 * for Team. */
export function roomLabel(settings) {
  const s = sanitizeRoom(settings);
  const [w, h] = SIZE_DIMENSIONS[s.size];
  const parts = [s.name, MODE_LABELS[s.mode], `${w} x ${h}`, `${s.minutes} min`];
  if (s.mode === "team") parts.push(`friendly fire ${s.friendly_fire ? "on" : "off"}`);
  parts.push(`${BOT_DIFFICULTY_LABELS[s.bot_difficulty]} bots`);
  return parts.filter((p) => p !== "").join(" - ");
}

/** The room carried by `Join`. */
export function roomPayload(room) {
  return sanitizeRoom(room);
}

/** The `SetRoom` message for new settings. */
export function setRoomMessage(room) {
  return { type: "SetRoom", settings: sanitizeRoom(room) };
}

/** The request that (re)fills the room list of the Join a game screen. */
export function listRoomsMessage() {
  return { type: "ListRooms", scope: "local" };
}

export function roomProtocolMessage(state, message, requestId) {
  if (["Join", "RoomQuery", "Leave"].includes(message.type)) return null;
  if (message.type === "Hello" || message.type === "Ping") return { ...message };
  if (["ListRooms", "CreateRoom", "JoinRoom", "ResumeRoom", "RequestStatus", "CancelRequest"].includes(message.type)) {
    return { ...message, request_id: requestId };
  }
  if (!state.room_id) return null;
  if (message.type === "Input") return { ...message, room_id: state.room_id };
  const scoped = { ...message, room_id: state.room_id, request_id: requestId };
  if (message.type === "SetRoom") {
    scoped.expected_revision = state.revision;
    scoped.capacity = message.capacity ?? state.capacity;
  }
  return scoped;
}

/** The settings saved on this device, or `fallback` when nothing usable is saved. */
export function loadRoom(storage, fallback = defaultRoom()) {
  try {
    const text = storage?.getItem(ROOM_KEY) ?? storage?.getItem(LEGACY_ROOM_KEY);
    if (text) return sanitizeRoom(JSON.parse(text));
  } catch {
    // unreadable storage or broken JSON: use the fallback
  }
  return sanitizeRoom(fallback);
}

/** Saves the settings; storage that is blocked or full is ignored. */
export function saveRoom(storage, room) {
  try {
    storage?.setItem(ROOM_KEY, JSON.stringify(sanitizeRoom(room)));
  } catch {
    // the settings just will not be remembered
  }
}

/** What the lobby shows about the room and what the owner may change. */
export function roomControls(state) {
  const settings = sanitizeRoom(state.lobby?.settings);
  const owner = state.lobby && Object.hasOwn(state.lobby, "owner_slot") ? state.lobby.owner_slot :
    state.lobby?.slots?.findIndex((s) => s.controller === "Local" || s.controller === "Remote");
  const enabled = state.stage === "lobby" && owner !== undefined && owner >= 0 && owner === state.slot;
  return {
    settings,
    summary: roomLabel(state.lobby?.settings),
    enabled,
    capacity: state.capacity ?? 4,
    occupied: state.lobby?.slots?.filter((slot) => slot.controller !== "Empty").length ?? 0,
    humans: state.lobby?.slots?.filter((slot) => slot.controller === "Local" || slot.controller === "Remote").length ?? 0,
    passwordRequired: state.password_required === true,
    showFriendlyFire: settings.mode === "team",
    note: settings.mode === "chaser" ? "One chaser with bombs and hearts, three runners without bombs. Roles are drawn at the start." : "",
  };
}

/** Whether the room list lets a player join `room`: bots give their seat to a joining human, so
 * only humans count against the capacity. */
export function roomJoinable(room) {
  if (room?.phase !== "lobby") return false;
  const humans = Number.isInteger(room.connected_humans)
    ? room.connected_humans + (room.reserved_humans ?? 0)
    : (room.occupied ?? 0) - (room.bots ?? 0);
  return humans < (room.capacity ?? 4);
}

const CAPACITIES = [1, 2, 3, 4];

/** The draft edited in the configuration panel, from the current lobby. */
export function roomConfigDraft(state) {
  const controls = roomControls(state);
  return { settings: controls.settings, capacity: controls.capacity, password: "", removePassword: false };
}

/** One change to the configuration draft; `cancel` drops it (nothing is sent). */
export function roomConfigReducer(draft, action) {
  if (!draft) return draft;
  switch (action.type) {
    case "cancel":
      return null;
    case "capacity": {
      const capacity = Number(action.value);
      if (!CAPACITIES.includes(capacity) || (draft.settings.mode !== "normal" && capacity !== 4)) return draft;
      return { ...draft, capacity };
    }
    case "password":
      return { ...draft, password: typeof action.value === "string" ? action.value : "", removePassword: false };
    case "removePassword":
      return { ...draft, removePassword: action.value === true, password: action.value === true ? "" : draft.password };
    default: {
      const settings = roomReducer(draft.settings, action);
      if (settings === draft.settings) return draft;
      // Team and Chaser need the four seats
      return { ...draft, settings, capacity: settings.mode === "normal" ? draft.capacity : 4 };
    }
  }
}

/** The single `SetRoom` that saving the panel sends; a password only when one was typed (or an
 * empty one to remove the protection). */
export function roomConfigMessage(draft) {
  if (!draft) return null;
  const message = { type: "SetRoom", settings: sanitizeRoom(draft.settings), capacity: draft.capacity };
  if (draft.password.length > 0) message.password = draft.password;
  else if (draft.removePassword) message.password = "";
  return message;
}
