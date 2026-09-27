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


CONTRACTS.update({
    "composition::proofs::identical_channels_are_fixed_for_every_opacity": (
        {"equal source and backdrop must be a fixed point for every valid opacity"},
        {"subnormal fixed point", "nonendpoint fixed point"},
    ),
    "observation::proofs::equality_requires_the_same_observation_allocation": (
        {"observation equality must bind stream revision and allocation identity"},
        {"same observation accepted", "separate backing rejected despite equal coordinates", "foreign stream rejected", "foreign revision rejected"},
    ),
    "certificate::proofs::ledger_budget_cannot_overflow_or_exceed_capacity": (
        {"ledger budget must reject all count size and overflow violations", "ledger accounting must equal the exact retained byte cost"},
        {"exact byte capacity admitted", "entry capacity rejected", "machine word overflow rejected", "first ledger entry admitted"},
    ),
    "session::proofs::current_evidence_never_promotes_historical_state": (
        {"render authority must never promote historical evidence to current", "last good evidence must remain distinct from current authority"},
        {"ready evidence authorizes", "historical evidence does not authorize", "no evidence remains absent"},
    ),
    "session::proofs::state_displacement_conserves_every_owned_payload": (
        {"displacement must empty the old state before publication", "displacement must retain exactly the old good and violation payloads"},
        {"failed state keeps two owned payloads", "first violation has no invented predecessor", "empty state has nothing to retire"},
    ),
    "program::attachment::proofs::mutation_stamp_preserves_epoch_and_cannot_wrap": (
        {"sink mutation must refuse exhausted sequence instead of wrapping", "sink CAS must preserve the entire expected token", "sink successor must advance exactly once within the same epoch"},
        {"sink sequence exhaustion", "last valid sink mutation", "full width binding epoch preserved"},
    ),
})

MUTANTS += (
    ("observation-foreign-backing", "observation.rs", "observation::proofs::equality_requires_the_same_observation_allocation",
     "observation equality must bind stream revision and allocation identity",
     "            && Rc::ptr_eq(&self.backing, &other.backing)", "            && true"),
    ("ledger-entry-boundary", "certificate.rs", "certificate::proofs::ledger_budget_cannot_overflow_or_exceed_capacity",
     "ledger budget must reject all count size and overflow violations",
     "    if records >= MAX_ADMISSION_ENTRIES_V1 {", "    if records > MAX_ADMISSION_ENTRIES_V1 {"),
    ("historical-evidence-promotion", "session.rs", "session::proofs::current_evidence_never_promotes_historical_state",
     "render authority must never promote historical evidence to current",
     "            Self::Waiting | Self::Stale { .. } | Self::Failed { .. } => None,",
     "            Self::Stale { previous } => Some(previous),\n            Self::Waiting | Self::Failed { .. } => None,"),
    ("sink-stamp-no-advance", "program/attachment.rs", "program::attachment::proofs::mutation_stamp_preserves_epoch_and_cannot_wrap",
     "sink successor must advance exactly once within the same epoch",
     "        match self.sequence.checked_add(1) {", "        match self.sequence.checked_add(0) {"),
)
SOURCES += ("session.rs", "observation.rs", "program/attachment.rs")


CONTRACTS["composition::proofs::multiply_preserves_the_entire_admitted_domain"] = (
    {"multiplying any admitted opacities must preserve the entire canonical unit domain"},
    {"positive product underflow", "subnormal product", "opaque product"},
)
MUTANTS += (
    ("opacity-multiply-as-addition", "composition.rs",
     "composition::proofs::multiply_preserves_the_entire_admitted_domain",
     "multiplying any admitted opacities must preserve the entire canonical unit domain",
     "        Self((self.value() * rhs.value()).to_bits())",
     "        Self((self.value() + rhs.value()).to_bits())"),
)

