#!/usr/bin/env node

import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import {
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  realpath,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import {
  basename,
  delimiter,
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

import {
  PRIVATE_PROGRAM_CANONICAL_BUILD,
  PRIVATE_PROGRAM_CONSUMER_PATH,
  PRIVATE_PROGRAM_WASM_PATH,
  PRIVATE_PROGRAM_WASM_SURFACE,
  copyDeclaredCargoRegistryIndex,
  validatePrivateProgramWasmSurface,
} from "./build-private-program.mjs";
import { PRIVATE_PROGRAM_BROWSER_PASS_RECEIPT } from "./test-private-program-browser.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
export const REPO_ROOT = resolve(dirname(SCRIPT_PATH), "..");

const PROOF_SCRIPT = resolve(REPO_ROOT, "scripts/test-private-program-browser.mjs");
const PRIVATE_PROGRAM_REQUEST_V1_LENGTH = 296;
const PRIVATE_PROGRAM_RESULT_V1_LENGTH = 95;
const PRIVATE_PROGRAM_REQUEST_LENGTH_EXPORT =
  "labcolors_private_fixture_request_v1_len";
const PRIVATE_PROGRAM_RESULT_LENGTH_EXPORT =
  "labcolors_private_fixture_result_v1_len";
const PRIVATE_PROGRAM_HOST_MODULE = "labcolors_private_fixture_host_v1";
const PRIVATE_PROGRAM_HOST_INSTALL = "labcolors_private_fixture_host_install_v1";
const PRIVATE_PROGRAM_HOST_CONFIRM =
  "labcolors_private_fixture_host_confirm_disposed_v1";
const CARGO_ARTIFACT_PATH = Object.freeze([
  "wasm32-unknown-unknown",
  "release",
  "labcolors_core.wasm",
]);
const CHILD_OUTPUT_LIMIT_BYTES = 2 * 1024 * 1024;
const BROWSER_PASS_RECEIPT = PRIVATE_PROGRAM_BROWSER_PASS_RECEIPT;
const BROWSER_ASSERTION_PREFIX =
  "private Program browser proof: browser assertion failed:";
const BROWSER_CLEANUP_FAILURE = "private Program browser proof or cleanup failed";
const MUTATION_RECEIPT = "LAB_COLORS_PRIVATE_PROGRAM_MUTATIONS_PASS v1";
const EXPECTED_SEMANTIC_MUTATION_COUNT = 6;
const EXPECTED_BINARY_DIFFERENTIAL_COUNT = 1;
const MUTATION_TIMEOUT_ENV = "LAB_COLORS_PRIVATE_MUTATION_TIMEOUT_MS";
const CHILD_TIMEOUT_ENV = "LAB_COLORS_PRIVATE_MUTATION_CHILD_TIMEOUT_MS";
const BROWSER_TIMEOUT_ENV = "LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS";
const SOURCE_WORKSPACE_PREFIX = "labcolors-private-program-mutations-";
const PACKAGE_SHELL_DIRECTORY = "node_modules/@labpics/colors";

function lines(...values) {
  return `${values.join("\n")}\n`;
}

function mutation(definition) {
  return Object.freeze(definition);
}

export const PRIVATE_PROGRAM_MUTATION_CASES = Object.freeze([
  mutation({
    id: "compiler-graph-edge-deletion",
    proof: "semantic",
    artifact: "rust-wasm",
    sourcePath: "crates/labcolors-core/src/private_fixture.rs",
    search: lines(
      "    draft.push_occurrence_surface(FILL_SURFACE, FILL_ON_PAGE);",
    ),
    replacement: "",
    expectedBrowserAssertion:
      "PrivateProgramConsumerError: private Program run failed with status 6",
  }),
  mutation({
    id: "hard-constraint-deletion",
    proof: "semantic",
    artifact: "rust-wasm",
    sourcePath: "crates/labcolors-core/src/private_fixture.rs",
    search: lines(
      "    draft.push_exact_visible_unary_hard(",
      "        FINAL_VISIBLE_IDENTITY,",
      "        LABEL_ON_FILL,",
      "        authored.expected_final_visible,",
      "    );",
    ),
    replacement: "",
    expectedBrowserAssertion:
      "PrivateProgramConsumerError: private Program run failed with status 6",
  }),
  mutation({
    id: "final-recheck-call-edge-deletion",
    proof: "semantic-source-deletion",
    artifact: "rust-wasm",
    sourcePath: "crates/labcolors-core/src/program_session.rs",
    search: lines(
      "    let has_hard_violation = if has_hard_constraints {",
      "        scan_program_candidate(",
      "            runtime,",
      "            epoch,",
      "            scenario_set,",
      "            candidate_state_index,",
      "            ProgramEvaluationPhaseV1::Hard,",
      "            ProgramCandidateCollectionV1 {",
      "                evidence: ProgramConstraintEvidenceCaptureV1::Report {",
      "                    cells: &mut arena.cells,",
      "                    relation_members: &mut arena.relation_members,",
      "                },",
      "                outputs: Some(&mut arena.outputs),",
      "                point_causal: Some(ProgramPointCausalBuffersV1 {",
      "                    considered_state_index: None,",
      "                    records: &mut arena.point_causal_records,",
      "                    steps: &mut arena.point_causal_steps,",
      "                }),",
      "            },",
      "        )?",
      "    } else {",
      "        false",
      "    };",
    ),
    replacement: lines("    let has_hard_violation = false;"),
    expectedBrowserAssertion:
      "PrivateProgramConsumerError: private Program run failed with status 8",
  }),
  mutation({
    id: "session-observed-update-bypass",
    proof: "semantic",
    artifact: "rust-wasm",
    sourcePath: "crates/labcolors-core/src/private_fixture.rs",
    search: lines(
      "attachment.update(UpdateV1::Observed {",
      "        revision: authored.observation_revision,",
      "        scenarios: &scenarios,",
      "    })",
    ).slice(0, -1),
    replacement: lines(
      "attachment.update(UpdateV1::Unknown {",
      "        revision: authored.observation_revision,",
      "        reason_id: first_scenario.id,",
      "    })",
    ).slice(0, -1),
    expectedBrowserAssertion:
      "PrivateProgramConsumerError: shipping trace permits exactly one SetAll callback",
  }),
  mutation({
    id: "external-attachment-handoff-binding-bypass",
    proof: "semantic-source-bypass",
    artifact: "rust-wasm",
    sourcePath: "crates/labcolors-core/src/private_fixture.rs",
    search: lines(
      "            handoff_point_sink(sink_output, host),",
    ),
    replacement: lines(
      "            handoff_point_sink(",
      "                HandoffPointSinkOutputIdV1::new(sink_output.value().wrapping_add(1)),",
      "                host,",
      "            ),",
    ),
    expectedBrowserAssertion:
      "PrivateProgramConsumerError: private Program run failed with status 7",
  }),
  mutation({
    id: "javascript-publish-deletion",
    proof: "semantic",
    artifact: "javascript",
    sourcePath: "packages/colors/private-program/consumer.js",
    search: lines(
      "    exactLeaseSuccess(",
      "      lease.publish(frozenPublication(outputBinding, css)),",
      '      "output lease publish",',
      "    );",
    ),
    replacement: lines("    frozenPublication(outputBinding, css);"),
    expectedBrowserAssertion:
      "Error: private Program browser fixture: computed background is the exact expected CSS literal",
  }),
]);

function fail(message, options) {
  throw new Error(`private Program mutation proof: ${message}`, options);
}

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function exactPositiveMilliseconds(environment, name) {
  const raw = environment[name];
  if (!/^[1-9][0-9]*$/u.test(raw ?? "")) {
    fail(`${name} must be a positive integer`);
  }
  const value = Number(raw);
  if (!Number.isSafeInteger(value)) fail(`${name} exceeds the safe integer range`);
  return value;
}

export function parseMutationTimeoutPolicy(environment = process.env) {
  const overallMilliseconds = exactPositiveMilliseconds(environment, MUTATION_TIMEOUT_ENV);
  const childMilliseconds = exactPositiveMilliseconds(environment, CHILD_TIMEOUT_ENV);
  const browserMilliseconds = exactPositiveMilliseconds(environment, BROWSER_TIMEOUT_ENV);
  if (childMilliseconds <= browserMilliseconds) {
    fail(`${CHILD_TIMEOUT_ENV} must exceed ${BROWSER_TIMEOUT_ENV} for browser cleanup`);
  }
  if (overallMilliseconds <= childMilliseconds) {
    fail(`${MUTATION_TIMEOUT_ENV} must exceed ${CHILD_TIMEOUT_ENV}`);
  }
  return Object.freeze({
    overallMilliseconds,
    childMilliseconds,
    browserMilliseconds,
  });
}

function parseArguments(argv = process.argv.slice(2)) {
  if (argv.length !== 2) {
    fail("expected exactly <verified-baseline.tgz> <lowercase-64-sha256>");
  }
  const tarball = resolve(argv[0]);
  if (extname(tarball) !== ".tgz") fail("the verified baseline must have a .tgz suffix");
  const expectedSha256 = argv[1];
  if (!/^[0-9a-f]{64}$/u.test(expectedSha256)) {
    fail("the verified baseline SHA-256 must be 64 lowercase hexadecimal characters");
  }
  return Object.freeze({ tarball, expectedSha256 });
}

function occurrenceCount(source, search) {
  let count = 0;
  let offset = 0;
  for (;;) {
    const index = source.indexOf(search, offset);
    if (index === -1) return count;
    count += 1;
    offset = index + search.length;
  }
}

export function applyExactMutation(source, definition) {
  if (
    typeof source !== "string" ||
    typeof definition?.search !== "string" ||
    typeof definition?.replacement !== "string"
  ) {
    fail("mutation source, search anchor, and replacement must be strings");
  }
  if (definition.search.length === 0) fail(`${definition.id} has an empty source anchor`);
  const count = occurrenceCount(source, definition.search);
  if (count !== 1) {
    fail(`${definition.id} expected exactly one source anchor, found ${count}`);
  }
  const mutated = source.replace(definition.search, () => definition.replacement);
  if (mutated === source) fail(`${definition.id} is a no-op`);
  return mutated;
}

export async function assertPrivateProgramMutationAnchors(repoRoot = REPO_ROOT) {
  const checked = [];
  for (const definition of PRIVATE_PROGRAM_MUTATION_CASES) {
    const sourcePath = resolve(repoRoot, definition.sourcePath);
    await assertAdmittedRegularFile(
      sourcePath,
      repoRoot,
      `${definition.id} repository source`,
    );
    const source = await readFile(sourcePath, "utf8");
    applyExactMutation(source, definition);
    checked.push(definition.id);
  }
  if (PRIVATE_PROGRAM_MUTATION_CASES.length !== EXPECTED_SEMANTIC_MUTATION_COUNT) {
    fail("mutation inventory cardinality drifted");
  }
  return Object.freeze(checked);
}

export function assertMutationSpecificBrowserFailure(definition, result) {
  const transcript = `${result.stdout ?? ""}\n${result.stderr ?? ""}`;
  if (!Number.isInteger(result.code) || result.code <= 0 || result.signal !== null) {
    fail(`${definition.id} did not exit as a normal nonzero browser assertion`);
  }
  if (transcript.includes(BROWSER_PASS_RECEIPT)) {
    fail(`${definition.id} emitted the browser PASS receipt`);
  }
  if (transcript.includes(BROWSER_CLEANUP_FAILURE)) {
    fail(`${definition.id} encountered browser cleanup failure`);
  }
  const assertionPrefixCount = occurrenceCount(transcript, BROWSER_ASSERTION_PREFIX);
  if (assertionPrefixCount !== 1) {
    fail(
      `${definition.id} expected exactly one browser assertion boundary, ` +
        `found ${assertionPrefixCount}`,
    );
  }
  const assertionLine = transcript
    .split(/\r?\n/u)
    .find((line) => line.includes(BROWSER_ASSERTION_PREFIX));
  const actualAssertion = assertionLine
    .slice(assertionLine.indexOf(BROWSER_ASSERTION_PREFIX) + BROWSER_ASSERTION_PREFIX.length)
    .trim();
  if (actualAssertion !== definition.expectedBrowserAssertion) {
    fail(
      `${definition.id} did not emit its mutation-specific failure evidence: ` +
        `expected=${JSON.stringify(definition.expectedBrowserAssertion)} ` +
        `actual=${JSON.stringify(actualAssertion)}`,
    );
  }
  return true;
}

function exactExportedLength(instance, name, expected) {
  const exported = instance.exports[name];
  if (typeof exported !== "function") fail(`validated WASM is missing ${name}`);
  const actual = exported();
  if (!Number.isSafeInteger(actual) || actual !== expected) {
    fail(`${name} differs from ABI v1: expected=${expected} actual=${String(actual)}`);
  }
}

export function validateMutationWasm(bytes) {
  const wasm = Buffer.from(bytes);
  validatePrivateProgramWasmSurface(wasm);
  const module = new WebAssembly.Module(wasm);
  const instance = new WebAssembly.Instance(module, {
    [PRIVATE_PROGRAM_HOST_MODULE]: {
      [PRIVATE_PROGRAM_HOST_INSTALL]: () => 0,
      [PRIVATE_PROGRAM_HOST_CONFIRM]: () => 0,
    },
  });
  exactExportedLength(
    instance,
    PRIVATE_PROGRAM_REQUEST_LENGTH_EXPORT,
    PRIVATE_PROGRAM_REQUEST_V1_LENGTH,
  );
  exactExportedLength(
    instance,
    PRIVATE_PROGRAM_RESULT_LENGTH_EXPORT,
    PRIVATE_PROGRAM_RESULT_V1_LENGTH,
  );
  return PRIVATE_PROGRAM_WASM_SURFACE;
}

function inheritedOperatingSystemEnvironment(environment = process.env) {
  const inherited = {};
  for (const name of ["SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT"]) {
    if (environment[name] !== undefined) inherited[name] = environment[name];
  }
  return inherited;
}

function isStrictlyWithin(root, candidate) {
  const fromRoot = relative(root, candidate);
  return (
    fromRoot !== "" &&
    fromRoot !== ".." &&
    !fromRoot.startsWith(`..${sep}`) &&
    !isAbsolute(fromRoot)
  );
}

function isWithinOrEqual(root, candidate) {
  const fromRoot = relative(root, candidate);
  return (
    fromRoot === "" ||
    (fromRoot !== ".." &&
      !fromRoot.startsWith(`..${sep}`) &&
      !isAbsolute(fromRoot))
  );
}

export async function assertAdmittedTree(
  treeRoot,
  boundaryRoot,
  { allowContainedFileSymlinks, label },
) {
  const lexicalRootMetadata = await lstat(treeRoot);
  if (!lexicalRootMetadata.isDirectory() || lexicalRootMetadata.isSymbolicLink()) {
    fail(`${label} root is not a physical directory`);
  }
  const [physicalTreeRoot, physicalBoundaryRoot] = await Promise.all([
    realpath(treeRoot),
    realpath(boundaryRoot),
  ]);
  if (!isWithinOrEqual(physicalBoundaryRoot, physicalTreeRoot)) {
    fail(`${label} root escapes its admitted physical boundary`);
  }

  async function walk(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = resolve(directory, entry.name);
      const metadata = await lstat(path);
      if (metadata.isSymbolicLink()) {
        if (!allowContainedFileSymlinks) {
          fail(`${label} contains a symlink or reparse point: ${entry.name}`);
        }
        const target = await realpath(path);
        if (!isStrictlyWithin(physicalBoundaryRoot, target)) {
          fail(`${label} symlink escapes its admitted physical boundary`);
        }
        const targetMetadata = await stat(target);
        if (!targetMetadata.isFile()) {
          fail(`${label} symlink does not resolve to a regular file`);
        }
        continue;
      }
      if (metadata.isDirectory()) {
        await walk(path);
      } else if (!metadata.isFile()) {
        fail(`${label} contains a non-file filesystem object`);
      }
    }
  }

  await walk(treeRoot);
}

export async function assertAdmittedRegularFile(path, boundaryRoot, label) {
  const lexicalBoundaryRoot = resolve(boundaryRoot);
  const lexicalPath = resolve(path);
  if (!isStrictlyWithin(lexicalBoundaryRoot, lexicalPath)) {
    fail(`${label} escapes its admitted lexical boundary`);
  }
  const boundaryMetadata = await lstat(lexicalBoundaryRoot);
  if (!boundaryMetadata.isDirectory() || boundaryMetadata.isSymbolicLink()) {
    fail(`${label} boundary is not a physical directory`);
  }
  let cursor = lexicalBoundaryRoot;
  const segments = relative(lexicalBoundaryRoot, lexicalPath).split(sep);
  for (let index = 0; index < segments.length; index += 1) {
    cursor = resolve(cursor, segments[index]);
    const metadata = await lstat(cursor);
    if (metadata.isSymbolicLink()) {
      fail(`${label} path contains a symlink or reparse point`);
    }
    const isFinal = index === segments.length - 1;
    if (isFinal ? !metadata.isFile() : !metadata.isDirectory()) {
      fail(`${label} path has an unexpected filesystem object`);
    }
  }
  const [physicalPath, physicalBoundaryRoot] = await Promise.all([
    realpath(lexicalPath),
    realpath(lexicalBoundaryRoot),
  ]);
  if (!isStrictlyWithin(physicalBoundaryRoot, physicalPath)) {
    fail(`${label} escapes its admitted physical boundary`);
  }
}

async function exactRegularFile(path, label) {
  const metadata = await stat(path);
  if (!metadata.isFile()) fail(`${label} is not a regular file`);
  return path;
}

function boundedTail(value) {
  const text = String(value ?? "");
  const limit = 16_384;
  return text.length <= limit ? text : text.slice(text.length - limit);
}

function terminateProcessTree(child) {
  try {
    if (process.platform !== "win32" && child.pid !== undefined) {
      process.kill(-child.pid, "SIGKILL");
    } else if (child.exitCode === null && child.signalCode === null) {
      child.kill("SIGKILL");
    }
  } catch (error) {
    if (error?.code !== "ESRCH") throw error;
  }
}

function createCommandRunner(policy) {
  const active = new Set();
  const terminationErrors = [];
  const startedAt = Date.now();
  const controller = new AbortController();
  const overallTimer = setTimeout(() => {
    for (const child of active) {
      try {
        terminateProcessTree(child);
      } catch (error) {
        terminationErrors.push(error);
      }
    }
    controller.abort(new Error("private Program mutation proof exceeded its overall deadline"));
  }, policy.overallMilliseconds);

  function remainingMilliseconds(label) {
    const remaining = policy.overallMilliseconds - (Date.now() - startedAt);
    if (remaining <= 0 || controller.signal.aborted) {
      fail(`${label} started after the overall mutation deadline`);
    }
    return remaining;
  }

  async function run(command, args, options = {}) {
    const label = options.label ?? basename(command);
    const timeout = Math.min(
      policy.childMilliseconds,
      remainingMilliseconds(label),
    );
    return new Promise((resolveRun, rejectRun) => {
      let childDeadlineExceeded = false;
      let childDeadlineError;
      let childDeadlineTimer;
      const child = execFile(
        command,
        args,
        {
          cwd: options.cwd ?? REPO_ROOT,
          env: options.env ?? process.env,
          encoding: "utf8",
          maxBuffer: CHILD_OUTPUT_LIMIT_BYTES,
          timeout,
          killSignal: "SIGKILL",
          windowsHide: true,
          detached: process.platform !== "win32",
          signal: controller.signal,
        },
        (error, stdout, stderr) => {
          clearTimeout(childDeadlineTimer);
          active.delete(child);
          let cleanupError;
          try {
            // The direct executor is Linux-only. Killing the detached group here also
            // removes descendants after a timed-out parent has already exited.
            terminateProcessTree(child);
          } catch (caught) {
            cleanupError = caught;
          }
          const result = Object.freeze({
            code: error === null ? 0 : error.code,
            signal: error?.signal ?? null,
            stdout: stdout ?? "",
            stderr: stderr ?? "",
          });
          if (cleanupError !== undefined) {
            rejectRun(
              new Error(`${label} process-group cleanup failed`, { cause: cleanupError }),
            );
            return;
          }
          if (childDeadlineExceeded) {
            rejectRun(
              new Error(
                `${label} infrastructure failure: child deadline ${timeout}ms exceeded`,
                { cause: childDeadlineError ?? error },
              ),
            );
            return;
          }
          if (error === null) {
            resolveRun(result);
            return;
          }
          if (
            options.allowNonzero === true &&
            Number.isInteger(error.code) &&
            error.code > 0 &&
            error.signal == null &&
            error.killed !== true
          ) {
            resolveRun(result);
            return;
          }
          rejectRun(
            new Error(
              `${label} infrastructure failure: code=${String(error.code)} ` +
                `signal=${String(error.signal)}\n` +
                `${boundedTail(stderr)}\n${boundedTail(stdout)}`,
              { cause: error },
            ),
          );
        },
      );
      active.add(child);
      childDeadlineTimer = setTimeout(() => {
        childDeadlineExceeded = true;
        try {
          terminateProcessTree(child);
        } catch (error) {
          childDeadlineError = error;
          try {
            child.kill("SIGKILL");
          } catch (fallbackError) {
            childDeadlineError = new AggregateError(
              [error, fallbackError],
              `${label} deadline process termination failed`,
            );
          }
        }
      }, timeout);
    });
  }

  function assertWithinDeadline(label) {
    remainingMilliseconds(label);
  }

  async function close() {
    clearTimeout(overallTimer);
    controller.abort(new Error("private Program mutation proof cleanup"));
    const errors = [...terminationErrors];
    for (const child of active) {
      try {
        terminateProcessTree(child);
      } catch (error) {
        errors.push(error);
      }
    }
    active.clear();
    if (errors.length !== 0) throw new AggregateError(errors, "child cleanup failed");
  }

  return Object.freeze({ run, assertWithinDeadline, close });
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
    return Object.freeze({ command: process.execPath, prefix: [lifecycleEntrypoint] });
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
    return Object.freeze({ command: process.execPath, prefix: [entrypoint] });
  }
  return Object.freeze({ command: "npm", prefix: [] });
}

