import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";
import { createWasmMeasurement } from "../../../scripts/record-wasm-measurement.mjs";
import {
  createWasmBuildEnvironment, createWasmBuildReceipt, expectedWasmCargoProfile, wasmCargoProfileArgs,
} from "../../../scripts/build-wasm.mjs";

const budget = JSON.parse(readFileSync(new URL("../bench/wasm.json", import.meta.url)));
const workflow = readFileSync(new URL("../../../.github/workflows/package-worker.yml", import.meta.url), "utf8");
const sourceSha = "1".repeat(40);

function fixture() {
  const runtime = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
  const input = {
    first: Buffer.from(runtime), runtime, sourceSha, budget: structuredClone(budget), workflow,
    tools: { platform: "linux-x64", ...budget.toolchain },
    inputs: { "Cargo.lock": "2".repeat(64) },
    env: {
      GITHUB_SHA: sourceSha, GITHUB_RUN_ID: "123", GITHUB_RUN_ATTEMPT: "2", GITHUB_JOB: "wasm",
      GITHUB_REPOSITORY: "Labpics-Team/lab-colors", GITHUB_EVENT_NAME: "pull_request",
      GITHUB_REF: "refs/pull/1/merge", GITHUB_WORKFLOW_REF: "Labpics-Team/lab-colors/.github/workflows/pr.yml@refs/pull/1/merge",
      GITHUB_WORKFLOW_SHA: sourceSha, GITHUB_SERVER_URL: "https://github.com",
      GITHUB_WORKSPACE: "/checkout", CARGO_HOME: "/cargo",
    },
  };
  input.build = createWasmBuildReceipt({ messages: cargoMessages(), runtime, budget: input.budget,
    sourceSha, inputs: input.inputs, tools: input.tools });
  input.firstBuild = structuredClone(input.build);
  return input;
}

function cargoMessages(profile = expectedWasmCargoProfile(budget)) {
  return [
    "wasm-pack progress",
    JSON.stringify({ reason: "compiler-artifact", target: { name: "labcolors_wasm", crate_types: ["cdylib"] },
      filenames: ["/target/wasm32-unknown-unknown/release/labcolors_wasm.wasm"], profile, fresh: true }),
    JSON.stringify({ reason: "build-finished", success: true }),
  ].join("\n");
}

test("measurement preserves reproduced bytes and source/run identity without admitting a release", () => {
  const input = fixture();
  const receipt = createWasmMeasurement(input);
  assert.equal(receipt.sourceSha, sourceSha);
  assert.equal(receipt.run.attempt, "2");
  assert.equal(receipt.run.url, "https://github.com/Labpics-Team/lab-colors/actions/runs/123/attempts/2");
  assert.equal(receipt.artifact.bytes, input.runtime.length);
  assert.equal(receipt.artifact.sha256, createHash("sha256").update(input.runtime).digest("hex"));
  assert.equal(receipt.qualification, "measurement-only");
  assert.deepEqual(receipt.reproducibility, { builds: 2, byteIdentical: true, scope: "runtime-wasm" });
  assert.deepEqual(receipt.build.profile, input.build.profile);
  assert.equal(receipt.build.receiptSha256, createHash("sha256").update(JSON.stringify(input.build)).digest("hex"));
});

test("an over-budget canonical build remains downloadable as a failed-budget measurement", () => {
  const input = fixture();
  input.budget.measurement.rawBytes = 1;
  input.budget.policy.maxRawBytes = 1;
  const receipt = createWasmMeasurement(input);
  assert.equal(receipt.budget.passed, false);
  assert.equal(receipt.artifact.bytes, 8);
  assert.equal(receipt.budget.maxRawBytes, 1);
});

