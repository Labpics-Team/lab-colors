use super::*;

fn scalar(value: f64) -> CertifiedScalarV1 {
    CertifiedScalarV1::exact(value).unwrap()
}

fn interval(lo: f64, hi: f64) -> CertifiedScalarV1 {
    CertifiedScalarV1 {
        lo: lift(lo).unwrap(),
        hi: lift(hi).unwrap(),
    }
}

fn rgb(value: [f64; 3]) -> CertifiedRgbV1 {
    CertifiedRgbV1::from_encoded(value).unwrap()
}

fn composite(source: [f64; 3], alpha: f64, backdrop: CertifiedRgbV1) -> CertifiedRgbV1 {
    rgb(source)
        .source_over(AdmittedOpacityV1::new(alpha).unwrap(), backdrop)
        .unwrap()
}

#[test]
fn exact_admission_preserves_dyadic_values_and_rejects_invalid_channels() {
    for value in [0.0, 1.0, f64::from_bits(1), f64::MAX, 255.0, 0.85, -0.1] {
        assert_eq!(scalar(value).nearest_even_cell().unwrap(), value.to_bits());
    }
    assert_eq!(scalar(-0.0).nearest_even_cell().unwrap(), 0.0_f64.to_bits());
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            CertifiedScalarV1::exact(value),
            Err(NumericUncertaintyV1::NonFiniteInput)
        ));
        assert!(matches!(
            CertifiedRgbV1::from_encoded([0.0, value, 0.0]),
            Err(NumericUncertaintyV1::NonFiniteInput)
        ));
    }
    for value in [-f64::from_bits(1), 256.0] {
        assert!(matches!(
            CertifiedRgbV1::from_encoded([0.0, value, 0.0]),
            Err(NumericUncertaintyV1::EncodedChannelOutsideDomain { channel: 1 })
        ));
    }
}

#[test]
fn numerical_failures_are_typed_and_inexact_enclosures_remain_usable() {
    assert!(matches!(
        scalar(1.0).div(interval(-1.0, 1.0)),
        Err(NumericUncertaintyV1::DivisionByZero)
    ));
    assert!(matches!(
        scalar(-1.0).root(3),
        Err(NumericUncertaintyV1::DomainViolation)
    ));
    assert!(matches!(
        scalar(1.0).root(2),
        Err(NumericUncertaintyV1::DomainViolation)
    ));
    assert!(matches!(
        scalar(0.0).ratio_loss(scalar(1.0)),
        Err(NumericUncertaintyV1::DomainViolation)
    ));
    assert!(matches!(
        scalar(-1.0).contrast(scalar(1.0)),
        Err(NumericUncertaintyV1::DomainViolation)
    ));
    assert!(matches!(
        checked(Quad::ZERO.div_r(Quad::ZERO, UP)),
        Err(NumericUncertaintyV1::InvalidOperation)
    ));
    assert!(matches!(
        checked(lift(1.0).unwrap().div_r(Quad::ZERO, UP)),
        Err(NumericUncertaintyV1::DivisionByZero)
    ));
    assert!(matches!(
        CertifiedScalarV1::point(Quad::largest()).mul(scalar(2.0)),
        Err(NumericUncertaintyV1::Overflow)
    ));
    assert!(matches!(
        CertifiedScalarV1::point(Quad::SMALLEST).div(scalar(2.0)),
        Err(NumericUncertaintyV1::Underflow)
    ));
    assert!(matches!(
        scalar(f64::MAX)
            .mul(scalar(2.0))
            .unwrap()
            .nearest_even_cell(),
        Err(NumericUncertaintyV1::Overflow)
    ));
    assert!(matches!(
        scalar(f64::from_bits(1))
            .div(scalar(2.0))
            .unwrap()
            .nearest_even_cell(),
        Err(NumericUncertaintyV1::Underflow)
    ));
    let third = scalar(1.0).div(scalar(3.0)).unwrap();
    assert!(third.lo < third.hi);
    assert_eq!(
        third.nearest_even_cell().unwrap(),
        (1.0_f64 / 3.0).to_bits()
    );
}

