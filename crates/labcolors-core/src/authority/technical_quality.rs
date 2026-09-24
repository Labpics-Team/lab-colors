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
use crate::field_effect::FieldExactReferenceReplayV1;
use crate::program_wire::{
    AttachedMaterializationAuthorityV1, ProgramAttachmentV1,
    ProgramMaterializationAuthorityErrorV1, ProgramPhysicalIdentityV1, ProgramPointSinkHostV1,
    ProgramRendererProvenanceV1,
};
use crate::sha256::Hasher;

/// Типизированные отказы единственного TQ admission path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TechnicalQualityAdmissionErrorV1 {
    /// Текущий attachment не может выдать точную materialization authority.
    Materialization(ProgramMaterializationAuthorityErrorV1),
    /// Point path поддерживает только доказанный физический профиль source-over.
    UnsupportedPhysicalIdentity,
    /// Fresh owner permit не совпал с только что проверенным proof.
    Permit(AuthorityPermitErrorV1),
    /// AUTH отклонил CAS/branch admission.
    Authority(AuthorityAdmissionErrorV1),
}

impl From<ProgramMaterializationAuthorityErrorV1> for TechnicalQualityAdmissionErrorV1 {
    fn from(value: ProgramMaterializationAuthorityErrorV1) -> Self {
        Self::Materialization(value)
    }
}

impl From<AuthorityPermitErrorV1> for TechnicalQualityAdmissionErrorV1 {
    fn from(value: AuthorityPermitErrorV1) -> Self {
        Self::Permit(value)
    }
}

impl From<AuthorityAdmissionErrorV1> for TechnicalQualityAdmissionErrorV1 {
    fn from(value: AuthorityAdmissionErrorV1) -> Self {
        Self::Authority(value)
    }
}

/// Закрытое доказательство TQ. Ни одна ветвь не принимает caller-authored digest.
enum TechnicalQualityProofV1 {
    ModeledPointV1(ModeledPointProofV1),
    ExactReferenceFieldV1(ExactReferenceFieldProofV1),
}

struct ModeledPointProofV1 {
    materialization: AttachedMaterializationAuthorityV1,
}

struct ExactReferenceFieldProofV1 {
    identity_material: [[u8; 32]; 3],
}

impl TechnicalQualityProofV1 {
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

    fn hash_identity_material(&self, hasher: &mut Hasher) {
        match self {
            Self::ModeledPointV1(proof) => proof.hash_identity_material(hasher),
            Self::ExactReferenceFieldV1(proof) => {
                hasher.update(b"exact-reference-field-v1\0");
                for identity in proof.identity_material {
                    hasher.update(&identity);
                }
            }
        }
    }
}

impl ModeledPointProofV1 {
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

    fn hash_identity_material(&self, hasher: &mut Hasher) {
        let materialization = self.materialization;
        let output = materialization.output();
        let sink_stamp = materialization.sink_stamp();

        hasher.update(b"modeled-point-v1\0");
        hasher.update(&materialization.content_identity());
        hasher.update(&[1]); // EncodedSrgb8SourceOverV1, checked before construction.
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

impl ExactReferenceFieldProofV1 {
    fn from_replay(replay: FieldExactReferenceReplayV1<'_, '_>) -> Self {
        // Keep three independently domain-separated copies of the exact replay
        // tuple. The sealed replay object, not these digests, is the proof.
        let identity_material = [
            field_replay_identity(b"labcolors.tq.field.release-material.v1\0", &replay),
            field_replay_identity(b"labcolors.tq.field.applicability-material.v1\0", &replay),
            field_replay_identity(b"labcolors.tq.field.provenance-material.v1\0", &replay),
        ];
        Self { identity_material }
    }
}

fn proof_identity(domain: &[u8], proof: &TechnicalQualityProofV1) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(domain);
    proof.hash_identity_material(&mut hasher);
    *hasher.finalize().as_bytes()
}

fn field_replay_identity(domain: &[u8], replay: &FieldExactReferenceReplayV1<'_, '_>) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(domain);
    replay.hash_tq_identity_material(&mut hasher);
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

    /// Допускает TQ полного поля только из свежего sealed replay-result,
    /// выданного `field_effect` для exact-reference whole-raster certificate.
    pub(crate) fn admit_exact_reference_field_technical_quality(
        &mut self,
        replay: FieldExactReferenceReplayV1<'_, '_>,
        expected: AuthorityExpectedCurrentV1,
    ) -> Result<AuthorityAdmissionOutcomeV1, TechnicalQualityAdmissionErrorV1> {
        let proof = TechnicalQualityProofV1::ExactReferenceFieldV1(
            ExactReferenceFieldProofV1::from_replay(replay),
        );
        self.admit_technical_quality_proof(proof, expected)
    }

    fn admit_technical_quality_proof(
        &mut self,
        proof: TechnicalQualityProofV1,
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
mod tests;
