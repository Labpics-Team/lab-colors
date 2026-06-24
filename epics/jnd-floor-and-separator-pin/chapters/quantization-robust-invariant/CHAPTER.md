---
id: quantization-robust-invariant
epic: jnd-floor-and-separator-pin
title: "Property test: the decorative floor HOLDS after 8-bit quantization on EVERY background"
user_story: "technical enabler"
enabler_for: "floor-and-separator-pin — proves the Lc15 floor the designer relies on is robust to the 256-code sRGB lattice, not merely a continuous target that can round below 15 (the #080808 -> 14.955 cliff)."
status: ready
priority: 1
depends_on: []
refine_after: []
parallel_group: ""
agent_profile:
  category: deep
  skills: [craft-qa, dive-rust, verification-runner]
---

# Property test: the floor holds AFTER 8-bit quantization on every background

## User Story
Technical enabler for `floor-and-separator-pin`. It closes CORPUS miss #2: the decorative floor must HOLD *after* 8-bit quantization on EVERY background, not merely as a continuous target. Witness: `#080808 -> continuous Lc 14.955`, which quantizes below a continuous Lc-15 target. Without this invariant, a floor set to a continuous value that rounds down through itself on some background ships sub-perceptual — invisible-for-many-users — and no existing test catches it (the current `:2317` check uses a `DECORATIVE_FLOOR_MIN - 1.0` tolerance and a small hand-picked bg set; it is continuous + non-exhaustive).

## Purpose
Add a Class-C property test that SWEEPS every background and asserts the decorative floor (separator, the Lc-decorative role) holds AFTER the engine's 8-bit quantization — the emitted, on-grid hex measured back against the background is `>= 15.0`, OR the role is an honest `Unreachable` (a background with no headroom is not a floor violation). A continuous-Lc-15 target that quantizes below 15 on ANY swept background is a FAILURE. The test must BITE: when the floor is set to a value that quantizes below 15 on some background (e.g. a continuous 15.0 that rounds to 14.955-class on `#080808`-type backgrounds, OR the old 7.6), the test FAILS; restoring the robust floor makes it pass. Edit surface stays in semantic.rs (its `#[cfg(test)]` module) — disjoint from the sentiment region.

## Tasks
| Task | Status | Agent | Depends On |
|------|--------|-------|------------|
| `tasks/define-sweep-and-predicate.md` | ready | deep | — |
| `tasks/cover-quantization-cliff.md` | ready | deep | define-sweep-and-predicate |
| `tasks/red-proof-the-invariant.md` | ready | deep | cover-quantization-cliff |

## Exit Criteria
Given/When/Then — these ARE the acceptance criteria.
- [ ] Given a property test in `semantic.rs` `#[cfg(test)]`, When it sweeps a representative-or-exhaustive set of 8-bit backgrounds, Then for each it asserts: the emitted (post-quantization, on-grid) decorative/separator colour measures |Lc| `>= 15.0` against that background, OR the role is `Unreachable` (honest no-headroom), never a silent sub-floor emission.
- [ ] Given the `#080808` cliff case, When swept, Then it is covered and the assertion distinguishes "continuous 14.955 < 15" from "robust" — i.e. the test is quantization-robust, not continuous-only.
- [ ] Given the predicate broken (floor set to a continuous target that quantizes below 15 on some background, e.g. the old 7.6 or a naive continuous 15.0), When the test runs, Then it FAILS (RED-proven, evidence recorded); restoring the robust floor makes it PASS (GREEN).
- [ ] Given the test class, When reviewed, Then it asserts a PROPERTY over the whole background class (Class C), not `test(#080808)` alone — a single-input regression test does NOT satisfy this criterion.
- [ ] Given the diff, When file paths are listed, Then only `semantic.rs` (test module) changed; sentiment/`S_PERC_MIN`/`RoleChroma`, solve.rs, neutral.rs untouched.