#[test]
fn signs_and_shared_square_dependence_preserve_bounds() {
    let multiplied = interval(2.0, 3.0).mul(scalar(-2.0)).unwrap();
    assert_eq!(
        (multiplied.lo, multiplied.hi),
        (lift(-6.0).unwrap(), lift(-4.0).unwrap())
    );
    let divided = interval(-6.0, 9.0).div(interval(-3.0, -2.0)).unwrap();
    assert_eq!(
        (divided.lo, divided.hi),
        (lift(-4.5).unwrap(), lift(3.0).unwrap())
    );
    let squared = interval(-2.0, 3.0).pow(2).unwrap();
    assert_eq!((squared.lo, squared.hi), (Quad::ZERO, lift(9.0).unwrap()));
    let loss = interval(0.5, 2.0).ratio_loss(scalar(1.0)).unwrap();
    assert_eq!((loss.lo, loss.hi), (lift(1.0).unwrap(), lift(2.0).unwrap()));
}

#[test]
fn roots_enclose_exact_rational_examples_and_small_rgb_domain() {
    for (input, degree, expected) in [(8.0, 3, 2.0), (243.0, 5, 3.0), (1.0 / 32.0, 5, 0.5)] {
        let value = scalar(input).root(degree).unwrap();
        let exact = lift(expected).unwrap();
        assert!(value.lo <= exact && value.hi >= exact);
        assert_eq!(value.nearest_even_cell().unwrap(), expected.to_bits());
    }
    // The seed's binary64 conversion is optional: a legitimate tiny RGB must
    // not fail merely because its positive LMS value is below binary64 range.
    let lightness = rgb([f64::from_bits(1); 3]).oklab().unwrap()[0];
    assert!(lightness.lo > Quad::ZERO);
    assert!(lightness.nearest_even_cell().is_ok());
}

#[test]
fn whole_cell_containment_respects_even_midpoints_without_epsilon() {
    assert!(scalar(1.0).proves_equal(scalar(1.0)));
    assert!(!interval(1.0, 2.0).proves_equal(interval(1.0, 2.0)));
    for offset in [0_u64, 1] {
        let lower = scalar(f64::from_bits(1.0_f64.to_bits() + offset));
        let upper = scalar(f64::from_bits(1.0_f64.to_bits() + offset + 1));
        let midpoint = lower.add(upper).unwrap().mul(scalar(0.5)).unwrap();
        let even = 1.0_f64.to_bits() + if offset == 0 { 0 } else { 2 };
        assert_eq!(midpoint.nearest_even_cell().unwrap(), even);
        let crossing = CertifiedScalarV1 {
            lo: checked(midpoint.lo.next_down()).unwrap(),
            hi: checked(midpoint.hi.next_up()).unwrap(),
        };
        assert_eq!(
            crossing.nearest_even_cell(),
            Err(NumericUncertaintyV1::UnresolvedRoundingCell)
        );
        assert_eq!(
            crossing.evidence(),
            Err(NumericUncertaintyV1::UnresolvedRoundingCell)
        );
    }
}

#[test]
fn gamma_branch_crossing_encloses_the_literal_coefficient_discontinuity() {
    let split = scalar(SRGB_DECODE_SPLIT).mul(scalar(255.0)).unwrap();
    let range = CertifiedScalarV1 {
        lo: checked(split.lo.next_down()).unwrap(),
        hi: checked(split.hi.next_up()).unwrap(),
    };
    let decoded = decode(range).unwrap();
    assert!(
        decoded
            .upper_difference(CertifiedScalarV1::point(decoded.lo))
            .unwrap()
            > 2.0e-9
    );
    assert_eq!(
        decoded.nearest_even_cell(),
        Err(NumericUncertaintyV1::UnresolvedRoundingCell)
    );
}

