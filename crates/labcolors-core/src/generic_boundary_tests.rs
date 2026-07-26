use std::ffi::OsStr;
use std::path::PathBuf;

const APPEARANCE_SOURCE: &str = include_str!("appearance.rs");
const CONSTRAINTS_SOURCE: &str = include_str!("constraints/mod.rs");
const EXACT_CONSTRAINT_SOURCE: &str = include_str!("constraints/exact.rs");
const JOINT_SOURCE: &str = include_str!("joint.rs");
const LIB_SOURCE: &str = include_str!("lib.rs");
const LCS_OCCURRENCE_SOURCE: &str = include_str!("lcs_occurrence.rs");
const OBSERVATION_SOURCE: &str = include_str!("observation.rs");
const OUTPUT_PROJECTION_SOURCE: &str = include_str!("output_projection.rs");
const PROGRAM_SOURCE: &str = include_str!("program.rs");
const POINT_SUPPORT_SOURCE: &str = include_str!("point_support.rs");
const PROGRAM_IDENTITY_SOURCE: &str = include_str!("program_identity.rs");
const PROGRAM_SESSION_SOURCE: &str = include_str!("program_session.rs");
const SESSION_SOURCE: &str = include_str!("session.rs");
const WCAG22_CONSTRAINT_SOURCE: &str = include_str!("constraints/wcag22.rs");

const GENERIC_SOURCES: [(&str, &str); 4] = [
    ("appearance.rs", APPEARANCE_SOURCE),
    ("lcs_occurrence.rs", LCS_OCCURRENCE_SOURCE),
    ("program_identity.rs", PROGRAM_IDENTITY_SOURCE),
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

fn assert_only_in_compile_fail(source: &str, needle: &str) {
    let mut in_compile_fail = false;
    let mut occurrences = 0;

    for (line_index, line) in source.lines().enumerate() {
        let doc = line.trim_start().strip_prefix("///").map(str::trim_start);
        match doc {
            Some("```compile_fail") => {
                assert!(
                    !in_compile_fail,
                    "nested compile_fail fence before line {}",
                    line_index + 1,
                );
                in_compile_fail = true;
                continue;
            }
            Some("```") if in_compile_fail => {
                in_compile_fail = false;
                continue;
            }
            _ => {}
        }

        assert!(
            !in_compile_fail || doc.is_some() || line.trim().is_empty(),
            "compile_fail sentinel was interrupted by live code at line {}",
            line_index + 1,
        );
        let line_occurrences = line.matches(needle).count();
        if line_occurrences == 0 {
            continue;
        }
        assert!(
            in_compile_fail && doc.is_some(),
            "`{needle}` escaped its negative compile_fail sentinel at line {}",
            line_index + 1,
        );
        occurrences += line_occurrences;
    }

    assert!(!in_compile_fail, "unclosed compile_fail fence");
    assert!(
        occurrences > 0,
        "`{needle}` must remain covered by a negative API sentinel",
    );
}

#[test]
fn compile_fail_scanner_rejects_live_code_between_document_fences() {
    let escaped = "/// ```compile_fail\npub type PackageProgram = u8;\n/// ```";
    assert!(
        std::panic::catch_unwind(|| assert_only_in_compile_fail(escaped, "PackageProgram"))
            .is_err(),
        "a live declaration must never inherit compile_fail state from adjacent documentation",
    );
}

fn production_rust_sources() -> Vec<(String, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut pending = vec![root.clone()];
    let mut sources = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("Core source directory must be readable")
        {
            let path = entry.expect("Core source entry must be readable").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let is_production_rust = path.extension() == Some(OsStr::new("rs"))
                && !path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| name.ends_with("_tests.rs"));
            if !is_production_rust {
                continue;
            }
            let relative = path
                .strip_prefix(&root)
                .expect("Core source must remain below its manifest root")
                .to_string_lossy()
                .into_owned();
            let source =
                std::fs::read_to_string(&path).expect("Core Rust source must be valid UTF-8");
            sources.push((relative, source));
        }
    }
    sources.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    sources
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
fn staged_program_module_is_private_module_qualified_and_transport_neutral() {
    let normalized_lib = LIB_SOURCE.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(
        LIB_SOURCE
            .lines()
            .filter(|line| line.trim() == "pub(crate) mod program;")
            .count(),
        1,
        "the crate root must retain exactly one crate-private file-backed Program module",
    );
    assert!(
        normalized_lib.contains("#[deny(missing_docs)] pub(crate) mod program;"),
        "the staged Program candidate must stay documented but crate-private",
    );
    assert!(
        !LIB_SOURCE
            .lines()
            .any(|line| line.trim() == "pub mod program;"),
        "the incomplete Program candidate must not become externally reachable",
    );
    assert!(
        PROGRAM_SOURCE
            .lines()
            .any(|line| line.trim() == "#![forbid(unreachable_pub)]"),
        "rustc must reject accidentally over-visible items inside the staged module",
    );

    let source_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    assert!(
        source_root.join("program.rs").is_file(),
        "the staged Program implementation must remain file-backed",
    );
    assert!(
        !source_root.join("package_bridge.rs").exists(),
        "the superseded transport-named module must not return",
    );
    assert!(
        !PROGRAM_SOURCE.contains("PackageProgram")
            && !contains_rust_identifier(PROGRAM_SOURCE, "package_bridge"),
        "the staged Program source must not retain transport-era vocabulary",
    );

    for (path, source) in production_rust_sources() {
        if path == "lib.rs" {
            continue;
        }
        assert!(
            !source.contains("PackageProgram")
                && !contains_rust_identifier(&source, "package_bridge"),
            "{path} must not retain the superseded public path or prefix",
        );
    }
    assert_only_in_compile_fail(LIB_SOURCE, "PackageProgram");
    assert_only_in_compile_fail(LIB_SOURCE, "package_bridge");
    assert_only_in_compile_fail(LIB_SOURCE, "use labcolors_core::program;");
}

