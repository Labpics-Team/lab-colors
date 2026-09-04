import assert from "node:assert/strict";
import { createHash, randomBytes } from "node:crypto";
import { execFileSync, spawn } from "node:child_process";
import { access, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { createServer as createTcpServer } from "node:net";
import { tmpdir } from "node:os";
import { extname, isAbsolute, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const LOOPBACK = "127.0.0.1";
const DRIVER_LOG_LIMIT_BYTES = 64 * 1024;
const DRIVER_START_ATTEMPTS = 4;
const DRIVER_START_ATTEMPT_TIMEOUT_MS = 5_000;
const DRIVER_STOP_TIMEOUT_MS = 5_000;
const REFERENCE_WIRE_HEX =
  "4c4350570100b3000000010000000b0000001414140100000015000000010b0000000000" +
  "000000000000010000001f00000000000000010000002900000001150000000100000033" +
  "000000011f000000010000003d000000290000003300000000000000000050409a999999" +
  "9999c93f0101000000470000003d00000001000000470000003d00000001000000510000" +
  "00093d000000030100000052000000013d000000141414010000005b00000029000000";

function fail(message, options) {
  throw new Error(`Program browser proof: ${message}`, options);
}

/**
 * WebDriver serializes object properties in an implementation-defined order.
 * Compare the decoded value structurally so that order is not mistaken for
 * a semantic runtime result, while still rejecting every missing, extra, or
 * type/value-drifted field.
 */
export function assertExactProgramResult(actual, expected) {
  try {
    assert.deepStrictEqual(actual, expected);
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    fail(
      `terminal Program result drifted: ${JSON.stringify(actual)}\n${detail}`,
      { cause: error },
    );
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
  const text = await response.text();
  let payload;
  try {
    payload = JSON.parse(text);
  } catch (error) {
    fail(`WebDriver returned non-JSON for ${method} ${path}: HTTP ${response.status}`, {
      cause: error,
    });
  }
  if (!response.ok || payload?.value?.error) {
    fail(`WebDriver rejected ${method} ${path}: ${payload?.value?.message ?? response.status}`);
  }
  return payload.value;
}

function captureBounded(stream) {
  const chunks = [];
  let keptBytes = 0;
  let totalBytes = 0;
  stream.on("data", (value) => {
    const chunk = Buffer.isBuffer(value) ? value : Buffer.from(value);
    totalBytes += chunk.length;
    const remaining = DRIVER_LOG_LIMIT_BYTES - keptBytes;
    if (remaining <= 0) return;
    const kept = chunk.subarray(0, remaining);
    chunks.push(kept);
    keptBytes += kept.length;
  });
  return {
    text() {
      const captured = Buffer.concat(chunks, keptBytes).toString("utf8");
      const omitted = totalBytes - keptBytes;
      return omitted > 0 ? `${captured}\n...[${omitted} bytes omitted]` : captured;
    },
  };
}

function observeDriver(child) {
  if (child.stdout === null || child.stderr === null) {
    fail("ChromeDriver stdio was not captured");
  }
  let spawnError;
  child.once("error", (error) => {
    spawnError = error;
  });
  const closed = new Promise((resolveClose) => {
    child.once("close", (code, signal) => resolveClose({ code, signal }));
  });
  return {
    stdout: captureBounded(child.stdout),
    stderr: captureBounded(child.stderr),
    closed,
    spawnError: () => spawnError,
  };
}

function driverDiagnostics(observation) {
  if (!observation) return "ChromeDriver was not started";
  return [
    "ChromeDriver stdout:",
    observation.stdout.text() || "<empty>",
    "ChromeDriver stderr:",
    observation.stderr.text() || "<empty>",
  ].join("\n");
}

async function reserveLoopbackPort() {
  const reservation = createTcpServer();
  await new Promise((resolveListen, rejectListen) => {
    reservation.once("error", rejectListen);
    reservation.listen(0, LOOPBACK, resolveListen);
  });
  const address = reservation.address();
  if (address === null || typeof address === "string") {
    reservation.close();
    fail("ChromeDriver port reservation has no TCP address");
  }
  await new Promise((resolveClose, rejectClose) => {
    reservation.close((error) => error ? rejectClose(error) : resolveClose());
  });
  return address.port;
}

async function waitForDriver(signal, child, observation, port, basePath, timeout) {
  const deadline = Date.now() + timeout;
  const startMarker = new RegExp(
    `ChromeDriver was started successfully on port ${port}\\.?`,
    "u",
  );
  while (Date.now() < deadline) {
    if (signal.aborted) throw signal.reason;
    if (observation.spawnError()) {
      fail("ChromeDriver failed to spawn", { cause: observation.spawnError() });
    }
    if (child.exitCode !== null || child.signalCode !== null) {
      fail(`ChromeDriver exited before readiness: code=${child.exitCode} signal=${child.signalCode}`);
    }
    const startedByThisChild = startMarker.test(observation.stdout.text())
      || startMarker.test(observation.stderr.text());
    if (startedByThisChild) {
      try {
        const probeSignal = AbortSignal.any([signal, AbortSignal.timeout(500)]);
        const response = await fetch(`http://${LOOPBACK}:${port}${basePath}/status`, {
          signal: probeSignal,
        });
        const payload = await response.json();
        if (
          response.ok
          && payload?.value?.ready === true
          && child.exitCode === null
          && child.signalCode === null
          && !observation.spawnError()
        ) {
          return;
        }
      } catch (error) {
        if (signal.aborted) throw error;
      }
    }
    await new Promise((resolveDelay) => setTimeout(resolveDelay, 25));
  }
  fail("ChromeDriver did not become ready");
}

function errorText(error) {
  return error instanceof Error ? error.stack ?? error.message : String(error);
}

export async function launchDriver(driver, signal, dependencies = {}) {
  const reservePort = dependencies.reservePort ?? reserveLoopbackPort;
  const spawnDriver = dependencies.spawnDriver ?? spawn;
  const randomToken = dependencies.randomToken
    ?? (() => randomBytes(16).toString("hex"));
  const attempts = dependencies.attempts ?? DRIVER_START_ATTEMPTS;
  const attemptTimeout = dependencies.attemptTimeout
    ?? DRIVER_START_ATTEMPT_TIMEOUT_MS;
  if (!Number.isSafeInteger(attempts) || attempts < 1) {
    fail("ChromeDriver start attempts must be a positive safe integer");
  }
  if (!Number.isSafeInteger(attemptTimeout) || attemptTimeout < 1) {
    fail("ChromeDriver attempt timeout must be a positive safe integer");
  }

  const failures = [];
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    if (signal.aborted) throw signal.reason;
    const port = await reservePort();
    if (!Number.isSafeInteger(port) || port < 1 || port > 65_535) {
      fail(`ChromeDriver port reservation returned invalid port ${port}`);
    }
    const token = randomToken();
    if (!/^[0-9a-f]{32}$/u.test(token)) {
      fail("ChromeDriver URL-base token must be 128-bit lowercase hex");
    }
    const basePath = `/labcolors-${token}`;
    const child = spawnDriver(
      driver,
      [
        `--port=${port}`,
        `--url-base=${basePath}`,
      ],
      {
        env: process.env,
        stdio: ["ignore", "pipe", "pipe"],
        windowsHide: true,
      },
    );
    const observation = observeDriver(child);
    try {
      await waitForDriver(
        signal,
        child,
        observation,
        port,
        basePath,
        attemptTimeout,
      );
      return {
        child,
        observation,
        origin: `http://${LOOPBACK}:${port}${basePath}`,
      };
    } catch (error) {
      const attemptErrors = [error];
      try {
        await stopDriver(child, observation);
      } catch (cleanupError) {
        attemptErrors.push(cleanupError);
      }
      failures.push(
        `attempt ${attempt}/${attempts} on port ${port}:\n`
        + `${attemptErrors.map(errorText).join("\n")}\n`
        + driverDiagnostics(observation),
      );
    }
  }
  fail(`ChromeDriver exhausted ${attempts} bounded start attempts:\n${failures.join("\n")}`);
}

async function settlesWithin(promise, timeout) {
  const expired = Symbol("expired");
  let timer;
  try {
    const outcome = await Promise.race([
      promise,
      new Promise((resolveTimeout) => {
        timer = setTimeout(resolveTimeout, timeout, expired);
      }),
    ]);
    return outcome !== expired;
  } finally {
    clearTimeout(timer);
  }
}

export async function stopDriver(child, observation) {
  if (child.exitCode === null && child.signalCode === null) child.kill("SIGTERM");
  if (await settlesWithin(observation.closed, DRIVER_STOP_TIMEOUT_MS)) return;
  child.kill("SIGKILL");
  if (!(await settlesWithin(observation.closed, DRIVER_STOP_TIMEOUT_MS))) {
    fail("ChromeDriver did not exit after SIGKILL");
  }
}

async function closeServer(server) {
  if (!server?.listening) return;
  await new Promise((resolveClose, rejectClose) => {
    server.close((error) => error ? rejectClose(error) : resolveClose());
    server.closeAllConnections();
  });
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
  let child;
  let observation;
  let controller;
  let timer;
  let driverOrigin;
  let sessionId;
  let primaryError;
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
          "content-type": pathname.endsWith(".wasm")
            ? "application/wasm"
            : "text/javascript; charset=utf-8",
          "x-content-type-options": "nosniff",
        });
        res.end(bytes);
        return;
      }
      res.writeHead(404).end();
    });
    const port = await listen(server);
    const origin = `http://${LOOPBACK}:${port}`;

    controller = new AbortController();
    timer = setTimeout(
      () => controller.abort(new Error("browser proof timed out")),
      timeout,
    );
    ({ child, observation, origin: driverOrigin } = await launchDriver(
      driver,
      controller.signal,
    ));
    const session = await request(driverOrigin, "/session", "POST", {
      capabilities: { alwaysMatch: { browserName: "chrome", "goog:chromeOptions": {
        binary: chrome,
        args: ["--headless=new", "--no-sandbox", "--disable-dev-shm-usage"],
      } } },
    }, controller.signal);
    if (typeof session?.sessionId !== "string" || session.sessionId.length === 0) {
      fail("ChromeDriver returned no session identity");
    }
    sessionId = session.sessionId;
    const base = `${driverOrigin}/session/${sessionId}`;
    await request(base, "/url", "POST", { url: origin }, controller.signal);
    const script = `
      const done = arguments[arguments.length - 1];
      (async () => {
        const api = await import(${JSON.stringify(`${origin}/index.js`)});
        await api.init({ module_or_path: fetch(${JSON.stringify(`${origin}/pkg/labcolors_bg.wasm`)}) });
        const wire = Uint8Array.from(
          ${JSON.stringify(REFERENCE_WIRE_HEX)}.match(/../gu),
          (octet) => Number.parseInt(octet, 16),
        );
        let readyRuntime;
        let readySnapshot;
        let retryRuntime;
        let retrySnapshot;
        try {
          readyRuntime = api.compileProgramWire(wire, 1);
          readySnapshot = readyRuntime.updateObserved(
            1n,
            new Uint32Array([1]),
            new Uint8Array([255, 255, 255]),
            1,
          );

          retryRuntime = api.compileProgramWire(wire, 7);
          let invalidRejected = false;
          try {
            const invalidSnapshot = retryRuntime.updateObserved(
              1n,
              new Uint32Array([1]),
              new Uint8Array([255, 255]),
              1,
            );
            invalidSnapshot.free();
          } catch {
            invalidRejected = true;
          }
          retrySnapshot = retryRuntime.updateObserved(
            1n,
            new Uint32Array([1]),
            new Uint8Array([255, 255, 255]),
            1,
          );
          const project = (snapshot) => ({
            state: snapshot.state,
            count: snapshot.outputCount(),
            slot: snapshot.outputSlot(0),
            rgb: Array.from(snapshot.outputRgb(0)),
            opacity: snapshot.outputOpacity(0),
          });
          return {
            ready: project(readySnapshot),
            invalidRejected,
            recovered: project(retrySnapshot),
          };
        } finally {
          retrySnapshot?.free();
          retryRuntime?.free();
          readySnapshot?.free();
          readyRuntime?.free();
        }
      })().then(done, (error) => done({ browserError: String(error?.stack ?? error) }));`;
    const result = await request(base, "/execute/async", "POST", {
      script,
      args: [],
    }, controller.signal);
    const expectedSnapshot = {
      state: "ready",
      count: 1,
      slot: 91,
      rgb: [20, 20, 20],
      opacity: 1,
    };
    const expected = {
      ready: expectedSnapshot,
      invalidRejected: true,
      recovered: expectedSnapshot,
    };
    assertExactProgramResult(result, expected);
    await request(base, "", "DELETE", undefined, controller.signal);
    sessionId = undefined;
  } catch (error) {
    primaryError = error;
  } finally {
    clearTimeout(timer);
    const cleanupErrors = [];
    if (sessionId && driverOrigin) {
      try {
        await request(
          `${driverOrigin}/session/${sessionId}`,
          "",
          "DELETE",
          undefined,
          AbortSignal.timeout(DRIVER_STOP_TIMEOUT_MS),
        );
      } catch (error) {
        cleanupErrors.push(error);
      }
    }
    if (child && observation) {
      try {
        await stopDriver(child, observation);
      } catch (error) {
        cleanupErrors.push(error);
      }
    }
    try {
      await closeServer(server);
    } catch (error) {
      cleanupErrors.push(error);
    }
    try {
      await rm(root, { recursive: true });
    } catch (error) {
      cleanupErrors.push(error);
    }
    if (primaryError || cleanupErrors.length > 0) {
      const failures = [primaryError, ...cleanupErrors]
        .filter(Boolean)
        .map((error) => error instanceof Error ? error.stack : String(error));
      fail(`${failures.join("\n")}\n${driverDiagnostics(observation)}`);
    }
  }

  console.log(`LAB_COLORS_PROGRAM_BROWSER_PASS sha256=${digest}`);
}

if (
  process.argv[1]
  && import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.stack : String(error));
    process.exitCode = 1;
  });
}