#[test]
fn nested_composition_retains_exact_value_hidden_by_shared_binary64_cell() {
    let backdrop = composite([244.0; 3], 0.85, rgb([255.0; 3]));
    let nested = composite([1.0; 3], 0.5, backdrop).0[0];
    let host = scalar(123.325);
    assert_eq!(
        nested.nearest_even_cell().unwrap(),
        host.nearest_even_cell().unwrap()
    );
    assert!(nested.strictly_below(host));
    let evidence = nested.evidence().unwrap();
    assert_eq!(
        evidence.lower_bits,
        0x4005_ed4c_cccc_cccc_ccf0_0000_0000_0000
    );
    assert_eq!(evidence.lower_bits, evidence.upper_bits);
    assert_eq!(
        nested.upper_difference(host).unwrap(),
        -2.7200464103316335e-15
    );
}

#[test]
fn admitted_alpha_keeps_deep_composition_inside_the_convex_physical_domain() {
    for initial in [[0.0; 3], [255.0; 3], [0.0, 255.0, 0.0]] {
        let mut composed = rgb(initial);
        for _ in 0..64 {
            composed = composite(initial, 0.7, composed);
        }
        for (value, expected) in composed.0.into_iter().zip(initial) {
            assert_eq!(value.lo, lift(expected).unwrap());
            assert_eq!(value.hi, lift(expected).unwrap());
        }
        let transparent = composite([100.0; 3], 0.0, composed);
        let opaque = composite([100.0; 3], 1.0, composed);
        for channel in 0..3 {
            assert_eq!(transparent.0[channel].lo, composed.0[channel].lo);
            assert_eq!(opaque.0[channel].hi, lift(100.0).unwrap());
        }
    }
}

#[test]
fn descending_profile_encloses_order_statistics_when_intervals_cross() {
    let mut values = [interval(0.0, 10.0), interval(7.0, 8.0), interval(3.0, 12.0)];
    let mut scratch = [scalar(0.0); 3];
    sort_descending_profile(&mut values, &mut scratch).unwrap();
    for (value, (lo, hi)) in values
        .into_iter()
        .zip([(7.0, 12.0), (3.0, 10.0), (0.0, 8.0)])
    {
        assert_eq!((value.lo, value.hi), (lift(lo).unwrap(), lift(hi).unwrap()));
    }
    let before = values.map(|value| (value.lo.to_bits(), value.hi.to_bits()));
    assert_eq!(
        sort_descending_profile(&mut values, &mut scratch[..2]),
        Err(NumericUncertaintyV1::InsufficientScratchSpace)
    );
    assert_eq!(
        values.map(|value| (value.lo.to_bits(), value.hi.to_bits())),
        before
    );
}

const SOURCE: [f64; 3] = [49.0, 85.0, 190.0];
const ANCHOR: [f64; 3] = [80.0, 89.0, 109.0];

