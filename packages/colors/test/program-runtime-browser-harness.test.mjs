import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { EventEmitter, once } from "node:events";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

import {
  observeChildErrors,
  releaseChild,
  terminateChild,
  waitForDriver,
} from "../../../scripts/browser-child-lifecycle.mjs";
import {
  BrowserProofCleanupError,
  browserCleanup,
  browserScenario,
  cleanupResources,
  packedBrowserFiles,
  runBrowserProof,
  reserveEphemeralPort,
  verifyBrowserConsumer,
  verifyCleanupFaultMatrix,
} from "../../../scripts/test-program-runtime-browser.mjs";
import { browserProofInvocation } from "../../../scripts/verify-package-release.mjs";

test("packed browser files include only the runtime's exact generated snippet", async () => {
  const installed = mkdtempSync(join(tmpdir(), "labcolors-browser-files-"));
  try {
    const snippet = "snippets/labcolors-wasm-0123456789abcdef/inline0.js";
    for (const file of ["index.js", "program-wire/abi-v1.js", "pkg/labcolors_bg.wasm", `pkg/${snippet}`]) {
      const destination = join(installed, file);
      mkdirSync(dirname(destination), { recursive: true });
      writeFileSync(destination, "fixture\n");
    }
    writeFileSync(join(installed, "pkg", "labcolors.js"), `import "./${snippet}";\n`);
    writeFileSync(join(installed, "pkg", "unexpected.js"), "must not be served\n");
    const files = await packedBrowserFiles(installed);
    assert.deepEqual([...files.keys()].sort(), [
      "/index.js", "/pkg/labcolors.js", "/pkg/labcolors_bg.wasm",
      `/pkg/${snippet}`, "/program-wire/abi-v1.js",
    ].sort());
  } finally {
    rmSync(installed, { recursive: true, force: true });
  }
});

test("cleanup fault matrix requires actual-path evidence, not a synthetic release list", async () => {
  let invoked = 0;
  await assert.rejects(
    verifyCleanupFaultMatrix(async () => { invoked += 1; return {}; }),
    /fault|evidence/iu,
  );
  assert.equal(invoked, 1);
});

test("fault matrix demands all seven boundaries, real readback and typed secondary failure", async () => {
  const names = ["temp-install", "browser", "browser-session", "server", "host", "runtime", "authority"];
  const reports = await verifyCleanupFaultMatrix(async ({ after, cleanup }) => {
    const acquired = names.slice(0, names.indexOf(after) + 1);
    return {
      acquired, released: acquired.toReversed(), readback: Object.fromEntries(acquired.map((name) => [name, true])),
      error: `Error: fault after ${after}`, code: "BROWSER_PROOF_INJECTED", operation: after,
      ...(cleanup ? { cleanupError: {
        code: "BROWSER_PROOF_CLEANUP_FAILED", resources: [cleanup], errors: [{ code: "BROWSER_PROOF_INJECTED_CLEANUP" }],
      } } : {}),
    };
  });
  assert.equal(reports.length, 14);
  for (const damage of [
    (report) => { report.released.pop(); },
    (report) => { report.readback["temp-install"] = false; },
    (report) => { delete report.error; },
    (report) => { report.cleanupError = { code: "untyped" }; },
  ]) {
    let index = 0;
    await assert.rejects(verifyCleanupFaultMatrix(async () => {
      const report = structuredClone(reports[index++]);
      damage(report);
      return report;
    }), /fault.*evidence/u);
  }
});

test("cleanup failure preserves the primary failure and exposes typed cleanup errors", async () => {
  const primary = new Error("primary browser failure");
  const cleanupFailure = new Error("browser close failure");
  const events = [];

  await assert.rejects(
    cleanupResources(
      [
        { name: "temp-install", release: async () => events.push("temp-install") },
        { name: "browser", release: async () => { events.push("browser"); throw cleanupFailure; } },
        { name: "server", release: async () => events.push("server") },
      ],
      primary,
    ),
    (error) => {
      assert.ok(error instanceof BrowserProofCleanupError);
      assert.equal(error.code, "BROWSER_PROOF_CLEANUP_FAILED");
      assert.equal(error.cause, primary);
      assert.deepEqual(error.cleanupErrors, [{ resource: "browser", error: cleanupFailure }]);
      return true;
    },
  );
  assert.deepEqual(events, ["server", "browser", "temp-install"]);
});