async function installImmutablePackageShell(root, tarball, runner) {
  const installRoot = resolve(root, "baseline-install");
  await mkdir(installRoot, { recursive: true });
  await writeFile(
    resolve(installRoot, "package.json"),
    `${JSON.stringify({ private: true }, null, 2)}\n`,
    { flag: "wx" },
  );
  const npm = npmInvocation();
  await runner.run(
    npm.command,
    [
      ...npm.prefix,
      "install",
      "--offline",
      "--ignore-scripts",
      "--no-audit",
      "--no-fund",
      "--no-package-lock",
      "--save=false",
      tarball,
    ],
    { cwd: installRoot, label: "immutable baseline package install" },
  );
  const lexicalPackageRoot = resolve(installRoot, PACKAGE_SHELL_DIRECTORY);
  const lexicalMetadata = await lstat(lexicalPackageRoot);
  if (!lexicalMetadata.isDirectory() || lexicalMetadata.isSymbolicLink()) {
    fail("installed baseline package shell is not a physical directory");
  }
  const packageRoot = await realpath(lexicalPackageRoot);
  if (!isStrictlyWithin(installRoot, packageRoot)) {
    fail("installed baseline package shell escapes its temporary root");
  }
  await assertAdmittedTree(packageRoot, packageRoot, {
    allowContainedFileSymlinks: false,
    label: "installed baseline package shell",
  });
  return packageRoot;
}

