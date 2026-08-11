#!/usr/bin/env node

import { spawn, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import {
  access,
  lstat,
  mkdtemp,
  mkdir,
  readFile,
  realpath,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import {
  dirname,
  extname,
  isAbsolute,
  join,
  relative,
  resolve,
  sep,
  win32 as windowsPath,
} from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const REPO_ROOT = resolve(dirname(SCRIPT_PATH), "..");
const FIXTURE_ROOT = resolve(REPO_ROOT, "fixtures/private-program-browser");
const LOOPBACK_HOST = "127.0.0.1";
const DRIVER_READY_POLL_MS = 50;
const PROCESS_STOP_TIMEOUT_MS = 1_000;
const DRIVER_LOG_LIMIT_BYTES = 8_192;
const EXPECTED_CHECKS = Object.freeze([
  "installed-physical-private-program",
  "caller-owned-request-literal",
  "pre-run-dispose-idempotence",
  "exact-computed-css",
  "exact-certified-receipt",
  "dispose",
  "post-run-dispose-idempotence",
]);
const PASS_RECEIPT = `LAB_COLORS_PRIVATE_PROGRAM_BROWSER_PASS v1 checks=${EXPECTED_CHECKS.length}`;
export const PRIVATE_PROGRAM_BROWSER_PASS_RECEIPT = PASS_RECEIPT;
const EXTERNAL_FIXTURE_FILES = Object.freeze({
  "/": Object.freeze({ path: "index.html", type: "text/html; charset=utf-8" }),
  "/proof.mjs": Object.freeze({ path: "proof.mjs", type: "text/javascript; charset=utf-8" }),
});
const INSTALLED_PACKAGE_FILES = Object.freeze({
  "/installed/private-program/consumer.js": Object.freeze({
    path: "private-program/consumer.js",
    type: "text/javascript; charset=utf-8",
  }),
  "/installed/private-program/labcolors_private_program.wasm": Object.freeze({
    path: "private-program/labcolors_private_program.wasm",
    type: "application/wasm",
  }),
  "/installed/output-sink.js": Object.freeze({
    path: "output-sink.js",
    type: "text/javascript; charset=utf-8",
  }),
  "/installed/output-bindings.js": Object.freeze({
    path: "output-bindings.js",
    type: "text/javascript; charset=utf-8",
  }),
  "/installed/sequence-identity-matches.js": Object.freeze({
    path: "sequence-identity-matches.js",
    type: "text/javascript; charset=utf-8",
  }),
});

function fail(message, options) {
  throw new Error(`private Program browser proof: ${message}`, options);
}

function parseArguments() {
  const args = process.argv.slice(2);
  if (args.length !== 2) {
    fail("expected exactly <tarball.tgz> <lowercase-64-sha256>");
  }
  const tarball = resolve(args[0]);
  if (extname(tarball) !== ".tgz") fail("the supplied artifact must have a .tgz suffix");
  const expectedSha256 = args[1];
  if (!/^[0-9a-f]{64}$/u.test(expectedSha256)) {
    fail("the supplied tarball SHA-256 must be exactly 64 lowercase hexadecimal characters");
  }
  return { tarball, expectedSha256 };
}

function parseProofTimeout() {
  const raw = process.env.LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS;
  if (!/^[1-9][0-9]*$/u.test(raw ?? "")) {
    fail("LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS must be a positive integer");
  }
  const milliseconds = Number(raw);
  if (!Number.isSafeInteger(milliseconds)) {
    fail("LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS exceeds the safe integer range");
  }
  return milliseconds;
}

async function regularFile(path, label) {
  await access(path);
  const metadata = await stat(path);
  if (!metadata.isFile()) fail(`${label} does not name a regular file`);
  return path;
}

async function executableFromEnv(name) {
  const value = process.env[name];
  if (!value || !isAbsolute(value)) fail(`${name} must name an absolute executable path`);
  return regularFile(value, name);
}

async function bindTarballIdentity(tarball, expectedSha256) {
  const bytes = await readFile(tarball);
  const actualSha256 = createHash("sha256").update(bytes).digest("hex");
  if (actualSha256 !== expectedSha256) {
    fail(`supplied tarball SHA-256 mismatch: expected ${expectedSha256}, got ${actualSha256}`);
  }
  return bytes;
}

function assertStrictlyContained(root, candidate, label) {
  const pathFromRoot = relative(root, candidate);
  if (
    pathFromRoot === "" ||
    pathFromRoot === ".." ||
    pathFromRoot.startsWith(`..${sep}`) ||
    isAbsolute(pathFromRoot)
  ) {
    fail(`${label} escapes its physical root`);
  }
}

function samePhysicalPath(left, right) {
  const normalizedLeft = resolve(left);
  const normalizedRight = resolve(right);
  return process.platform === "win32"
    ? normalizedLeft.toLowerCase() === normalizedRight.toLowerCase()
    : normalizedLeft === normalizedRight;
}

async function assertNoLinkPath(root, relativePath, finalKind, label) {
  let cursor = root;
  const segments = relativePath.split("/");
  for (let index = 0; index < segments.length; index += 1) {
    cursor = resolve(cursor, segments[index]);
    const metadata = await lstat(cursor);
    if (metadata.isSymbolicLink()) fail(`${label} contains a symlink or reparse point`);
    const final = index === segments.length - 1;
    if (final && finalKind === "file" && !metadata.isFile()) {
      fail(`${label} is not a regular file`);
    }
    if ((!final || finalKind === "directory") && !metadata.isDirectory()) {
      fail(`${label} contains a non-directory path segment`);
    }
  }
  return cursor;
}

async function inspectInstalledPackage(externalRoot, installedPackageRoot) {
  const physicalExternalRoot = await realpath(externalRoot);
  const physicalPackagePath = await assertNoLinkPath(
    physicalExternalRoot,
    "node_modules/@labpics/colors",
    "directory",
    "installed package directory",
  );
  const physicalPackageRoot = await realpath(physicalPackagePath);
  assertStrictlyContained(
    physicalExternalRoot,
    physicalPackageRoot,
    "installed package directory",
  );
  if (!samePhysicalPath(installedPackageRoot, physicalPackagePath)) {
    fail("installed package directory differs from the expected external location");
  }
  return physicalPackageRoot;
}

async function readPhysicalInstalledFile(installedPackageRoot, relativePath, label) {
  const lexicalPath = await assertNoLinkPath(
    installedPackageRoot,
    relativePath,
    "file",
    label,
  );
  const physicalPath = await realpath(lexicalPath);
  assertStrictlyContained(installedPackageRoot, physicalPath, label);
  return readFile(physicalPath);
}

function npmInvocation() {
  const lifecycleEntrypoint = process.env.npm_execpath?.trim();
  if (
    lifecycleEntrypoint &&
    (process.platform === "win32"
      ? windowsPath.isAbsolute(lifecycleEntrypoint)
      : isAbsolute(lifecycleEntrypoint)) &&
    /(?:^|[\\/])npm-cli\.(?:c?js|mjs)$/u.test(lifecycleEntrypoint) &&
    existsSync(lifecycleEntrypoint)
  ) {
    return { command: process.execPath, prefix: [lifecycleEntrypoint] };
  }
  if (process.platform === "win32") {
    const entrypoint = windowsPath.resolve(
      windowsPath.dirname(process.execPath),
      "node_modules",
      "npm",
      "bin",
      "npm-cli.js",
    );
    if (!existsSync(entrypoint)) fail(`npm CLI entrypoint is unavailable: ${entrypoint}`);
    return { command: process.execPath, prefix: [entrypoint] };
  }
  return { command: "npm", prefix: [] };
}

function installExactTarball(tarball, externalRoot, timeoutMilliseconds) {
  const invocation = npmInvocation();
  const args = [
    ...invocation.prefix,
    "install",
    "--offline",
    "--ignore-scripts",
    "--no-audit",
    "--no-fund",
    "--no-package-lock",
    "--save=false",
    tarball,
  ];
  try {
    execFileSync(invocation.command, args, {
      cwd: externalRoot,
      stdio: ["ignore", "pipe", "pipe"],
      timeout: timeoutMilliseconds,
      windowsHide: true,
    });
  } catch (error) {
    const details = [error?.stderr, error?.stdout]
      .map((value) => value?.toString("utf8").trim())
      .filter(Boolean)
      .join("\n");
    fail(`offline install of the supplied tarball failed${details ? `:\n${details}` : ""}`, {
      cause: error,
    });
  }
}

function boundedLog(text) {
  return text.length <= DRIVER_LOG_LIMIT_BYTES
    ? text
    : text.slice(text.length - DRIVER_LOG_LIMIT_BYTES);
}

function timeoutSignal(milliseconds, label) {
  const controller = new AbortController();
  const timeout = setTimeout(
    () => controller.abort(new Error(`${label} exceeded ${milliseconds} ms`)),
    milliseconds,
  );
  return { controller, clear: () => clearTimeout(timeout) };
}

async function abortableDelay(milliseconds, signal) {
  if (signal.aborted) throw signal.reason;
  await new Promise((resolveDelay, rejectDelay) => {
    const onAbort = () => {
      clearTimeout(timeout);
      rejectDelay(signal.reason);
    };
    const timeout = setTimeout(() => {
      signal.removeEventListener("abort", onAbort);
      resolveDelay();
    }, milliseconds);
    signal.addEventListener("abort", onAbort, { once: true });
  });
}

function processExited(child) {
  return child.exitCode !== null || child.signalCode !== null;
}

async function waitForProcessExit(child) {
  if (processExited(child)) return true;
  return new Promise((resolveWait) => {
    let settled = false;
    const finish = (exited) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      child.removeListener("exit", onExit);
      resolveWait(exited);
    };
    const onExit = () => finish(true);
    const timeout = setTimeout(() => finish(false), PROCESS_STOP_TIMEOUT_MS);
    child.once("exit", onExit);
    if (processExited(child)) finish(true);
  });
}

async function stopProcess(child) {
  if (processExited(child)) return;
  try {
    if (process.platform !== "win32" && child.pid !== undefined) {
      process.kill(-child.pid, "SIGTERM");
    } else {
      child.kill();
    }
  } catch (error) {
    if (error?.code !== "ESRCH") throw error;
  }
  if (await waitForProcessExit(child)) return;
  try {
    if (process.platform !== "win32" && child.pid !== undefined) {
      process.kill(-child.pid, "SIGKILL");
    } else {
      child.kill("SIGKILL");
    }
  } catch (error) {
    if (error?.code !== "ESRCH") throw error;
  }
  if (!(await waitForProcessExit(child))) fail("ChromeDriver did not terminate");
}

async function startChromeDriver(executable, timeoutMilliseconds, overallSignal) {
  const child = spawn(executable, ["--port=0"], {
    env: process.env,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
    detached: process.platform !== "win32",
  });
  let log = "";
  const startup = timeoutSignal(timeoutMilliseconds, "ChromeDriver startup");
  const signal = AbortSignal.any([startup.controller.signal, overallSignal]);
  try {
    const port = await new Promise((resolvePort, rejectPort) => {
      let settled = false;
      const finish = (callback, value) => {
        if (settled) return;
        settled = true;
        signal.removeEventListener("abort", onAbort);
        callback(value);
      };
      const onAbort = () => finish(rejectPort, signal.reason);
      const inspect = (chunk) => {
        log = boundedLog(log + chunk.toString("utf8"));
        const match = /started successfully on port ([1-9][0-9]*)/iu.exec(log);
        if (match) finish(resolvePort, Number(match[1]));
      };
      child.stdout.on("data", inspect);
      child.stderr.on("data", inspect);
      child.once("error", (error) => finish(rejectPort, error));
      child.once("exit", (code, exitSignal) => {
        finish(
          rejectPort,
          new Error(
            `ChromeDriver exited before readiness (code=${String(code)}, signal=${String(exitSignal)}): ${log}`,
          ),
        );
      });
      signal.addEventListener("abort", onAbort, { once: true });
      if (signal.aborted) onAbort();
    });
    return { child, port, log: () => log };
  } catch (error) {
    await stopProcess(child);
    throw error;
  } finally {
    startup.clear();
  }
}

async function driverRequest(port, path, init, timeoutMilliseconds, overallSignal) {
  const command = timeoutSignal(timeoutMilliseconds, `WebDriver ${init.method} ${path}`);
  const signal = AbortSignal.any([command.controller.signal, overallSignal]);
  try {
    const response = await fetch(`http://${LOOPBACK_HOST}:${port}${path}`, {
      ...init,
      headers: init.body === undefined ? undefined : { "content-type": "application/json" },
      signal,
    });
    const payload = await response.json().catch((cause) => {
      fail(`WebDriver returned non-JSON HTTP ${response.status}`, { cause });
    });
    const protocolError = typeof payload?.value?.error === "string" ? payload.value.error : null;
    if (!response.ok || protocolError !== null) {
      fail(
        `WebDriver rejected ${init.method} ${path}: ${protocolError ?? response.status} ${payload?.value?.message ?? ""}`,
      );
    }
    return payload.value;
  } finally {
    command.clear();
  }
}

async function waitForDriver(port, timeoutMilliseconds, overallSignal) {
  const deadline = Date.now() + timeoutMilliseconds;
  while (Date.now() < deadline) {
    try {
      const value = await driverRequest(
        port,
        "/status",
        { method: "GET" },
        Math.max(1, deadline - Date.now()),
        overallSignal,
      );
      if (value?.ready === true) return;
    } catch (error) {
      if (overallSignal.aborted) throw error;
    }
    await abortableDelay(
      Math.min(DRIVER_READY_POLL_MS, Math.max(1, deadline - Date.now())),
      overallSignal,
    );
  }
  fail("ChromeDriver did not become ready within its startup budget");
}

async function listen(server, overallSignal) {
  await new Promise((resolveListen, rejectListen) => {
    let settled = false;
    const finish = (callback, value) => {
      if (settled) return;
      settled = true;
      server.removeListener("error", onError);
      overallSignal.removeEventListener("abort", onAbort);
      callback(value);
    };
    const onAbort = () => finish(rejectListen, overallSignal.reason);
    const onError = (error) => finish(rejectListen, error);
    server.once("error", onError);
    overallSignal.addEventListener("abort", onAbort, { once: true });
    if (overallSignal.aborted) {
      onAbort();
      return;
    }
    server.listen({ port: 0, host: LOOPBACK_HOST }, () => finish(resolveListen));
  });
  const address = server.address();
  if (address === null || typeof address === "string") fail("proof server has no TCP address");
  return address.port;
}

async function closeServer(server, timeoutMilliseconds = PROCESS_STOP_TIMEOUT_MS) {
  if (!server.listening) return;
  const cleanup = timeoutSignal(timeoutMilliseconds, "proof server cleanup");
  try {
    await new Promise((resolveClose, rejectClose) => {
      const onAbort = () => rejectClose(cleanup.controller.signal.reason);
      cleanup.controller.signal.addEventListener("abort", onAbort, { once: true });
      server.close((error) => {
        cleanup.controller.signal.removeEventListener("abort", onAbort);
        if (error) rejectClose(error);
        else resolveClose();
      });
      server.closeAllConnections();
    });
  } finally {
    cleanup.clear();
  }
}

async function materializeFixture(externalRoot) {
  const fixtureRoot = resolve(externalRoot, "fixture");
  await mkdir(fixtureRoot, { recursive: true });
  await Promise.all(
    Object.values(EXTERNAL_FIXTURE_FILES).map(async ({ path }) => {
      const bytes = await readFile(resolve(FIXTURE_ROOT, path));
      await writeFile(resolve(fixtureRoot, path), bytes);
    }),
  );
  return fixtureRoot;
}

async function loadAllowedFiles(externalFixtureRoot, installedPackageRoot) {
  const entries = [];
  for (const [route, descriptor] of Object.entries(EXTERNAL_FIXTURE_FILES)) {
    entries.push([
      route,
      Object.freeze({ ...descriptor, bytes: await readFile(resolve(externalFixtureRoot, descriptor.path)) }),
    ]);
  }
  for (const [route, descriptor] of Object.entries(INSTALLED_PACKAGE_FILES)) {
    entries.push([
      route,
      Object.freeze({
        ...descriptor,
        bytes: await readPhysicalInstalledFile(
          installedPackageRoot,
          descriptor.path,
          `served installed file ${descriptor.path}`,
        ),
      }),
    ]);
  }
  return new Map(entries);
}

function proofServer(allowedFiles) {
  return createServer((request, response) => {
    const pathname = new URL(request.url ?? "/", `http://${LOOPBACK_HOST}`).pathname;
    const file = allowedFiles.get(pathname);
    if (request.method !== "GET") {
      response.writeHead(405, { allow: "GET" }).end();
      return;
    }
    if (!file) {
      response.writeHead(404).end();
      return;
    }
    const headers = {
      "cache-control": "no-store",
      "content-type": file.type,
      "cross-origin-opener-policy": "same-origin",
      "referrer-policy": "no-referrer",
      "x-content-type-options": "nosniff",
    };
    if (pathname === "/") {
      headers["content-security-policy"] =
        "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; " +
        "style-src 'unsafe-inline'; img-src 'none'; font-src 'none'; object-src 'none'; " +
        "base-uri 'none'; form-action 'none'; frame-ancestors 'none'";
    }
    response.writeHead(200, headers);
    response.end(file.bytes);
  });
}

async function main() {
  const { tarball: suppliedTarball, expectedSha256 } = parseArguments();
  const tarball = await regularFile(suppliedTarball, "supplied npm tarball");
  const verifiedTarballBytes = await bindTarballIdentity(tarball, expectedSha256);
  const timeoutMilliseconds = parseProofTimeout();
  const chromePath = await executableFromEnv("CHROME_PATH");
  const chromeDriverPath = await executableFromEnv("CHROMEDRIVER_PATH");
  const externalRoot = await mkdtemp(join(tmpdir(), "labcolors-private-program-browser-"));
  const overall = timeoutSignal(timeoutMilliseconds, "private Program browser proof");
  let server;
  let driver;
  let driverPort;
  let sessionId;
  let failure;
  try {
    const verifiedTarball = resolve(externalRoot, "verified-package.tgz");
    await writeFile(verifiedTarball, verifiedTarballBytes, { flag: "wx", mode: 0o600 });
    await writeFile(
      resolve(externalRoot, "package.json"),
      `${JSON.stringify({ private: true, type: "module" }, null, 2)}\n`,
    );
    const externalFixtureRoot = await materializeFixture(externalRoot);
    installExactTarball(verifiedTarball, externalRoot, timeoutMilliseconds);
    const installedPackageRoot = resolve(externalRoot, "node_modules/@labpics/colors");
    const physicalInstalledPackageRoot = await inspectInstalledPackage(
      externalRoot,
      installedPackageRoot,
    );
    const installedPackage = JSON.parse(
      (await readPhysicalInstalledFile(
        physicalInstalledPackageRoot,
        "package.json",
        "installed package manifest",
      )).toString("utf8"),
    );
    if (installedPackage.name !== "@labpics/colors") {
      fail(`installed ${String(installedPackage.name)} instead of @labpics/colors`);
    }

    const allowedFiles = await loadAllowedFiles(
      externalFixtureRoot,
      physicalInstalledPackageRoot,
    );
    server = proofServer(allowedFiles);
    const serverPort = await listen(server, overall.controller.signal);
    const started = await startChromeDriver(
      chromeDriverPath,
      Math.max(1, Math.floor(timeoutMilliseconds / 3)),
      overall.controller.signal,
    );
    driver = started.child;
    driverPort = started.port;
    const commandTimeoutMilliseconds = Math.max(1, Math.floor(timeoutMilliseconds / 2));
    await waitForDriver(driverPort, commandTimeoutMilliseconds, overall.controller.signal);

    const session = await driverRequest(
      driverPort,
      "/session",
      {
        method: "POST",
        body: JSON.stringify({
          capabilities: {
            alwaysMatch: {
              browserName: "chrome",
              pageLoadStrategy: "normal",
              "goog:chromeOptions": {
                binary: chromePath,
                args: [
                  "--headless=new",
                  "--disable-background-networking",
                  "--disable-client-side-phishing-detection",
                  "--disable-component-update",
                  "--disable-default-apps",
                  "--disable-dev-shm-usage",
                  "--disable-domain-reliability",
                  "--disable-extensions",
                  "--disable-features=AutofillServerCommunication,CertificateTransparencyComponentUpdater,MediaRouter,OptimizationHints",
                  "--disable-gpu",
                  "--disable-sync",
                  "--metrics-recording-only",
                  "--no-first-run",
                  "--no-sandbox",
                  "--no-proxy-server",
                  "--password-store=basic",
                  "--safebrowsing-disable-auto-update",
                  "--use-mock-keychain",
                  "--host-resolver-rules=EXCLUDE 127.0.0.1, MAP * ~NOTFOUND",
                  `--user-data-dir=${resolve(externalRoot, "chrome-profile")}`,
                  "--window-size=800,600",
                ],
              },
            },
          },
        }),
      },
      commandTimeoutMilliseconds,
      overall.controller.signal,
    );
    sessionId = session?.sessionId;
    if (typeof sessionId !== "string" || sessionId.length === 0) {
      fail("ChromeDriver created no W3C session id");
    }
    await driverRequest(
      driverPort,
      `/session/${sessionId}/timeouts`,
      {
        method: "POST",
        body: JSON.stringify({
          implicit: 0,
          pageLoad: commandTimeoutMilliseconds,
          script: commandTimeoutMilliseconds,
        }),
      },
      commandTimeoutMilliseconds,
      overall.controller.signal,
    );
    const origin = `http://${LOOPBACK_HOST}:${serverPort}`;
    await driverRequest(
      driverPort,
      `/session/${sessionId}/url`,
      { method: "POST", body: JSON.stringify({ url: origin }) },
      commandTimeoutMilliseconds,
      overall.controller.signal,
    );
    const result = await driverRequest(
      driverPort,
      `/session/${sessionId}/execute/async`,
      {
        method: "POST",
        body: JSON.stringify({
          script: `
            const done = arguments[arguments.length - 1];
            Promise.resolve(globalThis.__LAB_COLORS_PRIVATE_PROGRAM_PROOF__).then(
              (value) => done({ ok: true, value }),
              (error) => done({
                ok: false,
                error: {
                  name: error?.name ?? "Error",
                  message: error?.message ?? String(error),
                  stack: error?.stack ?? null,
                },
              }),
            );
          `,
          args: [],
        }),
      },
      commandTimeoutMilliseconds,
      overall.controller.signal,
    );
    if (result?.ok !== true) {
      fail(
        `browser assertion failed: ${result?.error?.name ?? "Error"}: ${result?.error?.message ?? "unknown failure"}\n${result?.error?.stack ?? ""}`,
      );
    }
    if (JSON.stringify(result.value?.checks) !== JSON.stringify(EXPECTED_CHECKS)) {
      fail(`browser returned an incomplete or reordered receipt: ${JSON.stringify(result.value)}`);
    }
    process.stdout.write(`${PASS_RECEIPT}\n`);
  } catch (error) {
    failure = error;
  } finally {
    const cleanupErrors = [];
    if (driverPort && sessionId) {
      const cleanup = timeoutSignal(PROCESS_STOP_TIMEOUT_MS, "WebDriver cleanup");
      try {
        await driverRequest(
          driverPort,
          `/session/${sessionId}`,
          { method: "DELETE" },
          PROCESS_STOP_TIMEOUT_MS,
          cleanup.controller.signal,
        ).catch((error) => cleanupErrors.push(error));
      } finally {
        cleanup.clear();
      }
    }
    if (driver) await stopProcess(driver).catch((error) => cleanupErrors.push(error));
    if (server) await closeServer(server).catch((error) => cleanupErrors.push(error));
    overall.clear();
    await rm(externalRoot, { recursive: true, force: true }).catch((error) => cleanupErrors.push(error));
    if (cleanupErrors.length > 0) {
      failure = new AggregateError(
        failure === undefined ? cleanupErrors : [failure, ...cleanupErrors],
        "private Program browser proof or cleanup failed",
      );
    }
  }
  if (failure !== undefined) throw failure;
}

if (process.argv[1] && resolve(process.argv[1]) === SCRIPT_PATH) {
  main().catch((error) => {
    process.stderr.write(`${error?.stack ?? error}\n`);
    process.exitCode = 1;
  });
}