test("browser cleanup preserves the primary and releases authority, runtime, then host", () => {
  const primary = new Error("browser scenario failed");
  const events = [];
  const outcome = browserCleanup(primary, [
    { name: "authority", release: () => { events.push("authority"); throw new Error("authority free failed"); } },
    { name: "runtime", release: () => events.push("runtime") },
    { name: "host", release: () => events.push("host") },
  ]);
  assert.deepEqual(events, ["authority", "runtime", "host"]);
  assert.equal(outcome.error, "Error: browser scenario failed");
  assert.equal(outcome.cleanupError.code, "BROWSER_PROOF_CLEANUP_FAILED");
  assert.deepEqual(outcome.cleanupError.resources, ["authority"]);
});

test("driver readiness is bounded only by the caller-owned signal", () => {
  const source = readFileSync(
    new URL("../../../scripts/browser-child-lifecycle.mjs", import.meta.url),
    "utf8",
  );
  assert.doesNotMatch(source, /Date\.now\(\) \+ 20_000|did not become ready/u);
});

test("driver reservation avoids occupied 9515 and releases its port before spawn", async () => {
  const occupied = createServer();
  await new Promise((resolve, reject) => {
    occupied.once("error", reject);
    occupied.listen(9515, "127.0.0.1", resolve);
  }).catch((error) => {
    if (error.code !== "EADDRINUSE") throw error;
  });
  const ownsListener = occupied.listening;
  const probe = createServer();
  try {
    const port = await reserveEphemeralPort();
    assert.notEqual(port, 9515);
    await new Promise((resolve, reject) => {
      probe.once("error", reject);
      probe.listen(port, "127.0.0.1", resolve);
    });
    assert.equal(occupied.listening, ownsListener);
    const source = readFileSync(new URL("../../../scripts/test-program-runtime-browser.mjs", import.meta.url), "utf8");
    assert.match(source, /const driverPort = await reserveEphemeralPort\(\)/u);
  } finally {
    await Promise.all([occupied, probe].filter((server) => server.listening).map((server) =>
      new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()))));
  }
});

test("browser proof grants real-process termination a 2000 ms budget", () => {
  const source = readFileSync(
    new URL("../../../scripts/test-program-runtime-browser.mjs", import.meta.url),
    "utf8",
  );
  assert.match(source, /releaseChild\(child, childErrors, 2_000\)/u);
});

