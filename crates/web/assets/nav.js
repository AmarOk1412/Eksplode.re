const FOCUS_SELECTOR = 'button, input, select, textarea, [tabindex]:not([tabindex="-1"])';
const DIRECTIONS = ["up", "down", "left", "right"];
const NEUTRAL_INPUT = Object.freeze({ direction: null, confirm: false, back: false, menu: false });
const OSK_ROWS = Object.freeze([
	Object.freeze([...[..."qwertyuiop"].map((value) => ({ type: "character", value }))]),
	Object.freeze([...[..."asdfghjkl"].map((value) => ({ type: "character", value }))]),
	Object.freeze([{ type: "shift" }, ...[..."zxcvbnm"].map((value) => ({ type: "character", value })), { type: "backspace" }]),
	Object.freeze([...[..."1234567890"].map((value) => ({ type: "character", value }))]),
	Object.freeze([{ type: "space" }, ...[...".,-_"].map((value) => ({ type: "character", value })), { type: "confirm" }, { type: "cancel" }]),
]);

function rectOf(element) {
	const target = element?.type === "checkbox" || element?.type === "radio"
		? element.closest?.("label") ?? element
		: element;
	if (typeof target?.getBoundingClientRect !== "function") return null;
	const rect = target.getBoundingClientRect();
	return rect && Number(rect.width) > 0 && Number(rect.height) > 0 ? rect : null;
}

function hasHiddenAncestor(element) {
	for (let current = element; current; current = current.parentElement) {
		if (current.hidden || current.inert || current.getAttribute?.("aria-hidden") === "true") return true;
	}
	return false;
}

export function isFocusTarget(element) {
	if (!element || element.disabled || element.getAttribute?.("aria-disabled") === "true") return false;
	if (hasHiddenAncestor(element) || !rectOf(element)) return false;
	const style = typeof getComputedStyle === "function" ? getComputedStyle(element) : null;
	return !style || (style.display !== "none" && style.visibility !== "hidden");
}

export function focusTargets(root = globalThis.document) {
	return [...(root?.querySelectorAll?.(FOCUS_SELECTOR) ?? [])].filter(isFocusTarget);
}

export function focusKey(element) {
	if (!element) return null;
	return element.id || element.dataset?.navKey || element.dataset?.bindAction || element.dataset?.colour || null;
}

export function restoreFocus(root, key) {
	if (!key) return null;
	const target = focusTargets(root).find((element) => focusKey(element) === key) ?? null;
	target?.focus?.();
	return target;
}

function center(rect) {
	return { x: (rect.left + rect.right) / 2, y: (rect.top + rect.bottom) / 2 };
}

export function moveFocus(current, direction, candidates = focusTargets()) {
	if (!DIRECTIONS.includes(direction)) return null;
	const targets = candidates.filter(isFocusTarget);
	if (targets.length === 0) return null;
	const currentIndex = targets.indexOf(current);
	if (currentIndex < 0) {
		targets[0].focus?.();
		return targets[0];
	}
	const originRect = rectOf(current);
	if (!originRect) return null;
	const origin = center(originRect);
	const forward = direction === "right" || direction === "down";
	const scored = targets.filter((target) => {
		if (target === current) return false;
		const point = center(rectOf(target));
		return direction === "right" ? point.x > origin.x
			: direction === "left" ? point.x < origin.x
			: direction === "down" ? point.y > origin.y
			: point.y < origin.y;
	}).map((target) => {
		const point = center(rectOf(target));
		const primary = direction === "right" || direction === "left" ? Math.abs(point.x - origin.x) : Math.abs(point.y - origin.y);
		const cross = direction === "right" || direction === "left" ? Math.abs(point.y - origin.y) : Math.abs(point.x - origin.x);
		return { target, score: primary + cross * 1.5 };
	}).sort((a, b) => a.score - b.score);
	const next = scored[0]?.target ?? targets[(currentIndex + (forward ? 1 : -1) + targets.length) % targets.length];
	next?.focus?.();
	return next ?? null;
}

export function moveFormFocus(current, direction, candidates = focusTargets()) {
	if (direction !== "up" && direction !== "down") return null;
	const groupOf = (element) => element?.getAttribute?.("role") === "radio"
		? element.closest?.('[role="radiogroup"]') ?? element
		: element;
	const fields = [];
	const groups = new Set();
	for (const element of candidates) {
		if (!isFocusTarget(element)) continue;
		if (element.matches?.(".osk-key")) continue;
		if (element.getAttribute?.("role") === "radio") {
			const group = groupOf(element);
			if (groups.has(group)) continue;
			groups.add(group);
			fields.push(element);
		} else if (element.tagName === "BUTTON" || element.tagName === "SELECT" ||
			(element.tagName === "INPUT" && ["text", "search"].includes(element.type))) {
			fields.push(element);
		}
	}
	const currentGroup = groupOf(current);
	const index = fields.findIndex((element) => element === current ||
		(element.getAttribute?.("role") === "radio" && groupOf(element) === currentGroup));
	if (index < 0) return null;
	const next = fields[index + (direction === "down" ? 1 : -1)];
	if (!next) return null;
	next.focus?.();
	return next;
}

