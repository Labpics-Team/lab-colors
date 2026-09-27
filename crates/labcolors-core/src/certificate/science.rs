#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "внутренний сертификат для следующего CLI-EVALUATE; публичный транспорт не расширяется"
    )
)]

//! Сертификат объявленного точечного профиля, связанный с текущим EVAL.
//! Целостные байты остаются недоверенными без живого результата того же профиля.
//! Подписей отправителя и эмпирического человеческого подтверждения здесь нет.
//!
//! Сырые данные не дают доступ к внутреннему производителю:
//! ```compile_fail
//! use labcolors_core::certificate::science::DeclaredPointCertificateV1;
//! ```

use super::{
    AdmissionKeyV1, CertificateEnvelopeV1, CertificateErrorV1, CertificateProducerErrorV1,
    EnvelopeClassV1, UntrustedEnvelopeV1, attest_bytes, compare_expected, source,
};
use crate::authority::AuthorityIdV1;
use crate::authority::evaluation::{CurrentPointEvaluationV1, HumanEvidenceEvaluationV1};
use core::fmt;

const PAYLOAD_BYTES: usize = 302;
const PAYLOAD_HEADER: &[u8; 6] = b"LCPQ\x00\x01";
const CONTEXT_PREFIX: &str = "declared-modeled-point-v1:";

/// Статическая диагностика без данных источников, байтов и идентификаторов.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointCertificateErrorV1 {
    Producer(CertificateProducerErrorV1),
    Envelope(CertificateErrorV1),
    InvalidPointPayload,
    DifferentCurrentEvaluation,
}

impl PointCertificateErrorV1 {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Producer(error) => error.code(),
            Self::Envelope(error) => error.code(),
            Self::InvalidPointPayload => "invalid_declared_point_payload",
            Self::DifferentCurrentEvaluation => "different_current_evaluation",
        }
    }
}
impl fmt::Debug for PointCertificateErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
impl fmt::Display for PointCertificateErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
impl From<CertificateErrorV1> for PointCertificateErrorV1 {
    fn from(error: CertificateErrorV1) -> Self {
        Self::Envelope(error)
    }
}
impl From<CertificateProducerErrorV1> for PointCertificateErrorV1 {
    fn from(error: CertificateProducerErrorV1) -> Self {
        Self::Producer(error)
    }
}

/// Полезная нагрузка не имеет внешнего конструктора из произвольных полей.
struct DeclaredPointPayloadV1([u8; PAYLOAD_BYTES]);

impl DeclaredPointPayloadV1 {
    fn from_evaluation(
        value: &CurrentPointEvaluationV1<'_>,
    ) -> Result<Self, PointCertificateErrorV1> {
        let profile = value.profile();
        let selection = profile.convention();
        let human = match value.human_evidence() {
            HumanEvidenceEvaluationV1::NotRequested => 0,
        };
        let technical = value.technical();
        let convention = value.convention();
        let technical_kind = match technical.id() {
            AuthorityIdV1::TechnicalQuality => 1,
            _ => return Err(PointCertificateErrorV1::InvalidPointPayload),
        };
        let convention_kind = match convention.id() {
            AuthorityIdV1::CleanConvention => 2,
            _ => return Err(PointCertificateErrorV1::InvalidPointPayload),
        };
        let mut bytes = [0; PAYLOAD_BYTES];
        let profile_id = profile.identity();
        let release = selection.release();
        let classes = [selection.scope() as u8, selection.admission() as u8, human];
        let subject = value.subject_identity();
        let tq_release = technical.release_identity();
        let tq_scope = technical.applicability_identity();
        let tq_provenance = technical.provenance_identity();
        let cc_release = convention.release_identity();
        let cc_scope = convention.applicability_identity();
        let cc_provenance = convention.provenance_identity();
        let color = value.materialization().terminal_composite().bytes();
        let mut offset = 0;
        for part in [
            PAYLOAD_HEADER.as_slice(),
            &profile_id,
            &release,
            &classes,
            &subject,
            &[technical_kind],
            tq_release.as_bytes(),
            tq_scope.as_bytes(),
            tq_provenance.as_bytes(),
            &[convention_kind],
            cc_release.as_bytes(),
            cc_scope.as_bytes(),
            cc_provenance.as_bytes(),
            &color,
        ] {
            let target = bytes
                .get_mut(offset..offset + part.len())
                .ok_or(PointCertificateErrorV1::InvalidPointPayload)?;
            target.copy_from_slice(part);
            offset += part.len();
        }
        if offset != PAYLOAD_BYTES {
            return Err(PointCertificateErrorV1::InvalidPointPayload);
        }
        Ok(Self(bytes))
    }
}

