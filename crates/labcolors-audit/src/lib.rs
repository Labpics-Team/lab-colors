pub mod dispose;
pub mod enumerate;
pub mod extractors;
mod input;
pub mod types;

pub use input::AuditInputError;

pub use dispose::assign_dispositions;
pub use enumerate::enumerate_production_artifacts;
pub use types::{ArtifactClass, AuditVerdict, Disposition, DispositionedArtifact, RawArtifact};

use std::path::Path;

/// Полный успешный вердикт требует непустого инвентаря без дефектов и пробелов оценки.
pub fn audit_gate(dispositioned: &[DispositionedArtifact]) -> AuditVerdict {
    let total_artifacts = dispositioned.len();
    let orphaned_count = dispositioned
        .iter()
        .filter(|a| matches!(a.disposition, Disposition::Orphaned { .. }))
        .count();
    let defective_count = dispositioned
        .iter()
        .filter(|a| matches!(a.disposition, Disposition::Defective { .. }))
        .count();
    let not_assessed_count = dispositioned
        .iter()
        .filter(|a| matches!(a.disposition, Disposition::NotAssessed { .. }))
        .count();

    AuditVerdict {
        passed: total_artifacts > 0
            && orphaned_count == 0
            && defective_count == 0
            && not_assessed_count == 0,
        orphaned_count,
        defective_count,
        not_assessed_count,
        total_artifacts,
    }
}

/// Точка входа полного пайплайна: enumerate → dispose → gate.
///
/// Удобная обёртка для CLI и интеграционных тестов. Возвращает вердикт
/// и полный список диспозиционированных артефактов для детального отчёта.
pub fn run_full_audit(
    source_root: &Path,
) -> Result<(AuditVerdict, Vec<DispositionedArtifact>), AuditInputError> {
    let raw = enumerate_production_artifacts(source_root)?;
    let dispositioned = assign_dispositions(&raw);
    let verdict = audit_gate(&dispositioned);
    Ok((verdict, dispositioned))
}
