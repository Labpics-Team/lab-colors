const APPEARANCE_SOURCE: &str = include_str!("appearance.rs");
const CONSTRAINTS_SOURCE: &str = include_str!("constraints/mod.rs");
const EXACT_CONSTRAINT_SOURCE: &str = include_str!("constraints/exact.rs");
const JOINT_SOURCE: &str = include_str!("joint.rs");
const LIB_SOURCE: &str = include_str!("lib.rs");
const LCS_OCCURRENCE_SOURCE: &str = include_str!("lcs_occurrence.rs");
const OBSERVATION_SOURCE: &str = include_str!("observation.rs");
const OUTPUT_PROJECTION_SOURCE: &str = include_str!("output_projection.rs");
const POINT_SUPPORT_SOURCE: &str = include_str!("point_support.rs");
const PROGRAM_SESSION_SOURCE: &str = include_str!("program_session.rs");
const SESSION_SOURCE: &str = include_str!("session.rs");
const WCAG22_CONSTRAINT_SOURCE: &str = include_str!("constraints/wcag22.rs");

const GENERIC_SOURCES: [(&str, &str); 3] = [
    ("appearance.rs", APPEARANCE_SOURCE),
    ("lcs_occurrence.rs", LCS_OCCURRENCE_SOURCE),
    ("program_session.rs", PROGRAM_SESSION_SOURCE),
];

