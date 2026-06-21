// TDD RED — the v2-governance-rescue durability + hygiene contract.
//
// WHAT THIS GUARDS (the CLASS, not one file): load-bearing governance that
// survives only in unreachable-by-tip git objects. The surface-JND ADR — cited
// by every downstream scope (jnd-floor-and-separator-pin, shadow-ramp-derivation)
// — currently exists ONLY as the untracked file set inside `stash@{0}` (tree
// `eb7ec00`), reachable today but on no branch tip: a `git gc --prune` /
// `git stash clear` erases the entire decision framework with zero warning. The
// same class includes the silent uncommitted RED scaffold and the forbidden
// 731-line `semantic.rs` stash edit that must NEVER be pulled. These tests pin
// the rescue's end-state BEFORE the governance work exists, so each must fail RED
// for the RIGHT reason — the branch / file / index / PR / isolation is MISSING,
// not a harness crash.
//
// HOW THEY BITE NOW: `feat/v2-governance` does not exist, the ADR skeleton is not
// committed, `docs/decisions/README.md` is absent, the RED scaffold is
// uncommitted, and no PR exists. Every assertion below names the precise missing
// governance artifact. When ch01-rescue-and-pin lands (branch created, ADR
// skeleton + index committed, RED parked on test/surface-shadow-tint-red, PR
// opened with the accounting + framing correction), these turn green — proving
// the durability operation actually happened, never a hand-waved "done".
//
// DESIGN NOTE ON HONEST RED: every `git`/`gh` invocation goes through a helper
// that CAPTURES failure into `{ ok, code, stdout, stderr }` instead of throwing.
// A missing branch therefore surfaces as a falsy assertion with a clear message
// (genuine behavioural RED), never an uncaught child-process throw masquerading
// as a harness/import error.

import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
// packages/colors/test -> repo root is three levels up.
const REPO_ROOT = join(here, "..", "..", "..");

// ----- topology constants (single source of truth) -----------------------------

const GOVERNANCE_BRANCH = "feat/v2-governance";
const RED_BRANCH = "test/surface-shadow-tint-red";
const MAIN = "main";

const ADR_PATH = "docs/decisions/surface-jnd.md";
const INDEX_PATH = "docs/decisions/README.md";

// The two existing tracked ADRs the index must reference alongside surface-jnd.
const EXISTING_ADRS = [
  "docs/decisions/apca-license.md",
  "docs/decisions/theme-invariant.md",
];
const ALL_THREE_ADRS = [ADR_PATH, ...EXISTING_ADRS];

// The four in-flight RED scaffold files that must be PARKED on the RED branch and
// ABSENT from the green governance branch (branch isolation).
const RED_SCAFFOLD_FILES = [
  "crates/labcolors-core/tests/surface_shadow_tint.rs",
  "crates/labcolors-wasm/tests/wasm_parity.rs",
  "packages/colors/test/full-ci-gate.smoke.test.mjs",
  "packages/colors/test/seam-stability.test.mjs",
];

// The forbidden source-code change: the stash index commit carries a 731-line
// edit to semantic.rs. Its blob must appear in NEITHER branch's tree.
const STASH_REF = "stash@{0}";
const SEMANTIC_PATH = "crates/labcolors-core/src/semantic.rs";

// ----- failure-capturing process helpers ---------------------------------------

function git(args, opts = {}) {
  const res = spawnSync("git", args, {
    cwd: REPO_ROOT,
    encoding: "utf8",
    ...opts,
  });
  return {
    ok: res.status === 0,
    code: res.status,
    stdout: (res.stdout ?? "").trim(),
    stderr: (res.stderr ?? "").trim(),
  };
}

function gh(args) {
  const res = spawnSync("gh", args, { cwd: REPO_ROOT, encoding: "utf8" });
  return {
    ok: res.status === 0,
    code: res.status,
    stdout: (res.stdout ?? "").trim(),
    stderr: (res.stderr ?? "").trim(),
    spawnErr: res.error ? String(res.error.message ?? res.error) : null,
  };
}

