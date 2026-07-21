//! P1 Pair frontend: emitted fill Surface, joint hard selection and anti-vacuum.

use crate::Srgb8;
use crate::composition::AdmittedOpacityV1;
use crate::config::fixture::labui_reference;
use crate::config::{LadderSource, RoleRecipe};
use crate::joint::CandidateOrdinalV1;
use crate::pair::{
    PairLabelCandidateV1, PairLabelRequirementV1, lower_fill, select_label_candidates,
};
use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec, resolve_named_set};
use crate::solve::{BgInput, Floor, SolveFailure};
use crate::spaces::vc::ViewingConditions;
use crate::wcag22::Wcag22CriterionV1;

fn enc(hex: &str) -> [f64; 3] {
    crate::spaces::srgb::srgb_encoded_from_hex(hex).unwrap()
}

#[test]
fn fill_is_one_exact_opaque_occurrence_without_pair_heuristic() {
    let source = Srgb8::new([0x00, 0x7A, 0xFF]);
    let backdrop = Srgb8::new([0xFF; 3]);
    let fill = lower_fill(source, AdmittedOpacityV1::OPAQUE, backdrop);

    assert_eq!(fill.paint().source(), source);
    assert_eq!(fill.paint().opacity(), AdmittedOpacityV1::OPAQUE);
    assert_eq!(fill.visible(), source);
    assert_eq!(
        fill.occurrence().certificate().backdrop_rgb(),
        backdrop.bytes()
    );
}

#[test]
fn wcag_joint_selection_rejects_preferred_tuple_and_selects_legal_tuple() {
    let dark_fill = Srgb8::new([0x10; 3]);
    let preferred_dark = Srgb8::new([0x20; 3]);
    let legal_light = Srgb8::new([0xFF; 3]);
    let verified = select_label_candidates(
        dark_fill,
        AdmittedOpacityV1::OPAQUE,
        vec![
            PairLabelCandidateV1::new(CandidateOrdinalV1::new(1), preferred_dark),
            PairLabelCandidateV1::new(CandidateOrdinalV1::new(2), legal_light),
        ],
        vec![CandidateOrdinalV1::new(1), CandidateOrdinalV1::new(2)],
        Srgb8::new([0xEE; 3]),
        PairLabelRequirementV1::Wcag22(Wcag22CriterionV1::Sc143TextDefault),
    )
    .unwrap();

    assert_eq!(verified.ordinal(), CandidateOrdinalV1::new(2));
    assert_eq!(verified.fill_occurrence().visible(), dark_fill.bytes());
    assert_eq!(verified.label_occurrence().visible(), legal_light.bytes());
    assert_eq!(
        verified.label_occurrence().certificate().backdrop_rgb(),
        dark_fill.bytes()
    );
}

#[test]
fn pair_fill_and_pair_label_share_the_same_emitted_surface() {
    let mut config = labui_reference();
    config.roles.push((
        "badge-label-brand".into(),
        RoleRecipe::PairLabel {
            source: LadderSource::Brand,
            fraction: 0.461,
            floor: Floor::AaUi,
        },
    ));
    let table = config.compile_named_role_table().unwrap();

    for (background, vc) in [
        ("#FFFFFF", ViewingConditions::srgb()),
        ("#101012", ViewingConditions::dim_surround()),
    ] {
        let set = resolve_named_set(&BgInput::solid(background).unwrap(), &table, &vc).unwrap();
        let fill = set
            .iter()
            .find(|(name, _)| name == "badge-fill-brand")
            .unwrap()
            .1
            .translucent()
            .unwrap();
        let label = set
            .iter()
            .find(|(name, _)| name == "badge-label-brand")
            .unwrap()
            .1
            .solved()
            .unwrap();

        assert_eq!(fill.alpha().to_bits(), 1.0_f64.to_bits());
        assert_eq!(fill.tint_hex(), fill.composite_hex());
        let ratio = crate::wcag::contrast_ratio(enc(label.hex()), enc(fill.composite_hex()));
        assert!(ratio >= 3.0, "{background}: got {ratio}");
    }
}

#[test]
fn pair_result_is_independent_of_client_role_names() {
    let blue = [0.0, 0.47843137254901963, 1.0];
    let tint = crate::ladder::LadderTint::new([blue; 4]).unwrap();
    let entries = |fill: &str, label: &str| {
        vec![
            (fill.into(), RoleSpec::PairFill { tint }),
            (
                label.into(),
                RoleSpec::PairLabel {
                    tint,
                    fraction: 0.461,
                    floor: Floor::AaUi,
                    surface_alpha_light: 1.0,
                    surface_alpha_dark: 1.0,
                },
            ),
        ]
    };
    let left = NamedRoleTable::new(entries("x", "y"), Vec::new(), RoleChroma::Neutral).unwrap();
    let right = NamedRoleTable::new(
        entries("danger-primary", "surface-hover"),
        Vec::new(),
        RoleChroma::Neutral,
    )
    .unwrap();
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let a = resolve_named_set(&bg, &left, &ViewingConditions::srgb()).unwrap();
    let b = resolve_named_set(&bg, &right, &ViewingConditions::srgb()).unwrap();

    assert_eq!(a[0].1, b[0].1);
    assert_eq!(a[1].1, b[1].1);
}

#[test]
fn nonopaque_pairlabel_transport_is_rejected_before_execution() {
    let tint = crate::ladder::LadderTint::new([[0.0, 0.0, 0.0]; 4]).unwrap();
    let error = NamedRoleTable::new(
        vec![(
            "label".into(),
            RoleSpec::PairLabel {
                tint,
                fraction: 0.5,
                floor: Floor::AaUi,
                surface_alpha_light: 0.122,
                surface_alpha_dark: 0.122,
            },
        )],
        Vec::new(),
        RoleChroma::Neutral,
    )
    .unwrap_err();

    assert!(matches!(error, SolveFailure::InvalidInput(message) if message.contains("opaque")));
}

#[test]
fn floor_none_has_no_tautological_exact_constraint() {
    let verified = select_label_candidates(
        Srgb8::new([0x80; 3]),
        AdmittedOpacityV1::OPAQUE,
        vec![PairLabelCandidateV1::new(
            CandidateOrdinalV1::new(1),
            Srgb8::new([0x81; 3]),
        )],
        vec![CandidateOrdinalV1::new(1)],
        Srgb8::new([0x00; 3]),
        PairLabelRequirementV1::None,
    )
    .unwrap();

    assert_eq!(verified.ordinal(), CandidateOrdinalV1::new(1));
}
