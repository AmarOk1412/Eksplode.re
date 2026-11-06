import { spawn, execFileSync } from "node:child_process";
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { cpus, totalmem, release } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";
import { defaultRoom } from "../crates/web/assets/room.js";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const option = (name, fallback) => args.includes(name) ? args[args.indexOf(name) + 1] : fallback;
const targets = option("--players", "4,400,4000").split(",").map(Number);
const seconds = Number(option("--seconds", "15"));
const binary = resolve(option("--binary", `${root}/target/release/eksplode-re-server`));
const basePort = Number(option("--base-port", "18080"));
if (process.platform !== "linux") throw new Error("Resource sampling requires Linux /proc.");
if (targets.some(count => !Number.isInteger(count) || count < 4 || count > 4000 || count % 4) || !Number.isInteger(seconds) || seconds < 5 || seconds > 60 || !Number.isInteger(basePort) || basePort < 1024 || basePort > 60000) {
  throw new Error("Use 4-4000 players in multiples of four, 5-60 seconds, and a base port from 1024 to 60000.");
}
const clockHz = Number(execFileSync("getconf", ["CLK_TCK"], { encoding: "utf8" }).trim());
let requestNumber = 0;

function usage(pid) {
  const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
  const fields = stat.slice(stat.lastIndexOf(")") + 2).split(/\s+/);
  const status = readFileSync(`/proc/${pid}/status`, "utf8");
  const value = key => Number(new RegExp(`^${key}:\\s+(\\d+)`, "m").exec(status)?.[1] ?? 0);
  return {
    cpuSeconds: (Number(fields[11]) + Number(fields[12])) / clockHz,
    rssMiB: value("VmRSS") / 1024,
    highWaterMiB: value("VmHWM") / 1024,
    threads: value("Threads"),
    descriptors: readdirSync(`/proc/${pid}/fd`).length,
  };
}

function startServer(index) {
  const httpPort = basePort + index;
  const gamePort = basePort + 100 + index;
  const serverId = `load-${process.pid}-${index}`;
  const child = spawn(binary, ["--bind", "127.0.0.1", "--http-port", String(httpPort), "--game-port", String(gamePort), "--seed", "42", "--max-rooms", "128", "--max-connections", "1024", "--server-id", serverId], { stdio: ["ignore", "pipe", "pipe"] });
  const ready = new Promise((accept, reject) => {
    const timer = setTimeout(() => reject(new Error("Server startup timed out")), 10000);
    let output = "";
    child.stdout.on("data", data => {
      output = (output + data.toString()).slice(-4096);
      if (output.includes('"event":"server.started"')) { clearTimeout(timer); accept(); }
    });
    child.on("error", error => { clearTimeout(timer); reject(error); });
    child.on("exit", code => { clearTimeout(timer); reject(new Error(`Server exited with code ${code}`)); });
  });
  child.stderr.resume();
  return { child, ready, serverId, url: `ws://127.0.0.1:${httpPort}/ws` };
}

class Player {
  constructor(server, name) {
    this.server = server;
    this.name = name;
    this.socket = new WebSocket(server.url);
    this.waiters = [];
    this.last = new Map();
    this.snapshots = 0;
    this.bytes = 0;
    this.closed = false;
    this.failure = null;
    this.socket.addEventListener("message", event => {
      const message = JSON.parse(event.data);
      this.bytes += Buffer.byteLength(event.data);
      this.last.set(message.type, message);
      if (message.type === "Error") this.failure = String(message.code);
      if (message.type === "Welcome") { this.roomId = message.room_id; this.slot = message.slot; }
      if (message.type === "Snapshot") {
        this.snapshots += 1;
        this.tick = message.tick;
        this.alive = message.state.players[this.slot]?.alive === true;
        if (this.socket.readyState === WebSocket.OPEN) this.socket.send(JSON.stringify({ type: "Input", room_id: this.roomId, tick: message.tick, mv: ["Right", "Down", "Left", "Up"][Math.floor(message.tick / 12) % 4], bomb: false, punch: false }));
      }
      for (const waiter of [...this.waiters]) {
        if (waiter.test(message)) waiter.accept(message);
        else if (message.type === "Error" && message.request_id === waiter.requestId) waiter.reject(new Error(`Admission error: ${message.code}`));
      }
    });
    this.socket.addEventListener("error", () => { this.failure = "WebSocket error"; });
    this.socket.addEventListener("close", () => { this.closed = true; });
  }

