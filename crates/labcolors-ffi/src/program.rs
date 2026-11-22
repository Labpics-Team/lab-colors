//! Нативная проекция общего Program API. Состояние и решения принадлежат Core.
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

use labcolors_core::program_wire as core;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum LabColorsError {
    #[error("{operation}: {code}")]
    Refused { code: String, operation: String },
}

impl LabColorsError {
    pub(crate) fn new(code: &str, operation: &str) -> Self {
        Self::Refused {
            code: code.into(),
            operation: operation.into(),
        }
    }
}

// UniFFI otherwise panics when a callback throws an undeclared foreign error.
// The normal host transaction must abort as a value even in panic=abort builds.
impl From<uniffi::UnexpectedUniFFICallbackError> for LabColorsError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::new("program_host_protocol", "updateObservedWith")
    }
}

fn enter<'a, T>(
    owner: &'a Mutex<T>,
    operation: &str,
    busy_code: &str,
) -> Result<MutexGuard<'a, T>, LabColorsError> {
    owner.try_lock().map_err(|error| {
        LabColorsError::new(
            match error {
                TryLockError::WouldBlock => busy_code,
                TryLockError::Poisoned(_) => "program_internal_invariant",
            },
            operation,
        )
    })
}

fn runtime_error(error: core::ProgramRuntimeErrorV1, operation: &str) -> LabColorsError {
    use core::ProgramRuntimeErrorV1 as E;
    LabColorsError::new(
        match error {
            E::Wire => "program_wire",
            E::Compile => "program_compile",
            E::FamilyArtifactsRequired => "program_family_artifacts_required",
            E::Instantiate => "program_instantiate",
            E::Update => "program_update",
            E::ResourceExhausted => "program_resource_exhausted",
            E::InternalInvariant => "program_internal_invariant",
            _ => "program_runtime",
        },
        operation,
    )
}

fn attach_error(error: core::ProgramAttachErrorV1, operation: &str) -> LabColorsError {
    use core::ProgramAttachErrorV1 as E;
    LabColorsError::new(
        match error {
            E::Binding => "program_attachment_binding",
            E::Instantiate => "program_attachment_instantiate",
            E::SinkAdmission => "program_attachment_sink_admission",
            E::ResourceExhausted => "program_attachment_resource_exhausted",
            E::InternalInvariant => "program_attachment_internal_invariant",
            E::NonTerminalTarget => "program_attachment_non_terminal_target",
            _ => "program_attachment",
        },
        operation,
    )
}

fn update_error(error: core::ProgramAttachmentUpdateErrorV1, operation: &str) -> LabColorsError {
    use core::{
        ProgramAttachmentUpdateErrorV1 as E, ProgramPointSinkErrorV1 as S,
        ProgramPointSinkHostErrorV1 as H,
    };
    LabColorsError::new(
        match error {
            E::Update => "program_attachment_update",
            E::ResourceExhausted => "program_attachment_resource_exhausted",
            E::InternalInvariant => "program_attachment_internal_invariant",
            E::AlreadyDisposed => "program_attachment_already_disposed",
            E::Sink(S::PatchScopeMismatch) => "program_attachment_patch_scope_mismatch",
            E::Sink(S::StampMismatch) => "program_attachment_stamp_mismatch",
            E::Sink(S::RevisionMismatch) => "program_attachment_revision_mismatch",
            E::Sink(S::AlreadyInstalled) => "program_attachment_already_installed",
            E::Sink(S::Host(H::Rejected)) => "program_attachment_host_rejected",
            E::Sink(S::Host(H::Protocol)) => "program_attachment_host_protocol",
            _ => "program_attachment_update_unknown",
        },
        operation,
    )
}

fn authority_error(error: core::ProgramMaterializationAuthorityErrorV1) -> LabColorsError {
    use core::ProgramMaterializationAuthorityErrorV1 as E;
    LabColorsError::new(
        match error {
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
        },
        "materializationAuthority",
    )
}

fn observation<'a>(
    ids: &'a [u32],
    surfaces: &'a [u8],
    count: u32,
    operation: &str,
) -> Result<core::ProgramObservationV1<'a>, LabColorsError> {
    core::ProgramObservationV1::from_rgb8(ids, surfaces, count as usize).map_err(|_| {
        LabColorsError::new(
            if operation == "attachmentUpdateObserved" {
                "program_attachment_update"
            } else {
                "program_update"
            },
            operation,
        )
    })
}