export function moveGroupFocus(current, direction, candidates = focusTargets()) {
	if (direction !== "left" && direction !== "right") return null;
	const group = current?.closest?.(".buttons");
	if (!group) return null;
	const buttons = candidates.filter((element) => element.tagName === "BUTTON" && group.contains(element));
	const index = buttons.indexOf(current);
	if (index < 0) return null;
	const next = buttons[index + (direction === "right" ? 1 : -1)];
	if (!next) return null;
	next.focus?.();
	return next;
}

function firstFocusable(root) {
	return focusTargets(root)[0] ?? null;
}

export class NavigationLayers {
	constructor() {
		this.stack = [];
	}

	get top() {
		return this.stack.at(-1) ?? null;
	}

	has(id) {
		return this.stack.some((layer) => layer.id === id);
	}

	open(id, root, opener = null) {
		if (this.has(id)) return false;
		this.stack.push({ id, root, opener });
		return true;
	}

	close() {
		const layer = this.stack.pop();
		if (!layer) return { closed: null, focus: null };
		const target = isFocusTarget(layer.opener) ? layer.opener : firstFocusable(this.top?.root);
		return { closed: layer, focus: target ?? null };
	}

	back() {
		return this.close();
	}
}

export function navigationMode(stage, hasLayer) {
	return stage === "playing" && !hasLayer ? "gameplay" : "navigation";
}

function buttonPressed(button) {
	return button === true || button?.pressed === true || Number(button?.value) > 0.5;
}

function stickDirection(gamepad, previousDirection) {
	const axes = gamepad?.axes ?? [];
	const x = Number(axes[0]) || 0;
	const y = Number(axes[1]) || 0;
	const magnitude = Math.max(Math.abs(x), Math.abs(y));
	if (previousDirection && magnitude > 0.3) {
		if (previousDirection === "left" && x < 0 || previousDirection === "right" && x > 0) return previousDirection;
		if (previousDirection === "up" && y < 0 || previousDirection === "down" && y > 0) return previousDirection;
	}
	if (magnitude < 0.5) return null;
	if (Math.abs(x) > Math.abs(y)) return x < 0 ? "left" : "right";
	return y < 0 ? "up" : "down";
}

function sampleControls(gamepad, previousStick) {
	if (!gamepad || gamepad.connected === false || gamepad.mapping !== "standard") return { direction: null, a: false, b: false, start: false, stick: null };
	const buttons = gamepad.buttons ?? [];
	const dpad = buttonPressed(buttons[12]) ? "up"
		: buttonPressed(buttons[13]) ? "down"
		: buttonPressed(buttons[14]) ? "left"
		: buttonPressed(buttons[15]) ? "right"
		: null;
	return {
		direction: dpad ?? stickDirection(gamepad, previousStick),
		a: buttonPressed(buttons[0]),
		b: buttonPressed(buttons[1]),
		start: buttonPressed(buttons[9]),
		stick: dpad ? previousStick : stickDirection(gamepad, previousStick),
	};
}

export class NavTracker {
	constructor() {
		this.previous = { a: false, b: false, start: false };
		this.previousStick = null;
		this.direction = null;
		this.nextRepeatAt = 0;
		this.needsNeutral = false;
		this.blocked = new Set();
	}

	contextChanged() {
		this.needsNeutral = true;
		this.blocked.clear();
		this.previous = { a: false, b: false, start: false };
		this.direction = null;
	}

	clear() {
		this.previous = { a: false, b: false, start: false };
		this.previousStick = null;
		this.direction = null;
		this.nextRepeatAt = 0;
		this.needsNeutral = false;
		this.blocked.clear();
	}

	sample(gamepad, now = 0) {
		if (!gamepad || gamepad.connected === false || gamepad.mapping !== "standard") {
			this.clear();
			return { ...NEUTRAL_INPUT };
		}
		const current = sampleControls(gamepad, this.previousStick);
		this.previousStick = current.stick;
		if (this.needsNeutral) {
			this.blocked = new Set([
				...(current.direction ? [`direction:${current.direction}`] : []),
				...(current.a ? ["a"] : []),
				...(current.b ? ["b"] : []),
				...(current.start ? ["start"] : []),
			]);
			this.needsNeutral = false;
		}
		for (const key of [...this.blocked]) {
			const held = key === "a" ? current.a : key === "b" ? current.b : key === "start" ? current.start : current.direction === key.slice("direction:".length);
			if (!held) this.blocked.delete(key);
		}
		const wasBlocked = this.blocked.size > 0;
		const confirm = current.a && !this.previous.a && !wasBlocked;
		const back = current.b && !this.previous.b && !wasBlocked;
		const menu = current.start && !this.previous.start && !wasBlocked;
		this.previous = { a: current.a, b: current.b, start: current.start };
		let direction = null;
		if (!wasBlocked && current.direction) {
			if (current.direction !== this.direction) {
				this.direction = current.direction;
				this.nextRepeatAt = now + 400;
				direction = current.direction;
			} else if (now >= this.nextRepeatAt) {
				direction = current.direction;
				this.nextRepeatAt = now + 120;
			}
		} else if (!current.direction) {
			this.direction = null;
			this.nextRepeatAt = 0;
		}
		return { direction, confirm, back, menu };
	}
}

