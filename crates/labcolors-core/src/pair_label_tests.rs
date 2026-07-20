//! P1 acceptance tests: frozen Pair authoring lowers one-way into the common
//! point graph. These tests intentionally do not reproduce the deleted
//! `PairSide`/Oklab/H-K heuristic or a second compositing oracle.

use crate::Srgb8;
use crate::composition::AdmittedOpacityV1;
use crate::joint::JointConstraintDecisionV1;
use crate::ladder::{LadderPosition, LadderTint};
use crate::pair::{PairLabelRequirementV1, PairLoweringErrorV1, lower_fill, verify_label};
use crate::semantic::{NamedRoleTable, Resolved, RoleChroma, RoleSpec, resolve_named_set};
use crate::solve::{BgInput, Floor, SolveFailure};
use crate::spaces::vc::ViewingConditions;
use crate::wcag22::Wcag22CriterionV1;

fn admitted(value: f64) -> AdmittedOpacityV1 {
    AdmittedOpacityV1::new(value).expect("test opacity is admitted")
}

fn tint(bytes: [u8; 3]) -> LadderTint {
    let encoded = bytes.map(|byte| f64::from(byte) / 255.0);
    LadderTint::new([encoded; 4]).expect("exact byte tint is valid")
}

fn bytes(hex: &str) -> Srgb8 {
    Srgb8::new(crate::srgb8::hex_bytes(hex).expect("resolver emits #RRGGBB"))
}

#[test]
fn pair_fill_is_one_real_source_over_occurrence() {
    crate::composition::reset_source_over_evaluation_count();
    let fill = lower_fill(
        Srgb8::new([255, 0, 0]),
        admitted(0.5),
        Srgb8::new([0, 0, 255]),
    );

    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    assert_eq!(fill.paint().source(), Srgb8::new([255, 0, 0]));
    assert_eq!(fill.paint().opacity(), admitted(0.5));
    assert_eq!(fill.occurrence().certificate().backdrop_rgb(), [0, 0, 255]);
    assert_eq!(fill.visible(), Srgb8::new([128, 0, 128]));
}

#[test]
fn pair_label_fresh_evidence_binds_upper_backdrop_to_emitted_fill() {
    crate::composition::reset_source_over_evaluation_count();
    let verified = verify_label(
        Srgb8::new([255, 0, 0]),
        admitted(0.5),
        Srgb8::new([255, 255, 255]),
        Srgb8::new([0, 0, 255]),
        PairLabelRequirementV1::Exact(Srgb8::new([255, 255, 255])),
    )
    .expect("opaque label exactly satisfies identity requirement");

    // Full report executes lower+upper once; mandatory fresh recheck repeats both.
    assert_eq!(crate::composition::source_over_evaluation_count(), 4);
    assert_eq!(verified.fill_occurrence().visible(), [128, 0, 128]);
    assert_eq!(
        verified.label_occurrence().certificate().backdrop_rgb(),
        verified.fill_occurrence().visible()
    );
    assert_eq!(verified.label_occurrence().visible(), [255, 255, 255]);
    assert_eq!(verified.evidence().fresh_executions().len(), 1);
    assert_eq!(verified.evidence().fresh_cells().len(), 1);
}

#[test]
fn wcag_is_checked_against_fill_surface_not_page_background() {
    // White label would be 21:1 against the black page, but only ~3.95:1
    // against the emitted 50%-white fill. AaText must therefore fail.
    let error = verify_label(
        Srgb8::new([255, 255, 255]),
        admitted(0.5),
        Srgb8::new([255, 255, 255]),
        Srgb8::new([0, 0, 0]),
        PairLabelRequirementV1::Wcag22(Wcag22CriterionV1::Sc143TextDefault),
    )
    .expect_err("actual fill surface does not satisfy 4.5:1");

    let PairLoweringErrorV1::Infeasible(report) = error else {
        panic!("valid negative decision must be Infeasible, not a protocol error");
    };
    assert_eq!(report.executions()[0].lower_visible(), Srgb8::new([128; 3]));
    assert_eq!(report.cells().len(), 1);
    assert!(matches!(
        report.cells()[0].decision(),
        JointConstraintDecisionV1::Wcag22Violation(_)
    ));
    assert_eq!(
        report.cells()[0].decision().target(),
        Srgb8::new([128; 3]),
        "WCAG evidence background is the emitted fill"
    );
}

