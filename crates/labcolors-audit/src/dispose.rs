use crate::types::{Disposition, DispositionedArtifact, RawArtifact};

/// Стадия 2: присвоение диспозиции и нормализация join-key.
///
/// На данном этапе — stub, помечающий каждый артефакт как NotAssessed.
/// Полная реализация будет сопоставлять артефакты с доказательствами
/// из GRAPH-01 и применять правила исключения/дефектности.
pub fn assign_dispositions(artifacts: &[RawArtifact]) -> Vec<DispositionedArtifact> {
    artifacts
        .iter()
        .map(|raw| DispositionedArtifact {
            raw: raw.clone(),
            disposition: Disposition::NotAssessed {
                reason: "scanner scaffold: dispose stage not yet implemented".into(),
                replan_trigger: "AUD-01 dispose implementation".into(),
            },
            normalized_join_key: format!("{}::{}::{}", raw.class.as_str(), raw.module, raw.raw_key),
        })
        .collect()
}

impl crate::types::ArtifactClass {
    /// Человекочитаемое имя класса для join-key и отчётов.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProductionSourceFile => "ProductionSourceFile",
            Self::PublicRustApi => "PublicRustApi",
            Self::PublicExport => "PublicExport",
            Self::Operation => "Operation",
            Self::ConformanceFamily => "ConformanceFamily",
            Self::SemanticBranch => "SemanticBranch",
            Self::PublicClaim => "PublicClaim",
            Self::ResourceDimension => "ResourceDimension",
            Self::DecisionSite => "DecisionSite",
            Self::WasmBoundary => "WasmBoundary",
            Self::NativeBoundary => "NativeBoundary",
            Self::CiBuildReleaseDeclaration => "CiBuildReleaseDeclaration",
            Self::ParallelSsot => "ParallelSsot",
            Self::GraphArtifactTest => "GraphArtifactTest",
        }
    }
}
