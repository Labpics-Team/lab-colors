//! Связывание доверенного certificate с адресом спрошенного определения.
//!
//! `contextual_region` отвечает, какой content address имеет спрошенный регион.
//! `family_artifact` отвечает, что байты точно соответствуют доверенному
//! certificate. Ни одна из границ не отвечает на вопрос «а этот certificate
//! вообще про спрошенный регион?»: целый artifact ДРУГОГО региона проходит обе
//! проверки. Модуль замыкает цепь одним сравнением двух адресов.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "V5b2b keeps the definition-bound admission private until the public family provider cutover"
    )
)]

use core::fmt;

use crate::contextual_region::{
    ContextualRegionFamilyProviderV1, ContextualRegionPipelineV1, PiecewiseLinearCartesianTubeV1,
};
use crate::family::FamilyDefinitionDigestV2;
use crate::family_artifact::{
    AdmittedFamilyArtifactV2, EncodedFamilyArtifactV2, FamilyArtifactLoadErrorV1,
    FamilyArtifactLoaderV1, FamilyImageCertificateV2,
};

/// Причина, по которой artifact не допущен как образ спрошенного региона.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefinitionBoundFamilyLoadErrorV1 {
    /// Certificate доверен и цел, но адресует другое определение.
    ForeignDefinition {
        asked: FamilyDefinitionDigestV2,
        certified: FamilyDefinitionDigestV2,
    },
    /// Certificate про спрошенное определение, но transport его не подтвердил.
    Artifact(FamilyArtifactLoadErrorV1),
}

/// Неуспех возвращает те же owned bytes: диагностика, исправление или повтор
/// не требуют refetch и clone.
#[derive(PartialEq, Eq)]
pub(crate) struct DefinitionBoundFamilyLoadFailureV1 {
    cause: DefinitionBoundFamilyLoadErrorV1,
    encoded: EncodedFamilyArtifactV2,
}

impl fmt::Debug for DefinitionBoundFamilyLoadFailureV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DefinitionBoundFamilyLoadFailureV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}

impl DefinitionBoundFamilyLoadFailureV1 {
    pub(crate) const fn cause(&self) -> DefinitionBoundFamilyLoadErrorV1 {
        self.cause
    }

    pub(crate) fn into_parts(self) -> (DefinitionBoundFamilyLoadErrorV1, EncodedFamilyArtifactV2) {
        (self.cause, self.encoded)
    }
}

/// Единственный вход, допускающий artifact к тому определению, которое
/// потребитель действительно спросил.
pub(crate) struct DefinitionBoundFamilyLoaderV1;

impl DefinitionBoundFamilyLoaderV1 {
    /// Спрошенный регион задаётся `pipeline` и `region`; семейство приходит
    /// извне как доверенный `certificate` и его `encoded` bytes.
    ///
    /// Сравнение адресов идёт по двум записям и завершается до parse envelope,
    /// хеширования payload и decode образа: чужое определение стоит одного
    /// content address, а не мегабайтов.
    pub(crate) fn load(
        pipeline: ContextualRegionPipelineV1,
        region: &PiecewiseLinearCartesianTubeV1,
        certificate: FamilyImageCertificateV2,
        encoded: EncodedFamilyArtifactV2,
    ) -> Result<AdmittedFamilyArtifactV2, DefinitionBoundFamilyLoadFailureV1> {
        let asked = ContextualRegionFamilyProviderV1::definition_digest(pipeline, region);
        let certified = certificate.definition_digest();
        if asked != certified {
            return Err(DefinitionBoundFamilyLoadFailureV1 {
                cause: DefinitionBoundFamilyLoadErrorV1::ForeignDefinition { asked, certified },
                encoded,
            });
        }
        FamilyArtifactLoaderV1::load(certificate, encoded).map_err(|failure| {
            let (cause, encoded) = failure.into_parts();
            DefinitionBoundFamilyLoadFailureV1 {
                cause: DefinitionBoundFamilyLoadErrorV1::Artifact(cause),
                encoded,
            }
        })
    }
}
