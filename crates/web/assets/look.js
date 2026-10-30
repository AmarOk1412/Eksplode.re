// Character looks (colour, head, face, costume): the options, the editor state and the storage. Presentation only: the server
// validates every look and settles colour conflicts; the drawing is done by the WebAssembly module.

export const COLOUR_NAMES = ["Blue", "Black", "Red", "Purple", "Green", "Orange", "Yellow", "Pink", "Teal", "White"];
export const HEADS = ["None", "Cap", "TopHat", "Crown", "Headphones"];
export const FACES = ["None", "Glasses", "Sunglasses", "Moustache", "Mask"];
export const COSTUMES = ["None", "Scarf", "Cape", "BowTie", "Medal"];

/** Swatch colours of the editor (the module draws the characters themselves). */
export const COLOUR_CSS = {
  Blue: "#5468d8",
  Black: "#0b0b0b",
  Red: "#a52c2c",
  Purple: "#c77ae6",
  Green: "#3a9d4a",
  Orange: "#e8761a",
  Yellow: "#e0c020",
  Pink: "#f08cc0",
  Teal: "#1f9d9d",
  White: "#e8e8ee",
};

export const LOOK_KEY = "eksplode.re.look.v1";
const LEGACY_LOOK_KEY = "bomberust.look.v1";

/** The look of a slot nobody customised: blue, black, red and purple characters. There is no
 * gender choice (feature 011): the graphics module picks the body silhouette from the slot. */
export function defaultLook(slot = 0) {
  const i = (((Number(slot) || 0) % 4) + 4) % 4;
  return { colour: COLOUR_NAMES[i], head: "None", face: "None", costume: "None" };
}

function pick(value, options, fallback) {
  return typeof value === "string" && options.includes(value) ? value : fallback;
}

/** A valid look from anything: bad fields fall back to the slot default one by one; the `gender`
 * of looks saved before feature 011 is dropped. */
export function sanitizeLook(value, slot = 0) {
  const d = defaultLook(slot);
  if (value === null || typeof value !== "object" || Array.isArray(value)) return d;
  return {
    colour: pick(value.colour, COLOUR_NAMES, d.colour),
    head: pick(value.head, HEADS, d.head),
    face: pick(value.face, FACES, d.face),
    costume: pick(value.costume, COSTUMES, d.costume),
  };
}

const OPTIONS = { colour: COLOUR_NAMES, head: HEADS, face: FACES, costume: COSTUMES };

/** State of the editor: the look being edited; actions change one choice at a time. */
export function editorReducer(look, action) {
  if (action.type === "random") {
    const rng = action.rng ?? Math.random;
    const choose = (list) => list[Math.min(list.length - 1, Math.floor(rng() * list.length))];
    return { colour: choose(COLOUR_NAMES), head: choose(HEADS), face: choose(FACES), costume: choose(COSTUMES) };
  }
  const options = OPTIONS[action.type];
  if (!options || !options.includes(action.value)) return look;
  return { ...look, [action.type]: action.value };
}

/** The look saved on this device, or the slot default when nothing usable is saved. */
export function loadLook(storage, slot = 0) {
  try {
    const text = storage?.getItem(LOOK_KEY) ?? storage?.getItem(LEGACY_LOOK_KEY);
    if (text) return sanitizeLook(JSON.parse(text), slot);
  } catch {
    // unreadable storage or broken JSON: use the default
  }
  return defaultLook(slot);
}

/** Saves the look; storage that is blocked or full is ignored. */
export function saveLook(storage, look) {
  try {
    storage?.setItem(LOOK_KEY, JSON.stringify(sanitizeLook(look)));
  } catch {
    // the look just will not be remembered
  }
}

/**
 * Commands of the preview character: the draft goes into slot 0 of the module, which draws it, and
 * `restore` (the looks of the lobby) is put back afterwards so the match view is not disturbed.
 */
export function previewCommands(renderer, draft, width, height, rows, restore = []) {
  renderer.setLooks([draft]);
  const commands = renderer.portrait(0, width, height, rows);
  renderer.setLooks(restore);
  return commands;
}

/** The JSON array of four looks that the module reads; bad entries become slot defaults. */
export function looksJson(looks) {
  return JSON.stringify([0, 1, 2, 3].map((i) => sanitizeLook(looks?.[i], i)));
}

/** A `Join` or `SetLook` message carrying the sanitized look. */
export function lookMessage(type, look, extra = {}) {
  return { type, ...extra, look: sanitizeLook(look) };
}