async function copySourceWorkspace(root) {
  const workspace = resolve(root, "source");
  await mkdir(workspace, { recursive: true });
  const rootFiles = ["Cargo.toml", "Cargo.lock", "LICENSE"];
  await assertAdmittedTree(resolve(REPO_ROOT, "crates"), REPO_ROOT, {
    allowContainedFileSymlinks: true,
    label: "repository Rust source tree",
  });
  await Promise.all(
    rootFiles.map((name) =>
      assertAdmittedRegularFile(
        resolve(REPO_ROOT, name),
        REPO_ROOT,
        `repository ${name}`,
      ),
    ),
  );
  await Promise.all([
    cp(resolve(REPO_ROOT, "crates"), resolve(workspace, "crates"), {
      recursive: true,
      dereference: false,
      preserveTimestamps: true,
      verbatimSymlinks: true,
    }),
    ...rootFiles.map((name) =>
      cp(resolve(REPO_ROOT, name), resolve(workspace, name), {
        dereference: false,
        verbatimSymlinks: true,
      }),
    ),
  ]);
  await assertAdmittedTree(workspace, workspace, {
    allowContainedFileSymlinks: true,
    label: "temporary Rust source tree",
  });
  return workspace;
}

async function resolveCanonicalRustExecutors(runner) {
  const rustupHome = await realpath(
    resolve(process.env.RUSTUP_HOME?.trim() || join(homedir(), ".rustup")),
  );
  const toolchainsRoot = resolve(rustupHome, "toolchains");
  const rust = PRIVATE_PROGRAM_CANONICAL_BUILD.toolchain.rust;
  const candidates = (await readdir(toolchainsRoot, { withFileTypes: true })).filter(
    (entry) => entry.isDirectory() && entry.name.startsWith(`${rust}-`),
  );
  if (candidates.length !== 1) {
    fail(`expected exactly one installed ${rust} toolchain, found ${candidates.length}`);
  }
  const toolchainRoot = await realpath(resolve(toolchainsRoot, candidates[0].name));
  if (!isStrictlyWithin(rustupHome, toolchainRoot)) {
    fail("canonical Rust toolchain resolves outside RUSTUP_HOME");
  }
  const suffix = process.platform === "win32" ? ".exe" : "";
  const cargo = await realpath(resolve(toolchainRoot, `bin/cargo${suffix}`));
  const rustc = await realpath(resolve(toolchainRoot, `bin/rustc${suffix}`));
  await Promise.all([
    exactRegularFile(cargo, "canonical cargo"),
    exactRegularFile(rustc, "canonical rustc"),
  ]);
  const probeEnvironment = {
    ...inheritedOperatingSystemEnvironment(),
    PATH: dirname(cargo),
    RUSTUP_HOME: rustupHome,
    LANG: "C",
    LC_ALL: "C",
  };
  const cargoVersion = await runner.run(cargo, ["--version", "--verbose"], {
    env: probeEnvironment,
    label: "canonical cargo identity probe",
  });
  const rustcVersion = await runner.run(rustc, ["--version", "--verbose"], {
    env: probeEnvironment,
    label: "canonical rustc identity probe",
  });
  const expected = PRIVATE_PROGRAM_CANONICAL_BUILD.toolchain;
  for (const [label, output, release, commit] of [
    ["cargo", cargoVersion.stdout, expected.cargo, expected.cargoCommit],
    ["rustc", rustcVersion.stdout, expected.rust, expected.rustcCommit],
  ]) {
    if (
      !output.includes(`release: ${release}\n`) ||
      !output.includes(`commit-hash: ${commit}\n`)
    ) {
      fail(`${label} identity differs from the private Program release recipe`);
    }
  }
  return Object.freeze({ cargo, rustc, rustupHome });
}

