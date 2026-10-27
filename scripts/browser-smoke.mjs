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

  await call("Runtime.enable");
  await call("Page.enable");
  await call("Page.navigate", { url });
  await waitFor("document.readyState === 'complete' && !document.getElementById('join').disabled", "the page");

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
    throw new Error(`peer timed out waiting for ${what}`);
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

  // fill the lobby with bots so that four cards show characters
  for (let i = 0; i < 3; i += 1) {
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
