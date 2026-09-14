use js_sys::{Array, Function, Object, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, prelude::*};

use labcolors_core::program_wire::{
    AppearanceSurroundV1, AttachedMaterializationAuthorityErrorV1,
    AttachedMaterializationAuthorityV1 as CoreAuthority, AttachedPointSinkErrorV1,
    AttachedPointSinkHostIntentV1, AttachedPointSinkHostPatchEntryV1, AttachedPointSinkHostV1,
    AttachedProgramAttachErrorV1, AttachedProgramCompileErrorV1, AttachedProgramEmissionBindingV1,
    AttachedProgramPresentationBindingV1, AttachedProgramUpdateErrorV1,
    AttachedProgramUpdateStateV1, CompiledAttachedProgramV1 as CoreCompiledAttachedProgram,
    ProgramScenarioV1, RendererProvenanceV1, compile_attached_program_wire_v1,
};

#[wasm_bindgen(typescript_custom_section)]
const ATTACHED_PROGRAM_TYPES: &str = r#"
/** One complete host patch entry admitted by the compiled Program. */
export interface AttachedPointSinkHostPatchEntryV1 {
  readonly output: number;
  readonly sinkOutput: number;
  readonly source: Uint8Array;
  readonly opacity: number;
}

/** Exact stamp-bound command issued by the attached Program runtime. */
export type AttachedPointSinkHostIntentV1 =
  | Readonly<{
      kind: "set-all";
      revision: bigint;
      bindingEpoch: bigint;
      expectedSequence: bigint;
      desiredSequence: bigint;
      patch: ReadonlyArray<AttachedPointSinkHostPatchEntryV1>;
    }>
  | Readonly<{
      kind: "revoke-all";
      revision: bigint;
      bindingEpoch: bigint;
      expectedSequence: bigint;
      desiredSequence: bigint;
    }>
  | Readonly<{
      kind: "confirm-exact";
      revision: bigint;
      bindingEpoch: bigint;
      publishedSequence: bigint;
      patch: ReadonlyArray<AttachedPointSinkHostPatchEntryV1>;
    }>;

/** Synchronous host transaction boundary. The implementation lives in the consumer harness. */
export interface AttachedPointSinkHostV1 {
  tryInstall(intent: AttachedPointSinkHostIntentV1): void;
}
"#;

const OP_COMPILE: &str = "compileAttachedProgramWire";
const OP_ATTACH: &str = "attachAttachedProgram";
const OP_UPDATE_OBSERVED: &str = "updateAttachedObserved";
const OP_UPDATE_UNKNOWN: &str = "updateAttachedUnknown";
const OP_VALIDATE_AUTHORITY: &str = "validateAttachedAuthority";
const OP_TAKE_AUTHORITY: &str = "takeAttachedAuthority";

fn attached_error(code: &str, operation: &str) -> JsValue {
    super::program_error("Attached Program operation failed", code, operation).into()
}

fn attached_error_with_cause(code: &str, operation: &str, cause: JsValue) -> JsValue {
    let error = super::program_error("Attached Program operation failed", code, operation);
    let _ = Reflect::set(error.as_ref(), &JsValue::from_str("cause"), &cause);
    error.into()
}

fn map_compile_error(error: AttachedProgramCompileErrorV1) -> JsValue {
    let code = match error {
        AttachedProgramCompileErrorV1::Wire => "attached_program_wire",
        AttachedProgramCompileErrorV1::Compile => "attached_program_compile",
        AttachedProgramCompileErrorV1::RootConsumedDownstream => {
            "attached_root_consumed_downstream"
        }
        AttachedProgramCompileErrorV1::FamilyArtifactsRequired => {
            "attached_family_artifacts_required"
        }
        _ => "attached_program_compile",
    };
    attached_error(code, OP_COMPILE)
}