#[derive(uniffi::Object)]
pub struct ProgramRuntime {
    inner: Mutex<core::ProgramSessionV1>,
}

/// Принимает подготовленный снимок после атомарного изменения принадлежащего
/// приложению контекста. false/Err сохраняют прежнюю принятую ревизию runtime.
/// Callback вызывается синхронно на потоке update и может сохранить снимок.
#[uniffi::export(callback_interface)]
pub trait ProgramUpdateHost: Send + Sync {
    fn accept(&self, snapshot: Arc<ProgramSnapshot>) -> Result<bool, LabColorsError>;
}

#[uniffi::export]
pub fn compile_program_wire(
    bytes: Vec<u8>,
    stream_id: u32,
) -> Result<Arc<ProgramRuntime>, LabColorsError> {
    let operation = "compileProgramWire";
    let owner = core::compile_program_wire(&bytes).map_err(|e| runtime_error(e, operation))?;
    let session = owner
        .instantiate(stream_id)
        .map_err(|e| runtime_error(e, operation))?;
    Ok(Arc::new(ProgramRuntime {
        inner: Mutex::new(session),
    }))
}

#[uniffi::export]
impl ProgramRuntime {
    pub fn update_observed(
        &self,
        revision: u64,
        scenario_ids: Vec<u32>,
        surfaces: Vec<u8>,
        surface_count: u32,
    ) -> Result<Arc<ProgramSnapshot>, LabColorsError> {
        let operation = "updateObserved";
        let mut inner = enter(&self.inner, operation, "program_busy")?;
        let values = observation(&scenario_ids, &surfaces, surface_count, operation)?;
        inner
            .update_observed_view(revision, values)
            .map(|inner| Arc::new(ProgramSnapshot { inner }))
            .map_err(|e| runtime_error(e, operation))
    }

    /// Поиск, final recheck и создание owned-снимка завершаются до callback.
    /// true принимает уже подготовленный переход; false сохраняет прежний.
    /// Host удерживает нужный ему снимок из callback. Примитивный bool проходит
    /// UniFFI без сериализации RustBuffer после принятия внешней записи.
    pub fn update_observed_with(
        &self,
        revision: u64,
        scenario_ids: Vec<u32>,
        surfaces: Vec<u8>,
        surface_count: u32,
        accept: Box<dyn ProgramUpdateHost>,
    ) -> Result<bool, LabColorsError> {
        let operation = "updateObservedWith";
        let mut inner = enter(&self.inner, operation, "program_busy")?;
        let values = observation(&scenario_ids, &surfaces, surface_count, operation)?;
        let prepared = inner
            .prepare_observed_view(revision, values)
            .map_err(|error| runtime_error(error, operation))?;
        prepared.commit_with(|inner| {
            let candidate = Arc::new(ProgramSnapshot { inner });
            // A Rust callback can unwind in unwind-enabled builds. Foreign
            // exceptions use typed UniFFI conversion independently of this guard.
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| accept.accept(candidate)))
                .map_err(|_| LabColorsError::new("program_host_protocol", operation))?
        })
    }

    pub fn update_unknown(
        &self,
        revision: u64,
        reason_id: u32,
    ) -> Result<Arc<ProgramSnapshot>, LabColorsError> {
        let operation = "updateUnknown";
        enter(&self.inner, operation, "program_busy")?
            .update_unknown(revision, reason_id)
            .map(|inner| Arc::new(ProgramSnapshot { inner }))
            .map_err(|e| runtime_error(e, operation))
    }
}

#[derive(uniffi::Object)]
pub struct ProgramSnapshot {
    inner: core::ProgramSnapshotV1,
}

fn output(
    snapshot: &core::ProgramSnapshotV1,
    index: u32,
) -> Result<core::ProgramPaintOutputV1, LabColorsError> {
    snapshot
        .outputs()
        .get(index as usize)
        .copied()
        .ok_or_else(|| LabColorsError::new("invalid_output_index", "readProgramOutput"))
}

