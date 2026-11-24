use super::super::{CertifiedOklabCacheV1, source_distance_squared};
use super::*;
use crate::composition::AdmittedOpacityV1;

fn assert_contains(screened: CertifiedScalarV1, refined: CertifiedScalarV1) {
    assert!(screened.lo <= refined.lo && screened.hi >= refined.hi);
}

#[test]
fn screened_basic_operations_enclose_refined_arithmetic() {
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
            let s = ScreenedScalarV1::exact(a).unwrap();
            let t = ScreenedScalarV1::exact(b).unwrap();
            let q = CertifiedScalarV1::exact(a).unwrap();
            let r = CertifiedScalarV1::exact(b).unwrap();
            for (screened, refined) in [
                (s.add(t), q.add(r)),
                (s.mul(t), q.mul(r)),
                (s.div(t), q.div(r)),
            ] {
                if let Ok(screened) = screened {
                    assert_contains(screened.certified().unwrap(), refined.unwrap());
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
        let screened = ScreenedScalarV1::range(lo, hi).unwrap();
        let refined = screened.certified().unwrap();
        assert_contains(
            screened.square().unwrap().certified().unwrap(),
            refined.square().unwrap(),
        );
    }
}

#[test]
fn root_brackets_are_verified_independently_of_seed_accuracy() {
    for degree in [3, 5] {
        for target in [1.0e-120_f64, 0.0001, 0.01, 0.2, 0.9, 2.0, 1.0e100] {
            let seed = if degree == 3 {
                target.cbrt()
            } else {
                target.powf(0.2)
            };
            for offset in [-32_i64, -8, 0, 8, 32] {
                let seed = f64::from_bits((seed.to_bits() as i64 + offset) as u64);
                if let Ok(bracket) = root_from_seed(target, degree, seed) {
                    let lower_power = CertifiedScalarV1::exact(bracket.lo)
                        .unwrap()
                        .pow(degree)
                        .unwrap();
                    let upper_power = CertifiedScalarV1::exact(bracket.hi)
                        .unwrap()
                        .pow(degree)
                        .unwrap();
                    let target = lift(target).unwrap();
                    assert!(lower_power.hi <= target && upper_power.lo >= target);
                }
            }
        }
    }
    assert!(root_from_seed(0.2, 5, f64::NAN).is_err());
    assert!(root_from_seed(0.2, 5, 1000.0).is_err());
    assert!(root_point(f64::from_bits(1), 5).is_err());
}

#[test]
fn screened_color_bounds_cover_points_composites_and_crossing_regions() {
    let mut random = 0x1234_5678_1234_5678_u64;
    let mut cases = Vec::new();
    let backdrop = CertifiedRgbV1::from_encoded([244.0, 234.0, 219.0]).unwrap();
    for _ in 0..128 {
        let channels = std::array::from_fn(|_| {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            (random >> 32) as u32 as f64 / u32::MAX as f64 * 255.0
        });
        let rgb = CertifiedRgbV1::from_encoded(channels).unwrap();
        cases.push(rgb);
        cases.push(
            rgb.source_over(AdmittedOpacityV1::new(0.5).unwrap(), backdrop)
                .unwrap(),
        );
    }
    for x in [0.0, 10.31475, 128.0, 255.0] {
        cases.push(CertifiedRgbV1::from_encoded([x; 3]).unwrap());
    }
    for (lo, hi) in [
        ([0.0; 3], [255.0; 3]),
        ([10.31474; 3], [10.31476; 3]),
        ([48.999, 84.999, 189.999], [49.001, 85.001, 190.001]),
    ] {
        cases.push(CertifiedRgbV1::from_encoded_bounds(lo, hi).unwrap());
    }
    for rgb in cases {
        for (screened, refined) in oklab(rgb).unwrap().into_iter().zip(rgb.oklab().unwrap()) {
            assert_contains(screened, refined);
        }
    }
}

#[test]
fn screening_does_not_turn_overlap_into_an_order_or_rounding_cell() {
    let anchor = CertifiedRgbV1::from_encoded([49.0, 85.0, 190.0])
        .unwrap()
        .oklab()
        .unwrap();
    let colors = [
        [45.55648583981751, 80.83558199614441, 185.48517133147033],
        [45.55648583981752, 80.83558199614438, 185.4851713314703],
    ]
    .map(|rgb| CertifiedRgbV1::from_encoded(rgb).unwrap());
    let coarse = colors.map(|rgb| source_distance_squared(oklab(rgb).unwrap(), anchor).unwrap());
    assert!(!coarse[0].strictly_below(coarse[1]) && !coarse[1].strictly_below(coarse[0]));
    let refined = colors.map(|rgb| source_distance_squared(rgb.oklab().unwrap(), anchor).unwrap());
    assert!(refined[0].strictly_below(refined[1]) || refined[1].strictly_below(refined[0]));
    assert!(
        oklab(colors[0])
            .unwrap()
            .iter()
            .all(|value| value.evidence().is_err())
    );
}

#[test]
fn memoization_upgrades_screened_values_and_fallback_uses_original_rgb() {
    let mut cache = CertifiedOklabCacheV1::default();
    let rgb = CertifiedRgbV1::from_encoded([49.0, 85.0, 190.0]).unwrap();
    let screened = cache
        .evaluate_screened(rgb)
        .unwrap()
        .map(CertifiedScalarV1::bounds);
    let refined = rgb.oklab().unwrap().map(CertifiedScalarV1::bounds);
    assert_ne!(screened, refined);
    assert_eq!(
        cache.evaluate(rgb).unwrap().map(CertifiedScalarV1::bounds),
        refined
    );
    assert_eq!(
        cache
            .evaluate_screened(rgb)
            .unwrap()
            .map(CertifiedScalarV1::bounds),
        refined
    );
    let tiny = CertifiedRgbV1::from_encoded([f64::from_bits(1); 3]).unwrap();
    assert!(oklab(tiny).is_err());
    assert_eq!(
        cache
            .evaluate_screened(tiny)
            .unwrap()
            .map(CertifiedScalarV1::bounds),
        tiny.oklab().unwrap().map(CertifiedScalarV1::bounds)
    );
}
