//! Одноразовая оценка существующего attachment и выпуск переносимого отчёта.
//!
//! Этот порт собирает вызовы действующих владельцев, не вычисляет цвет заново.
//! Отчёт описывает завершённый вызов. Его RGB и LCEN-байты не являются текущим
//! полномочием после изменения attachment; публичный транспорт не получает SCI.

use crate::Srgb8;
use crate::authority::{AuthorityExpectedCurrentV1, AuthorityStateV1};
use crate::authority::clean_convention::{
    CleanConventionAdmissionKindV1, CleanConventionErrorV1, CleanConventionScopeV1,
    CleanConventionSelectionV1,
};
use crate::authority::evaluation::{PointEvaluationErrorV1, PointQualityProfileV1};
use crate::authority::technical_quality::TechnicalQualityAdmissionErrorV1;
use crate::certificate::science::{DeclaredPointCertificateV1, PointCertificateErrorV1, VerifiedPointCertificateV1};
use crate::program_wire::{ProgramAttachmentV1, ProgramMaterializationAuthorityErrorV1, ProgramPointSinkHostV1};

/// Класс отказа для внешнего адаптера; успешного запасного результата нет.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointEvaluationFailureKindV1 {
    Unsupported,
    Evaluation,
    Resource,
    Internal,
}

/// Закрытая статическая диагностика без входных байтов и пользовательских строк.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointEvaluationFailureV1 {
    kind: PointEvaluationFailureKindV1,
    domain: &'static str,
    code: &'static str,
}

impl PointEvaluationFailureV1 {
    pub const fn kind(self) -> PointEvaluationFailureKindV1 { self.kind }
    pub const fn domain(self) -> &'static str { self.domain }
    pub const fn code(self) -> &'static str { self.code }

    fn new(kind: PointEvaluationFailureKindV1, domain: &'static str, code: &'static str) -> Self {
        Self { kind, domain, code }
    }
}

/// Обычные данные выполненного вызова, не сохраняющее свежесть доказательство.
/// У типа нет конструктора из произвольных полей или сырых LCEN-байтов.
#[derive(Debug, PartialEq, Eq)]
pub struct DeclaredPointReportV1 {
    rgb: Srgb8,
    revision: u64,
    program_identity: [u8; 32],
    profile_identity: [u8; 32],
    convention_release: [u8; 32],
    subject_identity: [u8; 32],
    certificate: Vec<u8>,
}

impl DeclaredPointReportV1 {
    pub const fn terminal_srgb8(&self) -> Srgb8 { self.rgb }
    pub const fn revision(&self) -> u64 { self.revision }
    pub const fn program_identity(&self) -> &[u8; 32] { &self.program_identity }
    pub const fn profile_identity(&self) -> &[u8; 32] { &self.profile_identity }
    pub const fn convention_release(&self) -> &[u8; 32] { &self.convention_release }
    pub const fn subject_identity(&self) -> &[u8; 32] { &self.subject_identity }
    /// Скопированный пакет снова недоверенный и проверяется с текущим EVAL.
    pub fn certificate_bytes(&self) -> &[u8] { &self.certificate }
}

/// Оценивает одну текущую моделируемую sRGB8-точку по явно выбранному выпуску.
///
/// Вход не принимает старые квитанции или внешний AUTH. Обе ветви выпускаются
/// из текущего attachment и проверяются прежним EVAL. Область только Declared;
/// человеческое доказательство не запрошено, renderer остаётся Unverified.
/// Функция не вызывает sink, не меняет attachment и не сохраняет AUTH.
pub fn evaluate_declared_point_v1<H: ProgramPointSinkHostV1>(
    attachment: &ProgramAttachmentV1<H>,
    convention_release: [u8; 32],
) -> Result<DeclaredPointReportV1, PointEvaluationFailureV1> {
    let selection = CleanConventionSelectionV1::select(
        convention_release,
        CleanConventionScopeV1::ModeledSrgb8Point,
        CleanConventionAdmissionKindV1::DeclaredPackagePolicyCandidate,
    ).map_err(convention_failure)?;
    let mut authority = AuthorityStateV1::new();
    authority.admit_modeled_point_technical_quality(attachment, AuthorityExpectedCurrentV1::Vacant)
        .map_err(technical_failure)?;
    authority.admit_modeled_point_clean_convention(attachment, Some(selection), AuthorityExpectedCurrentV1::Vacant)
        .map_err(convention_failure)?;
    let evaluation = authority.evaluate_declared_modeled_point(
        attachment, Some(PointQualityProfileV1::declared_point(selection)),
    ).map_err(evaluation_failure)?;
    let certificate = DeclaredPointCertificateV1::issue(&evaluation).map_err(certificate_failure)?;
    // Проверяется именно выданный пакет, прежде чем копия уйдёт в недоверенный вывод.
    let verified = VerifiedPointCertificateV1::verify(certificate.as_bytes(), &evaluation)
        .map_err(certificate_failure)?;
    let current = verified.evaluation();
    Ok(DeclaredPointReportV1 {
        rgb: current.materialization().terminal_composite(),
        revision: current.materialization().revision(),
        program_identity: current.materialization().content_identity(),
        profile_identity: current.profile().identity(),
        convention_release: current.profile().convention().release(),
        subject_identity: current.subject_identity(),
        certificate: certificate.try_to_bytes().map_err(certificate_failure)?,
    })
}

