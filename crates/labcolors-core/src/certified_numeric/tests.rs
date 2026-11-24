use super::*;

fn scalar(value: f64) -> CertifiedScalarV1 {
    CertifiedScalarV1::exact(value).expect("finite fixture scalar")
}

fn interval(lo: f64, hi: f64) -> CertifiedScalarV1 {
    CertifiedScalarV1 {
        lo: lift(lo).expect("finite fixture lower bound"),
        hi: lift(hi).expect("finite fixture upper bound"),
    }
}

fn rgb(value: [f64; 3]) -> CertifiedRgbV1 {
    CertifiedRgbV1::from_encoded(value).expect("fixture channels in the encoded sRGB domain")
}

fn composite(source: [f64; 3], alpha: f64, backdrop: CertifiedRgbV1) -> CertifiedRgbV1 {
    rgb(source)
        .source_over(
            AdmittedOpacityV1::new(alpha).expect("fixture material opacity in [0, 1]"),
            backdrop,
        )
        .expect("valid fixture source-over composition")
}

#[test]
fn exact_admission_preserves_dyadic_values_and_rejects_invalid_channels()
-> Result<(), NumericUncertaintyV1> {
    for value in [0.0, 1.0, f64::from_bits(1), f64::MAX, 255.0, 0.85, -0.1] {
        assert_eq!(scalar(value).nearest_even_cell()?, value.to_bits());
    }
    assert_eq!(scalar(-0.0).nearest_even_cell()?, 0.0_f64.to_bits());
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
    Ok(())
}

#[test]
fn encoded_equality_requires_exact_values_even_with_one_rounding_cell()
-> Result<(), NumericUncertaintyV1> {
    let anchor = rgb([128.0; 3]);
    let material = composite(
        [128.0; 3],
        f64::from_bits(1.0_f64.to_bits() - 1),
        rgb([127.0; 3]),
    );
    assert_eq!(material.0[0].nearest_even_cell()?, 128.0_f64.to_bits());
    assert!(!material.proves_equal(anchor));
    let region = CertifiedRgbV1::from_encoded_bounds([127.0; 3], [128.0; 3])?;
    assert!(!region.proves_equal(region));
    assert!(anchor.proves_equal(rgb([128.0; 3])));
    Ok(())
}

#[test]
fn numerical_failures_are_typed_and_inexact_enclosures_remain_usable()
-> Result<(), NumericUncertaintyV1> {
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
        checked(lift(1.0)?.div_r(Quad::ZERO, UP)),
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
        scalar(f64::MAX).mul(scalar(2.0))?.nearest_even_cell(),
        Err(NumericUncertaintyV1::Overflow)
    ));
    assert!(matches!(
        scalar(f64::from_bits(1))
            .div(scalar(2.0))?
            .nearest_even_cell(),
        Err(NumericUncertaintyV1::Underflow)
    ));
    let third = scalar(1.0).div(scalar(3.0))?;
    assert!(third.lo < third.hi);
    assert_eq!(third.nearest_even_cell()?, (1.0_f64 / 3.0).to_bits());
    Ok(())
}

#[test]
fn signs_and_shared_square_dependence_preserve_bounds() -> Result<(), NumericUncertaintyV1> {
    let multiplied = interval(2.0, 3.0).mul(scalar(-2.0))?;
    assert_eq!((multiplied.lo, multiplied.hi), (lift(-6.0)?, lift(-4.0)?));
    let divided = interval(-6.0, 9.0).div(interval(-3.0, -2.0))?;
    assert_eq!((divided.lo, divided.hi), (lift(-4.5)?, lift(3.0)?));
    let squared = interval(-2.0, 3.0).pow(2)?;
    assert_eq!((squared.lo, squared.hi), (Quad::ZERO, lift(9.0)?));
    let loss = interval(0.5, 2.0).ratio_loss(scalar(1.0))?;
    assert_eq!((loss.lo, loss.hi), (lift(1.0)?, lift(2.0)?));
    Ok(())
}

#[test]
fn roots_enclose_exact_rational_examples_and_small_rgb_domain() -> Result<(), NumericUncertaintyV1>
{
    for (input, degree, expected) in [(8.0, 3, 2.0), (243.0, 5, 3.0), (1.0 / 32.0, 5, 0.5)] {
        let value = scalar(input).root(degree)?;
        let exact = lift(expected)?;
        assert!(value.lo <= exact && value.hi >= exact);
        assert_eq!(value.nearest_even_cell()?, expected.to_bits());
    }
    // The seed's binary64 conversion is optional: a legitimate tiny RGB must
    // not fail merely because its positive LMS value is below binary64 range.
    let lightness = rgb([f64::from_bits(1); 3]).oklab()?[0];
    assert!(lightness.lo > Quad::ZERO);
    assert!(lightness.nearest_even_cell().is_ok());
    Ok(())
}

