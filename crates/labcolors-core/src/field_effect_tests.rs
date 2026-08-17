use crate::Srgb8;
use crate::field_effect::{
    CarrierIntentV1, DevicePixelRatioV1, EncodedSrgb8AlphaRasterViewV1, EncodedSrgb8AlphaV1,
    FieldCertificateReplayErrorV1, FieldEvaluationErrorV1, FieldEvaluationRequestV1,
    FieldEvaluationScratchV1, FieldEvidenceClassV1, FieldEvidenceIdentityV1, FieldEvidenceV1,
    FieldExtentV1, FieldGeometryV1, FieldHostConformanceIdV1, FieldHostConformancePermitV1,
    FieldInfluenceV1, FieldOpacityV1, FieldOperationV1, FieldOperatorInstanceIdV1,
    FieldOperatorKindV1, FieldOutputCapabilityV1, FieldPrecisionV1, FieldQuantizationV1,
    FieldRasterIdentityV1, FieldRasterViewV1, FieldRectV1, FieldRenderCapabilityV1,
    FieldRendererCapabilityV1, FieldRendererIdV1, FieldRequestIdV1, FieldSceneRevisionV1,
    FieldUnsupportedReasonIdV1, FieldWorkingSpaceV1, GaussianEdgeModeV1, GaussianKernelProfileV1,
    GaussianKernelV1, OpaqueSrgb8RasterViewV1, PremultipliedRgba8V1, ProspectiveObservedRasterV1,
    evaluate_reference_full, evaluate_reference_incremental, evaluate_whole_field,
    footprint_for_output, influence_for_input, request_digest, verify_certificate_replay,
};
use crate::observation::{ObservationStreamId, Revision};

fn extent(width: u32, height: u32) -> FieldExtentV1 {
    FieldExtentV1::try_new(width, height).unwrap()
}

fn rect(extent: FieldExtentV1, x: u32, y: u32, width: u32, height: u32) -> FieldRectV1 {
    FieldRectV1::try_new(extent, x, y, width, height).unwrap()
}

fn pixel(red: u8, green: u8, blue: u8, alpha: u8) -> PremultipliedRgba8V1 {
    PremultipliedRgba8V1::try_new([red, green, blue, alpha]).unwrap()
}

fn premultiplied_raster<'a>(
    identity: u64,
    extent: FieldExtentV1,
    pixels: &'a [PremultipliedRgba8V1],
) -> FieldRasterViewV1<'a> {
    FieldRasterViewV1::try_new(FieldRasterIdentityV1::new(identity), extent, pixels).unwrap()
}

fn opaque_raster<'a>(
    identity: u64,
    extent: FieldExtentV1,
    pixels: &'a [Srgb8],
) -> OpaqueSrgb8RasterViewV1<'a> {
    OpaqueSrgb8RasterViewV1::try_new(FieldRasterIdentityV1::new(identity), extent, pixels).unwrap()
}

fn screen_pixel(tint: [u8; 3], alpha: f64) -> EncodedSrgb8AlphaV1 {
    EncodedSrgb8AlphaV1::new(Srgb8::new(tint), FieldOpacityV1::try_new(alpha).unwrap())
}

fn screen_raster<'a>(
    identity: u64,
    extent: FieldExtentV1,
    pixels: &'a [EncodedSrgb8AlphaV1],
) -> EncodedSrgb8AlphaRasterViewV1<'a> {
    EncodedSrgb8AlphaRasterViewV1::try_new(FieldRasterIdentityV1::new(identity), extent, pixels)
        .unwrap()
}

fn dpr(raw: u8) -> DevicePixelRatioV1 {
    DevicePixelRatioV1::try_new(raw).unwrap()
}

fn reference_capability(output: FieldOutputCapabilityV1) -> FieldRenderCapabilityV1 {
    FieldRenderCapabilityV1::new(
        FieldRendererCapabilityV1::exact_reference(FieldRendererIdV1::new(10)),
        output,
    )
}

fn host_capability(output: FieldOutputCapabilityV1) -> FieldRenderCapabilityV1 {
    FieldRenderCapabilityV1::new(
        FieldRendererCapabilityV1::host_conformant(host_permit()),
        output,
    )
}

fn host_permit() -> FieldHostConformancePermitV1 {
    FieldHostConformancePermitV1::mint_for_test(
        FieldRendererIdV1::new(20),
        FieldHostConformanceIdV1::new(30),
    )
}

fn other_host_permit() -> FieldHostConformancePermitV1 {
    FieldHostConformancePermitV1::mint_for_test(
        FieldRendererIdV1::new(21),
        FieldHostConformanceIdV1::new(31),
    )
}

fn scene(revision: u64) -> FieldSceneRevisionV1 {
    scene_on(40, revision)
}

fn scene_on(stream: u32, revision: u64) -> FieldSceneRevisionV1 {
    FieldSceneRevisionV1::mint_for_test(ObservationStreamId::new(stream), Revision::new(revision))
}

fn request<'a>(
    request_id: u64,
    extent: FieldExtentV1,
    dpr: DevicePixelRatioV1,
    capability: FieldRenderCapabilityV1,
    revision: u64,
    carrier_intent: CarrierIntentV1,
    operation: FieldOperationV1<'a>,
) -> FieldEvaluationRequestV1<'a> {
    request_at_head(
        request_id,
        extent,
        dpr,
        capability,
        scene(revision),
        carrier_intent,
        operation,
    )
}

