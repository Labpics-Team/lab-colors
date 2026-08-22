import { createHash } from "node:crypto";
import { execFileSync, spawn } from "node:child_process";
import { access, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, isAbsolute, join, resolve } from "node:path";

const LOOPBACK = "127.0.0.1";
const REFERENCE_WIRE_HEX =
  "4c4350570100b3000000010000000b0000001414140100000015000000010b0000000000" +
  "000000000000010000001f00000000000000010000002900000001150000000100000033" +
  "000000011f000000010000003d000000290000003300000000000000000050409a999999" +
  "9999c93f0101000000470000003d00000001000000470000003d00000001000000510000" +
  "00093d000000030100000052000000013d000000141414010000005b00000029000000";

function fail(message, options) {
  throw new Error(`Program browser proof: ${message}`, options);
}

function positiveIntegerEnv(name) {
  const raw = process.env[name];
  if (!/^[1-9][0-9]*$/u.test(raw ?? "")) fail(`${name} must be a positive integer`);
  const value = Number(raw);
  if (!Number.isSafeInteger(value)) fail(`${name} exceeds the safe integer range`);
  return value;
}

async function executableEnv(name) {
  const value = process.env[name];
  if (!value || !isAbsolute(value)) fail(`${name} must name an absolute executable`);
  await access(value);
  if (!(await stat(value)).isFile()) fail(`${name} must name a regular file`);
  return value;
}

function parseArgs() {
  const args = process.argv.slice(2);
  if (args.length !== 2) fail("expected <tarball.tgz> <lowercase-sha256>");
  const tarball = resolve(args[0]);
  if (extname(tarball) !== ".tgz") fail("tarball must use the .tgz suffix");
  if (!/^[0-9a-f]{64}$/u.test(args[1])) fail("tarball digest must be lowercase SHA-256");
  return { tarball, digest: args[1] };
}

function npmInstall(tarball, root, timeout) {
  const npmExec = process.env.npm_execpath;
  const command = process.platform === "win32" && npmExec ? process.execPath : "npm";
  const prefix = process.platform === "win32" && npmExec ? [npmExec] : [];
  execFileSync(command, [
    ...prefix,
    "install",
    "--offline",
    "--ignore-scripts",
    "--no-audit",
    "--no-fund",
    "--no-package-lock",
    "--save=false",
    tarball,
  ], { cwd: root, stdio: ["ignore", "pipe", "pipe"], timeout });
}

async function listen(server) {
  await new Promise((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(0, LOOPBACK, resolveListen);
  });
  const address = server.address();
  if (address === null || typeof address === "string") fail("proof server has no TCP address");
  return address.port;
}

async function request(base, path, method, body, signal) {
  const response = await fetch(`${base}${path}`, {
    method,
    body: body === undefined ? undefined : JSON.stringify(body),
    headers: body === undefined ? undefined : { "content-type": "application/json" },
    signal,
  });
  const payload = await response.json();
  if (!response.ok || payload?.value?.error) {
    fail(`WebDriver rejected ${method} ${path}: ${payload?.value?.message ?? response.status}`);
  }
  return payload.value;
}

async function waitForDriver(signal, port, child) {
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    if (child?.killed || child?.exitCode !== null) {
      fail(`ChromeDriver exited prematurely with code ${child?.exitCode ?? "unknown"}`);
    }
    try {
      const response = await fetch(`http://127.0.0.1:${port}/status`, { signal });
      if (response.ok) return;
    } catch {}
    await new Promise((resolveDelay) => setTimeout(resolveDelay, 50));
  }
  fail("ChromeDriver did not become ready");
}