/// Independent integer arithmetic: for target = p/q, e = floor(log2(target)/d),
/// find k with k^d <= (p/q)*2^(d*(112-e)) < (k+1)^d. Those two adjacent
/// binary128 values must be enclosed. This uses no floating-point Newton or
/// root function. The wide exponents also bypass the optional binary64 seed.
#[test]
fn roots_enclose_integer_oracle_at_exact_power_neighbours_and_extreme_exponents()
-> Result<(), NumericUncertaintyV1> {
    let fixtures = [
        (
            0x3ffe_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
            3,
            [
                0x3ffe_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
                0x3fff_0000_0000_0000_0000_0000_0000_0000,
            ],
        ),
        (
            0x3fff_0000_0000_0000_0000_0000_0000_0001,
            3,
            [
                0x3fff_0000_0000_0000_0000_0000_0000_0000,
                0x3fff_0000_0000_0000_0000_0000_0000_0001,
            ],
        ),
        (
            0x4001_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
            3,
            [
                0x3fff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
                0x4000_0000_0000_0000_0000_0000_0000_0000,
            ],
        ),
        (
            0x4002_0000_0000_0000_0000_0000_0000_0001,
            3,
            [
                0x4000_0000_0000_0000_0000_0000_0000_0000,
                0x4000_0000_0000_0000_0000_0000_0000_0001,
            ],
        ),
        (
            0x4006_e5ff_ffff_ffff_ffff_ffff_ffff_ffff,
            5,
            [
                0x4000_7fff_ffff_ffff_ffff_ffff_ffff_ffff,
                0x4000_8000_0000_0000_0000_0000_0000_0000,
            ],
        ),
        (
            0x4006_e600_0000_0000_0000_0000_0000_0001,
            5,
            [
                0x4000_8000_0000_0000_0000_0000_0000_0000,
                0x4000_8000_0000_0000_0000_0000_0000_0001,
            ],
        ),
        (
            0x4000_0000_0000_0000_0000_0000_0000_0000,
            3,
            [
                0x3fff_428a_2f98_d728_ae22_3dda_b715_be25,
                0x3fff_428a_2f98_d728_ae22_3dda_b715_be26,
            ],
        ),
        (
            0x4000_0000_0000_0000_0000_0000_0000_0000,
            5,
            [
                0x3fff_2611_186b_ae67_496a_521e_4207_60e1,
                0x3fff_2611_186b_ae67_496a_521e_4207_60e2,
            ],
        ),
        (
            0x305e_1234_5678_9abc_deff_edcb_a987_6543,
            3,
            [
                0x3ac9_4a02_c314_7fde_17aa_6e7d_2c06_a5d6,
                0x3ac9_4a02_c314_7fde_17aa_6e7d_2c06_a5d7,
            ],
        ),
        (
            0x305e_1234_5678_9abc_deff_edcb_a987_6543,
            5,
            [
                0x3cde_c3e3_57ed_32cd_864c_45b5_47a5_7790,
                0x3cde_c3e3_57ed_32cd_864c_45b5_47a5_7791,
            ],
        ),
        (
            0x4fa8_7654_321a_9876_5432_10ab_cdef_1234,
            3,
            [
                0x4537_6e17_043d_957f_e13b_bdeb_c6a7_54cb,
                0x4537_6e17_043d_957f_e13b_bdeb_c6a7_54cc,
            ],
        ),
        (
            0x4fa8_7654_321a_9876_5432_10ab_cdef_1234,
            5,
            [
                0x4320_e0e9_c324_8f16_8fc9_0e33_6529_674c,
                0x4320_e0e9_c324_8f16_8fc9_0e33_6529_674d,
            ],
        ),
    ];
    for (target, degree, [lo, hi]) in fixtures {
        let root = root_point(Quad::from_bits(target), degree)?;
        assert!(
            root.lo <= Quad::from_bits(lo),
            "lower {target:x} ^ (1/{degree})"
        );
        assert!(
            root.hi >= Quad::from_bits(hi),
            "upper {target:x} ^ (1/{degree})"
        );
        // All fixtures have irrational roots: sharing a binary64 cell cannot
        // authorize collapsing the mathematical enclosure to an exact point.
        assert!(!root.proves_equal(root));
    }
    Ok(())
}

