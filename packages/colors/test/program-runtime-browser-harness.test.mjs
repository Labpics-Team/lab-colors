import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { EventEmitter, once } from "node:events";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
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
  cleanupResources,
  verifyCleanupFaultMatrix,
} from "../../../scripts/test-program-runtime-browser.mjs";
import { browserProofInvocation } from "../../../scripts/verify-package-release.mjs";

test("cleanup fault matrix releases each acquired browser-proof resource in reverse order", async () => {
  await assert.doesNotReject(verifyCleanupFaultMatrix);
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
