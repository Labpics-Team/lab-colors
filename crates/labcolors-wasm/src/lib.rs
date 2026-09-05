//! `@labpics/colors` — терминальная WASM-граница C7c.
//!
//! Один runtime-root: canonical Program wire → ProgramRuntime → ProgramSnapshot.
//! Legacy recipe engine (`LabColors.resolveTheme/loadConfig`) удалён. DOM/CSS
//! effects принадлежат npm-приложению, не WASM.

mod error;
mod terminal_projection;

use wasm_bindgen::prelude::*;

use crate::error::BindingError;

#[wasm_bindgen(typescript_custom_section)]
const TERMINAL_CAPABILITY_TYPES: &str = r#"
/** Proof-capable V2 site. Empty arrays explicitly mean no admitted evidence. */
export interface NumericalCapabilitySiteV2 {
  readonly siteId: string;
  readonly stableOutcomes: ReadonlyArray<string>;
  readonly compatibilityReleases: ReadonlyArray<string>;
  readonly evidenceClasses: ReadonlyArray<string>;
  readonly artifactIds: ReadonlyArray<string>;
  readonly boundIds: ReadonlyArray<string>;
  readonly proofIds: ReadonlyArray<string>;
  readonly runtimeAttestations: ReadonlyArray<string>;
}

/** Proof-capable numerical capability manifest. */
export interface NumericalCapabilityManifestV2 {
  readonly schemaVersion: 2;
  readonly coverage: string;
  readonly sites: ReadonlyArray<NumericalCapabilitySiteV2>;
  readonly checksum: string;
}

export type Wcag22CriterionV1 =
  | "sc-1.4.3-text-default"
  | "sc-1.4.3-text-large-scale"
  | "sc-1.4.11-ui-component-or-state"
  | "sc-1.4.11-graphical-object";
export type Wcag22DecisionV1 = "pass" | "fail";
export interface Wcag22Q55BoundsV1 {
  /** Decimal u64 string: Q55 values exceed JavaScript's safe integer range. */
  readonly lower: string;
  readonly upper: string;
}
export interface Wcag22AssessmentV1 {
  readonly kind: "evaluated";
  readonly profileId: "wcag22-srgb8-contrast-v1";
  readonly criterion: Wcag22CriterionV1;
  readonly foreground: string;
  readonly background: string;
  readonly foregroundLuminanceQ55: Wcag22Q55BoundsV1;
  readonly backgroundLuminanceQ55: Wcag22Q55BoundsV1;
  readonly q55Scale: string;
  readonly decision: Wcag22DecisionV1;
  readonly evidence: {
    readonly kind: "canonical-finite-bounded";
    readonly artifactId: "wcag22-srgb8-luminance-q55-v1";
    readonly artifactSha256: string;
    readonly boundId: "wcag22-srgb8-outward-q55-v1";
    readonly proofId: "wcag22-srgb8-full-domain-q55-v1";
    readonly proofSha256: string;
    readonly proofPayloadSha256: string;
    readonly generatorSha256: string;
    readonly verifierSha256: string;
    readonly profileChecksum: string;
    readonly profileSha256: string;
  };
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "NumericalCapabilityManifestV2")]
    pub type JsNumericalCapabilityManifestV2;

    #[wasm_bindgen(typescript_type = "Wcag22AssessmentV1")]
    pub type JsWcag22AssessmentV1;
}

fn to_js_error(error: BindingError) -> JsError {
    JsError::new(&error.to_string())
}

#[derive(Clone, Copy)]
enum ProgramOperation {
    CompileProgramWire,
    UpdateObserved,
    UpdateUnknown,
}

impl ProgramOperation {
    const fn key(self) -> &'static str {
        match self {
            Self::CompileProgramWire => "compileProgramWire",
            Self::UpdateObserved => "updateObserved",
            Self::UpdateUnknown => "updateUnknown",
        }
    }
}