const CLIENT_OR_LEGACY_VOCABULARY: [&str; 13] = [
    "Lab UI",
    "ThemeConfig",
    "RoleRecipe",
    "RoleSpec",
    "NamedRoleTable",
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

fn contains_rust_identifier(source: &str, identifier: &str) -> bool {
    fn is_continue(character: char) -> bool {
        character == '_' || character.is_ascii_alphanumeric()
    }

    source.match_indices(identifier).any(|(start, _)| {
        let before = source[..start].chars().next_back();
        let after = source[start + identifier.len()..].chars().next();
        !before.is_some_and(is_continue) && !after.is_some_and(is_continue)
    })
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
            "values: Box<[ColorSignal]>, ",
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
            "pub(crate) enum SessionState<Verified, Violation> {",
            "impl<Verified, Violation> SessionState<Verified, Violation>",
        ),
        concat!(
            "pub(crate) enum SessionState<Verified, Violation> { ",
            "Waiting, ",
            "Ready { current: Verified, }, ",
            "Stale { previous: Verified, }, ",
            "Failed { cause: Violation, previous: Option<Verified>, }, ",
            "}",
        ),
        "lifecycle state must not duplicate the current raw observation",
    );
    assert_eq!(
        normalized_source_scope(
            SESSION_SOURCE,
            "enum SessionObservationHeadV1 {",
            "impl ObservationOwnerV1 for SessionObservationHeadV1",
        ),
        concat!(
            "enum SessionObservationHeadV1 { ",
            "Empty, ",
            "Unknown(RevisionBoundUnknownV1), ",
            "Observed(RevisionBoundObservationV1), ",
            "}",
        ),
        "raw Empty/Unknown/Observed must remain separate from lifecycle state",
    );
    let session_owner = source_scope(
        SESSION_SOURCE,
        "pub(crate) struct Session<Plan: SessionPlanV1> {",
        "impl<Plan: SessionPlanV1> Session<Plan>",
    );
    for required in [
        "schema: CanonicalObservationSchemaV1,",
        "raw_head: SessionObservationHeadV1,",
        "state: SessionState<Plan::Verified, Plan::Violation>,",
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
    assert_eq!(
        SESSION_SOURCE.matches("pub(crate) struct Session<").count(),
        1,
        "production must have exactly one generic revision-bound Session owner",
    );
    for required in [
        "type Verified: SessionEvidenceV1;",
        "type Violation: SessionEvidenceV1;",
        ".is_same_binding_as(expected_observation)",
        "SessionUpdateError::EvidenceBindingInvariant",
    ] {
        assert!(
            SESSION_SOURCE.contains(required),
            "Session must reject detached evaluator evidence; missing `{required}`",
        );
    }
    assert_eq!(
        POINT_SUPPORT_SOURCE
            .matches("impl SessionPlanV1 for CompiledPointSupportRecheckV1")
            .count()
            + PROGRAM_SESSION_SOURCE
                .matches("SessionPlanV1 for ProgramSessionPlan<Evaluation>")
                .count(),
        2,
        "only the point-support and Program compiled plans may inhabit Session",
    );
    for (path, source) in [
        ("session.rs", SESSION_SOURCE),
        ("point_support.rs", POINT_SUPPORT_SOURCE),
        ("program_session.rs", PROGRAM_SESSION_SOURCE),
    ] {
        for forbidden in [
            "PointSupportSessionV1",
            "PointSupportSessionStateV1",
            "BoundPointSupportRecheckV1",
            "into_session_recheck",
            "ObservationStreamBinding",
            "ProgramExpired",
            "Weak<",
        ] {
            assert!(
                !source.contains(forbidden),
                "{path} must not restore a second owner or adapter `{forbidden}`",
            );
        }
    }

    let consuming_entry = source_scope(
        POINT_SUPPORT_SOURCE,
        "impl SessionPlanV1 for CompiledPointSupportRecheckV1 {",
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
        .find("values.get(*surface_index)")
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
fn program_session_owns_context_bound_lcs_evidence_and_one_session_scratch_cache() {
    for required in [
        "ProgramPointTargetV1",
        "ModeledLcsOccurrenceV1",
        "AppearanceContextId",
        "ProgramVisiblePointPassEvidence",
        "ProgramVisiblePointViolationEvidence",
        "ModeledLcsOccurrenceV1::from_signal_in_context(",
        "source.visible() != source.certificate().output_rgb()",
        "binding.context,",
        "assess_program_point_hard(",
    ] {
        assert!(
            PROGRAM_SESSION_SOURCE.contains(required),
            "Program must retain physical-source and context-bound LCS evidence; missing `{required}`",
        );
    }

    let program_evidence_binding = normalized_source_scope(
        CONSTRAINTS_SOURCE,
        "pub(crate) struct ProgramVisiblePointBindingV1 {",
        "impl ProgramVisiblePointBindingV1",
    );
    for required in [
        "physical: VisiblePointBindingV1,",
        "modeled_lcs: ModeledLcsOccurrenceV1,",
    ] {
        assert!(
            program_evidence_binding.contains(required),
            "Program evidence must bind source-over and LCS context in one value; missing `{required}`",
        );
    }

    let result = source_scope(
        PROGRAM_SESSION_SOURCE,
        "impl<Evaluation> ProgramConstraintResultV1<Evaluation>",
        "/// One canonical `physical case × constraint` report cell.",
    );
    for required in [
        "fn binding(&self) -> ProgramVisiblePointBindingV1",
        "fn modeled_lcs_occurrence(&self) -> ModeledLcsOccurrenceV1",
        "self.binding().modeled_lcs()",
    ] {
        assert!(
            result.contains(required),
            "Program result must project modeled LCS from its evidence SSOT; missing `{required}`",
        );
    }
    let cell = source_scope(
        PROGRAM_SESSION_SOURCE,
        "pub struct ProgramConstraintCellV1<Evaluation>",
        "impl<Evaluation> ProgramConstraintCellV1<Evaluation>",
    );
    assert!(
        cell.contains("result: ProgramConstraintResultV1<Evaluation>,"),
        "each Program cell must own the typed evidence result",
    );
    assert!(
        !cell.contains("modeled_lcs_occurrence: ModeledLcsOccurrenceV1,"),
        "a Program cell must not duplicate the modeled occurrence already owned by evidence",
    );

    let plan = source_scope(
        PROGRAM_SESSION_SOURCE,
        "pub struct ProgramSessionPlan<Evaluation>",
        "impl<Evaluation> session_private::PlanSealed for ProgramSessionPlan<Evaluation>",
    );
    assert_eq!(
        plan.matches("modeled_occurrences: Vec<Option<ModeledLcsOccurrenceV1>>,")
            .count(),
        1,
        "each Program Session must own exactly one reusable modeled-occurrence scratch cache",
    );
    assert_eq!(
        PROGRAM_SESSION_SOURCE
            .matches("modeled_occurrences: Vec<Option<ModeledLcsOccurrenceV1>>,")
            .count(),
        1,
        "the modeled-occurrence scratch cache must not be duplicated outside the Session plan",
    );

    let preparation = normalized_source_scope(
        PROGRAM_SESSION_SOURCE,
        "let all_occurrence_contexts = compile_occurrence_contexts(",
        "let outputs = compile_outputs(",
    );
    for required in [
        "compile_constraints::<Evaluation>(&graph, &all_occurrence_contexts, program.constraints)?",
        "compact_constraint_contexts(&all_occurrence_contexts, &mut constraints)?",
    ] {
        assert!(
            preparation.contains(required),
            "Program compilation must compact full occurrence metadata to constrained targets; missing `{required}`",
        );
    }

    let compaction = normalized_source_scope(
        PROGRAM_SESSION_SOURCE,
        "fn compact_constraint_contexts<Invocation>(",
        "fn compile_outputs(",
    );
    for required in [
        "targets.sort_unstable(); targets.dedup();",
        ".binary_search_by_key(&constraint.target_id, |binding| binding.occurrence)",
        "constraint.modeled_occurrence_index = index;",
    ] {
        assert!(
            compaction.contains(required),
            "cold compilation must deduplicate targets and remap every constraint; missing `{required}`",
        );
    }

    let hot_evaluation = source_scope(
        PROGRAM_SESSION_SOURCE,
        "fn evaluate_program_session<Evaluation>(",
        "fn map_program_execution_binding_error<EvaluationError>(",
    );
    assert!(
        !hot_evaluation.contains("binary_search"),
        "hot Program evaluation must consume compile-time direct indices without searching",
    );
    for required in [
        "plan.modeled_occurrences.fill(None);",
        ".get(constraint.modeled_occurrence_index)",
        ".get_mut(constraint.modeled_occurrence_index)",
    ] {
        assert!(
            hot_evaluation.contains(required),
            "hot Program evaluation must reuse the compact direct-index cache; missing `{required}`",
        );
    }
}

#[test]
fn program_lcs_boundary_has_no_legacy_color_or_projection_shortcuts() {
    for forbidden in [
        "LcsColor",
        "ViewingConditions",
        "from_hex",
        "to_hex",
        "is_dark_theme",
        "CAM16-UCS",
        "ucs_",
        "ProjectionSourceV1",
        "default_context",
        "default context",
        "DefaultContext",
    ] {
        assert!(
            !PROGRAM_SESSION_SOURCE.contains(forbidden),
            "program_session.rs must not bypass explicit source/context admission through `{forbidden}`",
        );
    }

    for (path, source) in [
        ("point_support.rs", POINT_SUPPORT_SOURCE),
        ("joint.rs", JOINT_SOURCE),
    ] {
        for forbidden in [
            "ProgramPointTargetV1",
            "ProgramPointEvaluatorV1",
            "ProgramPointInvocation",
            "ProgramVisiblePointBindingV1",
            "ProgramVisiblePointPassEvidence",
            "ProgramVisiblePointViolationEvidence",
            "ModeledLcsOccurrenceV1",
            "AppearanceContextId",
            "LcsOccurrence",
        ] {
            assert!(
                !contains_rust_identifier(source, forbidden),
                "{path} must not absorb Program-only contextual LCS type `{forbidden}`",
            );
        }
    }

    for forbidden in [
        "PointEvaluatorV1",
        "PointInvocation",
        "VisiblePointBindingV1",
        "VisiblePointPassEvidence",
        "VisiblePointViolationEvidence",
        "assess_visible_point_hard",
    ] {
        assert!(
            !contains_rust_identifier(PROGRAM_SESSION_SOURCE, forbidden),
            "program_session.rs must not route Program through legacy point evidence `{forbidden}`",
        );
    }

    for forbidden in [
        "AppearanceState",
        "Cam16ViewV1",
        "Cam16SurroundV1",
        "ViewingConditions",
        "derive_cam16_view_v1",
        "forward_correlates_v1",
    ] {
        assert!(
            !contains_rust_identifier(PROGRAM_SESSION_SOURCE, forbidden),
            "program_session.rs hot lowering must not derive CAM16 through `{forbidden}`",
        );
    }
    assert!(
        !PROGRAM_SESSION_SOURCE.contains(".cam16("),
        "program_session.rs hot lowering must not request a CAM16 view",
    );

    assert!(
        !OUTPUT_PROJECTION_SOURCE.contains("ProjectionSourceV1"),
        "output_projection.rs must consume ModeledLcsOccurrenceV1 directly, not define ProjectionSourceV1",
    );
}

#[test]
fn existing_encoded_evaluators_delegate_program_targets_without_parallel_formulae() {
    for (path, source, evaluator, next_impl) in [
        (
            "constraints/exact.rs",
            EXACT_CONSTRAINT_SOURCE,
            "ExactSrgb8IdentityV1",
            "impl HardClassifier<Srgb8, Srgb8> for ExactSrgb8IdentityV1",
        ),
        (
            "constraints/wcag22.rs",
            WCAG22_CONSTRAINT_SOURCE,
            "Wcag22Srgb8V1",
            "impl HardClassifier<Wcag22CriterionV1, ApplicableWcag22MeasurementV1> for Wcag22Srgb8V1",
        ),
    ] {
        let start = format!("impl Evaluator<ProgramPointTargetV1> for {evaluator} {{");
        let implementation = source_scope(source, &start, next_impl);
        assert_eq!(
            implementation
                .matches("<Self as Evaluator<ModeledSrgb8PointOccurrence>>::evaluate(")
                .count(),
            1,
            "{path} Program evaluator must delegate exactly once to its encoded-target SSOT",
        );
        assert_eq!(
            implementation.matches(".encoded(),").count(),
            1,
            "{path} Program evaluator must pass only the typed encoded target to its SSOT",
        );
        assert!(
            !implementation.contains(".modeled_lcs()"),
            "{path} encoded evaluator must not silently grow a contextual LCS formula",
        );
    }
}

#[test]
fn private_program_and_lcs_occurrence_types_are_not_publicly_exported() {
    for required in [
        "pub(crate) mod lcs_occurrence;",
        "pub(crate) mod program_session;",
    ] {
        assert!(
            LIB_SOURCE.contains(required),
            "the pre-hard-cut boundary must remain crate-private; missing `{required}`",
        );
    }
    for forbidden in [
        "pub mod lcs_occurrence;",
        "pub mod program_session;",
        "pub use lcs_occurrence::",
        "pub use crate::lcs_occurrence::",
        "pub use program_session::",
        "pub use crate::program_session::",
    ] {
        assert!(
            !LIB_SOURCE.contains(forbidden),
            "lib.rs must not expose private Program/LCS occurrence surface `{forbidden}`",
        );
    }
}

#[test]
fn program_compiler_cannot_regrow_the_superseded_runtime_facade() {
    for forbidden in [
        "PointRenderOwner",
        "PointRenderAttachError",
        "ObservationStreamBinding",
        "SurfaceUpdate",
        "OutputValueV1",
        "ExecutionFrame",
        "SessionState",
    ] {
        assert!(
            !PROGRAM_SESSION_SOURCE.contains(forbidden),
            "program_session.rs must remain compiler/lowering-only; found superseded `{forbidden}`"
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
