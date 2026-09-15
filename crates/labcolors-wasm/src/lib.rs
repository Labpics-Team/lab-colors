//! `@labpics/colors` — терминальная WASM-граница C7c.
//!
//! Один runtime-root: canonical Program wire → ProgramRuntime → ProgramSnapshot.
//! Legacy recipe engine (`LabColors.resolveTheme/loadConfig`) удалён. DOM/CSS
//! effects принадлежат npm-приложению, не WASM.

mod error;
mod terminal_projection;

use std::cell::{Cell, RefCell};

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

#[wasm_bindgen(inline_js = r#"
export function programError(message, code, operation) {
  try {
    const error = new Error(message);
    error.code = code;
    error.operation = operation;
    return error;
  } catch {
    return new Error("Program error projection failed");
  }
}

export function unsupportedPhysicalIdentityError() {
  return programError(
    "Unsupported program physical identity",
    "program_physical_identity",
    "physicalIdentity",
  );
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = programError)]
    fn program_error(message: &str, code: &str, operation: &str) -> js_sys::Error;

    #[wasm_bindgen(js_name = unsupportedPhysicalIdentityError)]
    fn unsupported_physical_identity_error() -> js_sys::Error;
}

fn to_js_error(error: BindingError) -> JsError {
    JsError::new(&error.to_string())
}

#[derive(Clone, Copy)]
enum ProgramOperation {
    CompileProgramWire,
    UpdateObserved,
    UpdateUnknown,
    AttachProgramWire,
    AttachmentUpdateObserved,
    AttachmentUpdateUnknown,
    AttachmentDispose,
    MaterializationAuthority,
}

