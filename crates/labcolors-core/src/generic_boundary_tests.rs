const APPEARANCE_SOURCE: &str = include_str!("appearance.rs");
const LCS_OCCURRENCE_SOURCE: &str = include_str!("lcs_occurrence.rs");
const OBSERVATION_SOURCE: &str = include_str!("observation.rs");
const POINT_SUPPORT_SOURCE: &str = include_str!("point_support.rs");
const PROGRAM_SESSION_SOURCE: &str = include_str!("program_session.rs");
const SESSION_SOURCE: &str = include_str!("session.rs");
const LIB_SOURCE: &str = include_str!("lib.rs");

const GENERIC_SOURCES: [(&str, &str); 3] = [
    ("appearance.rs", APPEARANCE_SOURCE),
    ("lcs_occurrence.rs", LCS_OCCURRENCE_SOURCE),
    ("program_session.rs", PROGRAM_SESSION_SOURCE),
];

const CLIENT_OR_LEGACY_VOCABULARY: [&str; 15] = [
    "Lab UI",
    "ThemeConfig",
    "RoleRecipe",
    "RoleSpec",
    "NamedRoleTable",
    "PairFill",
    "PairLabel",
    "Glow",
    "Material",
    "Ladder",
    "AlphaAnalog",
    "themeHandle",
    "resolveTheme",
    "Primary",
    "Danger",
];

fn source_scope<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start = source
        .find(start)
        .unwrap_or_else(|| panic!("missing source boundary start `{start}`"));
    let tail = &source[start..];
    let end = tail
        .find(end)
        .unwrap_or_else(|| panic!("missing source boundary end `{end}` after `{start}`"));
    &tail[..end]
}

