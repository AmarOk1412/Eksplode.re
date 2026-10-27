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

export const ROOM_KEY = "eksplode.re.room.v1";
const LEGACY_ROOM_KEY = "bomberust.room.v1";

/** The room of the game as it was before rooms: Normal, small board, 3 minutes. */
export function defaultRoom() {
  return { name: "", mode: "normal", size: "small", minutes: 3, friendly_fire: true };
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
    default:
      return room;
  }
}

/** One line for the lobby: `name - Normal - 19 x 15 - 5 min`, with friendly fire for Team. */
export function roomLabel(settings) {
  const s = sanitizeRoom(settings);
  const [w, h] = SIZE_DIMENSIONS[s.size];
  const parts = [s.name, MODE_LABELS[s.mode], `${w} x ${h}`, `${s.minutes} min`];
  if (s.mode === "team") parts.push(`friendly fire ${s.friendly_fire ? "on" : "off"}`);
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
  const owner = state.lobby?.slots?.findIndex((s) => s.controller === "Local" || s.controller === "Remote");
  const enabled = state.stage === "lobby" && owner !== undefined && owner >= 0 && owner === state.slot;
  return {
    settings,
    summary: roomLabel(state.lobby?.settings),
    enabled,
    showFriendlyFire: settings.mode === "team",
    note: settings.mode === "chaser" ? "One chaser with bombs and hearts, three runners without bombs. Roles are drawn at the start." : "",
  };
}
