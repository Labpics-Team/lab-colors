//! Полный перебор небольших геометрий и dirty-областей через настоящие raster API.
//! Прямой целочисленный 2D-эталон не использует разделяемые проходы или Q32-таблицу.
use super::*;

fn blur_request<'a>(
    geometry: FieldExtentV1,
    ratio: u8,
    revision: u64,
    pixels: &'a [PremultipliedRgba8V1],
) -> FieldEvaluationRequestV1<'a> {
    request(
        701,
        geometry,
        dpr(ratio),
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
        revision,
        CarrierIntentV1::Contributes,
        FieldOperationV1::GaussianBlur {
            source: premultiplied_raster(702, geometry, pixels),
            kernel: GaussianKernelV1::canonical_one_css_pixel(dpr(ratio)),
            edge_mode: GaussianEdgeModeV1::ClampToEdgeV1,
        },
    )
}

fn pascal_row(radius: u8) -> Vec<u128> {
    // Сложение соседей, не мультипликативная рекурсия production-validator.
    let mut row = vec![1_u128];
    for _ in 0..2 * radius {
        let mut next = vec![1; row.len() + 1];
        for index in 1..row.len() {
            next[index] = row[index - 1] + row[index];
        }
        row = next;
    }
    row
}

fn direct_binomial_2d(
    geometry: FieldExtentV1,
    radius: u8,
    pixels: &[PremultipliedRgba8V1],
) -> Vec<PremultipliedRgba8V1> {
    // Прямое двумерное ядро, без промежуточного округления.
    let row = pascal_row(radius);
    let denominator = 1_u128 << (4 * radius);
    let mut output = Vec::new();
    for y in 0..geometry.height() {
        for x in 0..geometry.width() {
            let mut numerator = [0_u128; 4];
            for (ky, wy) in row.iter().enumerate() {
                for (kx, wx) in row.iter().enumerate() {
                    let sx = (i64::from(x) + kx as i64 - i64::from(radius))
                        .clamp(0, i64::from(geometry.width()) - 1)
                        as usize;
                    let sy = (i64::from(y) + ky as i64 - i64::from(radius))
                        .clamp(0, i64::from(geometry.height()) - 1)
                        as usize;
                    let sample = pixels[sy * geometry.width() as usize + sx].channels();
                    for channel in 0..4 {
                        numerator[channel] += u128::from(sample[channel]) * wx * wy;
                    }
                }
            }
            output.push(
                PremultipliedRgba8V1::try_new(
                    numerator.map(|value| {
                        u8::try_from((value + denominator / 2) / denominator).unwrap()
                    }),
                )
                .unwrap(),
            );
        }
    }
    output
}