  wait(test, requestId = null) {
    return new Promise((accept, reject) => {
      const timer = setTimeout(() => finish(reject, new Error("Protocol response timed out")), 10000);
      const finish = (settle, value) => {
        clearTimeout(timer);
        this.waiters = this.waiters.filter(item => item !== waiter);
        settle(value);
      };
      const waiter = { test, requestId, accept: value => finish(accept, value), reject: value => finish(reject, value) };
      this.waiters.push(waiter);
    });
  }

  async connect() {
    if (this.closed || this.failure) throw new Error("WebSocket failed before handshake");
    if (this.socket.readyState !== WebSocket.OPEN) await new Promise((accept, reject) => {
      const timer = setTimeout(() => reject(new Error("WebSocket open timed out")), 10000);
      this.socket.addEventListener("open", () => { clearTimeout(timer); accept(); }, { once: true });
      this.socket.addEventListener("error", () => { clearTimeout(timer); reject(new Error("WebSocket could not open")); }, { once: true });
    });
    const reply = this.wait(message => message.type === "Hello");
    this.socket.send(JSON.stringify({ type: "Hello", version: 2 }));
    const hello = await reply;
    if (hello.server_id !== this.server.serverId) throw new Error("Benchmark connected to an unexpected server");
    return hello;
  }

  request(payload, expected) {
    const requestId = `load-${++requestNumber}`;
    const reply = this.wait(message => message.type === expected && (message.request_id === requestId || expected === "Start"), requestId);
    this.socket.send(JSON.stringify({ ...payload, request_id: requestId }));
    return reply;
  }

  ready() {
    this.socket.send(JSON.stringify({ type: "SetReady", request_id: `load-${++requestNumber}`, room_id: this.roomId, ready: true }));
  }
}