fn request_at_head<'a>(
    request_id: u64,
    extent: FieldExtentV1,
    dpr: DevicePixelRatioV1,
    capability: FieldRenderCapabilityV1,
    scene_revision: FieldSceneRevisionV1,
    carrier_intent: CarrierIntentV1,
    operation: FieldOperationV1<'a>,
) -> FieldEvaluationRequestV1<'a> {
    FieldEvaluationRequestV1::try_new(
        FieldRequestIdV1::new(request_id),
        FieldOperatorInstanceIdV1::new(50),
        FieldGeometryV1::new(extent),
        dpr,
        FieldWorkingSpaceV1::EncodedSrgb8PremultipliedV1,
        FieldPrecisionV1::FixedQ32V1,
        FieldQuantizationV1::RoundHalfUpSrgb8V1,
        capability,
        scene_revision,
        carrier_intent,
        operation,
    )
    .unwrap()
}

fn exact_evidence(identity: u64) -> FieldEvidenceV1<'static> {
    FieldEvidenceV1::ExactReferenceWholeRaster {
        identity: FieldEvidenceIdentityV1::new(identity),
    }
}

#[test]
fn weak_evidence_classes_never_upgrade_to_whole_field_proof() {
    let geometry = extent(1, 1);
    let source_pixels = [pixel(64, 32, 16, 128)];
    let destination_pixels = [pixel(8, 8, 8, 255)];
    let source = premultiplied_raster(1, geometry, &source_pixels);
    let destination = premultiplied_raster(2, geometry, &destination_pixels);
    let request = request(
        1,
        geometry,
        dpr(1),
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
        1,
        CarrierIntentV1::Contributes,
        FieldOperationV1::PremultipliedSourceOver {
            source,
            destination,
        },
    );
    let committed_pixels = [pixel(68, 36, 20, 255)];
    let committed = premultiplied_raster(3, geometry, &committed_pixels);
    let weak = [
        FieldEvidenceV1::PointSample {
            identity: FieldEvidenceIdentityV1::new(1),
        },
        FieldEvidenceV1::FieldSamples {
            identity: FieldEvidenceIdentityV1::new(2),
            sample_count: 9,
        },
        FieldEvidenceV1::FieldAverage {
            identity: FieldEvidenceIdentityV1::new(3),
        },
        FieldEvidenceV1::GradientStops {
            identity: FieldEvidenceIdentityV1::new(4),
            stop_count: 3,
        },
        FieldEvidenceV1::CommittedRaster {
            identity: FieldEvidenceIdentityV1::new(5),
            raster: committed,
        },
    ];

    for evidence in weak {
        let mut scratch = FieldEvaluationScratchV1::new();
        assert_eq!(
            evaluate_whole_field(&request, evidence, &mut scratch),
            Err(FieldEvaluationErrorV1::WeakEvidenceCannotProveWholeField {
                class: evidence.class(),
            })
        );
        assert!(scratch.output().is_empty());
    }
}

#[test]
fn renderer_capability_unknown_and_unsupported_are_typed_failures() {
    let geometry = extent(1, 1);
    let source_pixels = [pixel(32, 0, 0, 64)];
    let destination_pixels = [pixel(0, 0, 0, 255)];

    for (renderer, expected) in [
        (
            FieldRendererCapabilityV1::unknown(FieldRendererIdV1::new(77)),
            FieldEvaluationErrorV1::UnknownRenderer {
                renderer: FieldRendererIdV1::new(77),
            },
        ),
        (
            FieldRendererCapabilityV1::unsupported(
                FieldRendererIdV1::new(78),
                FieldUnsupportedReasonIdV1::new(79),
            ),
            FieldEvaluationErrorV1::UnsupportedRenderer {
                renderer: FieldRendererIdV1::new(78),
                reason: FieldUnsupportedReasonIdV1::new(79),
            },
        ),
    ] {
        let source = premultiplied_raster(1, geometry, &source_pixels);
        let destination = premultiplied_raster(2, geometry, &destination_pixels);
        let request = request(
            2,
            geometry,
            dpr(1),
            FieldRenderCapabilityV1::new(renderer, FieldOutputCapabilityV1::PremultipliedRgba8V1),
            1,
            CarrierIntentV1::Contributes,
            FieldOperationV1::PremultipliedSourceOver {
                source,
                destination,
            },
        );
        assert_eq!(
            evaluate_whole_field(
                &request,
                exact_evidence(1),
                &mut FieldEvaluationScratchV1::new(),
            ),
            Err(expected)
        );
    }
}