impl ProgramOperation {
    const fn key(self) -> &'static str {
        match self {
            Self::CompileProgramWire => "compileProgramWire",
            Self::UpdateObserved => "updateObserved",
            Self::UpdateUnknown => "updateUnknown",
            Self::AttachProgramWire => "attachProgramWire",
            Self::AttachmentUpdateObserved => "attachmentUpdateObserved",
            Self::AttachmentUpdateUnknown => "attachmentUpdateUnknown",
            Self::AttachmentDispose => "attachmentDispose",
            Self::MaterializationAuthority => "materializationAuthority",
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
    program_error("Program runtime operation failed", code, operation.key()).into()
}

fn attachment_js_error(message: &str, code: &str, operation: ProgramOperation) -> JsValue {
    program_error(message, code, operation.key()).into()
}

fn physical_identity_string(
    identity: Option<labcolors_core::program_wire::ProgramPhysicalIdentityV1>,
) -> Result<js_sys::JsString, JsValue> {
    match identity {
        Some(labcolors_core::program_wire::ProgramPhysicalIdentityV1::EncodedSrgb8SourceOverV1) => {
            Ok(js_sys::JsString::from("encoded-srgb8-source-over-v1"))
        }
        None => Ok(js_sys::JsString::from("unknown")),
        Some(_) => Err(unsupported_physical_identity_error().into()),
    }
}

fn to_attachment_error(
    error: labcolors_core::program_wire::ProgramAttachErrorV1,
    operation: ProgramOperation,
) -> JsValue {
    use labcolors_core::program_wire::ProgramAttachErrorV1 as E;
    let code = match error {
        E::Binding => "program_attachment_binding",
        E::Instantiate => "program_attachment_instantiate",
        E::SinkAdmission => "program_attachment_sink_admission",
        E::ResourceExhausted => "program_attachment_resource_exhausted",
        E::NonTerminalTarget => "program_attachment_non_terminal_target",
        _ => "program_attachment",
    };
    attachment_js_error("Program attachment admission failed", code, operation)
}

fn to_attachment_update_error(
    error: labcolors_core::program_wire::ProgramAttachmentUpdateErrorV1,
    operation: ProgramOperation,
) -> JsValue {
    use labcolors_core::program_wire::{
        ProgramAttachmentUpdateErrorV1 as E, ProgramPointSinkErrorV1 as Sink,
        ProgramPointSinkHostErrorV1 as Host,
    };
    let code = match error {
        E::Update => "program_attachment_update",
        E::ResourceExhausted => "program_attachment_resource_exhausted",
        E::InternalInvariant => "program_attachment_internal_invariant",
        E::AlreadyDisposed => "program_attachment_already_disposed",
        E::Sink(Sink::PatchScopeMismatch) => "program_attachment_patch_scope_mismatch",
        E::Sink(Sink::StampMismatch) => "program_attachment_stamp_mismatch",
        E::Sink(Sink::RevisionMismatch) => "program_attachment_revision_mismatch",
        E::Sink(Sink::AlreadyInstalled) => "program_attachment_already_installed",
        E::Sink(Sink::Host(Host::Rejected)) => "program_attachment_host_rejected",
        E::Sink(Sink::Host(Host::Protocol)) => "program_attachment_host_protocol",
        // Будущий non_exhaustive-вариант не должен маскироваться под
        // известный `Update`: package classifier обязан отказать ему.
        _ => "program_attachment_update_unknown",
    };
    attachment_js_error("Program attachment update failed", code, operation)
}

fn to_authority_error(
    error: labcolors_core::program_wire::ProgramMaterializationAuthorityErrorV1,
) -> JsValue {
    use labcolors_core::program_wire::ProgramMaterializationAuthorityErrorV1 as E;
    let code = match error {
        E::NotReady => "program_materialization_not_ready",
        E::SourceOrIntermediatePaint => "program_materialization_paint_not_authority",
        E::StaleRevision => "program_materialization_stale_revision",
        E::StaleIdentity => "program_materialization_stale_identity",
        E::StaleSinkStamp => "program_materialization_stale_sink_stamp",
        E::ForeignBindingEpoch => "program_materialization_foreign_binding_epoch",
        E::TerminalBindingMismatch => "program_materialization_terminal_binding_mismatch",
        E::MissingPointAbsenceProof => "program_materialization_missing_point_absence_proof",
        E::AmbiguousObservationCases => "program_materialization_ambiguous_observation_cases",
        _ => "program_materialization_authority",
    };
    attachment_js_error(
        "Attached materialization authority was refused",
        code,
        ProgramOperation::MaterializationAuthority,
    )
}

// На JS-границе integer ABI усекал/оборачивал вход ДО проверки Core.
// Проверяем исходный JsValue: as_f64 не вызывает пользовательское coercion,
// а u64::try_from дополнительно проверяет, что bigint не потерял старшие биты.
fn checked_u32(value: JsValue) -> Option<u32> {
    let number = value.as_f64()?;
    if number.is_finite() && number.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&number)
    {
        Some(number as u32)
    } else {
        None
    }
}

