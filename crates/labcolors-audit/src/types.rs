use serde::{Deserialize, Serialize};

/// Классы артефактов, подлежащих аудиту.
///
/// Каждый вариант соответствует одному инварианту из плана AUD-01 r5.
/// Имена намеренно дескриптивны: сериализация в JSON используется как
/// человекочитаемый отчёт, а не как wire-format с требованием компактности.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArtifactClass {
    ProductionSourceFile,
    PublicRustApi,
    PublicExport,
    Operation,
    ConformanceFamily,
    SemanticBranch,
    PublicClaim,
    ResourceDimension,
    DecisionSite,
    WasmBoundary,
    NativeBoundary,
    CiBuildReleaseDeclaration,
    ParallelSsot,
    GraphArtifactTest,
}

/// Диспозиция артефакта после стадии dispose.
///
/// Пять вариантов покрывают полную таксономию v2: покрыт доказательством,
/// осиротел (нет доказательства), исключён правилом, дефектен, не оценён
/// с триггером перепланирования. `evidence_key` / `reason` / `rule` /
/// `defect` / `replan_trigger` — строки для человекочитаемого вывода;
/// нормализованный join-key хранится отдельно в `DispositionedArtifact`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Disposition {
    Covered {
        evidence_key: String,
    },
    Orphaned {
        reason: String,
    },
    Excluded {
        rule: String,
    },
    Defective {
        defect: String,
    },
    NotAssessed {
        reason: String,
        replan_trigger: String,
    },
}

/// Сырой артефакт, извлечённый из исходников на стадии enumerate.
///
/// Не несёт диспозиции и нормализованного ключа — это ответственность
/// стадии dispose. `line` обязателен: даже для файловых артефактов
/// используем 1, чтобы сохранить единый тип без Option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawArtifact {
    pub class: ArtifactClass,
    pub module: String,
    pub line: usize,
    pub raw_key: String,
    pub raw_value: Option<String>,
}

/// Артефакт после стадии dispose: сырые данные + присвоенная диспозиция
/// + нормализованный join-key для склейки с доказательствами.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispositionedArtifact {
    pub raw: RawArtifact,
    pub disposition: Disposition,
    pub normalized_join_key: String,
}

/// Вердикт audit_gate: агрегированный результат проверки конформности.
///
/// Считается pass только когда нет Orphaned и Defective.
/// NotAssessed допустим, но выносится в отчёт отдельным списком.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditVerdict {
    pub passed: bool,
    pub orphaned_count: usize,
    pub defective_count: usize,
    pub not_assessed_count: usize,
    pub total_artifacts: usize,
}