async function executeScenario(fault, mutate = (source) => source) {
  const events = [], handles = [], elements = [], updates = [];
  let compileCount = 0;
  let nextEpoch = 100n;

  const typedError = (code, operation, cause) => Object.assign(new Error(code), {
    code, operation, ...(cause === undefined ? {} : { cause }),
  });
  const makeAuthority = ({ owner, identity, epoch, revision, sequence }) => {
    const value = {
      __wbg_ptr: 1,
      owner, identity, epoch, revision, sequence,
      publishedRevision: revision,
      sinkSequence: sequence,
      sinkBindingEpoch: epoch,
      presentationRoot: 71,
      presentationOccurrence: 61,
      terminalOccurrence: 61,
      opacity: 0.5,
      appearanceSurround: "average",
      rendererProvenance: "unverified",
      sourceRgb: () => new Uint8Array([64, 64, 64]),
      caseCount: () => 1,
      caseIndex: (index) => index === 0 ? 0 : undefined,
      caseComposite: (index) => index === 0 ? new Uint8Array([160, 160, 160]) : new Uint8Array(),
      free() { this.__wbg_ptr = 0; events.push("authority-handle"); },
    };
    handles.push(value);
    return value;
  };
  const makeUpdate = (state, authority) => {
    let current = authority;
    const value = {
      __wbg_ptr: 1,
      state,
      authorityCount: () => current === undefined ? 0 : 1,
      takeAuthority(index) {
        if (index !== 0 || current === undefined) {
          throw typedError("attached_authority_unavailable", "takeAttachedAuthority");
        }
        const taken = current;
        current = undefined;
        return taken;
      },
      free() { this.__wbg_ptr = 0; events.push("update-handle"); },
    };
    handles.push(value);
    return value;
  };
  const makeCompiled = (owner, identity) => {
    const compiled = {
      __wbg_ptr: 1,
      attach(_streamId, _bindings, host) {
        const epoch = ++nextEpoch;
        let revision = 0n;
        let sequence = 0n;
        const runtime = {
          __wbg_ptr: 1,
          updateObserved(nextRevision, ids, surfaces) {
            updates.push({ kind: "observed", revision: nextRevision, ids: Array.from(ids), surfaces: Array.from(surfaces) });
            if (ids.length !== 1 || surfaces.length !== 3 || nextRevision <= revision) {
              throw typedError("attached_update", "updateAttachedObserved");
            }
            const intent = {
              kind: "set-all", revision: nextRevision, bindingEpoch: epoch,
              expectedSequence: sequence, desiredSequence: sequence + 1n,
              patch: [{ output: 91, sinkOutput: 3, source: new Uint8Array([64, 64, 64]), opacity: 0.5 }],
            };
            try { host.tryInstall(intent); }
            catch (cause) { throw typedError("attached_host", "updateAttachedObserved", cause); }
            revision = nextRevision;
            sequence += 1n;
            return makeUpdate("ready", makeAuthority({ owner, identity, epoch, revision, sequence }));
          },
          updateUnknown(nextRevision, reasonId) {
            updates.push({ kind: "unknown", revision: nextRevision, reasonId });
            if (nextRevision <= revision) throw typedError("attached_update", "updateAttachedUnknown");
            const intent = {
              kind: "revoke-all", revision: nextRevision, bindingEpoch: epoch,
              expectedSequence: sequence, desiredSequence: sequence + 1n,
            };
            try { host.tryInstall(intent); }
            catch (cause) { throw typedError("attached_host", "updateAttachedUnknown", cause); }
            revision = nextRevision;
            sequence += 1n;
            return makeUpdate("stale");
          },
          validateAuthority(authority) {
            if (authority.identity !== identity) {
              throw typedError("attached_program_identity_mismatch", "validateAttachedAuthority");
            }
            if (authority.owner !== owner) {
              throw typedError("attached_foreign_owner_generation", "validateAttachedAuthority");
            }
            if (authority.epoch !== epoch) {
              throw typedError("attached_foreign_binding_epoch", "validateAttachedAuthority");
            }
            if (authority.revision !== revision || authority.sequence !== sequence) {
              throw typedError("attached_published_revision_mismatch", "validateAttachedAuthority");
            }
          },
          free() { this.__wbg_ptr = 0; events.push("runtime-handle"); },
        };
        handles.push(runtime);
        return runtime;
      },
      free() { this.__wbg_ptr = 0; events.push("compiled-handle"); },
    };
    handles.push(compiled);
    return compiled;
  };

  const api = {
    init: async () => {},
    isProgramError: (error) => error instanceof Error
      && typeof error.code === "string" && error.code.startsWith("attached_")
      && typeof error.operation === "string",
    compileAttachedProgramWire() {
      compileCount += 1;
      if (compileCount === 4) {
        throw typedError("attached_root_consumed_downstream", "compileAttachedProgramWire");
      }
      if (compileCount === 1) return makeCompiled(1, "identity-a");
      if (compileCount === 2) return makeCompiled(2, "identity-a");
      return makeCompiled(3, "identity-b");
    },
  };

  const makeElement = (connected = false) => {
    let cssText = "";
    const element = {
      isConnected: connected,
      style: {
        get cssText() { return cssText; },
        set cssText(value) { cssText = String(value); },
      },
      getAttribute(name) { return name === "style" ? cssText : null; },
      remove() { this.isConnected = false; events.push("host"); },
    };
    elements.push(element);
    return element;
  };
  const proofElement = makeElement(true);
  const document = {
    getElementById(id) { return id === "proof" ? proofElement : null; },
    createElement() { return makeElement(false); },
  };
  const wire = await import("../program-wire/abi-v1.js");
  const load = async (url) => url.endsWith("/index.js") ? api : wire;
  const source = mutate(browserScenario("https://proof.invalid", fault)).replaceAll("await import(", "await load(");
  const execute = new Function("load", "document", "getComputedStyle", "fetch", source);
  const envelope = await new Promise((done) => execute(
    load,
    document,
    (element) => {
      const match = element.style.cssText.match(/rgb\((\d+) (\d+) (\d+) \/ ([\d.]+)\)/u);
      return { color: match ? `rgba(${match[1]}, ${match[2]}, ${match[3]}, ${match[4]})` : "rgba(0, 0, 0, 0)" };
    },
    () => undefined,
    done,
  ));
  assert.equal(envelope.error, undefined, "proof errors must not collide with WebDriver value.error");
  return { report: envelope.proof, events, handles, elements, updates };
}