fn checked_output_index(value: JsValue) -> Result<usize, JsError> {
    checked_u32(value)
        .map(|index| index as usize)
        .ok_or_else(|| {
            to_js_error(BindingError::Internal {
                reason: "program output index must be a u32 number".to_string(),
            })
        })
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
    pub fn output_slot(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
    ) -> Result<u32, JsError> {
        let index = checked_output_index(index)?;
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
    pub fn output_rgb(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
    ) -> Result<Box<[u8]>, JsError> {
        let index = checked_output_index(index)?;
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
    pub fn output_opacity(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
    ) -> Result<f64, JsError> {
        let index = checked_output_index(index)?;
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
pub fn compile_program_wire(
    bytes: &[u8],
    #[wasm_bindgen(unchecked_param_type = "number")] stream_id: JsValue,
) -> Result<ProgramRuntime, JsValue> {
    use labcolors_core::program_wire::ProgramRuntimeErrorV1 as E;
    let stream_id = checked_u32(stream_id)
        .ok_or_else(|| to_program_js_error(E::Instantiate, ProgramOperation::CompileProgramWire))?;
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
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        scenario_ids: &[u32],
        surfaces: &[u8],
        #[wasm_bindgen(unchecked_param_type = "number")] surface_count: JsValue,
    ) -> Result<ProgramSnapshot, JsValue> {
        use labcolors_core::program_wire::ProgramRuntimeErrorV1 as E;
        let revision = u64::try_from(revision)
            .map_err(|_| to_program_js_error(E::Update, ProgramOperation::UpdateObserved))?;
        let surface_count = checked_u32(surface_count)
            .ok_or_else(|| to_program_js_error(E::Update, ProgramOperation::UpdateObserved))?
            as usize;
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
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        #[wasm_bindgen(unchecked_param_type = "number")] reason_id: JsValue,
    ) -> Result<ProgramSnapshot, JsValue> {
        use labcolors_core::program_wire::ProgramRuntimeErrorV1 as E;
        let revision = u64::try_from(revision)
            .map_err(|_| to_program_js_error(E::Update, ProgramOperation::UpdateUnknown))?;
        let reason_id = checked_u32(reason_id)
            .ok_or_else(|| to_program_js_error(E::Update, ProgramOperation::UpdateUnknown))?;
        self.inner
            .update_unknown(revision, reason_id)
            .map(|inner| ProgramSnapshot { inner })
            .map_err(|error| to_program_js_error(error, ProgramOperation::UpdateUnknown))
    }
}

/// Синхронный WASM-адаптер point sink, принадлежащего consumer.
#[derive(Clone)]
struct JsPointSinkHostV1 {
    callback: js_sys::Function,
}

impl labcolors_core::program_wire::ProgramPointSinkHostV1 for JsPointSinkHostV1 {
    fn try_install(
        &mut self,
        intent: labcolors_core::program_wire::ProgramPointSinkIntentV1,
    ) -> Result<(), labcolors_core::program_wire::ProgramPointSinkHostErrorV1> {
        let object = js_sys::Object::new();
        let object_value: JsValue = object.clone().into();
        let operation = match intent.operation() {
            labcolors_core::program_wire::ProgramPointSinkOperationV1::SetAll => "setAll",
            labcolors_core::program_wire::ProgramPointSinkOperationV1::RevokeAll => "revokeAll",
            labcolors_core::program_wire::ProgramPointSinkOperationV1::ConfirmExact => {
                "confirmExact"
            }
            _ => return Err(labcolors_core::program_wire::ProgramPointSinkHostErrorV1::Protocol),
        };
        let set = |key: &str, value: &JsValue| {
            js_sys::Reflect::set(&object_value, &JsValue::from_str(key), value).is_ok()
        };
        let bigint = |value: u64| -> JsValue { js_sys::BigInt::from(value).into() };
        if !set("operation", &JsValue::from_str(operation))
            || !set("revision", &bigint(intent.revision()))
            || !set(
                "expectedSequence",
                &bigint(intent.expected_stamp().sequence()),
            )
            || !set(
                "desiredSequence",
                &bigint(intent.desired_stamp().sequence()),
            )
            || !set(
                "bindingEpoch",
                &bigint(intent.expected_stamp().binding_epoch()),
            )
            || !set(
                "sinkOutput",
                &JsValue::from_f64(f64::from(intent.sink_output())),
            )
        {
            return Err(labcolors_core::program_wire::ProgramPointSinkHostErrorV1::Protocol);
        }
        let point = match intent.point() {
            Some(point) => {
                let point_object = js_sys::Object::new();
                let point_value: JsValue = point_object.clone().into();
                let rgb = point.source().bytes();
                let rgb = js_sys::Uint8Array::from(&rgb[..]);
                if js_sys::Reflect::set(
                    &point_value,
                    &JsValue::from_str("slot"),
                    &JsValue::from_f64(f64::from(point.slot())),
                )
                .is_err()
                    || js_sys::Reflect::set(&point_value, &JsValue::from_str("source"), &rgb.into())
                        .is_err()
                    || js_sys::Reflect::set(
                        &point_value,
                        &JsValue::from_str("opacity"),
                        &JsValue::from_f64(point.opacity()),
                    )
                    .is_err()
                {
                    return Err(
                        labcolors_core::program_wire::ProgramPointSinkHostErrorV1::Protocol,
                    );
                }
                point_value
            }
            None => JsValue::NULL,
        };
        if !set("point", &point) {
            return Err(labcolors_core::program_wire::ProgramPointSinkHostErrorV1::Protocol);
        }
        let status = self
            .callback
            .call1(&JsValue::UNDEFINED, &object_value)
            .map_err(|_| labcolors_core::program_wire::ProgramPointSinkHostErrorV1::Protocol)?;
        let accepted = status
            .as_bool()
            .ok_or(labcolors_core::program_wire::ProgramPointSinkHostErrorV1::Protocol)?;
        if !accepted {
            Err(labcolors_core::program_wire::ProgramPointSinkHostErrorV1::Rejected)
        } else {
            Ok(())
        }
    }
}

fn snapshot_state_key(state: labcolors_core::program_wire::ProgramSnapshotStateV1) -> &'static str {
    use labcolors_core::program_wire::ProgramSnapshotStateV1 as S;
    match state {
        S::Waiting => "waiting",
        S::Ready => "ready",
        S::Stale => "stale",
        S::Failed => "failed",
        _ => "unknown",
    }
}

fn attachment_snapshot_error(operation: ProgramOperation) -> JsValue {
    attachment_js_error(
        "Program attachment input shape is invalid",
        "program_attachment_update",
        operation,
    )
}

fn decode_attachment_scenarios(
    scenario_ids: &[u32],
    surfaces: &[u8],
    surface_count: JsValue,
    operation: ProgramOperation,
) -> Result<Vec<labcolors_core::program_wire::ProgramScenarioV1>, JsValue> {
    let surface_count =
        checked_u32(surface_count).ok_or_else(|| attachment_snapshot_error(operation))? as usize;
    let row_bytes = surface_count
        .checked_mul(3)
        .ok_or_else(|| attachment_snapshot_error(operation))?;
    let expected = scenario_ids
        .len()
        .checked_mul(row_bytes)
        .ok_or_else(|| attachment_snapshot_error(operation))?;
    if expected != surfaces.len() {
        return Err(attachment_snapshot_error(operation));
    }
    let mut scenarios = Vec::new();
    scenarios
        .try_reserve_exact(scenario_ids.len())
        .map_err(|_| {
            attachment_js_error(
                "Program attachment scenario allocation failed",
                "program_attachment_resource_exhausted",
                operation,
            )
        })?;
    for (row, scenario_id) in scenario_ids.iter().copied().enumerate() {
        let start = row * row_bytes;
        let mut values = Vec::new();
        values.try_reserve_exact(surface_count).map_err(|_| {
            attachment_js_error(
                "Program attachment surface allocation failed",
                "program_attachment_resource_exhausted",
                operation,
            )
        })?;
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
    Ok(scenarios)
}

/// Создаёт один публичный attachment и передаёт все внешние эффекты в `host`.
#[wasm_bindgen(js_name = attachProgramWire)]
pub fn attach_program_wire(
    bytes: &[u8],
    #[wasm_bindgen(unchecked_param_type = "number")] stream_id: JsValue,
    #[wasm_bindgen(unchecked_param_type = "number")] output_slot: JsValue,
    #[wasm_bindgen(unchecked_param_type = "number")] sink_output: JsValue,
    #[wasm_bindgen(unchecked_param_type = "number")] presentation_root: JsValue,
    #[wasm_bindgen(unchecked_param_type = "number")] occurrence: JsValue,
    host: &js_sys::Function,
) -> Result<ProgramAttachment, JsValue> {
    let parse = |value: JsValue| {
        checked_u32(value).ok_or_else(|| {
            attachment_js_error(
                "Program attachment identifiers must be u32 numbers",
                "program_attachment_binding",
                ProgramOperation::AttachProgramWire,
            )
        })
    };
    let stream_id = parse(stream_id)?;
    let output_slot = parse(output_slot)?;
    let sink_output = parse(sink_output)?;
    let presentation_root = parse(presentation_root)?;
    let occurrence = parse(occurrence)?;
    let compiled = labcolors_core::program_wire::compile_program_wire_v1(bytes)
        .map_err(|error| to_program_js_error(error, ProgramOperation::AttachProgramWire))?;
    let inner = compiled
        .attach(
            stream_id,
            output_slot,
            sink_output,
            presentation_root,
            occurrence,
            JsPointSinkHostV1 {
                callback: host.clone(),
            },
        )
        .map_err(|error| to_attachment_error(error, ProgramOperation::AttachProgramWire))?;
    Ok(ProgramAttachment {
        inner: RefCell::new(inner),
        busy: Cell::new(false),
    })
}

#[wasm_bindgen(js_name = ProgramAttachment)]
pub struct ProgramAttachment {
    // Общий JS receiver позволяет допустить busy на уровне Rust внутри host
    // callback; изменяемое Core attachment остаётся за этой WASM-only ячейкой.
    inner: RefCell<labcolors_core::program_wire::ProgramAttachmentV1<JsPointSinkHostV1>>,
    busy: Cell<bool>,
}

struct AttachmentBusyGuard<'a> {
    busy: &'a Cell<bool>,
}

impl Drop for AttachmentBusyGuard<'_> {
    fn drop(&mut self) {
        self.busy.set(false);
    }
}

impl ProgramAttachment {
    fn enter(
        busy: &Cell<bool>,
        operation: ProgramOperation,
    ) -> Result<AttachmentBusyGuard<'_>, JsValue> {
        if busy.get() {
            return Err(attachment_js_error(
                "Program attachment operation is already in progress",
                "program_attachment_busy",
                operation,
            ));
        }
        busy.set(true);
        Ok(AttachmentBusyGuard { busy })
    }
}

