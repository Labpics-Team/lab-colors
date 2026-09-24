#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "TQ-01 остаётся внутренним provider до EVAL-01; transport export запрещён r15"
    )
)]

//! Единственный Core-владелец TechnicalQuality AUTH V1.
//!
//! TQ подтверждает только уже доказанный результат: текущую point-materialization
//! attachment либо свежий exact-reference replay полного поля. Он не запускает
//! evaluator повторно, не сканирует растр и не превращает digest в доказательство.

use super::{
    AuthorityAdmissionErrorV1, AuthorityAdmissionOutcomeV1, AuthorityDescriptorV1,
    AuthorityExpectedCurrentV1, AuthorityIdV1, AuthorityOwnerCurrentV1, AuthorityPermitErrorV1,
    AuthorityStateV1, RendererObservationRequirementV1, issue_permit,
};
use crate::field_effect::{
    FieldCertificateReplayErrorV1, FieldEvaluationRequestV1, FieldExactReferenceReplayV1,
    FieldWholeRasterCertificateV1, verify_exact_reference_for_tq,
};
use crate::observation::ObservationHeadViewV1;
use crate::program_wire::{
    AttachedMaterializationAuthorityV1, ProgramAttachmentV1,
    ProgramMaterializationAuthorityErrorV1, ProgramPhysicalIdentityV1, ProgramPointSinkHostV1,
    ProgramRendererProvenanceV1,
};
use crate::session::{Session, SessionPlanV1};
use crate::sha256::Hasher;

/// Типизированные отказы единственного TQ admission path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TechnicalQualityAdmissionErrorV1 {
    /// Текущий attachment не может выдать точную materialization authority.
    Materialization(ProgramMaterializationAuthorityErrorV1),
    /// Point path поддерживает только доказанный физический профиль source-over.
    UnsupportedPhysicalIdentity,
    /// Exact-reference certificate/request не совпал с текущим field owner.
    FieldReplay(FieldCertificateReplayErrorV1),
    /// Текущий Session head не содержит наблюдаемой сцены.
    CurrentFieldSceneUnavailable,
    /// Fresh owner permit не совпал с только что проверенным proof.
    Permit(AuthorityPermitErrorV1),
    /// AUTH отклонил CAS/branch admission.
    Authority(AuthorityAdmissionErrorV1),
}

impl From<ProgramMaterializationAuthorityErrorV1> for TechnicalQualityAdmissionErrorV1 {
    /// Сохраняет точный вид отказа исходного владельца.
    fn from(value: ProgramMaterializationAuthorityErrorV1) -> Self {
        Self::Materialization(value)
    }
}

impl From<FieldCertificateReplayErrorV1> for TechnicalQualityAdmissionErrorV1 {
    /// Сохраняет точный вид отказа исходного владельца.
    fn from(value: FieldCertificateReplayErrorV1) -> Self {
        Self::FieldReplay(value)
    }
}

impl From<AuthorityPermitErrorV1> for TechnicalQualityAdmissionErrorV1 {
    /// Сохраняет точный вид отказа исходного владельца.
    fn from(value: AuthorityPermitErrorV1) -> Self {
        Self::Permit(value)
    }
}

impl From<AuthorityAdmissionErrorV1> for TechnicalQualityAdmissionErrorV1 {
    /// Сохраняет точный вид отказа исходного владельца.
    fn from(value: AuthorityAdmissionErrorV1) -> Self {
        Self::Authority(value)
    }
}

/// Закрытое доказательство TQ. Ни одна ветвь не принимает caller-authored digest.
enum TechnicalQualityProofV1<'proof> {
    ModeledPointV1(ModeledPointProofV1),
    ExactReferenceFieldV1(FieldExactReferenceReplayV1<'proof>),
}

struct ModeledPointProofV1 {
    materialization: AttachedMaterializationAuthorityV1,
}

impl TechnicalQualityProofV1<'_> {
    /// Связывает одну ветвь TQ с точными данными уже проверенного доказательства.
    fn descriptor(&self) -> AuthorityDescriptorV1 {
        let release = proof_identity(b"labcolors.tq.release.v1\0", self);
        let applicability = proof_identity(b"labcolors.tq.applicability.v1\0", self);
        let provenance = proof_identity(b"labcolors.tq.provenance.v1\0", self);
        AuthorityDescriptorV1::from_verified_owner(
            AuthorityIdV1::TechnicalQuality,
            release,
            applicability,
            provenance,
        )
    }

    /// Связывает только данные закрытого доказательства; не оценивает пиксели.
    fn hash_identity_material(&self, hasher: &mut Hasher) {
        match self {
            Self::ModeledPointV1(proof) => proof.hash_identity_material(hasher),
            Self::ExactReferenceFieldV1(proof) => {
                hasher.update(b"exact-reference-field-v1\0");
                proof.hash_tq_identity_material(hasher);
            }
        }
    }
}

