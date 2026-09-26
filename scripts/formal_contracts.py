"""Обязательства формальной проверки ядра: независимый список ожидаемых свойств."""

_AUTH = {
    "auth_permit_iff_exact_current_proof": (
        {"permit must match the full current proof binding"},
        {"valid permit remains reachable", "missing proof", "foreign lane", "stale release",
         "foreign applicability", "foreign provenance", "unobserved renderer"},
    ),
    "auth_admission_is_exact_atomic_and_lane_local": (
        {"admission must obey exact authorization and CAS", "only the addressed lane may change",
         "every refusal must preserve all lanes"},
        {"install", "replace", "duplicate", "refusal"},
    ),
    "auth_require_iff_full_binding": (
        {"require must compare every byte of every identity", "read must address the contract lane"},
        {"exact binding", "empty lane", "release mismatch", "applicability mismatch", "provenance mismatch"},
    ),
    "auth_issue_admit_require_composes": (
        {"fresh owner authorization must commit", "accepted binding must remain usable",
         "duplicate must remain a true no-op"},
        {"complete authorized path"},
    ),
}

CONTRACTS = {"authority::proofs::" + name: contract for name, contract in _AUTH.items()}
CONTRACTS.update({
    "srgb8::proofs::hex_admits_exactly_six_hex_digits": (
        {"hex syntax must contain six hexadecimal digits only", "hex channels must preserve both nibbles"},
        {"bare hex", "prefixed hex", "invalid six-byte hex", "overlong hex"},
    ),
    "srgb8::proofs::parser_preserves_all_srgb8_values": (
        {"all srgb8 values must survive hexadecimal transport", "bare transport must preserve identical colour bytes"},
        {"lowercase transport", "uppercase transport"},
    ),
    "composition::proofs::opacity_admission_is_exact_and_canonical": (
        {"opacity must admit exactly finite unit binary64 values", "opacity must preserve every nonzero bit and canonicalize zero", "opacity value must agree with its canonical bits", "opacity predecessor must be the previous admissible value"},
        {"negative zero", "smallest subnormal", "invalid opacity", "admitted opacity"},
    ),
    "composition::proofs::opacity_domain_preserves_all_boundaries": (
        {"opacity domain admission must reject invalid or reversed endpoints", "opacity membership must include exactly the closed interval"},
        {"fixed opacity domain", "nontrivial opacity domain", "invalid opacity domain"},
    ),
    "composition::proofs::opacity_multiplication_preserves_identity_and_zero": (
        {"unit opacity must be an exact multiplicative identity", "zero opacity must remain canonical under multiplication"},
        {"subnormal opacity identity", "opaque identity"},
    ),
    "composition::proofs::source_over_endpoints_preserve_channel_values": (
        {"transparent source must preserve every backdrop channel", "opaque source must preserve every source channel"},
        {"darkening endpoints", "lightening endpoints"},
    ),
    "certificate::proofs::reader_bounds_and_failure_atomicity": (
        {"reader must reject every out-of-bounds or overflowing span", "reader must return exactly the requested bytes", "failed exact read must not advance the cursor"},
        {"nonempty read", "empty read", "invalid span", "offset overflow"},
    ),
    "certificate::proofs::reader_big_endian_and_length_prefix": (
        {"certificate integers must be decoded in network byte order", "length prefix must never admit truncated payload"},
        {"complete delimited payload", "truncated delimited payload"},
    ),
    "certificate::proofs::revision_is_exact_lowercase_hex": (
        {"revision must contain exactly forty lowercase hexadecimal digits"},
        {"canonical revision", "malformed revision"},
    ),
    "certificate::proofs::wire_resource_length_is_exact": (
        {"wire resource admission must preserve nonempty bounded length"},
        {"length admitted", "empty length", "overbudget length"},
    ),
    "program::wire::proofs::fixed_reads_are_exact_and_atomic": (
        {"program wire must reject overflowing or truncated fixed reads", "program wire must preserve little endian bytes and cursor", "program wire failure must preserve the cursor"},
        {"fixed read accepted", "fixed read rejected"},
    ),
    "program::wire::proofs::floating_wire_keeps_every_bit": (
        {"wire float transport must preserve every binary64 bit", "complete float read must consume exactly eight bytes"},
        {"negative zero transport", "infinity stays raw transport", "NaN stays raw transport"},
    ),
    "program::wire::proofs::section_count_preserves_resource_limit": (
        {"program section length must never exceed its admitted budget"},
        {"empty section", "budget boundary", "overbudget section"},
    ),
    "wcag22::kernel::proofs::exact_points_are_total_symmetric_and_correct": (
        {"exact Q55 points must match the unsimplified contrast ratio", "contrast decision must be independent of polarity"},
        {"contrast passes", "contrast fails"},
    ),
    "wcag22::kernel::proofs::interval_verdict_is_sound_for_every_enclosed_point": (
        {"interval PASS must hold for every enclosed colour pair", "interval FAIL must hold for every enclosed colour pair", "interval classification must preserve polarity symmetry"},
        {"interval passes", "interval fails", "overlapping threshold stays uncertain"},
    ),
    "joint::proofs::joint_order_is_complete_unique_and_authored": (
        {"joint order must admit exactly the complete nonduplicated product", "joint admission must preserve authored tuple priority"},
        {"complete product", "invalid product", "nonlexicographic policy"},
    ),
    "joint::proofs::doubling_cardinality_cannot_wrap_into_empty_success": (
        {"doubling cardinality overflow must remain distinct from empty authored order"},
        {"overflowing doubled cardinality", "representable doubled cardinality"},
    ),
})