#[wasm_bindgen]
impl ProgramAttachment {
    /// Атомарно применяет observed revision через host sink.
    #[wasm_bindgen(js_name = updateObserved)]
    pub fn update_observed(
        &self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        scenario_ids: &[u32],
        surfaces: &[u8],
        #[wasm_bindgen(unchecked_param_type = "number")] surface_count: JsValue,
    ) -> Result<ProgramAttachedSnapshot, JsValue> {
        let _busy = Self::enter(&self.busy, ProgramOperation::AttachmentUpdateObserved)?;
        let revision = u64::try_from(revision)
            .map_err(|_| attachment_snapshot_error(ProgramOperation::AttachmentUpdateObserved))?;
        let scenarios = decode_attachment_scenarios(
            scenario_ids,
            surfaces,
            surface_count,
            ProgramOperation::AttachmentUpdateObserved,
        )?;
        let result = self
            .inner
            .borrow_mut()
            .update_observed(revision, &scenarios);
        result
            .map(|inner| ProgramAttachedSnapshot { inner })
            .map_err(|error| {
                to_attachment_update_error(error, ProgramOperation::AttachmentUpdateObserved)
            })
    }

    /// Атомарно отзывает owned sink для недоступной revision.
    #[wasm_bindgen(js_name = updateUnknown)]
    pub fn update_unknown(
        &self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        #[wasm_bindgen(unchecked_param_type = "number")] reason_id: JsValue,
    ) -> Result<ProgramAttachedSnapshot, JsValue> {
        let _busy = Self::enter(&self.busy, ProgramOperation::AttachmentUpdateUnknown)?;
        let revision = u64::try_from(revision)
            .map_err(|_| attachment_snapshot_error(ProgramOperation::AttachmentUpdateUnknown))?;
        let reason_id = checked_u32(reason_id)
            .ok_or_else(|| attachment_snapshot_error(ProgramOperation::AttachmentUpdateUnknown))?;
        let result = self.inner.borrow_mut().update_unknown(revision, reason_id);
        result
            .map(|inner| ProgramAttachedSnapshot { inner })
            .map_err(|error| {
                to_attachment_update_error(error, ProgramOperation::AttachmentUpdateUnknown)
            })
    }