#[test]
fn luminance_and_signed_oklab_enclose_independent_rational_oracle() {
    // Derived independently with exact Fraction arithmetic and integer nth
    // roots at 240 fractional bits, then rounded outward to binary128. These
    // are ideal-model bounds, not outputs copied from this implementation.
    let fixtures = [
        (
            SOURCE,
            [
                [
                    0x3ffb_bd23_ce57_fb92_15a9_fe17_7340_65dc,
                    0x3ffb_bd23_ce57_fb92_15a9_fe17_7340_65dd,
                ],
                [
                    0x3ffd_f1b8_d408_f896_3ffd_c4ea_0eba_4270,
                    0x3ffd_f1b8_d408_f896_3ffd_c4ea_0eba_4271,
                ],
                [
                    0xbff8_8e34_82c9_2398_eed3_fef4_6cbc_ceca,
                    0xbff8_8e34_82c9_2398_eed3_fef4_6cbc_cec9,
                ],
                [
                    0xbffc_5be5_e4bb_1271_fc1a_9728_c2de_53b1,
                    0xbffc_5be5_e4bb_1271_fc1a_9728_c2de_53b0,
                ],
            ],
        ),
        (
            ANCHOR,
            [
                [
                    0x3ffb_97bb_0040_cfff_8bad_71b1_fd0c_8d4c,
                    0x3ffb_97bb_0040_cfff_8bad_71b1_fd0c_8d4d,
                ],
                [
                    0x3ffd_db08_1325_54ec_60a9_ada8_13b1_9929,
                    0x3ffd_db08_1325_54ec_60a9_ada8_13b1_992a,
                ],
                [
                    0xbff6_3a89_f56c_54f9_c3fb_9784_6ae2_23e9,
                    0xbff6_3a89_f56c_54f9_c3fb_9784_6ae2_23e8,
                ],
                [
                    0xbffa_1a3b_6a85_abea_d8e1_2ab2_cd2b_1a56,
                    0xbffa_1a3b_6a85_abea_d8e1_2ab2_cd2b_1a55,
                ],
            ],
        ),
        (
            [45.55648583981751, 80.83558199614441, 185.48517133147033],
            [
                [
                    0x3ffb_97bb_0040_cfff_8b7f_2b2b_2c0f_6fa0,
                    0x3ffb_97bb_0040_cfff_8b7f_2b2b_2c0f_6fa1,
                ],
                [
                    0x3ffd_e3e9_de7c_456c_e932_f44f_14a6_1db1,
                    0x3ffd_e3e9_de7c_456c_e932_f44f_14a6_1db2,
                ],
                [
                    0xbff8_8e34_7e1b_17ba_fe5f_6428_94ed_c0fa,
                    0xbff8_8e34_7e1b_17ba_fe5f_6428_94ed_c0f9,
                ],
                [
                    0xbffc_5be5_e293_c4d1_a68f_e1b8_7e78_61f2,
                    0xbffc_5be5_e293_c4d1_a68f_e1b8_7e78_61f1,
                ],
            ],
        ),
    ];
    for (color, expected) in fixtures {
        let [l, a, b] = rgb(color).oklab().unwrap();
        let actual = [rgb(color).relative_luminance().unwrap(), l, a, b];
        for (actual, [lo, hi]) in actual.into_iter().zip(expected) {
            assert!(actual.lo <= Quad::from_bits(lo));
            assert!(actual.hi >= Quad::from_bits(hi));
        }
    }
}

/// The expected cells and drift bounds were independently checked with exact
/// Fraction arithmetic and integer nth-root enclosures at 240 fractional bits.
#[test]
fn implicit_composition_accepts_corrected_cells_but_rejects_old_f64_residual() {
    let materials = [[234.0, 221.0, 234.0], [211.0, 233.0, 221.0]];
    let opacity = [0.7, 0.65];
    let roots = [[217.0, 229.0, 250.0], [248.0, 243.0, 236.0]];
    let backgrounds = std::array::from_fn::<_, 4, _>(|i| {
        composite(materials[i % 2], opacity[i % 2], rgb(roots[i / 2]))
    });
    let reference = std::array::from_fn::<_, 4, _>(|i| {
        rgb(SOURCE)
            .contrast(composite(materials[i % 2], opacity[i % 2], rgb([255.0; 3])))
            .unwrap()
    });
    let measured = |color| backgrounds.map(|bg| rgb(color).contrast(bg).unwrap());
    let primary = |values: [CertifiedScalarV1; 4]| {
        values
            .into_iter()
            .zip(reference)
            .map(|(actual, target)| actual.ratio_loss(target).unwrap())
            .fold(scalar(1.0), |worst, loss| worst.max(loss).unwrap())
    };
    let anchor_m = measured(ANCHOR);
    let anchor_primary = primary(anchor_m);
    let corrected = [45.55648583981751, 80.83558199614441, 185.48517133147033];
    let corrected_m = measured(corrected);
    let expected = [
        0x4015_a048_0742_c6ab,
        0x4016_0c37_81aa_d26e,
        0x4016_96fa_1d26_1ee1,
        0x4017_2b87_bfb1_8961,
    ];
    for i in 0..4 {
        assert_eq!(anchor_m[i].nearest_even_cell().unwrap(), expected[i]);
        assert_eq!(corrected_m[i].nearest_even_cell().unwrap(), expected[i]);
    }
    let corrected_primary = primary(corrected_m);
    assert_eq!(
        anchor_primary.nearest_even_cell().unwrap(),
        0x3ff0_73a9_2a88_2f3d
    );
    assert_eq!(
        corrected_primary.nearest_even_cell().unwrap(),
        anchor_primary.nearest_even_cell().unwrap()
    );
    let source_lab = rgb(SOURCE).oklab().unwrap();
    let anchor_distance =
        source_distance_squared(source_lab, rgb(ANCHOR).oklab().unwrap()).unwrap();
    let corrected_distance =
        source_distance_squared(source_lab, rgb(corrected).oklab().unwrap()).unwrap();
    assert!(corrected_distance.strictly_below(anchor_distance));
    assert_eq!(
        f64::from_bits(anchor_distance.nearest_even_cell().unwrap()),
        0.01892467164678065
    );
    assert_eq!(
        f64::from_bits(corrected_distance.nearest_even_cell().unwrap()),
        0.00018183980706483807
    );
    // This gain in source distance accompanies a small true primary loss.
    assert!(anchor_primary.strictly_below(corrected_primary));
    assert!(corrected_primary.upper_difference(anchor_primary).unwrap() <= 1.0780454039519933e-18);
    for rejected in [
        [45.55648583981752, 80.83558199614438, 185.4851713314703],
        SOURCE,
    ] {
        let actual = measured(rejected);
        assert!(
            actual
                .into_iter()
                .zip(anchor_m)
                .any(|(a, b)| a.nearest_even_cell().unwrap() != b.nearest_even_cell().unwrap())
        );
        assert_ne!(
            primary(actual).nearest_even_cell().unwrap(),
            anchor_primary.nearest_even_cell().unwrap()
        );
    }
}

