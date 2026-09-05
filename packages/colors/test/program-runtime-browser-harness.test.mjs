import assert from "node:assert/strict";
import test from "node:test";

import {
  BrowserProofCleanupError,
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