#[test]
fn exact_mismatch_is_infeasible_not_fault() {
    let error = verify_label(
        Srgb8::new([0, 0, 0]),
        AdmittedOpacityV1::OPAQUE,
        Srgb8::new([1, 2, 3]),
        Srgb8::new([255, 255, 255]),
        PairLabelRequirementV1::Exact(Srgb8::new([9, 9, 9])),
    )
    .expect_err("identity mismatch is a lawful hard violation");
    assert!(matches!(error, PairLoweringErrorV1::Infeasible(_)));
}

#[test]
fn named_pair_frontend_uses_the_same_fill_occurrence_end_to_end() {
    let vc = ViewingConditions::srgb();
    let source = Srgb8::new([96; 3]);
    let source_tint = tint(source.bytes());
    let (alpha_light, alpha_dark) = LadderPosition::FillPrimary.alpha_pair();
    let table = NamedRoleTable::new(
        vec![
            ("pair-fill".into(), RoleSpec::PairFill { tint: source_tint }),
            (
                "pair-label".into(),
                RoleSpec::PairLabel {
                    tint: source_tint,
                    fraction: 0.9,
                    floor: Floor::AaText,
                    surface_alpha_light: alpha_light,
                    surface_alpha_dark: alpha_dark,
                },
            ),
        ],
        vec![],
        RoleChroma::Neutral,
    )
    .expect("canonical Pair frontend compiles");
    let page = BgInput::solid("#FFFFFF").unwrap();
    let set = resolve_named_set(&page, &table, &vc).expect("canonical Pair resolves");

    let fill = set
        .iter()
        .find(|(name, _)| name == "pair-fill")
        .and_then(|(_, resolved)| resolved.translucent())
        .expect("PairFill emits a translucent Paint");
    let label = set
        .iter()
        .find(|(name, _)| name == "pair-label")
        .and_then(|(_, resolved)| resolved.solved())
        .expect("PairLabel emits a verified Color");

    let opacity = admitted(LadderPosition::FillPrimary.alpha_for_vc(&vc));
    let lowered = lower_fill(source, opacity, Srgb8::new([255; 3]));
    assert_eq!(fill.tint_hex(), source.to_hex());
    assert_eq!(fill.alpha().to_bits(), opacity.value().to_bits());
    assert_eq!(fill.composite_hex(), lowered.visible().to_hex());
    assert!(label.wcag_ratio() >= 4.5);

    let verified = verify_label(
        source,
        opacity,
        bytes(label.hex()),
        Srgb8::new([255; 3]),
        PairLabelRequirementV1::Wcag22(Wcag22CriterionV1::Sc143TextDefault),
    )
    .expect("semantic proposal passes the same joint contract");
    assert_eq!(verified.fill_occurrence(), lowered.occurrence());
    assert_eq!(verified.label_paint().source(), bytes(label.hex()));
}

#[test]
fn pair_label_transport_alpha_cannot_fork_physical_semantics() {
    let source = tint([96; 3]);
    let (light, dark) = LadderPosition::FillPrimary.alpha_pair();
    let error = NamedRoleTable::new(
        vec![(
            "pair-label".into(),
            RoleSpec::PairLabel {
                tint: source,
                fraction: 0.9,
                floor: Floor::AaText,
                surface_alpha_light: f64::from_bits(light.to_bits() + 1),
                surface_alpha_dark: dark,
            },
        )],
        vec![],
        RoleChroma::Neutral,
    )
    .expect_err("noncanonical Pair surface alpha must fail at compilation");
    assert!(matches!(
        error,
        SolveFailure::InvalidInput(message)
            if message.contains("code-owned by FillPrimary")
    ));
}

#[test]
fn pair_frontend_has_only_color_or_translucent_terminal_shapes() {
    let source = tint([96; 3]);
    let (light, dark) = LadderPosition::FillPrimary.alpha_pair();
    let table = NamedRoleTable::new(
        vec![
            ("fill".into(), RoleSpec::PairFill { tint: source }),
            (
                "label".into(),
                RoleSpec::PairLabel {
                    tint: source,
                    fraction: 0.9,
                    floor: Floor::AaText,
                    surface_alpha_light: light,
                    surface_alpha_dark: dark,
                },
            ),
        ],
        vec![],
        RoleChroma::Neutral,
    )
    .unwrap();
    let results = resolve_named_set(
        &BgInput::solid("#FFFFFF").unwrap(),
        &table,
        &ViewingConditions::srgb(),
    )
    .unwrap();
    assert!(matches!(results[0].1, Resolved::Translucent(_)));
    assert!(matches!(results[1].1, Resolved::Color { .. }));
}
