import assert from "node:assert/strict";
import { test } from "node:test";

import {
  isCanonicalBuildEnvOverride,
  validateCanonicalBuildEnvironment,
} from "../../../scripts/build-private-program.mjs";
import {
  hermeticBuildEnvironment,
  runCanonicalPrivateProgramBuild,
} from "../../../scripts/run-canonical-private-program-build.mjs";

// The package `test` command builds the ignored private Program artifact in a
// hermetic child process so ambient executor/build overrides exported by the
// caller workflow (e.g. dtolnay/rust-toolchain's CARGO_INCREMENTAL=0) cannot
// influence the canonical build. These tests lock the boundary law: the child
// environment drops EXACTLY the variables the canonical builder forbids and
// keeps the required toolchain pins, while the builder itself still rejects
// the same variables when it is called directly.

const FORBIDDEN_OVERRIDES = [
  "CARGO",
  "CARGO_INCREMENTAL",
  "CARGO_BUILD_JOBS",
  "cargo_home_extra",
  "RUSTC",
  "RUSTFLAGS",
  "RUSTUP_TOOLCHAIN_EXTRA",
  "rustc",
  "NODE_OPTIONS",
  "NODE_PATH",
];

const ALLOWED_AMBIENT = [
  "CARGO_HOME",
  "CARGO_TERM_COLOR",
  "RUSTUP_HOME",
  "RUST_TOOLCHAIN",
];

const REQUIRED_BUILD_VARS = [
  "BINARYEN_ROOT",
  "BINARYEN_RELEASE",
  "BINARYEN_NODE_SHA256",
  "RUSTUP_HOME",
  "CARGO_HOME",
  "PATH",
  "TMPDIR",
  "LANG",
];

test("the canonical override law classifies every ambient executor/build variable", () => {
  for (const name of FORBIDDEN_OVERRIDES) {
    assert.equal(
      isCanonicalBuildEnvOverride(name),
      true,
      `${name} must be a forbidden canonical build override`,
    );
  }
  for (const name of ALLOWED_AMBIENT) {
    assert.equal(
      isCanonicalBuildEnvOverride(name),
      false,
      `${name} must stay admitted by the canonical ambient allowlist`,
    );
  }
  for (const name of [...REQUIRED_BUILD_VARS, "HOME", "USER", "SECRET_MARKER"]) {
    assert.equal(
      isCanonicalBuildEnvOverride(name),
      false,
      `${name} must not be classified as a canonical build override`,
    );
  }
});

test("the hermetic child environment drops every forbidden override and keeps required pins", () => {
  const parent = {
    ...process.env,
    CARGO_INCREMENTAL: "0",
    CARGO_BUILD_JOBS: "8",
    RUSTFLAGS: "-C debug-assertions",
    NODE_OPTIONS: "--max-old-space-size=128",
    NODE_PATH: "/some/where",
    RUSTC: "/usr/bin/rustc",
    BINARYEN_ROOT: "/opt/binaryen-version_117",
    BINARYEN_RELEASE: "version_117",
    BINARYEN_NODE_SHA256: "2d5a42f2d167a7cc2b4b6664c44c5ace1690d13db4f527324f052afbad461a07",
    RUSTUP_HOME: "/opt/rustup",
    CARGO_HOME: "/opt/cargo",
    PATH: "/opt/node/bin:/usr/bin:/bin",
    TMPDIR: "/tmp/sandbox",
    LANG: "C",
    SECRET_MARKER: "must-not-reach-the-build",
  };
  const child = hermeticBuildEnvironment(parent);
  for (const name of FORBIDDEN_OVERRIDES) {
    assert.equal(
      Object.hasOwn(child, name),
      false,
      `${name} must not reach the hermetic child environment`,
    );
  }
  for (const name of REQUIRED_BUILD_VARS) {
    assert.equal(child[name], parent[name], `${name} must be preserved for the child build`);
  }
  assert.equal(child.SECRET_MARKER, parent.SECRET_MARKER);
});

test("the hermetic child environment satisfies the strict canonical validator", () => {
  // The parent spells out every required canonical pin explicitly instead of
  // inheriting them from the ambient process.env: the strict validator must
  // pass deterministically outside CI, regardless of which CARGO_HOME,
  // RUSTUP_HOME, RUST_TOOLCHAIN, PATH, TMPDIR, or LANG the caller machine
  // happens to export.
  const parent = {
    CARGO_INCREMENTAL: "0",
    RUSTFLAGS: "-C debug-assertions",
    NODE_OPTIONS: "--max-old-space-size=128",
    PATH: "/opt/node/bin:/usr/bin:/bin",
    CARGO_HOME: "/opt/cargo",
    RUSTUP_HOME: "/opt/rustup",
    RUST_TOOLCHAIN: "1.96.0",
    TMPDIR: "/tmp/sandbox",
    LANG: "C",
    BINARYEN_ROOT: "/opt/binaryen-version_117",
    BINARYEN_RELEASE: "version_117",
    BINARYEN_NODE_SHA256: "2d5a42f2d167a7cc2b4b6664c44c5ace1690d13db4f527324f052afbad461a07",
  };
  const child = hermeticBuildEnvironment(parent);
  assert.equal(
    validateCanonicalBuildEnvironment(child),
    true,
    "the filtered child environment must pass the canonical law unchanged — no weakening",
  );
});

test("the canonical builder still rejects forbidden overrides when called directly", () => {
  for (const name of ["CARGO_INCREMENTAL", "RUSTFLAGS", "NODE_OPTIONS", "NODE_PATH"]) {
    assert.throws(
      () => validateCanonicalBuildEnvironment({ ...process.env, [name]: "0" }),
      /canonical environment contains forbidden executor or build overrides/u,
      `the direct builder must reject ${name}`,
    );
  }
  // The positive case must not inherit the ambient environment: in CI the
  // caller workflow itself exports CARGO_INCREMENTAL, which the direct
  // builder must still reject there.
  assert.equal(
    validateCanonicalBuildEnvironment({
      PATH: process.env.PATH ?? "/usr/bin:/bin",
      CARGO_HOME: "/opt/cargo",
      RUSTUP_HOME: "/opt/rustup",
      RUST_TOOLCHAIN: "1.96.0",
      BINARYEN_ROOT: "/opt/binaryen-version_117",
      BINARYEN_RELEASE: "version_117",
      BINARYEN_NODE_SHA256:
        "2d5a42f2d167a7cc2b4b6664c44c5ace1690d13db4f527324f052afbad461a07",
      LANG: "C",
    }),
    true,
  );
});

test("the test prerequisite and the release gate share one hermetic child primitive", async () => {
  // Importing the full graph proves it is acyclic: the shared primitive,
  // the package test prerequisite, and the release gate must load together
  // without circular-import failures.
  const ensure = await import("../../../scripts/ensure-private-program-artifact.mjs");
  const verify = await import("../../../scripts/verify-package-release.mjs");
  assert.equal(typeof runCanonicalPrivateProgramBuild, "function");
  assert.equal(typeof hermeticBuildEnvironment, "function");
  assert.equal(typeof ensure.ensurePrivateProgramArtifact, "function");
  assert.equal(typeof ensure.verifiedArtifactExists, "function");
  assert.equal(typeof verify.verifyPackageRelease, "function");
});
