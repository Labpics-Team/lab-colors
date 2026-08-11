#!/usr/bin/env node

// Package tests read the ignored private Program WASM artifact. This command
// is the explicit prerequisite of the package `test` script: it reuses an
// artifact whose build receipt already verifies against the current Core
// source, and otherwise delegates to the shared hermetic canonical build
// primitive (run-canonical-private-program-build.mjs) — the same child
// boundary the release gate uses. A missing artifact or a failed build fails
// loudly — tests never silently skip their fixture and never assume a
// previous CI step produced the artifact.

import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  PRIVATE_PROGRAM_BUILD_RECEIPT_PATH,
  PRIVATE_PROGRAM_WASM_PATH,
  privateProgramCoreSourceDigest,
  validatePrivateProgramBuildReceipt,
} from "./build-private-program.mjs";
import { runCanonicalPrivateProgramBuild } from "./run-canonical-private-program-build.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const REPO_ROOT = resolve(dirname(SCRIPT_PATH), "..");
const PACKAGE_DIR = resolve(REPO_ROOT, "packages/colors");
const wasmPath = resolve(PACKAGE_DIR, PRIVATE_PROGRAM_WASM_PATH);
const receiptPath = resolve(PACKAGE_DIR, PRIVATE_PROGRAM_BUILD_RECEIPT_PATH);

export async function verifiedArtifactExists() {
  if (!existsSync(wasmPath) || !existsSync(receiptPath)) return false;
  try {
    const receipt = JSON.parse(readFileSync(receiptPath, "utf8"));
    const wasm = readFileSync(wasmPath);
    const source = await privateProgramCoreSourceDigest();
    validatePrivateProgramBuildReceipt(receipt, {
      source,
      wasm,
      requireOptimizer: true,
    });
    return true;
  } catch {
    // A stale, tampered, or raw-contact artifact is rebuilt; it is never
    // silently reused as the test fixture.
    return false;
  }
}

export async function ensurePrivateProgramArtifact() {
  if (await verifiedArtifactExists()) {
    process.stdout.write(
      `private Program artifact already verified: ${PRIVATE_PROGRAM_WASM_PATH}\n`,
    );
    return;
  }
  const { output } = runCanonicalPrivateProgramBuild();
  process.stdout.write(`private Program artifact ensured: ${output}\n`);
}

const invokedDirectly =
  process.argv[1] !== undefined && resolve(process.argv[1]) === SCRIPT_PATH;

if (invokedDirectly) {
  ensurePrivateProgramArtifact().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
    process.exitCode = 1;
  });
}