fn map_attach_error(error: AttachedProgramAttachErrorV1) -> JsValue {
    let code = match error {
        AttachedProgramAttachErrorV1::ResourceExhausted => "attached_resource_exhausted",
        AttachedProgramAttachErrorV1::Instantiate => "attached_instantiate",
        AttachedProgramAttachErrorV1::InvalidBindings => "attached_invalid_bindings",
        AttachedProgramAttachErrorV1::ScopeChanged => "attached_scope_changed",
        AttachedProgramAttachErrorV1::EpochExhausted => "attached_epoch_exhausted",
        _ => "attached_attach",
    };
    attached_error(code, OP_ATTACH)
}

fn map_authority_error(error: AttachedMaterializationAuthorityErrorV1, operation: &str) -> JsValue {
    let code = match error {
        AttachedMaterializationAuthorityErrorV1::NonTerminalRoot => "attached_non_terminal_root",
        AttachedMaterializationAuthorityErrorV1::RootConsumedDownstream => {
            "attached_root_consumed_downstream"
        }
        AttachedMaterializationAuthorityErrorV1::PublishedRevisionMismatch => {
            "attached_published_revision_mismatch"
        }
        AttachedMaterializationAuthorityErrorV1::ProgramIdentityMismatch => {
            "attached_program_identity_mismatch"
        }
        AttachedMaterializationAuthorityErrorV1::ForeignOwnerGeneration => {
            "attached_foreign_owner_generation"
        }
        AttachedMaterializationAuthorityErrorV1::ForeignBindingEpoch => {
            "attached_foreign_binding_epoch"
        }
        AttachedMaterializationAuthorityErrorV1::MissingExactPointAbsenceProof => {
            "attached_missing_exact_point_absence_proof"
        }
        AttachedMaterializationAuthorityErrorV1::EmptyFinalOwnedDomain => {
            "attached_empty_final_owned_domain"
        }
        AttachedMaterializationAuthorityErrorV1::ResourceExhausted => "attached_resource_exhausted",
        _ => "attached_authority",
    };
    attached_error(code, operation)
}

fn map_sink_error(error: AttachedPointSinkErrorV1<JsValue>, operation: &str) -> JsValue {
    match error {
        AttachedPointSinkErrorV1::PatchScopeMismatch => {
            attached_error("attached_patch_scope_mismatch", operation)
        }
        AttachedPointSinkErrorV1::StampMismatch => {
            attached_error("attached_stamp_mismatch", operation)
        }
        AttachedPointSinkErrorV1::RevisionMismatch => {
            attached_error("attached_revision_mismatch", operation)
        }
        AttachedPointSinkErrorV1::AlreadyInstalled => {
            attached_error("attached_already_installed", operation)
        }
        AttachedPointSinkErrorV1::Host(cause) => {
            attached_error_with_cause("attached_host", operation, cause)
        }
        _ => attached_error("attached_sink", operation),
    }
}

fn map_update_error(error: AttachedProgramUpdateErrorV1<JsValue>, operation: &str) -> JsValue {
    match error {
        AttachedProgramUpdateErrorV1::ResourceExhausted => {
            attached_error("attached_resource_exhausted", operation)
        }
        AttachedProgramUpdateErrorV1::Update => attached_error("attached_update", operation),
        AttachedProgramUpdateErrorV1::SinkPrepare(error)
        | AttachedProgramUpdateErrorV1::SinkInstall(error) => map_sink_error(error, operation),
        AttachedProgramUpdateErrorV1::Authority(error) => map_authority_error(error, operation),
        AttachedProgramUpdateErrorV1::InternalInvariant => {
            attached_error("attached_internal_invariant", operation)
        }
        _ => attached_error("attached_update", operation),
    }
}

fn set_property(object: &Object, key: &str, value: JsValue) -> Result<(), JsValue> {
    if Reflect::set(object.as_ref(), &JsValue::from_str(key), &value)? {
        Ok(())
    } else {
        Err(JsValue::from_str("failed to project attached host intent"))
    }
}

