import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  observeChildErrors,
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

  await assert.rejects(
    waitForDriver(AbortSignal.timeout(1_000), 1, child, errors),
    (error) => error?.code === "ENOENT" && error.path === missingExecutable,
  );
  errors.release();
});

test("child termination observes a synchronous exit emitted by kill", async () => {
  const listeners = new Map();
  const child = {
    exitCode: null,
    signalCode: null,
    once: (event, listener) => listeners.set(event, listener),
    removeListener: (event) => listeners.delete(event),
    on: (event, listener) => listeners.set(event, listener),
    kill: () => {
      child.signalCode = "SIGTERM";
      listeners.get("exit")?.(null, "SIGTERM");
      return true;
    },
  };
  await assert.doesNotReject(terminateChild(child, 100));
});

test("child termination force-kills and reaps a real graceful-signal survivor", async () => {
  const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], { stdio: "ignore" });
  await once(child, "spawn");
  const nativeKill = child.kill.bind(child);
  const signals = [];
  child.kill = (signal) => {
    signals.push(signal);
    return signal === "SIGTERM" ? true : nativeKill(signal);
  };

  await assert.doesNotReject(terminateChild(child, 20));

  assert.deepEqual(signals, ["SIGTERM", "SIGKILL"]);
  assert.notEqual(child.signalCode, null);
});

test("signaled child is rejected before readiness polling", async () => {
  const child = { killed: false, exitCode: null, signalCode: "SIGTERM" };
  const errors = { failure: new Promise(() => {}) };
  await assert.rejects(
    waitForDriver(AbortSignal.timeout(1_000), 1, child, errors),
    /exited prematurely/u,
  );
});

test("real child exit cancels and joins an in-flight readiness poll", async () => {
  const server = createServer(() => {});
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  assert.notEqual(address, null);
  assert.equal(typeof address, "object");
  const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], { stdio: "ignore" });
  await once(child, "spawn");
  const errors = observeChildErrors(child);
  const outer = new AbortController();
  const waiting = waitForDriver(outer.signal, address.port, child, errors);
  setTimeout(() => child.kill("SIGTERM"), 20);
  try {
    await assert.rejects(
      Promise.race([
        waiting,
        new Promise((_, reject) => setTimeout(() => reject(new Error("readiness poll was not cancelled")), 500)),
      ]),
      /exited prematurely/u,
    );
    assert.equal(outer.signal.aborted, false);
    assert.notEqual(child.signalCode, null);
  } finally {
    errors.release();
    if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
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
  const child = { killed: false, exitCode: null, signalCode: null };
  const errors = { failure: new Promise(() => {}) };
  try {
    await assert.rejects(
      waitForDriver(AbortSignal.timeout(100), address.port, child, errors),
      /timeout|aborted/u,
    );
  } finally {
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
});

test("child termination waits for exit and refuses a process that stays alive", async () => {
  const listeners = new Map();
  const child = {
    exitCode: null,
    signalCode: null,
    once: (event, listener) => listeners.set(event, listener),
    removeListener: (event) => listeners.delete(event),
    kill: () => true,
  };
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
