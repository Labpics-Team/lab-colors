import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const ROOT = resolve(import.meta.dirname, "../../..");
const SELF = fileURLToPath(import.meta.url);
const CLAIM_EXT = /\.(?:js|md|mjs|rs|ts)$/u;
const CLAIM_SKIP = /(?:^|\/)(?:node_modules|pkg|target|\.git)(?:\/|$)|mutants\.out/u;
const HUMAN_CLEANLINESS_VERDICTS = [
  /Закон Грязи/u,
  /Muddiness Law/u,
  /0\s*[—-]\s*чистый,\s*1\s*[—-]\s*грязный/u,
  /оценка [«"]грязи[»"]/u,
];
const WHOLE_GLOW_CLAIM =
  /(?:glow[^.!?\n]*полного результата|полного результата[^.!?\n]*glow)/iu;

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}

function mudStatusPattern(id, status) {
  return new RegExp(
    `^\\|\\s*${escapeRegExp(id)}\\s*\\|[^|]*\\|[^|]*\\|[^|]*\\|\\s*${escapeRegExp(status)}\\s*\\|`,
    "mu",
  );
}

function claimFiles(path, files = []) {
  if (!existsSync(path) || CLAIM_SKIP.test(path)) return files;
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    const child = join(path, entry.name);
    if (entry.isDirectory()) claimFiles(child, files);
    else if (CLAIM_EXT.test(entry.name)) files.push(child);
  }
  return files;
}

function knownFalseClaims(path, source) {
  const failures = [];
  // Regression laws from the false-claim cleanup: these are exact known lies,
  // not a vocabulary policy or a substitute for scientific review.
  if (/(^|[^0-9A-Fa-f])#89(?![0-9A-Fa-f])/u.test(source)) {
    failures.push(`${path}: #89 is not the Material owner`);
  }
  if (WHOLE_GLOW_CLAIM.test(source)) {
    failures.push(`${path}: point Glow evidence was promoted to a whole-effect claim`);
  }
  if (source.includes("labui-material.css")) {
    failures.push(`${path}: names a consumer that does not exist`);
  }
  if (/platform-characterized/iu.test(source)) {
    failures.push(`${path}: claims a stronger status than legacy-platform-dependent`);
  }
  return failures;
}

function publicClaimFiles() {
  return [
    ...claimFiles(join(ROOT, "crates")),
    ...claimFiles(join(ROOT, "packages", "colors")),
    ...claimFiles(join(ROOT, "docs")),
    join(ROOT, "README.md"),
    join(ROOT, "conformance", "README.md"),
    join(ROOT, "bindings", "swift", "README.md"),
  ].filter((file) => file !== SELF);
}

test("false-claim detector bites without treating hex colours as Issue links", () => {
  assert.equal(knownFalseClaims("x.md", "см. #89").length, 1);
  assert.equal(knownFalseClaims("x.md", "цвета #89CFF0 и #8944AB").length, 0);
  assert.equal(knownFalseClaims("x.md", "Glow полного результата").length, 1);
  assert.equal(
    knownFalseClaims("x.md", "Glow описан как point layer.\nполного результата здесь нет.")
      .length,
    0,
  );
  assert.equal(knownFalseClaims("x.md", "потребляет labui-material.css").length, 1);
  assert.equal(knownFalseClaims("x.md", "platform-characterized").length, 1);
});

test("cleanliness-verdict quarantine bites on every rejected public meaning", () => {
  for (const claim of [
    "Закон Грязи",
    "Muddiness Law",
    "0 — чистый, 1 — грязный",
    "оценка «грязи»",
  ]) {
    assert.equal(
      HUMAN_CLEANLINESS_VERDICTS.some((pattern) => pattern.test(claim)),
      true,
      `quarantine did not detect: ${claim}`,
    );
  }
});

test("known false Material/Glow claims stay absent from public surfaces", () => {
  const files = publicClaimFiles();
  const failures = files.flatMap((file) =>
    knownFalseClaims(relative(ROOT, file), readFileSync(file, "utf8")),
  );
  assert.deepEqual(failures, []);
});

test("public claim inventory includes the shipped Swift README", () => {
  assert.ok(publicClaimFiles().includes(join(ROOT, "bindings", "swift", "README.md")));
});

test("inventory status checks cannot be satisfied by rationale text", () => {
  const wrongStatus =
    "| M-01 | `C0` | `0.0395` | `cleanliness.rs` | cited-measured | rationale says Indeterminate provenance |";
  assert.doesNotMatch(wrongStatus, mudStatusPattern("M-01", "Indeterminate provenance"));
});

test("legacy cleanliness surfaces remain explicitly quarantined", () => {
  const surfaces = [
    "crates/labcolors-core/src/cleanliness.rs",
    "crates/labcolors-wasm/src/lib.rs",
    "crates/labcolors-ffi/src/lib.rs",
    "crates/labcolors-conformance/src/lib.rs",
    "conformance/README.md",
    "packages/colors/README.md",
    "bindings/swift/README.md",
  ];
  const publicText = surfaces
    .map((path) => {
      const source = readFileSync(join(ROOT, path), "utf8");
      assert.match(
        source,
        /experimental compatibility proxy/iu,
        `${path}: legacy name must not become a human cleanliness verdict`,
      );
      return source;
    })
    .join("\n");

  for (const forbidden of HUMAN_CLEANLINESS_VERDICTS) {
    assert.doesNotMatch(publicText, forbidden);
  }

  const inventory = readFileSync(
    join(ROOT, "docs", "empirical-inventory.md"),
    "utf8",
  );
  for (const [id, status] of [
    ["M-01", "Indeterminate provenance"],
    ["M-02", "universal JND Rejected; Indeterminate value"],
    ["M-04", "Rejected provenance; Indeterminate value"],
    ["M-05", "Rejected provenance; Indeterminate value"],
    ["M-12", "arithmetic admitted; observer claim Rejected"],
  ]) {
    assert.match(
      inventory,
      mudStatusPattern(id, status),
      `${id}: empirical inventory promoted a rejected provenance claim`,
    );
  }
  for (const id of ["M-04", "M-05"]) {
    assert.doesNotMatch(inventory, mudStatusPattern(id, "cited-measured"));
  }
});
