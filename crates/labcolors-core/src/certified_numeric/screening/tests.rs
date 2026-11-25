use super::super::{CertifiedOklabCacheV1, source_distance_squared};
use super::*;
use crate::composition::AdmittedOpacityV1;

fn assert_contains(screened: CertifiedScalarV1, refined: CertifiedScalarV1) {
    assert!(screened.lo <= refined.lo && screened.hi >= refined.hi);
}

#[test]
fn screened_basic_operations_enclose_refined_arithmetic() -> Result<(), NumericUncertaintyV1> {
    let values = [
        0.0,
        -0.0,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        0.04045,
        -0.055,
        1.0,
        -1.0,
        12.92,
        255.0,
        1.0e-150,
        -1.0e150,
        f64::MAX,
        -f64::MAX,
    ];
    for a in values {
        for b in values {
            let s = ScreenedScalarV1::exact(a)?;
            let t = ScreenedScalarV1::exact(b)?;
            let q = CertifiedScalarV1::exact(a)?;
            let r = CertifiedScalarV1::exact(b)?;
            // Exact-zero identities need no outward step. Other operations
            // need both adjacent binary64 values, even at the finite extrema.
            let rounds_out_of_range = |value: f64| value.abs() >= f64::MAX;
            for (screened, refined, expected_error) in [
                (
                    s.add(t),
                    q.add(r),
                    (a != 0.0 && b != 0.0 && rounds_out_of_range(a + b))
                        .then_some(NumericUncertaintyV1::PrecisionExhausted),
                ),
                (
                    s.mul(t),
                    q.mul(r),
                    rounds_out_of_range(a * b).then_some(NumericUncertaintyV1::PrecisionExhausted),
                ),
                (
                    s.div(t),
                    q.div(r),
                    if b == 0.0 {
                        Some(NumericUncertaintyV1::DivisionByZero)
                    } else {
                        rounds_out_of_range(a / b)
                            .then_some(NumericUncertaintyV1::PrecisionExhausted)
                    },
                ),
            ] {
                match expected_error {
                    Some(reason) => {
                        assert_eq!(screened.err(), Some(reason), "inputs {a}, {b}");
                        if reason == NumericUncertaintyV1::DivisionByZero {
                            assert_eq!(refined.err(), Some(reason));
                        } else {
                            refined?;
                        }
                    }
                    None => assert_contains(screened?.certified()?, refined?),
                }
            }
        }
    }
    for (lo, hi) in [
        (-10.0, 12.0),
        (-2.0, -1.0),
        (0.0, 255.0),
        (1.0e-150, 2.0e-150),
    ] {
        let screened = ScreenedScalarV1::range(lo, hi)?;
        let refined = screened.certified()?;
        assert_contains(screened.square()?.certified()?, refined.square()?);
    }
    Ok(())
}

#[test]
fn root_brackets_are_verified_independently_of_seed_accuracy() -> Result<(), NumericUncertaintyV1> {
    for degree in [3, 5] {
        for target in [1.0e-120_f64, 0.0001, 0.01, 0.2, 0.9, 2.0, 1.0e100] {
            // Center offsets on an independently certified nearest binary64
            // cell. Host powf(0.2) may itself be many ulps from a fifth root.
            let target_quad = lift(target)?;
            let center_bits = super::super::root_point(target_quad, degree)?.nearest_even_cell()?;
            for offset in [-32_i64, -8, 0, 8, 32] {
                let seed = f64::from_bits(center_bits.wrapping_add_signed(offset));
                let result = root_from_seed(target, degree, seed);
                if offset.unsigned_abs() == 32 {
                    assert_eq!(
                        result.err(),
                        Some(NumericUncertaintyV1::PrecisionExhausted),
                        "unexpected bracket at degree {degree}, target {target}, offset {offset}"
                    );
                } else {
                    let bracket = result?;
                    let lower_power = CertifiedScalarV1::exact(bracket.lo)?.pow(degree)?;
                    let upper_power = CertifiedScalarV1::exact(bracket.hi)?.pow(degree)?;
                    assert!(lower_power.hi <= target_quad && upper_power.lo >= target_quad);
                }
            }
        }
    }
    assert!(root_from_seed(0.2, 5, f64::NAN).is_err());
    assert!(root_from_seed(0.2, 5, 1000.0).is_err());
    assert!(root_point(f64::from_bits(1), 5).is_err());
    Ok(())
}

#[test]
fn screened_color_bounds_cover_points_composites_and_crossing_regions()
-> Result<(), NumericUncertaintyV1> {
    let mut random = 0x1234_5678_1234_5678_u64;
    let mut cases = Vec::new();
    let backdrop = CertifiedRgbV1::from_encoded([244.0, 234.0, 219.0])?;
    for _ in 0..128 {
        let channels = std::array::from_fn(|_| {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            (random >> 32) as u32 as f64 / u32::MAX as f64 * 255.0
        });
        let rgb = CertifiedRgbV1::from_encoded(channels)?;
        cases.push(rgb);
        cases.push(rgb.source_over(
            AdmittedOpacityV1::new(0.5).expect("half opacity is admitted"),
            backdrop,
        )?);
    }
    for x in [0.0, 10.31475, 128.0, 255.0] {
        cases.push(CertifiedRgbV1::from_encoded([x; 3])?);
    }
    for (lo, hi) in [
        ([0.0; 3], [255.0; 3]),
        ([10.31474; 3], [10.31476; 3]),
        ([48.999, 84.999, 189.999], [49.001, 85.001, 190.001]),
    ] {
        cases.push(CertifiedRgbV1::from_encoded_bounds(lo, hi)?);
    }
    for rgb in cases {
        for (screened, refined) in oklab(rgb)?.into_iter().zip(rgb.oklab()?) {
            assert_contains(screened, refined);
        }
    }
    Ok(())
}