fn to_program_js_error(
    error: labcolors_core::program_wire::ProgramRuntimeErrorV1,
    operation: ProgramOperation,
) -> JsValue {
    use labcolors_core::program_wire::ProgramRuntimeErrorV1 as E;
    let code = match error {
        E::Wire => "program_wire",
        E::Compile => "program_compile",
        E::FamilyArtifactsRequired => "program_family_artifacts_required",
        E::Instantiate => "program_instantiate",
        E::Update => "program_update",
        _ => "program_runtime",
    };
    let error = js_sys::Error::new("Program runtime operation failed");
    let code_set = js_sys::Reflect::set(
        error.as_ref(),
        &JsValue::from_str("code"),
        &JsValue::from_str(code),
    );
    let operation_set = js_sys::Reflect::set(
        error.as_ref(),
        &JsValue::from_str("operation"),
        &JsValue::from_str(operation.key()),
    );
    if !matches!(code_set, Ok(true)) || !matches!(operation_set, Ok(true)) {
        return js_sys::Error::new("Program error projection failed").into();
    }
    error.into()
}

/// Единственный публичный манифест численных возможностей.
#[wasm_bindgen(js_name = numericalCapabilityManifest)]
pub fn numerical_capability_manifest() -> Result<JsNumericalCapabilityManifestV2, JsError> {
    let json = terminal_projection::capability_manifest_json();
    js_sys::JSON::parse(&json)
        .map(JsValue::unchecked_into)
        .map_err(|_| {
            to_js_error(BindingError::Internal {
                reason: "capability manifest не распарсился как JSON".to_string(),
            })
        })
}

/// Точная оценка WCAG 2.2 одной финальной пары sRGB8.
#[wasm_bindgen(js_name = evaluateWcag22)]
pub fn evaluate_wcag22(
    foreground_hex: &str,
    background_hex: &str,
    criterion: &str,
) -> Result<JsWcag22AssessmentV1, JsError> {
    use labcolors_core::wcag22::Wcag22CriterionV1 as C;
    let criterion = C::parse(criterion).ok_or_else(|| {
        to_js_error(BindingError::UnknownWcag22Criterion {
            requested: criterion.to_string(),
        })
    })?;
    let assessment =
        labcolors_core::wcag22::evaluate_wcag22_hex(foreground_hex, background_hex, criterion)
            .map_err(|error| {
                use labcolors_core::wcag22::Wcag22EvaluationErrorV1 as E;
                to_js_error(match error {
                    E::InvalidSrgb8 { field, reason } => BindingError::InvalidColor {
                        reason: format!("{field}: {reason}"),
                    },
                    other => BindingError::Internal {
                        reason: other.to_string(),
                    },
                })
            })?;
    let json = terminal_projection::wcag22_json(&assessment).map_err(to_js_error)?;
    js_sys::JSON::parse(&json)
        .map(JsValue::unchecked_into)
        .map_err(|_| {
            to_js_error(BindingError::Internal {
                reason: "WCAG22 projection не распарсился как JSON".to_string(),
            })
        })
}

/// Публичный runtime одного скомпилированного Program.
#[wasm_bindgen]
pub struct ProgramRuntime {
    inner: labcolors_core::program_wire::ProgramSessionV1,
}

/// Owned snapshot одного Program update.
#[wasm_bindgen]
pub struct ProgramSnapshot {
    inner: labcolors_core::program_wire::ProgramSnapshotV1,
}

#[wasm_bindgen]
impl ProgramSnapshot {
    /// Stable lifecycle key: waiting|ready|stale|failed.
    #[wasm_bindgen(getter)]
    pub fn state(&self) -> String {
        use labcolors_core::program_wire::ProgramSnapshotStateV1 as S;
        match self.inner.state() {
            S::Waiting => "waiting",
            S::Ready => "ready",
            S::Stale => "stale",
            S::Failed => "failed",
            _ => "unknown",
        }
        .to_string()
    }

    #[wasm_bindgen(js_name = outputCount)]
    pub fn output_count(&self) -> usize {
        self.inner.outputs().len()
    }

    #[wasm_bindgen(js_name = outputSlot)]
    pub fn output_slot(&self, index: usize) -> Result<u32, JsError> {
        self.inner
            .outputs()
            .get(index)
            .map(|output| output.slot())
            .ok_or_else(|| {
                to_js_error(BindingError::Internal {
                    reason: format!("program output index {index} is out of bounds"),
                })
            })
    }

