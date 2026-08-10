#!/usr/bin/env node

// Shared hermetic boundary for the canonical private Program build. Both the
// package test prerequisite (ensure-private-program-artifact.mjs) and the
// release gate (verify-package-release.mjs) produce the exact optimized
// artifact by spawning the canonical builder as a child process whose
// environment drops exactly the variables isCanonicalBuildEnvOverride
// classifies as executor/build overrides. The ambient parent process is never
// mutated, the strict validator is never weakened, and the required toolchain
// pins pass through untouched.

import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  PRIVATE_PROGRAM_BUILD_TIMEOUT_MS,
  PRIVATE_PROGRAM_WASM_PATH,
  isCanonicalBuildEnvOverride,
} from "./build-private-program.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const REPO_ROOT = resolve(dirname(SCRIPT_PATH), "..");
const BUILD_SCRIPT = resolve(dirname(SCRIPT_PATH), "build-private-program.mjs");
const PACKAGE_DIR = resolve(REPO_ROOT, "packages/colors");
// Two canonical passes, the optimizer, and the registry-index copy each own
// the declared per-pass budget; the child gets three budgets of headroom and
// still bounds a wedged build instead of hanging the caller forever.
const CHILD_BUILD_TIMEOUT_MS = PRIVATE_PROGRAM_BUILD_TIMEOUT_MS * 3;

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

/**
 * Run the canonical private Program build (two optimized passes) in a
 * hermetic child process. Throws a typed Error on spawn failure or timeout
 * and a loud Error carrying the child exit status when the build itself
 * fails; the child's own diagnostics stream through inherited stdio. Returns
 * the generated artifact path on success.
 */
export function runCanonicalPrivateProgramBuild({ environment = process.env } = {}) {
  const child = spawnSync(
    process.execPath,
    [BUILD_SCRIPT, "--require-optimizer"],
    {
      cwd: REPO_ROOT,
      env: hermeticBuildEnvironment(environment),
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
  return { output: resolve(PACKAGE_DIR, PRIVATE_PROGRAM_WASM_PATH) };
}
