// Drives a real headless browser through the Chrome DevTools Protocol: joins the server's page,
// starts a match, plays a few keys and saves screenshots. Optional manual-style check, not part of
// `cargo test`. Usage:
//   node scripts/browser-smoke.mjs http://127.0.0.1:8080/ [out-dir]
// MODE=normal|team picks the mode of the room (default normal).
// SIZE=small|medium|large picks the board of the room (default medium).
// CLOSING=1 plays a 2-minute round and saves screenshots of the end pattern (takes about 80 s).
// Needs a Chromium based browser (BROWSER=/path/to/browser, default chromium-browser).
import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";

const url = process.argv[2] ?? "http://127.0.0.1:8080/";
const out = process.argv[3] ?? "browser-smoke";
const browser = process.env.BROWSER ?? "chromium-browser";
const port = 9300 + Math.floor(Math.random() * 500);
const profile = mkdtempSync(path.join(homedir(), ".eksplode_re-smoke-"));
mkdirSync(out, { recursive: true });

const child = spawn(
  browser,
  [
    "--headless=new",
    "--no-sandbox",
    "--disable-gpu",
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${profile}`,
    "--window-size=1100,800",
    "about:blank",
  ],
  { stdio: "ignore" },
);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const size = process.env.SIZE ?? "medium";
const other = size === "large" ? "small" : "large";
const dimensions = { small: "15 x 13", medium: "19 x 15", large: "25 x 19" };

async function targetSocket() {
  for (let i = 0; i < 50; i += 1) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      const page = list.find((t) => t.type === "page");
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      /* browser still starting */
    }
    await sleep(200);
  }
  throw new Error("the browser did not start");
}

async function main() {
  const ws = new WebSocket(await targetSocket());
  await new Promise((resolve) => ws.addEventListener("open", resolve));
  let id = 0;
  const pending = new Map();
  const consoleLines = [];
  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) pending.get(msg.id)(msg);
    if (msg.method === "Runtime.consoleAPICalled") {
      consoleLines.push(msg.params.args.map((a) => a.value ?? a.description).join(" "));
    }
    if (msg.method === "Runtime.exceptionThrown") {
      consoleLines.push("EXCEPTION " + JSON.stringify(msg.params.exceptionDetails.exception?.description));
    }
  });
  const call = (method, params = {}) =>
    new Promise((resolve) => {
      id += 1;
      pending.set(id, resolve);
      ws.send(JSON.stringify({ id, method, params }));
    });
  const evaluate = async (expression) =>
    (await call("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result?.result?.value;
  const waitFor = async (expression, what, ms = 8000) => {
    for (let t = 0; t < ms; t += 100) {
      if (await evaluate(expression)) return;
      await sleep(100);
    }
    throw new Error(`timed out waiting for ${what}; console: ${consoleLines.join(" | ")}`);
  };
  const shot = async (name) => {
    const r = await call("Page.captureScreenshot", { format: "png" });
    writeFileSync(path.join(out, name), Buffer.from(r.result.data, "base64"));
    console.log("saved", path.join(out, name));
  };
  const key = async (code, keyName, vk) => {
    for (const type of ["keyDown", "keyUp"]) {
      await call("Input.dispatchKeyEvent", { type, code, key: keyName, windowsVirtualKeyCode: vk });
      await sleep(type === "keyDown" ? 250 : 30);
    }
  };
  const shiftTab = async () => {
    await call("Input.dispatchKeyEvent", { type: "keyDown", code: "ShiftLeft", key: "Shift", windowsVirtualKeyCode: 16 });
    await call("Input.dispatchKeyEvent", { type: "keyDown", code: "Tab", key: "Tab", windowsVirtualKeyCode: 9, modifiers: 8 });
    await call("Input.dispatchKeyEvent", { type: "keyUp", code: "Tab", key: "Tab", windowsVirtualKeyCode: 9, modifiers: 8 });
    await call("Input.dispatchKeyEvent", { type: "keyUp", code: "ShiftLeft", key: "Shift", windowsVirtualKeyCode: 16 });
  };
  const setFakeGamepad = async (pressedButtons = [], axes = [0, 0], mapping = "standard", count = 1) => {
    if (await evaluate("typeof window.setSmokeGamepads !== 'function'")) {
      await evaluate(`(() => {
        let gamepads = [];
        Object.defineProperty(Navigator.prototype, "getGamepads", { configurable: true, value: () => gamepads });
        window.setSmokeGamepads = (next) => {
          const wasConnected = gamepads.length > 0;
          gamepads = next;
          window.dispatchEvent(new Event(next.length ? "gamepadconnected" : wasConnected ? "gamepaddisconnected" : "gamepadconnected"));
        };
        window.__smokeInputs ??= [];
        if (!window.__smokeSendWrapped) {
          const send = WebSocket.prototype.send;
          WebSocket.prototype.send = function (data) {
            try {
              const message = JSON.parse(data);
              if (message.type === "Input") window.__smokeInputs.push(message);
            } catch {}
            return send.call(this, data);
          };
          window.__smokeSendWrapped = true;
        }
        return 1;
      })()`);
    }
    const pads = Array.from({ length: count }, (_, index) => ({
      index,
      id: `Smoke Xbox controller ${index + 1}`,
      mapping,
      connected: true,
      buttons: Array.from({ length: 16 }, (__, button) => ({ pressed: pressedButtons.includes(button), value: 0 })),
      axes,
    }));
    await evaluate(`window.setSmokeGamepads(${JSON.stringify(pads)}); 1`);
  };
  const pulseGamepad = async (buttons) => {
    await setFakeGamepad(buttons, [0, 0], "standard", 2);
    await sleep(120);
    await setFakeGamepad([], [0, 0], "standard", 2);
    await sleep(100);
  };

  await call("Page.addScriptToEvaluateOnNewDocument", {
    source: `(() => {
      let gamepads = [];
      Object.defineProperty(Navigator.prototype, "getGamepads", { configurable: true, value: () => gamepads });
      window.__smokeInputs = [];
      const send = WebSocket.prototype.send;
      WebSocket.prototype.send = function (data) {
        try {
          const message = JSON.parse(data);
          if (message.type === "Input") window.__smokeInputs.push(message);
        } catch {}
        return send.call(this, data);
      };
      window.setSmokeGamepads = (next) => {
        const wasConnected = gamepads.length > 0;
        gamepads = next;
        window.dispatchEvent(new Event(next.length ? "gamepadconnected" : wasConnected ? "gamepaddisconnected" : "gamepadconnected"));
      };
    })();`,
  });
  await call("Page.navigate", { url });
  await waitFor("document.readyState === 'complete' && !document.getElementById('join').disabled", "the page");

  if (["keyboard", "controller"].includes(process.env.MAIN_FLOW)) {
    const flow = process.env.MAIN_FLOW;
    await waitFor("!document.getElementById('room-panel').hidden", "fresh room creation screen");
    if (flow === "keyboard") {
      await waitFor("document.activeElement.id === 'room-name'", "initial keyboard focus on room name");
      await key("KeyR", "r", 82);
      await key("Tab", "Tab", 9);
      await key("ArrowRight", "ArrowRight", 39);
      await waitFor("document.querySelector('#room-modes [aria-checked=true]')?.textContent.toLowerCase() === 'team'", "keyboard selecting Team mode");
      await key("ArrowLeft", "ArrowLeft", 37);
      await waitFor("document.querySelector('#room-modes [aria-checked=true]')?.textContent.toLowerCase() === 'normal'", "keyboard returning to Normal mode");
      for (let i = 0; i < 16 && await evaluate("document.activeElement.id !== 'room-create'"); i += 1) await key("Tab", "Tab", 9);
      await waitFor("document.activeElement.id === 'room-create'", "keyboard focus on Create room");
      await key("Enter", "Enter", 13);
      await waitFor("!document.getElementById('join-panel').hidden", "keyboard opening the join screen");
      await waitFor("document.activeElement.id === 'name'", "keyboard focus on player name");
      await key("End", "End", 35);
      await key("KeyX", "x", 88);
      await key("Tab", "Tab", 9);
      await waitFor("document.activeElement.id === 'join'", "keyboard focus on Join");
      await key("Enter", "Enter", 13);
      await waitFor("!document.getElementById('lobby-panel').hidden", "keyboard entering the lobby");
      for (let i = 0; i < 20 && await evaluate("document.activeElement.id !== 'start'"); i += 1) await key("Tab", "Tab", 9);
      await waitFor("document.activeElement.id === 'start'", "keyboard focus on Start");
      await key("Enter", "Enter", 13);
      await waitFor("!document.getElementById('game-panel').hidden", "keyboard starting the match");
      const inputsBeforeMenu = await evaluate("window.__smokeInputs.length");
      await key("Escape", "Escape", 27);
      await waitFor("!document.getElementById('menu-hub').hidden", "keyboard opening the in-match hub");
      await sleep(120);
      const unsafeInputs = await evaluate(`window.__smokeInputs.slice(${inputsBeforeMenu}).filter((input) => input.mv || input.bomb || input.punch).length`);
      if (unsafeInputs) throw new Error("gameplay input escaped while the in-match hub was open");
      await key("Escape", "Escape", 27);
      await waitFor("document.getElementById('menu-hub').hidden", "keyboard resuming the match");
      await waitFor("document.activeElement.id === 'view'", "keyboard focus returning to the arena");
    } else {
      await setFakeGamepad([], [0, 0], "standard", 1);
      await sleep(100);
      await pulseGamepad([0]);
      await waitFor("!document.getElementById('osk').hidden", "controller opening room-name entry");
      await pulseGamepad([0]);
      await waitFor("document.getElementById('osk-preview').textContent.toLowerCase().includes('q')", "controller entering room text");
      for (let i = 0; i < 4; i += 1) await pulseGamepad([13]);
      for (let i = 0; i < 5; i += 1) await pulseGamepad([15]);
      await waitFor("document.activeElement.id === 'osk-confirm'", "controller focus on room-name Confirm");
      await pulseGamepad([0]);
      await waitFor("document.getElementById('osk').hidden", "controller confirming room-name entry");
      await setFakeGamepad([], [0, 0], "standard", 1);
      await sleep(100);
      await pulseGamepad([13]);
      await waitFor("document.activeElement.getAttribute('role') === 'radio'", "controller focus on room mode");
      await pulseGamepad([15]);
      await waitFor("document.querySelector('#room-modes [aria-checked=true]')?.textContent.toLowerCase() === 'team'", "controller selecting Team mode");
      await pulseGamepad([14]);
      await waitFor("document.querySelector('#room-modes [aria-checked=true]')?.textContent.toLowerCase() === 'normal'", "controller returning to Normal mode");
      await pulseGamepad([13]);
      await waitFor("document.activeElement.id === 'room-size'", "controller focus on board size");
      await pulseGamepad([15]);
      await pulseGamepad([13]);
      await waitFor("document.activeElement.id === 'room-minutes'", "controller focus on round time");
      for (let i = 0; i < 4 && await evaluate("document.activeElement.id !== 'room-create'"); i += 1) await pulseGamepad([13]);
      await waitFor("document.activeElement.id === 'room-create'", "controller focus on Create room");
      await pulseGamepad([0]);
      await waitFor("!document.getElementById('join-panel').hidden", "controller opening the join screen");
      await pulseGamepad([0]);
      await waitFor("!document.getElementById('osk').hidden", "controller opening player-name entry");
      await pulseGamepad([0]);
      for (let i = 0; i < 4; i += 1) await pulseGamepad([13]);
      for (let i = 0; i < 5; i += 1) await pulseGamepad([15]);
      await pulseGamepad([0]);
      await waitFor("document.getElementById('osk').hidden", "controller confirming the player name");
      await setFakeGamepad([], [0, 0], "standard", 1);
      await sleep(100);
      await pulseGamepad([13]);
      await waitFor("document.activeElement.id === 'join'", "controller focus on Join");
      await pulseGamepad([0]);
      await waitFor("!document.getElementById('lobby-panel').hidden", "controller entering the lobby");
      for (let i = 0; i < 12 && await evaluate("document.activeElement.id !== 'start'"); i += 1) {
        await pulseGamepad([13]);
      }
      await waitFor("document.activeElement.id === 'start'", "controller focus on Start");
      await pulseGamepad([0]);
      await waitFor("!document.getElementById('game-panel').hidden", "controller starting the match");
      await setFakeGamepad([], [0, 0], "standard", 1);
      await sleep(100);
      const inputsBeforeMenu = await evaluate("window.__smokeInputs.length");
      await pulseGamepad([9]);
      await waitFor("!document.getElementById('menu-hub').hidden", "controller opening the in-match hub");
      await sleep(120);
      const unsafeInputs = await evaluate(`window.__smokeInputs.slice(${inputsBeforeMenu}).filter((input) => input.mv || input.bomb || input.punch).length`);
      if (unsafeInputs) throw new Error("gameplay input escaped while the controller hub was open");
      await pulseGamepad([9]);
      await waitFor("document.getElementById('menu-hub').hidden", "controller resuming the match");
      await waitFor("document.activeElement.id === 'view'", "controller focus returning to the arena");
    }
    console.log(`${flow} main-screen flow passed`);
    return;
  }

  // room settings (feature 004): an empty server shows the creation menu first
  await waitFor("!document.getElementById('room-panel').hidden", "the room creation menu");
  await shot("0-room-menu.png");
  await evaluate(`(() => {
    document.getElementById('room-name').value = 'Smoke room';
    document.getElementById('room-name').dispatchEvent(new Event('input'));
    for (const [id, value] of [['room-size', '${size}'], ['room-minutes', '${process.env.CLOSING ? 2 : 5}']]) {
      const select = document.getElementById(id);
      select.value = value;
      select.dispatchEvent(new Event('change'));
    }
    for (const b of document.querySelectorAll('#room-modes button')) {
      if (b.textContent.toLowerCase() === '${process.env.MODE ?? 'normal'}') b.click();
    }
    document.getElementById('room-create').click();
    return 1;
  })()`);
  await waitFor("!document.getElementById('join-panel').hidden && document.getElementById('room-panel').hidden", "the join screen after the menu");
  await shot("1-join.png");

  // mouse-free settings: Escape and keyboard focus, then the synthetic controller
  await key("Escape", "Escape", 27);
  await waitFor("!document.getElementById('menu-hub').hidden", "the keyboard-opened menu hub");
  await key("Enter", "Enter", 13);
  await waitFor("!document.getElementById('sound-panel').hidden", "Sound from the menu hub");
  await waitFor("document.getElementById('menu-hub').hidden", "hiding the hub while Sound is open");
  const musicBefore = await evaluate("Number(document.getElementById('music-volume').value)");
  await key("ArrowRight", "ArrowRight", 39);
  await waitFor(`Number(document.getElementById('music-volume').value) !== ${musicBefore}`, "keyboard sound adjustment");
  await key("Escape", "Escape", 27);
  await waitFor("document.activeElement.id === 'hub-sound'", "focus returning to Sound in the hub");
  await key("ArrowDown", "ArrowDown", 40);
  await waitFor("document.activeElement.id === 'hub-controls'", "keyboard focus moving through the hub");
  await key("Enter", "Enter", 13);
  await waitFor("!document.getElementById('controls-panel').hidden", "Controls from the hub");
  await waitFor("document.getElementById('menu-hub').hidden", "hiding the hub while Controls is open");
  const upBindingBefore = await evaluate("document.querySelector('[data-bind-action=up]').textContent");
  await key("Enter", "Enter", 13);
  await key("Escape", "Escape", 27);
  await waitFor("document.getElementById('controls-status').textContent.includes('cancelled')", "Escape cancelling key capture");
  await key("Tab", "Tab", 9);
  await key("Tab", "Tab", 9);
  await key("Tab", "Tab", 9);
  await key("Tab", "Tab", 9);
  await key("Tab", "Tab", 9);
  await key("Tab", "Tab", 9);
  await key("Enter", "Enter", 13);
  await waitFor("document.getElementById('controls-reset').textContent === 'Confirm reset'", "the reset confirmation step");
  await key("Enter", "Enter", 13);
  await waitFor("JSON.parse(localStorage.getItem('eksplode.re.controls.v1')).bindings.up.includes('ArrowUp')", "restored default binding");
  if (await evaluate("document.querySelector('[data-bind-action=up]').textContent") !== upBindingBefore) throw new Error("Escape changed the captured key");
  await key("Escape", "Escape", 27);
  await waitFor("!document.getElementById('menu-hub').hidden", "returning to the hub from Controls");
  await key("Escape", "Escape", 27);
  await waitFor("document.getElementById('menu-hub').hidden", "closing the menu hub");

  await setFakeGamepad([], [0, 0], "standard", 2);
  await sleep(100);
  await pulseGamepad([9]);
  await waitFor("!document.getElementById('menu-hub').hidden", "controller Start opening the hub");
  await pulseGamepad([0]);
  await waitFor("!document.getElementById('sound-panel').hidden", "controller A opening Sound");
  await waitFor("document.getElementById('menu-hub').hidden", "controller hiding the hub while Sound is open");
  const controllerMusicBefore = await evaluate("Number(document.getElementById('music-volume').value)");
  await pulseGamepad([15]);
  await waitFor(`Number(document.getElementById('music-volume').value) !== ${controllerMusicBefore}`, "controller sound adjustment");
  await pulseGamepad([13]);
  await pulseGamepad([13]);
  await pulseGamepad([0]);
  await waitFor("document.getElementById('mute').checked", "controller toggling Mute");
  await pulseGamepad([1]);
  await waitFor("document.getElementById('sound-panel').hidden && !document.getElementById('menu-hub').hidden", "controller B returning from Sound to the hub");
  await pulseGamepad([13]);
  await pulseGamepad([0]);
  await waitFor("!document.getElementById('controls-panel').hidden", "controller A opening Controls");
  await waitFor("document.getElementById('menu-hub').hidden", "controller hiding the hub while Controls is open");
  await waitFor("document.getElementById('controller-select').options.length === 2", "controller selector choices");
  const controllerIndexBefore = await evaluate("document.getElementById('controller-select').value");
  for (let i = 0; i < 6; i += 1) await pulseGamepad([13]);
  await pulseGamepad([15]);
  await waitFor(`document.getElementById('controller-select').value !== '${controllerIndexBefore}'`, "controller cycling the selected device");
  await pulseGamepad([1]);
  await waitFor("document.getElementById('controls-panel').hidden && !document.getElementById('menu-hub').hidden", "controller B returning from Controls to the hub");
  await pulseGamepad([1]);
  await waitFor("document.getElementById('menu-hub').hidden", "controller B closing the menu hub");
  await setFakeGamepad();
  await sleep(100);

  // the sound settings (feature 003): open the panel, move a slider, check that it is stored
  await evaluate("document.getElementById('sound-button').click(); 1");
  await evaluate(`(() => {
    const m = document.getElementById('music-volume');
    m.value = '60';
    m.dispatchEvent(new Event('input'));
    document.getElementById('mute').click();
    return 1;
  })()`);
  await sleep(200);
  await shot("1a-sound.png");
  console.log("audio settings:", await evaluate("localStorage.getItem('eksplode.re.audio.v1')"));
  await evaluate("document.getElementById('mute').click(); document.getElementById('close-sound').click(); 1");

  // keyboard controls (feature 005): edit a binding, reload, reject a conflict, and reset
  await setFakeGamepad([], [0, 0], "");
  await evaluate("document.getElementById('controls-button').click(); 1");
  await waitFor("document.getElementById('controls-panel') && !document.getElementById('controls-panel').hidden", "the controls panel");
  await waitFor("document.getElementById('controller-status').textContent.includes('does not have a supported')", "the unsupported controller fallback");
  await setFakeGamepad([], [0, 0], "standard", 2);
  await waitFor("document.getElementById('controller-select').options.length === 2", "multiple standard controllers");
  await evaluate(`(() => {
    const select = document.getElementById('controller-select');
    select.value = '1';
    select.dispatchEvent(new Event('change'));
    return 1;
  })()`);
  await waitFor("document.getElementById('controller-status').textContent.includes('controller 2')", "the selected second controller");
  await setFakeGamepad();
  await waitFor("document.getElementById('controller-select').options.length === 1 && document.getElementById('controller-status').textContent.includes('connected')", "the detected standard controller");
  console.log("controller panel:", await evaluate("document.getElementById('controller-status').textContent"));
  await evaluate(`document.querySelector('[data-bind-action="up"]').click(); 1`);
  await key("KeyI", "i", 73);
  await waitFor("JSON.parse(localStorage.getItem('eksplode.re.controls.v1')).bindings.up.includes('KeyI')", "the saved movement binding");
  await call("Page.reload");
  await waitFor("document.readyState === 'complete' && !document.getElementById('join').disabled", "the page after reloading controls");
  await setFakeGamepad();
  await evaluate("document.getElementById('controls-button').click(); 1");
  await waitFor("!document.getElementById('controls-panel').hidden && document.querySelector('[data-bind-action=up]').textContent.includes('I')", "the restored movement binding");
  await evaluate(`document.querySelector('[data-bind-action="down"]').click(); 1`);
  await key("KeyI", "i", 73);
  await waitFor("document.getElementById('controls-status').textContent.toLowerCase().includes('already assigned')", "the duplicate-key message");
  await evaluate("document.getElementById('controls-reset').click(); 1");
  await waitFor("document.getElementById('controls-reset').textContent === 'Confirm reset'", "the reset confirmation step");
  await waitFor("JSON.parse(localStorage.getItem('eksplode.re.controls.v1')).bindings.up.includes('KeyI')", "the unchanged binding before reset confirmation");
  await evaluate("document.getElementById('controls-reset').click(); 1");
  await waitFor("JSON.parse(localStorage.getItem('eksplode.re.controls.v1')).bindings.up.includes('ArrowUp')", "the reset keyboard defaults");
  await shot("1c-controls.png");
  await evaluate("document.getElementById('controls-button').click(); 1");

  // pick a look in the character editor (feature 003) and save a screenshot of it
  await evaluate(`(() => {
    document.querySelector('#swatches [data-colour="Teal"]').click();
    for (const [id, value] of [["gender", "Female"], ["head", "Crown"], ["face", "Glasses"]]) {
      const select = document.getElementById(id);
      select.value = value;
      select.dispatchEvent(new Event("change"));
    }
    return 1;
  })()`);
  await sleep(300);
  await shot("1b-editor.png");
  console.log("stored look:", await evaluate("localStorage.getItem('eksplode.re.look.v1')"));

  await evaluate("document.getElementById('name').value = 'SmokeTest'; document.getElementById('join').click(); 1");
  await waitFor("!document.getElementById('lobby-panel').hidden", "the lobby");
  await waitFor("document.activeElement.id === 'lobby-room-name'", "initial lobby focus on the room name");
  const roomNameBeforeOsk = await evaluate("document.getElementById('lobby-room-name').value");
  await setFakeGamepad();
  await pulseGamepad([0]);
  await waitFor("!document.getElementById('osk').hidden", "controller opening the room-name keyboard");
  await pulseGamepad([0]);
  await waitFor("document.getElementById('osk-preview').textContent.includes('q')", "controller entering a character");
  await pulseGamepad([1]);
  await waitFor("document.getElementById('osk').hidden && document.activeElement.id === 'lobby-room-name'", "controller cancelling text entry");
  if (await evaluate("document.getElementById('lobby-room-name').value") !== roomNameBeforeOsk) throw new Error("OSK cancel changed the room name");
  await pulseGamepad([0]);
  await waitFor("!document.getElementById('osk').hidden", "controller reopening the room-name keyboard");
  await pulseGamepad([0]);
  for (let i = 0; i < 4; i += 1) await pulseGamepad([13]);
  for (let i = 0; i < 5; i += 1) await pulseGamepad([15]);
  await waitFor("document.activeElement.id === 'osk-confirm'", "controller focus on OSK confirm");
  await pulseGamepad([0]);
  await waitFor(`document.getElementById('lobby-room-name').value === ${JSON.stringify(`${roomNameBeforeOsk}q`)}`, "controller confirming room-name entry");
  await setFakeGamepad([], [0, 0], "standard", 2);
  await sleep(100);
  for (const [id, direction, inverse] of [["lobby-mode", 14, 15], ["lobby-size", 14, 15], ["lobby-minutes", 14, 15]]) {
    await pulseGamepad([13]);
    await waitFor(`document.activeElement.id === '${id}'`, `controller focus on ${id}`);
    const before = await evaluate(`document.getElementById('${id}').value`);
    await pulseGamepad([direction]);
    await waitFor(`document.getElementById('${id}').value !== ${JSON.stringify(before)}`, `controller adjustment of ${id}`);
    await pulseGamepad([inverse]);
    await waitFor(`document.getElementById('${id}').value === ${JSON.stringify(before)}`, `controller restoring ${id}`);
  }
  await pulseGamepad([13]);
  await waitFor("document.activeElement.id === 'start'", "controller focus leaving Time for the lobby actions");
  for (let i = 0; i < 4; i += 1) await pulseGamepad([15]);
  await waitFor("document.activeElement.id === 'change-look'", "controller focus on Change look");
  await pulseGamepad([0]);
  await waitFor("!document.getElementById('editor').hidden", "controller opening the character editor");
  const controllerColourBefore = await evaluate("document.querySelector('#swatches .selected')?.dataset.colour");
  await pulseGamepad([15]);
  await waitFor(`document.querySelector('#swatches .selected')?.dataset.colour !== ${JSON.stringify(controllerColourBefore)}`, "controller changing the colour choice");
  for (let i = 0; i < 20 && await evaluate("document.activeElement.id !== 'done-look'"); i += 1) await pulseGamepad([13]);
  await waitFor("document.activeElement.id === 'done-look'", "controller focus on Done");
  await pulseGamepad([0]);
  await waitFor("document.getElementById('editor').hidden && document.activeElement.id === 'change-look'", "controller returning from the editor");
  await pulseGamepad([0]);
  await waitFor("!document.getElementById('editor').hidden", "controller reopening the editor");
  await pulseGamepad([1]);
  await waitFor("document.getElementById('editor').hidden && document.activeElement.id === 'change-look'", "controller B returning from the editor");
  await setFakeGamepad();
  await evaluate("document.getElementById('lobby-room-name').focus(); 1");
  for (const [id, delta] of [["lobby-mode", "ArrowDown"], ["lobby-size", "ArrowDown"], ["lobby-minutes", "ArrowDown"]]) {
    await key("Tab", "Tab", 9);
    const before = await evaluate(`document.getElementById('${id}').value`);
    await key(delta, delta, delta === "ArrowDown" ? 40 : 39);
    await waitFor(`document.getElementById('${id}').value !== ${JSON.stringify(before)}`, `keyboard adjustment of ${id}`);
    await key(delta === "ArrowDown" ? "ArrowUp" : "ArrowLeft", delta === "ArrowDown" ? "ArrowUp" : "ArrowLeft", delta === "ArrowDown" ? 38 : 37);
    await waitFor(`document.getElementById('${id}').value === ${JSON.stringify(before)}`, `keyboard restoring ${id}`);
  }
  await key("Tab", "Tab", 9);
  await waitFor("document.activeElement.id === 'start'", "keyboard focus on Start");
  await key("Tab", "Tab", 9);
  await key("Tab", "Tab", 9);
  await key("Tab", "Tab", 9);
  await waitFor("document.activeElement.id === 'ready'", "keyboard focus on Ready");
  await key("Enter", "Enter", 13);
  await key("Tab", "Tab", 9);
  await waitFor("document.activeElement.id === 'change-look'", "keyboard focus on Change look");
  await key("Enter", "Enter", 13);
  await waitFor("!document.getElementById('editor').hidden", "keyboard opening the character editor");
  const colourBefore = await evaluate("document.querySelector('#swatches .selected')?.dataset.colour");
  await key("ArrowRight", "ArrowRight", 39);
  await waitFor(`document.querySelector('#swatches .selected')?.dataset.colour !== ${JSON.stringify(colourBefore)}`, "keyboard changing the colour choice");
  for (let i = 0; i < 20 && await evaluate("document.activeElement.id !== 'done-look'"); i += 1) await key("Tab", "Tab", 9);
  await waitFor("document.activeElement.id === 'done-look'", "keyboard focus on Done");
  await key("Enter", "Enter", 13);
  await waitFor("document.getElementById('editor').hidden && document.activeElement.id === 'change-look'", "keyboard returning from the editor");
  await key("Enter", "Enter", 13);
  await waitFor("!document.getElementById('editor').hidden", "keyboard reopening the editor");
  await key("Escape", "Escape", 27);
  await waitFor("document.getElementById('editor').hidden && document.activeElement.id === 'change-look'", "Escape returning from the editor");
  // a second participant joins the open room, sees the settings and cannot change them
  const wsUrl = `ws://${new URL(url).host}/ws`;
  const peer = new WebSocket(wsUrl);
  const seen = [];
  peer.addEventListener("message", (ev) => seen.push(JSON.parse(ev.data)));
  await new Promise((resolve) => peer.addEventListener("open", resolve));
  const until = async (pred, what) => {
    for (let t = 0; t < 4000; t += 50) {
      const hit = seen.find(pred);
      if (hit) return hit;
      await sleep(50);
    }
    throw new Error(`peer timed out waiting for ${what}; received: ${JSON.stringify(seen)}`);
  };
  peer.send(JSON.stringify({ type: "RoomQuery" }));
  const info = await until((m) => m.type === "RoomInfo", "RoomInfo");
  console.log("peer sees the room:", JSON.stringify(info));
  peer.send(JSON.stringify({ type: "Join", version: 1, name: "Peer" }));
  const welcome = await until((m) => m.type === "Welcome", "Welcome");
  console.log("peer lobby settings:", JSON.stringify(welcome.lobby.settings));
  peer.send(JSON.stringify({ type: "SetRoom", settings: { size: other } }));
  const refused = await until((m) => m.type === "Error" && m.code === "NotOwner", "NotOwner");
  console.log("peer refused:", refused.message);
  await evaluate(`(() => {
    const s = document.getElementById('lobby-size');
    s.value = '${other}';
    s.dispatchEvent(new Event('change'));
    return 1;
  })()`);
  const changed = await until((m) => m.type === "Lobby" && m.lobby.settings.size === other, "the owner's change");
  console.log("peer sees the change:", changed.lobby.settings.size);
  await waitFor(`document.getElementById('room-summary').innerText.includes('${dimensions[other]}')`, "the summary of the new size");
  await evaluate(`(() => {
    const s = document.getElementById('lobby-size');
    s.value = '${size}';
    s.dispatchEvent(new Event('change'));
    return 1;
  })()`);
  peer.close();
  await sleep(300);

  const botsBeforeController = await evaluate("[...document.querySelectorAll('#slots .caption')].filter((item) => item.textContent.toLowerCase().includes('bot')).length");
  await shiftTab();
  await shiftTab();
  await shiftTab();
  await waitFor("document.activeElement.id === 'add-bot'", "keyboard focus on Add bot");
  await key("Enter", "Enter", 13);
  await waitFor(`[...document.querySelectorAll('#slots .caption')].filter((item) => item.textContent.toLowerCase().includes('bot')).length === ${botsBeforeController + 1}`, "Enter activating Add bot without starting");
  if (await evaluate("!document.getElementById('game-panel').hidden")) throw new Error("Enter on Add bot started the match");
  await key("Tab", "Tab", 9);
  await waitFor("document.activeElement.id === 'remove-bot'", "keyboard focus on Remove bot");
  await key("Enter", "Enter", 13);
  await waitFor(`[...document.querySelectorAll('#slots .caption')].filter((item) => item.textContent.toLowerCase().includes('bot')).length === ${botsBeforeController}`, "keyboard removing the smoke bot to preserve room capacity");

  await setFakeGamepad([], [0, 0], "standard", 2);
  await sleep(100);
  await pulseGamepad([15]);
  await pulseGamepad([15]);
  await waitFor("document.activeElement.id === 'change-look'", "controller focus on Change look");
  for (let i = 0; i < 3; i += 1) await pulseGamepad([14]);
  await waitFor("document.activeElement.id === 'add-bot'", "controller focus on Add bot");
  await pulseGamepad([0]);
  await waitFor(`[...document.querySelectorAll('#slots .caption')].filter((item) => item.textContent.toLowerCase().includes('bot')).length === ${botsBeforeController + 1}`, "controller A adding a bot");
  await pulseGamepad([15]);
  await waitFor("document.activeElement.id === 'remove-bot'", "controller focus on Remove bot");
  await pulseGamepad([0]);
  await waitFor(`[...document.querySelectorAll('#slots .caption')].filter((item) => item.textContent.toLowerCase().includes('bot')).length === ${botsBeforeController}`, "controller removing the smoke bot to preserve room capacity");
  await setFakeGamepad();

  // fill the lobby with bots so that four cards show characters
  for (let i = 0; i < 2; i += 1) {
    await evaluate("document.getElementById('add-bot').click(); 1");
    await sleep(200);
  }
  await sleep(500);
  console.log("lobby:", await evaluate("document.getElementById('slots').innerText"));
  console.log("room summary:", await evaluate("document.getElementById('room-summary').innerText"));
  await shot("2-lobby.png");

  await sleep(1300);
  await shot("2b-lobby-later.png");

  // developer mode (feature 003): the panel exists only when the server runs with --dev
  const devVisible = await evaluate("!document.getElementById('dev-panel').hidden");
  console.log("dev panel visible:", devVisible);
  if (devVisible) {
    await key("Escape", "Escape", 27);
    await waitFor("!document.getElementById('menu-hub').hidden && !document.getElementById('hub-dev').hidden", "keyboard opening the developer menu action");
    await key("ArrowDown", "ArrowDown", 40);
    await key("ArrowDown", "ArrowDown", 40);
    await waitFor("document.activeElement.id === 'hub-dev'", "keyboard focus on Developer");
    await key("Enter", "Enter", 13);
    await waitFor("!document.getElementById('dev-panel').hidden && document.activeElement.id === 'dev-pause'", "keyboard opening developer controls");
    const pauseBeforeKeyboard = await evaluate("document.getElementById('dev-pause').textContent");
    await key("Enter", "Enter", 13);
    await waitFor(`document.getElementById('dev-pause').textContent !== ${JSON.stringify(pauseBeforeKeyboard)}`, "keyboard toggling developer pause");
    await key("Tab", "Tab", 9);
    await key("Tab", "Tab", 9);
    await waitFor("document.activeElement.id === 'dev-drop-ExtraBomb'", "keyboard focus on first developer slider");
    const dropBeforeKeyboard = await evaluate("Number(document.getElementById('dev-drop-ExtraBomb').value)");
    await key("ArrowRight", "ArrowRight", 39);
    await waitFor(`Number(document.getElementById('dev-drop-ExtraBomb').value) !== ${dropBeforeKeyboard}`, "keyboard adjusting developer slider");
    await key("Escape", "Escape", 27);
    await waitFor("!document.getElementById('menu-hub').hidden && document.activeElement.id === 'hub-dev'", "keyboard returning to hub from developer controls");
    await key("Escape", "Escape", 27);
    await waitFor("document.getElementById('menu-hub').hidden", "keyboard closing developer hub");

    await setFakeGamepad([], [0, 0], "standard", 1);
    await sleep(100);
    await pulseGamepad([9]);
    await waitFor("!document.getElementById('menu-hub').hidden", "controller opening developer hub");
    await pulseGamepad([13]);
    await pulseGamepad([13]);
    await waitFor("document.activeElement.id === 'hub-dev'", "controller focus on Developer");
    await pulseGamepad([0]);
    await waitFor("document.activeElement.id === 'dev-pause'", "controller opening developer controls");
    const pauseBeforeController = await evaluate("document.getElementById('dev-pause').textContent");
    await pulseGamepad([0]);
    await waitFor(`document.getElementById('dev-pause').textContent !== ${JSON.stringify(pauseBeforeController)}`, "controller toggling developer pause");
    for (let i = 0; i < 2; i += 1) await pulseGamepad([13]);
    await waitFor("document.activeElement.id === 'dev-drop-ExtraBomb'", "controller focus on first developer slider");
    const dropBeforeController = await evaluate("Number(document.getElementById('dev-drop-ExtraBomb').value)");
    await pulseGamepad([15]);
    await waitFor(`Number(document.getElementById('dev-drop-ExtraBomb').value) !== ${dropBeforeController}`, "controller adjusting developer slider");
    await pulseGamepad([1]);
    await waitFor("!document.getElementById('menu-hub').hidden && !document.getElementById('dev-panel').classList.contains('navigation-layer')", "controller returning from developer controls to the hub");
    await pulseGamepad([1]);
    await waitFor("document.getElementById('menu-hub').hidden", "controller closing the developer hub");
    await setFakeGamepad();

    await evaluate(`(() => {
      ["ExtraBomb", "Range", "Speed", "Punch", "Slow", "HyperSpeed", "SmallBomb"].forEach((k) => {
        const r = document.getElementById("dev-drop-" + k);
        r.value = k === "Punch" ? "100" : "0";
        r.dispatchEvent(new Event("input"));
        r.dispatchEvent(new Event("change"));
      });
      document.getElementById("dev-pause").click();
      return 1;
    })()`);
    await sleep(500);
    console.log("dev pause button:", await evaluate("document.getElementById('dev-pause').textContent"));
    console.log("dev sliders:", await evaluate("[...document.querySelectorAll('#dev-sliders input')].map((i) => i.value).join(',')"));
    await shot("2c-dev-panel.png");
  }

  await evaluate("document.getElementById('start').click(); 1");
  await waitFor("!document.getElementById('game-panel').hidden", "the game");
  await evaluate("window.dispatchEvent(new Event('blur')); 1");
  await setFakeGamepad();
  await sleep(100);
  const bombInputsWhileBlurred = await evaluate("window.__smokeInputs.filter((input) => input.bomb).length");
  await setFakeGamepad([0]);
  await sleep(150);
  await waitFor(`window.__smokeInputs.filter((input) => input.bomb).length === ${bombInputsWhileBlurred}`, "no controller action while the page is blurred");
  await setFakeGamepad();
  await evaluate("window.dispatchEvent(new Event('focus')); 1");
  await sleep(100);
  await setFakeGamepad([0]);
  await waitFor("document.getElementById('hud').innerText.includes('You: bombs 1/1')", "the Xbox A bomb action");
  console.log("controller A action:", await evaluate("document.getElementById('hud').innerText.split('\\n').find((line) => line.startsWith('You:'))"));
  await setFakeGamepad();
  await sleep(1500);
  await shot("3-game.png");

  for (const [code, name, vk] of [
    ["ArrowDown", "ArrowDown", 40],
    ["ArrowRight", "ArrowRight", 39],
    ["Space", " ", 32],
  ]) {
    await key(code, name, vk);
  }
  await sleep(800);
  await shot("4-after-keys.png");
  console.log("hud:", await evaluate("document.getElementById('hud').innerText"));

  // end pattern (feature 004): CLOSING=1 plays a 2-minute round and waits for the last minute
  if (process.env.CLOSING) {
    // walk toward the middle, which is covered last
    for (let i = 0; i < 4; i += 1) {
      await key("ArrowRight", "ArrowRight", 39);
      await key("ArrowDown", "ArrowDown", 40);
    }
    await waitFor("document.getElementById('timer').innerText === 'CLOSING'", "the end pattern", 90000);
    await sleep(600);
    console.log("banner:", await evaluate("document.getElementById('banner').hidden + ' ' + document.getElementById('banner').innerText"));
    await shot("5-closing-start.png");
    await sleep(8000);
    await shot("6-closing-later.png");
    console.log("timer:", await evaluate("document.getElementById('timer').innerText"));
  }
  console.log("notice:", await evaluate("document.getElementById('notice').textContent"));
  console.log("canvas:", await evaluate("document.getElementById('view').width + 'x' + document.getElementById('view').height"));
  console.log("console:", consoleLines.length ? consoleLines.join(" | ") : "(clean)");
  ws.close();
}

try {
  await main();
} catch (e) {
  console.error("FAILED:", e.message);
  process.exitCode = 1;
} finally {
  child.kill();
  await sleep(300);
  rmSync(profile, { recursive: true, force: true });
}