#[test]
fn whole_field_evidence_kind_must_match_renderer_authority() {
    let geometry = extent(1, 1);
    let source_pixels = [pixel(32, 0, 0, 64)];
    let destination_pixels = [pixel(0, 0, 0, 255)];
    let operation = || FieldOperationV1::PremultipliedSourceOver {
        source: premultiplied_raster(1, geometry, &source_pixels),
        destination: premultiplied_raster(2, geometry, &destination_pixels),
    };

    let host_request = request(
        200,
        geometry,
        dpr(1),
        host_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
        1,
        CarrierIntentV1::Contributes,
        operation(),
    );
    assert_eq!(
        evaluate_whole_field(
            &host_request,
            exact_evidence(1),
            &mut FieldEvaluationScratchV1::new(),
        ),
        Err(FieldEvaluationErrorV1::ExactReferenceCannotProveHostRenderer)
    );

    let reference_request = request(
        201,
        geometry,
        dpr(1),
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
        1,
        CarrierIntentV1::Contributes,
        operation(),
    );
    let observed_pixels =
        evaluate_reference_full(&reference_request, &mut FieldEvaluationScratchV1::new())
            .unwrap()
            .to_vec();
    let prospective = ProspectiveObservedRasterV1::from_host_observation(
        FieldEvidenceIdentityV1::new(2),
        request_digest(&reference_request),
        scene(1),
        FieldOutputCapabilityV1::PremultipliedRgba8V1,
        host_permit(),
        premultiplied_raster(3, geometry, &observed_pixels),
    );
    assert_eq!(
        evaluate_whole_field(
            &reference_request,
            FieldEvidenceV1::ProspectiveObservedWholeRaster(prospective),
            &mut FieldEvaluationScratchV1::new(),
        ),
        Err(FieldEvaluationErrorV1::ProspectiveObservationRequiresHostConformantRenderer)
    );
}

#[test]
fn host_conformant_prospective_observation_binds_request_scene_and_capability() {
    let geometry = extent(2, 1);
    let source_pixels = [
        screen_pixel([128, 0, 0], 0.5),
        screen_pixel([0, 128, 0], 0.25),
    ];
    let backdrop_pixels = [Srgb8::new([20, 30, 40]), Srgb8::new([50, 60, 70])];
    let capability = host_capability(FieldOutputCapabilityV1::OpaqueSrgb8V1);
    let request = request(
        3,
        geometry,
        dpr(1),
        capability,
        9,
        CarrierIntentV1::Contributes,
        FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop {
            source: screen_raster(1, geometry, &source_pixels),
            backdrop: opaque_raster(2, geometry, &backdrop_pixels),
        },
    );

    let mut reference_scratch = FieldEvaluationScratchV1::new();
    let reference = evaluate_reference_full(&request, &mut reference_scratch)
        .unwrap()
        .to_vec();
    let observed = premultiplied_raster(90, geometry, &reference);
    let evidence = FieldEvidenceV1::ProspectiveObservedWholeRaster(
        ProspectiveObservedRasterV1::from_host_observation(
            FieldEvidenceIdentityV1::new(91),
            request_digest(&request),
            scene(9),
            FieldOutputCapabilityV1::OpaqueSrgb8V1,
            host_permit(),
            observed,
        ),
    );

    let certificate =
        evaluate_whole_field(&request, evidence, &mut FieldEvaluationScratchV1::new()).unwrap();
    assert_eq!(
        certificate.evidence_class(),
        FieldEvidenceClassV1::ProspectiveObservedWholeRaster
    );
    assert_eq!(certificate.scene_revision(), scene(9));
    assert_eq!(certificate.render_capability(), capability);
    assert_eq!(certificate.request_digest(), request_digest(&request));
    assert_eq!(
        certificate.operator_kind(),
        FieldOperatorKindV1::EncodedSrgb8ScreenOpaqueBackdropV1
    );
    assert_ne!(
        certificate.output_digest(),
        crate::field_effect::FieldRasterDigestV1::from_bytes_for_test([0; 32])
    );
    assert_eq!(request.geometry(), FieldGeometryV1::new(geometry));
    assert_eq!(request.render_capability(), capability);
    assert_eq!(request.scene_revision(), scene(9));
}

#[test]
fn prospective_observation_rejects_revision_capability_request_and_raster_drift() {
    let geometry = extent(1, 1);
    let source_pixels = [screen_pixel([128, 0, 0], 0.5)];
    let backdrop_pixels = [Srgb8::new([20, 30, 40])];
    let capability = host_capability(FieldOutputCapabilityV1::OpaqueSrgb8V1);
    let request = request(
        4,
        geometry,
        dpr(1),
        capability,
        4,
        CarrierIntentV1::Contributes,
        FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop {
            source: screen_raster(1, geometry, &source_pixels),
            backdrop: opaque_raster(2, geometry, &backdrop_pixels),
        },
    );
    let expected_pixels = evaluate_reference_full(&request, &mut FieldEvaluationScratchV1::new())
        .unwrap()
        .to_vec();
    let expected = premultiplied_raster(3, geometry, &expected_pixels);

    let cases = [
        (
            ProspectiveObservedRasterV1::from_host_observation(
                FieldEvidenceIdentityV1::new(1),
                request_digest(&request),
                scene(5),
                FieldOutputCapabilityV1::OpaqueSrgb8V1,
                host_permit(),
                expected,
            ),
            FieldEvaluationErrorV1::EvidenceSceneRevisionMismatch {
                expected: scene(4),
                actual: scene(5),
            },
        ),
        (
            ProspectiveObservedRasterV1::from_host_observation(
                FieldEvidenceIdentityV1::new(2),
                request_digest(&request),
                scene(4),
                FieldOutputCapabilityV1::OpaqueSrgb8V1,
                other_host_permit(),
                expected,
            ),
            FieldEvaluationErrorV1::EvidenceRenderCapabilityMismatch,
        ),
        (
            ProspectiveObservedRasterV1::from_host_observation(
                FieldEvidenceIdentityV1::new(3),
                crate::field_effect::FieldRequestDigestV1::from_bytes([0xA5; 32]),
                scene(4),
                FieldOutputCapabilityV1::OpaqueSrgb8V1,
                host_permit(),
                expected,
            ),
            FieldEvaluationErrorV1::EvidenceRequestDigestMismatch,
        ),
    ];

    for (observed, expected_error) in cases {
        assert_eq!(
            evaluate_whole_field(
                &request,
                FieldEvidenceV1::ProspectiveObservedWholeRaster(observed),
                &mut FieldEvaluationScratchV1::new(),
            ),
            Err(expected_error)
        );
    }

    let mismatched_pixels = [pixel(255, 255, 255, 255)];
    let mismatched = premultiplied_raster(4, geometry, &mismatched_pixels);
    assert_eq!(
        evaluate_whole_field(
            &request,
            FieldEvidenceV1::ProspectiveObservedWholeRaster(
                ProspectiveObservedRasterV1::from_host_observation(
                    FieldEvidenceIdentityV1::new(4),
                    request_digest(&request),
                    scene(4),
                    FieldOutputCapabilityV1::OpaqueSrgb8V1,
                    host_permit(),
                    mismatched,
                ),
            ),
            &mut FieldEvaluationScratchV1::new(),
        ),
        Err(FieldEvaluationErrorV1::ObservedRasterMismatch { pixel_index: 0 })
    );
}

