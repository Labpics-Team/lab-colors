// TDD RED — the JS seam must not move across the surface-shadow-tint chapter.
//
// BUG CLASS this guards: the surface-shadow-tint chapter adds a colour ROLE in
// the engine. A role addition flows through `resolveTheme`'s data (more keys in
// the role map) WITHOUT reshaping the JS entry points: `apply-theme`,
// `watch-theme`, `adapt-theme` (and their `.d.ts`) are the stable consumer seam.
// Their SHAPE — the bytes of these six files — must be byte-identical pre/post
// chapter. If the chapter quietly edits a seam file (a new export, a changed
// signature, a renamed option) this test catches it.
//
// HOW IT BITES NOW (RED): the expected hashes are pinned against a baseline
// fixture the chapter is required to commit — `test/seam-baseline.json`. That
// fixture does not exist yet, so the test fails: the chapter must (a) leave the
// six seam files byte-identical and (b) record the baseline so future chapters
// are held to it. The pinned `EXPECTED` map below is the frozen pre-chapter
// truth (captured 2026-06-21); the fixture must agree with it.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const pkgRoot = join(here, "..");

// The six files that ARE the consumer seam. Adding a role must not touch any.
const SEAM_FILES = [
  "apply-theme.js",
  "apply-theme.d.ts",
  "watch-theme.js",
  "watch-theme.d.ts",
  "adapt-theme.js",
  "adapt-theme.d.ts",
];

// Frozen pre-chapter SHA-256 of each seam file (captured 2026-06-21 on
// test/surface-shadow-tint-red). The seam must hash to exactly these after the
// chapter — a single differing byte fails the build.
const EXPECTED = {
  "apply-theme.js":
    "cfa8816474ad469ebcf40469445c821df650fc08e6d29f449a6c06aa89bbf9a8",
  "apply-theme.d.ts":
    "33fe1eb131ffeec203106a7d9721814fb6bd24f6eb4eb2c1e4cd111be35ee5bd",
  "watch-theme.js":
    "c72aa2d965cef2b9837ea50857d5c27f0ab839dae2c5ba59bc15e8d01de6af86",
  "watch-theme.d.ts":
    "0b3b2ce0242b45a2d70ad227cc99a795bf0dc17ac5cf8efe45767d2fa614f450",
  "adapt-theme.js":
    "b158a663eb2d490a8c96bd2bde7bb6585c893f550330a6efe6151d46795c99e2",
  "adapt-theme.d.ts":
    "1bbe584a504621a7dc07abd32e0a6185e74c0b74ef05a9f9cb51bc40cc47720b",
};

function sha256(absPath) {
  return createHash("sha256").update(readFileSync(absPath)).digest("hex");
}

// ONE test, so it cannot half-pass at birth: the chapter must commit a baseline
// fixture (`test/seam-baseline.json`) recording the seam hashes, AND every seam
// file must hash to that committed baseline (which must, in turn, equal the
// frozen pre-chapter truth). The fixture does not exist yet, so this bites RED on
// its absence — the byte-identity comparison only runs against the committed
// baseline, never against the inline literal alone, so there is no
// always-green check.
test("seam is held byte-identical by a committed baseline", () => {
  const fixture = join(here, "seam-baseline.json");
  assert.ok(
    existsSync(fixture),
    "surface-shadow-tint chapter must commit test/seam-baseline.json pinning the seam hashes",
  );

  const recorded = JSON.parse(readFileSync(fixture, "utf8"));

  // The committed baseline must equal the frozen pre-chapter truth — a chapter
  // cannot quietly re-baseline a moved seam.
  assert.deepEqual(
    recorded,
    EXPECTED,
    "committed seam baseline must equal the frozen pre-chapter hashes",
  );

  // Every seam file must currently hash to that committed baseline.
  for (const name of SEAM_FILES) {
    const got = sha256(join(pkgRoot, name));
    assert.equal(
      got,
      recorded[name],
      `${name} seam moved — the consumer entry-point shape must not change`,
    );
  }
});
