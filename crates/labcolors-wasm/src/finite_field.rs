//! WASM передаёт только complete final bytes после current Core admission.
use super::*;
use labcolors_core::program_wire::{
    FinalRasterAssessmentVerdictV1, FinalRasterCleanlinessAssessmentV1,
    FinalRasterCleanlinessRequirementV1, MAX_FINAL_RASTER_PIXELS_V1,
    ProgramFinalRasterErrorV1 as E, ProgramFinalRasterInputV1, final_raster_source_over_wire_v1,
};

fn code(error: E) -> &'static str {
    match error {
        E::InvalidExtent => "program_field_extent",
        E::InvalidDevicePixelRatio => "program_field_dpr",
        E::InvalidLength => "program_field_length",
        E::InvalidPremultipliedPixel => "program_field_premultiplied",
        E::PortBinding => "program_field_ports",
        E::ResourceExhausted => "program_resource_exhausted",
        E::Runtime(labcolors_core::program_wire::ProgramRuntimeErrorV1::ResourceExhausted) => {
            "program_resource_exhausted"
        }
        E::Runtime(_) => "program_field_update",
        E::Unsupported => "program_field_unsupported",
        E::TransparentFinalPixel { .. } => "program_field_transparent",
        E::ConventionRejected { .. } => "program_field_convention",
        E::TechnicalQuality => "program_field_tq",
        E::CleanConvention => "program_field_cc",
        E::Stale => "program_field_stale",
        E::ProgramBinding => "program_field_program",
        E::OutputBinding => "program_field_output",
        E::ContextBinding => "program_field_context",
        _ => "program_field_internal",
    }
}
fn error(error: E, operation: &str) -> JsValue {
    let object: JsValue = program_error("Final raster was refused", code(error), operation).into();
    match error {
        E::TransparentFinalPixel { pixel_index } | E::ConventionRejected { pixel_index, .. } => {
            let _ =
                js_sys::Reflect::set(&object, &"pixelIndex".into(), &(pixel_index as f64).into());
        }
        _ => {}
    }
    if let E::ConventionRejected {
        composite,
        rejected_blue_interval,
        ..
    } = error
    {
        let _ = js_sys::Reflect::set(
            &object,
            &"composite".into(),
            &js_sys::Uint8Array::from(composite.as_slice()).into(),
        );
        let _ = js_sys::Reflect::set(
            &object,
            &"rejectedBlueInterval".into(),
            &js_sys::Uint8Array::from(rejected_blue_interval.as_slice()).into(),
        );
    }
    object
}
fn fixed<const N: usize>(bytes: &[u8], operation: &str) -> Result<[u8; N], JsValue> {
    bytes
        .try_into()
        .map_err(|_| error(E::InvalidLength, operation))
}
fn rgba(bytes: &[u8], count: usize) -> Result<Vec<[u8; 4]>, JsValue> {
    if bytes.len() != count * 4 {
        return Err(error(E::InvalidLength, "updateFinalRaster"));
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(count)
        .map_err(|_| error(E::ResourceExhausted, "updateFinalRaster"))?;
    for pixel in bytes.chunks_exact(4) {
        result.push([pixel[0], pixel[1], pixel[2], pixel[3]]);
    }
    Ok(result)
}

#[wasm_bindgen(js_name = finalRasterSourceOverWire)]
pub fn final_raster_wire(
    #[wasm_bindgen(unchecked_param_type = "bigint")] source: JsValue,
    #[wasm_bindgen(unchecked_param_type = "bigint")] destination: JsValue,
    #[wasm_bindgen(unchecked_param_type = "bigint")] operator: JsValue,
    #[wasm_bindgen(unchecked_param_type = "number")] output: JsValue,
    #[wasm_bindgen(
        unchecked_param_type = "\"off\" | \"lint-exact-nominal-srgb8-v2\" | \"mandatory-exact-nominal-srgb8-v2\""
    )]
    cleanliness: JsValue,
) -> Result<Vec<u8>, JsValue> {
    let cleanliness = match cleanliness.as_string().as_deref() {
        Some("off") => FinalRasterCleanlinessRequirementV1::Off,
        Some("lint-exact-nominal-srgb8-v2") => {
            FinalRasterCleanlinessRequirementV1::ReportOnlyExactNominalSrgb8V2
        }
        Some("mandatory-exact-nominal-srgb8-v2") => {
            FinalRasterCleanlinessRequirementV1::MandatoryExactNominalSrgb8V2
        }
        _ => return Err(error(E::Unsupported, "finalRasterSourceOverWire")),
    };
    let invalid = || error(E::PortBinding, "finalRasterSourceOverWire");
    let source = u64::try_from(source).map_err(|_| invalid())?;
    let destination = u64::try_from(destination).map_err(|_| invalid())?;
    let operator = u64::try_from(operator).map_err(|_| invalid())?;
    let output = checked_u32(output).ok_or_else(invalid)?;
    Ok(
        final_raster_source_over_wire_v1(source, destination, operator, output, cleanliness)
            .to_vec(),
    )
}
/// Только owned-проекция значений Core; classifier и правила policy здесь не повторяются.
fn raster_cleanliness_projection(
    assessment: FinalRasterCleanlinessAssessmentV1,
) -> Result<JsValue, JsValue> {
    let resource = || error(E::ResourceExhausted, "consumeFinalRaster");
    let checked = u32::try_from(assessment.checked_pixels()).map_err(|_| resource())?;
    let violations = u32::try_from(assessment.violation_count()).map_err(|_| resource())?;
    let mode = match assessment.requirement() {
        FinalRasterCleanlinessRequirementV1::Off => "off",
        FinalRasterCleanlinessRequirementV1::ReportOnlyExactNominalSrgb8V2 => "lint",
        FinalRasterCleanlinessRequirementV1::MandatoryExactNominalSrgb8V2 => "mandatory",
    };
    let verdict = match assessment.verdict() {
        FinalRasterAssessmentVerdictV1::NotApplied => "not-applied",
        FinalRasterAssessmentVerdictV1::Pass => "pass",
        FinalRasterAssessmentVerdictV1::Violation => "violation",
    };
    let first = if let Some(first) = assessment.first_violation() {
        let index = u32::try_from(first.pixel_index()).map_err(|_| resource())?;
        let [red, green, blue] = first.composite();
        let [lower, upper] = first.rejected_blue_interval();
        report_final_raster_violation(index, red, green, blue, lower, upper)
            .map_err(|_| resource())?
    } else {
        JsValue::NULL
    };
    let release = assessment.profile_release();
    let profile = if release.is_some() {
        JsValue::from_str("ExactNominalSrgb8CleanSetV2")
    } else {
        JsValue::NULL
    };
    report_final_raster_cleanliness(
        mode,
        verdict,
        checked,
        violations,
        &first,
        &profile,
        release.as_ref().map_or(&[], |bytes| bytes.as_slice()),
    )
    .map_err(|_| resource())
}