async function validateOptimizerPayload(path, expected, label) {
  const bytes = await readFile(path);
  if (bytes.length !== expected.bytes || sha256(bytes) !== expected.sha256) {
    fail(`${label} differs from the byte-bound private Program optimizer`);
  }
}

async function resolveCanonicalOptimizer(root, runner) {
  const optimizer = PRIVATE_PROGRAM_CANONICAL_BUILD.toolchain.optimizer;
  if (process.version !== `v${optimizer.node}`) {
    fail(`Node ${process.version} differs from canonical v${optimizer.node}`);
  }
  if (process.env.BINARYEN_RELEASE !== optimizer.binaryenRelease) {
    fail("BINARYEN_RELEASE differs from the private Program release recipe");
  }
  if (process.env.BINARYEN_NODE_SHA256 !== optimizer.binaryenNodeArchiveSha256) {
    fail("BINARYEN_NODE_SHA256 differs from the private Program release recipe");
  }
  const configuredRoot = process.env.BINARYEN_ROOT?.trim();
  if (!configuredRoot || !isAbsolute(configuredRoot)) {
    fail("BINARYEN_ROOT must name the absolute byte-bound optimizer directory");
  }
  const physicalRoot = await realpath(configuredRoot);
  for (const [name, expected] of Object.entries(optimizer.files)) {
    await validateOptimizerPayload(
      resolve(physicalRoot, name),
      expected,
      `optimizer payload ${name}`,
    );
  }
  const script = resolve(physicalRoot, "wasm-opt.js");
  const temporaryDirectory = resolve(root, "optimizer-temp");
  await mkdir(temporaryDirectory, { recursive: true });
  const environment = {
    ...inheritedOperatingSystemEnvironment(),
    LANG: "C",
    LC_ALL: "C",
    TMPDIR: temporaryDirectory,
    TEMP: temporaryDirectory,
    TMP: temporaryDirectory,
  };
  const version = await runner.run(process.execPath, [script, "--version"], {
    env: environment,
    label: "canonical wasm-opt identity probe",
  });
  if (version.stdout.trim() !== optimizer.wasmOptVersion) {
    fail("wasm-opt version differs from the private Program release recipe");
  }
  const recipe = PRIVATE_PROGRAM_CANONICAL_BUILD.recipe.optimizer;
  if (
    recipe.command !== "$NODE_EXECUTABLE" ||
    recipe.script !== "$BINARYEN_ROOT/wasm-opt.js" ||
    recipe.args[0] !== "$RAW_WASM" ||
    recipe.args[1] !== "-o" ||
    recipe.args[2] !== "$OPTIMIZED_WASM"
  ) {
    fail("private Program optimizer recipe placeholders drifted");
  }
  return Object.freeze({
    script,
    flags: Object.freeze(recipe.args.slice(3)),
    environment,
  });
}