    #[wasm_bindgen(js_name = outputRgb)]
    pub fn output_rgb(&self, index: usize) -> Result<Box<[u8]>, JsError> {
        self.inner
            .outputs()
            .get(index)
            .map(|output| output.source().bytes().to_vec().into_boxed_slice())
            .ok_or_else(|| {
                to_js_error(BindingError::Internal {
                    reason: format!("program output index {index} is out of bounds"),
                })
            })
    }

    #[wasm_bindgen(js_name = outputOpacity)]
    pub fn output_opacity(&self, index: usize) -> Result<f64, JsError> {
        self.inner
            .outputs()
            .get(index)
            .map(|output| output.opacity())
            .ok_or_else(|| {
                to_js_error(BindingError::Internal {
                    reason: format!("program output index {index} is out of bounds"),
                })
            })
    }
}

/// Компилирует canonical Program wire bytes и создаёт одну runtime Session.
#[wasm_bindgen(js_name = compileProgramWire)]
pub fn compile_program_wire(bytes: &[u8], stream_id: u32) -> Result<ProgramRuntime, JsValue> {
    let compiled = labcolors_core::program_wire::compile_program_wire_v1(bytes)
        .map_err(|error| to_program_js_error(error, ProgramOperation::CompileProgramWire))?;
    let session = compiled
        .instantiate(stream_id)
        .map_err(|error| to_program_js_error(error, ProgramOperation::CompileProgramWire))?;
    Ok(ProgramRuntime { inner: session })
}

#[wasm_bindgen]
impl ProgramRuntime {
    /// Атомарный observed update. `surfaces` — row-major
    /// `scenario_ids.len() × surface_count × 3` bytes.
    #[wasm_bindgen(js_name = updateObserved)]
    pub fn update_observed(
        &mut self,
        revision: u64,
        scenario_ids: &[u32],
        surfaces: &[u8],
        surface_count: usize,
    ) -> Result<ProgramSnapshot, JsValue> {
        use labcolors_core::program_wire::ProgramRuntimeErrorV1 as E;
        let row_bytes = surface_count
            .checked_mul(3)
            .ok_or_else(|| to_program_js_error(E::Update, ProgramOperation::UpdateObserved))?;
        let expected = scenario_ids
            .len()
            .checked_mul(row_bytes)
            .ok_or_else(|| to_program_js_error(E::Update, ProgramOperation::UpdateObserved))?;
        if expected != surfaces.len() {
            return Err(to_program_js_error(
                E::Update,
                ProgramOperation::UpdateObserved,
            ));
        }
        let mut scenarios = Vec::new();
        scenarios
            .try_reserve_exact(scenario_ids.len())
            .map_err(|_| to_program_js_error(E::Update, ProgramOperation::UpdateObserved))?;
        for (row, scenario_id) in scenario_ids.iter().copied().enumerate() {
            let start = row * row_bytes;
            let mut values = Vec::new();
            values
                .try_reserve_exact(surface_count)
                .map_err(|_| to_program_js_error(E::Update, ProgramOperation::UpdateObserved))?;
            for offset in 0..surface_count {
                let byte = start + offset * 3;
                values.push(labcolors_core::Srgb8::new([
                    surfaces[byte],
                    surfaces[byte + 1],
                    surfaces[byte + 2],
                ]));
            }
            scenarios.push(labcolors_core::program_wire::ProgramScenarioV1::new(
                scenario_id,
                values,
            ));
        }
        self.inner
            .update_observed(revision, &scenarios)
            .map(|inner| ProgramSnapshot { inner })
            .map_err(|error| to_program_js_error(error, ProgramOperation::UpdateObserved))
    }