#[wasm_bindgen]
impl ProgramRuntime {
    #[wasm_bindgen(js_name = contentIdentity)]
    pub fn final_raster_content_identity(&self) -> Vec<u8> {
        self.inner.content_identity().to_vec()
    }

    #[wasm_bindgen(js_name = updateFinalRaster)]
    #[allow(clippy::too_many_arguments)]
    pub fn update_final_raster(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        #[wasm_bindgen(unchecked_param_type = "number")] width: JsValue,
        #[wasm_bindgen(unchecked_param_type = "number")] height: JsValue,
        #[wasm_bindgen(unchecked_param_type = "number")] dpr: JsValue,
        context: &[u8],
        #[wasm_bindgen(unchecked_param_type = "bigint")] source_port: JsValue,
        source: &[u8],
        #[wasm_bindgen(unchecked_param_type = "bigint")] destination_port: JsValue,
        destination: &[u8],
    ) -> Result<ProgramSnapshot, JsValue> {
        let invalid = || error(E::InvalidExtent, "updateFinalRaster");
        let revision = u64::try_from(revision).map_err(|_| error(E::Stale, "updateFinalRaster"))?;
        let width = checked_u32(width).ok_or_else(invalid)?;
        let height = checked_u32(height).ok_or_else(invalid)?;
        let dpr = checked_u32(dpr)
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| error(E::InvalidDevicePixelRatio, "updateFinalRaster"))?;
        let count = usize::try_from(u64::from(width) * u64::from(height)).map_err(|_| invalid())?;
        if count == 0 {
            return Err(invalid());
        }
        if count > MAX_FINAL_RASTER_PIXELS_V1 {
            return Err(error(E::ResourceExhausted, "updateFinalRaster"));
        }
        let context = fixed(context, "updateFinalRaster")?;
        let source_port =
            u64::try_from(source_port).map_err(|_| error(E::PortBinding, "updateFinalRaster"))?;
        let destination_port = u64::try_from(destination_port)
            .map_err(|_| error(E::PortBinding, "updateFinalRaster"))?;
        let source = rgba(source, count)?;
        let destination = rgba(destination, count)?;
        let input = ProgramFinalRasterInputV1::try_new(
            width,
            height,
            dpr,
            context,
            source_port,
            &source,
            destination_port,
            &destination,
        )
        .map_err(|cause| error(cause, "updateFinalRaster"))?;
        self.inner
            .update_final_raster(revision, &input)
            .map(|inner| ProgramSnapshot { inner })
            .map_err(|cause| error(cause, "updateFinalRaster"))
    }

    #[wasm_bindgen(js_name = finalRasterRefusal)]
    pub fn final_raster_refusal(&self) -> JsValue {
        self.inner
            .final_raster_refusal()
            .map_or(JsValue::NULL, |cause| error(cause, "updateFinalRaster"))
    }

    /// Callback синхронный; boolean подтверждает приём bytes, не renderer conformance.
    #[wasm_bindgen(js_name = consumeFinalRaster)]
    pub fn consume_final_raster(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        content: &[u8],
        #[wasm_bindgen(unchecked_param_type = "number")] output: JsValue,
        context: &[u8],
        #[wasm_bindgen(unchecked_param_type = "FinalRasterConsumerV1")] consumer: js_sys::Function,
    ) -> Result<(), JsValue> {
        let revision =
            u64::try_from(revision).map_err(|_| error(E::Stale, "consumeFinalRaster"))?;
        let output =
            checked_u32(output).ok_or_else(|| error(E::OutputBinding, "consumeFinalRaster"))?;
        let content = fixed(content, "consumeFinalRaster")?;
        let context = fixed(context, "consumeFinalRaster")?;
        let raster = self
            .inner
            .current_final_raster(content, output, revision, context)
            .map_err(|cause| error(cause, "consumeFinalRaster"))?;
        let mut rgb = Vec::new();
        rgb.try_reserve_exact(raster.pixel_count() * 3)
            .map_err(|_| error(E::ResourceExhausted, "consumeFinalRaster"))?;
        for pixel in raster.pixels() {
            rgb.extend_from_slice(&pixel);
        }
        let cleanliness = raster_cleanliness_projection(raster.cleanliness_assessment())?;
        let accepted = consume_final_raster_rgb(
            &rgb,
            raster.width(),
            raster.height(),
            raster.device_pixel_ratio(),
            raster.revision(),
            raster.output_slot(),
            &cleanliness,
            &consumer,
        )?;
        if !accepted {
            return Err(program_error(
                "Final raster consumer refused bytes",
                "program_field_consumer_rejected",
                "consumeFinalRaster",
            )
            .into());
        }
        Ok(())
    }
}

