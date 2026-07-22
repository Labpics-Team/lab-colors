#!/usr/bin/env python3
"""Independent proof of the point-support Q55 retained-surplus law.

The verifier deliberately does not import production Rust. It binds the exact
whole-file private semantic cone, consumes the already proved WCAG Q55 artifact
as an immutable dependency, and replays the rational law with Python big integers.
"""

from __future__ import annotations

import hashlib
import json
import random
import struct
import sys
from fractions import Fraction
from pathlib import Path

if not __debug__:
    raise RuntimeError("point-support proof verifier requires Python assertions")


REPO_ROOT = Path(__file__).resolve().parents[1]
POINT_SOURCE = REPO_ROOT / "crates/labcolors-core/src/point_support.rs"
OBSERVATION_SOURCE = REPO_ROOT / "crates/labcolors-core/src/observation.rs"
NUMERICS_SOURCE = REPO_ROOT / "crates/labcolors-core/src/numerics.rs"
SESSION_SOURCE = REPO_ROOT / "crates/labcolors-core/src/session.rs"
COMPOSITION_SOURCE = REPO_ROOT / "crates/labcolors-core/src/composition.rs"
APPEARANCE_SOURCE = REPO_ROOT / "crates/labcolors-core/src/appearance.rs"
CONSTRAINTS_SOURCE = REPO_ROOT / "crates/labcolors-core/src/constraints/mod.rs"
EXACT_CONSTRAINT_SOURCE = REPO_ROOT / "crates/labcolors-core/src/constraints/exact.rs"
WCAG22_CONSTRAINT_SOURCE = REPO_ROOT / "crates/labcolors-core/src/constraints/wcag22.rs"
WCAG22_SOURCE = REPO_ROOT / "crates/labcolors-core/src/wcag22.rs"
WCAG22_KERNEL_SOURCE = REPO_ROOT / "crates/labcolors-core/src/wcag22/kernel.rs"
WCAG22_Q55_DATA_SOURCE = REPO_ROOT / "crates/labcolors-core/src/wcag22/q55_data.rs"
WCAG22_EVIDENCE_SOURCE = REPO_ROOT / "crates/labcolors-core/src/wcag22_evidence.rs"
SRGB8_SOURCE = REPO_ROOT / "crates/labcolors-core/src/srgb8.rs"
HASH_SOURCE = REPO_ROOT / "crates/labcolors-core/src/hash.rs"
LIB_SOURCE = REPO_ROOT / "crates/labcolors-core/src/lib.rs"
WCAG22_PROFILE_SOURCE = REPO_ROOT / "crates/labcolors-core/contracts/wcag22-srgb8-v1.json"
Q55_PROOF = (
    REPO_ROOT / "crates/labcolors-core/contracts/wcag22-srgb8-q55-proof-v1.json"
)
PROOF_PATH = (
    REPO_ROOT
    / "crates/labcolors-core/contracts/point-support-reference-surplus-q55-bps-proof-v1.json"
)
VERIFIER_PATH = Path(__file__).resolve()

SCHEMA_VERSION = 2
SOURCE_BINDING_SCHEMA_VERSION = 2
PROFILE_ID = "srgb8-q55-retained-reference-surplus-bps-v1"
BOUND_ID = "point-support-reference-surplus-q55-bps-v1"
PROOF_ID = "point-support-reference-surplus-integer-v1"
ARTIFACT_ID = "wcag22-srgb8-luminance-q55-v1"
SITE_ID = "point-support-retained-reference-surplus-v1"
SOURCE_BINDING_LAW = "point-support-rust-whole-file-semantic-cone-v2"
SOURCE_BINDING_DOMAIN = b"labcolors.point-support.rust-whole-file-semantic-cone.v2"
EXPECTED_SOURCE_CAPSULE_SHA256 = (
    "c399666d9db623b1f0879912ab2ed7878d99d75d760327ad022b00e2eb30eb5e"
)
EXPECTED_Q55_PROOF_SHA256 = (
    "ac59cf89503170c789223b91d775213a19d4e571ef930f2ea609fcd51b14defd"
)
EXPECTED_Q55_PAYLOAD_SHA256 = (
    "3c639a7c875046c46b56b51ecdd67d5ecaf14a1134490c88a222e7037b63c0f2"
)

DROP_SCALE = 10_000
U64_MAX = (1 << 64) - 1
I128_MAX = (1 << 127) - 1
U128_MAX = (1 << 128) - 1
RANDOM_SEED = 0xC8D_417A
RANDOM_CASES = 250_000
DENSE_NUMERATOR_STOP = 31
DENSE_DENOMINATOR_STOP = 31