#[test]
fn gaussian_footprint_and_influence_are_dpr_aware_and_overflow_is_typed() {
    let geometry = extent(9, 9);
    let source_pixels = vec![PremultipliedRgba8V1::TRANSPARENT; 81];
    let make_request = |ratio| {
        let kernel = GaussianKernelV1::canonical_one_css_pixel(ratio);
        request(
            5,
            geometry,
            ratio,
            reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
            1,
            CarrierIntentV1::Present,
            FieldOperationV1::GaussianBlur {
                source: premultiplied_raster(1, geometry, &source_pixels),
                kernel,
                edge_mode: GaussianEdgeModeV1::ClampToEdgeV1,
            },
        )
    };
    let dpr1_request = make_request(dpr(1));
    let dpr4_request = make_request(dpr(4));
    let centre = rect(geometry, 4, 4, 1, 1);
    let dpr1 = influence_for_input(&dpr1_request, centre).unwrap();
    let dpr4 = influence_for_input(&dpr4_request, centre).unwrap();

    assert_eq!(dpr1.exact(), rect(geometry, 3, 3, 3, 3));
    assert_eq!(dpr1.exact(), dpr1.conservative());
    assert_eq!(dpr4.exact(), rect(geometry, 0, 0, 9, 9));
    assert_eq!(dpr1.exact().width(), 3);
    assert_eq!(dpr1.exact().height(), 3);
    assert_ne!(request_digest(&dpr1_request), request_digest(&dpr4_request));

    let near_max = extent(u32::MAX, 1);
    let overflow_rect = rect(near_max, u32::MAX - 1, 0, 1, 1);
    assert_eq!(
        overflow_rect.expanded(2, near_max),
        Err(FieldEvaluationErrorV1::GeometryOverflow)
    );
    assert_eq!(
        GaussianKernelV1::try_new(
            GaussianKernelProfileV1::BinomialGaussianQ32V1,
            u32::MAX,
            dpr(4),
            Vec::new(),
        ),
        Err(FieldEvaluationErrorV1::GeometryOverflow)
    );
    assert_eq!(
        GaussianKernelV1::try_new(
            GaussianKernelProfileV1::BinomialGaussianQ32V1,
            17,
            dpr(1),
            Vec::new(),
        ),
        Err(FieldEvaluationErrorV1::UnsupportedBinomialDeviceRadius {
            actual: 17,
            maximum: 16,
        })
    );
    assert_eq!(
        GaussianKernelV1::try_new(
            GaussianKernelProfileV1::BinomialGaussianQ32V1,
            5,
            dpr(4),
            Vec::new(),
        ),
        Err(FieldEvaluationErrorV1::UnsupportedBinomialDeviceRadius {
            actual: 20,
            maximum: 16,
        })
    );
}

#[test]
fn binomial_profile_rejects_symmetric_normalized_non_binomial_weights() {
    assert_eq!(
        GaussianKernelV1::try_new(
            GaussianKernelProfileV1::BinomialGaussianQ32V1,
            1,
            dpr(1),
            vec![1 << 29, 3 << 30, 1 << 29],
        ),
        Err(FieldEvaluationErrorV1::KernelWeightsDoNotMatchProfile)
    );

    for raw_dpr in 1..=4 {
        let ratio = dpr(raw_dpr);
        let canonical = GaussianKernelV1::canonical_one_css_pixel(ratio);
        let admitted = GaussianKernelV1::try_new(
            canonical.profile(),
            canonical.css_radius_px(),
            ratio,
            canonical.weights_q32().to_vec(),
        )
        .unwrap();
        assert_eq!(admitted, canonical);
    }
    for device_radius in 1..=16_u32 {
        let expected = pascal_q32_row(device_radius);
        for raw_dpr in 1..=4_u8 {
            let divisor = u32::from(raw_dpr);
            if !device_radius.is_multiple_of(divisor) {
                continue;
            }
            let admitted = GaussianKernelV1::try_new(
                GaussianKernelProfileV1::BinomialGaussianQ32V1,
                device_radius / divisor,
                dpr(raw_dpr),
                expected.clone(),
            )
            .unwrap();
            assert_eq!(admitted.weights_q32(), expected);
        }
    }

    let radius_16 = pascal_q32_row(16);
    let mut interior_corruption = radius_16.clone();
    interior_corruption[15] -= 1;
    interior_corruption[17] -= 1;
    interior_corruption[16] += 2;
    assert_eq!(
        GaussianKernelV1::try_new(
            GaussianKernelProfileV1::BinomialGaussianQ32V1,
            16,
            dpr(1),
            interior_corruption,
        ),
        Err(FieldEvaluationErrorV1::KernelWeightsDoNotMatchProfile)
    );
}