    /// Допускает authority только из текущего committed head attachment.
    #[wasm_bindgen(js_name = materializationAuthority)]
    pub fn materialization_authority(&self) -> Result<AttachedMaterializationAuthority, JsValue> {
        let _busy = Self::enter(&self.busy, ProgramOperation::MaterializationAuthority)?;
        let result = self.inner.borrow().current_materialization_authority();
        result
            .map(|inner| AttachedMaterializationAuthority { inner })
            .map_err(to_authority_error)
    }

    /// Проверяет переданные caller-ом revision, identity и epoch против current head.
    #[wasm_bindgen(js_name = materializationAuthorityFor)]
    pub fn materialization_authority_for(
        &self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        content_identity: &[u8],
        #[wasm_bindgen(unchecked_param_type = "bigint")] binding_epoch: JsValue,
    ) -> Result<AttachedMaterializationAuthority, JsValue> {
        let _busy = Self::enter(&self.busy, ProgramOperation::MaterializationAuthority)?;
        let render = self.inner.borrow().current_render().ok_or_else(|| {
            to_authority_error(
                labcolors_core::program_wire::ProgramMaterializationAuthorityErrorV1::NotReady,
            )
        })?;
        let content_identity: [u8; 32] = content_identity.try_into().map_err(|_| {
            attachment_js_error(
                "Materialization content identity must contain 32 bytes",
                "program_materialization_stale_identity",
                ProgramOperation::MaterializationAuthority,
            )
        })?;
        let revision = u64::try_from(revision).map_err(|_| {
            to_authority_error(
                labcolors_core::program_wire::ProgramMaterializationAuthorityErrorV1::StaleRevision,
            )
        })?;
        let binding_epoch = u64::try_from(binding_epoch).map_err(|_| {
            to_authority_error(
                labcolors_core::program_wire::ProgramMaterializationAuthorityErrorV1::ForeignBindingEpoch,
            )
        })?;
        let expected = render
            .expectation()
            .with_content_identity(content_identity)
            .with_revision(revision)
            .with_binding_epoch(binding_epoch);
        let result = self.inner.borrow().materialization_authority(
            labcolors_core::program_wire::ProgramMaterializationCandidateV1::AttachedCommit,
            expected,
        );
        result
            .map(|inner| AttachedMaterializationAuthority { inner })
            .map_err(to_authority_error)
    }