#[test]
fn every_small_dirty_rectangle_matches_independent_full_convolution() {
    let mut cases = 0;
    for height in 1..=5_u32 {
        for width in 1..=5_u32 {
            let geometry = extent(width, height);
            let before: Vec<_> = (0..width * height)
                .map(|index| {
                    let alpha = ((index * 61 + 97) % 256) as u8;
                    pixel(alpha, alpha / 2, alpha / 3, alpha)
                })
                .collect();
            for ratio in 1..=4_u8 {
                let old = blur_request(geometry, ratio, 1, &before);
                let baseline = direct_binomial_2d(geometry, ratio, &before);
                for y in 0..height {
                    for x in 0..width {
                        for dh in 1..=height - y {
                            for dw in 1..=width - x {
                                let dirty = rect(geometry, x, y, dw, dh);
                                let mut after = before.clone();
                                for cy in y..y + dh {
                                    for cx in x..x + dw {
                                        let index = (cy * width + cx) as usize;
                                        let alpha = 255 - after[index].alpha();
                                        after[index] = pixel(alpha / 3, alpha, alpha / 2, alpha);
                                    }
                                }
                                let next = blur_request(geometry, ratio, 2, &after);
                                let mut incremental = FieldEvaluationScratchV1::new();
                                assert_eq!(
                                    evaluate_reference_full(&old, &mut incremental).unwrap(),
                                    baseline
                                );
                                let capacity = incremental.capacity_snapshot_for_test();
                                let pointers = incremental.pointer_snapshot_for_test();
                                let influence = evaluate_reference_incremental(
                                    &old,
                                    &next,
                                    dirty,
                                    &mut incremental,
                                )
                                .unwrap();
                                let radius = u32::from(ratio);
                                let left = x.saturating_sub(radius);
                                let top = y.saturating_sub(radius);
                                let right = (x + dw + radius).min(width);
                                let bottom = (y + dh + radius).min(height);
                                let expected_region =
                                    rect(geometry, left, top, right - left, bottom - top);
                                let oracle = direct_binomial_2d(geometry, ratio, &after);
                                assert_eq!(
                                    incremental.output(),
                                    oracle,
                                    "extent={width}x{height}, DPR={ratio}, dirty={dirty:?}"
                                );
                                let mut full = FieldEvaluationScratchV1::new();
                                assert_eq!(
                                    evaluate_reference_full(&next, &mut full).unwrap(),
                                    oracle
                                );
                                assert_eq!(influence.exact(), expected_region);
                                assert_eq!(influence.conservative(), expected_region);
                                assert_eq!(incremental.capacity_snapshot_for_test(), capacity);
                                assert_eq!(incremental.pointer_snapshot_for_test(), pointers);
                                for cy in 0..height {
                                    for cx in 0..width {
                                        if cx < left || cx >= right || cy < top || cy >= bottom {
                                            let index = (cy * width + cx) as usize;
                                            assert_eq!(
                                                incremental.output()[index],
                                                baseline[index]
                                            );
                                        }
                                    }
                                }
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        cases, 4_900,
        "all geometry/rectangle/DPR cases must execute"
    );
}

#[test]
fn rejected_dirty_scope_preserves_output_and_allows_correct_retry() {
    let geometry = extent(5, 3);
    let before = vec![pixel(32, 16, 8, 64); 15];
    let old = blur_request(geometry, 2, 1, &before);
    let mut after = before.clone();
    after[7] = pixel(192, 128, 64, 224);
    after[0] = PremultipliedRgba8V1::TRANSPARENT;
    let next = blur_request(geometry, 2, 2, &after);
    let mut scratch = FieldEvaluationScratchV1::new();
    let initial = evaluate_reference_full(&old, &mut scratch)
        .unwrap()
        .to_vec();
    let capacity = scratch.capacity_snapshot_for_test();
    assert_eq!(
        evaluate_reference_incremental(&old, &next, rect(geometry, 2, 1, 1, 1), &mut scratch),
        Err(FieldEvaluationErrorV1::IncrementalChangeOutsideDirtyRegion { pixel_index: 0 })
    );
    assert_eq!(scratch.output(), initial);
    assert_eq!(scratch.capacity_snapshot_for_test(), capacity);
    evaluate_reference_incremental(&old, &next, rect(geometry, 0, 0, 3, 2), &mut scratch).unwrap();
    assert_eq!(scratch.output(), direct_binomial_2d(geometry, 2, &after));
    // После успешного retry прежний запрос больше не удостоверяет рабочий буфер.
    let committed = scratch.output().to_vec();
    assert_eq!(
        evaluate_reference_incremental(&old, &next, FieldRectV1::full(geometry), &mut scratch),
        Err(FieldEvaluationErrorV1::IncrementalPreviousRequestMismatch)
    );
    assert_eq!(scratch.output(), committed);
}

fn pointwise_request<'a>(
    geometry: FieldExtentV1,
    lighter: bool,
    src: &'a [PremultipliedRgba8V1],
    dst: &'a [PremultipliedRgba8V1],
    revision: u64,
) -> FieldEvaluationRequestV1<'a> {
    let source = premultiplied_raster(711, geometry, src);
    let destination = premultiplied_raster(712, geometry, dst);
    let operation = if lighter {
        FieldOperationV1::PorterDuffLighter {
            source,
            destination,
        }
    } else {
        FieldOperationV1::PremultipliedSourceOver {
            source,
            destination,
        }
    };
    request(
        713,
        geometry,
        dpr(1),
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
        revision,
        CarrierIntentV1::Contributes,
        operation,
    )
}

#[test]
fn pointwise_consumers_preserve_operator_laws_and_track_both_inputs() {
    let geometry = extent(2, 1);
    let palette = [
        pixel(0, 0, 0, 0),
        pixel(1, 0, 0, 1),
        pixel(64, 32, 16, 128),
        pixel(0, 127, 0, 127),
        pixel(128, 1, 7, 192),
        pixel(255, 128, 64, 255),
    ];
    for source in palette {
        for destination in palette {
            for lighter in [false, true] {
                let sources = [source; 2];
                let destinations = [destination; 2];
                let old = pointwise_request(geometry, lighter, &sources, &destinations, 1);
                let expected = std::array::from_fn(|c| {
                    let s = u32::from(source.channels()[c]);
                    let d = u32::from(destination.channels()[c]);
                    if lighter {
                        (s + d).min(255) as u8
                    } else {
                        ((255 * s + d * (255 - u32::from(source.alpha())) + 127) / 255) as u8
                    }
                });
                let mut scratch = FieldEvaluationScratchV1::new();
                assert_eq!(
                    evaluate_reference_full(&old, &mut scratch).unwrap()[0].channels(),
                    expected
                );
                for change_source in [false, true] {
                    let mut next_sources = sources;
                    let mut next_destinations = destinations;
                    let changed = if change_source {
                        &mut next_sources[0]
                    } else {
                        &mut next_destinations[0]
                    };
                    *changed = if *changed == PremultipliedRgba8V1::TRANSPARENT {
                        pixel(255, 128, 64, 255)
                    } else {
                        PremultipliedRgba8V1::TRANSPARENT
                    };
                    let next =
                        pointwise_request(geometry, lighter, &next_sources, &next_destinations, 2);
                    let baseline = evaluate_reference_full(&old, &mut scratch)
                        .unwrap()
                        .to_vec();
                    assert_eq!(
                        evaluate_reference_incremental(
                            &old,
                            &next,
                            rect(geometry, 1, 0, 1, 1),
                            &mut scratch
                        ),
                        Err(
                            FieldEvaluationErrorV1::IncrementalChangeOutsideDirtyRegion {
                                pixel_index: 0
                            }
                        )
                    );
                    assert_eq!(scratch.output(), baseline);
                    let influence = evaluate_reference_incremental(
                        &old,
                        &next,
                        rect(geometry, 0, 0, 1, 1),
                        &mut scratch,
                    )
                    .unwrap();
                    assert_eq!(influence.exact(), rect(geometry, 0, 0, 1, 1));
                    let mut full = FieldEvaluationScratchV1::new();
                    assert_eq!(
                        scratch.output(),
                        evaluate_reference_full(&next, &mut full).unwrap()
                    );
                    assert_eq!(scratch.output()[1], baseline[1]);
                }
            }
        }
    }
}

#[test]
fn every_supported_binomial_radius_matches_the_direct_oracle() {
    let mut profiles = 0;
    for ratio in 1..=4_u8 {
        for css_radius in 1..=16 / u32::from(ratio) {
            let radius = (css_radius * u32::from(ratio)) as u8;
            let weights = pascal_row(radius)
                .into_iter()
                .map(|coefficient| u32::try_from(coefficient << (32 - 2 * radius)).unwrap())
                .collect();
            let kernel = GaussianKernelV1::try_new(
                GaussianKernelProfileV1::BinomialGaussianQ32V1,
                css_radius,
                dpr(ratio),
                weights,
            )
            .unwrap();
            for (width, height) in [(1, 1), (3, 2), (2, 5)] {
                let geometry = extent(width, height);
                let before: Vec<_> = (0..width * height)
                    .map(|index| {
                        let alpha = ((index * 61 + 97) % 256) as u8;
                        pixel(alpha / 3, alpha / 2, alpha, alpha)
                    })
                    .collect();
                let mut after = before.clone();
                after[0] = pixel(255, 128, 64, 255);
                let make = |pixels, revision| {
                    request(
                        751,
                        geometry,
                        dpr(ratio),
                        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
                        revision,
                        CarrierIntentV1::Contributes,
                        FieldOperationV1::GaussianBlur {
                            source: premultiplied_raster(752, geometry, pixels),
                            kernel: kernel.clone(),
                            edge_mode: GaussianEdgeModeV1::ClampToEdgeV1,
                        },
                    )
                };
                let old = make(&before, 1);
                let next = make(&after, 2);
                let mut scratch = FieldEvaluationScratchV1::new();
                assert_eq!(
                    evaluate_reference_full(&old, &mut scratch).unwrap(),
                    direct_binomial_2d(geometry, radius, &before)
                );
                evaluate_reference_incremental(
                    &old,
                    &next,
                    rect(geometry, 0, 0, 1, 1),
                    &mut scratch,
                )
                .unwrap();
                assert_eq!(
                    scratch.output(),
                    direct_binomial_2d(geometry, radius, &after),
                    "CSS radius={css_radius}, DPR={ratio}, extent={width}x{height}"
                );
            }
            profiles += 1;
        }
    }
    assert_eq!(profiles, 33);
}