fn project_patch(patch: &[AttachedPointSinkHostPatchEntryV1]) -> Result<Array, JsValue> {
    let projected = Array::new();
    for entry in patch {
        let value = Object::new();
        set_property(&value, "output", JsValue::from(entry.output()))?;
        set_property(&value, "sinkOutput", JsValue::from(entry.sink_output()))?;
        let source = entry.source().bytes();
        set_property(&value, "source", Uint8Array::from(source.as_slice()).into())?;
        set_property(&value, "opacity", JsValue::from_f64(entry.opacity()))?;
        projected.push(&value);
    }
    Ok(projected)
}

fn project_host_intent(intent: AttachedPointSinkHostIntentV1<'_>) -> Result<JsValue, JsValue> {
    let value = Object::new();
    match intent {
        AttachedPointSinkHostIntentV1::SetAll {
            revision,
            binding_epoch,
            expected_sequence,
            desired_sequence,
            patch,
        } => {
            set_property(&value, "kind", JsValue::from_str("set-all"))?;
            set_property(&value, "revision", JsValue::from(revision))?;
            set_property(&value, "bindingEpoch", JsValue::from(binding_epoch))?;
            set_property(&value, "expectedSequence", JsValue::from(expected_sequence))?;
            set_property(&value, "desiredSequence", JsValue::from(desired_sequence))?;
            set_property(&value, "patch", project_patch(patch)?.into())?;
        }
        AttachedPointSinkHostIntentV1::RevokeAll {
            revision,
            binding_epoch,
            expected_sequence,
            desired_sequence,
        } => {
            set_property(&value, "kind", JsValue::from_str("revoke-all"))?;
            set_property(&value, "revision", JsValue::from(revision))?;
            set_property(&value, "bindingEpoch", JsValue::from(binding_epoch))?;
            set_property(&value, "expectedSequence", JsValue::from(expected_sequence))?;
            set_property(&value, "desiredSequence", JsValue::from(desired_sequence))?;
        }
        AttachedPointSinkHostIntentV1::ConfirmExact {
            revision,
            binding_epoch,
            published_sequence,
            patch,
        } => {
            set_property(&value, "kind", JsValue::from_str("confirm-exact"))?;
            set_property(&value, "revision", JsValue::from(revision))?;
            set_property(&value, "bindingEpoch", JsValue::from(binding_epoch))?;
            set_property(
                &value,
                "publishedSequence",
                JsValue::from(published_sequence),
            )?;
            set_property(&value, "patch", project_patch(patch)?.into())?;
        }
    }
    Ok(value.into())
}

struct JsPointSinkHost {
    target: JsValue,
}

impl AttachedPointSinkHostV1 for JsPointSinkHost {
    type Error = JsValue;

    fn try_install(
        &mut self,
        intent: AttachedPointSinkHostIntentV1<'_>,
    ) -> Result<(), Self::Error> {
        let callback = Reflect::get(&self.target, &JsValue::from_str("tryInstall"))?;
        let callback = callback
            .dyn_into::<Function>()
            .map_err(|_| JsValue::from_str("attached host tryInstall must be a function"))?;
        let intent = project_host_intent(intent)?;
        callback.call1(&self.target, &intent).map(|_| ())
    }
}

pub(super) fn project_program_scenarios(
    scenario_ids: &[u32],
    surfaces: &[u8],
    surface_count: usize,
) -> Result<Vec<ProgramScenarioV1>, ()> {
    let row_bytes = surface_count.checked_mul(3).ok_or(())?;
    let expected = scenario_ids.len().checked_mul(row_bytes).ok_or(())?;
    if expected != surfaces.len() {
        return Err(());
    }

    let mut scenarios = Vec::new();
    scenarios
        .try_reserve_exact(scenario_ids.len())
        .map_err(|_| ())?;
    for (row, scenario_id) in scenario_ids.iter().copied().enumerate() {
        let start = row.checked_mul(row_bytes).ok_or(())?;
        let mut values = Vec::new();
        values.try_reserve_exact(surface_count).map_err(|_| ())?;
        for offset in 0..surface_count {
            let byte = start
                .checked_add(offset.checked_mul(3).ok_or(())?)
                .ok_or(())?;
            values.push(labcolors_core::Srgb8::new([
                surfaces[byte],
                surfaces[byte + 1],
                surfaces[byte + 2],
            ]));
        }
        scenarios.push(ProgramScenarioV1::new(scenario_id, values));
    }
    Ok(scenarios)
}