CONTRACTS["composition::proofs::source_over_is_bounded_before_quantization"] = (
    {"every source-over value must be finite and in byte range before quantization"},
    {"intermediate darkening", "intermediate lightening"},
)
MUTANTS += (
    ("source-over-before-clamping", "composition.rs",
     "composition::proofs::source_over_is_bounded_before_quantization",
     "every source-over value must be finite and in byte range before quantization",
     "    f64::from(backdrop) + alpha * (f64::from(tint) - f64::from(backdrop))",
     "    f64::from(backdrop) + alpha * (f64::from(tint) + f64::from(backdrop))"),
)


CONTRACTS.update({
    "field_effect::proofs::premultiplied_admission_and_lighter_are_exact": (
        {"premultiplied admission must reject exactly channels above alpha",
         "lighter must clamp the exact sum of every channel including alpha",
         "lighter channels must remain below their own alpha"},
        {"transparent lighter source", "lighter saturation", "invalid premultiplied pixel"},
    ),
    "field_effect::proofs::premultiplied_source_over_has_unique_nearest_integer_output": (
        {"premultiplied source-over must return the unique nearest rational value",
         "source-over channels must remain below their own alpha"},
        {"transparent premultiplied source", "opaque premultiplied source", "two translucent pixels"},
    ),
    "field_effect::proofs::rectangles_admit_exactly_nonempty_in_bounds_geometry": (
        {"field rectangle must admit exactly its nonempty bounded geometry",
         "rectangle admission must preserve all declared coordinates"},
        {"rectangle touches right edge", "empty rectangle rejected",
         "rectangle addition overflow rejected", "outside rectangle rejected"},
    ),
    "field_effect::proofs::expanded_rectangles_preserve_exact_clipped_influence": (
        {"field expansion must reject a foreign extent", "clipping must not hide overflowing declared geometry",
         "field expansion must contain exactly the clipped radius on every edge"},
        {"zero radius is identity", "both horizontal edges clipped",
         "expansion overflow rejected", "foreign expansion extent rejected"},
    ),
    "field_effect::proofs::gaussian_sampling_clamps_without_coordinate_wrap": (
        {"gaussian sample must reject unrepresentable index or coordinate",
         "gaussian sample must equal exact clamped coordinate"},
        {"gaussian left edge clamp", "gaussian right edge clamp", "gaussian interior sample", "gaussian index overflow"},
    ),
})
MUTANTS += (
    ("field-lighter-low-cap", "field_effect.rs",
     "field_effect::proofs::premultiplied_admission_and_lighter_are_exact",
     "lighter must clamp the exact sum of every channel including alpha",
     "            .min(u16::from(u8::MAX)) as u8;", "            .min(u16::from(u8::MAX) - 1) as u8;"),
    ("field-source-over-biased-rounding", "field_effect.rs",
     "field_effect::proofs::premultiplied_source_over_has_unique_nearest_integer_output",
     "premultiplied source-over must return the unique nearest rational value",
     "        let attenuated = (u16::from(destination[channel]) * inverse_alpha + 127) / 255;",
     "        let attenuated = (u16::from(destination[channel]) * inverse_alpha + 126) / 255;"),
    ("field-empty-rectangle", "field_effect.rs",
     "field_effect::proofs::rectangles_admit_exactly_nonempty_in_bounds_geometry",
     "field rectangle must admit exactly its nonempty bounded geometry",
     "        if width == 0 || height == 0 {\n            return Err(FieldEvaluationErrorV1::EmptyRect);\n        }",
     "        if false {\n            return Err(FieldEvaluationErrorV1::EmptyRect);\n        }"),
    ("field-truncated-influence", "field_effect.rs",
     "field_effect::proofs::expanded_rectangles_preserve_exact_clipped_influence",
     "field expansion must contain exactly the clipped radius on every edge",
     "        let left = self.x.saturating_sub(radius);", "        let left = self.x;"),
    ("field-sample-outside-edge", "field_effect.rs",
     "field_effect::proofs::gaussian_sampling_clamps_without_coordinate_wrap",
     "gaussian sample must equal exact clamped coordinate",
     "                Ok(limit - 1)", "                Ok(limit)"),
)
SOURCES += ("field_effect.rs",)


