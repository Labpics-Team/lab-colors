#!/usr/bin/env node

// Package tests read the ignored private Program WASM artifact. This command
// is the explicit prerequisite of the package `test` script: it reuses an
// artifact whose build receipt already verifies against the current Core
// source, and otherwise performs the canonical build in a hermetic child
// process. The child environment drops exactly the ambient executor/build
// overrides the canonical builder forbids (see isCanonicalBuildEnvOverride),
// so caller-workflow executor overrides cannot influence the build and the
// strict validator is never weakened. A missing artifact or a failed build
// fails loudly — tests never silently skip their fixture and never assume a
// previous CI step produced the artifact.

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  PRIVATE_PROGRAM_BUILD_RECEIPT_PATH,
  PRIVATE_PROGRAM_BUILD_TIMEOUT_MS,
  PRIVATE_PROGRAM_WASM_PATH,
  isCanonicalBuildEnvOverride,
  privateProgramCoreSourceDigest,
  validatePrivateProgramBuildReceipt,
} from "./build-private-program.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const REPO_ROOT = resolve(dirname(SCRIPT_PATH), "..");
const BUILD_SCRIPT = resolve(SCRIPT_PATH, "..", "build-private-program.mjs");
const PACKAGE_DIR = resolve(REPO_ROOT, "packages/colors");
// Two canonical passes, the optimizer, and the registry-index copy each own
// the declared per-pass budget; the child gets three budgets of headroom and
// still bounds a wedged build instead of hanging the test command forever.
const CHILD_BUILD_TIMEOUT_MS = PRIVATE_PROGRAM_BUILD_TIMEOUT_MS * 3;
const wasmPath = resolve(PACKAGE_DIR, PRIVATE_PROGRAM_WASM_PATH);
const receiptPath = resolve(PACKAGE_DIR, PRIVATE_PROGRAM_BUILD_RECEIPT_PATH);

/**
 * The environment handed to the hermetic build child: the ambient parent
 * environment minus exactly the variables the canonical builder classifies as
 * executor/build overrides. Required pins (BINARYEN_*, RUSTUP_HOME,
 * CARGO_HOME, PATH, ...) pass through untouched, so the child builds with the
 * same canonical toolchain the release gate requires.
 */
export function hermeticBuildEnvironment(environment = process.env) {
  const filtered = {};
  for (const [key, value] of Object.entries(environment)) {
    if (isCanonicalBuildEnvOverride(key)) continue;
    filtered[key] = value;
  }
  return filtered;
}

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
  const child = spawnSync(
    process.execPath,
    [BUILD_SCRIPT, "--require-optimizer"],
    {
      cwd: REPO_ROOT,
      env: hermeticBuildEnvironment(),
      stdio: "inherit",
      timeout: CHILD_BUILD_TIMEOUT_MS,
    },
  );
  if (child.error) throw child.error;
  if (child.status !== 0) {
    throw new Error(
      `private Program canonical build child exited with status ${child.status}`,
    );
  }
  process.stdout.write(`private Program artifact ensured: ${PRIVATE_PROGRAM_WASM_PATH}\n`);
}

const invokedDirectly =
  process.argv[1] !== undefined && resolve(process.argv[1]) === SCRIPT_PATH;

if (invokedDirectly) {
  ensurePrivateProgramArtifact().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
    process.exitCode = 1;
  });
}
