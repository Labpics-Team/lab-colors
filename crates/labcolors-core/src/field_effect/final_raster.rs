//! Полный окончательный растр. Его область не требует происхождения каждого
//! carrier и не утверждает соответствие последующего host rendering.
use super::*;

/// Политика опубликованной конвенции для полного окончательного растра.
/// Locked tuple не изменяется ни одним из этих режимов.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FinalRasterCleanlinessRequirementV1 {
    Off = 0,
    MandatoryExactNominalSrgb8V2 = 1,
    ReportOnlyExactNominalSrgb8V2 = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalRasterAssessmentVerdictV1 {
    NotApplied,
    Pass,
    Violation,
}

/// Первый подтверждённый нарушающий пиксель; индексы row-major.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinalRasterCleanlinessViolationV1 {
    pixel_index: usize,
    composite: [u8; 3],
    rejected_blue_interval: [u8; 2],
}
impl FinalRasterCleanlinessViolationV1 {
    pub(crate) const fn new(
        pixel_index: usize,
        composite: [u8; 3],
        rejected_blue_interval: [u8; 2],
    ) -> Self {
        Self {
            pixel_index,
            composite,
            rejected_blue_interval,
        }
    }
    #[must_use]
    pub const fn pixel_index(self) -> usize {
        self.pixel_index
    }
    #[must_use]
    pub const fn composite(self) -> [u8; 3] {
        self.composite
    }
    #[must_use]
    pub const fn rejected_blue_interval(self) -> [u8; 2] {
        self.rejected_blue_interval
    }
}

/// Итог полного fold всех final pixels, а не point certificate или capability.
/// При нарушениях retained результат содержит точный count и первый witness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinalRasterCleanlinessAssessmentV1 {
    requirement: FinalRasterCleanlinessRequirementV1,
    checked_pixels: usize,
    violation_count: usize,
    first_violation: Option<FinalRasterCleanlinessViolationV1>,
}
impl FinalRasterCleanlinessAssessmentV1 {
    pub(crate) const fn off() -> Self {
        Self {
            requirement: FinalRasterCleanlinessRequirementV1::Off,
            checked_pixels: 0,
            violation_count: 0,
            first_violation: None,
        }
    }
    pub(crate) fn assessed(
        requirement: FinalRasterCleanlinessRequirementV1,
        checked_pixels: usize,
        violation_count: usize,
        first_violation: Option<FinalRasterCleanlinessViolationV1>,
    ) -> Result<Self, ()> {
        if requirement == FinalRasterCleanlinessRequirementV1::Off
            || checked_pixels == 0
            || checked_pixels > MAX_FINAL_RASTER_PIXELS_V1
            || violation_count > checked_pixels
            || (violation_count == 0) != first_violation.is_none()
            || first_violation.is_some_and(|first| first.pixel_index >= checked_pixels)
        {
            return Err(());
        }
        Ok(Self {
            requirement,
            checked_pixels,
            violation_count,
            first_violation,
        })
    }
    #[must_use]
    pub const fn requirement(self) -> FinalRasterCleanlinessRequirementV1 {
        self.requirement
    }
    #[must_use]
    pub const fn profile_release(self) -> Option<[u8; 32]> {
        match self.requirement {
            FinalRasterCleanlinessRequirementV1::Off => None,
            _ => Some(crate::clean_set::EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2),
        }
    }
    #[must_use]
    pub const fn checked_pixels(self) -> usize {
        self.checked_pixels
    }
    #[must_use]
    pub const fn violation_count(self) -> usize {
        self.violation_count
    }
    #[must_use]
    pub const fn first_violation(self) -> Option<FinalRasterCleanlinessViolationV1> {
        self.first_violation
    }
    #[must_use]
    pub const fn verdict(self) -> FinalRasterAssessmentVerdictV1 {
        if matches!(self.requirement, FinalRasterCleanlinessRequirementV1::Off) {
            FinalRasterAssessmentVerdictV1::NotApplied
        } else if self.violation_count == 0 {
            FinalRasterAssessmentVerdictV1::Pass
        } else {
            FinalRasterAssessmentVerdictV1::Violation
        }
    }
}