/** True iff a local branch ref exists, without throwing on absence. */
function branchExists(branch) {
  return git(["rev-parse", "--verify", "--quiet", `refs/heads/${branch}`]).ok;
}

/** The blob the given branch tree holds for `path`, or null if unreachable. */
function blobAt(branch, path) {
  const r = git(["rev-parse", `${branch}:${path}`]);
  return r.ok ? r.stdout : null;
}

/** The PR body for the governance branch, or null if no PR / gh unavailable. */
function governancePrBody() {
  const r = gh([
    "pr",
    "view",
    GOVERNANCE_BRANCH,
    "--json",
    "body",
    "--jq",
    ".body",
  ]);
  return r.ok ? r.stdout : null;
}

// ----- t-adr-tracked-on-real-branch --------------------------------------------

// CONTRACT — the ADR is reachable from a real branch tip, not stash/orphan-only.
// This is the core durability contract: `git cat-file -e <branch>:<adr>` must
// succeed, which is only true once the ADR is committed onto a branch a `git gc`
// cannot prune.
//
// RED REASON: `feat/v2-governance` does not exist yet, so `cat-file -e
// feat/v2-governance:docs/decisions/surface-jnd.md` returns non-zero (the object
// name cannot be resolved) — the ADR is still stash-only.
test("adr-tracked-on-real-branch", () => {
  assert.ok(
    branchExists(GOVERNANCE_BRANCH),
    `${GOVERNANCE_BRANCH} must exist as a real local branch (durability requires a tip, not a stash)`,
  );
  const res = git(["cat-file", "-e", `${GOVERNANCE_BRANCH}:${ADR_PATH}`]);
  assert.ok(
    res.ok,
    `${ADR_PATH} must be reachable from ${GOVERNANCE_BRANCH} tip — git gc must not be able to erase it (cat-file rc=${res.code}: ${res.stderr})`,
  );
});

// ----- t-decisions-index-present-and-references-all ----------------------------

// CONTRACT — a decisions index exists on the governance branch and references all
// THREE ADRs, with the surface-jnd ADR's derived magnitudes flagged TBD. Locks the
// "index references the ADR alongside the two existing ADRs" criterion.
//
// RED REASON: `feat/v2-governance` (hence its README.md) does not exist, so
// `cat-file -e` fails and the index content cannot reference anything yet.
test("decisions-index-present-and-references-all", () => {
  assert.ok(
    branchExists(GOVERNANCE_BRANCH),
    `${GOVERNANCE_BRANCH} must exist before its decisions index can be checked`,
  );

  const exists = git(["cat-file", "-e", `${GOVERNANCE_BRANCH}:${INDEX_PATH}`]);
  assert.ok(
    exists.ok,
    `${INDEX_PATH} must be committed on ${GOVERNANCE_BRANCH} (cat-file rc=${exists.code})`,
  );

  const show = git(["show", `${GOVERNANCE_BRANCH}:${INDEX_PATH}`]);
  assert.ok(show.ok, `must read ${INDEX_PATH} from ${GOVERNANCE_BRANCH}`);
  const body = show.stdout;

  for (const adr of ALL_THREE_ADRS) {
    const basename = adr.split("/").pop();
    assert.ok(
      body.includes(basename) || body.includes(adr),
      `decisions index must reference ${basename}`,
    );
  }

  assert.ok(
    /TBD/.test(body) || /surface-jnd/.test(body),
    "index must reference the surface-jnd ADR (whose derived magnitudes stay flagged TBD)",
  );
});

// ----- t-adr-skeleton-has-section0-and-section3-hardblocker --------------------