#[test]
fn staged_program_draft_wraps_the_single_canonical_core_graph() {
    assert_eq!(
        normalized_source_scope(
            PROGRAM_SOURCE,
            "pub(crate) struct DraftV1 {",
            "/// Ошибка изменения Draft до компиляции.",
        ),
        "pub(crate) struct DraftV1 { inner: CoreProgramDraftV1, }",
        "the staged seam must forward actual IR nodes into the sole Core draft",
    );
    assert_eq!(
        normalized_source_scope(
            PROGRAM_SESSION_SOURCE,
            "pub(crate) struct CoreProgramDraftV1 {",
            "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub(crate) enum CoreProgramDraftErrorV1",
        ),
        "pub(crate) struct CoreProgramDraftV1 { program: CoreProgramV1, }",
        "the mutable Core seam must own the actual concrete Program, not mirror its fields",
    );
    for forbidden in [
        "PairFill",
        "PairLabel",
        "Glow",
        "Material",
        "Ladder",
        "AlphaAnalog",
        "ThemeConfig",
        "RoleRecipe",
        "serde",
        "serde_json",
        "String",
        "HashMap",
        "dyn Program",
        "OutputProfileId",
        "ObservationGroupIdV1",
        "OpacityIdV1",
        "SurfaceInputIdV1",
        "push_opacity(",
        "push_surface_input(",
        "surface_input_slots(",
    ] {
        assert!(
            !PROGRAM_SOURCE.contains(forbidden),
            "the staged concrete lowerer must not acquire `{forbidden}`",
        );
    }
}