#[wasm_bindgen]
pub struct CompiledAttachedProgram {
    inner: CoreCompiledAttachedProgram,
}

#[wasm_bindgen(js_name = compileAttachedProgramWire)]
pub fn compile_attached_program_wire(bytes: &[u8]) -> Result<CompiledAttachedProgram, JsValue> {
    compile_attached_program_wire_v1(bytes)
        .map(|inner| CompiledAttachedProgram { inner })
        .map_err(map_compile_error)
}

#[wasm_bindgen]
impl CompiledAttachedProgram {
    #[wasm_bindgen(js_name = contentIdentity)]
    pub fn content_identity(&self) -> Box<[u8]> {
        self.inner.content_identity().to_vec().into_boxed_slice()
    }

    #[wasm_bindgen(getter, js_name = surfaceInputCount)]
    pub fn surface_input_count(&self) -> usize {
        self.inner.surface_input_count()
    }

    #[wasm_bindgen(js_name = attach)]
    #[allow(clippy::too_many_arguments)]
    pub fn attach(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] stream_id: JsValue,
        emission_outputs: &[u32],
        emission_sink_outputs: &[u32],
        presentation_outputs: &[u32],
        presentation_roots: &[u32],
        presentation_occurrences: &[u32],
        #[wasm_bindgen(unchecked_param_type = "AttachedPointSinkHostV1")] host: JsValue,
    ) -> Result<AttachedProgramRuntime, JsValue> {
        let stream_id = super::checked_u32(stream_id)
            .ok_or_else(|| attached_error("attached_instantiate", OP_ATTACH))?;
        if emission_outputs.len() != emission_sink_outputs.len()
            || presentation_outputs.len() != presentation_roots.len()
            || presentation_outputs.len() != presentation_occurrences.len()
        {
            return Err(attached_error("attached_invalid_bindings", OP_ATTACH));
        }

        let mut emissions = Vec::new();
        emissions
            .try_reserve_exact(emission_outputs.len())
            .map_err(|_| attached_error("attached_resource_exhausted", OP_ATTACH))?;
        emissions.extend(
            emission_outputs
                .iter()
                .copied()
                .zip(emission_sink_outputs.iter().copied())
                .map(|(output, sink_output)| {
                    AttachedProgramEmissionBindingV1::new(output, sink_output)
                }),
        );

        let mut presentations = Vec::new();
        presentations
            .try_reserve_exact(presentation_outputs.len())
            .map_err(|_| attached_error("attached_resource_exhausted", OP_ATTACH))?;
        presentations.extend(
            presentation_outputs
                .iter()
                .copied()
                .zip(presentation_roots.iter().copied())
                .zip(presentation_occurrences.iter().copied())
                .map(|((output, root), occurrence)| {
                    AttachedProgramPresentationBindingV1::new(output, root, occurrence)
                }),
        );

        let surface_input_count = self.inner.surface_input_count();
        self.inner
            .attach(
                stream_id,
                &emissions,
                &presentations,
                JsPointSinkHost { target: host },
            )
            .map(|inner| AttachedProgramRuntime {
                inner,
                surface_input_count,
            })
            .map_err(map_attach_error)
    }
}

#[wasm_bindgen]
pub struct AttachedProgramRuntime {
    inner: labcolors_core::program_wire::AttachedProgramV1<JsPointSinkHost>,
    surface_input_count: usize,
}