function assertScenario({ report, handles, elements }, after, cleanup) {
  const order = ["host", "runtime", "authority"];
  const expected = order.slice(0, order.indexOf(after) + 1);
  assert.deepEqual(report.acquired, expected);
  assert.deepEqual(report.released, expected.toReversed());
  assert.ok(handles.every((handle) => handle.__wbg_ptr === 0));
  assert.ok(elements.filter((element) => element.isConnected).length === 0);
  assert.ok(expected.every((name) => report.readback[name] === true));
  assert.equal(report.error, `Error: fault after ${after}`);
  assert.equal(report.code, "BROWSER_PROOF_INJECTED");
  assert.equal(report.operation, after);
  if (cleanup) {
    assert.equal(report.cleanupError.code, "BROWSER_PROOF_CLEANUP_FAILED");
    assert.deepEqual(report.cleanupError.resources, [cleanup]);
    assert.equal(report.cleanupError.errors[0].code, "BROWSER_PROOF_INJECTED_CLEANUP");
  } else assert.equal(report.cleanupError, undefined);
}

for (const after of ["host", "runtime", "authority"]) {
  test(`serialized actual browser acquisition path cleans up after ${after}`, async () => {
    for (const cleanup of [undefined, after]) {
      assertScenario(await executeScenario({ after, cleanup }), after, cleanup);
    }
  });
}

test("serialized normal browser path proves attached authority, rollback and invalidation", async () => {
  const { report, handles, elements, updates } = await executeScenario();
  assert.doesNotThrow(() => verifyBrowserConsumer(report));
  assert.ok(handles.every((handle) => handle.__wbg_ptr === 0));
  assert.ok(elements.every((element) => !element.isConnected));
  assert.deepEqual(updates, [
    { kind: "observed", revision: 1n, ids: [1], surfaces: [255, 255, 255] },
    { kind: "observed", revision: 2n, ids: [1], surfaces: [255, 255, 255] },
    { kind: "observed", revision: 2n, ids: [1], surfaces: [255, 255, 255] },
    { kind: "unknown", revision: 3n, reasonId: 77 },
  ]);
});

for (const [name, before, after] of [
  ["error narrowing loss", "rejected: api.isProgramError(error)", "rejected: false"],
  ["independent composite constant", "const independentComposite = source.map((channel) => Math.round(255 + authority1.opacity * (channel - 255)));", "const independentComposite = [0, 0, 0];"],
  ["source substituted for final composite", "const composite = Array.from(authority1.caseComposite(0));", "const composite = [...source];"],
  ["CSS RGB drift", "rgb(${r} ${g} ${b}", "rgb(${r + 1} ${g} ${b}"],
  ["host rejection bypass", "if (state.rejectNext) {", "if (false && state.rejectNext) {"],
  ["published sequence freeze", "state.sequence = intent.desiredSequence;", "state.sequence = intent.expectedSequence;"],
  ["old-authority stale check loss", "const oldAfterRetry = capture(() => runtime.validateAuthority(authority1));", "const oldAfterRetry = { rejected: false };"],
  ["stale revoke loss", 'publish(intent, [], "");', "publish(intent, state.patch, state.cssText);"],
  ["renderer provenance overclaim", "rendererProvenance: authority1.rendererProvenance,", 'rendererProvenance: "observed",'],
  ["foreign identity check loss", "const changedIdentity = capture(() => changedIdentityRuntime.validateAuthority(authority1));", "const changedIdentity = { rejected: false };"],
]) {
  test(`production attached oracle kills ${name} sabotage`, async () => {
    const { report } = await executeScenario(undefined, (source) => {
      assert.ok(source.includes(before), "sabotage must reach the serialized execution path");
      return source.replace(before, after);
    });
    assert.throws(() => verifyBrowserConsumer(report), /browser consumer result drifted/u);
  });
}