WOLFRAM_SYMBOLIC_QUERY = (
    "FullSimplify[{20 g/d - 0 == 20 g/d, 20 g/d - 2 == (20 g - 2 d)/d, "
    "20 g/d - 7/2 == (40 g - 7 d)/(2 d), Equivalent[a/b >= p (s-x)/(q s), "
    "a q s >= p (s-x) b], Max[p/q, 0] (s-x)/s == Piecewise[{{0, p <= 0}}, "
    "p (s-x)/(q s)]}, Assumptions -> Element[{a,b,p,q,s,x,g,d}, Integers] && "
    "a >= 0 && b > 0 && q > 0 && s > 0 && 0 <= x <= s && d > 0 && g >= 0]"
)
WOLFRAM_SYMBOLIC_RESULT = "{True, True, True, True, True}"
EXPECTED_WOLFRAM_QUERY_SHA256 = (
    "8cdbb9964583030c8b92498961896cb2a98613f1cb31eb7c54acdf8e16beff10"
)
EXPECTED_WOLFRAM_RESULT_SHA256 = (
    "13a8f2ee8d0fde335a638e46d7cc8a8427b9a1437c77d22cfcf925bb87fa6303"
)

SOURCE_CONE_PATHS = (
    POINT_SOURCE,
    OBSERVATION_SOURCE,
    SESSION_SOURCE,
    NUMERICS_SOURCE,
    COMPOSITION_SOURCE,
    APPEARANCE_SOURCE,
    CONSTRAINTS_SOURCE,
    EXACT_CONSTRAINT_SOURCE,
    WCAG22_CONSTRAINT_SOURCE,
    WCAG22_SOURCE,
    WCAG22_KERNEL_SOURCE,
    WCAG22_Q55_DATA_SOURCE,
    WCAG22_EVIDENCE_SOURCE,
    SRGB8_SOURCE,
    HASH_SOURCE,
    LIB_SOURCE,
    WCAG22_PROFILE_SOURCE,
    Q55_PROOF,
)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def length_prefixed(value: bytes) -> bytes:
    return struct.pack("<I", len(value)) + value


def read_source_cone() -> dict[Path, bytes]:
    return {path: path.read_bytes() for path in SOURCE_CONE_PATHS}


def source_files(
    source_overrides: dict[Path, bytes] | None = None,
) -> tuple[tuple[bytes, bytes, bytes], ...]:
    """Bind complete files for the closed private production dependency cone.

    Selected-item binding was too easy to under-specify: a new local helper,
    projection or import could redirect a bound call while remaining outside the
    digest. Complete-file records intentionally trade regeneration convenience
    for fail-closed semantics. The exact ordered path set is code, not input.
    """
    overrides = {} if source_overrides is None else source_overrides
    assert set(overrides).issubset(SOURCE_CONE_PATHS)
    repo = REPO_ROOT.resolve()
    ordered_paths = tuple(
        sorted(SOURCE_CONE_PATHS, key=lambda path: path.relative_to(REPO_ROOT).as_posix())
    )
    assert len(ordered_paths) == len(set(ordered_paths))
    for path in ordered_paths:
        assert path.is_file() and not path.is_symlink()
        assert path.resolve().is_relative_to(repo)
    return tuple(
        (
            path.relative_to(REPO_ROOT).as_posix().encode("utf-8"),
            b"rust-source" if path.suffix == ".rs" else b"compile-time-input",
            overrides.get(path, path.read_bytes()),
        )
        for path in ordered_paths
    )


def source_closure_digest(source_overrides: dict[Path, bytes] | None = None) -> str:
    records = source_files(source_overrides)
    preimage = bytearray(length_prefixed(SOURCE_BINDING_DOMAIN))
    preimage.extend(struct.pack("<I", SOURCE_BINDING_SCHEMA_VERSION))
    preimage.extend(length_prefixed(SOURCE_BINDING_LAW.encode("utf-8")))
    preimage.extend(struct.pack("<I", len(records)))
    for path, kind, item in records:
        preimage.extend(length_prefixed(path))
        preimage.extend(length_prefixed(kind))
        preimage.extend(length_prefixed(item))
    return sha256(bytes(preimage))


def mutate_source(
    sources: dict[Path, bytes], path: Path, old: bytes, new: bytes
) -> dict[Path, bytes]:
    assert old != new
    assert sources[path].count(old) == 1, f"expected one mutation target in {path}: {old!r}"
    mutated = dict(sources)
    mutated[path] = sources[path].replace(old, new, 1)
    return mutated