    #[wasm_bindgen(js_name = updateUnknown)]
    pub fn update_unknown(
        &mut self,
        revision: u64,
        reason_id: u32,
    ) -> Result<ProgramSnapshot, JsValue> {
        self.inner
            .update_unknown(revision, reason_id)
            .map(|inner| ProgramSnapshot { inner })
            .map_err(|error| to_program_js_error(error, ProgramOperation::UpdateUnknown))
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod browser_tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    const REFERENCE_WIRE_HEX: &str = concat!(
        "4c4350570100b3000000010000000b0000001414140100000015000000010b0000000000",
        "000000000000010000001f00000000000000010000002900000001150000000100000033",
        "000000011f000000010000003d000000290000003300000000000000000050409a999999",
        "9999c93f0101000000470000003d00000001000000470000003d00000001000000510000",
        "00093d000000030100000052000000013d000000141414010000005b00000029000000",
    );

    fn reference_wire() -> Vec<u8> {
        REFERENCE_WIRE_HEX
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let text = core::str::from_utf8(pair).expect("ASCII hex");
                u8::from_str_radix(text, 16).expect("canonical hex")
            })
            .collect()
    }

    fn assert_program_error(error: JsValue, code: &str, operation: &str) {
        assert!(error.is_instance_of::<js_sys::Error>());
        assert_eq!(
            js_sys::Reflect::get(&error, &JsValue::from_str("code"))
                .expect("code property")
                .as_string()
                .as_deref(),
            Some(code)
        );
        assert_eq!(
            js_sys::Reflect::get(&error, &JsValue::from_str("operation"))
                .expect("operation property")
                .as_string()
                .as_deref(),
            Some(operation)
        );
    }

    #[wasm_bindgen_test]
    fn program_error_projection_distinguishes_every_runtime_failure_class() {
        use labcolors_core::program_wire::ProgramRuntimeErrorV1 as E;

        for (error, operation, code) in [
            (
                E::Wire,
                ProgramOperation::CompileProgramWire,
                "program_wire",
            ),
            (
                E::Compile,
                ProgramOperation::CompileProgramWire,
                "program_compile",
            ),
            (
                E::FamilyArtifactsRequired,
                ProgramOperation::CompileProgramWire,
                "program_family_artifacts_required",
            ),
            (
                E::Instantiate,
                ProgramOperation::CompileProgramWire,
                "program_instantiate",
            ),
            (
                E::Update,
                ProgramOperation::UpdateObserved,
                "program_update",
            ),
        ] {
            assert_program_error(to_program_js_error(error, operation), code, operation.key());
        }
    }

    #[wasm_bindgen_test]
    fn terminal_program_wire_and_update_failures_keep_operation_context() {
        let wire_error = match compile_program_wire(&[], 1) {
            Ok(_) => panic!("empty wire must fail"),
            Err(error) => error,
        };
        assert_program_error(wire_error, "program_wire", "compileProgramWire");

        let mut runtime = compile_program_wire(&reference_wire(), 7).expect("canonical wire");
        let update_error = match runtime.update_observed(1, &[], &[], 1) {
            Ok(_) => panic!("empty scenario set must fail"),
            Err(error) => error,
        };
        assert_program_error(update_error, "program_update", "updateObserved");

        let shape_error = match runtime.update_observed(1, &[1], &[255, 255], 1) {
            Ok(_) => panic!("incomplete surface matrix must fail"),
            Err(error) => error,
        };
        assert_program_error(shape_error, "program_update", "updateObserved");
    }

    #[wasm_bindgen_test]
    fn terminal_program_compiles_updates_and_projects_one_snapshot() {
        let mut runtime = compile_program_wire(&reference_wire(), 1).expect("canonical wire");
        let snapshot = runtime
            .update_observed(1, &[1], &[255, 255, 255], 1)
            .expect("observed update");

        assert_eq!(snapshot.state(), "ready");
        assert_eq!(snapshot.output_count(), 1);
        assert_eq!(snapshot.output_slot(0).unwrap(), 91);
        assert_eq!(snapshot.output_rgb(0).unwrap().as_ref(), &[20, 20, 20]);
        assert_eq!(snapshot.output_opacity(0).unwrap(), 1.0);
    }

    #[wasm_bindgen_test]
    fn rejected_surface_matrix_does_not_poison_the_next_atomic_update() {
        let mut runtime = compile_program_wire(&reference_wire(), 7).expect("canonical wire");
        assert!(runtime.update_observed(1, &[1], &[255, 255], 1).is_err());

        let snapshot = runtime
            .update_observed(1, &[1], &[255, 255, 255], 1)
            .expect("valid update after refusal");
        assert_eq!(snapshot.state(), "ready");
        assert_eq!(snapshot.output_count(), 1);
    }
}