fn materialization_failure(domain: &'static str, error: ProgramMaterializationAuthorityErrorV1) -> PointEvaluationFailureV1 {
    use ProgramMaterializationAuthorityErrorV1::*;
    let code = match error {
        NotReady => "materialization_not_ready",
        SourceOrIntermediatePaint => "source_or_intermediate_paint",
        StaleRevision => "stale_revision",
        StaleIdentity => "stale_identity",
        StaleSinkStamp => "stale_sink_stamp",
        ForeignBindingEpoch => "foreign_binding_epoch",
        TerminalBindingMismatch => "terminal_binding_mismatch",
        MissingPointAbsenceProof => "missing_point_absence_proof",
        AmbiguousObservationCases => "ambiguous_observation_cases",
    };
    PointEvaluationFailureV1::new(PointEvaluationFailureKindV1::Evaluation, domain, code)
}

fn technical_failure(error: TechnicalQualityAdmissionErrorV1) -> PointEvaluationFailureV1 {
    use PointEvaluationFailureKindV1 as Kind;
    match error {
        TechnicalQualityAdmissionErrorV1::Materialization(error) => materialization_failure("technical-quality", error),
        TechnicalQualityAdmissionErrorV1::UnsupportedPhysicalIdentity =>
            PointEvaluationFailureV1::new(Kind::Unsupported, "technical-quality", "unsupported_physical_identity"),
        _ => PointEvaluationFailureV1::new(Kind::Internal, "technical-quality", "authority_inconsistent"),
    }
}
fn convention_failure(error: CleanConventionErrorV1) -> PointEvaluationFailureV1 {
    use CleanConventionErrorV1::*;
    use PointEvaluationFailureKindV1 as Kind;
    let (kind, code) = match error {
        UnsupportedRelease => (Kind::Unsupported, "unsupported_convention_release"),
        UnsupportedScope => (Kind::Unsupported, "unsupported_scope"),
        UnsupportedAdmission => (Kind::Unsupported, "unsupported_admission"),
        UnsupportedPhysicalIdentity => (Kind::Unsupported, "unsupported_physical_identity"),
        RejectedByConvention { .. } => (Kind::Evaluation, "rejected_by_convention"),
        Materialization(error) => return materialization_failure("clean-convention", error),
        SelectionRequired => (Kind::Evaluation, "selection_required"),
        Permit(_) | Authority(_) => (Kind::Internal, "authority_inconsistent"),
    };
    PointEvaluationFailureV1::new(kind, "clean-convention", code)
}
fn evaluation_failure(error: PointEvaluationErrorV1) -> PointEvaluationFailureV1 {
    match error {
        PointEvaluationErrorV1::Technical(error) => technical_failure(error),
        PointEvaluationErrorV1::Convention(error) => convention_failure(error),
        _ => PointEvaluationFailureV1::new(PointEvaluationFailureKindV1::Internal, "evaluation", "authority_inconsistent"),
    }
}
fn certificate_failure(error: PointCertificateErrorV1) -> PointEvaluationFailureV1 {
    let kind = if error.code() == "resource_limit_exceeded" {
        PointEvaluationFailureKindV1::Resource
    } else {
        PointEvaluationFailureKindV1::Evaluation
    };
    PointEvaluationFailureV1::new(kind, "certificate", error.code())
}

#[cfg(test)]
#[path = "point_evaluation_tests.rs"]
mod tests;