test("different builds, source, recipe, tools and non-CI contexts cannot produce an admitted measurement", () => {
  const changes = [
    [(input) => { input.first = Buffer.concat([input.first, Buffer.from([0])]); }, /differs between/u],
    [(input) => { input.runtime = Buffer.alloc(8); }, /not a WebAssembly/u],
    [(input) => { input.env.GITHUB_SHA = "2".repeat(40); }, /source differs/u],
    [(input) => { delete input.env.GITHUB_RUN_ATTEMPT; }, /GITHUB_RUN_ATTEMPT/u],
    [(input) => { input.tools.platform = "darwin-arm64"; }, /canonical CI platform/u],
    [(input) => { input.tools.rustcCommit = "0".repeat(40); }, /rustcCommit/u],
    [(input) => { input.tools.binaryenNodeArchiveSha256 = "0".repeat(64); }, /binaryenNodeArchiveSha256/u],
    [(input) => { input.workflow = input.workflow.replace('node "$GITHUB_WORKSPACE/scripts/build-wasm.mjs" "$PWD"', "wasm-pack build"); }, /build recipe/u],
    [(input) => { delete input.build; }, /WASM build receipt/u],
    [(input) => { delete input.firstBuild; }, /build receipts differ/u],
    [(input) => { input.inputs["Cargo.lock"] = "3".repeat(64); }, /build receipts differ/u],
    [(input) => { input.build.artifact.sha256 = "0".repeat(64); input.firstBuild = structuredClone(input.build); }, /receipt differs from runtime/u],
    [(input) => { input.build.sourceSha = "2".repeat(40); input.firstBuild = structuredClone(input.build); }, /different source/u],
    [(input) => { input.build.profile.opt_level = "2"; input.firstBuild = structuredClone(input.build); }, /measured Cargo profile/u],
    [(input) => { input.build.recipe.cargoProfile["opt-level"] = 2; input.firstBuild = structuredClone(input.build); }, /declared recipe/u],
    [(input) => { input.build.tools.node = "0.0.0"; input.firstBuild = structuredClone(input.build); }, /measured node|built node/u],
  ];
  for (const [edit, reason] of changes) {
    const input = fixture();
    edit(input);
    assert.throws(() => createWasmMeasurement(input), reason);
  }
});

test("one WASM recipe supplies every Cargo profile argument without changing native manifests", () => {
  const args = wasmCargoProfileArgs(budget);
  assert.equal(args.length, Object.keys(budget.recipe.cargoProfile).length * 2);
  assert.ok(args.includes('profile.release.opt-level="s"'));
  const changed = structuredClone(budget);
  changed.recipe.cargoProfile["opt-level"] = 2;
  assert.ok(wasmCargoProfileArgs(changed).includes("profile.release.opt-level=2"));
  assert.match(readFileSync(new URL("../../../Cargo.toml", import.meta.url), "utf8"), /\[profile\.release\]\s+opt-level = 2/u);
});

test("build paths follow the requested source root while toolchain selection follows the recipe", () => {
  const ambient = { PATH: "/tools", CARGO_HOME: "/cargo", GITHUB_WORKSPACE: "/head", RUSTUP_TOOLCHAIN: "stable",
    RUSTC: "/other/rustc", RUSTC_WRAPPER: "sccache", RUSTC_WORKSPACE_WRAPPER: "/tools/workspace-wrapper" };
  const before = structuredClone(ambient);
  const actual = createWasmBuildEnvironment(budget, "/base", ambient);
  assert.equal(actual.GITHUB_WORKSPACE, resolve("/base"));
  assert.equal(actual.RUSTUP_TOOLCHAIN, budget.toolchain.rust);
  assert.equal(actual.CARGO_ENCODED_RUSTFLAGS,
    `--remap-path-prefix=${resolve("/base")}=/workspace/lab-colors\x1f--remap-path-prefix=${resolve("/cargo")}=/cargo-home`);
  assert.equal(actual.RUSTC, "rustc");
  assert.equal(actual.RUSTC_WRAPPER, ambient.RUSTC_WRAPPER);
  assert.equal(actual.RUSTC_WORKSPACE_WRAPPER, ambient.RUSTC_WORKSPACE_WRAPPER);
  assert.equal(actual.CARGO_INCREMENTAL, "0");
  assert.deepEqual(ambient, before);
});

test("the build owns its flags and incremental setting without rejecting ordinary Cargo environments", () => {
  for (const name of ["CARGO_PROFILE_RELEASE_OPT_LEVEL", "CARGO_PROFILE_RELEASE_LTO",
    "CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_OPT_LEVEL", "CARGO_PROFILE_DEV_DEBUG", "cargo_profile_release_opt_level",
    "CARGO_INCREMENTAL", "CARGO_BUILD_INCREMENTAL", "CARGO_BUILD_RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_BUILD_RUSTC", "CARGO_BUILD_RUSTC_WRAPPER", "CARGO_BUILD_TARGET", "RUSTFLAGS", "RUSTC",
    "RUSTC_BOOTSTRAP", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
    "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS", "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER"]) {
    const actual = createWasmBuildEnvironment(budget, "/checkout", { [name]: "external" });
    assert.equal(actual.RUSTC, "rustc", name);
    assert.equal(actual.CARGO_INCREMENTAL, "0", name);
    assert.match(actual.CARGO_ENCODED_RUSTFLAGS, /^--remap-path-prefix=/u, name);
  }
  const ambient = { CARGO_BUILD_JOBS: "2", CARGO_TARGET_DIR: "/cache", CARGO_NET_OFFLINE: "true",
    CARGO_PROFILE_DEV_DEBUG: "true", CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER: "clang",
    HTTPS_PROXY: "http://proxy.test", CARGO_PROFILE_RELEASE_OPT_LEVEL: "2" };
  const actual = createWasmBuildEnvironment(budget, "/checkout", ambient);
  for (const [name, value] of Object.entries(ambient)) assert.equal(actual[name], value, name);
  const changed = structuredClone(budget);
  changed.recipe.cargoProfile.incremental = true;
  assert.equal(createWasmBuildEnvironment(changed, "/checkout", { CARGO_INCREMENTAL: "0" }).CARGO_INCREMENTAL, "1");
});