/// Предел публичного locked raster: 65536 device pixels.
/// Два входа 8N, scratch 36N, retained output 4N, RGB copy 3N.
/// Два поколения и prospective work остаются ниже 8 MiB полезных bytes.
pub(crate) const MAX_FINAL_RASTER_PIXELS_V1: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FinalRasterInputErrorV1 {
    Extent,
    DevicePixelRatio,
    Length,
    PremultipliedPixel,
    ResourceExhausted,
    PortBinding,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FieldInputPixelsV1 {
    pub(crate) port: FieldInputPortIdV1,
    pub(crate) pixels: Box<[PremultipliedRgba8V1]>,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FieldObservedPayloadV1 {
    pub(crate) extent: FieldExtentV1,
    pub(crate) dpr: DevicePixelRatioV1,
    pub(crate) context: [u8; 32],
    pub(crate) inputs: [FieldInputPixelsV1; 2],
}
impl FieldObservedPayloadV1 {
    pub(crate) fn try_new(
        extent: FieldExtentV1,
        dpr: u8,
        context: [u8; 32],
        source_port: u64,
        source: &[[u8; 4]],
        destination_port: u64,
        destination: &[[u8; 4]],
    ) -> Result<Self, FinalRasterInputErrorV1> {
        let count = extent
            .pixel_count()
            .map_err(|_| FinalRasterInputErrorV1::Extent)?;
        if count > MAX_FINAL_RASTER_PIXELS_V1 {
            return Err(FinalRasterInputErrorV1::ResourceExhausted);
        }
        let dpr = match dpr {
            1 => DevicePixelRatioV1::One,
            2 => DevicePixelRatioV1::Two,
            3 => DevicePixelRatioV1::Three,
            4 => DevicePixelRatioV1::Four,
            _ => return Err(FinalRasterInputErrorV1::DevicePixelRatio),
        };
        if source.len() != count || destination.len() != count {
            return Err(FinalRasterInputErrorV1::Length);
        }
        if source_port == destination_port {
            return Err(FinalRasterInputErrorV1::PortBinding);
        }
        fn pixels(
            values: &[[u8; 4]],
        ) -> Result<Box<[PremultipliedRgba8V1]>, FinalRasterInputErrorV1> {
            let mut result = Vec::new();
            result
                .try_reserve_exact(values.len())
                .map_err(|_| FinalRasterInputErrorV1::ResourceExhausted)?;
            for &pixel in values {
                result.push(
                    PremultipliedRgba8V1::try_new(pixel)
                        .map_err(|_| FinalRasterInputErrorV1::PremultipliedPixel)?,
                );
            }
            Ok(result.into_boxed_slice())
        }
        let mut inputs = [
            FieldInputPixelsV1 {
                port: FieldInputPortIdV1::new(source_port),
                pixels: pixels(source)?,
            },
            FieldInputPixelsV1 {
                port: FieldInputPortIdV1::new(destination_port),
                pixels: pixels(destination)?,
            },
        ];
        inputs.sort_unstable_by_key(|input| input.port);
        Ok(Self {
            extent,
            dpr,
            context,
            inputs,
        })
    }
    pub(crate) fn raster(
        &self,
        port: FieldInputPortIdV1,
    ) -> Result<FieldRasterViewV1<'_>, FieldEvaluationErrorV1> {
        let pixels = self
            .inputs
            .iter()
            .find(|input| input.port == port)
            .ok_or(FieldEvaluationErrorV1::ExtentMismatch)?;
        FieldRasterViewV1::try_new(
            FieldRasterIdentityV1::new(port.value()),
            self.extent,
            &pixels.pixels,
        )
    }
}

/// Закрытый production mint для exact reference конечных opaque bytes.
/// В отличие от carrier proof, он проверяет полноту и каждый terminal alpha.
/// Caller-owned host-renderer, samples, averages и precomputed bytes не входят.
pub(crate) fn evaluate_final_opaque_reference(
    request: &FieldEvaluationRequestV1<'_>,
    evidence_identity: FieldEvidenceIdentityV1,
    scratch: &mut FieldEvaluationScratchV1,
) -> Result<FieldWholeRasterCertificateV1, FieldEvaluationErrorV1> {
    let evidence = FieldEvidenceV1::ExactReferenceWholeRaster {
        identity: evidence_identity,
    };
    admit_proof_evidence(request, evidence)?;
    evaluate_reference_full(request, scratch)?;
    if let Some(pixel_index) = scratch.output.iter().position(|pixel| pixel.alpha() != 255) {
        return Err(FieldEvaluationErrorV1::ObservedRasterMismatch { pixel_index });
    }
    let request_digest = request_digest(request);
    let output_digest = raster_digest(request.geometry().extent(), &scratch.output);
    let kernel_digest = kernel_digest(&request.operation);
    let evidence_class = evidence.class();
    let digest = certificate_digest(
        request_digest,
        output_digest,
        kernel_digest,
        evidence_identity,
        evidence_class,
        request,
        evidence,
    );
    Ok(FieldWholeRasterCertificateV1 {
        request_digest,
        output_digest,
        kernel_digest,
        evidence_identity,
        evidence_class,
        operator_kind: request.operator_kind(),
        scene_revision: request.scene_revision,
        render_capability: request.render_capability(),
        digest,
    })
}