function dispatch(element, type) {
	if (typeof element.dispatchEvent !== "function") return;
	const event = typeof Event === "function" ? new Event(type, { bubbles: true }) : { type };
	element.dispatchEvent(event);
}

export function adjustControl(element, direction) {
	if (!element || (direction !== "left" && direction !== "right")) return false;
	const delta = direction === "right" ? 1 : -1;
	if (element.tagName === "SELECT") {
		const options = [...(element.options ?? [])].filter((option) => !option.disabled);
		if (!options.length) return false;
		const currentIndex = options.findIndex((option) => option.value === element.value);
		const next = options[(currentIndex + delta + options.length) % options.length];
		element.value = next.value;
		if ("selectedIndex" in element) element.selectedIndex = [...element.options].indexOf(next);
		dispatch(element, "change");
		return true;
	}
	if (element.type === "range") {
		const minimum = Number(element.min) || 0;
		const maximum = Number(element.max) || 100;
		const step = Number(element.step) > 0 ? Number(element.step) : 1;
		const value = Math.min(maximum, Math.max(minimum, Number(element.value) + step * delta));
		element.value = String(value);
		dispatch(element, "input");
		dispatch(element, "change");
		return true;
	}
	if (element.type === "radio") {
		const group = element.form?.querySelectorAll?.('input[type="radio"]') ?? document.querySelectorAll(`input[type="radio"][name="${element.name}"]`);
		const radios = [...group].filter((radio) => !radio.disabled);
		const current = radios.indexOf(element);
		if (current < 0 || radios.length < 2) return false;
		const next = radios[(current + delta + radios.length) % radios.length];
		next.checked = true;
		dispatch(next, "change");
		return true;
	}
	return false;
}

export function activateControl(element) {
	if (!element || element.disabled) return "ignored";
	if (element.type === "text" || element.type === "search" || element.tagName === "TEXTAREA") return "osk";
	if (element.tagName === "BUTTON" || element.type === "checkbox" || element.type === "radio") {
		element.click?.();
		return "activated";
	}
	return "ignored";
}

function sanitizeBuffer(value, maxLength) {
	return String(value ?? "").replace(/[\u0000-\u001f\u007f-\u009f]/g, "").slice(0, Math.max(0, maxLength));
}

function clampCursor(cursor = {}) {
	const row = Math.min(OSK_ROWS.length - 1, Math.max(0, Number(cursor.row) || 0));
	const column = Math.min(OSK_ROWS[row].length - 1, Math.max(0, Number(cursor.column) || 0));
	return { row, column };
}

export function createOskState(fieldId, original, maxLength, cursor = { row: 0, column: 0 }) {
	const safeOriginal = sanitizeBuffer(original, maxLength);
	return {
		fieldId,
		original: safeOriginal,
		buffer: safeOriginal,
		maxLength: Math.max(0, Number(maxLength) || 0),
		shift: false,
		cursor: clampCursor(cursor),
		result: null,
		closed: false,
	};
}

export function reduceOsk(state, action) {
	if (!state || state.closed || !action) return state;
	const next = { ...state, cursor: { ...state.cursor } };
	switch (action.type) {
		case "character": {
			const value = String(action.value ?? "").replace(/[\u0000-\u001f\u007f-\u009f]/g, "");
			const character = next.shift ? value.toUpperCase() : value.toLowerCase();
			next.buffer = sanitizeBuffer(next.buffer + character, next.maxLength);
			return next;
		}
		case "space":
			next.buffer = sanitizeBuffer(next.buffer + " ", next.maxLength);
			return next;
		case "backspace":
			next.buffer = next.buffer.slice(0, -1);
			return next;
		case "shift":
			next.shift = !next.shift;
			return next;
		case "move": {
			const rowDelta = action.direction === "down" ? 1 : action.direction === "up" ? -1 : 0;
			const colDelta = action.direction === "right" ? 1 : action.direction === "left" ? -1 : 0;
			const row = Math.min(OSK_ROWS.length - 1, Math.max(0, next.cursor.row + rowDelta));
			const column = Math.min(OSK_ROWS[row].length - 1, Math.max(0, next.cursor.column + colDelta));
			next.cursor = { row, column };
			return next;
		}
		case "confirm":
			return { ...next, result: next.buffer, closed: true };
		case "cancel":
			return { ...next, result: next.original, closed: true };
		default:
			return next;
	}
}

export function oskKeyAt(state) {
	return OSK_ROWS[state.cursor.row]?.[state.cursor.column] ?? OSK_ROWS[0][0];
}

export { OSK_ROWS };
