const NEUTRAL_INPUT = Object.freeze({ mv: null, bomb: false, punch: false });

export function isTouchCapable({ primaryCoarse = false, anyCoarse = false, maxTouchPoints = 0, hasTouchEvent = false } = {}) {
	return primaryCoarse === true || anyCoarse === true || Number(maxTouchPoints) > 0 || hasTouchEvent === true;
}

export function directionFromVector(deltaX, deltaY, deadZone = 24) {
	if (!Number.isFinite(deltaX) || !Number.isFinite(deltaY) || !Number.isFinite(deadZone)) return null;
	if (Math.hypot(deltaX, deltaY) <= Math.max(0, deadZone)) return null;
	if (Math.abs(deltaX) >= Math.abs(deltaY)) return deltaX < 0 ? "Left" : "Right";
	return deltaY < 0 ? "Up" : "Down";
}

export class TouchInputTracker {
	constructor(deadZone = 24) {
		this.deadZone = Math.max(0, deadZone);
		this.joystickPointer = null;
		this.origin = null;
		this.direction = null;
		this.actionPointers = new Map();
		this.pendingBomb = false;
		this.pendingPunch = false;
	}

	startJoystick(pointerId, x, y) {
		if (this.joystickPointer !== null || !Number.isFinite(x) || !Number.isFinite(y)) return false;
		this.joystickPointer = pointerId;
		this.origin = { x, y };
		this.direction = null;
		return true;
	}

	moveJoystick(pointerId, x, y) {
		if (pointerId !== this.joystickPointer || !Number.isFinite(x) || !Number.isFinite(y)) return false;
		this.direction = directionFromVector(x - this.origin.x, y - this.origin.y, this.deadZone);
		return true;
	}

	endJoystick(pointerId) {
		if (pointerId !== this.joystickPointer) return false;
		this.joystickPointer = null;
		this.origin = null;
		this.direction = null;
		return true;
	}

	pressAction(pointerId, action) {
		if ((action !== "bomb" && action !== "punch") || this.actionPointers.has(pointerId)) return false;
		this.actionPointers.set(pointerId, action);
		if (action === "bomb") this.pendingBomb = true;
		else this.pendingPunch = true;
		return true;
	}

	releaseAction(pointerId) {
		return this.actionPointers.delete(pointerId);
	}

	take() {
		const input = {
			mv: this.direction,
			bomb: this.pendingBomb,
			punch: this.pendingPunch,
		};
		this.pendingBomb = false;
		this.pendingPunch = false;
		return input;
	}

	clear() {
		this.joystickPointer = null;
		this.origin = null;
		this.direction = null;
		this.actionPointers.clear();
		this.pendingBomb = false;
		this.pendingPunch = false;
	}
}

export { NEUTRAL_INPUT };