async function main() {
  const { tarball, digest } = parseArgs();
  const timeout = positiveIntegerEnv("LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS");
  const chrome = await executableEnv("CHROME_PATH");
  const driver = await executableEnv("CHROMEDRIVER_PATH");
  const actualDigest = createHash("sha256").update(await readFile(tarball)).digest("hex");
  if (actualDigest !== digest) fail(`tarball digest mismatch: ${actualDigest}`);

  const root = await mkdtemp(join(tmpdir(), "labcolors-program-browser-"));
  let server;
  const driverPort = 9515;
  const child = spawn(driver, [`--port=${driverPort}`], {
    env: process.env,
    stdio: "ignore",
    windowsHide: true,
  });
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(new Error("browser proof timed out")), timeout);
  let sessionId;
  try {
    await writeFile(join(root, "package.json"), '{"private":true,"type":"module"}\n');
    npmInstall(tarball, root, timeout);
    const installed = join(root, "node_modules", "@labpics", "colors");
    const files = new Map();
    for (const path of ["index.js", "pkg/labcolors.js", "pkg/labcolors_bg.wasm"]) {
      files.set(`/${path}`, await readFile(join(installed, path)));
    }
    server = createServer((req, res) => {
      const pathname = new URL(req.url ?? "/", `http://${LOOPBACK}`).pathname;
      if (pathname === "/") {
        res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
        res.end("<!doctype html><meta charset=utf-8><title>Lab Colors Program proof</title>");
        return;
      }
      const bytes = files.get(pathname);
      if (bytes) {
        res.writeHead(200, {
          "cache-control": "no-store",
          "content-type": pathname.endsWith(".wasm") ? "application/wasm" : "text/javascript; charset=utf-8",
          "x-content-type-options": "nosniff",
        });
        res.end(bytes);
        return;
      }
      res.writeHead(404).end();
    });
    const port = await listen(server);
    const origin = `http://${LOOPBACK}:${port}`;
    await waitForDriver(controller.signal, driverPort, child);
    const session = await request(`http://127.0.0.1:${driverPort}`, "/session", "POST", {
      capabilities: { alwaysMatch: { browserName: "chrome", "goog:chromeOptions": {
        binary: chrome,
        args: ["--headless=new", "--no-sandbox", "--disable-dev-shm-usage"],
      } } },
    }, controller.signal);
    sessionId = session.sessionId;
    const base = `http://127.0.0.1:9515/session/${sessionId}`;
    await request(base, "/url", "POST", { url: origin }, controller.signal);
    const script = `const done=arguments[arguments.length-1];(async()=>{` +
      `const api=await import(${JSON.stringify(`${origin}/index.js`)});` +
      `await api.init({module_or_path:fetch(${JSON.stringify(`${origin}/pkg/labcolors_bg.wasm`)})});` +
      `const wire=Uint8Array.from(${JSON.stringify(REFERENCE_WIRE_HEX)}.match(/../g),x=>parseInt(x,16));` +
      `const runtime=api.compileProgramWire(wire,1);` +
      `const snapshot=runtime.updateObserved(1n,new Uint32Array([1]),new Uint8Array([255,255,255]),1);` +
      `const value={state:snapshot.state,count:snapshot.outputCount(),slot:snapshot.outputSlot(0),rgb:Array.from(snapshot.outputRgb(0)),opacity:snapshot.outputOpacity(0)};` +
      `snapshot.free();runtime.free();done(value);})().catch(error=>done({error:String(error)}));`;
    const result = await request(base, "/execute/async", "POST", { script, args: [] }, controller.signal);
    if (
      result?.error ||
      result?.state !== "ready" ||
      result?.count !== 1 ||
      result?.slot !== 91 ||
      JSON.stringify(result?.rgb) !== "[20,20,20]" ||
      result?.opacity !== 1
    ) fail(`terminal Program result drifted: ${JSON.stringify(result)}`);
    console.log(`LAB_COLORS_PROGRAM_BROWSER_PASS sha256=${digest}`);
    await request(base, "", "DELETE", undefined, controller.signal);
    sessionId = undefined;
  } finally {
    clearTimeout(timer);
    if (sessionId) {
      const cleanupSignal = AbortSignal.timeout(5_000);
      try { await request(`http://127.0.0.1:${driverPort}/session/${sessionId}`, "", "DELETE", undefined, cleanupSignal); } catch {}
    }
    child.kill();
    if (server) await new Promise((resolveClose) => server.close(resolveClose));
    await rm(root, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack : String(error));
  process.exitCode = 1;
});