#[wasm_bindgen]
impl AttachedProgramRuntime {
    #[wasm_bindgen(js_name = updateObserved)]
    pub fn update_observed(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        scenario_ids: &[u32],
        surfaces: &[u8],
    ) -> Result<AttachedProgramUpdate, JsValue> {
        let revision = u64::try_from(revision)
            .map_err(|_| attached_error("attached_update", OP_UPDATE_OBSERVED))?;
        let scenarios = project_program_scenarios(scenario_ids, surfaces, self.surface_input_count)
            .map_err(|_| attached_error("attached_update", OP_UPDATE_OBSERVED))?;
        self.inner
            .update_observed(revision, &scenarios)
            .map(AttachedProgramUpdate::from_core)
            .map_err(|error| map_update_error(error, OP_UPDATE_OBSERVED))
    }

    #[wasm_bindgen(js_name = updateUnknown)]
    pub fn update_unknown(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        #[wasm_bindgen(unchecked_param_type = "number")] reason_id: JsValue,
    ) -> Result<AttachedProgramUpdate, JsValue> {
        let revision = u64::try_from(revision)
            .map_err(|_| attached_error("attached_update", OP_UPDATE_UNKNOWN))?;
        let reason_id = super::checked_u32(reason_id)
            .ok_or_else(|| attached_error("attached_update", OP_UPDATE_UNKNOWN))?;
        self.inner
            .update_unknown(revision, reason_id)
            .map(AttachedProgramUpdate::from_core)
            .map_err(|error| map_update_error(error, OP_UPDATE_UNKNOWN))
    }

    #[wasm_bindgen(js_name = validateAuthority)]
    pub fn validate_authority(
        &self,
        authority: &AttachedMaterializationAuthority,
    ) -> Result<(), JsValue> {
        self.inner
            .validate_authority(&authority.inner)
            .map_err(|error| map_authority_error(error, OP_VALIDATE_AUTHORITY))
    }
}

#[wasm_bindgen]
pub struct AttachedProgramUpdate {
    state: AttachedProgramUpdateStateV1,
    authorities: Vec<Option<CoreAuthority>>,
}

impl AttachedProgramUpdate {
    fn from_core(update: labcolors_core::program_wire::AttachedProgramUpdateV1) -> Self {
        let state = update.state();
        let authorities = update.into_authorities().into_iter().map(Some).collect();
        Self { state, authorities }
    }
}

#[wasm_bindgen]
impl AttachedProgramUpdate {
    #[wasm_bindgen(getter)]
    pub fn state(&self) -> String {
        match self.state {
            AttachedProgramUpdateStateV1::Waiting => "waiting",
            AttachedProgramUpdateStateV1::Ready => "ready",
            AttachedProgramUpdateStateV1::Stale => "stale",
            AttachedProgramUpdateStateV1::Failed => "failed",
            _ => "unknown",
        }
        .to_string()
    }

    #[wasm_bindgen(js_name = authorityCount)]
    pub fn authority_count(&self) -> usize {
        self.authorities.len()
    }

    #[wasm_bindgen(js_name = takeAuthority)]
    pub fn take_authority(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
    ) -> Result<AttachedMaterializationAuthority, JsValue> {
        let index = super::checked_u32(index)
            .map(|value| value as usize)
            .ok_or_else(|| attached_error("attached_authority_unavailable", OP_TAKE_AUTHORITY))?;
        let authority = self
            .authorities
            .get_mut(index)
            .and_then(Option::take)
            .ok_or_else(|| attached_error("attached_authority_unavailable", OP_TAKE_AUTHORITY))?;
        Ok(AttachedMaterializationAuthority { inner: authority })
    }
}

#[wasm_bindgen]
pub struct AttachedMaterializationAuthority {
    inner: CoreAuthority,
}