async function measure(count) {
  if (count > 512) return measureShards(count);
  const rooms = count / 4;
  const servers = Array.from({ length: Math.ceil(rooms / 128) }, (_, index) => startServer(index));
  const players = [];
  const started = performance.now();
  let nextRoom = 0;
  let setupError = null;
  let sampler = null;
  try {
    await Promise.all(servers.map(server => server.ready));
    await Promise.all(Array.from({ length: Math.min(8, rooms) }, async () => {
      while (nextRoom < rooms && !setupError) {
        const index = nextRoom++;
        try {
          const server = servers[Math.floor(index / 128)];
          const members = Array.from({ length: 4 }, (_, slot) => new Player(server, `Load${index * 4 + slot}`));
          players.push(...members);
          const hello = await members[0].connect();
          const owner = await members[0].request({ type: "CreateRoom", name: members[0].name, capacity: 4, settings: { ...defaultRoom(), name: `Load ${index}` } }, "Welcome");
          await Promise.all(members.slice(1).map(async member => {
            await member.connect();
            await member.request({ type: "JoinRoom", name: member.name, room_id: owner.room_id, server_epoch: hello.server_epoch }, "Welcome");
          }));
          const ready = members[0].wait(message => message.type === "RoomUpdated" && message.lobby.slots.every((slot, slotIndex) => slot.controller !== "Bot" && slot.controller !== "Empty" && (slotIndex === owner.slot || slot.ready)));
          members.slice(1).forEach(member => member.ready());
          await ready;
          await members[0].request({ type: "Start", room_id: owner.room_id }, "Start");
        } catch (error) { setupError = error.message; }
      }
    }));
    if (setupError) {
      const samples = servers.map(server => usage(server.child.pid));
      return {
        requestedPlayers: count, instances: servers.length, status: "setup-failed",
        resourcePhase: "incomplete setup, not a steady-state capacity measurement",
        admittedPlayers: players.filter(player => player.roomId).length,
        serverHighWaterMiB: samples.reduce((sum, sample) => sum + sample.highWaterMiB, 0),
        clientErrors: players.filter(player => player.failure || player.closed).length,
        errorTypes: Object.fromEntries([...new Set(players.map(player => player.failure).filter(Boolean))].map(type => [type, players.filter(player => player.failure === type).length])),
        error: setupError,
      };
    }
    await delay(2000);
    const before = servers.map(server => usage(server.child.pid));
    const snapshots = players.map(player => player.snapshots);
    const bytes = players.reduce((sum, player) => sum + player.bytes, 0);
    const cpuClient = process.cpuUsage();
    let peakRss = before.reduce((sum, sample) => sum + sample.rssMiB, 0);
    let peakThreads = 0;
    let peakDescriptors = 0;
    const sample = () => {
      const values = servers.map(server => usage(server.child.pid));
      peakRss = Math.max(peakRss, values.reduce((sum, value) => sum + value.rssMiB, 0));
      peakThreads = Math.max(peakThreads, values.reduce((sum, value) => sum + value.threads, 0));
      peakDescriptors = Math.max(peakDescriptors, values.reduce((sum, value) => sum + value.descriptors, 0));
    };
    sampler = setInterval(sample, 200);
    const measureStart = performance.now();
    const measurementStartedMs = Date.now();
    await delay(seconds * 1000);
    const elapsed = (performance.now() - measureStart) / 1000;
    sample();
    const after = servers.map(server => usage(server.child.pid));
    const rates = players.map((player, index) => (player.snapshots - snapshots[index]) / elapsed).sort((left, right) => left - right);
    const percentile = fraction => rates[Math.min(rates.length - 1, Math.floor(fraction * rates.length))];
    const active = players.filter(player => !player.closed && player.alive && !player.failure).length;
    const clientCpu = process.cpuUsage(cpuClient);
    return {
      requestedPlayers: count, admittedPlayers: players.filter(player => player.roomId).length, activePlayers: active, rooms, instances: servers.length,
      status: active === count && percentile(0.05) >= 18 ? "target-met" : "target-not-met",
      setupSeconds: (measureStart - started) / 1000, measuredSeconds: elapsed,
      measurementStartedMs, measurementEndedMs: Date.now(),
      serverCpuCores: after.reduce((sum, value, index) => sum + value.cpuSeconds - before[index].cpuSeconds, 0) / elapsed,
      serverPeakRssMiB: peakRss, serverHighWaterMiB: after.reduce((sum, value) => sum + value.highWaterMiB, 0),
      serverPeakThreads: peakThreads, serverPeakDescriptors: peakDescriptors,
      payloadEgressMbps: (players.reduce((sum, player) => sum + player.bytes, 0) - bytes) * 8 / elapsed / 1e6,
      snapshotHzP05: percentile(0.05), snapshotHzMedian: percentile(0.5), snapshotHzMinimum: percentile(0),
      loadGeneratorCpuCores: (clientCpu.user + clientCpu.system) / elapsed / 1e6,
      loadGeneratorRssMiB: process.memoryUsage().rss / 1024 / 1024,
      clientErrors: players.filter(player => player.failure || player.closed).length,
      errorTypes: Object.fromEntries([...new Set(players.map(player => player.failure).filter(Boolean))].map(type => [type, players.filter(player => player.failure === type).length])),
      ...(args.includes("--raw-rates") ? { snapshotRatesHz: rates } : {}),
    };
  } finally {
    if (sampler) clearInterval(sampler);
    players.forEach(player => { player.socket.close(); player.waiters.forEach(waiter => waiter.reject(new Error("Benchmark finished"))); });
    await Promise.all(servers.map(server => new Promise(accept => {
      if (server.child.exitCode !== null) return accept();
      server.child.once("exit", accept);
      server.child.kill("SIGTERM");
    })));
    await delay(250);
  }
}