fn expected_key(
    evaluation: &CurrentPointEvaluationV1<'_>,
) -> Result<AdmissionKeyV1, PointCertificateErrorV1> {
    let mut context = [0_u8; CONTEXT_PREFIX.len() + 64];
    context[..CONTEXT_PREFIX.len()].copy_from_slice(CONTEXT_PREFIX.as_bytes());
    for (byte, pair) in evaluation
        .profile()
        .identity()
        .iter()
        .zip(context[CONTEXT_PREFIX.len()..].chunks_exact_mut(2))
    {
        let hex = b"0123456789abcdef";
        pair[0] = hex[usize::from(byte >> 4)];
        pair[1] = hex[usize::from(byte & 15)];
    }
    let context =
        core::str::from_utf8(&context).map_err(|_| PointCertificateErrorV1::InvalidPointPayload)?;
    let mut key = source::current_key(context, evaluation.subject_identity())?;
    key.authority_kind = EnvelopeClassV1::DeclaredPoint.authority();
    key.payload_type = EnvelopeClassV1::DeclaredPoint.payload();
    Ok(key)
}

/// Исходники нельзя изменить до последнего использования выданного сертификата.
/// Его скопированные байты — переносимое свидетельство, но не текущее полномочие.
pub(crate) struct DeclaredPointCertificateV1<'a> {
    envelope: CertificateEnvelopeV1,
    evaluation: &'a CurrentPointEvaluationV1<'a>,
}
impl fmt::Debug for DeclaredPointCertificateV1<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeclaredPointCertificateV1 { .. }")
    }
}
impl<'a> DeclaredPointCertificateV1<'a> {
    pub(crate) fn issue(
        evaluation: &'a CurrentPointEvaluationV1<'a>,
    ) -> Result<Self, PointCertificateErrorV1> {
        let payload = DeclaredPointPayloadV1::from_evaluation(evaluation)?;
        let key = expected_key(evaluation)?;
        let attestation = attest_bytes(key, &payload.0)?;
        let envelope = CertificateEnvelopeV1::issue_bound_bytes(&payload.0, attestation)?;
        Ok(Self {
            envelope,
            evaluation,
        })
    }
    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.envelope.as_bytes()
    }
    pub(crate) fn try_to_bytes(&self) -> Result<Vec<u8>, PointCertificateErrorV1> {
        self.envelope.try_to_bytes().map_err(Into::into)
    }
    pub(crate) fn evaluation(&self) -> &'a CurrentPointEvaluationV1<'a> {
        self.evaluation
    }
}

/// Допуск полученных байтов только относительно данного действующего результата.
pub(crate) struct VerifiedPointCertificateV1<'a> {
    bytes: &'a [u8],
    evaluation: &'a CurrentPointEvaluationV1<'a>,
}
impl fmt::Debug for VerifiedPointCertificateV1<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VerifiedPointCertificateV1 { .. }")
    }
}
impl<'a> VerifiedPointCertificateV1<'a> {
    pub(crate) fn verify(
        bytes: &'a [u8],
        evaluation: &'a CurrentPointEvaluationV1<'a>,
    ) -> Result<Self, PointCertificateErrorV1> {
        // Предельный кортеж LCEN + единственная фиксированная нагрузка + binding digest.
        if bytes.len() > super::MAX_TUPLE_BYTES_V1 + PAYLOAD_BYTES + 32 {
            return Err(CertificateErrorV1::ResourceLimitExceeded.into());
        }
        let decoded = UntrustedEnvelopeV1::decode_class::<true>(bytes)?;
        let payload = decoded.payload_bytes();
        if payload.len() != PAYLOAD_BYTES || !payload.starts_with(PAYLOAD_HEADER) {
            return Err(PointCertificateErrorV1::InvalidPointPayload);
        }
        let key = expected_key(evaluation)?;
        compare_expected(decoded.admission_key(), &key)?;
        let expected = DeclaredPointPayloadV1::from_evaluation(evaluation)?;
        if payload != expected.0 {
            return Err(PointCertificateErrorV1::DifferentCurrentEvaluation);
        }
        Ok(Self { bytes, evaluation })
    }
    /// Заимствует именно проверенные байты; исходный буфер нельзя подменить
    /// до последнего использования этого результата.
    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.bytes
    }
    pub(crate) fn evaluation(&self) -> &'a CurrentPointEvaluationV1<'a> {
        self.evaluation
    }
}

#[cfg(test)]
#[path = "science_tests.rs"]
mod tests;