fn pascal_q32_row(device_radius: u32) -> Vec<u32> {
    let order = usize::try_from(device_radius * 2).unwrap();
    let scale = 1_u64 << (32 - u32::try_from(order).unwrap());
    let mut coefficients = vec![0_u64; order + 1];
    coefficients[0] = 1;
    for level in 1..=order {
        for index in (1..=level).rev() {
            coefficients[index] += coefficients[index - 1];
        }
    }
    coefficients
        .into_iter()
        .map(|coefficient| u32::try_from(coefficient * scale).unwrap())
        .collect()
}

#[test]
fn unsupported_numeric_profiles_are_rejected_before_evaluation() {
    let geometry = extent(1, 1);
    let source_pixels = [pixel(32, 0, 0, 64)];
    let destination_pixels = [pixel(0, 0, 0, 255)];
    let make_operation = || FieldOperationV1::PremultipliedSourceOver {
        source: premultiplied_raster(1, geometry, &source_pixels),
        destination: premultiplied_raster(2, geometry, &destination_pixels),
    };
    let fixed = (
        FieldRequestIdV1::new(100),
        FieldOperatorInstanceIdV1::new(101),
        FieldGeometryV1::new(geometry),
        dpr(1),
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
        scene(1),
        CarrierIntentV1::Contributes,
    );

    assert!(matches!(
        FieldEvaluationRequestV1::try_new(
            fixed.0,
            fixed.1,
            fixed.2,
            fixed.3,
            FieldWorkingSpaceV1::LinearSrgbQ31PremultipliedV1,
            FieldPrecisionV1::FixedQ32V1,
            FieldQuantizationV1::RoundHalfUpSrgb8V1,
            fixed.4,
            fixed.5,
            fixed.6,
            make_operation(),
        ),
        Err(FieldEvaluationErrorV1::UnsupportedWorkingSpace)
    ));
    assert!(matches!(
        FieldEvaluationRequestV1::try_new(
            fixed.0,
            fixed.1,
            fixed.2,
            fixed.3,
            FieldWorkingSpaceV1::EncodedSrgb8PremultipliedV1,
            FieldPrecisionV1::Binary32V1,
            FieldQuantizationV1::RoundHalfUpSrgb8V1,
            fixed.4,
            fixed.5,
            fixed.6,
            make_operation(),
        ),
        Err(FieldEvaluationErrorV1::UnsupportedPrecision)
    ));
    assert!(matches!(
        FieldEvaluationRequestV1::try_new(
            fixed.0,
            fixed.1,
            fixed.2,
            fixed.3,
            FieldWorkingSpaceV1::EncodedSrgb8PremultipliedV1,
            FieldPrecisionV1::FixedQ32V1,
            FieldQuantizationV1::RoundTiesToEvenSrgb8V1,
            fixed.4,
            fixed.5,
            fixed.6,
            make_operation(),
        ),
        Err(FieldEvaluationErrorV1::UnsupportedQuantization)
    ));
}

#[test]
fn kernel_digest_binds_css_radius_dpr_and_canonical_weights() {
    let geometry = extent(3, 1);
    let source_pixels = [
        PremultipliedRgba8V1::TRANSPARENT,
        pixel(255, 0, 0, 255),
        PremultipliedRgba8V1::TRANSPARENT,
    ];
    let first = GaussianKernelV1::canonical_one_css_pixel(dpr(2));
    let second = GaussianKernelV1::try_new(
        GaussianKernelProfileV1::BinomialGaussianQ32V1,
        2,
        dpr(1),
        vec![1 << 28, 1 << 30, 3 << 29, 1 << 30, 1 << 28],
    )
    .unwrap();
    assert_eq!(first.device_radius_px(), second.device_radius_px());
    assert_eq!(first.weights_q32(), second.weights_q32());
    let make_request = |request_id, ratio, kernel| {
        request(
            request_id,
            geometry,
            ratio,
            reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
            1,
            CarrierIntentV1::Contributes,
            FieldOperationV1::GaussianBlur {
                source: premultiplied_raster(1, geometry, &source_pixels),
                kernel,
                edge_mode: GaussianEdgeModeV1::ClampToEdgeV1,
            },
        )
    };
    let first_request = make_request(6, dpr(2), first);
    let second_request = make_request(6, dpr(1), second);
    assert_ne!(
        request_digest(&first_request),
        request_digest(&second_request)
    );

    let first_certificate = evaluate_whole_field(
        &first_request,
        exact_evidence(1),
        &mut FieldEvaluationScratchV1::new(),
    )
    .unwrap();
    let second_certificate = evaluate_whole_field(
        &second_request,
        exact_evidence(1),
        &mut FieldEvaluationScratchV1::new(),
    )
    .unwrap();
    assert_ne!(first_certificate.digest(), second_certificate.digest());
    assert_ne!(
        first_certificate.kernel_digest(),
        second_certificate.kernel_digest()
    );
}