    /// Потребляет attachment только после подтверждения внешнего revoke caller-ом.
    #[wasm_bindgen]
    pub fn dispose(&self, confirmed: bool) -> Result<(), JsValue> {
        let _busy = Self::enter(&self.busy, ProgramOperation::AttachmentDispose)?;
        let result = self
            .inner
            .borrow_mut()
            .dispose(|| if confirmed { Ok(()) } else { Err(()) });
        result.map_err(|error| match error {
            labcolors_core::program_wire::ProgramDisposeErrorV1::AlreadyDisposed => {
                attachment_js_error(
                    "Program attachment is already disposed",
                    "program_attachment_already_disposed",
                    ProgramOperation::AttachmentDispose,
                )
            }
            labcolors_core::program_wire::ProgramDisposeErrorV1::Confirmation(()) => {
                attachment_js_error(
                    "External sink revoke was not confirmed",
                    "program_attachment_revoke_unconfirmed",
                    ProgramOperation::AttachmentDispose,
                )
            }
            _ => attachment_js_error(
                "Program attachment disposal failed",
                "program_attachment_dispose",
                ProgramOperation::AttachmentDispose,
            ),
        })
    }
}

#[wasm_bindgen(js_name = ProgramAttachedSnapshot)]
pub struct ProgramAttachedSnapshot {
    inner: labcolors_core::program_wire::ProgramAttachedSnapshotV1,
}

#[wasm_bindgen]
impl ProgramAttachedSnapshot {
    #[wasm_bindgen(getter)]
    pub fn state(&self) -> String {
        snapshot_state_key(self.inner.snapshot().state()).to_string()
    }

    #[wasm_bindgen(js_name = outputCount)]
    pub fn output_count(&self) -> usize {
        self.inner.snapshot().outputs().len()
    }