// SMOKE — the rescued ADR is a SKELETON: it ships §0 (the metric the contract is
// written in) and §3 (the shadow section) with an explicit HARD BLOCKER line, and
// it leaves the derived magnitudes as TBD placeholders — it does NOT smuggle in
// ratified science (floor 15.0 / separator pin / shadow ramp steps as final).
//
// RED REASON: the ADR is not on `feat/v2-governance` at all yet, so reading it
// from that branch fails and none of the required sections/markers are present.
test("adr-skeleton-has-section0-and-section3-hardblocker", () => {
  assert.ok(
    branchExists(GOVERNANCE_BRANCH),
    `${GOVERNANCE_BRANCH} must exist before the ADR skeleton can be inspected`,
  );

  const show = git(["show", `${GOVERNANCE_BRANCH}:${ADR_PATH}`]);
  assert.ok(
    show.ok,
    `ADR must be readable from ${GOVERNANCE_BRANCH} (show rc=${show.code}: ${show.stderr})`,
  );
  const adr = show.stdout;

  // §0 — the metric section.
  assert.match(
    adr,
    /(^|\n)#+\s*0\.|##\s*0\b|section\s*0/i,
    "ADR must carry a §0 metric section (the unit the decorative contract is written in)",
  );

  // §3 — the shadow section with an explicit HARD BLOCKER line.
  assert.match(
    adr,
    /(^|\n)#+\s*3\.|##\s*3\b/,
    "ADR must carry a §3 shadow section",
  );
  assert.match(
    adr,
    /HARD\s*BLOCK(ER|ED)/i,
    "§3 must contain an explicit HARD BLOCKER line for the shadow ramp",
  );

  // The skeleton ratifies NO science: derived magnitudes stay TBD placeholders.
  assert.match(
    adr,
    /TBD/,
    "the ADR skeleton must leave derived magnitudes as TBD placeholders (this epic ratifies no science)",
  );
});

// ----- t-governance-branch-touches-no-rust -------------------------------------

// REGRESSION — the governance branch is docs+topology only. `git diff --name-only
// main..feat/v2-governance` must list ONLY docs/decisions/** paths; any crates/**
// path is a red flag that the agent touched code (behaviour-neutral invariant).
//
// RED REASON: `feat/v2-governance` does not exist, so the diff cannot be computed
// (non-zero) — the branch that must prove behaviour-neutrality is absent.
test("governance-branch-touches-no-rust", () => {
  assert.ok(
    branchExists(GOVERNANCE_BRANCH),
    `${GOVERNANCE_BRANCH} must exist before its diff vs ${MAIN} can be checked`,
  );

  const diff = git(["diff", "--name-only", `${MAIN}..${GOVERNANCE_BRANCH}`]);
  assert.ok(
    diff.ok,
    `git diff ${MAIN}..${GOVERNANCE_BRANCH} must succeed (rc=${diff.code}: ${diff.stderr})`,
  );

  const changed = diff.stdout.split("\n").filter(Boolean);
  assert.ok(
    changed.length > 0,
    "governance branch must actually add the ADR + index (an empty diff means nothing was rescued)",
  );

  const offenders = changed.filter((p) => !p.startsWith("docs/decisions/"));
  assert.deepEqual(
    offenders,
    [],
    `governance branch must touch ONLY docs/decisions/** — stray paths: ${offenders.join(", ")}`,
  );

  const rust = changed.filter((p) => p.startsWith("crates/"));
  assert.deepEqual(
    rust,
    [],
    `no crates/** path may appear on the governance branch — touched: ${rust.join(", ")}`,
  );
});

// ----- t-ci-gate-green-on-governance-branch ------------------------------------