fn output_table_storage(output_count: usize) -> Result<Vec<f64>, LabColorsError> {
    let exhausted = || {
        runtime_error(
            core::ProgramRuntimeErrorV1::ResourceExhausted,
            "copyOutputTable",
        )
    };
    let length = output_count.checked_mul(5).ok_or_else(exhausted)?;
    let mut table = Vec::new();
    table.try_reserve_exact(length).map_err(|_| exhausted())?;
    Ok(table)
}

fn copy_snapshot_outputs(snapshot: &core::ProgramSnapshotV1) -> Result<Vec<f64>, LabColorsError> {
    let mut table = output_table_storage(snapshot.outputs().len())?;
    for output in snapshot.outputs() {
        let [red, green, blue] = output.source().bytes();
        table.extend_from_slice(&[
            f64::from(output.slot()),
            f64::from(red),
            f64::from(green),
            f64::from(blue),
            output.opacity(),
        ]);
    }
    Ok(table)
}

#[uniffi::export]
impl ProgramSnapshot {
    pub fn snapshot_version(&self) -> u32 {
        self.inner.snapshot_version()
    }
    pub fn revision(&self) -> u64 {
        self.inner.revision()
    }
    pub fn reason(&self) -> Option<String> {
        self.inner.reason().map(str::to_owned)
    }
    pub fn optimality(&self) -> Option<String> {
        self.inner.optimality().map(|value| value.key().to_owned())
    }
    /// Независимая типизированная копия финальных оценок текущего Ready.
    pub fn assessment_report(
        &self,
    ) -> Result<Option<crate::ProgramAssessmentReport>, LabColorsError> {
        crate::program_reports::assessment_report(self.inner.assessment_report())
    }
    /// Та же Core-сводка без копирования полного массива оценок.
    pub fn cleanliness_assessment(
        &self,
    ) -> Result<Option<crate::ProgramCleanlinessAssessment>, LabColorsError> {
        self.inner
            .cleanliness_assessment()
            .map(|assessment| {
                crate::program_reports::cleanliness_assessment(assessment, "cleanlinessAssessment")
            })
            .transpose()
    }
    /// Независимая типизированная копия witness текущего отказа или поиска.
    pub fn diagnostics(&self) -> Result<Option<crate::ProgramDiagnostics>, LabColorsError> {
        crate::program_reports::diagnostics(self.inner.diagnostics())
    }
    pub fn state(&self) -> String {
        self.inner.state().key().into()
    }
    pub fn output_count(&self) -> u64 {
        self.inner.outputs().len() as u64
    }
    /// Независимая таблица `[slot, red, green, blue, opacity]` в порядке
    /// поштучных геттеров. Целые поля и исходная f64 opacity сохраняются точно.
    pub fn copy_output_table(&self) -> Result<Vec<f64>, LabColorsError> {
        copy_snapshot_outputs(&self.inner)
    }
    pub fn output_slot(&self, index: u32) -> Result<u32, LabColorsError> {
        Ok(output(&self.inner, index)?.slot())
    }
    pub fn output_rgb(&self, index: u32) -> Result<Vec<u8>, LabColorsError> {
        Ok(output(&self.inner, index)?.source().bytes().to_vec())
    }
    pub fn output_opacity(&self, index: u32) -> Result<f64, LabColorsError> {
        Ok(output(&self.inner, index)?.opacity())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ProgramPointSinkOperation {
    SetAll,
    RevokeAll,
    ConfirmExact,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ProgramPointPaint {
    pub slot: u32,
    pub source: Vec<u8>,
    pub opacity: f64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ProgramPointSinkIntent {
    pub operation: ProgramPointSinkOperation,
    pub revision: u64,
    pub expected_sequence: u64,
    pub desired_sequence: u64,
    pub binding_epoch: u64,
    pub sink_output: u32,
    pub point: Option<ProgramPointPaint>,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ProgramPointSinkCallbackError {
    #[error("host callback failed")]
    InvocationFailed,
}

impl From<uniffi::UnexpectedUniFFICallbackError> for ProgramPointSinkCallbackError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::InvocationFailed
    }
}

/// Возвращает true только после атомарной установки всей команды. false и ошибка
/// оставляют прежний внешний scope. Callback исполняется на вызывающем потоке.
#[uniffi::export(callback_interface)]
pub trait ProgramPointSinkHost: Send + Sync {
    fn try_install(
        &self,
        intent: ProgramPointSinkIntent,
    ) -> Result<bool, ProgramPointSinkCallbackError>;
}

struct NativeHost {
    callback: Box<dyn ProgramPointSinkHost>,
}

impl core::ProgramPointSinkHostV1 for NativeHost {
    fn try_install(
        &mut self,
        intent: core::ProgramPointSinkIntentV1,
    ) -> Result<(), core::ProgramPointSinkHostErrorV1> {
        use core::ProgramPointSinkHostErrorV1 as E;
        let operation = match intent.operation() {
            core::ProgramPointSinkOperationV1::SetAll => ProgramPointSinkOperation::SetAll,
            core::ProgramPointSinkOperationV1::RevokeAll => ProgramPointSinkOperation::RevokeAll,
            core::ProgramPointSinkOperationV1::ConfirmExact => {
                ProgramPointSinkOperation::ConfirmExact
            }
            _ => return Err(E::Protocol),
        };
        let point = intent.point().map(|point| ProgramPointPaint {
            slot: point.slot(),
            source: point.source().bytes().to_vec(),
            opacity: point.opacity(),
        });
        let projected = ProgramPointSinkIntent {
            operation,
            revision: intent.revision(),
            expected_sequence: intent.expected_stamp().sequence(),
            desired_sequence: intent.desired_stamp().sequence(),
            binding_epoch: intent.expected_stamp().binding_epoch(),
            sink_output: intent.sink_output(),
            point,
        };
        match self.callback.try_install(projected) {
            Ok(true) => Ok(()),
            Ok(false) => Err(E::Rejected),
            Err(_) => Err(E::Protocol),
        }
    }
}

#[derive(uniffi::Object)]
pub struct ProgramAttachment {
    inner: Mutex<core::ProgramAttachmentV1<NativeHost>>,
}

#[uniffi::export]
pub fn attach_program_wire(
    bytes: Vec<u8>,
    stream_id: u32,
    output_slot: u32,
    sink_output: u32,
    presentation_root: u32,
    occurrence: u32,
    host: Box<dyn ProgramPointSinkHost>,
) -> Result<Arc<ProgramAttachment>, LabColorsError> {
    let operation = "attachProgramWire";
    let owner = core::compile_program_wire(&bytes).map_err(|e| runtime_error(e, operation))?;
    let inner = owner
        .attach(
            stream_id,
            output_slot,
            sink_output,
            presentation_root,
            occurrence,
            NativeHost { callback: host },
        )
        .map_err(|e| attach_error(e, operation))?;
    Ok(Arc::new(ProgramAttachment {
        inner: Mutex::new(inner),
    }))
}

#[uniffi::export]
impl ProgramAttachment {
    pub fn update_observed(
        &self,
        revision: u64,
        scenario_ids: Vec<u32>,
        surfaces: Vec<u8>,
        surface_count: u32,
    ) -> Result<Arc<ProgramAttachedSnapshot>, LabColorsError> {
        let operation = "attachmentUpdateObserved";
        let mut inner = enter(&self.inner, operation, "program_attachment_busy")?;
        let values = observation(&scenario_ids, &surfaces, surface_count, operation)?;
        inner
            .update_observed_view(revision, values)
            .map(|inner| Arc::new(ProgramAttachedSnapshot { inner }))
            .map_err(|e| update_error(e, operation))
    }

    pub fn update_unknown(
        &self,
        revision: u64,
        reason_id: u32,
    ) -> Result<Arc<ProgramAttachedSnapshot>, LabColorsError> {
        let operation = "attachmentUpdateUnknown";
        enter(&self.inner, operation, "program_attachment_busy")?
            .update_unknown(revision, reason_id)
            .map(|inner| Arc::new(ProgramAttachedSnapshot { inner }))
            .map_err(|e| update_error(e, operation))
    }

    pub fn materialization_authority(
        &self,
    ) -> Result<Arc<AttachedMaterializationAuthority>, LabColorsError> {
        enter(
            &self.inner,
            "materializationAuthority",
            "program_attachment_busy",
        )?
        .current_materialization_authority()
        .map(|inner| Arc::new(AttachedMaterializationAuthority { inner }))
        .map_err(authority_error)
    }

    pub fn materialization_authority_for(
        &self,
        revision: u64,
        content_identity: Vec<u8>,
        binding_epoch: u64,
    ) -> Result<Arc<AttachedMaterializationAuthority>, LabColorsError> {
        let inner = enter(
            &self.inner,
            "materializationAuthority",
            "program_attachment_busy",
        )?;
        let render = inner.current_render().ok_or_else(|| {
            authority_error(core::ProgramMaterializationAuthorityErrorV1::NotReady)
        })?;
        let identity: [u8; 32] = content_identity.as_slice().try_into().map_err(|_| {
            authority_error(core::ProgramMaterializationAuthorityErrorV1::StaleIdentity)
        })?;
        let expected = render
            .expectation()
            .with_revision(revision)
            .with_content_identity(identity)
            .with_binding_epoch(binding_epoch);
        inner
            .materialization_authority(
                core::ProgramMaterializationCandidateV1::AttachedCommit,
                expected,
            )
            .map(|inner| Arc::new(AttachedMaterializationAuthority { inner }))
            .map_err(authority_error)
    }

    /// Освобождение памяти через ARC не отзывает внешний scope. Его владелец
    /// сначала отзывает scope, затем передаёт true; false сохраняет attachment.
    pub fn dispose(&self, confirmed: bool) -> Result<(), LabColorsError> {
        let operation = "attachmentDispose";
        enter(&self.inner, operation, "program_attachment_busy")?
            .dispose(|| if confirmed { Ok(()) } else { Err(()) })
            .map_err(|error| {
                LabColorsError::new(
                    match error {
                        core::ProgramDisposeErrorV1::AlreadyDisposed => {
                            "program_attachment_already_disposed"
                        }
                        core::ProgramDisposeErrorV1::Confirmation(()) => {
                            "program_attachment_revoke_unconfirmed"
                        }
                        _ => "program_attachment_dispose",
                    },
                    operation,
                )
            })
    }
}

#[derive(uniffi::Object)]
pub struct ProgramAttachedSnapshot {
    inner: core::ProgramAttachedSnapshotV1,
}

#[uniffi::export]
impl ProgramAttachedSnapshot {
    pub fn state(&self) -> String {
        self.inner.snapshot().state().key().into()
    }
    pub fn output_count(&self) -> u64 {
        self.inner.snapshot().outputs().len() as u64
    }
    /// Независимая таблица `[slot, red, green, blue, opacity]` в порядке
    /// поштучных геттеров. Целые поля и исходная f64 opacity сохраняются точно.
    pub fn copy_output_table(&self) -> Result<Vec<f64>, LabColorsError> {
        copy_snapshot_outputs(self.inner.snapshot())
    }
    pub fn output_slot(&self, index: u32) -> Result<u32, LabColorsError> {
        Ok(output(self.inner.snapshot(), index)?.slot())
    }
    pub fn output_rgb(&self, index: u32) -> Result<Vec<u8>, LabColorsError> {
        Ok(output(self.inner.snapshot(), index)?
            .source()
            .bytes()
            .to_vec())
    }
    pub fn output_opacity(&self, index: u32) -> Result<f64, LabColorsError> {
        Ok(output(self.inner.snapshot(), index)?.opacity())
    }
    pub fn has_render(&self) -> bool {
        self.inner.render().is_some()
    }
    pub fn render(&self) -> Option<Arc<ProgramAttachedRender>> {
        self.inner
            .render()
            .map(|inner| Arc::new(ProgramAttachedRender { inner }))
    }
}

fn physical_identity(
    value: Option<core::ProgramPhysicalIdentityV1>,
) -> Result<String, LabColorsError> {
    match value {
        Some(core::ProgramPhysicalIdentityV1::EncodedSrgb8SourceOverV1) => {
            Ok("encoded-srgb8-source-over-v1".into())
        }
        None => Ok("unknown".into()),
        Some(_) => Err(LabColorsError::new(
            "program_physical_identity",
            "physicalIdentity",
        )),
    }
}

#[derive(uniffi::Object)]
pub struct ProgramAttachedRender {
    inner: core::ProgramAttachedRenderV1,
}

#[uniffi::export]
impl ProgramAttachedRender {
    pub fn output_slot(&self) -> u32 {
        self.inner.output().slot()
    }
    pub fn sink_output(&self) -> u32 {
        self.inner.sink_output()
    }
    pub fn output_rgb(&self) -> Vec<u8> {
        self.inner.output().source().bytes().to_vec()
    }
    pub fn output_opacity(&self) -> f64 {
        self.inner.output().opacity()
    }
    pub fn revision(&self) -> u64 {
        self.inner.revision()
    }
    pub fn sink_sequence(&self) -> u64 {
        self.inner.sink_stamp().sequence()
    }
    pub fn binding_epoch(&self) -> u64 {
        self.inner.sink_stamp().binding_epoch()
    }
    pub fn content_identity(&self) -> Vec<u8> {
        self.inner.content_identity().to_vec()
    }
    pub fn context_identity(&self) -> Vec<u8> {
        self.inner.context().identity_bytes().to_vec()
    }
    pub fn presentation_root(&self) -> u32 {
        self.inner.presentation_root()
    }
    pub fn occurrence(&self) -> u32 {
        self.inner.occurrence()
    }
    pub fn physical_identity(&self) -> Result<String, LabColorsError> {
        physical_identity(self.inner.physical_identity())
    }
    pub fn terminal_composite_rgb(&self) -> Option<Vec<u8>> {
        self.inner
            .terminal_composite()
            .map(|rgb| rgb.bytes().to_vec())
    }
}

#[derive(uniffi::Object)]
pub struct AttachedMaterializationAuthority {
    inner: core::AttachedMaterializationAuthorityV1,
}

#[uniffi::export]
impl AttachedMaterializationAuthority {
    pub fn output_slot(&self) -> u32 {
        self.inner.output().slot()
    }
    pub fn output_rgb(&self) -> Vec<u8> {
        self.inner.output().source().bytes().to_vec()
    }
    pub fn output_opacity(&self) -> f64 {
        self.inner.output().opacity()
    }
    pub fn revision(&self) -> u64 {
        self.inner.revision()
    }
    pub fn sink_sequence(&self) -> u64 {
        self.inner.sink_stamp().sequence()
    }
    pub fn binding_epoch(&self) -> u64 {
        self.inner.sink_stamp().binding_epoch()
    }
    pub fn content_identity(&self) -> Vec<u8> {
        self.inner.content_identity().to_vec()
    }
    pub fn context_identity(&self) -> Vec<u8> {
        self.inner.context().identity_bytes().to_vec()
    }
    pub fn presentation_root(&self) -> u32 {
        self.inner.presentation_root()
    }
    pub fn occurrence(&self) -> u32 {
        self.inner.occurrence()
    }
    pub fn physical_identity(&self) -> Result<String, LabColorsError> {
        physical_identity(self.inner.physical_identity())
    }
    pub fn renderer_provenance(&self) -> Result<String, LabColorsError> {
        match self.inner.renderer_provenance() {
            core::ProgramRendererProvenanceV1::Unverified => Ok("unverified".into()),
            _ => Err(LabColorsError::new(
                "program_materialization_authority",
                "rendererProvenance",
            )),
        }
    }
    pub fn terminal_composite_rgb(&self) -> Vec<u8> {
        self.inner.terminal_composite().bytes().to_vec()
    }
}

#[cfg(test)]
mod output_table_tests {
    use super::*;

    #[test]
    fn output_table_allocation_refuses_element_and_byte_overflow() {
        let byte_overflow = (isize::MAX as usize / std::mem::size_of::<f64>()) / 5 + 1;
        for count in [usize::MAX, byte_overflow] {
            let LabColorsError::Refused { code, operation } =
                output_table_storage(count).expect_err("impossible table must be refused");
            assert_eq!(code, "program_resource_exhausted");
            assert_eq!(operation, "copyOutputTable");
        }
        assert!(
            output_table_storage(0)
                .expect("empty table requires no allocation")
                .is_empty()
        );
    }
}