    #[wasm_bindgen(js_name = outputSlot)]
    pub fn output_slot(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
    ) -> Result<u32, JsError> {
        let index = checked_output_index(index)?;
        self.inner
            .snapshot()
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
    pub fn output_rgb(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
    ) -> Result<Box<[u8]>, JsError> {
        let index = checked_output_index(index)?;
        self.inner
            .snapshot()
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
    pub fn output_opacity(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
    ) -> Result<f64, JsError> {
        let index = checked_output_index(index)?;
        self.inner
            .snapshot()
            .outputs()
            .get(index)
            .map(|output| output.opacity())
            .ok_or_else(|| {
                to_js_error(BindingError::Internal {
                    reason: format!("program output index {index} is out of bounds"),
                })
            })
    }

    #[wasm_bindgen(js_name = hasRender)]
    pub fn has_render(&self) -> bool {
        self.inner.render().is_some()
    }

    #[wasm_bindgen]
    pub fn render(&self) -> Option<ProgramAttachedRender> {
        self.inner
            .render()
            .map(|inner| ProgramAttachedRender { inner })
    }
}

#[wasm_bindgen(js_name = ProgramAttachedRender)]
pub struct ProgramAttachedRender {
    inner: labcolors_core::program_wire::ProgramAttachedRenderV1,
}

#[wasm_bindgen]
impl ProgramAttachedRender {
    #[wasm_bindgen(js_name = outputSlot)]
    pub fn output_slot(&self) -> u32 {
        self.inner.output().slot()
    }

    #[wasm_bindgen(js_name = sinkOutput)]
    pub fn sink_output(&self) -> u32 {
        self.inner.sink_output()
    }

    #[wasm_bindgen(js_name = outputRgb)]
    pub fn output_rgb(&self) -> Box<[u8]> {
        self.inner
            .output()
            .source()
            .bytes()
            .to_vec()
            .into_boxed_slice()
    }

    #[wasm_bindgen(js_name = outputOpacity)]
    pub fn output_opacity(&self) -> f64 {
        self.inner.output().opacity()
    }

    #[wasm_bindgen]
    pub fn revision(&self) -> u64 {
        self.inner.revision()
    }

    #[wasm_bindgen(js_name = sinkSequence)]
    pub fn sink_sequence(&self) -> u64 {
        self.inner.sink_stamp().sequence()
    }

    #[wasm_bindgen(js_name = bindingEpoch)]
    pub fn binding_epoch(&self) -> u64 {
        self.inner.sink_stamp().binding_epoch()
    }

    #[wasm_bindgen(js_name = contentIdentity)]
    pub fn content_identity(&self) -> Box<[u8]> {
        self.inner.content_identity().to_vec().into_boxed_slice()
    }

    #[wasm_bindgen(js_name = contextIdentity)]
    pub fn context_identity(&self) -> Box<[u8]> {
        self.inner
            .context()
            .identity_bytes()
            .to_vec()
            .into_boxed_slice()
    }

    #[wasm_bindgen(js_name = presentationRoot)]
    pub fn presentation_root(&self) -> u32 {
        self.inner.presentation_root()
    }

    #[wasm_bindgen]
    pub fn occurrence(&self) -> u32 {
        self.inner.occurrence()
    }

    #[wasm_bindgen(js_name = physicalIdentity)]
    pub fn physical_identity(&self) -> String {
        match self.inner.physical_identity() {
            Some(
                labcolors_core::program_wire::ProgramPhysicalIdentityV1::EncodedSrgb8SourceOverV1,
            ) => "encoded-srgb8-source-over-v1",
            None => "unknown",
            Some(_) => "unsupported",
        }
        .to_string()
    }

    #[wasm_bindgen(js_name = terminalCompositeRgb)]
    pub fn terminal_composite_rgb(&self) -> Option<Box<[u8]>> {
        self.inner
            .terminal_composite()
            .map(|value| value.bytes().to_vec().into_boxed_slice())
    }
}

#[wasm_bindgen]
pub struct AttachedMaterializationAuthority {
    inner: labcolors_core::program_wire::AttachedMaterializationAuthorityV1,
}

#[wasm_bindgen]
impl AttachedMaterializationAuthority {
    #[wasm_bindgen(js_name = outputSlot)]
    pub fn output_slot(&self) -> u32 {
        self.inner.output().slot()
    }

    #[wasm_bindgen(js_name = outputRgb)]
    pub fn output_rgb(&self) -> Box<[u8]> {
        self.inner
            .output()
            .source()
            .bytes()
            .to_vec()
            .into_boxed_slice()
    }

    #[wasm_bindgen(js_name = outputOpacity)]
    pub fn output_opacity(&self) -> f64 {
        self.inner.output().opacity()
    }

    #[wasm_bindgen]
    pub fn revision(&self) -> u64 {
        self.inner.revision()
    }

    #[wasm_bindgen(js_name = sinkSequence)]
    pub fn sink_sequence(&self) -> u64 {
        self.inner.sink_stamp().sequence()
    }

    #[wasm_bindgen(js_name = bindingEpoch)]
    pub fn binding_epoch(&self) -> u64 {
        self.inner.sink_stamp().binding_epoch()
    }

    #[wasm_bindgen(js_name = contentIdentity)]
    pub fn content_identity(&self) -> Box<[u8]> {
        self.inner.content_identity().to_vec().into_boxed_slice()
    }

    #[wasm_bindgen(js_name = contextIdentity)]
    pub fn context_identity(&self) -> Box<[u8]> {
        self.inner
            .context()
            .identity_bytes()
            .to_vec()
            .into_boxed_slice()
    }

    #[wasm_bindgen(js_name = presentationRoot)]
    pub fn presentation_root(&self) -> u32 {
        self.inner.presentation_root()
    }

    #[wasm_bindgen]
    pub fn occurrence(&self) -> u32 {
        self.inner.occurrence()
    }

    #[wasm_bindgen(js_name = physicalIdentity)]
    pub fn physical_identity(&self) -> Result<js_sys::JsString, JsValue> {
        physical_identity_string(self.inner.physical_identity())
    }

    #[wasm_bindgen(js_name = rendererProvenance)]
    pub fn renderer_provenance(&self) -> String {
        match self.inner.renderer_provenance() {
            labcolors_core::program_wire::ProgramRendererProvenanceV1::Unverified => "unverified",
            _ => "unknown",
        }
        .to_string()
    }

    #[wasm_bindgen(js_name = terminalCompositeRgb)]
    pub fn terminal_composite_rgb(&self) -> Box<[u8]> {
        self.inner
            .terminal_composite()
            .bytes()
            .to_vec()
            .into_boxed_slice()
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
        let wire_error = match compile_program_wire(&[], 1.into()) {
            Ok(_) => panic!("empty wire must fail"),
            Err(error) => error,
        };
        assert_program_error(wire_error, "program_wire", "compileProgramWire");

        let mut runtime =
            compile_program_wire(&reference_wire(), 7.into()).expect("canonical wire");
        let update_error = match runtime.update_observed(1_u64.into(), &[], &[], 1.into()) {
            Ok(_) => panic!("empty scenario set must fail"),
            Err(error) => error,
        };
        assert_program_error(update_error, "program_update", "updateObserved");

        let shape_error = match runtime.update_observed(1_u64.into(), &[1], &[255, 255], 1.into()) {
            Ok(_) => panic!("incomplete surface matrix must fail"),
            Err(error) => error,
        };
        assert_program_error(shape_error, "program_update", "updateObserved");
    }

    #[wasm_bindgen_test]
    fn terminal_program_compiles_updates_and_projects_one_snapshot() {
        let mut runtime =
            compile_program_wire(&reference_wire(), 1.into()).expect("canonical wire");
        let snapshot = runtime
            .update_observed(1_u64.into(), &[1], &[255, 255, 255], 1.into())
            .expect("observed update");

        assert_eq!(snapshot.state(), "ready");
        assert_eq!(snapshot.output_count(), 1);
        assert_eq!(snapshot.output_slot(0.into()).unwrap(), 91);
        assert_eq!(
            snapshot.output_rgb(0.into()).unwrap().as_ref(),
            &[20, 20, 20]
        );
        assert_eq!(snapshot.output_opacity(0.into()).unwrap(), 1.0);
    }

    #[wasm_bindgen_test]
    fn rejected_surface_matrix_does_not_poison_the_next_atomic_update() {
        let mut runtime =
            compile_program_wire(&reference_wire(), 7.into()).expect("canonical wire");
        assert!(
            runtime
                .update_observed(1_u64.into(), &[1], &[255, 255], 1.into())
                .is_err()
        );

        let snapshot = runtime
            .update_observed(1_u64.into(), &[1], &[255, 255, 255], 1.into())
            .expect("valid update after refusal");
        assert_eq!(snapshot.state(), "ready");
        assert_eq!(snapshot.output_count(), 1);
    }
}
