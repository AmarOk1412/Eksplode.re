export const CONTROL_STORAGE_KEY = "eksplode.re.controls.v1";

export const DEFAULT_BINDINGS = Object.freeze({
	up: Object.freeze(["ArrowUp", "KeyW"]),
	down: Object.freeze(["ArrowDown", "KeyS"]),
	left: Object.freeze(["ArrowLeft", "KeyA"]),
	right: Object.freeze(["ArrowRight", "KeyD"]),
	bomb: Object.freeze(["Space", "KeyE"]),
	punch: Object.freeze(["KeyZ", "KeyQ"]),
});

const ACTIONS = Object.keys(DEFAULT_BINDINGS);
const RESERVED_CODES = new Set([
	"Escape",
	"Tab",
	"Enter",
	"Backspace",
	"Delete",
	"F1",
	"F2",
	"F3",
	"F4",
	"F5",
	"F6",
	"F7",
	"F8",
	"F9",
	"F10",
	"F11",
	"F12",
	"ShiftLeft",
	"ShiftRight",
	"ControlLeft",
	"ControlRight",
	"AltLeft",
	"AltRight",
	"MetaLeft",
	"MetaRight",
]);
const NAMED_CODES = new Set([
	"Minus",
	"Equal",
	"BracketLeft",
	"BracketRight",
	"Backslash",
	"Semicolon",
	"Quote",
	"Backquote",
	"Comma",
	"Period",
	"Slash",
	"NumpadAdd",
	"NumpadSubtract",
	"NumpadMultiply",
	"NumpadDivide",
	"NumpadEnter",
	"NumpadDecimal",
]);

const validCode = (code) =>
	typeof code === "string" &&
	!RESERVED_CODES.has(code) &&
	(/^(Arrow(Up|Down|Left|Right)|Space|Key[A-Z]|Digit[0-9]|Numpad[0-9])$/.test(code) ||
		NAMED_CODES.has(code));

const copyDefaults = () => Object.fromEntries(ACTIONS.map((action) => [action, [...DEFAULT_BINDINGS[action]]]));

/** Sanitizes a saved binding map, falling back action-by-action and avoiding collisions. */
export function sanitizeBindings(value) {
	if (!value || typeof value !== "object" || Array.isArray(value)) return copyDefaults();
	const bindings = {};
	const custom = new Set();
	for (const action of ACTIONS) {
		const saved = Array.isArray(value[action]) ? value[action] : [];
		const accepted = saved.filter((code, index) => validCode(code) && saved.indexOf(code) === index);
		if (accepted.length > 0) {
			bindings[action] = accepted;
			custom.add(action);
		} else {
			bindings[action] = [...DEFAULT_BINDINGS[action]];
		}
	}
	let conflicts = true;
	while (conflicts) {
		conflicts = false;
		const reset = new Set();
		for (let left = 0; left < ACTIONS.length; left += 1) {
			for (let right = left + 1; right < ACTIONS.length; right += 1) {
				const first = ACTIONS[left];
				const second = ACTIONS[right];
				if (!bindings[first].some((code) => bindings[second].includes(code))) continue;
				if (custom.has(first) && custom.has(second)) {
					reset.add(second);
				} else if (custom.has(first)) {
					reset.add(first);
				} else if (custom.has(second)) {
					reset.add(second);
				}
			}
		}
		for (const action of reset) {
			bindings[action] = [...DEFAULT_BINDINGS[action]];
			custom.delete(action);
			conflicts = true;
		}
	}
	return bindings;
}

/** Assigns one key to an action, leaving the original map unchanged on rejection. */
export function assignBinding(value, action, code) {
	const bindings = sanitizeBindings(value);
	if (!ACTIONS.includes(action)) return { ok: false, bindings, reason: "unknown-action" };
	if (!validCode(code)) return { ok: false, bindings, reason: "reserved" };
	if (ACTIONS.some((other) => other !== action && bindings[other].includes(code))) {
		return { ok: false, bindings, reason: "conflict" };
	}
	return { ok: true, bindings: { ...bindings, [action]: [code] }, reason: null };
}