for (const [name, before, after] of [
  ["missing authority release", "freeHandles(authorityHandles);", "void authorityHandles;"],
  ["wrong order", "const outcome = browserCleanup(primary, resources.toReversed());", "const outcome = browserCleanup(primary, resources);"],
  ["primary masking", "browserCleanup(primary, resources.toReversed())", "browserCleanup(undefined, resources.toReversed())"],
]) {
  test(`actual attached browser path oracle kills ${name} sabotage`, async () => {
    const execution = await executeScenario({ after: "authority", cleanup: "authority" }, (source) => {
      assert.ok(source.includes(before), "sabotage must reach the serialized execution path");
      return source.replace(before, after);
    });
    const detected = (() => {
      try {
        assertScenario(execution, "authority", "authority");
        return false;
      } catch {
        return true;
      }
    })();
    assert.equal(detected, true);
  });
}

test("driver startup propagates the actual asynchronous child error", async () => {
  const missingExecutable = join(tmpdir(), `missing-chromedriver-${process.pid}`);
  const child = spawn(missingExecutable, [], { stdio: "ignore" });
  const errors = observeChildErrors(child);

  try {
    await assert.rejects(
      waitForDriver(AbortSignal.timeout(1_000), 1, child, errors),
      (error) => error?.code === "ENOENT" && error.path === missingExecutable,
    );
    await errors.close;
  } finally {
    errors.release();
  }
});

function fakeChild(kill) {
  const child = new EventEmitter();
  child.exitCode = null;
  child.signalCode = null;
  child.kill = (signal) => kill(child, signal);
  return child;
}

test("browser release reaps a live child after a post-spawn operational error", { timeout: 10_000 }, async () => {
  const child = spawn(
    process.execPath,
    ["-e", "process.send('ready'); setInterval(() => {}, 1000)"],
    { stdio: ["ignore", "ignore", "ignore", "ipc"] },
  );
  await once(child, "message");
  const errors = observeChildErrors(child);
  const operationalFailure = new Error("post-spawn transport failure");

  try {
    child.emit("error", operationalFailure);
    child.emit("error", new Error("later error must not replace the first"));
    assert.equal(await errors.failure, operationalFailure);
    assert.equal(errors.operationalError, operationalFailure);
    assert.equal(errors.exited, false);
    assert.doesNotThrow(() => process.kill(child.pid, 0));
    const closed = errors.close;
    await assert.rejects(
      releaseChild(child, errors, 1_000),
      (error) => error === operationalFailure,
    );
    await closed;

    assert.equal(errors.closed, true);
    assert.throws(() => process.kill(child.pid, 0), { code: "ESRCH" });
    assert.equal(child.listenerCount("error"), 0);
    assert.equal(child.listenerCount("exit"), 0);
  } finally {
    if (!errors.closed) child.kill("SIGKILL");
    await errors.close;
    errors.release();
  }
});

test("child termination waits for close after exit", async () => {
  let releaseClose;
  const closeReleased = new Promise((resolve) => { releaseClose = resolve; });
  const child = fakeChild((current, signal) => {
    current.signalCode = signal;
    current.emit("exit", null, signal);
    closeReleased.then(() => current.emit("close", null, signal));
    return true;
  });

  let settled = false;
  const termination = terminateChild(child, 100).then(() => { settled = true; });
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(settled, false);
  releaseClose();
  await termination;
});

test("TERM failure still escalates to SIGKILL", async () => {
  const signals = [];
  const termFailure = new Error("TERM failed");
  const child = fakeChild((current, signal) => {
    signals.push(signal);
    if (signal === "SIGTERM") throw termFailure;
    current.signalCode = signal;
    current.emit("exit", null, signal);
    current.emit("close", null, signal);
    return true;
  });

  await assert.doesNotReject(terminateChild(child, 1));
  assert.deepEqual(signals, ["SIGTERM", "SIGKILL"]);
});

