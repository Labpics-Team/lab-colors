import { createHash } from "node:crypto";
import { execFileSync, spawn } from "node:child_process";
import { access, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { observeChildErrors, releaseChild, waitForDriver } from "./browser-child-lifecycle.mjs";
import { retainImportedRuntimeSnippets } from "./package-runtime-snippets.mjs";

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

export function browserCleanup(primary, resources) {
  const cleanupErrors = [];
  for (const resource of resources) {
    try {
      resource.release();
    } catch (error) {
      cleanupErrors.push({ resource: resource.name, error });
    }
  }
  const outcome = primary === undefined
    ? {}
    : { error: String(primary), code: primary?.code, operation: primary?.operation };
  if (cleanupErrors.length > 0) {
    outcome.cleanupError = {
      code: "BROWSER_PROOF_CLEANUP_FAILED",
      resources: cleanupErrors.map(({ resource }) => resource),
      errors: cleanupErrors.map(({ error }) => ({ message: String(error), code: error?.code })),
    };
  }
  return outcome;
}

function acquisition(evidence, name, fault) {
  evidence.acquired.push(name);
  if (fault?.after === name) {
    throw Object.assign(new Error(`fault after ${name}`), {
      code: "BROWSER_PROOF_INJECTED", operation: name,
    });
  }
}

function released(evidence, name, fault) {
  evidence.released.push(name);
  // Ошибка моделируется после настоящего освобождения, чтобы QA не оставлял утечки.
  if (fault?.cleanup === name) {
    throw Object.assign(new Error(`cleanup fault at ${name}`), { code: "BROWSER_PROOF_INJECTED_CLEANUP" });
  }
}

export async function verifyCleanupFaultMatrix(run) {
  if (typeof run !== "function") fail("fault matrix requires an actual-path runner");
  const reports = [];
  for (const after of RESOURCE_ORDER) {
    for (const cleanup of [undefined, after]) {
      const report = await run({ after, cleanup });
      const expected = RESOURCE_ORDER.slice(0, RESOURCE_ORDER.indexOf(after) + 1);
      if (report.error !== `Error: fault after ${after}` || report.code !== "BROWSER_PROOF_INJECTED" || report.operation !== after) {
        fail(`fault primary evidence drifted after ${after}: ${JSON.stringify(report)}`);
      }
      if (JSON.stringify(report.acquired) !== JSON.stringify(expected)
        || JSON.stringify(report.released) !== JSON.stringify(expected.toReversed())
        || expected.some((name) => report.readback?.[name] !== true)) {
        fail(`fault cleanup evidence drifted after ${after}: ${JSON.stringify(report)}`);
      }
      const cleanupError = report.cleanupError;
      if (cleanup === undefined ? cleanupError !== undefined
        : cleanupError?.code !== "BROWSER_PROOF_CLEANUP_FAILED"
          || JSON.stringify(cleanupError.resources) !== JSON.stringify([cleanup])
          || cleanupError.errors?.[0]?.code !== "BROWSER_PROOF_INJECTED_CLEANUP") {
        fail(`fault typed cleanup evidence drifted after ${after}`);
      }
      reports.push(report);
    }
  }
  return reports;
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

export async function packedBrowserFiles(installed) {
  const paths = ["index.js", "program-wire/abi-v1.js", "pkg/labcolors.js", "pkg/labcolors_bg.wasm"];
  const runtimeSource = await readFile(join(installed, "pkg/labcolors.js"), "utf8");
  paths.push(...await retainImportedRuntimeSnippets(installed, runtimeSource));
  return new Map(await Promise.all(
    paths.map(async (path) => [`/${path}`, await readFile(join(installed, path))]),
  ));
}

export function browserScenario(origin, fault) {
  // В браузер передаются те же функции, которые исполняет regression, без второй cleanup-реализации.
  return `const done=arguments[arguments.length-1];
    const browserCleanup=${browserCleanup.toString()};
    const acquisition=${acquisition.toString()};
    const released=${released.toString()};
    (${browserConsumer.toString()})(${JSON.stringify(origin)},${JSON.stringify(fault)})
      .then(proof=>done({proof}),error=>done({proof:browserCleanup(error,[])}));`;
}

async function browserConsumer(origin, fault) {
  const resources = [];
  const evidence = { acquired: [], released: [], readback: {} };
  let primary, result, runtime, snapshot, element;
  try {
    const api = await import(`${origin}/index.js`);
    const wire = await import(`${origin}/program-wire/abi-v1.js`);
    await api.init({ module_or_path: fetch(`${origin}/pkg/labcolors_bg.wasm`) });
    const builder = new wire.ProgramWireBuilderV1();
    builder.source(11, [20, 20, 20]).fixedTarget(21, 11).surfaceInputPort(31)
      .solidPaint(41, 21).inputSurface(51, 31)
      .sourceOverOccurrence(61, 41, 51, 64, .2, wire.SURROUND_AVERAGE_V1)
      .presentationRoot(71, 61).presentationTarget(71, 61)
      .wcag22VisibleUnary(true, 81, 61, wire.WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1)
      .exactVisibleUnary(false, 82, 61, [20, 20, 20]).output(91, 41);
    runtime = api.compileProgramWire(builder.finish(), 1);
    resources.push({ name: "runtime", release() { runtime.free(); released(evidence, "runtime", fault); } });
    acquisition(evidence, "runtime", fault);
    snapshot = runtime.updateObserved(1n, new Uint32Array([1]), new Uint8Array([255, 255, 255]), 1);
    resources.push({ name: "snapshot", release() { snapshot.free(); released(evidence, "snapshot", fault); } });
    acquisition(evidence, "snapshot", fault);
    const token = "consumer.foreground", slots = new Map([[token, 91]]);
    element = document.createElement("div");
    let hostDisposed = false;
    const host = { free() {
      if (hostDisposed) return;
      element.style.removeProperty("--consumer-color");
      element.remove();
      hostDisposed = true;
    } };
    resources.push({ name: "host", release() { host.free(); host.free(); released(evidence, "host", fault); } });
    element.style.color = "var(--consumer-color)";
    document.body.append(element);
    acquisition(evidence, "host", fault);
    const materialize = (s) => {
      let found = -1;
      for (let i = 0; i < s.outputCount(); i += 1) if (s.outputSlot(i) === slots.get(token)) found = i;
      if (found < 0) throw new Error("opaque consumer token was not materialized");
      const [r, g, b] = s.outputRgb(found);
      element.style.setProperty("--consumer-color", `rgb(${r} ${g} ${b} / ${s.outputOpacity(found)})`);
    };
    const rgba = (value) => {
      const channels = value.match(/[\d.]+/g)?.map(Number);
      if (!channels || channels.length < 3) throw new Error("computed color was not RGB");
      return [channels[0], channels[1], channels[2], channels[3] ?? 1];
    };
    materialize(snapshot);
    const before = rgba(getComputedStyle(element).color);
    let rejected = false;
    try {
      const unexpected = runtime.updateObserved(2n, new Uint32Array([]), new Uint8Array([]), 1);
      unexpected.free();
    } catch (error) {
      rejected = api.isProgramError(error) && error.code === "program_update" && error.operation === "updateObserved";
    }
    const after = rgba(getComputedStyle(element).color);
    result = { before, after, rejected, oracle: [20, 20, 20, 1], slot: snapshot.outputSlot(0), state: snapshot.state };
  } catch (error) {
    primary = error;
  }
  const outcome = browserCleanup(primary, resources.toReversed());
  // Проверяется инвалидированный handle точного wasm-bindgen artifact, не размер WASM heap.
  if (runtime) evidence.readback.runtime = runtime.__wbg_ptr === 0;
  if (snapshot) evidence.readback.snapshot = snapshot.__wbg_ptr === 0;
  if (element) evidence.readback.host = !element.isConnected && element.style.getPropertyValue("--consumer-color") === "";
  return { ...result, ...outcome, ...evidence };
}

export async function runBrowserProof({ tarball, timeout, chrome, driver }, fault) {
  const resources = [];
  const evidence = { acquired: [], released: [], readback: {} };
  let primary, result, root, child, childErrors, server, base;
  let timer;
  try {
    root = await mkdtemp(join(tmpdir(), "labcolors-program-browser-"));
    resources.push({ name: "temp-install", release: async () => {
      await rm(root, { recursive: true, force: true });
      released(evidence, "temp-install", fault);
    } });
    acquisition(evidence, "temp-install", fault);
    await writeFile(join(root, "package.json"), '{"private":true,"type":"module"}\n');
    npmInstall(tarball, root, timeout);
    const driverPort = 9515;
    child = spawn(driver, [`--port=${driverPort}`], { env: process.env, stdio: "ignore", windowsHide: true });
    childErrors = observeChildErrors(child);
    resources.push({
      name: "browser",
      release: async () => {
        await releaseChild(child, childErrors, 2_000);
        released(evidence, "browser", fault);
      },
    });
    acquisition(evidence, "browser", fault);
    const controller = new AbortController();
    timer = setTimeout(() => controller.abort(new Error("browser proof timed out")), timeout);
    await waitForDriver(controller.signal, driverPort, child, childErrors);
    const session = await request(`http://${LOOPBACK}:${driverPort}`, "/session", "POST", { capabilities: { alwaysMatch: { browserName: "chrome", "goog:chromeOptions": { binary: chrome, args: ["--headless=new", "--no-sandbox", "--disable-dev-shm-usage"] } } } }, controller.signal);
    base = `http://${LOOPBACK}:${driverPort}/session/${session.sessionId}`;
    resources.push({ name: "browser-session", release: async () => {
      await request(base, "", "DELETE", undefined, AbortSignal.timeout(5_000));
      const response = await fetch(`${base}/url`, { signal: AbortSignal.timeout(5_000) });
      evidence.readback["browser-session"] = (await response.json())?.value?.error === "invalid session id";
      released(evidence, "browser-session", fault);
    } });
    acquisition(evidence, "browser-session", fault);
    const installed = join(root, "node_modules", "@labpics", "colors");
    const files = await packedBrowserFiles(installed);
    server = createServer((req, res) => {
      const pathname = new URL(req.url ?? "/", `http://${LOOPBACK}`).pathname;
      if (pathname === "/") { res.writeHead(200, { "content-type": "text/html; charset=utf-8" }); res.end("<!doctype html><style>#proof{color:var(--consumer-color)}</style><div id=proof></div>"); return; }
      const bytes = files.get(pathname);
      if (!bytes) { res.writeHead(404).end(); return; }
      res.writeHead(200, { "cache-control": "no-store", "content-type": pathname.endsWith(".wasm") ? "application/wasm" : "text/javascript; charset=utf-8", "x-content-type-options": "nosniff" }); res.end(bytes);
    });
    resources.push({ name: "server", release: async () => {
      if (server.listening) {
        server.closeAllConnections();
        await new Promise((accept, reject) => server.close((error) => error ? reject(error) : accept()));
      }
      released(evidence, "server", fault);
    } });
    const port = await listen(server);
    acquisition(evidence, "server", fault);
    const origin = `http://${LOOPBACK}:${port}`;
    await request(base, "/url", "POST", { url: origin }, controller.signal);
    const response = await request(base, "/execute/async", "POST", { script: browserScenario(origin, fault), args: [] }, controller.signal);
    result = response.proof;
    evidence.acquired.push(...result.acquired);
    evidence.released.push(...result.released);
    Object.assign(evidence.readback, result.readback);
  } catch (error) {
    primary = error;
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
  let outcome = {};
  try {
    await cleanupResources(resources, primary);
  } catch (error) {
    if (error instanceof BrowserProofCleanupError) {
      outcome = browserCleanup(error.cause, []);
      outcome.cleanupError = {
        code: error.code,
        resources: error.cleanupErrors.map(({ resource }) => resource),
        errors: error.cleanupErrors.map(({ error: failure }) => ({ message: String(failure), code: failure?.code })),
      };
    } else {
      outcome = browserCleanup(error, []);
    }
  }
  if (root) {
    try { await stat(root); evidence.readback["temp-install"] = false; }
    catch (error) {
      if (error.code !== "ENOENT") throw error;
      evidence.readback["temp-install"] = true;
    }
  }
  if (child) {
    let absent = false;
    if (child.pid !== undefined) {
      try { process.kill(child.pid, 0); }
      catch (error) { if (error.code !== "ESRCH") throw error; absent = true; }
    }
    evidence.readback.browser = absent && childErrors.closed && child.listenerCount("error") === 0;
  }
  if (server) evidence.readback.server = !server.listening && server.address() === null;
  // Ошибки двух execution realms объединяются, а не перекрываются object spread.
  if (result?.cleanupError && outcome.cleanupError) {
    outcome.cleanupError.resources.unshift(...result.cleanupError.resources);
    outcome.cleanupError.errors.unshift(...result.cleanupError.errors);
  }
  return { ...result, ...outcome, ...evidence };
}

async function main() {
  const { tarball, digest } = parseArgs();
  const timeout = positiveIntegerEnv("LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS");
  const [chrome, driver] = await Promise.all([executableEnv("CHROME_PATH"), executableEnv("CHROMEDRIVER_PATH")]);
  const actualDigest = createHash("sha256").update(await readFile(tarball)).digest("hex");
  if (actualDigest !== digest) fail(`tarball digest mismatch: ${actualDigest}`);
  const run = (fault) => runBrowserProof({ tarball, timeout, chrome, driver }, fault);
  const result = await run();
  if (result.error || result.cleanupError || result.state !== "ready" || result.slot !== 91 || !result.rejected
    || JSON.stringify(result.before) !== JSON.stringify(result.oracle)
    || JSON.stringify(result.after) !== JSON.stringify(result.oracle)
    || JSON.stringify(result.acquired) !== JSON.stringify(RESOURCE_ORDER)
    || JSON.stringify(result.released) !== JSON.stringify(RESOURCE_ORDER.toReversed())
    || RESOURCE_ORDER.some((name) => result.readback[name] !== true)) {
    fail(`browser consumer result drifted: ${JSON.stringify(result)}`);
  }
  const reports = await verifyCleanupFaultMatrix(run);
  for (const report of reports) console.log(`LAB_COLORS_PROGRAM_BROWSER_FAULT ${JSON.stringify(report)}`);
  console.log(`LAB_COLORS_PROGRAM_BROWSER_PASS sha256=${digest}`);
}

const invokedDirectly = process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) main().catch((error) => { console.error(error instanceof Error ? error.stack : String(error)); process.exitCode = 1; });