# Один предметный фальсификатор на независимую область; старый AUTH сохранён.
# name, source, harness, assertion, exact-before, exact-after
MUTANTS = (
    ("authority-binding-prefix", "authority.rs", "authority::proofs::auth_require_iff_full_binding",
     "require must compare every byte of every identity",
     "        if current.applicability != expected.applicability {",
     "        if current.applicability.0[0] != expected.applicability.0[0] {"),
    ("hex-plus-admission", "srgb8.rs", "srgb8::proofs::hex_admits_exactly_six_hex_digits",
     "hex syntax must contain six hexadecimal digits only",
     "        _ => None,\n    };", "        b'+' => Some(0),\n        _ => None,\n    };"),
    ("opacity-signed-zero", "composition.rs", "composition::proofs::opacity_admission_is_exact_and_canonical",
     "opacity must preserve every nonzero bit and canonicalize zero",
     "        let canonical = if alpha == 0.0 { 0.0 } else { alpha };",
     "        let canonical = alpha;"),
    ("certificate-offset", "certificate.rs", "certificate::proofs::reader_bounds_and_failure_atomicity",
     "reader must reject every out-of-bounds or overflowing span",
     "            .get(self.offset..end)", "            .get(..length)"),
    ("wire-budget-boundary", "program/wire.rs", "program::wire::proofs::section_count_preserves_resource_limit",
     "program section length must never exceed its admitted budget",
     "        if count > MAX_SECTION_ENTRIES_V1 {", "        if count >= MAX_SECTION_ENTRIES_V1 {"),
    ("wcag-normal-text-threshold", "wcag22/kernel.rs", "wcag22::kernel::proofs::exact_points_are_total_symmetric_and_correct",
     "exact Q55 points must match the unsimplified contrast ratio",
     "        Wcag22CriterionV1::Sc143TextDefault => ThresholdV1::FourAndHalf,",
     "        Wcag22CriterionV1::Sc143TextDefault => ThresholdV1::Three,"),
    ("joint-duplicate-admission", "joint.rs", "joint::proofs::joint_order_is_complete_unique_and_authored",
     "joint order must admit exactly the complete nonduplicated product",
     "        if *first != usize::MAX {", "        if false {"),
)

SOURCES = ("authority.rs", "srgb8.rs", "composition.rs", "certificate.rs", "program/wire.rs", "wcag22/kernel.rs", "joint.rs")