test("rejected TERM escalates while rejected SIGKILL requires terminal close", async () => {
  const liveSignals = [];
  const liveChild = fakeChild((_current, signal) => {
    liveSignals.push(signal);
    return false;
  });
  await assert.rejects(terminateChild(liveChild, 1), /rejected SIGKILL/u);
  assert.deepEqual(liveSignals, ["SIGTERM", "SIGKILL"]);

  const terminalSignals = [];
  const terminalChild = fakeChild((current, signal) => {
    terminalSignals.push(signal);
    if (signal === "SIGKILL") {
      current.emit("exit", null, signal);
      current.emit("close", null, signal);
    }
    return false;
  });
  await assert.doesNotReject(terminateChild(terminalChild, 1));
  assert.deepEqual(terminalSignals, ["SIGTERM", "SIGKILL"]);
});

test("concurrent browser release is memoized and waits for close", async () => {
  let releaseClose;
  const closeReleased = new Promise((resolve) => { releaseClose = resolve; });
  const signals = [];
  const child = fakeChild((current, signal) => {
    signals.push(signal);
    current.signalCode = signal;
    current.emit("exit", null, signal);
    closeReleased.then(() => current.emit("close", null, signal));
    return true;
  });
  const errors = observeChildErrors(child);

  const first = releaseChild(child, errors, 100);
  const second = releaseChild(child, errors, 100);
  assert.equal(first, second);
  releaseClose();
  await Promise.all([first, second]);
  assert.deepEqual(signals, ["SIGTERM"]);
  assert.equal(releaseChild(child, errors, 100), first);
});

test("browser release rejects an unconsumed operational error after successful close", async () => {
  const operationalFailure = new Error("unhandled driver transport failure");
  const child = fakeChild((current, signal) => {
    current.signalCode = signal;
    current.emit("exit", null, signal);
    current.emit("close", null, signal);
    return true;
  });
  const errors = observeChildErrors(child);
  child.emit("error", operationalFailure);

  await assert.rejects(
    releaseChild(child, errors, 100),
    (error) => error === operationalFailure,
  );
});

test("browser release preserves ordered distinct operational and cleanup errors", async () => {
  const operationalFailure = new Error("driver transport failed");
  const cleanupFailure = new Error("SIGKILL failed");
  const child = fakeChild((_current, signal) => {
    if (signal === "SIGTERM") return false;
    throw cleanupFailure;
  });
  const errors = observeChildErrors(child);
  child.emit("error", operationalFailure);

  await assert.rejects(
    releaseChild(child, errors, 1),
    (error) => {
      assert.ok(error instanceof AggregateError);
      assert.equal(error.cause, operationalFailure);
      assert.deepEqual(error.errors, [operationalFailure, cleanupFailure]);
      return true;
    },
  );
});

test("child termination force-kills and reaps a real graceful-signal survivor", { timeout: 10_000 }, async () => {
  const child = spawn(
    process.execPath,
    ["-e", "process.send('ready'); setInterval(() => {}, 1000)"],
    { stdio: ["ignore", "ignore", "ignore", "ipc"] },
  );
  await once(child, "message");
  const nativeKill = child.kill.bind(child);
  const closed = once(child, "close");
  const signals = [];
  child.kill = (signal) => {
    signals.push(signal);
    return signal === "SIGTERM" ? true : nativeKill(signal);
  };

  try {
    await assert.doesNotReject(terminateChild(child, 1_000));

    assert.deepEqual(signals, ["SIGTERM", "SIGKILL"]);
    assert.notEqual(child.signalCode, null);
  } finally {
    if (child.exitCode === null && child.signalCode === null) nativeKill("SIGKILL");
    await closed;
  }
});

test("signaled child is rejected before readiness polling", async () => {
  const child = fakeChild(() => true);
  const errors = observeChildErrors(child);
  child.signalCode = "SIGTERM";
  child.emit("exit", null, "SIGTERM");
  try {
    await assert.rejects(
      waitForDriver(AbortSignal.timeout(1_000), 1, child, errors),
      /exited prematurely/u,
    );
  } finally {
    errors.release();
  }
});