#[test]
fn screening_does_not_turn_overlap_into_an_order_or_rounding_cell()
-> Result<(), NumericUncertaintyV1> {
    let anchor = CertifiedRgbV1::from_encoded([49.0, 85.0, 190.0])?.oklab()?;
    let colors = [
        CertifiedRgbV1::from_encoded([45.55648583981751, 80.83558199614441, 185.48517133147033])?,
        CertifiedRgbV1::from_encoded([45.55648583981752, 80.83558199614438, 185.4851713314703])?,
    ];
    let coarse = [
        source_distance_squared(oklab(colors[0])?, anchor)?,
        source_distance_squared(oklab(colors[1])?, anchor)?,
    ];
    assert!(!coarse[0].strictly_below(coarse[1]) && !coarse[1].strictly_below(coarse[0]));
    let refined = [
        source_distance_squared(colors[0].oklab()?, anchor)?,
        source_distance_squared(colors[1].oklab()?, anchor)?,
    ];
    assert!(refined[0].strictly_below(refined[1]) || refined[1].strictly_below(refined[0]));
    assert!(
        oklab(colors[0])?
            .iter()
            .all(|value| value.evidence().is_err())
    );
    Ok(())
}

#[test]
fn memoization_upgrades_screened_values_and_fallback_uses_original_rgb()
-> Result<(), NumericUncertaintyV1> {
    let mut cache = CertifiedOklabCacheV1::default();
    let mut transforms =
        crate::certified_numeric::PointTransformCache::new().expect("bounded test workspace");
    let rgb = CertifiedRgbV1::from_encoded([49.0, 85.0, 190.0])?;
    let screened = cache
        .evaluate_screened(rgb, &mut transforms)?
        .map(CertifiedScalarV1::bounds);
    let refined = rgb.oklab()?.map(CertifiedScalarV1::bounds);
    assert_ne!(screened, refined);
    assert_eq!(
        cache
            .evaluate(rgb, &mut transforms)?
            .map(CertifiedScalarV1::bounds),
        refined
    );
    assert_eq!(
        cache
            .evaluate_screened(rgb, &mut transforms)?
            .map(CertifiedScalarV1::bounds),
        refined
    );
    let tiny = CertifiedRgbV1::from_encoded([f64::from_bits(1); 3])?;
    assert!(oklab(tiny).is_err());
    assert_eq!(
        cache
            .evaluate_screened(tiny, &mut transforms)?
            .map(CertifiedScalarV1::bounds),
        tiny.oklab()?.map(CertifiedScalarV1::bounds)
    );
    Ok(())
}

#[test]
fn region_witness_bounds_the_refined_lower_loss() -> Result<(), NumericUncertaintyV1> {
    let backdrop = CertifiedRgbV1::from_encoded([21.0, 75.0, 249.0])?;
    let goals = [[0.0; 3], [255.0; 3], [68.0, 21.0, 213.0]];
    let alpha = AdmittedOpacityV1::new(0.53).expect("fixture material opacity");
    let mut seed = 17_u32;
    let mut channel = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        255.0 * f64::from(seed) / f64::from(u32::MAX)
    };
    let mut boxes = vec![
        CertifiedRgbV1::from_encoded_bounds([0.0; 3], [255.0; 3])?,
        CertifiedRgbV1::from_encoded_bounds([10.31474; 3], [10.31476; 3])?,
        CertifiedRgbV1::from_encoded_bounds([10.31475, 7.0, 20.0], [10.31475, 217.0, 20.0])?,
        backdrop,
    ];
    for _ in 0..96 {
        let a: [f64; 3] = std::array::from_fn(|_| channel());
        let b: [f64; 3] = std::array::from_fn(|_| channel());
        boxes.push(CertifiedRgbV1::from_encoded_bounds(
            std::array::from_fn(|i| a[i].min(b[i])),
            std::array::from_fn(|i| a[i].max(b[i])),
        )?);
    }
    // The uncoupled LMS enclosure of the full gamut admits each preferred
    // opponent coordinate. A useless infinite loss would pass the lower-bound
    // check below, but cannot rule out a region comparison.
    let goal = CertifiedRgbV1::from_encoded(goals[2])?.oklab()?;
    let small = CertifiedScalarV1::exact(1e-30)?;
    assert!(region_loss_witness(boxes[0], goal)?.strictly_below(small));
    for region in boxes {
        for rgb in [region, region.source_over(alpha, backdrop)?] {
            let lab = rgb.oklab()?;
            for goal in goals {
                let preferred = CertifiedRgbV1::from_encoded(goal)?.oklab()?;
                let refined = source_distance_squared(lab, preferred)?;
                let witness = region_loss_witness(rgb, preferred)?;
                assert!(
                    refined.lo <= witness.hi,
                    "refined lower loss exceeds its upper bound"
                );
            }
        }
    }
    Ok(())
}