function cargoEnvironment({ workspace, root, target, executors }) {
  const cargoHome = resolve(root, "cargo-home");
  const temporaryDirectory = resolve(root, "cargo-temp");
  const recipe = PRIVATE_PROGRAM_CANONICAL_BUILD.recipe.cargo;
  const replacements = new Map([
    ["$REPO_ROOT", workspace],
    ["$CARGO_HOME", cargoHome],
    ["$RUSTUP_HOME", executors.rustupHome],
    ["$ISOLATED_CARGO_TARGET_DIR", target],
    ["$ISOLATED_TEMP_DIR", temporaryDirectory],
  ]);
  const encodedRustflags = recipe.encodedRustflags.map((template) => {
    let resolved = template;
    for (const [placeholder, value] of replacements) {
      resolved = resolved.replaceAll(placeholder, () => value);
    }
    if (resolved.includes("$")) fail(`unresolved Rust flag placeholder: ${resolved}`);
    return resolved;
  });
  return Object.freeze({
    cargoHome,
    temporaryDirectory,
    environment: {
      ...inheritedOperatingSystemEnvironment(),
      PATH: [...new Set([dirname(executors.cargo), dirname(executors.rustc)])].join(
        delimiter,
      ),
      CARGO_HOME: cargoHome,
      RUSTUP_HOME: executors.rustupHome,
      RUSTC: executors.rustc,
      CARGO_ENCODED_RUSTFLAGS: encodedRustflags.join("\x1f"),
      CARGO_INCREMENTAL: "0",
      CARGO_NET_OFFLINE: "true",
      CARGO_TERM_COLOR: "never",
      LANG: "C",
      LC_ALL: "C",
      TMPDIR: temporaryDirectory,
      TEMP: temporaryDirectory,
      TMP: temporaryDirectory,
    },
  });
}