impl ModeledPointProofV1 {
    /// Перечитывает attachment и отклоняет недоказанный физический профиль.
    fn from_current<H>(
        attachment: &ProgramAttachmentV1<H>,
    ) -> Result<Self, TechnicalQualityAdmissionErrorV1>
    where
        H: ProgramPointSinkHostV1,
    {
        let materialization = attachment.current_materialization_authority()?;
        if materialization.physical_identity()
            != Some(ProgramPhysicalIdentityV1::EncodedSrgb8SourceOverV1)
        {
            return Err(TechnicalQualityAdmissionErrorV1::UnsupportedPhysicalIdentity);
        }
        Ok(Self { materialization })
    }

    /// Связывает только данные закрытого доказательства; не оценивает пиксели.
    fn hash_identity_material(&self, hasher: &mut Hasher) {
        let materialization = self.materialization;
        let output = materialization.output();
        let sink_stamp = materialization.sink_stamp();

        hasher.update(b"modeled-point-v1\0");
        hasher.update(&materialization.content_identity());
        hasher.update(&[1]); // EncodedSrgb8SourceOverV1 проверен до создания доказательства.
        hasher.update(&materialization.revision().to_be_bytes());
        hasher.update(&sink_stamp.sequence().to_be_bytes());
        hasher.update(&sink_stamp.binding_epoch().to_be_bytes());
        hasher.update(&materialization.presentation_root().to_be_bytes());
        hasher.update(&materialization.occurrence().to_be_bytes());
        hasher.update(&materialization.context().identity_bytes());
        hasher.update(&[match materialization.renderer_provenance() {
            ProgramRendererProvenanceV1::Unverified => 0,
        }]);
        hasher.update(&output.slot().to_be_bytes());
        hasher.update(&output.source().bytes());
        hasher.update(&output.opacity().to_bits().to_be_bytes());
        hasher.update(&materialization.terminal_composite().bytes());
    }
}

/// Разделяет выпуск, применимость и происхождение по доменам одного доказательства.
fn proof_identity(domain: &[u8], proof: &TechnicalQualityProofV1<'_>) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(domain);
    proof.hash_identity_material(&mut hasher);
    *hasher.finalize().as_bytes()
}

impl AuthorityStateV1 {
    /// Допускает TQ только из current committed point attachment.
    ///
    /// Сохранённая ранее `AttachedMaterializationAuthorityV1` не принимается:
    /// current head attachment читается внутри этого вызова непосредственно
    /// перед выпуском fresh owner permit.
    pub(crate) fn admit_modeled_point_technical_quality<H>(
        &mut self,
        attachment: &ProgramAttachmentV1<H>,
        expected: AuthorityExpectedCurrentV1,
    ) -> Result<AuthorityAdmissionOutcomeV1, TechnicalQualityAdmissionErrorV1>
    where
        H: ProgramPointSinkHostV1,
    {
        let proof =
            TechnicalQualityProofV1::ModeledPointV1(ModeledPointProofV1::from_current(attachment)?);
        self.admit_technical_quality_proof(proof, expected)
    }

    /// Допускает TQ полного поля только после синхронной replay-проверки
    /// exact-reference certificate против текущего Session head. Сырой
    /// сертификат не превращается в proof: `field_effect` выдаёт закрытый
    /// replay-result внутри этого вызова непосредственно перед AUTH admission.
    pub(crate) fn admit_exact_reference_field_technical_quality<Plan>(
        &mut self,
        certificate: &FieldWholeRasterCertificateV1,
        request: &FieldEvaluationRequestV1<'_>,
        session: &Session<Plan>,
        expected: AuthorityExpectedCurrentV1,
    ) -> Result<AuthorityAdmissionOutcomeV1, TechnicalQualityAdmissionErrorV1>
    where
        Plan: SessionPlanV1,
    {
        let current_observation = match session.raw_head() {
            ObservationHeadViewV1::Observed(observation) => observation,
            ObservationHeadViewV1::Empty | ObservationHeadViewV1::Unknown(_) => {
                return Err(TechnicalQualityAdmissionErrorV1::CurrentFieldSceneUnavailable);
            }
        };
        let replay = verify_exact_reference_for_tq(certificate, request, current_observation)?;
        let proof = TechnicalQualityProofV1::ExactReferenceFieldV1(replay);
        self.admit_technical_quality_proof(proof, expected)
    }

    /// Выдаёт одноразовое разрешение из закрытого доказательства; отказ сохраняет состояние AUTH.
    fn admit_technical_quality_proof(
        &mut self,
        proof: TechnicalQualityProofV1<'_>,
        expected: AuthorityExpectedCurrentV1,
    ) -> Result<AuthorityAdmissionOutcomeV1, TechnicalQualityAdmissionErrorV1> {
        let descriptor = proof.descriptor();
        let owner_current = AuthorityOwnerCurrentV1::new(descriptor);
        let permit = issue_permit(
            &owner_current,
            descriptor,
            true,
            RendererObservationRequirementV1::NotRequired,
            ProgramRendererProvenanceV1::Unverified,
        )?;
        self.admit(descriptor, expected, permit).map_err(Into::into)
    }
}

#[cfg(test)]
#[path = "technical_quality_tests.rs"]
mod tests;