#[test]
fn incremental_gaussian_update_is_byte_identical_to_full_recompute() {
    let geometry = extent(7, 7);
    let mut before = vec![PremultipliedRgba8V1::TRANSPARENT; 49];
    before[24] = pixel(64, 32, 16, 128);
    let mut after = before.clone();
    after[24] = pixel(128, 64, 32, 192);
    let kernel = GaussianKernelV1::canonical_one_css_pixel(dpr(2));

    let before_request = request(
        7,
        geometry,
        dpr(2),
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
        1,
        CarrierIntentV1::Contributes,
        FieldOperationV1::GaussianBlur {
            source: premultiplied_raster(1, geometry, &before),
            kernel: kernel.clone(),
            edge_mode: GaussianEdgeModeV1::ClampToEdgeV1,
        },
    );
    let after_request = request(
        7,
        geometry,
        dpr(2),
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
        2,
        CarrierIntentV1::Contributes,
        FieldOperationV1::GaussianBlur {
            source: premultiplied_raster(1, geometry, &after),
            kernel,
            edge_mode: GaussianEdgeModeV1::ClampToEdgeV1,
        },
    );

    let mut incremental = FieldEvaluationScratchV1::new();
    evaluate_reference_full(&before_request, &mut incremental).unwrap();
    let dirty = rect(geometry, 3, 3, 1, 1);
    let influence =
        evaluate_reference_incremental(&before_request, &after_request, dirty, &mut incremental)
            .unwrap();
    assert_eq!(
        influence,
        FieldInfluenceV1::new(rect(geometry, 1, 1, 5, 5), rect(geometry, 1, 1, 5, 5))
    );

    let mut full = FieldEvaluationScratchV1::new();
    evaluate_reference_full(&after_request, &mut full).unwrap();
    assert_eq!(incremental.output(), full.output());
}

#[test]
fn scratch_storage_is_caller_owned_and_reused_without_capacity_growth() {
    let geometry = extent(4, 4);
    let source_pixels = vec![pixel(32, 16, 8, 64); 16];
    let request = request(
        8,
        geometry,
        dpr(2),
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
        1,
        CarrierIntentV1::Contributes,
        FieldOperationV1::GaussianBlur {
            source: premultiplied_raster(1, geometry, &source_pixels),
            kernel: GaussianKernelV1::canonical_one_css_pixel(dpr(2)),
            edge_mode: GaussianEdgeModeV1::ClampToEdgeV1,
        },
    );
    let mut scratch = FieldEvaluationScratchV1::new();
    evaluate_reference_full(&request, &mut scratch).unwrap();
    let capacities = scratch.capacity_snapshot_for_test();
    let pointers = scratch.pointer_snapshot_for_test();

    evaluate_reference_full(&request, &mut scratch).unwrap();
    assert_eq!(scratch.capacity_snapshot_for_test(), capacities);
    assert_eq!(scratch.pointer_snapshot_for_test(), pointers);
}

#[test]
fn carrier_presence_contribution_and_variation_fail_closed() {
    let geometry = extent(2, 1);
    let transparent = [screen_pixel([255, 255, 255], 0.0); 2];
    let black = [screen_pixel([0, 0, 0], 1.0); 2];
    let opaque_backdrop = [Srgb8::new([255, 255, 255]); 2];

    let absent = request(
        9,
        geometry,
        dpr(1),
        reference_capability(FieldOutputCapabilityV1::OpaqueSrgb8V1),
        1,
        CarrierIntentV1::Present,
        FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop {
            source: screen_raster(1, geometry, &transparent),
            backdrop: opaque_raster(2, geometry, &opaque_backdrop),
        },
    );
    assert_eq!(
        evaluate_whole_field(
            &absent,
            exact_evidence(1),
            &mut FieldEvaluationScratchV1::new(),
        ),
        Err(FieldEvaluationErrorV1::CarrierAbsent)
    );

    let erased = request(
        10,
        geometry,
        dpr(1),
        reference_capability(FieldOutputCapabilityV1::OpaqueSrgb8V1),
        1,
        CarrierIntentV1::Contributes,
        FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop {
            source: screen_raster(3, geometry, &black),
            backdrop: opaque_raster(4, geometry, &opaque_backdrop),
        },
    );
    assert_eq!(
        evaluate_whole_field(
            &erased,
            exact_evidence(2),
            &mut FieldEvaluationScratchV1::new(),
        ),
        Err(FieldEvaluationErrorV1::CarrierErased)
    );

    let flat_source = [screen_pixel([128, 0, 0], 0.25); 2];
    let flat_backdrop = [Srgb8::new([0, 0, 0]); 2];
    let variation_erased = request(
        11,
        geometry,
        dpr(1),
        reference_capability(FieldOutputCapabilityV1::OpaqueSrgb8V1),
        1,
        CarrierIntentV1::SpatialVariation,
        FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop {
            source: screen_raster(5, geometry, &flat_source),
            backdrop: opaque_raster(6, geometry, &flat_backdrop),
        },
    );
    assert_eq!(
        evaluate_whole_field(
            &variation_erased,
            exact_evidence(3),
            &mut FieldEvaluationScratchV1::new(),
        ),
        Err(FieldEvaluationErrorV1::CarrierVariationErased)
    );
}

