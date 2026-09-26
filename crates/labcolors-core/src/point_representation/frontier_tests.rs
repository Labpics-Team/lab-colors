//! Проверка границы поиска независимо от его binary search и endpoint-предиката.
//! Oracle перебирает все 256 источников, но вызывает тот же объявленный композитор:
//! это доказательство полноты поиска на названных границах, не модель другого renderer.
use super::*;
use crate::composition::source_over_channel_srgb8;

fn matching_sources(target: u8, backdrop: u8, opacity: AdmittedOpacityV1) -> Vec<u8> {
    (0..=u8::MAX)
        .filter(|source| source_over_channel_srgb8(*source, opacity.value(), backdrop) == target)
        .collect()
}

#[test]
fn exhaustive_channel_frontiers_keep_exact_solutions_and_reject_predecessors() {
    let mut pairs = 0;
    let mut nonzero = 0;
    for backdrop in 0..=u8::MAX {
        for target in 0..=u8::MAX {
            let target_rgb = Srgb8::new([target; 3]);
            let backdrop_rgb = Srgb8::new([backdrop; 3]);
            let first = first_opacity(target_rgb, backdrop_rgb);
            let sources = matching_sources(target, backdrop, first);
            assert!(
                !sources.is_empty(),
                "frontier lost a source: target={target} backdrop={backdrop}"
            );
            let fixed = OpacityDomainV1::try_new(first.value(), first.value()).unwrap();
            let verified = resolve_exact_point_representation_v1(target_rgb, fixed, backdrop_rgb)
                .expect("a fixed feasible frontier must remain usable");
            assert_eq!(verified.opacity(), first);
            assert_eq!(verified.evidence().actual, target_rgb);
            assert!(
                verified
                    .source()
                    .bytes()
                    .iter()
                    .all(|source| sources.contains(source))
            );
            if let Some(previous) = first.predecessor() {
                nonzero += 1;
                assert!(
                    matching_sources(target, backdrop, previous).is_empty(),
                    "frontier is not minimal at the predecessor: target={target} backdrop={backdrop}"
                );
                assert_eq!(source_at_opacity(target_rgb, previous, backdrop_rgb), None);
                let rejected =
                    OpacityDomainV1::try_new(previous.value(), previous.value()).unwrap();
                assert_eq!(
                    resolve_exact_point_representation_v1(target_rgb, rejected, backdrop_rgb),
                    Err(ResolvePointRepresentationErrorV1::Proposal(
                        PointRepresentationProposalErrorV1::NoFeasibleOpacity {
                            domain: rejected,
                            first_feasible: first,
                        }
                    ))
                );
            } else {
                assert_eq!(target, backdrop);
                assert_eq!(first, AdmittedOpacityV1::TRANSPARENT);
            }
            if first != AdmittedOpacityV1::OPAQUE {
                let successor = AdmittedOpacityV1::new(f64::from_bits(first.bits() + 1)).unwrap();
                let next_sources = matching_sources(target, backdrop, successor);
                let next_fixed =
                    OpacityDomainV1::try_new(successor.value(), successor.value()).unwrap();
                let next =
                    resolve_exact_point_representation_v1(target_rgb, next_fixed, backdrop_rgb)
                        .expect("a fixed successor may not be silently moved or rejected");
                assert_eq!(next.opacity(), successor);
                assert_eq!(next.evidence().actual, target_rgb);
                assert!(
                    next.source()
                        .bytes()
                        .iter()
                        .all(|source| next_sources.contains(source))
                );
            }
            pairs += 1;
        }
    }
    assert_eq!(pairs, 65_536);
    assert_eq!(nonzero, 65_536 - 256);
}

#[test]
fn crossed_rgb_frontiers_require_all_three_channels() {
    let mut cases = 0;
    for source_seed in 0_u8..=u8::MAX {
        for backdrop_seed in 0_u8..=u8::MAX {
            let target = [
                source_seed,
                u8::MAX - source_seed,
                source_seed.rotate_left(1),
            ];
            let backdrop = [
                backdrop_seed,
                u8::MAX - backdrop_seed,
                backdrop_seed.rotate_left(1),
            ];
            let scalar = std::array::from_fn::<_, 3, _>(|i| {
                first_opacity(Srgb8::new([target[i]; 3]), Srgb8::new([backdrop[i]; 3]))
            });
            let required = scalar.into_iter().max_by_key(|value| value.bits()).unwrap();
            let target = Srgb8::new(target);
            let backdrop = Srgb8::new(backdrop);
            assert_eq!(
                first_opacity(target, backdrop),
                required,
                "a successful RGB frontier must satisfy every channel, not just one"
            );
            let domain = OpacityDomainV1::try_new(0.0, 1.0).unwrap();
            let verified = resolve_exact_point_representation_v1(target, domain, backdrop).unwrap();
            assert_eq!(verified.opacity(), required);
            assert_eq!(verified.evidence().target, verified.evidence().actual);
            if let Some(previous) = required.predecessor() {
                let tight_channel = scalar.iter().position(|value| *value == required).unwrap();
                assert!(
                    matching_sources(
                        target.bytes()[tight_channel],
                        backdrop.bytes()[tight_channel],
                        previous
                    )
                    .is_empty()
                );
            }
            cases += 1;
        }
    }
    assert_eq!(cases, 65_536);
}