async function measureShards(count) {
  const chunks = Array.from({ length: Math.ceil(count / 512) }, (_, index) => Math.min(512, count - index * 512));
  const children = [];
  try {
    const reports = await Promise.all(chunks.map((players, index) => new Promise((accept, reject) => {
      const child = spawn(process.execPath, [fileURLToPath(import.meta.url), "--players", String(players), "--seconds", String(seconds), "--binary", binary, "--base-port", String(basePort + index * 200), "--raw-rates"], { stdio: ["ignore", "pipe", "pipe"] });
      children.push(child);
      let output = "";
      child.stdout.on("data", data => { output += data.toString(); });
      child.stderr.resume();
      child.on("error", reject);
      child.on("exit", code => {
        if (code !== 0) return reject(new Error(`Load generator ${index} exited with code ${code}`));
        try { accept(JSON.parse(output).results[0]); } catch { reject(new Error(`Load generator ${index} returned no measurement`)); }
      });
    })));
    const sum = key => reports.reduce((total, report) => total + (report[key] ?? 0), 0);
    if (reports.some(report => report.status === "setup-failed")) {
      return {
        requestedPlayers: count, instances: chunks.length, generators: chunks.length, status: "setup-failed",
        resourcePhase: "incomplete setup/partial windows, not a stable 4000-player measurement",
        admittedPlayers: reports.reduce((total, report) => total + (report.admittedPlayers ?? report.requestedPlayers), 0),
        serverHighWaterMiB: sum("serverHighWaterMiB"), clientErrors: sum("clientErrors"),
        shards: reports.map(({ snapshotRatesHz, ...report }) => report),
      };
    }
    const rates = reports.flatMap(report => report.snapshotRatesHz).sort((left, right) => left - right);
    const percentile = fraction => rates[Math.min(rates.length - 1, Math.floor(fraction * rates.length))];
    const commonSeconds = (Math.min(...reports.map(report => report.measurementEndedMs)) - Math.max(...reports.map(report => report.measurementStartedMs))) / 1000;
    return {
      requestedPlayers: count, activePlayers: sum("activePlayers"), rooms: count / 4,
      instances: chunks.length, generators: chunks.length,
      status: reports.every(report => report.status === "target-met") && commonSeconds >= seconds * 0.8 ? "target-met" : "target-not-met",
      measuredSeconds: seconds, commonConcurrentMeasurementSeconds: Math.max(0, commonSeconds),
      serverCpuCores: sum("serverCpuCores"), serverPeakRssMiB: sum("serverPeakRssMiB"), serverHighWaterMiB: sum("serverHighWaterMiB"),
      peakMethod: "sum of per-instance observed peaks over overlapping windows",
      serverPeakThreads: sum("serverPeakThreads"), serverPeakDescriptors: sum("serverPeakDescriptors"),
      payloadEgressMbps: sum("payloadEgressMbps"),
      snapshotHzP05: percentile(0.05), snapshotHzMedian: percentile(0.5), snapshotHzMinimum: percentile(0),
      loadGeneratorCpuCores: sum("loadGeneratorCpuCores"), loadGeneratorRssMiB: sum("loadGeneratorRssMiB"), clientErrors: sum("clientErrors"),
      shards: reports.map(({ snapshotRatesHz, ...report }) => report),
    };
  } finally {
    for (const child of children) if (child.exitCode === null) child.kill("SIGTERM");
  }
}

const report = {
  date: new Date().toISOString(),
  hardware: { cpu: cpus()[0]?.model, logicalCpus: cpus().length, ramGiB: totalmem() / 1024 ** 3, kernel: release(), node: process.version },
  workload: { transport: "WebSocket over loopback", profile: "release", mode: "normal", board: "small 15x13", humansPerRoom: 4, bots: 0, bombs: false, input: "one movement input per received snapshot", warmupSeconds: 2, seconds, targetSnapshotHz: 20, passingP05Hz: 18, tls: false, nginx: false },
  results: [],
};
for (const count of targets) {
  console.error(`Measuring ${count} active players...`);
  report.results.push(await measure(count));
  console.error(`${count} players: ${report.results.at(-1).status}`);
}
const output = JSON.stringify(report, null, 2);
if (args.includes("--output")) writeFileSync(resolve(option("--output")), output + "\n");
console.log(output);