#[test]
fn explicit_forward_lightness_and_primary_cells_preserve_source_improvement() {
    let source_lab = rgb(SOURCE).oklab().unwrap();
    let anchor_lab = rgb(ANCHOR).oklab().unwrap();
    let corrected_lab = rgb([43.36346865614138, 78.15569710108728, 182.58830650509347])
        .oklab()
        .unwrap();
    let offset = scalar(RELATIVE_LUMINANCE_RATIO_OFFSET);
    let target = scalar(0.6).pow(3).unwrap().add(offset).unwrap();
    let primary = |lightness: CertifiedScalarV1| {
        lightness
            .pow(3)
            .unwrap()
            .add(offset)
            .unwrap()
            .ratio_loss(target)
            .unwrap()
    };
    let anchor_primary = primary(anchor_lab[0]);
    let corrected_primary = primary(corrected_lab[0]);
    assert_eq!(
        anchor_lab[0].nearest_even_cell().unwrap(),
        0x3fdd_b081_3255_4ec6
    );
    assert_eq!(
        corrected_lab[0].nearest_even_cell().unwrap(),
        anchor_lab[0].nearest_even_cell().unwrap()
    );
    assert_eq!(
        anchor_primary.nearest_even_cell().unwrap(),
        0x3ffc_67be_48bc_b9aa
    );
    assert_eq!(
        corrected_primary.nearest_even_cell().unwrap(),
        anchor_primary.nearest_even_cell().unwrap()
    );
    let distance = source_distance_squared(source_lab, corrected_lab).unwrap();
    assert!(distance.strictly_below(source_distance_squared(source_lab, anchor_lab).unwrap()));
    assert_eq!(
        f64::from_bits(distance.nearest_even_cell().unwrap()),
        0.0004910051490285367
    );
    assert!(anchor_primary.strictly_below(corrected_primary));
    assert!(corrected_primary.upper_difference(anchor_primary).unwrap() <= 6.5839731038796344e-18);
    assert_ne!(
        source_lab[0].nearest_even_cell().unwrap(),
        anchor_lab[0].nearest_even_cell().unwrap()
    );
    assert_ne!(
        primary(source_lab[0]).nearest_even_cell().unwrap(),
        anchor_primary.nearest_even_cell().unwrap()
    );
}
