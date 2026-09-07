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
  const names = ["temp-install", "browser", "browser-session", "server", "runtime", "snapshot", "host"];
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

test("browser cleanup preserves the primary and releases snapshot, runtime, then host", () => {
  const primary = new Error("browser scenario failed");
  const events = [];
  const outcome = browserCleanup(primary, [
    { name: "snapshot", release: () => { events.push("snapshot"); throw new Error("snapshot free failed"); } },
    { name: "runtime", release: () => events.push("runtime") },
    { name: "host", release: () => events.push("host") },
  ]);
  assert.deepEqual(events, ["snapshot", "runtime", "host"]);
  assert.equal(outcome.error, "Error: browser scenario failed");
  assert.equal(outcome.cleanupError.code, "BROWSER_PROOF_CLEANUP_FAILED");
  assert.deepEqual(outcome.cleanupError.resources, ["snapshot"]);
});

test("driver readiness is bounded only by the caller-owned signal", () => {
  const source = readFileSync(
    new URL("../../../scripts/browser-child-lifecycle.mjs", import.meta.url),
    "utf8",
  );
  assert.doesNotMatch(source, /Date\.now\(\) \+ 20_000|did not become ready/u);
});

test("browser proof grants real-process termination a 2000 ms budget", () => {
  const source = readFileSync(
    new URL("../../../scripts/test-program-runtime-browser.mjs", import.meta.url),
    "utf8",
  );
  assert.match(source, /releaseChild\(child, childErrors, 2_000\)/u);
});

async function executeScenario(fault, mutate = (source) => source) {
  const events = [], handles = [], elements = [];
  const snapshot = () => {
    const value = {
      __wbg_ptr: 1, state: "ready", outputCount: () => 1, outputSlot: () => 91,
      outputRgb: () => [20, 20, 20], outputOpacity: () => 1,
      free() { this.__wbg_ptr = 0; events.push("snapshot"); },
    };
    handles.push(value);
    return value;
  };
  const api = {
    init: async () => {}, isProgramError: (error) => error.code === "program_update",
    compileProgramWire() {
      const value = {
        __wbg_ptr: 1,
        updateObserved(revision) {
          if (revision === 2n) throw Object.assign(new Error("rejected"), { code: "program_update", operation: "updateObserved" });
          return snapshot();
        },
        free() { this.__wbg_ptr = 0; events.push("runtime"); },
      };
      handles.push(value);
      return value;
    },
  };
  const document = {
    body: { append(element) { element.isConnected = true; } },
    createElement() {
      const properties = new Map();
      const element = {
        isConnected: false,
        style: {
          setProperty: (key, value) => properties.set(key, value),
          removeProperty: (key) => properties.delete(key),
          getPropertyValue: (key) => properties.get(key) ?? "",
        },
        remove() { this.isConnected = false; events.push("host"); },
      };
      elements.push(element);
      return element;
    },
  };
  const wire = await import("../program-wire/abi-v1.js");
  const load = async (url) => url.endsWith("/index.js") ? api : wire;
  // Заменяется только транспорт импорта и DOM/WASM окружение; cleanup исполняется из production script.
  const source = mutate(browserScenario("https://proof.invalid", fault)).replaceAll("await import(", "await load(");
  const execute = new Function("load", "document", "getComputedStyle", "fetch", source);
  const envelope = await new Promise((done) => execute(load, document,
    (element) => ({ color: element.style.getPropertyValue("--consumer-color") }), () => undefined, done));
  assert.equal(envelope.error, undefined, "proof errors must not collide with WebDriver value.error");
  return { report: envelope.proof, events, handles, elements };
}

function assertScenario({ report, events, handles, elements }, after, cleanup) {
  const expected = ["runtime", "snapshot", "host"].slice(0, ["runtime", "snapshot", "host"].indexOf(after) + 1);
  assert.deepEqual(report.acquired, expected);
  assert.deepEqual(report.released, expected.toReversed());
  assert.deepEqual(events, expected.toReversed());
  assert.ok(handles.every((handle) => handle.__wbg_ptr === 0));
  assert.ok(elements.every((element) => !element.isConnected));
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

for (const after of ["runtime", "snapshot", "host"]) {
  test(`serialized actual browser acquisition path cleans up after ${after}`, async () => {
    for (const cleanup of [undefined, after]) {
      assertScenario(await executeScenario({ after, cleanup }), after, cleanup);
    }
  });
}

test("serialized normal browser path keeps the computed-color and rejection proof", async () => {
  const { report, events } = await executeScenario();
  assert.equal(report.error, undefined);
  assert.equal(report.cleanupError, undefined);
  assert.equal(report.state, "ready");
  assert.equal(report.slot, 91);
  assert.equal(report.rejected, true);
  assert.deepEqual(report.before, [20, 20, 20, 1]);
  assert.deepEqual(report.after, report.before);
  assert.deepEqual(events, ["host", "snapshot", "runtime"]);
});

for (const [name, before, after] of [
  ["missing release", "snapshot.free();", "void snapshot;"],
  ["wrong order", "resources.toReversed()", "resources"],
  ["primary masking", "browserCleanup(primary, resources.toReversed())", "browserCleanup(undefined, resources.toReversed())"],
]) {
  test(`actual browser path oracle kills ${name} sabotage`, async () => {
    const execution = await executeScenario({ after: "host", cleanup: "host" }, (source) => {
      assert.ok(source.includes(before), "sabotage must reach the serialized execution path");
      return source.replace(before, after);
    });
    assert.throws(() => assertScenario(execution, "host", "host"), assert.AssertionError);
  });
}

test("actual temp-install boundary removes its real directory and preserves combined failure", async () => {
  for (const cleanup of [undefined, "temp-install"]) {
    const report = await runBrowserProof({}, { after: "temp-install", cleanup });
    assert.deepEqual(report.acquired, ["temp-install"]);
    assert.deepEqual(report.released, ["temp-install"]);
    assert.equal(report.readback["temp-install"], true);
    assert.equal(report.error, "Error: fault after temp-install");
    assert.equal(report.code, "BROWSER_PROOF_INJECTED");
    if (cleanup) {
      assert.equal(report.cleanupError.code, "BROWSER_PROOF_CLEANUP_FAILED");
      assert.deepEqual(report.cleanupError.resources, [cleanup]);
      assert.equal(report.cleanupError.errors[0].code, "BROWSER_PROOF_INJECTED_CLEANUP");
    } else assert.equal(report.cleanupError, undefined);
  }
});

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
