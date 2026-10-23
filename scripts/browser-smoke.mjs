// Drives a real headless browser through the Chrome DevTools Protocol: joins the server's page,
// starts a match, plays a few keys and saves screenshots. Optional manual-style check, not part of
// `cargo test`. Usage:
//   node scripts/browser-smoke.mjs http://127.0.0.1:8080/ [out-dir]
// Needs a Chromium based browser (BROWSER=/path/to/browser, default chromium-browser).
import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";

const url = process.argv[2] ?? "http://127.0.0.1:8080/";
const out = process.argv[3] ?? "browser-smoke";
const browser = process.env.BROWSER ?? "chromium-browser";
const port = 9300 + Math.floor(Math.random() * 500);
const profile = mkdtempSync(path.join(homedir(), ".bomberust-smoke-"));
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
  console.log("audio settings:", await evaluate("localStorage.getItem('bomberust.audio.v1')"));
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
  console.log("stored look:", await evaluate("localStorage.getItem('bomberust.look.v1')"));

  await evaluate("document.getElementById('name').value = 'SmokeTest'; document.getElementById('join').click(); 1");
  await waitFor("!document.getElementById('lobby-panel').hidden", "the lobby");
  // fill the lobby with bots so that four cards show characters
  for (let i = 0; i < 3; i += 1) {
    await evaluate("document.getElementById('add-bot').click(); 1");
    await sleep(200);
  }
  await sleep(500);
  console.log("lobby:", await evaluate("document.getElementById('slots').innerText"));
  await shot("2-lobby.png");
  await sleep(1300);
  await shot("2b-lobby-later.png");

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