// SMOKE — the chapter gate runs `cargo fmt --all --check && cargo clippy
// --workspace --all-targets -- -D warnings && cargo test --workspace` on
// `feat/v2-governance` and it is GREEN: docs do not break the build. A failure
// means code was accidentally modified. This smoke pins the cheap structural
// precondition for that gate — the governance branch exists, equals main on all
// Rust source (so the build is provably identical to a known-green main), and
// adds nothing under crates/. It deliberately does NOT shell out to a multi-minute
// cargo run; it proves the build CANNOT have changed by proving no Rust moved.
//
// RED REASON: `feat/v2-governance` does not exist, so neither the branch nor its
// rust-tree-equality to main can be established — the gate's precondition is unmet.
test("ci-gate-green-on-governance-branch", () => {
  assert.ok(
    branchExists(GOVERNANCE_BRANCH),
    `${GOVERNANCE_BRANCH} must exist for the CI gate to run on it`,
  );

  // The whole crates/ tree on the governance branch must be byte-identical to
  // main: if no Rust source differs, `cargo fmt/clippy/test --workspace` produces
  // exactly main's (known-green) result. This is the honest, deterministic proxy
  // for "the gate is green and no code was touched".
  const diff = git([
    "diff",
    "--name-only",
    `${MAIN}..${GOVERNANCE_BRANCH}`,
    "--",
    "crates/",
  ]);
  assert.ok(
    diff.ok,
    `git diff for crates/ must succeed (rc=${diff.code}: ${diff.stderr})`,
  );
  assert.equal(
    diff.stdout,
    "",
    `the governance branch must leave crates/ identical to ${MAIN} so the cargo gate stays green; changed: ${diff.stdout}`,
  );
});

// ----- t-red-scaffold-parked-not-in-green-branch -------------------------------

// CONTRACT — the four scaffold files are committed on `test/surface-shadow-tint-red`
// (with the GREEN-owner tracking note) AND absent from `feat/v2-governance`. Locks
// branch isolation so the RED never reddens the green branch.
//
// RED REASON: the scaffold is still UNCOMMITTED in the working tree (not on the
// RED branch's tip), and `feat/v2-governance` does not exist — so neither the
// "parked on RED" nor the "absent on governance" half can be satisfied yet.
test("red-scaffold-parked-not-in-green-branch", () => {
  assert.ok(
    branchExists(RED_BRANCH),
    `${RED_BRANCH} must exist to hold the parked scaffold`,
  );
  assert.ok(
    branchExists(GOVERNANCE_BRANCH),
    `${GOVERNANCE_BRANCH} must exist to prove the scaffold is absent from it`,
  );

  for (const f of RED_SCAFFOLD_FILES) {
    const onRed = git(["cat-file", "-e", `${RED_BRANCH}:${f}`]);
    assert.ok(
      onRed.ok,
      `scaffold ${f} must be COMMITTED on ${RED_BRANCH} (cat-file rc=${onRed.code})`,
    );

    const onGov = git(["cat-file", "-e", `${GOVERNANCE_BRANCH}:${f}`]);
    assert.ok(
      !onGov.ok,
      `scaffold ${f} must be ABSENT from ${GOVERNANCE_BRANCH} (branch isolation; it must not redden the green branch)`,
    );
  }

  // The GREEN-owner tracking note must travel with the parked scaffold.
  const noteShow = git([
    "show",
    `${RED_BRANCH}:crates/labcolors-core/tests/surface_shadow_tint.rs`,
  ]);
  assert.ok(noteShow.ok, "parked scaffold must be readable on the RED branch");
  assert.match(
    noteShow.stdout,
    /RED/,
    "the parked scaffold must carry its RED / GREEN-owner tracking note",
  );
});

// ----- t-no-uncommitted-ghosts-remain ------------------------------------------

// SMOKE — after the rescue, `git status --porcelain` is empty on BOTH branches:
// the no-silent-ghost invariant that motivated the whole epic. Every artifact is
// either committed to a tip or consciously discarded — no third "lost in stash"
// state.
//
// RED REASON: right now the working tree is DIRTY (the uncommitted RED scaffold +
// the surface_shadow_tint.rs / wasm_parity.rs / *.mjs ghosts), so
// `git status --porcelain` is non-empty — the ghost the epic exists to remove is
// still present.
test("no-uncommitted-ghosts-remain", () => {
  // We assert the CURRENT checkout is clean. The rescue must leave whichever
  // branch is checked out ghost-free; a dirty tree here is exactly the unsaved
  // ghost the epic must eliminate.
  const status = git(["status", "--porcelain"]);
  assert.ok(status.ok, "git status must run");
  assert.equal(
    status.stdout,
    "",
    `working tree must have zero uncommitted ghosts after the rescue; still dirty:\n${status.stdout}`,
  );
});