#[test]
fn operators_have_separate_identities_and_reference_results() {
    let geometry = extent(1, 1);
    let source_pixels = [pixel(64, 32, 16, 128)];
    let screen_source_pixels = [screen_pixel([128, 64, 32], 0.5)];
    let destination_pixels = [pixel(32, 16, 8, 128)];
    let opaque_backdrop = [Srgb8::new([32, 16, 8])];

    let source_over = request(
        12,
        geometry,
        dpr(1),
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
        1,
        CarrierIntentV1::Contributes,
        FieldOperationV1::PremultipliedSourceOver {
            source: premultiplied_raster(1, geometry, &source_pixels),
            destination: premultiplied_raster(2, geometry, &destination_pixels),
        },
    );
    let screen = request(
        12,
        geometry,
        dpr(1),
        reference_capability(FieldOutputCapabilityV1::OpaqueSrgb8V1),
        1,
        CarrierIntentV1::Contributes,
        FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop {
            source: screen_raster(1, geometry, &screen_source_pixels),
            backdrop: opaque_raster(3, geometry, &opaque_backdrop),
        },
    );
    let lighter = request(
        12,
        geometry,
        dpr(1),
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
        1,
        CarrierIntentV1::Contributes,
        FieldOperationV1::PorterDuffLighter {
            source: premultiplied_raster(1, geometry, &source_pixels),
            destination: premultiplied_raster(2, geometry, &destination_pixels),
        },
    );

    assert_eq!(
        source_over.operator_kind(),
        FieldOperatorKindV1::PremultipliedSourceOverV1
    );
    assert_eq!(
        screen.operator_kind(),
        FieldOperatorKindV1::EncodedSrgb8ScreenOpaqueBackdropV1
    );
    assert_eq!(
        lighter.operator_kind(),
        FieldOperatorKindV1::PorterDuffLighterV1
    );
    assert_ne!(request_digest(&source_over), request_digest(&screen));
    assert_ne!(request_digest(&source_over), request_digest(&lighter));

    let source_over_output =
        evaluate_reference_full(&source_over, &mut FieldEvaluationScratchV1::new())
            .unwrap()
            .to_vec();
    let screen_output = evaluate_reference_full(&screen, &mut FieldEvaluationScratchV1::new())
        .unwrap()
        .to_vec();
    let lighter_output = evaluate_reference_full(&lighter, &mut FieldEvaluationScratchV1::new())
        .unwrap()
        .to_vec();
    assert_ne!(source_over_output, screen_output);
    assert_ne!(source_over_output, lighter_output);
    assert_ne!(screen_output, lighter_output);
}

#[test]
fn encoded_srgb8_screen_preserves_straight_alpha_single_rounding_reference() {
    let geometry = extent(1, 1);
    let capability = reference_capability(FieldOutputCapabilityV1::OpaqueSrgb8V1);

    let seam_source = [screen_pixel([5, 5, 5], 0.122)];
    let seam_backdrop = [Srgb8::new([46, 46, 46])];
    let seam = request(
        120,
        geometry,
        dpr(1),
        capability,
        1,
        CarrierIntentV1::Present,
        FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop {
            source: screen_raster(1, geometry, &seam_source),
            backdrop: opaque_raster(2, geometry, &seam_backdrop),
        },
    );
    assert_eq!(
        evaluate_reference_full(&seam, &mut FieldEvaluationScratchV1::new()).unwrap(),
        &[pixel(46, 46, 46, 255)]
    );

    let fixture_source = [screen_pixel([0xC0, 0xB2, 0xFA], 0.122)];
    let fixture_backdrop = [Srgb8::new([0x10, 0x10, 0x12])];
    let fixture = request(
        121,
        geometry,
        dpr(1),
        capability,
        1,
        CarrierIntentV1::Contributes,
        FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop {
            source: screen_raster(3, geometry, &fixture_source),
            backdrop: opaque_raster(4, geometry, &fixture_backdrop),
        },
    );
    assert_eq!(
        evaluate_reference_full(&fixture, &mut FieldEvaluationScratchV1::new()).unwrap(),
        &[pixel(0x26, 0x24, 0x2E, 255)]
    );
    assert_eq!(
        FieldOpacityV1::try_new(f64::NAN),
        Err(FieldEvaluationErrorV1::NonFiniteOpacity)
    );
    assert_eq!(
        FieldOpacityV1::try_new(1.0 + f64::EPSILON),
        Err(FieldEvaluationErrorV1::OpacityOutsideUnitInterval)
    );
}