function cargoArguments(target) {
  return PRIVATE_PROGRAM_CANONICAL_BUILD.recipe.cargo.args.map((argument) =>
    argument === "$ISOLATED_CARGO_TARGET_DIR" ? target : argument,
  );
}

async function buildWorkspaceWasm(context, label) {
  const rawPath = resolve(context.target, ...CARGO_ARTIFACT_PATH);
  const optimizedPath = resolve(context.artifacts, `${label}.wasm`);
  await Promise.all([
    rm(rawPath, { force: true }),
    rm(optimizedPath, { force: true }),
  ]);
  await context.runner.run(context.executors.cargo, cargoArguments(context.target), {
    cwd: context.workspace,
    env: context.cargo.environment,
    label: `Cargo build for ${label}`,
  });
  await exactRegularFile(rawPath, `${label} raw WASM`);
  await context.runner.run(
    process.execPath,
    [
      context.optimizer.script,
      rawPath,
      "-o",
      optimizedPath,
      ...context.optimizer.flags,
    ],
    {
      cwd: context.workspace,
      env: context.optimizer.environment,
      label: `wasm-opt for ${label}`,
    },
  );
  const bytes = await readFile(optimizedPath);
  validateMutationWasm(bytes);
  return bytes;
}

async function withSourceMutation(context, definition, action) {
  const path = resolve(context.workspace, definition.sourcePath);
  const original = await readFile(path, "utf8");
  const mutated = applyExactMutation(original, definition);
  await writeFile(path, mutated, "utf8");
  try {
    return await action();
  } finally {
    await writeFile(path, original, "utf8");
  }
}