fn normalized_source_scope(source: &str, start: &str, end: &str) -> String {
    source_scope(source, start, end)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn generic_physical_and_transport_modules_contain_no_client_or_legacy_vocabulary() {
    for (path, source) in GENERIC_SOURCES {
        for forbidden in CLIENT_OR_LEGACY_VOCABULARY {
            assert!(
                !source.contains(forbidden),
                "{path} must remain client-semantic agnostic; found `{forbidden}`"
            );
        }
    }
}

#[test]
fn shared_observation_ssot_has_one_backing_without_lifecycle_or_adapter_facades() {
    assert_eq!(
        normalized_source_scope(
            OBSERVATION_SOURCE,
            "struct ObservedScenarioSet {",
            "impl ObservedScenarioSet",
        ),
        concat!(
            "struct ObservedScenarioSet { ",
            "cases: Box<[PhysicalScenario]>, ",
            "values: Box<[Srgb8]>, ",
            "provenance: Box<[ScenarioId]>, ",
            "}",
        ),
        "the canonical scenario set must keep one flat physical backing",
    );
    assert_eq!(
        normalized_source_scope(
            OBSERVATION_SOURCE,
            "struct ObservationBackingV1 {",
            "/// Sealed observation admitted",
        ),
        concat!(
            "struct ObservationBackingV1 { ",
            "schema: CanonicalObservationSchemaV1, ",
            "set: ObservedScenarioSet, ",
            "}",
        ),
        "the shared backing must own only canonical schema and scenario data",
    );
    assert_eq!(
        normalized_source_scope(
            OBSERVATION_SOURCE,
            "pub(crate) struct RevisionBoundObservationV1 {",
            "impl RevisionBoundObservationV1",
        ),
        concat!(
            "pub(crate) struct RevisionBoundObservationV1 { ",
            "stream: ObservationStreamId, ",
            "revision: Revision, ",
            "backing: Rc<ObservationBackingV1>, ",
            "}",
        ),
        "revision identity must wrap exactly one shared immutable backing",
    );
    assert_eq!(
        OBSERVATION_SOURCE
            .matches("pub(crate) struct RevisionBoundObservationV1")
            .count(),
        1,
        "production must define exactly one revision-bound observation value",
    );
    assert!(
        OBSERVATION_SOURCE.contains("use std::rc::Rc;"),
        "single-threaded Core observations must use Rc",
    );
    assert!(
        OBSERVATION_SOURCE
            .contains("pub(crate) struct CanonicalObservationSchemaV1(Rc<[SurfaceInputPortId]>);"),
        "compiled schema and observations must share the same Rc-backed schema",
    );
    for forbidden in ["std::sync::Arc", "Arc<", "RefCell<", "Mutex<", "RwLock<"] {
        assert!(
            !OBSERVATION_SOURCE.contains(forbidden),
            "immutable single-threaded observation backing must not acquire `{forbidden}`",
        );
    }

    assert_eq!(
        normalized_source_scope(
            SESSION_SOURCE,
            "pub(crate) enum PointSupportSessionStateV1 {",
            "impl PointSupportSessionStateV1",
        ),
        concat!(
            "pub(crate) enum PointSupportSessionStateV1 { ",
            "Waiting, ",
            "Ready { current: VerifiedPointSupportV1, }, ",
            "Stale { previous: VerifiedPointSupportV1, }, ",
            "Failed { cause: PointSupportViolationV1, previous: Option<VerifiedPointSupportV1>, }, ",
            "}",
        ),
        "lifecycle state must not duplicate the current raw observation",
    );
    assert_eq!(
        normalized_source_scope(
            SESSION_SOURCE,
            "enum SessionObservationHeadV1 {",
            "impl SessionObservationHeadV1",
        ),
        concat!(
            "enum SessionObservationHeadV1 { ",
            "Empty, ",
            "Unknown(RevisionBoundUnknownV1), ",
            "Observed(crate::observation::RevisionBoundObservationV1), ",
            "}",
        ),
        "raw Empty/Unknown/Observed must remain separate from lifecycle state",
    );
    let session_owner = source_scope(
        SESSION_SOURCE,
        "pub(crate) struct PointSupportSessionV1 {",
        "impl PointSupportSessionV1",
    );
    for required in [
        "raw_head: SessionObservationHeadV1,",
        "state: PointSupportSessionStateV1,",
    ] {
        assert_eq!(
            session_owner.matches(required).count(),
            1,
            "Session must own exactly one `{required}` field",
        );
    }
    for forbidden in [
        "current_unknown",
        "observation: RevisionBoundObservationV1",
        "unknown: RevisionBoundUnknownV1",
    ] {
        assert!(
            !session_owner.contains(forbidden),
            "Session owner must not duplicate raw storage through `{forbidden}`",
        );
    }

    let consuming_entry = source_scope(
        POINT_SUPPORT_SOURCE,
        "impl BoundPointSupportRecheckV1 {",
        "pub(crate) enum PointSupportEvaluationErrorV1",
    );
    for required in [
        "observation: RevisionBoundObservationV1,",
        "evaluate_bound_point_support(self, &observation)?",
        "assessment.bind(observation)",
    ] {
        assert!(
            consuming_entry.contains(required),
            "point support must consume the shared observation directly; missing `{required}`",
        );
    }
    for forbidden in [
        ".clone()",
        ".to_vec()",
        "ObservationAdapter",
        "adapt_observation",
    ] {
        assert!(
            !consuming_entry.contains(forbidden),
            "point-support entry must not introduce observation façade `{forbidden}`",
        );
    }

    let evaluator = source_scope(
        POINT_SUPPORT_SOURCE,
        "fn evaluate_bound_point_support(",
        "fn reference_distance(",
    );
    assert!(
        evaluator.contains("observation: &RevisionBoundObservationV1"),
        "the evaluator must borrow the shared revision-bound observation",
    );
    assert_eq!(
        evaluator.matches(".physical_values(case_index)").count(),
        1,
        "point support must read each canonical physical case through the observation API",
    );
    let physical_values = evaluator
        .find(".physical_values(case_index)")
        .expect("physical-values route must exist");
    let indexed_surface = evaluator[physical_values..]
        .find("values.get(surface_index)")
        .expect("the prebound surface index must read from physical values");
    assert!(
        indexed_surface > 0,
        "surface lookup must follow the canonical physical-values projection",
    );
    for forbidden in [
        "physical_bindings",
        "SurfaceInputBinding",
        ".clone()",
        ".to_vec()",
        "ObservationAdapter",
        "adapt_observation",
    ] {
        assert!(
            !evaluator.contains(forbidden),
            "point evaluator must not reconstruct or adapt observations through `{forbidden}`",
        );
    }

    let report = source_scope(
        POINT_SUPPORT_SOURCE,
        "pub(crate) struct RevisionBoundPointSupportReportV1 {",
        "impl RevisionBoundPointSupportReportV1",
    );
    assert_eq!(
        report
            .matches("observation: RevisionBoundObservationV1,")
            .count(),
        1,
        "a report must own the same revision-bound observation without a parallel snapshot",
    );

    for (path, source) in [
        ("observation.rs", OBSERVATION_SOURCE),
        ("session.rs", SESSION_SOURCE),
        ("point_support.rs", POINT_SUPPORT_SOURCE),
    ] {
        for forbidden in [
            "FrozenObservationV1",
            "PriorObservation",
            "Availability",
            "ObservationSnapshot",
            "WaitingRecheckV1",
            "StaleRecheckV1",
            "PresentationHoldV1",
            "HoldErrorV1",
            "reuse_for",
            "ReuseErrorV1",
            "FinalRecheckOutcomeV1",
            "ObservationAdapter",
            "adapt_observation",
        ] {
            assert!(
                !source.contains(forbidden),
                "{path} must not restore compatibility or adapter API `{forbidden}`",
            );
        }
    }
}

#[test]
fn encoded_point_transport_does_not_claim_lcs_observation_types() {
    for forbidden in ["TristimulusSample", "LcsOccurrence", "AppearanceState"] {
        assert!(
            !PROGRAM_SESSION_SOURCE.contains(forbidden),
            "program_session.rs is encoded point transport, not `{forbidden}` evidence"
        );
    }
}

#[test]
fn program_session_module_docs_disclaim_transport_only_scope() {
    let module_docs = PROGRAM_SESSION_SOURCE
        .lines()
        .take_while(|line| line.starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();

    for required in ["transport-only", "encoded", "not", "lcs", "evidence"] {
        assert!(
            module_docs.contains(required),
            "program_session.rs module docs must explicitly disclaim transport-only scope; missing `{required}`"
        );
    }
}

#[test]
fn pre_f2_outputs_reuse_the_shared_encoded_point_paint_without_a_parallel_value() {
    fn declaration<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        let start = source
            .find(start)
            .unwrap_or_else(|| panic!("missing S0 declaration start `{start}`"));
        let end = source[start..]
            .find(end)
            .map(|offset| start + offset)
            .unwrap_or_else(|| panic!("missing S0 declaration end `{end}`"));
        &source[start..end]
    }

    assert!(
        LIB_SOURCE.contains("pub(crate) mod program_session;"),
        "the pre-F2 execution slice must remain crate-private",
    );
    assert!(
        !PROGRAM_SESSION_SOURCE.contains("OutputPaintV1"),
        "S0 allows only appearance::EncodedPointPaintV1 as the physical point Paint value",
    );

    let output_value = declaration(
        PROGRAM_SESSION_SOURCE,
        "pub struct OutputValueV1 {",
        "impl OutputValueV1 {",
    );
    let fields_start = output_value
        .find('{')
        .expect("OutputValueV1 declaration must open its field list");
    let fields_end = output_value
        .rfind('}')
        .expect("OutputValueV1 declaration must close its field list");
    let fields = output_value[fields_start + 1..fields_end]
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(
        fields,
        ["output: OutputSlotId,", "value: EncodedPointPaintV1,"],
        "OutputValueV1 must own exactly one route and the shared S0 Paint",
    );
    assert!(
        !output_value.contains("PaintId"),
        "OutputValueV1 must not store any PaintId beside the same ID in EncodedPointPaintV1",
    );

    let output_value_impl = declaration(
        PROGRAM_SESSION_SOURCE,
        "impl OutputValueV1 {",
        "struct ExecutionFrame",
    );
    for required in [
        "self.value.id()",
        "pub const fn value(self) -> EncodedPointPaintV1",
    ] {
        assert!(
            output_value_impl.contains(required),
            "OutputValueV1 must project directly from its shared S0 Paint; missing `{required}`",
        );
    }

    let materialization = declaration(
        PROGRAM_SESSION_SOURCE,
        "for (index, output) in epoch.outputs.iter().enumerate() {",
        "if has_hard_violation",
    );
    let nominal_guard = materialization
        .find("if paint.id() != output.paint_id {")
        .expect("routed output must fail closed on compiled/materialized Paint ID drift");
    let exact_copy = materialization
        .find("value: *paint,")
        .expect("routed output must copy the exact graph-materialized Paint");
    assert!(
        nominal_guard < exact_copy,
        "nominal Paint ID drift must be rejected before the graph Paint is routed",
    );
    assert!(
        materialization[nominal_guard..exact_copy]
            .contains("return Err(SessionUpdateError::InternalInvariant);"),
        "compiled/materialized Paint ID drift must fail closed",
    );
    assert!(
        materialization[exact_copy..].contains("value: *paint,"),
        "the routed value must copy the exact graph-materialized EncodedPointPaintV1",
    );
    for forbidden in ["source: paint.source()", "straight_alpha: paint.opacity()"] {
        assert!(
            !materialization.contains(forbidden),
            "graph materialization must not be reconstructed through `{forbidden}`",
        );
    }

    let module_docs = PROGRAM_SESSION_SOURCE
        .lines()
        .take_while(|line| line.starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n");
    for required in [
        "private pre-F2",
        "does not mint a terminal output certificate",
    ] {
        assert!(
            module_docs.contains(required),
            "the routed pre-F2 value must not claim a public terminal certificate; missing `{required}`",
        );
    }
}

#[test]
fn current_og0_program_epoch_has_one_explicit_group_without_scenario_scope_creep() {
    fn declaration<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        let start = source
            .find(start)
            .unwrap_or_else(|| panic!("missing OG0 declaration start `{start}`"));
        let end = source[start..]
            .find(end)
            .map(|offset| start + offset)
            .unwrap_or_else(|| panic!("missing OG0 declaration end `{end}`"));
        &source[start..end]
    }

    // This pins only the current private OG0 Program/epoch shape. It does not
    // prescribe how a future explicit join or independent component API works.
    let program = declaration(
        PROGRAM_SESSION_SOURCE,
        "pub struct Program<",
        "impl<Evaluation> Program",
    );
    let epoch = declaration(
        PROGRAM_SESSION_SOURCE,
        "struct ProgramEpochV1<",
        "/// Fully validated immutable Program",
    );
    assert_eq!(
        program
            .matches("observation_group: ObservationGroup,")
            .count(),
        1,
        "the current OG0 authored Program owns one explicit atomic group"
    );
    assert_eq!(
        epoch
            .matches("observation_group: CompiledObservationGroupV1,")
            .count(),
        1,
        "the current OG0 epoch must retain that one compiled group"
    );
    for (name, scope) in [("Program", program), ("ProgramEpochV1", epoch)] {
        for forbidden in ["Vec<ObservationGroup>", "Scenario"] {
            assert!(
                !scope.contains(forbidden),
                "current OG0 {name} declaration must not grow `{forbidden}` scope"
            );
        }
    }

    for forbidden in [
        "Vec<ObservationGroup>",
        "ScenarioId",
        "ScenarioSet",
        "ObservedScenarioSet",
        "GraphTemplate",
        "Cartesian",
        "wasm_bindgen",
        "serde",
        "Dto",
        "DTO",
    ] {
        assert!(
            !PROGRAM_SESSION_SOURCE.contains(forbidden),
            "current OG0 transport module must not acquire `{forbidden}` scope"
        );
    }
}

#[test]
fn f0_signal_transform_has_no_renderer_alias_or_raw_xyz_production_constructor() {
    for forbidden in [
        "RenderProfileId",
        "AppearanceContextReleaseId",
        "enum ObserveError",
        "fn observe(",
        "pub(crate) fn new(\n        xyz",
    ] {
        assert!(
            !LCS_OCCURRENCE_SOURCE.contains(forbidden),
            "lcs_occurrence.rs must keep the F0 identity boundary sealed; found `{forbidden}`",
        );
    }

    for required in [
        "ColorimetricTransformReleaseId",
        "AppearanceContextSchemaReleaseId",
        "fn admitted_binding(",
        "match output_profile",
        "fn derive_sample_with_binding(",
        "binding.transform_release()",
        "xyz_d65_from_srgb8_v1",
        "ModeledTristimulusProvenanceV1",
        "ModeledTristimulusDerivationV1",
    ] {
        assert!(
            LCS_OCCURRENCE_SOURCE.contains(required),
            "lcs_occurrence.rs must retain the sealed F0 route; missing `{required}`",
        );
    }

    let raw_xyz_declaration = LCS_OCCURRENCE_SOURCE
        .lines()
        .find(|line| line.contains("fn try_from_registered_xyz("))
        .expect("registered raw-XYZ admission must remain visible to this source gate");
    assert_eq!(
        raw_xyz_declaration.trim(),
        "fn try_from_registered_xyz(",
        "registered raw-XYZ admission must remain module-private",
    );

    let raw_frame_declaration = LCS_OCCURRENCE_SOURCE
        .lines()
        .find(|line| line.contains("const fn registered("))
        .expect("registered frame constructor must remain visible to this source gate");
    assert_eq!(
        raw_frame_declaration.trim(),
        "const fn registered(",
        "registered frame construction must remain module-private",
    );

    let dispatch_start = LCS_OCCURRENCE_SOURCE
        .find("fn admitted_binding(")
        .expect("binding dispatch must remain present");
    let dispatch_end = LCS_OCCURRENCE_SOURCE[dispatch_start..]
        .find("/// Derive one modeled tristimulus")
        .map(|offset| dispatch_start + offset)
        .expect("modeled derivation docs must delimit the dispatch source gate");
    let dispatch_source = &LCS_OCCURRENCE_SOURCE[dispatch_start..dispatch_end];
    assert!(
        !dispatch_source.contains("_ =>"),
        "closed F0 profile/transform dispatch must not silently fall back",
    );
}

#[test]
fn f0_modeled_derivation_mints_no_observation_or_bounded_evidence() {
    for forbidden in [
        "BoundEvidence",
        "NumericalDecisionEvidenceV1",
        "CanonicalFiniteBoundedEvidenceV1",
        "SourceOverCertificateV1",
        "GlowCompositeCertificateV1",
        "RendererCapability",
        "RenderObservation",
        "crate::constraints",
        "crate::wcag22_evidence",
    ] {
        assert!(
            !LCS_OCCURRENCE_SOURCE.contains(forbidden),
            "deterministic signal lowering cannot mint `{forbidden}`",
        );
    }
}