// ----- t-semantic-edit-not-pulled ----------------------------------------------

// DIFFERENTIAL — the stash's 731-line `semantic.rs` edit appears in NEITHER
// branch's tree. The forbidden source-change guard: extraction must be via
// `git restore --source=eb7ec00 -- <docs>`, never `git stash pop/apply` (which
// would drag this blob in). We compare the stash blob id against each branch's
// blob id for semantic.rs — they must DIFFER (each branch keeps main's blob).
//
// RED REASON: `feat/v2-governance` does not exist, so its semantic.rs blob cannot
// be resolved — the guard cannot yet be satisfied for the branch that must prove
// it never pulled the forbidden edit.
test("semantic-edit-not-pulled", () => {
  const forbidden = git(["rev-parse", `${STASH_REF}:${SEMANTIC_PATH}`]);
  assert.ok(
    forbidden.ok,
    `must resolve the forbidden stash blob ${STASH_REF}:${SEMANTIC_PATH} (rc=${forbidden.code})`,
  );
  const forbiddenBlob = forbidden.stdout;

  const mainBlob = blobAt(MAIN, SEMANTIC_PATH);
  assert.ok(mainBlob, "main must have a semantic.rs blob");
  assert.notEqual(
    forbiddenBlob,
    mainBlob,
    "sanity: the forbidden stash edit must differ from main (it is the +731-line change)",
  );

  for (const branch of [GOVERNANCE_BRANCH, RED_BRANCH]) {
    assert.ok(
      branchExists(branch),
      `${branch} must exist to prove it never pulled the forbidden semantic.rs edit`,
    );
    const blob = blobAt(branch, SEMANTIC_PATH);
    assert.ok(blob, `${branch} must have a semantic.rs blob`);
    assert.notEqual(
      blob,
      forbiddenBlob,
      `${branch} must NOT carry the forbidden 731-line semantic.rs edit (it must keep main's blob)`,
    );
    assert.equal(
      blob,
      mainBlob,
      `${branch} semantic.rs must equal main's blob (no source change on either branch)`,
    );
  }
});

// ----- t-pr-body-enumerates-all-buckets-and-corrects-framing -------------------

// CONTRACT — the PR body is the accounting deliverable. It must enumerate every
// artifact bucket — RESCUED (the ADR), PARKED (the RED scaffold), KEEP (the
// cusp_relative verdict), NOT-pulled (semantic.rs), epics-location (only in
// .agents/) — PLUS the explicit "clean main" correction (HEAD == main tip, tree
// was dirty). Locks the accounting + stale-framing-correction deliverables.
//
// RED REASON: no PR exists for `feat/v2-governance` yet (the branch is not even
// created), so `gh pr view` returns nothing and not a single required marker is
// present.
test("pr-body-enumerates-all-buckets-and-corrects-framing", () => {
  const body = governancePrBody();
  assert.ok(
    body !== null && body.length > 0,
    `a PR for ${GOVERNANCE_BRANCH} must exist with a non-empty body (the accounting deliverable)`,
  );

  const requiredMarkers = [
    { name: "RESCUED bucket", re: /RESCUED/i },
    { name: "PARKED bucket", re: /PARKED/i },
    { name: "KEEP / cusp_relative verdict", re: /cusp_relative/i },
    { name: "NOT-pulled semantic.rs guard", re: /semantic\.rs/i },
    { name: "epics-location note (.agents)", re: /\.agents/i },
    {
      name: "clean-main framing correction (HEAD == main tip, tree dirty)",
      re: /clean[\s-]*main|HEAD\s*==\s*main|f21aac7|dirty/i,
    },
  ];

  const missing = requiredMarkers
    .filter((m) => !m.re.test(body))
    .map((m) => m.name);

  assert.deepEqual(
    missing,
    [],
    `PR body must enumerate all buckets + correct the stale framing; missing: ${missing.join("; ")}`,
  );
});