#[test]
fn whole_cell_containment_respects_even_midpoints_without_epsilon()
-> Result<(), NumericUncertaintyV1> {
    assert!(scalar(1.0).proves_equal(scalar(1.0)));
    assert!(!interval(1.0, 2.0).proves_equal(interval(1.0, 2.0)));
    for offset in [0_u64, 1] {
        let lower = scalar(f64::from_bits(1.0_f64.to_bits() + offset));
        let upper = scalar(f64::from_bits(1.0_f64.to_bits() + offset + 1));
        let midpoint = lower.add(upper)?.mul(scalar(0.5))?;
        let even = 1.0_f64.to_bits() + if offset == 0 { 0 } else { 2 };
        assert_eq!(midpoint.nearest_even_cell()?, even);
        let crossing = CertifiedScalarV1 {
            lo: checked(midpoint.lo.next_down())?,
            hi: checked(midpoint.hi.next_up())?,
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
    Ok(())
}

#[test]
fn gamma_branch_crossing_encloses_the_literal_coefficient_discontinuity()
-> Result<(), NumericUncertaintyV1> {
    let split = scalar(SRGB_DECODE_SPLIT).mul(scalar(255.0))?;
    let range = CertifiedScalarV1 {
        lo: checked(split.lo.next_down())?,
        hi: checked(split.hi.next_up())?,
    };
    let decoded = decode(range)?;
    assert!(decoded.upper_difference(CertifiedScalarV1::point(decoded.lo))? > 2.0e-9);
    assert_eq!(
        decoded.nearest_even_cell(),
        Err(NumericUncertaintyV1::UnresolvedRoundingCell)
    );
    Ok(())
}

#[test]
fn nested_composition_retains_exact_value_hidden_by_shared_binary64_cell()
-> Result<(), NumericUncertaintyV1> {
    let backdrop = composite([244.0; 3], 0.85, rgb([255.0; 3]));
    let nested = composite([1.0; 3], 0.5, backdrop).0[0];
    let host = scalar(123.325);
    assert_eq!(nested.nearest_even_cell()?, host.nearest_even_cell()?);
    assert!(nested.strictly_below(host));
    let evidence = nested.evidence()?;
    assert_eq!(
        evidence.lower_bits,
        0x4005_ed4c_cccc_cccc_ccf0_0000_0000_0000
    );
    assert_eq!(evidence.lower_bits, evidence.upper_bits);
    assert_eq!(nested.upper_difference(host)?, -2.7200464103316335e-15);
    Ok(())
}

#[test]
fn admitted_alpha_keeps_deep_composition_inside_the_convex_physical_domain()
-> Result<(), NumericUncertaintyV1> {
    for initial in [[0.0; 3], [255.0; 3], [0.0, 255.0, 0.0]] {
        let mut composed = rgb(initial);
        for _ in 0..64 {
            composed = composite(initial, 0.7, composed);
        }
        for (value, expected) in composed.0.into_iter().zip(initial) {
            assert_eq!(value.lo, lift(expected)?);
            assert_eq!(value.hi, lift(expected)?);
        }
        let transparent = composite([100.0; 3], 0.0, composed);
        let opaque = composite([100.0; 3], 1.0, composed);
        for channel in 0..3 {
            assert_eq!(transparent.0[channel].lo, composed.0[channel].lo);
            assert_eq!(opaque.0[channel].hi, lift(100.0)?);
        }
    }
    Ok(())
}

#[test]
fn descending_profile_encloses_order_statistics_when_intervals_cross()
-> Result<(), NumericUncertaintyV1> {
    let mut values = [interval(0.0, 10.0), interval(7.0, 8.0), interval(3.0, 12.0)];
    let mut scratch = [scalar(0.0); 3];
    sort_descending_profile(&mut values, &mut scratch)?;
    for (value, (lo, hi)) in values
        .into_iter()
        .zip([(7.0, 12.0), (3.0, 10.0), (0.0, 8.0)])
    {
        assert_eq!((value.lo, value.hi), (lift(lo)?, lift(hi)?));
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
    Ok(())
}

const SOURCE: [f64; 3] = [49.0, 85.0, 190.0];
const ANCHOR: [f64; 3] = [80.0, 89.0, 109.0];

#[test]
fn luminance_and_signed_oklab_enclose_independent_rational_oracle()
-> Result<(), NumericUncertaintyV1> {
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
        let [l, a, b] = rgb(color).oklab()?;
        let actual = [rgb(color).relative_luminance()?, l, a, b];
        for (actual, [lo, hi]) in actual.into_iter().zip(expected) {
            assert!(actual.lo <= Quad::from_bits(lo));
            assert!(actual.hi >= Quad::from_bits(hi));
        }
    }
    Ok(())
}

/// The expected cells and drift bounds were independently checked with exact
/// Fraction arithmetic and integer nth-root enclosures at 240 fractional bits.
#[test]
fn implicit_composition_accepts_corrected_cells_but_rejects_old_f64_residual()
-> Result<(), NumericUncertaintyV1> {
    let materials = [[234.0, 221.0, 234.0], [211.0, 233.0, 221.0]];
    let opacity = [0.7, 0.65];
    let roots = [[217.0, 229.0, 250.0], [248.0, 243.0, 236.0]];
    let backgrounds = std::array::from_fn::<_, 4, _>(|i| {
        composite(materials[i % 2], opacity[i % 2], rgb(roots[i / 2]))
    });
    let reference = std::array::from_fn::<_, 4, _>(|i| {
        rgb(SOURCE)
            .contrast(composite(materials[i % 2], opacity[i % 2], rgb([255.0; 3])))
            .expect("reference material contrast in finite sRGB fixture")
    });
    let measured =
        |color| backgrounds.map(|bg| rgb(color).contrast(bg).expect("fixture backdrop contrast"));
    let primary = |values: [CertifiedScalarV1; 4]| {
        values
            .into_iter()
            .zip(reference)
            .try_fold(scalar(1.0), |worst, (actual, target)| {
                worst.max(actual.ratio_loss(target)?)
            })
    };
    let anchor_m = measured(ANCHOR);
    let anchor_primary = primary(anchor_m)?;
    let corrected = [45.55648583981751, 80.83558199614441, 185.48517133147033];
    let corrected_m = measured(corrected);
    let expected = [
        0x4015_a048_0742_c6ab,
        0x4016_0c37_81aa_d26e,
        0x4016_96fa_1d26_1ee1,
        0x4017_2b87_bfb1_8961,
    ];
    for i in 0..4 {
        assert_eq!(anchor_m[i].nearest_even_cell()?, expected[i]);
        assert_eq!(corrected_m[i].nearest_even_cell()?, expected[i]);
    }
    let corrected_primary = primary(corrected_m)?;
    assert_eq!(anchor_primary.nearest_even_cell()?, 0x3ff0_73a9_2a88_2f3d);
    assert_eq!(
        corrected_primary.nearest_even_cell()?,
        anchor_primary.nearest_even_cell()?
    );
    let source_lab = rgb(SOURCE).oklab()?;
    let anchor_distance = source_distance_squared(source_lab, rgb(ANCHOR).oklab()?)?;
    let corrected_distance = source_distance_squared(source_lab, rgb(corrected).oklab()?)?;
    assert!(corrected_distance.strictly_below(anchor_distance));
    assert_eq!(
        f64::from_bits(anchor_distance.nearest_even_cell()?),
        0.01892467164678065
    );
    assert_eq!(
        f64::from_bits(corrected_distance.nearest_even_cell()?),
        0.00018183980706483807
    );
    // This gain in source distance accompanies a small true primary loss.
    assert!(anchor_primary.strictly_below(corrected_primary));
    assert!(corrected_primary.upper_difference(anchor_primary)? <= 1.0780454039519933e-18);
    for rejected in [
        [45.55648583981752, 80.83558199614438, 185.4851713314703],
        SOURCE,
    ] {
        let actual = measured(rejected);
        let mut distinct_cell = false;
        for (value, anchor) in actual.into_iter().zip(anchor_m) {
            distinct_cell |= value.nearest_even_cell()? != anchor.nearest_even_cell()?;
        }
        assert!(distinct_cell);
        assert_ne!(
            primary(actual)?.nearest_even_cell()?,
            anchor_primary.nearest_even_cell()?
        );
    }
    Ok(())
}

#[test]
fn explicit_forward_lightness_and_primary_cells_preserve_source_improvement()
-> Result<(), NumericUncertaintyV1> {
    let source_lab = rgb(SOURCE).oklab()?;
    let anchor_lab = rgb(ANCHOR).oklab()?;
    let corrected_lab = rgb([43.36346865614138, 78.15569710108728, 182.58830650509347]).oklab()?;
    let offset = scalar(RELATIVE_LUMINANCE_RATIO_OFFSET);
    let target = scalar(0.6).pow(3)?.add(offset)?;
    let primary = |lightness: CertifiedScalarV1| lightness.pow(3)?.add(offset)?.ratio_loss(target);
    let anchor_primary = primary(anchor_lab[0])?;
    let corrected_primary = primary(corrected_lab[0])?;
    assert_eq!(anchor_lab[0].nearest_even_cell()?, 0x3fdd_b081_3255_4ec6);
    assert_eq!(
        corrected_lab[0].nearest_even_cell()?,
        anchor_lab[0].nearest_even_cell()?
    );
    assert_eq!(anchor_primary.nearest_even_cell()?, 0x3ffc_67be_48bc_b9aa);
    assert_eq!(
        corrected_primary.nearest_even_cell()?,
        anchor_primary.nearest_even_cell()?
    );
    let distance = source_distance_squared(source_lab, corrected_lab)?;
    assert!(distance.strictly_below(source_distance_squared(source_lab, anchor_lab)?));
    assert_eq!(
        f64::from_bits(distance.nearest_even_cell()?),
        0.0004910051490285367
    );
    assert!(anchor_primary.strictly_below(corrected_primary));
    assert!(corrected_primary.upper_difference(anchor_primary)? <= 6.5839731038796344e-18);
    assert_ne!(
        source_lab[0].nearest_even_cell()?,
        anchor_lab[0].nearest_even_cell()?
    );
    assert_ne!(
        primary(source_lab[0])?.nearest_even_cell()?,
        anchor_primary.nearest_even_cell()?
    );
    Ok(())
}
#[test]
fn encoded_box_requires_ordered_finite_physical_endpoints() -> Result<(), NumericUncertaintyV1> {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 256.0] {
        assert!(CertifiedRgbV1::from_encoded_bounds([invalid, 0.0, 0.0], [255.0; 3]).is_err());
        assert!(CertifiedRgbV1::from_encoded_bounds([0.0; 3], [255.0, invalid, 255.0]).is_err());
    }
    assert!(matches!(
        CertifiedRgbV1::from_encoded_bounds([2.0; 3], [1.0; 3]),
        Err(NumericUncertaintyV1::DomainViolation)
    ));
    let point = CertifiedRgbV1::from_encoded_bounds([32.0; 3], [32.0; 3])?;
    let exact = CertifiedRgbV1::from_encoded([32.0; 3])?;
    assert_eq!(
        point.0.map(CertifiedScalarV1::bounds),
        exact.0.map(CertifiedScalarV1::bounds)
    );
    let region = CertifiedRgbV1::from_encoded_bounds([0.0, 32.0, 128.0], [64.0, 100.0, 255.0])?;
    for (channel, (lo, hi)) in region
        .0
        .iter()
        .zip([(0.0, 64.0), (32.0, 100.0), (128.0, 255.0)])
    {
        assert_eq!(
            channel.bounds()[0],
            CertifiedScalarV1::exact(lo)?.bounds()[0]
        );
        assert_eq!(
            channel.bounds()[1],
            CertifiedScalarV1::exact(hi)?.bounds()[1]
        );
    }
    Ok(())
}
#[test]
fn oklab_memoization_preserves_interval_outputs_without_equality_evidence()
-> Result<(), NumericUncertaintyV1> {
    let mut cache = CertifiedOklabCacheV1::default();
    let mut transforms =
        crate::certified_numeric::PointTransformCache::new().expect("bounded test workspace");
    let colors = [
        CertifiedRgbV1::from_encoded_bounds([42.0, 50.0, 80.0], [43.0, 51.0, 81.0])?,
        CertifiedRgbV1::from_encoded([42.5, 50.5, 80.5])?,
        CertifiedRgbV1::from_encoded([255.0, 230.0, 123.0])?,
    ];
    for index in [0, 1, 0, 1, 2, 0] {
        let rgb = colors[index];
        assert_eq!(
            cache
                .evaluate(rgb, &mut transforms)?
                .map(CertifiedScalarV1::bounds),
            rgb.oklab()?.map(CertifiedScalarV1::bounds)
        );
    }
    assert!(
        !colors[0].proves_equal(colors[0]),
        "memoization of an enclosure does not prove real equality"
    );
    Ok(())
}

#[test]
fn point_transform_cache_keeps_refined_endpoints_across_region_splits_and_collisions()
-> Result<(), NumericUncertaintyV1> {
    let mut cache = PointTransformCache::new().expect("bounded test workspace");
    for i in 0..192_u32 {
        // Child and parent boxes share some, but never all, channel bounds.
        // The repeated upper g endpoint also exercises non-integral blending.
        let r = f64::from((i * 37) % 129);
        let b = f64::from((i * 19) % 129);
        let rgb = CertifiedRgbV1::from_encoded_bounds([r, 0.0, b], [r + 1.0, 127.5, b + 12.0])?;
        assert_eq!(
            cache.oklab(rgb)?.map(CertifiedScalarV1::bounds),
            rgb.oklab()?.map(CertifiedScalarV1::bounds),
        );
    }
    Ok(())
}