async function packMutant(packageRoot, packRoot, runner, label) {
  await mkdir(packRoot, { recursive: true });
  const npm = npmInvocation();
  const result = await runner.run(
    npm.command,
    [
      ...npm.prefix,
      "pack",
      "--ignore-scripts",
      "--json",
      `--pack-destination=${packRoot}`,
      packageRoot,
    ],
    { cwd: packRoot, label: `npm pack for ${label}` },
  );
  let report;
  try {
    report = JSON.parse(result.stdout);
  } catch (error) {
    fail(`${label} npm pack returned non-JSON output`, { cause: error });
  }
  if (!Array.isArray(report) || report.length !== 1) {
    fail(`${label} npm pack returned an inexact result inventory`);
  }
  const filename = report[0]?.filename;
  if (typeof filename !== "string" || basename(filename) !== filename) {
    fail(`${label} npm pack returned an unsafe filename`);
  }
  const tarball = resolve(packRoot, filename);
  if (!isStrictlyWithin(packRoot, tarball)) fail(`${label} tarball escapes pack root`);
  const bytes = await readFile(tarball);
  if (bytes.length === 0) fail(`${label} tarball is empty`);
  return Object.freeze({ tarball, sha256: sha256(bytes) });
}

async function packMutation(context, definition, wasm) {
  const packageRoot = resolve(context.root, "packages", definition.id);
  await cp(context.packageShell, packageRoot, {
    recursive: true,
    dereference: false,
    preserveTimestamps: true,
  });
  await assertAdmittedTree(packageRoot, packageRoot, {
    allowContainedFileSymlinks: false,
    label: `${definition.id} package shell copy`,
  });
  const consumerPath = resolve(packageRoot, PRIVATE_PROGRAM_CONSUMER_PATH);
  const wasmPath = resolve(packageRoot, PRIVATE_PROGRAM_WASM_PATH);
  if (definition.artifact === "rust-wasm") {
    await writeFile(wasmPath, wasm);
    const copiedConsumer = await readFile(consumerPath);
    if (!copiedConsumer.equals(context.baselineConsumer)) {
      fail(`${definition.id} changed the packed JavaScript consumer`);
    }
  } else if (definition.artifact === "javascript") {
    const source = (await readFile(consumerPath)).toString("utf8");
    await writeFile(consumerPath, applyExactMutation(source, definition), "utf8");
    const copiedWasm = await readFile(wasmPath);
    if (!copiedWasm.equals(context.baselineWasm)) {
      fail(`${definition.id} changed the packed private WASM`);
    }
    validateMutationWasm(copiedWasm);
  } else {
    fail(`${definition.id} has an unknown artifact class`);
  }
  await assertAdmittedTree(packageRoot, packageRoot, {
    allowContainedFileSymlinks: false,
    label: `${definition.id} mutated package`,
  });

  return packMutant(
    packageRoot,
    resolve(context.root, "tarballs", definition.id),
    context.runner,
    definition.id,
  );
}

async function packageAndKillMutation(context, definition, wasm) {
  const packed = await packMutation(context, definition, wasm);
  const browserResult = await context.runner.run(
    process.execPath,
    [PROOF_SCRIPT, packed.tarball, packed.sha256],
    {
      cwd: REPO_ROOT,
      env: process.env,
      label: `real-browser proof for ${definition.id}`,
      allowNonzero: true,
    },
  );
  assertMutationSpecificBrowserFailure(definition, browserResult);
  process.stdout.write(
    `private Program mutant killed: ${definition.id} tarball_sha256=${packed.sha256}\n`,
  );
}