def verify_source_binding() -> tuple[str, int]:
    sources = read_source_cone()
    digest = source_closure_digest(sources)
    assert digest == EXPECTED_SOURCE_CAPSULE_SHA256, (
        f"point-support semantic source drifted: {digest} != "
        f"{EXPECTED_SOURCE_CAPSULE_SHA256}"
    )
    mutations = (
        (POINT_SOURCE, b"matches!(self.decision, PointSupportStabilityDecisionV1::NotRetained)", b"false"),
        (POINT_SOURCE, b"matches!(self, Self::RequiredFailure(_))", b"false"),
        (POINT_SOURCE, b"matches!(self, Self::Failure(_))", b"false"),
        (POINT_SOURCE, b"            criterion,\n            stability,\n        }\n    }\n}\n\n#[derive(Debug, Clone, PartialEq, Eq)]", b"            criterion,\n            stability: PointSupportStabilityPolicyV1::Disabled,\n        }\n    }\n}\n\n#[derive(Debug, Clone, PartialEq, Eq)]"),
        (POINT_SOURCE, b"NumericalSiteIdV2::PointSupportRetainedReferenceSurplusV1;", b"NumericalSiteIdV2::Wcag22Srgb8ContrastV1;"),
        (POINT_SOURCE, b"let current_distance = reference_distance(current_measurement)?;", b"let current_distance = baseline.distance;"),
        (POINT_SOURCE, b"Ok(assessment.bind(observation))", b"Ok(assessment.bind_unchecked(observation))"),
        (POINT_SOURCE, b"            let backdrop = values.get(surface_index).copied().ok_or(\n", b"            let backdrop = values.first().copied().ok_or(\n"),
        (POINT_SOURCE, b"    if !observation.shares_schema_backing_with(&plan.surface_schema) {\n", b"    if observation.shares_schema_backing_with(&plan.surface_schema) {\n"),
        (POINT_SOURCE, b"        _permit: SessionObservationBindingPermitV1,\n", b"        _permit: (),\n"),
        (POINT_SOURCE, b"use crate::wcag22::{Wcag22CriterionV1, Wcag22MeasurementV1, measure_wcag22_srgb8};", b"use crate::wcag22::{Wcag22CriterionV1, Wcag22MeasurementV1, measure_wcag22_srgb8 as canonical_measure_wcag22_srgb8};\nfn measure_wcag22_srgb8(foreground: [u8; 3], background: [u8; 3]) -> Wcag22MeasurementV1 { canonical_measure_wcag22_srgb8(background, foreground) }"),
        (OBSERVATION_SOURCE, b"        self.backing.set.values(case_index)\n", b"        None\n"),
        (OBSERVATION_SOURCE, b"        values.extend(bindings.iter().map(|binding| binding.value));\n", b"        values.extend(bindings.iter().map(|_| Srgb8::new([0, 0, 0])));\n"),
        (OBSERVATION_SOURCE, b"        Rc::ptr_eq(&self.0, &other.0)\n", b"        self == other\n"),
        (OBSERVATION_SOURCE, b"                        schema: schema.clone(),\n", b"                        schema: CanonicalObservationSchemaV1(Rc::from(schema.as_slice())),\n"),
        (OBSERVATION_SOURCE, b"if expected_input != actual_input", b"if expected_input == actual_input"),
        (OBSERVATION_SOURCE, b"Some(observation.revision)", b"None"),
        (OBSERVATION_SOURCE, b"(self.owner, self.observation)", b"unreachable!()"),
        (SESSION_SOURCE, b"            Self::Observed(observation) => ObservationHeadViewV1::Observed(observation),\n", b"            Self::Observed(_) => ObservationHeadViewV1::Empty,\n"),
        (SESSION_SOURCE, b"                let next_raw_head = SessionObservationHeadV1::Observed(observation.clone());\n", b"                let next_raw_head = SessionObservationHeadV1::Empty;\n"),
        (SESSION_SOURCE, b"                *raw_head = next_raw_head;\n", b"                *raw_head = SessionObservationHeadV1::Empty;\n"),
        (SESSION_SOURCE, b"recheck: compiled.into_session_recheck(),", b"recheck: unreachable!(),"),
        (SESSION_SOURCE, b"                    #[cfg(not(test))]\n                    {\n                        self.recheck\n                            .evaluate(observation, SessionObservationBindingPermitV1::mint())\n                    }", b"                    #[cfg(not(test))]\n                    {\n                        self.recheck\n                            .evaluate(observation, SessionObservationBindingPermitV1::bypass())\n                    }"),
        (SESSION_SOURCE, b"PointSupportEvaluationErrorV1::ResourceExhausted => {\n            PointSupportSessionUpdateErrorV1::ResourceExhausted", b"PointSupportEvaluationErrorV1::ResourceExhausted => {\n            PointSupportSessionUpdateErrorV1::InternalInvariant"),
        (SESSION_SOURCE, b"PointSupportSessionStateV1::Ready { current } => Some(current),", b"PointSupportSessionStateV1::Ready { .. } => None,"),
        (NUMERICS_SOURCE, b"proof_ids: [NumericalProofIdV2::PointSupportReferenceSurplusIntegerV1],\n            bound_status: Available", b"proof_ids: [NumericalProofIdV2::PointSupportReferenceSurplusIntegerV1],\n            bound_status: Unavailable"),
        (COMPOSITION_SOURCE, b"f64::from(backdrop) + alpha * (f64::from(tint) - f64::from(backdrop))", b"f64::from(tint)"),
        (APPEARANCE_SOURCE, b"self.opacity\n", b"crate::composition::AdmittedOpacityV1::OPAQUE\n"),
        (CONSTRAINTS_SOURCE, b"let classification = evaluator.classify(&invocation, &measurement);", b"let classification = unreachable!();"),
        (EXACT_CONSTRAINT_SOURCE, b"if actual == *invocation", b"if actual != *invocation"),
        (WCAG22_CONSTRAINT_SOURCE, b"Wcag22ApplicableDecisionV1::Pass => HardDecision::Pass(Wcag22PassV1(()))", b"Wcag22ApplicableDecisionV1::Pass => HardDecision::Violation(Wcag22ViolationV1(()))"),
        (WCAG22_SOURCE, b"foreground_luminance: kernel::luminance_bounds(foreground),", b"foreground_luminance: kernel::luminance_bounds(background),"),
        (WCAG22_KERNEL_SOURCE, b"10 * light_lower >= 30 * dark_upper + scale", b"10 * light_lower > 30 * dark_upper + scale"),
        (WCAG22_Q55_DATA_SOURCE, b"pub(crate) const Q55_SCALE: u64 = 36028797018963968;", b"pub(crate) const Q55_SCALE: u64 = 36028797018963967;"),
        (WCAG22_EVIDENCE_SOURCE, b"NumericalSiteIdV2::Wcag22Srgb8ContrastV1;", b"NumericalSiteIdV2::PointSupportRetainedReferenceSurplusV1;"),
        (SRGB8_SOURCE, b"self.0\n", b"[0, 0, 0]\n"),
        (HASH_SOURCE, b"const FNV1A_32_PRIME: u32 = 16777619;", b"const FNV1A_32_PRIME: u32 = 16777621;"),
        (LIB_SOURCE, b"pub(crate) mod point_support;", b"#[path = \"alternate_point_support.rs\"]\npub(crate) mod point_support;"),
        (WCAG22_PROFILE_SOURCE, b'"normalTextRatio":"4.5"', b'"normalTextRatio":"4.4"'),
        (Q55_PROOF, b'"proof_payload_sha256":"3c639a7c875046c46b56b51ecdd67d5ecaf14a1134490c88a222e7037b63c0f2"', b'"proof_payload_sha256":"0000000000000000000000000000000000000000000000000000000000000000"'),
    )
    for path, old, new in mutations:
        mutated = mutate_source(sources, path, old, new)
        assert source_closure_digest(mutated) != digest, "source mutation escaped complete-file binding"

    for path in SOURCE_CONE_PATHS:
        appended = dict(sources)
        appended[path] += b"\n// any added helper or route is proof-significant\n"
        assert source_closure_digest(appended) != digest
    return digest, len(mutations)