#[wasm_bindgen]
impl AttachedMaterializationAuthority {
    #[wasm_bindgen(js_name = contentIdentity)]
    pub fn content_identity(&self) -> Box<[u8]> {
        self.inner.content_identity().to_vec().into_boxed_slice()
    }

    #[wasm_bindgen(getter, js_name = publishedRevision)]
    pub fn published_revision(&self) -> u64 {
        self.inner.published_revision()
    }

    #[wasm_bindgen(getter, js_name = sinkSequence)]
    pub fn sink_sequence(&self) -> u64 {
        self.inner.sink_sequence()
    }

    #[wasm_bindgen(getter, js_name = sinkBindingEpoch)]
    pub fn sink_binding_epoch(&self) -> u64 {
        self.inner.sink_binding_epoch()
    }

    #[wasm_bindgen(getter, js_name = presentationRoot)]
    pub fn presentation_root(&self) -> u32 {
        self.inner.presentation_root()
    }

    #[wasm_bindgen(getter, js_name = presentationOccurrence)]
    pub fn presentation_occurrence(&self) -> u32 {
        self.inner.presentation_occurrence()
    }

    #[wasm_bindgen(getter, js_name = terminalOccurrence)]
    pub fn terminal_occurrence(&self) -> u32 {
        self.inner.terminal_occurrence()
    }

    #[wasm_bindgen(js_name = sourceRgb)]
    pub fn source_rgb(&self) -> Box<[u8]> {
        self.inner.source().bytes().to_vec().into_boxed_slice()
    }

    #[wasm_bindgen(getter)]
    pub fn opacity(&self) -> f64 {
        self.inner.opacity()
    }

    #[wasm_bindgen(js_name = caseCount)]
    pub fn case_count(&self) -> usize {
        self.inner.case_count()
    }

    #[wasm_bindgen(js_name = caseIndex)]
    pub fn case_index(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
    ) -> Result<usize, JsValue> {
        let index = super::checked_u32(index)
            .map(|value| value as usize)
            .ok_or_else(|| attached_error("attached_authority_case", OP_VALIDATE_AUTHORITY))?;
        self.inner
            .cases()
            .nth(index)
            .map(|case| case.case_index())
            .ok_or_else(|| attached_error("attached_authority_case", OP_VALIDATE_AUTHORITY))
    }

    #[wasm_bindgen(js_name = caseComposite)]
    pub fn case_composite(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
    ) -> Result<Box<[u8]>, JsValue> {
        let index = super::checked_u32(index)
            .map(|value| value as usize)
            .ok_or_else(|| attached_error("attached_authority_case", OP_VALIDATE_AUTHORITY))?;
        self.inner
            .cases()
            .nth(index)
            .map(|case| case.composite().bytes().to_vec().into_boxed_slice())
            .ok_or_else(|| attached_error("attached_authority_case", OP_VALIDATE_AUTHORITY))
    }

    #[wasm_bindgen(getter, js_name = adaptingLuminanceCdM2)]
    pub fn adapting_luminance_cd_m2(&self) -> f64 {
        self.inner.adapting_luminance_cd_m2()
    }

    #[wasm_bindgen(getter, js_name = backgroundLuminanceRatioYbYw)]
    pub fn background_luminance_ratio_yb_yw(&self) -> f64 {
        self.inner.background_luminance_ratio_yb_yw()
    }

    #[wasm_bindgen(getter, js_name = appearanceSurround)]
    pub fn appearance_surround(&self) -> String {
        match self.inner.appearance_surround() {
            AppearanceSurroundV1::Average => "average",
            AppearanceSurroundV1::Dim => "dim",
            AppearanceSurroundV1::Dark => "dark",
            _ => "unknown",
        }
        .to_string()
    }

    #[wasm_bindgen(getter, js_name = rendererProvenance)]
    pub fn renderer_provenance(&self) -> String {
        match self.inner.renderer_provenance() {
            RendererProvenanceV1::Unverified => "unverified",
            _ => "unknown",
        }
        .to_string()
    }
}