#[test]
fn staged_session_keeps_evidence_but_owner_alone_grants_updates_and_operations() {
    assert_eq!(
        normalized_source_scope(
            PROGRAM_SOURCE,
            "pub(crate) struct SessionV1 {",
            "impl SessionV1",
        ),
        concat!(
            "pub(crate) struct SessionV1 { ",
            "scenario_order_scratch: Vec<usize>, ",
            "session: CoreProgramSessionV1, ",
            "}",
        ),
        "the staged Session must not duplicate owner schema, outputs, stream, or lifecycle state",
    );

    let session_api = source_scope(
        PROGRAM_SOURCE,
        "impl SessionV1 {",
        "struct ScenarioSourceV1",
    );
    assert_eq!(
        session_api.matches("pub(crate) fn evidence(").count(),
        1,
        "historical evidence is the Session's sole boundary projection",
    );
    assert_eq!(
        session_api.matches("pub(crate) ").count(),
        1,
        "Session must not expose a second authority by changing function qualifiers",
    );
    for forbidden in [
        "fn state(",
        "fn update(",
        "fn surface_input_port_count(",
        "fn surface_input_ports(",
        "fn output_slots(",
    ] {
        assert!(
            !session_api.contains(forbidden),
            "Session must not regain owner authority through `{forbidden}`",
        );
    }

    let evidence_api = source_scope(
        PROGRAM_SOURCE,
        "impl<'a> EvidenceViewV1<'a> {",
        "struct BorrowScopeV1<'owner, 'session>",
    );
    for forbidden in [
        "fn revision(",
        "const fn revision(",
        "fn stream(",
        "const fn stream(",
    ] {
        assert!(
            !evidence_api.contains(forbidden),
            "evidence must not flatten atomic raw-head provenance through `{forbidden}`",
        );
    }

    let owner_api = source_scope(
        PROGRAM_SOURCE,
        "impl OwnerV1 {",
        "pub(crate) struct ScenarioV1<'a> {",
    );
    for required in [
        "pub(crate) fn project<'owner, 'session>(",
        "pub(crate) fn update<'owner, 'session>(",
        ".owns_session(&session.session)",
    ] {
        assert!(
            owner_api.contains(required),
            "the exact owner must remain the only operation authority; missing `{required}`",
        );
    }
    let update = source_scope(
        owner_api,
        "pub(crate) fn update<'owner, 'session>(",
        "pub(crate) fn instantiate(",
    );
    assert!(
        update
            .find(".owns_session(&session.session)")
            .expect("owner update must preflight exact membership")
            < update
                .find("session.apply_update(update)?")
                .expect("owner update must delegate one atomic Session update"),
        "owner mismatch must be rejected before admission, allocation, or evaluation",
    );

    let staged_access_errors = source_scope(
        PROGRAM_SOURCE,
        "pub(crate) enum AccessErrorV1 {",
        "impl OwnerV1",
    );
    assert!(
        staged_access_errors.contains("OwnerMismatch,")
            && !staged_access_errors.contains("OwnerExpired"),
        "operation projection must distinguish foreign ownership, not expose internal expiry",
    );
    let staged_update_errors = source_scope(
        PROGRAM_SOURCE,
        "pub(crate) enum UpdateErrorKindV1 {",
        "pub(crate) enum UpdateErrorV1 {",
    );
    assert!(
        staged_update_errors.contains("OwnerMismatch,")
            && !staged_update_errors.contains("OwnerExpired"),
        "owner expiry is an internal invariant after a matching owner borrow",
    );
    assert!(
        PROGRAM_SOURCE.contains("pub(crate) enum UpdateErrorV1 {")
            && !PROGRAM_SOURCE.contains("pub(crate) struct UpdateErrorV1 {")
            && PROGRAM_SOURCE
                .contains("fn map_observation_error(error: ObservationError) -> UpdateErrorV1",)
            && PROGRAM_SOURCE
                .contains("fn map_plan_error(error: CoreProgramPlanErrorV1) -> UpdateErrorV1",),
        "update errors must retain payloads in the authoritative enum before kind projection",
    );

    for (payload, end) in [
        (
            "pub(crate) struct SetV1<'owner, 'session> {",
            "impl<'session> SetV1<'_, 'session>",
        ),
        (
            "pub(crate) struct RemoveV1<'owner, 'session> {",
            "impl RemoveV1<'_, '_>",
        ),
    ] {
        assert!(
            source_scope(PROGRAM_SOURCE, payload, end)
                .contains("_scope: BorrowScopeV1<'owner, 'session>,"),
            "{payload} must retain both owner and immutable Session borrows",
        );
    }
    assert!(
        !PROGRAM_SOURCE.contains("HoldV1") && !PROGRAM_SOURCE.contains("OperationV1::Hold"),
        "past evidence must not become a current emission authority",
    );
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
    assert!(
        OBSERVATION_SOURCE.contains(
            "#[derive(Debug, PartialEq, Eq)]\n#[cfg_attr(test, derive(Clone))]\npub(crate) struct CanonicalObservationSchemaV1",
        ),
        "production schema ownership must not expose a general Clone capability",
    );
    assert_eq!(
        OBSERVATION_SOURCE
            .matches("schema.share_for_observation()")
            .count(),
        2,
        "only keyed and schema-ordered admission may share a schema handle",
    );
    assert!(
        !OBSERVATION_SOURCE.contains("schema: schema.clone()"),
        "admission must use the private schema-sharing capability",
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
        "raw_head: SessionObservationHeadV1,",
        "state: SessionState<Plan::Verified, Plan::Violation>,",
    ] {
        assert_eq!(
            session_owner.matches(required).count(),
            1,
            "Session must own exactly one `{required}` field",
        );
    }
    assert!(
        !session_owner.contains("schema: CanonicalObservationSchemaV1,"),
        "the concrete plan is the sole Session-local owner of its canonical schema",
    );
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
        "type OwnerLease;",
        "type Verified: SessionEvidenceV1;",
        "type Violation: SessionEvidenceV1;",
        "fn try_acquire_owner(&self) -> Option<Self::OwnerLease>;",
        "owner: &'a Self::OwnerLease,",
        "SessionUpdateError::OwnerExpired",
        ".is_same_binding_as(expected_observation)",
        "SessionUpdateError::EvidenceBindingInvariant",
    ] {
        assert!(
            SESSION_SOURCE.contains(required),
            "Session must reject detached evaluator evidence; missing `{required}`",
        );
    }
    let session_plan_implementors = production_rust_sources()
        .into_iter()
        .filter_map(|(path, source)| {
            let count = source.matches("SessionPlanV1 for").count();
            (count != 0).then_some((path, count))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        session_plan_implementors,
        vec![
            ("point_support.rs".to_owned(), 1),
            ("program_session.rs".to_owned(), 1),
        ],
        "only the audited point-support and Program plans may inhabit Session",
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
        ] {
            assert!(
                !source.contains(forbidden),
                "{path} must not restore a second owner or adapter `{forbidden}`",
            );
        }
    }
    for (path, source) in [
        ("session.rs", SESSION_SOURCE),
        ("point_support.rs", POINT_SUPPORT_SOURCE),
    ] {
        assert!(
            !source.contains("Weak<"),
            "{path} must not create another weak ownership boundary",
        );
    }

    for (name, update, prepare) in [
        (
            "keyed",
            source_scope(
                SESSION_SOURCE,
                "pub(crate) fn update(",
                "/// Stream-affine `Unknown` admission",
            ),
            "prepare_observation(",
        ),
        (
            "schema-ordered",
            source_scope(
                SESSION_SOURCE,
                "pub(crate) fn update_schema_ordered",
                "fn apply_prepared_update",
            ),
            "prepare_schema_ordered_observation(",
        ),
    ] {
        let owner_preflight = update
            .find(".try_acquire_owner()")
            .unwrap_or_else(|| panic!("{name} update must acquire the exact owner generation"));
        let schema = update
            .find("let schema = self.plan.observation_schema(&owner);")
            .unwrap_or_else(|| panic!("{name} update must derive schema from that owner"));
        let admission = update
            .find(prepare)
            .unwrap_or_else(|| panic!("{name} update must perform canonical admission"));
        assert!(
            owner_preflight < schema && schema < admission,
            "{name} update must pin owner, derive its schema, then admit",
        );
        assert_eq!(
            update
                .matches("let schema = self.plan.observation_schema(&owner);")
                .count(),
            1,
            "{name} update must borrow exactly one schema",
        );
        assert!(
            !update.contains("observation_schema(&owner).clone()"),
            "{name} admission must not create a transient schema owner",
        );
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
fn program_session_keeps_physical_evidence_separate_from_lazy_lcs_capability() {
    for required in [
        "ProgramPointOccurrenceV1::from_resolved(source, binding.context)",
        "AppearanceContextId",
        "ProgramVisiblePointPassEvidence",
        "ProgramVisiblePointViolationEvidence",
        "source.visible() != source.certificate().output_rgb()",
        "assess_program_point_hard(",
    ] {
        assert!(
            PROGRAM_SESSION_SOURCE.contains(required),
            "Program must retain physical-source evidence and declared context; missing `{required}`",
        );
    }
    for forbidden in [
        "ModeledLcsOccurrenceV1::from_signal_in_context(",
        "modeled_occurrences: Vec<Option<ModeledLcsOccurrenceV1>>",
        "ProgramPointAssessmentErrorV1::Binding",
        "ProgramSessionEvaluationError::ModeledOccurrence",
    ] {
        assert!(
            !PROGRAM_SESSION_SOURCE.contains(forbidden),
            "encoded Program execution must not restore eager LCS path `{forbidden}`",
        );
    }

    let encoded_target = normalized_source_scope(
        CONSTRAINTS_SOURCE,
        "pub(crate) struct ProgramPointTargetV1 {",
        "impl ProgramPointTargetV1",
    );
    assert!(
        encoded_target.contains("encoded: ModeledSrgb8PointOccurrence,"),
        "encoded target must retain the physical sRGB8 point",
    );
    assert!(
        !encoded_target.contains("ModeledLcsOccurrenceV1")
            && !encoded_target.contains("AppearanceContextId"),
        "encoded evaluator target must expose neither a derived LCS view nor appearance context",
    );

    let program_evidence_binding = normalized_source_scope(
        CONSTRAINTS_SOURCE,
        "pub(crate) struct ProgramVisiblePointBindingV1 {",
        "impl ProgramVisiblePointBindingV1",
    );
    for required in [
        "physical: VisiblePointBindingV1,",
        "context: AppearanceContextId,",
    ] {
        assert!(
            program_evidence_binding.contains(required),
            "base Program evidence must bind source-over and declared context; missing `{required}`",
        );
    }
    assert!(
        !program_evidence_binding.contains("ModeledLcsOccurrenceV1"),
        "base Program evidence must not smuggle a derived LCS view",
    );
    let projected_binding = source_scope(
        PROGRAM_SOURCE,
        "impl<'a> PointBindingV1<'a> {",
        "/// Закрытое семейство точной физической композиции.",
    );
    assert!(projected_binding.contains("appearance_context(self)"));
    for forbidden in [
        "modeled(self)",
        "ModeledPointV1",
        ".modeled_lcs()",
        "xyz(self)",
    ] {
        assert!(
            !projected_binding.contains(forbidden),
            "base projected binding must not restore derived view `{forbidden}`",
        );
    }

    let result = source_scope(
        PROGRAM_SESSION_SOURCE,
        "impl<Evaluation> ProgramConstraintResultV1<Evaluation>",
        "/// One canonical `physical case × constraint` report cell.",
    );
    assert!(result.contains("fn binding(&self) -> ProgramVisiblePointBindingV1"));
    assert!(!result.contains("modeled_lcs"));
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
        "pub(crate) struct ProgramSessionPlan<Evaluation>",
        "impl<Evaluation> session_private::PlanSealed for ProgramSessionPlan<Evaluation>",
    );
    assert_eq!(
        plan.matches("owner_generation: Weak<ProgramEpochV1<Evaluation>>,")
            .count(),
        1,
        "a Program Session must hold exactly one weak compiled-generation binding",
    );
    assert!(
        !plan.contains("epoch: Rc<ProgramEpochV1<Evaluation>>,"),
        "a Program Session must not prolong its CompiledProgram owner",
    );
    assert!(
        !plan.contains("schema: CanonicalObservationSchemaV1,"),
        "a Program Session must derive schema from its pinned owner generation",
    );
    let instantiate = source_scope(
        PROGRAM_SESSION_SOURCE,
        "pub(crate) fn instantiate(",
        "/// Failure while preparing mutable storage",
    );
    assert!(
        !instantiate.contains("observation_group.schema.clone()"),
        "empty Program Sessions must not add persistent schema handles",
    );
    let compiled = source_scope(
        PROGRAM_SESSION_SOURCE,
        "pub struct CompiledProgram<Evaluation>",
        "impl<Evaluation> CompiledProgram<Evaluation>",
    );
    assert_eq!(
        compiled
            .matches("owner_generation: Rc<ProgramEpochV1<Evaluation>>,")
            .count(),
        1,
        "CompiledProgram must be the one strong owner of its generation",
    );
    assert!(!plan.contains("ModeledLcsOccurrenceV1"));

    let preparation = normalized_source_scope(
        PROGRAM_SESSION_SOURCE,
        "let all_occurrence_contexts = compile_occurrence_contexts(",
        "let outputs = compile_outputs(",
    );
    for required in [
        "compile_constraints::<Evaluation>(&graph, &all_occurrence_contexts, &program.constraints)?",
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
        "constraint.occurrence_context_index = index;",
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
    assert!(hot_evaluation.contains(".get(constraint.occurrence_context_index)"));

    for required in [
        "pub(crate) struct ProgramLcsPointAdapterV1",
        "OnceCell<Result<ModeledLcsOccurrenceV1, ModeledLcsOccurrenceFormationErrorV1>>",
        "pub(crate) struct ProgramLcsPointTargetV1",
        "pub(crate) struct ProgramLcsVisiblePointBindingV1",
        "assess_program_lcs_point_hard",
    ] {
        assert!(
            CONSTRAINTS_SOURCE.contains(required),
            "the explicit lazy LCS capability is incomplete; missing `{required}`",
        );
    }
    let lcs_adapter = source_scope(
        CONSTRAINTS_SOURCE,
        "pub(crate) struct ProgramLcsPointAdapterV1 {",
        "#[cfg(test)]\nimpl Evaluator<ProgramLcsPointTargetV1>",
    );
    assert!(
        !lcs_adapter.contains("Option<ModeledLcsOccurrenceV1>"),
        "LCS capability must be a typed adapter, not an optional field state",
    );
    let lcs_target_impl = normalized_source_scope(
        CONSTRAINTS_SOURCE,
        "impl ProgramLcsPointTargetV1 {",
        "/// One lazy derived LCS capability",
    );
    let lcs_binding_impl = normalized_source_scope(
        CONSTRAINTS_SOURCE,
        "impl ProgramLcsVisiblePointBindingV1 {",
        "type BoundProgramPointMeasurement",
    );
    for (surface, source) in [
        ("LCS target", lcs_target_impl.as_str()),
        ("LCS evidence", lcs_binding_impl.as_str()),
    ] {
        assert!(
            !source.contains("fn bind("),
            "{surface} must have no independent bind constructor",
        );
    }
    assert!(
        !CONSTRAINTS_SOURCE.contains("verify_program_lcs_point_binding"),
        "byte/context equality cannot certify physical occurrence identity",
    );
}

#[test]
fn program_identity_binds_lcs_releases_only_through_lcs_constraint_content() {
    let identity_sources = [
        ("program_identity.rs", PROGRAM_IDENTITY_SOURCE),
        ("program_session.rs", PROGRAM_SESSION_SOURCE),
        ("program.rs", PROGRAM_SOURCE),
    ];
    for retired in [
        "ProgramContentIdentityV1",
        "ContentIdentityV1",
        "DOMAIN_V1",
        "PROGRAM_SCHEMA_V1",
        "compile_program_content_identity_v1",
    ] {
        for (path, source) in identity_sources {
            assert!(
                !contains_rust_identifier(source, retired),
                "the V2 content-address cut must not retain legacy identity symbol `{retired}` in {path}",
            );
        }
    }
    assert!(!PROGRAM_IDENTITY_SOURCE.contains("labcolors.program-content-identity.v1"));
    for required in [
        "const DOMAIN_V2: &[u8] = b\"labcolors.program-content-identity.v2\\0\";",
        "pub(super) const PROGRAM_SCHEMA_V2: u8 = 2;",
    ] {
        assert!(
            PROGRAM_IDENTITY_SOURCE.contains(required),
            "the V2 content-address type must bind its exact domain and schema tag; missing `{required}`",
        );
    }

    let root = normalized_source_scope(
        PROGRAM_IDENTITY_SOURCE,
        "fn program_root_color()",
        "fn write_signal(",
    );
    assert!(
        !root.contains("MODELED_LCS_OCCURRENCE_V1"),
        "an encoded-only Program must not inherit the modeled-LCS release",
    );

    let signal = normalized_source_scope(
        PROGRAM_IDENTITY_SOURCE,
        "fn write_signal(",
        "fn source_color(",
    );
    assert!(
        !signal.contains("transform_release")
            && !signal.contains("IEC_SRGB8_TO_XYZ_D65_TRANSFORM_V1"),
        "encoded signal identity must not inherit an unused sRGB-to-XYZ transform",
    );

    let content = normalized_source_scope(
        CONSTRAINTS_SOURCE,
        "pub(crate) enum ProgramConstraintContentV1 {",
        "/// Внутрикрейтное описание generic test seam",
    );
    assert!(content.contains("ModeledLcs"));
    assert!(content.contains("release: ProgramLcsDependencyReleaseV1"));
    let dependency_release = normalized_source_scope(
        CONSTRAINTS_SOURCE,
        "pub(crate) struct ProgramLcsDependencyReleaseV1 {",
        "impl ProgramLcsDependencyReleaseV1",
    );
    assert!(dependency_release.contains("modeled_lcs_release: ModeledLcsOccurrenceReleaseId"));
    assert!(dependency_release.contains("transform_release: ColorimetricTransformReleaseId"));
    let constraint_identity = normalized_source_scope(
        PROGRAM_IDENTITY_SOURCE,
        "fn constraint_color(",
        "fn build_graph<",
    );
    assert!(constraint_identity.contains("release.modeled_lcs_release()"));
    assert!(constraint_identity.contains("release.transform_release()"));
}

#[test]
fn cold_program_normalization_reuses_owned_unordered_buffers() {
    let compiler = PROGRAM_SESSION_SOURCE
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for required in [
        "authored_targets: &mut [Target]",
        "authored_selection: Option<&mut DeclaredJointSelectionV1>",
        "let TargetDomainV1::Finite(candidates) = &mut target.domain",
        "authored_state .choices .sort_unstable_by_key",
        "authored: &mut [OutputBinding]",
    ] {
        assert!(
            compiler.contains(required),
            "cold Program compilation must normalize owned buffers in place; missing `{required}`",
        );
    }
    for forbidden in [
        "candidates.extend_from_slice(authored_candidates)",
        "choices.extend_from_slice(&authored_state.choices)",
        "authored.extend_from_slice(authored_outputs)",
    ] {
        assert!(
            !compiler.contains(forbidden),
            "cold Program compilation must not restore avoidable shadow copy `{forbidden}`",
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
        for forbidden in [
            "ProgramLcsPointTargetV1",
            "ProgramLcsPointAdapterV1",
            "ModeledLcsOccurrenceV1",
            "ColorSignal",
        ] {
            assert!(
                !contains_rust_identifier(source, forbidden),
                "{path} encoded evaluator must not acquire LCS capability `{forbidden}`",
            );
        }
    }
}

#[test]
fn staged_program_and_lcs_occurrence_modules_remain_private() {
    for required in [
        "pub(crate) mod lcs_occurrence;",
        "pub(crate) mod program;",
        "pub(crate) mod program_session;",
    ] {
        assert!(
            LIB_SOURCE.contains(required),
            "the pre-hard-cut boundary must remain crate-private; missing `{required}`",
        );
    }
    for forbidden in [
        "pub mod lcs_occurrence;",
        "pub mod program;",
        "pub mod program_session;",
    ] {
        assert!(
            !LIB_SOURCE.contains(forbidden),
            "lib.rs must not publish a staged private module `{forbidden}`",
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