async function executeMutationProof({ tarball, expectedSha256 }, policy) {
  const root = await mkdtemp(join(tmpdir(), SOURCE_WORKSPACE_PREFIX));
  const runner = createCommandRunner(policy);
  let failure;
  let receipt;
  try {
    const suppliedMetadata = await lstat(tarball);
    if (!suppliedMetadata.isFile() || suppliedMetadata.isSymbolicLink()) {
      fail("verified baseline is not a physical regular file");
    }
    const suppliedBytes = await readFile(tarball);
    const actualSha256 = sha256(suppliedBytes);
    if (actualSha256 !== expectedSha256) {
      fail(`verified baseline SHA-256 mismatch: expected=${expectedSha256} actual=${actualSha256}`);
    }
    const immutableTarball = resolve(root, "verified-baseline.tgz");
    await writeFile(immutableTarball, suppliedBytes, { flag: "wx", mode: 0o600 });
    const packageShell = await installImmutablePackageShell(root, immutableTarball, runner);
    const baselineConsumer = await readFile(
      resolve(packageShell, PRIVATE_PROGRAM_CONSUMER_PATH),
    );
    const baselineWasm = await readFile(resolve(packageShell, PRIVATE_PROGRAM_WASM_PATH));
    validateMutationWasm(baselineWasm);

    await assertPrivateProgramMutationAnchors();
    const javascriptMutations = PRIVATE_PROGRAM_MUTATION_CASES.filter(
      ({ artifact }) => artifact === "javascript",
    );
    if (javascriptMutations.length !== 1) {
      fail(`expected exactly one JavaScript mutation, found ${javascriptMutations.length}`);
    }
    const repositoryConsumer = await readFile(
      resolve(REPO_ROOT, javascriptMutations[0].sourcePath),
    );
    if (!repositoryConsumer.equals(baselineConsumer)) {
      fail(
        "verified baseline consumer differs from the exact admitted repository consumer",
      );
    }
    runner.assertWithinDeadline("source workspace copy");
    const workspace = await copySourceWorkspace(root);
    const artifacts = resolve(root, "wasm-artifacts");
    const target = resolve(root, "cargo-target");
    await Promise.all([
      mkdir(artifacts, { recursive: true }),
      mkdir(target, { recursive: true }),
      mkdir(resolve(root, "cargo-home"), { recursive: true }),
      mkdir(resolve(root, "cargo-temp"), { recursive: true }),
    ]);
    await copyDeclaredCargoRegistryIndex(resolve(root, "cargo-home"));
    const executors = await resolveCanonicalRustExecutors(runner);
    const optimizer = await resolveCanonicalOptimizer(root, runner);
    const cargo = cargoEnvironment({ workspace, root, target, executors });
    const buildContext = Object.freeze({
      root,
      workspace,
      artifacts,
      target,
      executors,
      optimizer,
      cargo,
      runner,
    });

    const controlWasm = await buildWorkspaceWasm(buildContext, "production-control");
    if (!controlWasm.equals(baselineWasm)) {
      fail(
        "same-recipe production control differs from the exact verified baseline private WASM",
      );
    }

    const packageContext = Object.freeze({
      root,
      runner,
      packageShell,
      baselineConsumer,
      baselineWasm,
    });
    let killed = 0;
    let binaryDifferentials = 0;
    for (const definition of PRIVATE_PROGRAM_MUTATION_CASES) {
      runner.assertWithinDeadline(definition.id);
      if (definition.artifact === "rust-wasm") {
        const wasm = await withSourceMutation(buildContext, definition, () =>
          buildWorkspaceWasm(buildContext, definition.id),
        );
        if (definition.id === "final-recheck-call-edge-deletion") {
          if (wasm.equals(controlWasm)) {
            fail("final-recheck call-edge deletion did not reach optimized WASM bytes");
          }
          binaryDifferentials += 1;
          process.stdout.write(
            `private Program supporting binary differential: ${definition.id} ` +
              `control_sha256=${sha256(controlWasm)} mutant_sha256=${sha256(wasm)}\n`,
          );
        }
        await packageAndKillMutation(packageContext, definition, wasm);
      } else {
        await packageAndKillMutation(packageContext, definition, baselineWasm);
      }
      killed += 1;
    }
    if (killed !== EXPECTED_SEMANTIC_MUTATION_COUNT) {
      fail(`semantic mutation kill count drifted: ${killed}`);
    }
    if (binaryDifferentials !== EXPECTED_BINARY_DIFFERENTIAL_COUNT) {
      fail(`supporting binary differential count drifted: ${binaryDifferentials}`);
    }
    const immutableBytesAfterProof = await readFile(immutableTarball);
    if (
      !immutableBytesAfterProof.equals(suppliedBytes) ||
      sha256(immutableBytesAfterProof) !== expectedSha256
    ) {
      fail("immutable verified baseline tarball changed during mutation proof");
    }
    runner.assertWithinDeadline("mutation receipt");
    receipt =
      `${MUTATION_RECEIPT} killed=${killed} ` +
      `binary_differentials=${binaryDifferentials}\n`;
  } catch (error) {
    failure = error;
  } finally {
    const cleanupErrors = [];
    await runner.close().catch((error) => cleanupErrors.push(error));
    await rm(root, { recursive: true, force: true }).catch((error) =>
      cleanupErrors.push(error),
    );
    if (cleanupErrors.length !== 0) {
      failure = new AggregateError(
        failure === undefined ? cleanupErrors : [failure, ...cleanupErrors],
        "private Program mutation proof or cleanup failed",
      );
    }
  }
  if (failure !== undefined) throw failure;
  if (receipt === undefined) fail("mutation receipt was not constructed");
  process.stdout.write(receipt);
}

const invokedDirectly =
  process.argv[1] !== undefined && resolve(process.argv[1]) === SCRIPT_PATH;

if (invokedDirectly) {
  Promise.resolve()
    .then(() => {
      if (process.platform !== "linux") {
        fail(
          "the full mutation executor is Linux-only so timeout cleanup can kill the whole child process group",
        );
      }
      return executeMutationProof(parseArguments(), parseMutationTimeoutPolicy());
    })
    .catch((error) => {
      process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
      process.exitCode = 1;
    });
}