def verify_q55_dependency() -> dict[str, object]:
    raw = Q55_PROOF.read_bytes()
    assert sha256(raw) == EXPECTED_Q55_PROOF_SHA256
    proof = json.loads(raw)
    payload = dict(proof)
    payload_digest = payload.pop("proof_payload_sha256")
    canonical_payload = json.dumps(
        payload, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    assert sha256(canonical_payload) == payload_digest
    expected = {
        "schema_version": 2,
        "artifact_id": ARTIFACT_ID,
        "bound_id": "wcag22-srgb8-outward-q55-v1",
        "proof_id": "wcag22-srgb8-full-domain-q55-v1",
        "proof_payload_sha256": EXPECTED_Q55_PAYLOAD_SHA256,
        "q55_scale": 1 << 55,
        "colors": 256**3,
        "unique_intervals": 256**3,
        "max_color_interval_width": 3,
    }
    for field, value in expected.items():
        assert proof.get(field) == value, f"Q55 dependency drift at {field}"
    envelope = proof["integer_replay_envelope"]
    assert envelope["maximum_luminance_upper"] == proof["q55_scale"] + 3
    assert envelope["outward_interval_width_bound"] == 3
    return {
        "artifact_id": proof["artifact_id"],
        "artifact_sha256": proof["artifact_sha256"],
        "proof_id": proof["proof_id"],
        "proof_sha256": EXPECTED_Q55_PROOF_SHA256,
        "proof_payload_sha256": proof["proof_payload_sha256"],
        "q55_scale": proof["q55_scale"],
        "maximum_luminance_upper": envelope["maximum_luminance_upper"],
        "outward_interval_width_bound": envelope["outward_interval_width_bound"],
    }


def reference_distance(
    foreground: tuple[int, int], background: tuple[int, int], scale: int
) -> tuple[str, int, int]:
    f_lower, f_upper = foreground
    b_lower, b_upper = background
    if f_lower > b_upper:
        return "foreground-lighter", f_lower - b_upper, 20 * b_upper + scale
    if b_lower > f_upper:
        return "background-lighter", b_lower - f_upper, 20 * f_upper + scale
    return "unseparated", 0, scale


def anchor_surplus(anchor: str, gap: int, denominator: int) -> Fraction:
    if anchor == "ratio-1":
        return Fraction(20 * gap, denominator)
    if anchor == "ratio-3":
        return Fraction(20 * gap - 2 * denominator, denominator)
    if anchor == "ratio-4.5":
        return Fraction(40 * gap - 7 * denominator, 2 * denominator)
    raise AssertionError(f"unknown anchor {anchor}")


def required_surplus(baseline: Fraction, drop_bps: int) -> Fraction:
    assert 0 <= drop_bps <= DROP_SCALE
    if baseline <= 0 or drop_bps == DROP_SCALE:
        return Fraction()
    return baseline * Fraction(DROP_SCALE - drop_bps, DROP_SCALE)


def compare_nonnegative_rationals(a: int, b: int, c: int, d: int) -> int:
    assert min(a, c) >= 0 and min(b, d) > 0
    reversed_order = False
    while True:
        left_quotient, left_remainder = divmod(a, b)
        right_quotient, right_remainder = divmod(c, d)
        if left_quotient != right_quotient:
            order = (left_quotient > right_quotient) - (left_quotient < right_quotient)
            return -order if reversed_order else order
        if left_remainder == 0 or right_remainder == 0:
            order = (left_remainder > 0) - (right_remainder > 0)
            return -order if reversed_order else order
        a, b = b, left_remainder
        c, d = d, right_remainder
        reversed_order = not reversed_order


def oracle_order(a: int, b: int, c: int, d: int) -> int:
    left = a * d
    right = c * b
    return (left > right) - (left < right)


def verify_universal_algebra() -> dict[str, object]:
    """Dependency-free sparse-polynomial certificate for the general laws."""
    variable_names = (
        "L",
        "L1",
        "L2",
        "D",
        "D1",
        "D2",
        "Q",
        "a",
        "b",
        "p",
        "q",
        "B",
        "x",
    )
    zero_exponents = (0,) * len(variable_names)

    def constant(value: int) -> dict[tuple[int, ...], int]:
        return {} if value == 0 else {zero_exponents: value}

    def variable(name: str) -> dict[tuple[int, ...], int]:
        exponents = [0] * len(variable_names)
        exponents[variable_names.index(name)] = 1
        return {tuple(exponents): 1}

    def add(
        left: dict[tuple[int, ...], int], right: dict[tuple[int, ...], int]
    ) -> dict[tuple[int, ...], int]:
        result = dict(left)
        for monomial, coefficient in right.items():
            result[monomial] = result.get(monomial, 0) + coefficient
            if result[monomial] == 0:
                del result[monomial]
        return result

    def scale(
        coefficient: int, value: dict[tuple[int, ...], int]
    ) -> dict[tuple[int, ...], int]:
        return {monomial: coefficient * item for monomial, item in value.items() if coefficient * item}

    def subtract(
        left: dict[tuple[int, ...], int], right: dict[tuple[int, ...], int]
    ) -> dict[tuple[int, ...], int]:
        return add(left, scale(-1, right))

    def multiply(
        left: dict[tuple[int, ...], int], right: dict[tuple[int, ...], int]
    ) -> dict[tuple[int, ...], int]:
        result: dict[tuple[int, ...], int] = {}
        for left_monomial, left_coefficient in left.items():
            for right_monomial, right_coefficient in right.items():
                monomial = tuple(
                    left_power + right_power
                    for left_power, right_power in zip(
                        left_monomial, right_monomial, strict=True
                    )
                )
                result[monomial] = (
                    result.get(monomial, 0) + left_coefficient * right_coefficient
                )
                if result[monomial] == 0:
                    del result[monomial]
        return result

    def rational_difference(
        left_numerator: dict[tuple[int, ...], int],
        left_denominator: dict[tuple[int, ...], int],
        right_numerator: dict[tuple[int, ...], int],
        right_denominator: dict[tuple[int, ...], int],
    ) -> tuple[dict[tuple[int, ...], int], dict[tuple[int, ...], int]]:
        return (
            subtract(
                multiply(left_numerator, right_denominator),
                multiply(right_numerator, left_denominator),
            ),
            multiply(left_denominator, right_denominator),
        )

    l_value, l1, l2 = variable("L"), variable("L1"), variable("L2")
    d_value, d1, d2 = variable("D"), variable("D1"), variable("D2")
    q55_scale = variable("Q")
    current_numerator, current_denominator = variable("a"), variable("b")
    baseline_numerator, baseline_denominator = variable("p"), variable("q")
    basis_point_scale, drop = variable("B"), variable("x")

    # Subtract each declared ratio from the contrast definition
    # (20L+Q)/(20D+Q), then compare both numerator and denominator with
    # anchor_surplus's closed forms. The six explicit mutants ensure that a
    # wrong 20/2/40/7 coefficient or the 4.5 denominator factor is observable.
    gap = subtract(l_value, d_value)
    denominator = add(scale(20, d_value), q55_scale)
    contrast_numerator = add(scale(20, l_value), q55_scale)
    anchor_derivations = (
        rational_difference(
            contrast_numerator, denominator, constant(1), constant(1)
        ),
        rational_difference(
            contrast_numerator, denominator, constant(3), constant(1)
        ),
        rational_difference(
            contrast_numerator, denominator, constant(9), constant(2)
        ),
    )
    anchor_closed_forms = (
        (scale(20, gap), denominator),
        (subtract(scale(20, gap), scale(2, denominator)), denominator),
        (
            subtract(scale(40, gap), scale(7, denominator)),
            scale(2, denominator),
        ),
    )
    assert anchor_derivations == anchor_closed_forms
    anchor_mutants = (
        (scale(19, gap), denominator),
        (subtract(scale(19, gap), scale(2, denominator)), denominator),
        (subtract(scale(20, gap), scale(3, denominator)), denominator),
        (
            subtract(scale(39, gap), scale(7, denominator)),
            scale(2, denominator),
        ),
        (
            subtract(scale(40, gap), scale(8, denominator)),
            scale(2, denominator),
        ),
        (subtract(scale(40, gap), scale(7, denominator)), denominator),
    )
    assert all(mutant not in anchor_derivations for mutant in anchor_mutants)

    # f(L,D)=20(L-D)/(20D+Q): increasing L and decreasing D are polynomial
    # consequences with strictly positive denominators.
    lighter_left = subtract(
        multiply(scale(20, subtract(l2, d_value)), denominator),
        multiply(scale(20, subtract(l1, d_value)), denominator),
    )
    lighter_right = multiply(scale(20, subtract(l2, l1)), denominator)
    assert lighter_left == lighter_right
    darker_left = subtract(
        multiply(subtract(l_value, d1), add(scale(20, d2), q55_scale)),
        multiply(subtract(l_value, d2), add(scale(20, d1), q55_scale)),
    )
    darker_right = multiply(subtract(d2, d1), add(scale(20, l_value), q55_scale))
    assert darker_left == darker_right

    # For positive baseline p/q, the retained threshold is exactly
    # p(B-x)/(qB). Derive current-minus-required as one rational; positivity of
    # b, q and B makes its sign exactly the sign of the cleared numerator.
    retained_numerator = multiply(
        baseline_numerator, subtract(basis_point_scale, drop)
    )
    retained_denominator = multiply(baseline_denominator, basis_point_scale)
    retained_difference = rational_difference(
        current_numerator,
        current_denominator,
        retained_numerator,
        retained_denominator,
    )
    retained_closed_form = (
        subtract(
            multiply(
                multiply(current_numerator, baseline_denominator),
                basis_point_scale,
            ),
            multiply(
                multiply(
                    baseline_numerator, subtract(basis_point_scale, drop)
                ),
                current_denominator,
            ),
        ),
        multiply(
            multiply(current_denominator, baseline_denominator),
            basis_point_scale,
        ),
    )
    assert retained_difference == retained_closed_form
    retained_mutants = (
        rational_difference(
            current_numerator,
            current_denominator,
            multiply(baseline_numerator, add(basis_point_scale, drop)),
            retained_denominator,
        ),
        rational_difference(
            current_numerator,
            current_denominator,
            retained_numerator,
            basis_point_scale,
        ),
        (
            retained_closed_form[0],
            multiply(baseline_denominator, basis_point_scale),
        ),
        (
            retained_closed_form[0],
            multiply(current_denominator, baseline_denominator),
        ),
        (
            subtract(
                multiply(
                    multiply(current_numerator, baseline_denominator),
                    basis_point_scale,
                ),
                multiply(
                    baseline_numerator, subtract(basis_point_scale, drop)
                ),
            ),
            retained_closed_form[1],
        ),
    )
    assert all(mutant != retained_difference for mutant in retained_mutants)

    query_digest = sha256(WOLFRAM_SYMBOLIC_QUERY.encode("utf-8"))
    result_digest = sha256(WOLFRAM_SYMBOLIC_RESULT.encode("utf-8"))
    assert query_digest == EXPECTED_WOLFRAM_QUERY_SHA256
    assert result_digest == EXPECTED_WOLFRAM_RESULT_SHA256
    return {
        "method": "exact-sparse-integer-polynomial-identities-plus-positive-denominator-order-lemma-v1",
        "domain": "integers; Q55 scale Q>0; anchor L>=D>=0; lighter monotonicity L2>=L1>D>=0; darker monotonicity L>D2>=D1>=0; current/baseline denominators b,q>0; basis-point scale B>0 instantiated as 10000; p>0; a>=0; 0<=drop_bps<=B",
        "identities": [
            "three explicit anchor-surplus formulas after denominator clearing",
            "reference distance is monotone increasing in lighter L",
            "reference distance is monotone decreasing in darker D",
            "positive-baseline retained threshold is p*(B-drop)/(q*B)",
            "a/b >= p*(B-drop)/(q*B) iff a*q*B >= p*(B-drop)*b",
        ],
        "basis_point_scale_instantiation": DROP_SCALE,
        "symbolic_mutation_controls": {
            "anchor_coefficients_and_denominator": len(anchor_mutants),
            "retained_cross_product": len(retained_mutants),
        },
        "nonpositive_baseline_case": "max(baseline,0)=0; retained threshold is exactly zero",
        "wolfram_language_cross_check": {
            "query": WOLFRAM_SYMBOLIC_QUERY,
            "query_sha256": query_digest,
            "result": WOLFRAM_SYMBOLIC_RESULT,
            "result_sha256": result_digest,
        },
    }


def verify_reference_and_anchor_laws(scale: int, maximum: int) -> dict[str, object]:
    # Endpoint monotonicity proves the separated-interval lower bound: the
    # distance is increasing in lighter luminance and decreasing in darker.
    points = sorted({0, 1, 2, scale // 2, scale - 1, scale, maximum})
    checks = 0
    for light_lower in points:
        for light_upper in points:
            if light_lower > light_upper:
                continue
            for dark_lower in points:
                for dark_upper in points:
                    if dark_lower > dark_upper or light_lower <= dark_upper:
                        continue
                    _, gap, denominator = reference_distance(
                        (light_lower, light_upper), (dark_lower, dark_upper), scale
                    )
                    lower = Fraction(20 * gap, denominator)
                    for light in (light_lower, light_upper):
                        for dark in (dark_lower, dark_upper):
                            actual = Fraction(20 * (light - dark), 20 * dark + scale)
                            assert lower <= actual
                            checks += 1
                    swapped = reference_distance(
                        (dark_lower, dark_upper), (light_lower, light_upper), scale
                    )
                    assert swapped[0] == "background-lighter"
                    assert swapped[1:] == (gap, denominator)
    assert reference_distance((0, 3), (2, 5), scale) == ("unseparated", 0, scale)

    anchor_checks = 0
    for gap in (0, 1, 7, scale // 2, maximum):
        for darker in (0, 1, scale // 2, scale, maximum):
            denominator = 20 * darker + scale
            distance = Fraction(20 * gap, denominator)
            contrast_numerator = 20 * (darker + gap) + scale
            for anchor, threshold in (
                ("ratio-1", Fraction(1)),
                ("ratio-3", Fraction(3)),
                ("ratio-4.5", Fraction(9, 2)),
            ):
                derived_from_definition = (
                    Fraction(contrast_numerator, denominator) - threshold
                )
                assert (
                    anchor_surplus(anchor, gap, denominator)
                    == derived_from_definition
                    == distance - (threshold - 1)
                )
                anchor_checks += 1
    # Synthetic exact threshold equalities exercise every rational formula.
    assert anchor_surplus("ratio-1", 0, 37) == 0
    assert anchor_surplus("ratio-3", 1, 10) == 0
    assert anchor_surplus("ratio-4.5", 7, 40) == 0
    return {
        "separated_endpoint_checks": checks,
        "anchor_identity_checks": anchor_checks,
        "overlap_lower_distance": "0/1",
        "orientation_law": "distance-magnitude-symmetric-orientation-reported-separately",
    }


def verify_basis_point_law() -> dict[str, object]:
    baselines = (
        Fraction(-7, 3),
        Fraction(),
        Fraction(1, U128_MAX),
        Fraction(3, 7),
        Fraction(33 * (1 << 55) + 120, 1),
    )
    drops = (0, 1, 2_500, 5_000, 9_999, 10_000)
    checks = 0
    for baseline in baselines:
        for drop in drops:
            required = required_surplus(baseline, drop)
            expected = max(baseline, Fraction()) * Fraction(DROP_SCALE - drop, DROP_SCALE)
            assert required == expected
            assert compare_nonnegative_rationals(
                required.numerator,
                required.denominator,
                expected.numerator,
                expected.denominator,
            ) == 0
            checks += 1
    return {
        "checks": checks,
        "drop_domain_inclusive": [0, DROP_SCALE],
        "drop_all_semantics": "zero required surplus; current must still meet the anchor",
        "nonpositive_baseline_semantics": "zero required surplus; current must meet the anchor",
    }


def verify_comparator() -> dict[str, object]:
    dense = 0
    for a in range(DENSE_NUMERATOR_STOP + 1):
        for b in range(1, DENSE_DENOMINATOR_STOP + 1):
            for c in range(DENSE_NUMERATOR_STOP + 1):
                for d in range(1, DENSE_DENOMINATOR_STOP + 1):
                    assert compare_nonnegative_rationals(a, b, c, d) == oracle_order(a, b, c, d)
                    dense += 1

    adversarial = [
        (0, 1, 0, U128_MAX),
        (1, 3, 2, 6),
        (U128_MAX - 1, U128_MAX, 1, 2),
        (1, U128_MAX, 2, U128_MAX),
        (U128_MAX, 1, U128_MAX - 1, 1),
        (U128_MAX, U128_MAX, 1, 1),
    ]
    fib = [0, 1]
    while fib[-1] + fib[-2] <= U128_MAX:
        fib.append(fib[-1] + fib[-2])
    adversarial.extend(
        (
            fib[index],
            fib[index - 1],
            fib[index - 1],
            fib[index - 2],
        )
        for index in range(3, len(fib))
    )
    for case in adversarial:
        assert compare_nonnegative_rationals(*case) == oracle_order(*case)

    generator = random.Random(RANDOM_SEED)
    random_digest = hashlib.sha256()
    for _ in range(RANDOM_CASES):
        a = generator.getrandbits(128)
        b = generator.getrandbits(128) or 1
        c = generator.getrandbits(128)
        d = generator.getrandbits(128) or 1
        assert compare_nonnegative_rationals(a, b, c, d) == oracle_order(a, b, c, d)
        random_digest.update(a.to_bytes(16, "little"))
        random_digest.update(b.to_bytes(16, "little"))
        random_digest.update(c.to_bytes(16, "little"))
        random_digest.update(d.to_bytes(16, "little"))
    return {
        "algorithm": "euclidean-continued-fraction-ordering-v1",
        "invariant": "equal integer parts; reciprocal proper fractions reverse order",
        "termination": "each nonterminal denominator becomes a strictly smaller remainder",
        "dense_small_cases": dense,
        "dense_numerator_inclusive": [0, DENSE_NUMERATOR_STOP],
        "dense_denominator_inclusive": [1, DENSE_DENOMINATOR_STOP],
        "u128_adversarial_cases": len(adversarial),
        "largest_fibonacci_index": len(fib) - 1,
        "random_seed": RANDOM_SEED,
        "random_cases": RANDOM_CASES,
        "random_corpus_sha256": random_digest.hexdigest(),
        "oracle": "unbounded-integer-cross-product",
    }


def verify_integer_envelope(scale: int, maximum: int) -> dict[str, int | str]:
    denominator_max = 20 * maximum + scale
    signed_anchor_abs_max = max(20 * maximum, 2 * denominator_max, 7 * denominator_max)
    positive_baseline_numerator_max = 40 * maximum - 7 * scale
    rational_denominator_max = 2 * denominator_max
    required_numerator_max = positive_baseline_numerator_max * DROP_SCALE
    required_denominator_max = rational_denominator_max * DROP_SCALE
    assert denominator_max <= U64_MAX
    assert signed_anchor_abs_max <= I128_MAX
    assert required_numerator_max <= U128_MAX
    assert required_denominator_max <= U128_MAX
    return {
        "assumption": "every Q55 luminance upper <= scale + 3",
        "u64_max": U64_MAX,
        "i128_max": I128_MAX,
        "u128_max": U128_MAX,
        "offset_cleared_denominator_max": denominator_max,
        "signed_anchor_abs_coarse_max": signed_anchor_abs_max,
        "positive_baseline_numerator_max": positive_baseline_numerator_max,
        "rational_denominator_max": rational_denominator_max,
        "required_numerator_max": required_numerator_max,
        "required_denominator_max": required_denominator_max,
    }


def canonical_proof() -> dict[str, object]:
    dependency = verify_q55_dependency()
    scale = int(dependency["q55_scale"])
    maximum = int(dependency["maximum_luminance_upper"])
    source_closure, source_controls = verify_source_binding()
    payload: dict[str, object] = {
        "schema_version": SCHEMA_VERSION,
        "profile_id": PROFILE_ID,
        "site_id": SITE_ID,
        "artifact_id": ARTIFACT_ID,
        "bound_id": BOUND_ID,
        "proof_id": PROOF_ID,
        "declared_operation_law": "q55-lower-reference-distance-explicit-anchor-bps-retention-v1",
        "certified_claim": "for every successfully evaluated enabled stability cell, decision is Retained iff current_lower_surplus >= (10000-drop_bps)/10000 * max(baseline_lower_surplus,0); the declared anchor remains a separate hard floor",
        "excluded_claim": "does not certify retention against the unknown exact baseline surplus, renderer equivalence outside encoded-sRGB8 source-over, or a successful result when evaluation fails",
        "q55_dependency": dependency,
        "source_binding_schema_version": SOURCE_BINDING_SCHEMA_VERSION,
        "source_binding_law": SOURCE_BINDING_LAW,
        "source_binding_scope": "exact bytes of the private point-support Rust semantic cone and its two WCAG include_str inputs; comments and cfg(test) text are intentionally significant",
        "source_binding_exclusions": [
            "whole-crate compilation or compiler/toolchain attestation",
            "binary, package, FFI, renderer, or browser transport attestation",
            "unrelated Lab Colors modules outside the declared point-support semantic cone",
        ],
        "source_closure_sha256": source_closure,
        "source_negative_controls": source_controls,
        "source_files": [
            {
                "path": path.decode("utf-8"),
                "kind": kind.decode("utf-8"),
                "sha256": sha256(item),
            }
            for path, kind, item in source_files()
        ],
        "universal_algebraic_certificate": verify_universal_algebra(),
        "reference_and_anchor_proof": verify_reference_and_anchor_laws(scale, maximum),
        "basis_point_proof": verify_basis_point_law(),
        "comparator_proof": verify_comparator(),
        "integer_replay_envelope": verify_integer_envelope(scale, maximum),
        "verifier_sha256": sha256(VERIFIER_PATH.read_bytes()),
    }
    canonical_payload = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return {**payload, "proof_payload_sha256": sha256(canonical_payload)}


def main() -> int:
    if sys.argv[1:] == ["--source-closure-digest"]:
        print(source_closure_digest())
        return 0
    emit_only = sys.argv[1:] == ["--emit"]
    if sys.argv[1:] not in ([], ["--emit"]):
        raise ValueError("usage: verify_point_support_surplus.py [--emit|--source-closure-digest]")
    proof = canonical_proof()
    canonical = json.dumps(proof, sort_keys=True, separators=(",", ":")) + "\n"
    if not emit_only:
        assert PROOF_PATH.read_text(encoding="utf-8") == canonical, (
            "committed point-support surplus proof drifted; regenerate explicitly "
            "with --emit only after numerical review"
        )
    sys.stdout.write(canonical)
    print("point-support retained-surplus independent verification: PASS", file=sys.stderr)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError, ValueError) as error:
        print(
            f"point-support retained-surplus independent verification: FAIL: {error}",
            file=sys.stderr,
        )
        raise SystemExit(1) from error
