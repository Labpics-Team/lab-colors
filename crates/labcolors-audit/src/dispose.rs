use std::collections::HashSet;

use crate::types::{ArtifactClass, Disposition, DispositionedArtifact, RawArtifact};

/// Стадия 2: присвоение диспозиции и нормализация join-key.
///
/// Для артефактов класса `DecisionSite` проверяет наличие записи в
/// proof-capable V2 registry (`numerical_registry_v2`). Если site
/// зарегистрирован — Covered с evidence_key = site_id.key(). Если нет —
/// Orphaned. Все остальные классы пока помечаются NotAssessed со ссылкой
/// на будущую реализацию сопоставления с GRAPH-01 доказательствами.
pub fn assign_dispositions(artifacts: &[RawArtifact]) -> Vec<DispositionedArtifact> {
    // Build a set of registered V2 site keys for O(1) lookup.
    let registered_sites: HashSet<&'static str> = labcolors_core::numerical_registry_v2()
        .iter()
        .map(|row| row.site_id.key())
        .collect();

    artifacts
        .iter()
        .map(|raw| {
            let normalized_join_key =
                format!("{}::{}::{}", raw.class.as_str(), raw.module, raw.raw_key);

            let disposition = match raw.class {
                ArtifactClass::DecisionSite => {
                    if registered_sites.contains(raw.raw_key.as_str()) {
                        Disposition::Covered {
                            evidence_key: format!("v2-registry::{}", raw.raw_key),
                        }
                    } else {
                        Disposition::Orphaned {
                            reason: format!(
                                "decision site '{}' not found in numerical_registry_v2",
                                raw.raw_key
                            ),
                        }
                    }
                }
                ArtifactClass::ParallelSsot => Disposition::NotAssessed {
                    reason: format!(
                        "parallel SSOT marker '{}' requires cross-reference with docs/empirical-inventory.md",
                        raw.raw_key
                    ),
                    replan_trigger: "AUD-01 EXT-09 ssot-registry integration".into(),
                },
                ArtifactClass::PublicClaim => Disposition::NotAssessed {
                    reason: format!(
                        "public claim '{}' requires verification against test evidence or formal proof",
                        raw.raw_key
                    ),
                    replan_trigger: "AUD-01 EXT-09 claims-registry integration".into(),
                },
                _ => Disposition::NotAssessed {
                    reason: format!(
                        "dispose stage: {} matching not yet implemented",
                        raw.class.as_str()
                    ),
                    replan_trigger: "AUD-01 dispose graph-01 integration".into(),
                },
            };

            DispositionedArtifact {
                raw: raw.clone(),
                disposition,
                normalized_join_key,
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_site_registered_in_v2_is_covered() {
        let artifacts = vec![RawArtifact {
            class: ArtifactClass::DecisionSite,
            module: "numerics".into(),
            line: 430,
            raw_key: "wcag22-srgb8-contrast-v1".into(),
            raw_value: None,
        }];

        let result = assign_dispositions(&artifacts);
        assert_eq!(result.len(), 1);
        assert!(matches!(
            &result[0].disposition,
            Disposition::Covered { evidence_key } if evidence_key == "v2-registry::wcag22-srgb8-contrast-v1"
        ));
    }

    #[test]
    fn decision_site_not_in_v2_is_orphaned() {
        let artifacts = vec![RawArtifact {
            class: ArtifactClass::DecisionSite,
            module: "numerics".into(),
            line: 999,
            raw_key: "nonexistent-site-v1".into(),
            raw_value: None,
        }];

        let result = assign_dispositions(&artifacts);
        assert_eq!(result.len(), 1);
        assert!(matches!(
            &result[0].disposition,
            Disposition::Orphaned { reason } if reason.contains("not found in numerical_registry_v2")
        ));
    }

    #[test]
    fn non_decision_site_is_not_assessed() {
        let artifacts = vec![RawArtifact {
            class: ArtifactClass::PublicRustApi,
            module: "solve".into(),
            line: 10,
            raw_key: "solve".into(),
            raw_value: None,
        }];

        let result = assign_dispositions(&artifacts);
        assert_eq!(result.len(), 1);
        assert!(matches!(
            &result[0].disposition,
            Disposition::NotAssessed { .. }
        ));
    }
}
