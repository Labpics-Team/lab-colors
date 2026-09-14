import { createHash } from "node:crypto";
import { execFileSync, spawn } from "node:child_process";
import { access, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";

import { observeChildErrors, releaseChild, waitForDriver } from "./browser-child-lifecycle.mjs";
import { retainImportedRuntimeSnippets } from "./package-runtime-snippets.mjs";

const LOOPBACK = "127.0.0.1";
const RESOURCE_ORDER = [
  "temp-install",
  "browser",
  "browser-session",
  "server",
  "host",
  "runtime",
  "authority",
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

export async function reserveEphemeralPort() {
  const reservation = createServer();
  const port = await listen(reservation);
  await new Promise((accept, reject) => reservation.close((error) => error ? reject(error) : accept()));
  return port;
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
  const runtimeHandles = [];
  const authorityHandles = [];
  let primary, result, element;

  const freeHandles = (handles) => {
    let firstError;
    for (const handle of handles.toReversed()) {
      try {
        if (handle?.__wbg_ptr !== 0) handle.free();
      } catch (error) {
        firstError ??= error;
      }
    }
    if (firstError !== undefined) throw firstError;
  };

  try {
    const api = await import(`${origin}/index.js`);
    const wire = await import(`${origin}/program-wire/abi-v1.js`);
    await api.init({ module_or_path: fetch(`${origin}/pkg/labcolors_bg.wasm`) });

    element = document.getElementById("proof");
    if (!element) throw new Error("proof element is absent");

    const hostFailure = (code, message) => Object.assign(new Error(message), { code });
    const makeHost = (target) => {
      const state = {
        bindingEpoch: null,
        sequence: 0n,
        revision: null,
        patch: [],
        cssText: "",
        rejectNext: false,
        disposed: false,
      };
      const normalizePatch = (patch) => {
        if (!Array.isArray(patch) || patch.length !== 1) {
          throw hostFailure("HOST_PATCH_SCOPE_MISMATCH", "host patch must cover the exact one-output scope");
        }
        const seen = new Set();
        return patch.map((entry) => {
          if (entry?.output !== 91 || entry?.sinkOutput !== 3 || seen.has(entry.sinkOutput)) {
            throw hostFailure("HOST_PATCH_SCOPE_MISMATCH", "host patch scope or output identity drifted");
          }
          seen.add(entry.sinkOutput);
          if (!(entry.source instanceof Uint8Array) || entry.source.length !== 3
            || !Number.isFinite(entry.opacity) || entry.opacity < 0 || entry.opacity > 1) {
            throw hostFailure("HOST_PATCH_INVALID", "host patch payload is invalid");
          }
          return {
            output: entry.output,
            sinkOutput: entry.sinkOutput,
            source: Array.from(entry.source),
            opacity: entry.opacity,
          };
        });
      };
      const samePatch = (left, right) => JSON.stringify(left) === JSON.stringify(right);
      const checkEpoch = (intent) => {
        if (typeof intent.bindingEpoch !== "bigint") {
          throw hostFailure("HOST_STAMP_MISMATCH", "binding epoch must be bigint");
        }
        if (state.bindingEpoch !== null && intent.bindingEpoch !== state.bindingEpoch) {
          throw hostFailure("HOST_STAMP_MISMATCH", "binding epoch changed");
        }
      };
      const checkMutation = (intent) => {
        checkEpoch(intent);
        if (intent.expectedSequence !== state.sequence
          || intent.desiredSequence !== state.sequence + 1n) {
          throw hostFailure("HOST_STAMP_MISMATCH", "mutation sequence mismatch");
        }
        if (typeof intent.revision !== "bigint"
          || (state.revision !== null && intent.revision <= state.revision)) {
          throw hostFailure("HOST_REVISION_MISMATCH", "mutation revision is not monotonic");
        }
      };
      const publish = (intent, patch, cssText) => {
        if (state.rejectNext) {
          state.rejectNext = false;
          throw hostFailure("HOST_REJECTED", "injected host rejection before publication");
        }
        target.style.cssText = cssText;
        state.bindingEpoch ??= intent.bindingEpoch;
        state.sequence = intent.desiredSequence;
        state.revision = intent.revision;
        state.patch = patch;
        state.cssText = target.getAttribute("style") ?? "";
      };
      return {
        rejectNextInstall() { state.rejectNext = true; },
        snapshot() {
          return {
            bindingEpoch: state.bindingEpoch?.toString() ?? null,
            sequence: state.sequence.toString(),
            revision: state.revision?.toString() ?? null,
            patch: state.patch.map((entry) => ({ ...entry, source: [...entry.source] })),
            cssText: target.getAttribute("style") ?? "",
          };
        },
        tryInstall(intent) {
          if (!intent || typeof intent.kind !== "string") {
            throw hostFailure("HOST_INTENT_INVALID", "host intent is invalid");
          }
          if (intent.kind === "set-all") {
            checkMutation(intent);
            const patch = normalizePatch(intent.patch);
            const entry = patch[0];
            const [r, g, b] = entry.source;
            const cssText = `--consumer-color: rgb(${r} ${g} ${b} / ${entry.opacity});`;
            publish(intent, patch, cssText);
            return;
          }
          if (intent.kind === "revoke-all") {
            checkMutation(intent);
            publish(intent, [], "");
            return;
          }
          if (intent.kind === "confirm-exact") {
            checkEpoch(intent);
            const patch = normalizePatch(intent.patch);
            if (intent.publishedSequence !== state.sequence || intent.revision !== state.revision) {
              throw hostFailure("HOST_STAMP_MISMATCH", "confirmation stamp changed");
            }
            if (!samePatch(patch, state.patch)
              || (target.getAttribute("style") ?? "") !== state.cssText) {
              throw hostFailure("HOST_CONFIRMATION_MISMATCH", "confirmed host snapshot drifted");
            }
            return;
          }
          throw hostFailure("HOST_INTENT_INVALID", `unknown host intent ${intent.kind}`);
        },
        free() {
          if (state.disposed) return;
          target.style.cssText = "";
          target.remove();
          state.disposed = true;
        },
      };
    };

    const host = makeHost(element);
    resources.push({
      name: "host",
      release() {
        host.free();
        host.free();
        released(evidence, "host", fault);
      },
    });
    acquisition(evidence, "host", fault);

    const buildWire = (adaptingLuminance = 64) => new wire.ProgramWireBuilderV1()
      .source(11, [64, 64, 64]).fixedTarget(21, 11).surfaceInputPort(31).opacityInput(32, 0.5)
      .solidPaint(41, 21).opacityPaint(42, 41, 32).inputSurface(51, 31)
      .sourceOverOccurrence(61, 42, 51, adaptingLuminance, 0.2, wire.SURROUND_AVERAGE_V1)
      .presentationRoot(71, 61).presentationTarget(71, 61)
      .exactVisibleUnary(true, 81, 61, [160, 160, 160]).output(91, 42)
      .finish();
    const buildConsumedRootWire = () => new wire.ProgramWireBuilderV1()
      .source(1, [64, 64, 64]).fixedTarget(2, 1).surfaceInputPort(3).opacityInput(4, 0.5)
      .solidPaint(5, 2).opacityPaint(6, 5, 4).inputSurface(7, 3)
      .sourceOverOccurrence(8, 6, 7, 64, 0.2, wire.SURROUND_AVERAGE_V1)
      .presentationRoot(9, 8).presentationTarget(9, 8)
      .occurrenceSurface(10, 8)
      .sourceOverOccurrence(11, 6, 10, 64, 0.2, wire.SURROUND_AVERAGE_V1)
      .exactVisibleUnary(true, 12, 11, [160, 160, 160]).output(13, 6)
      .finish();
    const bindings = {
      emissionOutputs: new Uint32Array([91]),
      emissionSinkOutputs: new Uint32Array([3]),
      presentationOutputs: new Uint32Array([91]),
      presentationRoots: new Uint32Array([71]),
      presentationOccurrences: new Uint32Array([61]),
    };
    const scenarioIds = new Uint32Array([1]);
    const whiteBackdrop = new Uint8Array([255, 255, 255]);
    const bytes = buildWire();
    const compiled = api.compileAttachedProgramWire(bytes);
    runtimeHandles.push(compiled);
    const runtime = compiled.attach(1, bindings, host);
    runtimeHandles.push(runtime);
    resources.push({
      name: "runtime",
      release() {
        freeHandles(runtimeHandles);
        released(evidence, "runtime", fault);
      },
    });
    acquisition(evidence, "runtime", fault);

    const update1 = runtime.updateObserved(1n, scenarioIds, whiteBackdrop);
    authorityHandles.push(update1);
    if (update1.state !== "ready" || update1.authorityCount() !== 1) {
      throw new Error("first attached update did not publish exactly one Ready authority");
    }
    const authority1 = update1.takeAuthority(0);
    authorityHandles.push(authority1);
    resources.push({
      name: "authority",
      release() {
        freeHandles(authorityHandles);
        released(evidence, "authority", fault);
      },
    });
    acquisition(evidence, "authority", fault);

    runtime.validateAuthority(authority1);
    const source = Array.from(authority1.sourceRgb());
    const composite = Array.from(authority1.caseComposite(0));
    const independentComposite = source.map((channel) => Math.round(255 + authority1.opacity * (channel - 255)));
    const rgba = (value) => {
      const channels = value.match(/[\d.]+/g)?.map(Number);
      if (!channels || channels.length < 3) throw new Error("computed color was not RGB");
      return [channels[0], channels[1], channels[2], channels[3] ?? 1];
    };
    const computed = rgba(getComputedStyle(element).color);
    const readyHost = host.snapshot();

    const capture = (operation) => {
      try {
        operation();
        return { rejected: false };
      } catch (error) {
        return {
          rejected: api.isProgramError(error),
          code: error?.code,
          operation: error?.operation,
          causeCode: error?.cause?.code,
        };
      }
    };

    host.rejectNextInstall();
    const rejectionBefore = host.snapshot();
    const hostRejection = capture(() => {
      const unexpected = runtime.updateObserved(2n, scenarioIds, whiteBackdrop);
      authorityHandles.push(unexpected);
    });
    const rejectionAfter = host.snapshot();
    let oldAuthorityValidAfterReject = true;
    try { runtime.validateAuthority(authority1); }
    catch { oldAuthorityValidAfterReject = false; }

    const update2 = runtime.updateObserved(2n, scenarioIds, whiteBackdrop);
    authorityHandles.push(update2);
    const authority2 = update2.takeAuthority(0);
    authorityHandles.push(authority2);
    runtime.validateAuthority(authority2);
    const oldAfterRetry = capture(() => runtime.validateAuthority(authority1));

    const foreignEpochHost = makeHost(document.createElement("div"));
    const foreignEpochRuntime = compiled.attach(2, bindings, foreignEpochHost);
    runtimeHandles.push(foreignEpochRuntime);
    const foreignEpoch = capture(() => foreignEpochRuntime.validateAuthority(authority1));

    const sameBytesOwner = api.compileAttachedProgramWire(bytes);
    runtimeHandles.push(sameBytesOwner);
    const foreignOwnerRuntime = sameBytesOwner.attach(3, bindings, makeHost(document.createElement("div")));
    runtimeHandles.push(foreignOwnerRuntime);
    const foreignOwner = capture(() => foreignOwnerRuntime.validateAuthority(authority1));

    const changedIdentityOwner = api.compileAttachedProgramWire(buildWire(65));
    runtimeHandles.push(changedIdentityOwner);
    const changedIdentityRuntime = changedIdentityOwner.attach(4, bindings, makeHost(document.createElement("div")));
    runtimeHandles.push(changedIdentityRuntime);
    const changedIdentity = capture(() => changedIdentityRuntime.validateAuthority(authority1));

    const consumedRoot = capture(() => {
      const unexpected = api.compileAttachedProgramWire(buildConsumedRootWire());
      runtimeHandles.push(unexpected);
    });

    const staleUpdate = runtime.updateUnknown(3n, 77);
    authorityHandles.push(staleUpdate);
    const staleHost = host.snapshot();

    result = {
      ready: {
        state: update1.state,
        authorityCount: 1,
        source,
        composite,
        independentComposite,
        computed,
        cssText: readyHost.cssText,
        host: readyHost,
        publishedRevision: authority1.publishedRevision.toString(),
        sinkSequence: authority1.sinkSequence.toString(),
        sinkBindingEpoch: authority1.sinkBindingEpoch.toString(),
        presentationRoot: authority1.presentationRoot,
        presentationOccurrence: authority1.presentationOccurrence,
        terminalOccurrence: authority1.terminalOccurrence,
        appearanceSurround: authority1.appearanceSurround,
        rendererProvenance: authority1.rendererProvenance,
        caseCount: authority1.caseCount(),
        caseIndex: authority1.caseIndex(0),
      },
      rejection: {
        error: hostRejection,
        before: rejectionBefore,
        after: rejectionAfter,
        oldAuthorityValid: oldAuthorityValidAfterReject,
      },
      retry: {
        state: update2.state,
        newSequence: authority2.sinkSequence.toString(),
        newRevision: authority2.publishedRevision.toString(),
        oldAuthorityError: oldAfterRetry,
      },
      stale: {
        state: staleUpdate.state,
        authorityCount: staleUpdate.authorityCount(),
        host: staleHost,
      },
      foreign: { epoch: foreignEpoch, owner: foreignOwner, identity: changedIdentity },
      consumedRoot,
    };
  } catch (error) {
    primary = error;
  }

  const outcome = browserCleanup(primary, resources.toReversed());
  if (runtimeHandles.length > 0) {
    evidence.readback.runtime = runtimeHandles.every((handle) => handle.__wbg_ptr === 0);
  }
  if (authorityHandles.length > 0) {
    evidence.readback.authority = authorityHandles.every((handle) => handle.__wbg_ptr === 0);
  }
  if (element) {
    evidence.readback.host = !element.isConnected && element.style.cssText === "";
  }
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
    const driverPort = await reserveEphemeralPort();
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

export function verifyBrowserConsumer(result) {
  const equal = isDeepStrictEqual;
  const ready = result.ready;
  const rejection = result.rejection;
  const retry = result.retry;
  const stale = result.stale;
  const foreign = result.foreign;
  const epochIsNonZero = typeof ready?.sinkBindingEpoch === "string" && /^[1-9][0-9]*$/u.test(ready.sinkBindingEpoch);
  if (result.error || result.cleanupError
    || ready?.state !== "ready" || ready.authorityCount !== 1
    || !equal(ready.source, [64, 64, 64])
    || !equal(ready.composite, [160, 160, 160])
    || !equal(ready.independentComposite, [160, 160, 160])
    || equal(ready.source, ready.composite)
    || !equal(ready.computed, [64, 64, 64, 0.5])
    || typeof ready.cssText !== "string" || ready.cssText === ""
    || ready.publishedRevision !== "1" || ready.sinkSequence !== "1" || !epochIsNonZero
    || ready.host?.bindingEpoch !== ready.sinkBindingEpoch
    || ready.host?.sequence !== "1" || ready.host?.revision !== "1"
    || !equal(ready.host?.patch, [{ output: 91, sinkOutput: 3, source: [64, 64, 64], opacity: 0.5 }])
    || ready.presentationRoot !== 71 || ready.presentationOccurrence !== 61 || ready.terminalOccurrence !== 61
    || ready.appearanceSurround !== "average" || ready.rendererProvenance !== "unverified"
    || ready.caseCount !== 1 || ready.caseIndex !== 0
    || rejection?.error?.rejected !== true || rejection.error.code !== "attached_host"
    || rejection.error.operation !== "updateAttachedObserved" || rejection.error.causeCode !== "HOST_REJECTED"
    || !equal(rejection.before, rejection.after) || rejection.oldAuthorityValid !== true
    || retry?.state !== "ready" || retry.newSequence !== "2" || retry.newRevision !== "2"
    || retry.oldAuthorityError?.rejected !== true
    || retry.oldAuthorityError.code !== "attached_published_revision_mismatch"
    || retry.oldAuthorityError.operation !== "validateAttachedAuthority"
    || stale?.state !== "stale" || stale.authorityCount !== 0
    || stale.host?.sequence !== "3" || stale.host?.revision !== "3" || stale.host?.cssText !== ""
    || !equal(stale.host?.patch, [])
    || foreign?.epoch?.code !== "attached_foreign_binding_epoch"
    || foreign.epoch.operation !== "validateAttachedAuthority"
    || foreign?.owner?.code !== "attached_foreign_owner_generation"
    || foreign.owner.operation !== "validateAttachedAuthority"
    || foreign?.identity?.code !== "attached_program_identity_mismatch"
    || foreign.identity.operation !== "validateAttachedAuthority"
    || result.consumedRoot?.rejected !== true
    || result.consumedRoot.code !== "attached_root_consumed_downstream"
    || result.consumedRoot.operation !== "compileAttachedProgramWire") {
    fail(`browser consumer result drifted: ${JSON.stringify(result)}`);
  }
}

async function main() {
  const { tarball, digest } = parseArgs();
  const timeout = positiveIntegerEnv("LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS");
  const [chrome, driver] = await Promise.all([executableEnv("CHROME_PATH"), executableEnv("CHROMEDRIVER_PATH")]);
  const actualDigest = createHash("sha256").update(await readFile(tarball)).digest("hex");
  if (actualDigest !== digest) fail(`tarball digest mismatch: ${actualDigest}`);
  const run = (fault) => runBrowserProof({ tarball, timeout, chrome, driver }, fault);
  const result = await run();
  verifyBrowserConsumer(result);
  if (JSON.stringify(result.acquired) !== JSON.stringify(RESOURCE_ORDER)
    || JSON.stringify(result.released) !== JSON.stringify(RESOURCE_ORDER.toReversed())
    || RESOURCE_ORDER.some((name) => result.readback[name] !== true)) {
    fail(`browser consumer result drifted: ${JSON.stringify(result)}`);
  }
  console.log(`LAB_COLORS_PROGRAM_BROWSER_RESULT ${JSON.stringify(result)}`);
  const reports = await verifyCleanupFaultMatrix(run);
  for (const report of reports) console.log(`LAB_COLORS_PROGRAM_BROWSER_FAULT ${JSON.stringify(report)}`);
  console.log(`LAB_COLORS_PROGRAM_BROWSER_PASS sha256=${digest}`);
}

const invokedDirectly = process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) main().catch((error) => { console.error(error instanceof Error ? error.stack : String(error)); process.exitCode = 1; });