CONTRACTS.update({
    "program::attachment::handoff::proofs::preparation_is_exact_and_has_no_publication_effect": (
        {"handoff preparation must retain the exact previous and desired state",
         "handoff command must preserve every publication field",
         "handoff command must preserve the requested operation",
         "handoff preparation must reject exactly wrong stamp scope or revision",
         "preparing or abandoning a handoff must never publish or advance state"},
        {"valid set prepared", "valid revoke prepared", "published value confirmed",
         "empty value confirmed", "foreign stamp rejected", "foreign scope rejected", "foreign revision rejected"},
    ),
    "program::attachment::handoff::proofs::rejected_install_is_atomic_retryable_and_success_is_once_only": (
        {"handoff must propagate the exact host refusal",
         "failed handoff must preserve committed and retryable state",
         "exact handoff retry must remain usable",
         "successful handoff must commit the complete acknowledged command exactly once",
         "a consumed handoff must never call the host again",
         "finishing the handoff must preserve its acknowledged result"},
        {"rejected set retried", "rejected revoke retried",
         "exhausted stamp still permits confirmation", "protocol refusal is retryable"},
    ),
    "program::attachment::handoff::proofs::stale_prepared_command_never_reaches_the_host": (
        {"stale prepared handoff must fail before any host call or state mutation"},
        {"same sequence foreign epoch rejected", "same epoch foreign sequence rejected"},
    ),
})
MUTANTS += (
    ("handoff-foreign-confirmation", "program/attachment/handoff.rs",
     "program::attachment::handoff::proofs::preparation_is_exact_and_has_no_publication_effect",
     "handoff preparation must reject exactly wrong stamp scope or revision",
     "                if current.revision != Some(revision) {", "                if false {"),
    ("handoff-swallowed-host-refusal", "program/attachment/handoff.rs",
     "program::attachment::handoff::proofs::rejected_install_is_atomic_retryable_and_success_is_once_only",
     "handoff must propagate the exact host refusal",
     "            .map_err(HandoffPointSinkErrorV1::Host)?;",
     "            .map_err(HandoffPointSinkErrorV1::Host).unwrap_or(());"),
    ("handoff-repeated-install", "program/attachment/handoff.rs",
     "program::attachment::handoff::proofs::rejected_install_is_atomic_retryable_and_success_is_once_only",
     "successful handoff must commit the complete acknowledged command exactly once",
     "        self.staged = None;", "        self.staged = Some(staged);"),
    ("handoff-stale-install", "program/attachment/handoff.rs",
     "program::attachment::handoff::proofs::stale_prepared_command_never_reaches_the_host",
     "stale prepared handoff must fail before any host call or state mutation",
     "        if self.writer.committed.stamp != staged.expected_stamp {", "        if false {"),
)
SOURCES += ("program/attachment/handoff.rs",)


# Конъюнкция четырёх конкретных критериев равносильна прежнему одному
# обязательству с символическим выбором критерия. Ни одна часть не факультативна.
WCAG_INTERVAL_PARTITIONS = (
    "interval_default_text_is_sound_for_every_enclosed_point",
    "interval_large_text_is_sound_for_every_enclosed_point",
    "interval_ui_component_is_sound_for_every_enclosed_point",
    "interval_graphical_object_is_sound_for_every_enclosed_point",
)
for partition in WCAG_INTERVAL_PARTITIONS:
    CONTRACTS[f"wcag22::kernel::proofs::{partition}"] = (
        {"interval PASS must hold for every enclosed colour pair",
         "interval FAIL must hold for every enclosed colour pair",
         "interval classification must preserve polarity symmetry"},
        {"interval passes", "interval fails", "overlapping threshold stays uncertain"},
    )
MUTANTS += (
    ("wcag-optimistic-interval-pass", "wcag22/kernel.rs",
     "wcag22::kernel::proofs::interval_large_text_is_sound_for_every_enclosed_point",
     "interval PASS must hold for every enclosed colour pair",
     "            10 * light_lower >= 30 * dark_upper + scale,",
     "            10 * light_upper >= 30 * dark_lower + scale,"),
)
