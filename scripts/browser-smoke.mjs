// Drives a real headless browser through the Chrome DevTools Protocol: joins the server's page,
// starts a match, plays a few keys and saves screenshots. Optional manual-style check, not part of
// `cargo test`. Usage:
//   node scripts/browser-smoke.mjs http://0.0.0.0:8080/ [out-dir]
// MODE=normal|team picks the mode of the room (default normal).
// SIZE=small|medium|large picks the board of the room (default medium).
// CLOSING=1 plays a 2-minute round and saves screenshots of the end pattern (takes about 80 s).
// Needs a Chromium based browser (BROWSER=/path/to/browser, default chromium-browser).
import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";

const sceneMode = process.argv.includes("--lobby-scene");
const adaptiveMode = process.argv.includes("--adaptive-controls");
const isolatedMode = sceneMode || adaptiveMode;
const url = isolatedMode ? process.env.BASE_URL ?? "http://127.0.0.1:8080/" : process.argv[2] ?? "http://0.0.0.0:8080/";
const out = isolatedMode ? process.env.OUT_DIR ?? "browser-smoke" : process.argv[3] ?? "browser-smoke";
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
      const list = await (await fetch(`http://0.0.0.0:${port}/json`)).json();
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
  const sentInputs = [];
  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) pending.get(msg.id)(msg);
    if (msg.method === "Network.webSocketFrameSent") {
      try {
        const payload = JSON.parse(msg.params.response.payloadData);
        if (payload.type === "Input") sentInputs.push(payload);
      } catch {}
    }
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
  const waitForSentInput = async (after, predicate, what, ms = 8000) => {
    for (let t = 0; t < ms; t += 100) {
      if (sentInputs.slice(after).some(predicate)) return;
      await sleep(100);
    }
    throw new Error(`timed out waiting for ${what}; recent input: ${JSON.stringify(sentInputs.slice(-5))}`);
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
  const setViewport = async (width, height, mobile) => {
    await call("Emulation.setDeviceMetricsOverride", {
      width,
      height,
      deviceScaleFactor: 1,
      mobile,
      screenOrientation: {
        type: width > height ? "landscapePrimary" : "portraitPrimary",
        angle: width > height ? 90 : 0,
      },
    });
    await call("Emulation.setTouchEmulationEnabled", { enabled: mobile, maxTouchPoints: 5 });
  };
  const touchElement = async (selector) => {
    const point = await evaluate(`(() => {
      const element = document.querySelector(${JSON.stringify(selector)});
      if (!element || element.hidden || element.disabled) return null;
      const rect = element.getBoundingClientRect();
      const x = rect.x + rect.width / 2;
      const y = rect.y + rect.height / 2;
      const hit = document.elementFromPoint(x, y);
      return {
        x,
        y,
        hitId: hit?.id,
        hitMatches: hit === element || element.contains(hit),
        viewportWidth: innerWidth,
        viewportHeight: innerHeight,
      };
    })()`);
    if (!point) throw new Error(`cannot touch ${selector}`);
    if (!point.hitId) {
      throw new Error(`${selector} is outside the visible ${point.viewportWidth}x${point.viewportHeight} viewport`);
    }
    if (!point.hitMatches) throw new Error(`${selector} is covered by #${point.hitId}`);
    const pressed = await call("Input.dispatchMouseEvent", {
      type: "mousePressed",
      x: point.x,
      y: point.y,
      button: "left",
      clickCount: 1,
      pointerType: "touch",
    });
    if (pressed.error) throw new Error(`touch press failed: ${pressed.error.message}`);
    await sleep(80);
    const released = await call("Input.dispatchMouseEvent", {
      type: "mouseReleased",
      x: point.x,
      y: point.y,
      button: "left",
      clickCount: 1,
      pointerType: "touch",
    });
    if (released.error) throw new Error(`touch release failed: ${released.error.message}`);
    await sleep(100);
  };
  const activeTouches = new Map();
  const dispatchTouch = async (type, pointerId, x = 0, y = 0) => {
    if (type === "touchEnd" || type === "touchCancel") activeTouches.delete(pointerId);
    else activeTouches.set(pointerId, { x, y });
    const result = await call("Input.dispatchTouchEvent", {
      type,
      touchPoints: [...activeTouches].map(([id, point]) => ({ ...point, id, radiusX: 4, radiusY: 4, force: 1 })),
    });
    if (result.error) throw new Error(`touch dispatch failed: ${result.error.message}`);
    await sleep(70);
  };
  const elementCenter = async (selector) => evaluate(`(() => {
    const rect = document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect();
    return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
  })()`);
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
      const NativeWebSocket = window.WebSocket;
      window.WebSocket = new Proxy(NativeWebSocket, {
        construct(target, args, newTarget) {
          const socket = Reflect.construct(target, args, newTarget);
          if (String(args[0]).endsWith("/ws")) {
            window.__smokeGameSocket = socket;
            socket.addEventListener("message", (event) => {
              try {
                const received = JSON.parse(event.data);
                if (received.type === "Welcome" || received.type === "Lobby") window.__smokeLobby = received.lobby;
              } catch {}
            });
          }
          return socket;
        },
      });
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
  await call("Network.enable");
  if (sceneMode) {
    await call("Page.enable");
    const seed = Number(process.env.SCENE_SEED ?? 4);
    if (!Number.isInteger(seed) || seed < 0 || seed > 0xffffffff) throw new Error("Invalid SCENE_SEED");
    await call("Page.addScriptToEvaluateOnNewDocument", { source: `(() => {
      const original = Crypto.prototype.getRandomValues;
      Crypto.prototype.getRandomValues = function (array) {
        if (array instanceof Uint32Array && array.length === 1) { array[0] = ${seed}; return array; }
        return original.call(this, array);
      };
      const instantiate=WebAssembly.instantiate;
      WebAssembly.instantiate=async function (...args) {
        const result=await instantiate.apply(this,args);
        if (!result.instance || window.__sceneRuntime) return result;
        const originalExports=result.instance.exports;
        if (!originalExports.eksplode_re_lobby_reset) return result;
        const { LobbySceneController } = await import('/app.js');
        const setRenderer = LobbySceneController.prototype.setRenderer;
        LobbySceneController.prototype.setRenderer = function (renderer) {
          window.__sceneController = this;
          return setRenderer.call(this, renderer);
        };
        const trace=window.__sceneRuntime={ticks:0,resets:0,frames:0};
        const exports={...originalExports};
        exports.eksplode_re_lobby_reset=(seed)=>{trace.resets+=1;return originalExports.eksplode_re_lobby_reset(seed);};
        exports.eksplode_re_lobby_advance=(ticks)=>{const code=originalExports.eksplode_re_lobby_advance(ticks);if(code===0)trace.ticks+=ticks;return code;};
        exports.eksplode_re_lobby_frame=(...args)=>{trace.frames+=1;return originalExports.eksplode_re_lobby_frame(...args);};
        return {module:result.module,instance:{exports}};
      };
    })()` });
  }
  await call("Page.navigate", { url });
  if (adaptiveMode) {
    await waitFor("document.readyState === 'complete' && document.getElementById('main-menu') && !document.getElementById('room-create').disabled", "ready main menu");
    await setViewport(1440,900,false);
    if (!await evaluate("document.getElementById('donate')?.href === 'https://buymeacoffee.com/kaelinnchacuit' && document.getElementById('donate').rel === 'noopener noreferrer'")) throw new Error("Donate destination/security attributes are incorrect");
    await setFakeGamepad([], [0,0], "standard", 2);
    await pulseGamepad([]);
    await evaluate("window.__donateActivated=false;document.getElementById('donate').addEventListener('click',event=>{event.preventDefault();window.__donateActivated=true;});document.getElementById('donate').focus()");
    await pulseGamepad([0]);
    await waitFor("window.__donateActivated", "controller activates Donate link");
    await shot("adaptive-main-menu.png");
    await setFakeGamepad([], [0,0], "standard", 0);
    await evaluate("document.getElementById('menu-options').click()");
    await waitFor("!document.getElementById('tab-controls').hidden", "desktop Controls tab");
    await evaluate("document.getElementById('tab-controls').click()");
    await waitFor("!document.getElementById('keyboard-controls').hidden && document.getElementById('controller-controls').hidden", "keyboard-only controls");
    const bindings = await evaluate("document.querySelector('[data-bind-action=up]').textContent");
    await evaluate("document.querySelector('[data-bind-action=up]').click()");
    await shot("adaptive-keyboard.png");
    await setFakeGamepad([], [0,0], "standard", 2);
    await waitFor("document.getElementById('keyboard-controls').hidden && !document.getElementById('controller-controls').hidden && document.getElementById('app').classList.contains('controller-input')", "controller-only settings on plug");
    if (!await evaluate("[...document.querySelectorAll('[data-bind-action]')].every(button=>!button.hasAttribute('aria-pressed'))")) throw new Error("Hidden keyboard capture was not cancelled");
    await evaluate("document.getElementById('controller-select').focus()");
    const selected = await evaluate("document.getElementById('controller-select').value");
    await pulseGamepad([15]);
    await waitFor(`document.getElementById('controller-select').value !== ${JSON.stringify(selected)}`, "right changes controller selection");
    await pulseGamepad([14]);
    await waitFor(`document.getElementById('controller-select').value === ${JSON.stringify(selected)}`, "left restores controller selection");
    const arrows = await evaluate("getComputedStyle(document.getElementById('controller-select')).appearance === 'none' && getComputedStyle(document.getElementById('controller-select').parentElement,'::before').content !== 'none'");
    if (!arrows) throw new Error("Controller select lacks left/right affordances");
    await shot("adaptive-controller.png");
    await setFakeGamepad([], [0,0], "standard", 0);
    await waitFor("!document.getElementById('keyboard-controls').hidden && document.getElementById('controller-controls').hidden", "keyboard settings restored on unplug");
    if (await evaluate("document.querySelector('[data-bind-action=up]').textContent") !== bindings) throw new Error("Plug/unplug changed keyboard bindings");
    await setViewport(390,844,true);
    await waitFor("document.getElementById('tab-controls').hidden && document.getElementById('controls-panel').hidden && !document.getElementById('sound-panel').hidden", "mobile has no Controls menu");
    await setFakeGamepad([], [0,0], "standard", 2);
    await waitFor("document.getElementById('tab-controls').hidden && !document.getElementById('app').classList.contains('controller-input')", "mobile priority with gamepad");
    await shot("adaptive-mobile.png");
    await setViewport(1440,900,false);
    await waitFor("!document.getElementById('tab-controls').hidden", "Controls returns on desktop");
    await evaluate("document.getElementById('tab-controls').click()");
    await waitFor("!document.getElementById('controller-controls').hidden", "desktop controller settings restored");
    await setFakeGamepad([], [0,0], "standard", 0);
    await evaluate(`Object.defineProperty(Navigator.prototype,'getGamepads',{configurable:true,value:()=>[{index:0,id:'Polling controller',mapping:'standard',connected:true,buttons:[],axes:[0,0]}]})`);
    await waitFor("document.getElementById('keyboard-controls').hidden", "polling discovers a controller");
    if (consoleLines.some(line=>line.startsWith("EXCEPTION"))) throw new Error(consoleLines.join(" | "));
    console.log("adaptive controls: platform menus, hot-plug, horizontal arrows, mobile priority and bindings preservation passed");
    ws.close();
    return;
  }
  if (sceneMode) {
    await waitFor("document.getElementById('main-menu') && !document.getElementById('main-menu').hidden", "main menu");
    const started = Date.now();
    await waitFor(`(() => {
      const canvas = document.getElementById('lobby-scene-canvas');
      return canvas && !canvas.hidden && canvas.width > 0 && canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data.some(value => value > 0);
    })()`, "f14_us1_as1_ animated main-menu scene", 1000);
    const pixels = `(() => {
      const canvas = document.getElementById('lobby-scene-canvas');
      return canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data.reduce((hash, value, index) => (hash + value * (index % 997)) >>> 0, 0);
    })()`;
    const first = await evaluate(pixels);
    await waitFor(`(${pixels}) !== ${first}`, "moving scene pixels", 1000);
    console.log("f14_us1_as1_ moving scene startup:", Date.now() - started, "ms");
    await shot("f14-main-menu.png");
    if (process.env.GENERATE_FALLBACK) {
      const image = await evaluate(`(async () => {
        const { createRenderer, paintCommands } = await import('/app.js');
        const { instance } = await WebAssembly.instantiate(await (await fetch('/eksplode_re_render.wasm')).arrayBuffer(), {});
        const renderer = createRenderer(instance.exports);
        renderer.resetLobby(4);
        for (let index=0; index<8; index+=1) renderer.advanceLobby(15);
        const canvas=document.createElement('canvas'); canvas.width=640; canvas.height=200;
        paintCommands(canvas.getContext('2d'), renderer.lobbyFrame(640,200,0));
        return canvas.toDataURL('image/png');
      })()`);
      if (!image?.startsWith("data:image/png;base64,")) throw new Error("Fallback PNG generation failed");
      writeFileSync(process.env.GENERATE_FALLBACK, Buffer.from(image.split(",")[1], "base64"));
      console.log("generated first-party scene PNG:", process.env.GENERATE_FALLBACK);
      ws.close();
      return;
    }
    if (process.env.SCENE_SET) {
      await waitFor("window.__sceneController && !window.__sceneController.failed", "fixture controller");
      await evaluate("window.__sceneController.togglePause()");
      await evaluate(`(async () => {
        const { createRenderer, paintCommands } = await import('/app.js');
        const { instance } = await WebAssembly.instantiate(await (await fetch('/eksplode_re_render.wasm')).arrayBuffer(), {});
        window.__sceneFixture = { renderer: createRenderer(instance.exports), paintCommands };
      })()`);
      const fixtures = [
        [0,"run-drop",120], [1,"background",90], [2,"same-level",90],
        [3,"kart-right",120], [4,"elimination",200], [5,"kart-left",120], [6,"flatten",185],
      ].filter(([stage]) => process.env.SCENE_SET !== "encounters" || [1,2,4,6].includes(stage));
      for (const [width,height] of [[1440,900],[390,844],[320,568],[844,390]]) {
        await call("Emulation.setDeviceMetricsOverride", { width,height,deviceScaleFactor:1,mobile:width<500 });
        await evaluate("new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))");
        if (!await evaluate("document.documentElement.scrollWidth <= innerWidth && document.getElementById('lobby-scene').getBoundingClientRect().bottom <= document.getElementById('name').getBoundingClientRect().top")) {
          throw new Error(`Fixture layout overlaps or overflows ${width}x${height}`);
        }
        for (const [stage,label,tick] of fixtures) {
          const count = await evaluate(`(() => {
            const {renderer,paintCommands}=window.__sceneFixture;
            renderer.resetLobby(4);
            let ticks=${stage * 300 + tick};
            while(ticks>0){const delta=Math.min(ticks,15);renderer.advanceLobby(delta);ticks-=delta;}
            const canvas=document.getElementById('lobby-scene-canvas');
            const commands=renderer.lobbyFrame(canvas.width,canvas.height,0);
            canvas.getContext('2d').clearRect(0,0,canvas.width,canvas.height);
            paintCommands(canvas.getContext('2d'),commands);
            return commands.count;
          })()`);
          if (count <= 80) throw new Error(`Missing visible ${label} fixture: only ${count} commands`);
          await shot(`f14-${label}-${width}.png`);
        }
      }
      const liveSeconds=Number(process.env.SCENE_LIVE_SECONDS ?? 0);
      if (process.env.SCENE_NAV_ONLY) {
        await evaluate("window.__sceneController.togglePause()");
        for (const [open,panel,close] of [["menu-create","room-panel","room-back"],["menu-join","directory-panel","directory-back"],["menu-customize","customize-panel","customize-close"],["menu-options","options-panel","options-close"]]) {
          await evaluate(`document.getElementById('${open}').click()`);
          await waitFor(`!document.getElementById('${panel}').hidden`,"navigation regression destination");
          const frozen=await evaluate("window.__sceneRuntime.ticks");
          await evaluate("new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))");
          if(await evaluate("window.__sceneRuntime.ticks")!==frozen)throw new Error(`Scene advanced behind ${panel}`);
          await evaluate(`document.getElementById('${close}').click()`);
        }
        const hidden = await call("Target.createTarget", { url:"about:blank" });
        await call("Target.activateTarget", { targetId:hidden.result.targetId });
        const pageHidden=await evaluate("document.hidden");
        if (pageHidden) {
          const frozen=await evaluate("window.__sceneRuntime.ticks");
          const other=await (await fetch(`http://127.0.0.1:${port}/json`)).json();
          const original=other.find(target=>target.webSocketDebuggerUrl===ws.url)?.id;
          await call("Target.activateTarget", { targetId:original });
          if(await evaluate("window.__sceneRuntime.ticks")-frozen>15)throw new Error("Hidden tab caught up accumulated ticks");
        }
        await call("Target.closeTarget", { targetId:hidden.result.targetId });
        await call("Page.bringToFront");
        await evaluate("document.getElementById('menu-create').click()");
        await waitFor("!document.getElementById('room-create').disabled", "room creation enabled");
        await evaluate("document.getElementById('room-name').value='Scene validation';document.getElementById('room-create').click()");
        await waitFor("!document.getElementById('lobby-panel').hidden && document.querySelectorAll('#slots .portrait').length===4", "unchanged bot-filled room lobby");
        const frozen=await evaluate("window.__sceneRuntime.ticks");
        await evaluate("new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))");
        if(await evaluate("window.__sceneRuntime.ticks")!==frozen)throw new Error("Scene advanced inside the room lobby");
        await shot("f14-unchanged-room-lobby.png");
        console.log("f14_us2_as6_ all navigation destinations freeze scene ticks");
      }
      if (liveSeconds) {
        if (!Number.isInteger(liveSeconds) || liveSeconds<90 || liveSeconds>600) throw new Error("SCENE_LIVE_SECONDS must be an integer from 90 to 600");
        await call("Emulation.setDeviceMetricsOverride", {width:1440,height:900,deviceScaleFactor:1,mobile:false});
        await evaluate("window.__sceneController.togglePause()");
        const startTicks=await evaluate("window.__sceneRuntime.ticks");
        for(let elapsed=0;elapsed<liveSeconds;elapsed+=5){
          const duration=Math.min(5,liveSeconds-elapsed)*1000;
          await evaluate(`new Promise(resolve=>{const end=performance.now()+${duration};const frame=()=>performance.now()>=end?resolve():requestAnimationFrame(frame);requestAnimationFrame(frame);})`);
          if(elapsed%30===0 && liveSeconds>=300){
            const choices=[["menu-create","room-panel","room-back"],["menu-join","directory-panel","directory-back"],["menu-customize","customize-panel","customize-close"],["menu-options","options-panel","options-close"]];
            const [open,panel,close]=choices[Math.floor(elapsed/30)%choices.length];
            await evaluate(`document.getElementById('${open}').click()`);
            await waitFor(`!document.getElementById('${panel}').hidden`,"live menu action");
            const frozen=await evaluate("window.__sceneRuntime.ticks");
            await evaluate("new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))");
            if(await evaluate("window.__sceneRuntime.ticks")!==frozen)throw new Error("off-menu scene advanced");
            await evaluate(`document.getElementById('${close}').click()`);
          }
          if(elapsed%15===0)console.log("live main-menu playback:",Math.min(elapsed+5,liveSeconds),"seconds");
        }
        const trace=await evaluate("window.__sceneRuntime");
        if(trace.resets!==1 || trace.ticks-startTicks<liveSeconds*60*0.9)throw new Error(`Live scene stalled or reset: ${JSON.stringify(trace)}`);
        if(sentInputs.length)throw new Error("Decorative menu sent gameplay input");
        console.log("live scene evidence:",JSON.stringify(trace));
        await shot("f14-live-complete.png");
      }
      console.log("actual WASM fixture pixels passed:", process.env.SCENE_SET);
      ws.close();
      return;
    }
    if (await evaluate("document.getElementById('lobby-scene-pause') !== null")) throw new Error("f14_us2_as3_ scene icon must be absent");
    await waitFor("window.__sceneController", "fixture controller");
    await evaluate("window.__sceneController.togglePause()");
    const frozen = await evaluate(pixels);
    await evaluate("new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))");
    if (await evaluate(pixels) !== frozen) throw new Error("f14_us2_as3_ paused pixels moved");
    await call("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
    await waitFor("document.getElementById('lobby-scene-canvas').hidden && !document.getElementById('lobby-scene-fallback').hidden", "f14_us2_as2_ reduced motion still image");
    await call("Emulation.setEmulatedMedia", { features: [] });
    await waitFor("window.__sceneController.userPaused && !window.__sceneController.conditions.reducedMotion", "retained test-fixture pause");
    for (const [width, height] of [[1440,900],[390,844],[320,568],[844,390]]) {
      await call("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: width < 500 });
      const fits = await evaluate(`(() => {
        const scene=document.getElementById('lobby-scene').getBoundingClientRect();
        const name=document.getElementById('name').getBoundingClientRect();
        return scene.width>0 && scene.bottom<=name.top && document.documentElement.scrollWidth<=innerWidth;
      })()`);
      if (!fits) throw new Error(`f14_us2_as4_ scene overlaps or overflows ${width}x${height}`);
      await shot(`f14-${width}x${height}.png`);
      await call("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
      await waitFor("document.getElementById('lobby-scene-fallback').complete && document.getElementById('lobby-scene-fallback').naturalWidth > 0 && document.getElementById('lobby-scene-canvas').hidden", "loaded motion-safe PNG");
      await shot(`f14-still-${width}x${height}.png`);
      await call("Emulation.setEmulatedMedia", { features: [] });
    }
    console.log("f14_us2_as1_ menu actions and decorative focus isolation:", await evaluate("document.querySelectorAll('#main-menu .menu-actions button').length"));
    for (const [open, panel, close] of [
      ["menu-create", "room-panel", "room-back"],
      ["menu-join", "directory-panel", "directory-back"],
      ["menu-customize", "customize-panel", "customize-close"],
      ["menu-options", "options-panel", "options-close"],
    ]) {
      await evaluate(`document.getElementById('${open}').click()`);
      await waitFor(`!document.getElementById('${panel}').hidden`, `f14_us2_as1_ ${panel}`);
      await evaluate(`document.getElementById('${close}').click()`);
      await waitFor("!document.getElementById('main-menu').hidden && window.__sceneController.userPaused", "f14_us2_as6_ preserved fixture after navigation");
    }
    await call("Network.setCacheDisabled", { cacheDisabled: true });
    for (const failure of ["old-module", "scene-error", "canvas", "wasm"]) {
      let identifier = null;
      if (failure === "wasm") await call("Network.setBlockedURLs", { urls: ["*eksplode_re_render.wasm*"] });
      else {
        const source = failure === "canvas" ? `HTMLCanvasElement.prototype.getContext = function () { return null; };` : `(() => {
          const instantiate=WebAssembly.instantiate;
          WebAssembly.instantiate=async function (...args) {
            const result=await instantiate.apply(this,args);
            if (!result.instance) return result;
            const exports={...result.instance.exports};
            if (${JSON.stringify(failure)} === 'old-module') {
              for (const name of ['reset','advance','frame']) delete exports['eksplode_re_lobby_'+name];
            } else exports.eksplode_re_lobby_frame=()=>0;
            return {module:result.module,instance:{exports}};
          };
        })();`;
        identifier = (await call("Page.addScriptToEvaluateOnNewDocument", { source })).result.identifier;
      }
      await call("Page.navigate", { url });
      await waitFor("document.getElementById('lobby-scene-canvas')?.hidden && document.getElementById('lobby-scene-fallback').naturalWidth > 0 && document.getElementById('retry').hidden && !document.getElementById('room-create').disabled", `f14_us2_as5_ ${failure} fallback`);
      await evaluate("document.getElementById('menu-create').click()");
      await waitFor("!document.getElementById('room-panel').hidden", `${failure} preserves Create`);
      await evaluate("document.getElementById('room-back').click()");
      await evaluate("document.getElementById('menu-customize').click()");
      await waitFor("!document.getElementById('customize-panel').hidden", `${failure} preserves Customize`);
      await evaluate("document.getElementById('customize-close').click()");
      await shot(`f14-fallback-${failure}.png`);
      if (identifier) await call("Page.removeScriptToEvaluateOnNewDocument", { identifier });
      await call("Network.setBlockedURLs", { urls: [] });
    }
    console.log("f14_us2_as2_as3_as4_as5_as6_ motion, layout, navigation and failure checks passed");
    if (consoleLines.some(line => line.startsWith("EXCEPTION"))) throw new Error(consoleLines.join(" | "));
    ws.close();
    return;
  }
  await waitFor("document.readyState === 'complete' && !document.getElementById('join').disabled", "the page");

  if (process.env.MOBILE_FLOW === "setup") {
    await waitFor("!document.getElementById('room-panel').hidden", "fresh room creation screen");
    await setViewport(390, 844, true);
    await waitFor("matchMedia('(orientation: portrait)').matches", "portrait phone viewport");
    if (await evaluate("document.documentElement.scrollWidth > window.innerWidth")) {
      throw new Error("room setup overflows the portrait phone viewport");
    }
    await setViewport(844, 390, true);
    await waitFor("matchMedia('(orientation: landscape)').matches", "landscape phone viewport");
    if (await evaluate("document.documentElement.scrollWidth > window.innerWidth")) {
      throw new Error("room setup overflows the landscape phone viewport");
    }
    await touchElement("#room-create");
    await waitFor("!document.getElementById('join-panel').hidden", "touch opening the join screen");
    await touchElement("#name");
    await waitFor("document.activeElement.id === 'name'", "touch focusing the player name");
    await evaluate(`(() => {
      const name = document.getElementById('name');
      name.value = 'MobileSmoke';
      name.dispatchEvent(new Event('input', { bubbles: true }));
      return 1;
    })()`);
    await touchElement("#join");
    await waitFor("!document.getElementById('lobby-panel').hidden", "touch joining the lobby");
    if (await evaluate("document.documentElement.scrollWidth > window.innerWidth")) {
      throw new Error("lobby overflows the landscape phone viewport");
    }
    await setViewport(1100, 800, false);
    await call("Page.bringToFront");
    await evaluate("document.getElementById('start').focus(); 1");
    await waitFor("document.activeElement.id === 'start'", "desktop keyboard focus on Start");
    await key("Enter", "Enter", 13);
    await waitFor("!document.getElementById('game-panel').hidden", "desktop keyboard starting the match after mobile flow");
    console.log("mobile setup and lobby touch controls, plus desktop keyboard regression, passed");
    return;
  }

  if (process.env.MOBILE_FLOW === "controls") {
    await waitFor("!document.getElementById('room-panel').hidden", "fresh room creation screen");
    await evaluate(`(() => {
      const nativeMatchMedia = window.matchMedia.bind(window);
      window.matchMedia = (query) => {
        const result = nativeMatchMedia(query);
        if (query === "(pointer: coarse)" || query === "(any-pointer: coarse)") {
          return new Proxy(result, {
            get(target, property) {
              return property === "matches" ? false : Reflect.get(target, property, target);
            },
          });
        }
        return result;
      };
      Object.defineProperty(navigator, "maxTouchPoints", { configurable: true, value: 5 });
      return 1;
    })()`);
    await evaluate(`(() => {
      const NativeWebSocket = window.WebSocket;
      window.__smokeLobby = null;
      window.WebSocket = new Proxy(NativeWebSocket, {
        construct(target, args, newTarget) {
          const socket = Reflect.construct(target, args, newTarget);
          if (String(args[0]).endsWith("/ws")) {
            window.__smokeGameSocket = socket;
            socket.addEventListener("message", (event) => {
              try {
                const received = JSON.parse(event.data);
                if (received.type === "Welcome" || received.type === "Lobby") window.__smokeLobby = received.lobby;
              } catch {}
            });
          }
          return socket;
        },
      });
      return 1;
    })()`);
    await setViewport(844, 390, true);
    await touchElement("#room-create");
    await waitFor("!document.getElementById('join-panel').hidden", "touch opening the join screen");
    await evaluate(`(() => {
      const name = document.getElementById('name');
      name.value = 'TouchSmoke';
      name.dispatchEvent(new Event('input', { bubbles: true }));
      return 1;
    })()`);
    await touchElement("#join");
    await waitFor("!document.getElementById('lobby-panel').hidden", "touch joining the lobby");
    await touchElement("#start");
    await waitFor("!document.getElementById('game-panel').hidden", "touch starting the match");
    await waitFor("!document.getElementById('touch-controls').hidden", "mobile match controls");
    if (await evaluate("!document.getElementById('touch-punch').hidden")) {
      throw new Error("Punch was visible before the player acquired the power");
    }

    await setViewport(390, 844, true);
    await waitFor("!document.getElementById('touch-controls').hidden", "portrait gameplay controls");
    const portraitLayout = await evaluate(`(() => {
      const stick = document.getElementById('touch-joystick').getBoundingClientRect();
      const bomb = document.getElementById('touch-bomb').getBoundingClientRect();
      return stick.left < innerWidth / 2 && stick.top > innerHeight / 2 &&
        bomb.left > innerWidth / 2 && bomb.top > innerHeight / 2;
    })()`);
    if (!portraitLayout) throw new Error("portrait controls are not positioned in opposite bottom corners");
    const portraitStick = await elementCenter("#touch-joystick");
    const beforePortraitMove = sentInputs.length;
    await dispatchTouch("touchStart", 5, portraitStick.x, portraitStick.y);
    await dispatchTouch("touchMove", 5, portraitStick.x + 70, portraitStick.y);
    await waitForSentInput(beforePortraitMove, (input) => input.mv === "Right", "portrait joystick movement");
    await dispatchTouch("touchCancel", 5);
    const beforePortraitNeutral = sentInputs.length;
    await waitForSentInput(beforePortraitNeutral, (input) => !input.mv && !input.bomb && !input.punch, "portrait neutral input after release");

    await setViewport(844, 390, true);
    await waitFor("!document.getElementById('touch-controls').hidden", "landscape gameplay controls");
    const stick = await elementCenter("#touch-joystick");
    await dispatchTouch("touchStart", 1, stick.x, stick.y);
    await dispatchTouch("touchMove", 1, stick.x + 70, stick.y);
    await waitForSentInput(0, (input) => input.mv === "Right", "rightward joystick movement");

    const bomb = await elementCenter("#touch-bomb");
    const beforeBomb = sentInputs.length;
    await dispatchTouch("touchStart", 2, bomb.x, bomb.y);
    await waitForSentInput(beforeBomb, (input) => input.mv === "Right" && input.bomb, "bomb while moving");
    await dispatchTouch("touchEnd", 2);
    if (await evaluate("!document.getElementById('touch-punch').hidden")) {
      throw new Error("Punch was visible without the Punch power");
    }

    await dispatchTouch("touchCancel", 1);
    const beforeNeutral = sentInputs.length;
    await waitForSentInput(beforeNeutral, (input) => !input.mv && !input.bomb && !input.punch, "neutral input after pointer cancellation");
    const resultChecks = await evaluate(`(() => {
      const socket = window.__smokeGameSocket;
      const lobby = window.__smokeLobby;
      if (!socket || !lobby) return "missing-game-socket-or-lobby";
      const deliver = (message) => socket.dispatchEvent(new MessageEvent("message", { data: JSON.stringify(message) }));
      deliver({ type: "Ended", outcome: { Winner: 0 } });
      const shown = !document.getElementById("results-panel").hidden &&
        document.getElementById("results-title").textContent.includes("You won") &&
        document.querySelectorAll("#results-rows tr").length > 0;
      deliver({ type: "Lobby", lobby });
      const persisted = !document.getElementById("results-panel").hidden;
      document.getElementById("return-to-lobby").click();
      const returned = document.getElementById("results-panel").hidden && !document.getElementById("lobby-panel").hidden;
      return [shown, persisted, returned].join(",");
    })()`);
    if (resultChecks !== "true,true,true") throw new Error(`round result flow failed: ${resultChecks}`);
    console.log("mobile input, orientation, Firefox-capability and persistent results flows passed");
    return;
  }

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
      await waitFor("getComputedStyle(document.getElementById('view')).outlineStyle === 'none'", "no red focus ring on the playing map");
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
      await waitFor("getComputedStyle(document.getElementById('view')).outlineStyle === 'none'", "no red focus ring on the playing map");
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