#[test]
fn incremental_update_rejects_unrelated_scratch_and_changes_outside_dirty_region() {
    let geometry = extent(5, 5);
    let kernel = GaussianKernelV1::canonical_one_css_pixel(dpr(1));
    let mut baseline_pixels = vec![PremultipliedRgba8V1::TRANSPARENT; 25];
    baseline_pixels[12] = pixel(64, 32, 16, 128);
    let mut outside_dirty_pixels = baseline_pixels.clone();
    outside_dirty_pixels[0] = pixel(32, 16, 8, 64);
    let unrelated_pixels = vec![pixel(8, 4, 2, 16); 25];
    let baseline = request(
        130,
        geometry,
        dpr(1),
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
        1,
        CarrierIntentV1::Present,
        FieldOperationV1::GaussianBlur {
            source: premultiplied_raster(1, geometry, &baseline_pixels),
            kernel: kernel.clone(),
            edge_mode: GaussianEdgeModeV1::ClampToEdgeV1,
        },
    );
    let outside_dirty = request(
        130,
        geometry,
        dpr(1),
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
        2,
        CarrierIntentV1::Present,
        FieldOperationV1::GaussianBlur {
            source: premultiplied_raster(1, geometry, &outside_dirty_pixels),
            kernel: kernel.clone(),
            edge_mode: GaussianEdgeModeV1::ClampToEdgeV1,
        },
    );
    let unrelated = request(
        131,
        geometry,
        dpr(1),
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
        1,
        CarrierIntentV1::Present,
        FieldOperationV1::GaussianBlur {
            source: premultiplied_raster(1, geometry, &unrelated_pixels),
            kernel,
            edge_mode: GaussianEdgeModeV1::ClampToEdgeV1,
        },
    );
    let dirty = rect(geometry, 2, 2, 1, 1);
    let mut scratch = FieldEvaluationScratchV1::new();
    evaluate_reference_full(&baseline, &mut scratch).unwrap();

    assert_eq!(
        evaluate_reference_incremental(&unrelated, &outside_dirty, dirty, &mut scratch),
        Err(FieldEvaluationErrorV1::IncrementalPreviousRequestMismatch)
    );
    assert_eq!(
        evaluate_reference_incremental(&baseline, &outside_dirty, dirty, &mut scratch),
        Err(FieldEvaluationErrorV1::IncrementalChangeOutsideDirtyRegion { pixel_index: 0 })
    );
}

#[test]
fn request_and_replay_bind_the_session_stream_as_well_as_revision() {
    let geometry = extent(1, 1);
    let source_pixels = [pixel(64, 0, 0, 128)];
    let destination_pixels = [pixel(0, 0, 0, 255)];
    let capability = reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1);
    let make_request = |stream, revision| {
        request_at_head(
            140,
            geometry,
            dpr(1),
            capability,
            scene_on(stream, revision),
            CarrierIntentV1::Contributes,
            FieldOperationV1::PremultipliedSourceOver {
                source: premultiplied_raster(1, geometry, &source_pixels),
                destination: premultiplied_raster(2, geometry, &destination_pixels),
            },
        )
    };
    let baseline = make_request(40, 1);
    let stream_drift = make_request(41, 1);
    let revision_drift = make_request(40, 2);
    assert_ne!(request_digest(&baseline), request_digest(&stream_drift));
    assert_ne!(request_digest(&baseline), request_digest(&revision_drift));

    let certificate = evaluate_whole_field(
        &baseline,
        exact_evidence(1),
        &mut FieldEvaluationScratchV1::new(),
    )
    .unwrap();
    assert_eq!(
        verify_certificate_replay(&certificate, &stream_drift),
        Err(FieldCertificateReplayErrorV1::SceneRevision {
            expected: scene_on(40, 1),
            actual: scene_on(41, 1),
        })
    );
}

#[test]
fn certificate_replay_reports_scene_capability_and_request_mismatch_separately() {
    let geometry = extent(1, 1);
    let source_pixels = [pixel(64, 0, 0, 128)];
    let destination_pixels = [pixel(0, 0, 0, 255)];
    let make_request = |request_id, revision, capability| {
        request(
            request_id,
            geometry,
            dpr(1),
            capability,
            revision,
            CarrierIntentV1::Contributes,
            FieldOperationV1::PremultipliedSourceOver {
                source: premultiplied_raster(1, geometry, &source_pixels),
                destination: premultiplied_raster(2, geometry, &destination_pixels),
            },
        )
    };
    let baseline = make_request(
        13,
        1,
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
    );
    let certificate = evaluate_whole_field(
        &baseline,
        exact_evidence(1),
        &mut FieldEvaluationScratchV1::new(),
    )
    .unwrap();
    assert_eq!(verify_certificate_replay(&certificate, &baseline), Ok(()));

    let revision_drift = make_request(
        13,
        2,
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
    );
    assert_eq!(
        verify_certificate_replay(&certificate, &revision_drift),
        Err(FieldCertificateReplayErrorV1::SceneRevision {
            expected: scene(1),
            actual: scene(2),
        })
    );

    let capability_drift = make_request(
        13,
        1,
        host_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
    );
    assert_eq!(
        verify_certificate_replay(&certificate, &capability_drift),
        Err(FieldCertificateReplayErrorV1::RenderCapability)
    );

    let request_drift = make_request(
        14,
        1,
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
    );
    assert_eq!(
        verify_certificate_replay(&certificate, &request_drift),
        Err(FieldCertificateReplayErrorV1::Request)
    );
}

#[test]
fn output_footprint_is_exact_for_finite_kernel_and_pointwise_operators() {
    let geometry = extent(7, 7);
    let source_pixels = vec![pixel(16, 8, 4, 32); 49];
    let blur = request(
        15,
        geometry,
        dpr(2),
        reference_capability(FieldOutputCapabilityV1::PremultipliedRgba8V1),
        1,
        CarrierIntentV1::Contributes,
        FieldOperationV1::GaussianBlur {
            source: premultiplied_raster(1, geometry, &source_pixels),
            kernel: GaussianKernelV1::canonical_one_css_pixel(dpr(2)),
            edge_mode: GaussianEdgeModeV1::ClampToEdgeV1,
        },
    );
    let output = rect(geometry, 3, 3, 1, 1);
    let footprint = footprint_for_output(&blur, output).unwrap();
    assert_eq!(footprint.output(), output);
    assert_eq!(footprint.exact_input(), rect(geometry, 1, 1, 5, 5));
    assert_eq!(footprint.exact_input(), footprint.conservative_input());
}