test("a handled driver error remains primary while cleanup succeeds", async () => {
  const child = fakeChild((current, signal) => {
    current.signalCode = signal;
    current.emit("exit", null, signal);
    current.emit("close", null, signal);
    return true;
  });
  const errors = observeChildErrors(child);
  const operationalFailure = new Error("handled driver transport failure");
  child.emit("error", operationalFailure);

  await assert.rejects(
    waitForDriver(AbortSignal.timeout(1_000), 1, child, errors),
    (error) => error === operationalFailure,
  );
  await assert.doesNotReject(releaseChild(child, errors, 100));
});

test("post-ready operational error fails the active driver wait", async () => {
  const server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ value: { ready: true } }));
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  assert.notEqual(address, null);
  assert.equal(typeof address, "object");
  const child = fakeChild(() => true);
  const errors = observeChildErrors(child);
  const operationalFailure = new Error("driver transport failed after status response");
  server.once("request", () => queueMicrotask(() => child.emit("error", operationalFailure)));

  try {
    await assert.rejects(
      waitForDriver(AbortSignal.timeout(1_000), address.port, child, errors),
      (error) => error === operationalFailure,
    );
  } finally {
    errors.release();
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
});

test("real child exit aborts an in-flight readiness poll and releases observers", async () => {
  let requestAborted;
  const requestWasAborted = new Promise((resolve) => { requestAborted = resolve; });
  let child;
  const server = createServer((_request, response) => {
    response.once("close", () => requestAborted());
    child.send("exit");
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  assert.notEqual(address, null);
  assert.equal(typeof address, "object");
  child = spawn(
    process.execPath,
    ["-e", "process.on('message', () => process.exit(23)); setInterval(() => {}, 1000)"],
    { stdio: ["ignore", "ignore", "ignore", "ipc"] },
  );
  await once(child, "spawn");
  const errors = observeChildErrors(child);
  const baselineErrorListeners = child.listenerCount("error");
  const baselineExitListeners = child.listenerCount("exit");
  const unhandled = [];
  const onUnhandled = (reason) => unhandled.push(reason);
  process.on("unhandledRejection", onUnhandled);

  try {
    await assert.rejects(
      waitForDriver(AbortSignal.timeout(2_000), address.port, child, errors),
      { message: "Program browser proof: ChromeDriver exited prematurely with code 23" },
    );
    await requestWasAborted;
    await new Promise((resolve) => setImmediate(resolve));

    assert.equal(child.exitCode, 23);
    assert.equal(child.killed, false);
    assert.equal(child.listenerCount("error"), baselineErrorListeners);
    assert.equal(child.listenerCount("exit"), baselineExitListeners);
    assert.deepEqual(unhandled, []);
  } finally {
    process.removeListener("unhandledRejection", onUnhandled);
    if (!errors.closed && child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
    await errors.close;
    errors.release();
    server.closeAllConnections();
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
});

test("HTTP status is accepted only when WebDriver reports ready", async () => {
  const server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ value: { ready: false } }));
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  assert.notEqual(address, null);
  assert.equal(typeof address, "object");
  const child = fakeChild(() => true);
  const errors = observeChildErrors(child);
  try {
    await assert.rejects(
      waitForDriver(AbortSignal.timeout(100), address.port, child, errors),
      /timeout|aborted/u,
    );
  } finally {
    errors.release();
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
});

test("child termination waits for close and refuses a process that stays alive", async () => {
  const child = fakeChild(() => true);
  await assert.rejects(terminateChild(child, 1), /survived forced termination/u);
});

test("release verifier invokes the browser proof with the exact snapshot identity", () => {
  const [script, tarball, digest] = browserProofInvocation("exact.tgz", "a".repeat(64));
  assert.match(script, /scripts[\\/]test-program-runtime-browser\.mjs$/u);
  assert.match(tarball, /[\\/]exact\.tgz$/u);
  assert.equal(digest, "a".repeat(64));
  assert.throws(
    () => browserProofInvocation("/tmp/exact.tgz", "A".repeat(64)),
    /lowercase SHA-256/u,
  );
});