#[wasm_bindgen(typescript_custom_section)]
const FINAL_RASTER_TYPES: &str = r#"
/** Полные immutable final RGB8 bytes; область относится к точному растру Core. */
export interface FinalRasterBytesV1 {
  readonly scope: "ModeledFinalRasterV1";
  readonly width: number;
  readonly height: number;
  readonly devicePixelRatio: number;
  readonly revision: bigint;
  readonly outputSlot: number;
  readonly pixels: Uint8Array;
  readonly cleanliness: FinalRasterCleanlinessAssessmentV1;
}
export interface FinalRasterCleanlinessViolationV1 {
  readonly pixelIndex: number;
  readonly composite: readonly [number, number, number];
  readonly rejectedBlueInterval: readonly [number, number];
}
/** Полный count подтверждённых нарушений и первый witness; человеческое восприятие не оценивается. */
export type FinalRasterCleanlinessAssessmentV1 = Readonly<{
  mode: "off"; verdict: "not-applied"; profile: null; profileRelease: null;
  checkedPixelCount: 0; violationCount: 0; firstViolation: null;
}> | Readonly<{
  mode: "lint"; verdict: "pass" | "violation";
  profile: "ExactNominalSrgb8CleanSetV2"; profileRelease: Uint8Array;
  checkedPixelCount: number; violationCount: number;
  firstViolation: FinalRasterCleanlinessViolationV1 | null;
}> | Readonly<{
  mode: "mandatory"; verdict: "pass";
  profile: "ExactNominalSrgb8CleanSetV2"; profileRelease: Uint8Array;
  checkedPixelCount: number; violationCount: 0; firstViolation: null;
}>;
/** Синхронное true подтверждает приём bytes; host/human conformance не выводится. */
export type FinalRasterConsumerV1 = (raster: FinalRasterBytesV1) => boolean;
"#;
