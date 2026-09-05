import { createHash } from "node:crypto";
import { execFileSync, spawn } from "node:child_process";
import { access, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const LOOPBACK = "127.0.0.1";
const RESOURCE_ORDER = [
  "temp-install",
  "browser",
  "browser-session",
  "server",
  "runtime",
  "snapshot",
  "host",
];

export class BrowserProofCleanupError extends Error {
  constructor(primary, cleanupErrors) {
    super("Program browser proof cleanup failed", { cause: primary });
    this.name = "BrowserProofCleanupError";
    this.code = "BROWSER_PROOF_CLEANUP_FAILED";
    this.cleanupErrors = cleanupErrors;
  }
}

function fail(message, options) {
  throw new Error(`Program browser proof: ${message}`, options);
}

export async function cleanupResources(resources, primary) {
  const cleanupErrors = [];
  for (const resource of resources.toReversed()) {
    try {
      await resource.release();
    } catch (error) {
      cleanupErrors.push({ resource: resource.name, error });
    }
  }
  if (cleanupErrors.length > 0) throw new BrowserProofCleanupError(primary, cleanupErrors);
  if (primary !== undefined) throw primary;
}

export async function verifyCleanupFaultMatrix() {
  for (let faultIndex = 0; faultIndex < RESOURCE_ORDER.length; faultIndex += 1) {
    const acquired = [];
    const released = [];
    const primary = new Error(`fault after ${RESOURCE_ORDER[faultIndex]}`);
    for (const name of RESOURCE_ORDER.slice(0, faultIndex + 1)) {
      acquired.push({ name, release: async () => released.push(name) });
    }
    try {
      await cleanupResources(acquired, primary);
      fail("fault injection did not preserve its primary failure");
    } catch (error) {
      if (error !== primary) throw error;
    }
    const expected = RESOURCE_ORDER.slice(0, faultIndex + 1).reverse();
    if (JSON.stringify(released) !== JSON.stringify(expected)) {
      fail(`cleanup order drifted after ${RESOURCE_ORDER[faultIndex]}`);
    }
  }
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
  execFileSync(command, [...prefix, "install", "--offline", "--ignore-scripts", "--no-audit", "--no-fund", "--no-package-lock", "--save=false", tarball], {
    cwd: root, stdio: ["ignore", "pipe", "pipe"], timeout,
  });
}

async function listen(server) {
  await new Promise((accept, reject) => {
    server.once("error", reject);
    server.listen(0, LOOPBACK, accept);
  });
  const address = server.address();
  if (address === null || typeof address === "string") fail("proof server has no TCP address");
  return address.port;
}

async function request(base, path, method, body, signal) {
  const response = await fetch(`${base}${path}`, {
    method, body: body === undefined ? undefined : JSON.stringify(body),
    headers: body === undefined ? undefined : { "content-type": "application/json" }, signal,
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
    if (child.killed || child.exitCode !== null) fail(`ChromeDriver exited prematurely with code ${child.exitCode ?? "unknown"}`);
    try {
      const response = await fetch(`http://${LOOPBACK}:${port}/status`, { signal });
      if (response.ok) return;
    } catch (error) {
      if (signal.aborted) throw error;
    }
    await new Promise((accept) => setTimeout(accept, 50));
  }
  fail("ChromeDriver did not become ready");
}

function browserScenario(origin) {
  return `const done=arguments[arguments.length-1];(async()=>{const released=[];let host,runtime,snapshot,result;try{` +
    `const api=await import(${JSON.stringify(`${origin}/index.js`)}),wire=await import(${JSON.stringify(`${origin}/program-wire/abi-v1.js`)});` +
    `await api.init({module_or_path:fetch(${JSON.stringify(`${origin}/pkg/labcolors_bg.wasm`)})});` +
    `const builder=new wire.ProgramWireBuilderV1();builder.source(11,[20,20,20]).fixedTarget(21,11).surfaceInputPort(31).solidPaint(41,21).inputSurface(51,31).sourceOverOccurrence(61,41,51,64,.2,wire.SURROUND_AVERAGE_V1).presentationRoot(71,61).presentationTarget(71,61).wcag22VisibleUnary(true,81,61,wire.WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1).exactVisibleUnary(false,82,61,[20,20,20]).output(91,41);` +
    `runtime=api.compileProgramWire(builder.finish(),1);snapshot=runtime.updateObserved(1n,new Uint32Array([1]),new Uint8Array([255,255,255]),1);` +
    `const token="consumer.foreground",slots=new Map([[token,91]]),element=document.createElement("div");element.style.color="var(--consumer-color)";document.body.append(element);let hostDisposed=false;host={free(){if(hostDisposed)return;hostDisposed=true;element.style.removeProperty("--consumer-color");element.remove()}};` +
    `const materialize=s=>{let found=-1;for(let i=0;i<s.outputCount();i+=1)if(s.outputSlot(i)===slots.get(token))found=i;if(found<0)throw new Error("opaque consumer token was not materialized");const [r,g,b]=s.outputRgb(found);element.style.setProperty("--consumer-color","rgb("+r+" "+g+" "+b+" / "+s.outputOpacity(found)+")")};` +
    `const rgba=value=>{const channels=value.match(/[\\d.]+/g)?.map(Number);if(!channels||channels.length<3)throw new Error("computed color was not RGB");return [channels[0],channels[1],channels[2],channels[3]??1]};materialize(snapshot);` +
    `const before=rgba(getComputedStyle(element).color);let rejected=false;try{runtime.updateObserved(2n,new Uint32Array([]),new Uint8Array([]),1)}catch(error){rejected=error instanceof Error&&error.name==="Error"&&error.code==="program_update"&&error.operation==="updateObserved"}const after=rgba(getComputedStyle(element).color);` +
    `result={before,after,rejected,oracle:[20,20,20,1],slot:snapshot.outputSlot(0),state:snapshot.state};` +
    `}catch(error){result={error:String(error),code:error?.code}}finally{if(host){host.free();host.free();released.push("host")}if(snapshot){snapshot.free();released.push("snapshot")}if(runtime){runtime.free();released.push("runtime")}}result.released=released;done(result)})()`;
}

async function main() {
  await verifyCleanupFaultMatrix();
  const { tarball, digest } = parseArgs();
  const timeout = positiveIntegerEnv("LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS");
  const [chrome, driver] = await Promise.all([executableEnv("CHROME_PATH"), executableEnv("CHROMEDRIVER_PATH")]);
  const actualDigest = createHash("sha256").update(await readFile(tarball)).digest("hex");
  if (actualDigest !== digest) fail(`tarball digest mismatch: ${actualDigest}`);

  const resources = [];
  let primary;
  let timer;
  try {
    const root = await mkdtemp(join(tmpdir(), "labcolors-program-browser-"));
    resources.push({ name: "temp-install", release: () => rm(root, { recursive: true, force: true }) });
    await writeFile(join(root, "package.json"), '{"private":true,"type":"module"}\n');
    npmInstall(tarball, root, timeout);
    const driverPort = 9515;
    const child = spawn(driver, [`--port=${driverPort}`], { env: process.env, stdio: "ignore", windowsHide: true });
    resources.push({
      name: "browser",
      release: async () => {
        if (child.exitCode !== null) return;
        const exited = new Promise((accept, reject) => {
          child.once("exit", accept);
          child.once("error", reject);
        });
        if (!child.kill()) fail("ChromeDriver rejected termination");
        await exited;
      },
    });
    const controller = new AbortController();
    timer = setTimeout(() => controller.abort(new Error("browser proof timed out")), timeout);
    await waitForDriver(controller.signal, driverPort, child);
    const session = await request(`http://${LOOPBACK}:${driverPort}`, "/session", "POST", { capabilities: { alwaysMatch: { browserName: "chrome", "goog:chromeOptions": { binary: chrome, args: ["--headless=new", "--no-sandbox", "--disable-dev-shm-usage"] } } } }, controller.signal);
    const base = `http://${LOOPBACK}:${driverPort}/session/${session.sessionId}`;
    resources.push({ name: "browser-session", release: () => request(base, "", "DELETE", undefined, AbortSignal.timeout(5_000)) });
    const installed = join(root, "node_modules", "@labpics", "colors");
    const files = new Map();
    for (const path of ["index.js", "program-wire/abi-v1.js", "pkg/labcolors.js", "pkg/labcolors_bg.wasm"]) files.set(`/${path}`, await readFile(join(installed, path)));
    const server = createServer((req, res) => {
      const pathname = new URL(req.url ?? "/", `http://${LOOPBACK}`).pathname;
      if (pathname === "/") { res.writeHead(200, { "content-type": "text/html; charset=utf-8" }); res.end("<!doctype html><style>#proof{color:var(--consumer-color)}</style><div id=proof></div>"); return; }
      const bytes = files.get(pathname);
      if (!bytes) { res.writeHead(404).end(); return; }
      res.writeHead(200, { "cache-control": "no-store", "content-type": pathname.endsWith(".wasm") ? "application/wasm" : "text/javascript; charset=utf-8", "x-content-type-options": "nosniff" }); res.end(bytes);
    });
    const port = await listen(server);
    resources.push({ name: "server", release: () => new Promise((accept, reject) => server.close((error) => error ? reject(error) : accept())) });
    const origin = `http://${LOOPBACK}:${port}`;
    await request(base, "/url", "POST", { url: origin }, controller.signal);
    const result = await request(base, "/execute/async", "POST", { script: browserScenario(origin), args: [] }, controller.signal);
    clearTimeout(timer);
    if (result.error || result.state !== "ready" || result.slot !== 91 || !result.rejected || JSON.stringify(result.before) !== JSON.stringify(result.oracle) || JSON.stringify(result.after) !== JSON.stringify(result.oracle) || JSON.stringify(result.released) !== '["host","snapshot","runtime"]') fail(`browser consumer result drifted: ${JSON.stringify(result)}`);
    console.log(`LAB_COLORS_PROGRAM_BROWSER_PASS sha256=${digest}`);
  } catch (error) {
    primary = error;
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
  await cleanupResources(resources, primary);
}

const invokedDirectly = process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) main().catch((error) => { console.error(error instanceof Error ? error.stack : String(error)); process.exitCode = 1; });