/** Loads versioned local preferences; unavailable or invalid storage returns the defaults. */
export function loadBindings(storage) {
	try {
		const text = storage?.getItem(CONTROL_STORAGE_KEY);
		if (!text) return copyDefaults();
		const saved = JSON.parse(text);
		return saved?.version === 1 ? sanitizeBindings(saved.bindings) : copyDefaults();
	} catch {
		return copyDefaults();
	}
}

/** Persists sanitized preferences when browser storage is available. */
export function saveBindings(storage, value) {
	try {
		storage?.setItem(CONTROL_STORAGE_KEY, JSON.stringify({ version: 1, bindings: sanitizeBindings(value) }));
	} catch {
		// Private mode or full storage must not block keyboard play.
	}
}

/** Lists live controllers that the browser has mapped to the standard layout. */
export function compatibleGamepads(gamepads) {
	return Array.from(gamepads ?? [])
		.filter((gamepad) =>
			gamepad &&
			gamepad.connected !== false &&
			gamepad.mapping === "standard" &&
			Number.isInteger(gamepad.index) &&
			gamepad.index >= 0,
		)
		.map(({ index, id }) => ({ index, id: typeof id === "string" && id ? id : `Controller ${index}` }));
}

function pressed(button) {
	return button === true || button?.pressed === true || Number(button?.value) > 0.5;
}

function gamepadDirection(gamepad, deadZone) {
	const buttons = gamepad.buttons ?? [];
	if (pressed(buttons[12])) return "Up";
	if (pressed(buttons[13])) return "Down";
	if (pressed(buttons[14])) return "Left";
	if (pressed(buttons[15])) return "Right";

	const axes = gamepad.axes ?? [];
	const x = Number(axes[0]) || 0;
	const y = Number(axes[1]) || 0;
	if (Math.max(Math.abs(x), Math.abs(y)) <= deadZone) return null;
	if (Math.abs(x) > Math.abs(y)) return x < 0 ? "Left" : "Right";
	return y < 0 ? "Up" : "Down";
}

/** Samples a selected standard gamepad into the existing one-tick player input. */
export class GamepadTracker {
	constructor(deadZone = 0.25) {
		this.deadZone = Math.min(0.9, Math.max(0, deadZone));
		this.selectedIndex = null;
		this.previousBomb = false;
		this.previousPunch = false;
		this.suppressHeldActions = false;
	}

	select(index) {
		this.selectedIndex = Number.isInteger(index) && index >= 0 ? index : null;
		this.clear();
	}

	clear(suppressHeldActions = true) {
		this.previousBomb = false;
		this.previousPunch = false;
		this.suppressHeldActions = suppressHeldActions;
	}

	sample(gamepads) {
		const pads = Array.from(gamepads ?? []);
		const available = compatibleGamepads(pads);
		if (available.length === 0) {
			const suppressHeldActions = this.selectedIndex !== null || this.suppressHeldActions;
			this.selectedIndex = null;
			this.clear(suppressHeldActions);
			return { mv: null, bomb: false, punch: false };
		}
		if (!available.some((gamepad) => gamepad.index === this.selectedIndex)) {
			const suppressHeldActions = this.selectedIndex !== null || this.suppressHeldActions;
			this.selectedIndex = available[0].index;
			this.clear(suppressHeldActions);
		}

		const gamepad = pads.find((candidate) => candidate?.index === this.selectedIndex && candidate.connected !== false);
		if (!gamepad || gamepad.mapping !== "standard") {
			this.clear();
			return { mv: null, bomb: false, punch: false };
		}

		const bombDown = pressed(gamepad.buttons?.[0]);
		const punchDown = pressed(gamepad.buttons?.[1]);
		const mv = gamepadDirection(gamepad, this.deadZone);
		if (this.suppressHeldActions) {
			this.previousBomb = bombDown;
			this.previousPunch = punchDown;
			this.suppressHeldActions = false;
			return { mv, bomb: false, punch: false };
		}
		const input = {
			mv,
			bomb: bombDown && !this.previousBomb,
			punch: punchDown && !this.previousPunch,
		};
		this.previousBomb = bombDown;
		this.previousPunch = punchDown;
		return input;
	}
}