test("controlled Windows environment spellings cannot compete with the build recipe", () => {
  const actual = createWasmBuildEnvironment(budget, "/checkout", {
    cargo_home: "/cargo", rustc: "other", cargo_incremental: "1", cargo_encoded_rustflags: "other",
    rustup_toolchain: "nightly", github_workspace: "/other",
  });
  assert.equal(actual.CARGO_HOME, resolve("/cargo"));
  assert.equal(actual.RUSTC, "rustc");
  assert.equal(actual.CARGO_INCREMENTAL, "0");
  assert.equal(actual.RUSTUP_TOOLCHAIN, budget.toolchain.rust);
  assert.ok(Object.keys(actual).every((name) => name === name.toUpperCase()));
});

test("measurement checks the recorded build while ambient settings are normalized by the builder", () => {
  const input = fixture();
  input.env.CARGO_ENCODED_RUSTFLAGS = "-Ctarget-cpu=native";
  input.env.CARGO_PROFILE_RELEASE_OPT_LEVEL = "2";
  input.env.CARGO_INCREMENTAL = "1";
  assert.doesNotThrow(() => createWasmMeasurement(input));
});

test("build receipts require the actual Cargo profile and a completed product artifact", () => {
  const input = fixture();
  assert.throws(() => createWasmBuildReceipt({ ...input, messages: cargoMessages({ ...input.build.profile, opt_level: "2" }) }),
    /actual Cargo profile/u);
  for (const messages of ["", cargoMessages().replace('"success":true', '"success":false'),
    cargoMessages().replace('"name":"labcolors_wasm"', '"name":"other"'),
    cargoMessages() + "\n" + cargoMessages()]) {
    assert.throws(() => createWasmBuildReceipt({ ...input, messages }), /WASM|Cargo/u);
  }
  assert.doesNotThrow(() => createWasmBuildReceipt({ ...input, messages: cargoMessages().replaceAll("\n", "\r\n") }));
});

test("Cargo's null debuginfo has the same documented meaning as level zero", () => {
  const input = fixture();
  const receipt = createWasmBuildReceipt({ ...input,
    messages: cargoMessages({ ...input.build.profile, debuginfo: null }) });
  assert.deepEqual(receipt.profile, input.build.profile);
});

test("a runtime dependency with an overridden profile is rejected while host build tools retain their profile", () => {
  const input = fixture();
  const dependency = { reason: "compiler-artifact", target: { name: "dependency", crate_types: ["lib"] },
    filenames: ["C:\\target\\wasm32-unknown-unknown\\release\\deps\\dependency.rlib"],
    profile: { ...input.build.profile, opt_level: "2" } };
  assert.throws(() => createWasmBuildReceipt({ ...input,
    messages: `${JSON.stringify(dependency)}\n${cargoMessages()}` }), /actual Cargo profile.*dependency/u);
  dependency.filenames = ["C:\\target\\release\\deps\\dependency.rlib"];
  assert.doesNotThrow(() => createWasmBuildReceipt({ ...input,
    messages: `${JSON.stringify(dependency)}\n${cargoMessages()}` }));
});

test("a valid WASM header cannot admit a malformed runtime measurement", () => {
  const input = fixture();
  input.runtime = Buffer.concat([input.runtime, Buffer.from([0])]);
  input.first = Buffer.from(input.runtime);
  input.build.artifact = { bytes: input.runtime.length, sha256: createHash("sha256").update(input.runtime).digest("hex") };
  input.firstBuild = structuredClone(input.build);
  assert.throws(() => createWasmMeasurement(input), /not a WebAssembly binary/u);
});

test("CRLF checkouts preserve the canonical workflow recipe", () => {
  const input = fixture();
  input.workflow = workflow.replaceAll("\r\n", "\n").replaceAll("\n", "\r\n");
  assert.doesNotThrow(() => createWasmMeasurement(input));
});
