//! Compiled per-invocation numerical execution plan (#289/#292).
//!
//! Execution mode — статическое свойство каждой compiled invocation, а не
//! глобальный профиль package/config. План — canonical DERIVED-проекция
//! compiled invocations (не второй mutable map): resolver исполняет typed mode,
//! сохранённый в самой invocation, и не делает plan lookup в hot path.
//!
//! Identity invocation строится core-owned length-prefixed canonical encoding
//! из opaque node ID bytes, site ID и ЛОКАЛЬНОГО occurrence ordinal внутри пары
//! `(node, site)`. Глобальные declaration/post-sort индексы запрещены:
//! перестановка деклараций и вставка другого node не меняют уже существующие
//! invocation IDs. Canonical порядок относится только к plan-проекции
//! (`invocation_id bytes → site_id`); production declaration/resolution order
//! `NamedRoleTable` не переупорядочивается.
//!
//! Plan checksum — переносимый drift-sentinel (FNV-1a-32, как capability
//! checksum), НЕ security/cache/certificate identity: exact typed projection
//! остаётся authority.

use std::collections::BTreeMap;

use crate::numerics::{
    NumericalCompatibilityReleaseIdV1, NumericalSiteIdV1, push_len_prefixed, registry_row,
};

/// Версия plan-схемы. Независимый version domain (см. #289).
pub const NUMERICAL_PLAN_SCHEMA_VERSION_V1: u32 = 1;

/// Домен-сепараторы canonical encoding.
const INVOCATION_ID_DOMAIN_V1: &[u8] = b"labcolors.numerical-invocation.v1";
const PLAN_CHECKSUM_DOMAIN_V1: &[u8] = b"labcolors.numerical-plan.v1";

/// Typed execution mode одной compiled invocation.
///
/// `Auto`, missing или unresolved legacy choice непредставимы: compiled
/// artifact всегда несёт ровно один явный mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalExecutionModeV1 {
    /// Только доказуемые методы: exact evidence либо typed `Indeterminate`.
    StableOnly,
    /// Явно выбранный зарегистрированный прежний алгоритм.
    ExplicitCompatibility {
        /// Registered compatibility release, который будет исполнен.
        release_id: NumericalCompatibilityReleaseIdV1,
    },
}

impl NumericalExecutionModeV1 {
    /// Стабильный tag mode (release кодируется отдельно).
    pub fn tag(self) -> &'static str {
        match self {
            Self::StableOnly => "stable-only",
            Self::ExplicitCompatibility { .. } => "explicit-compatibility",
        }
    }
}

/// Identity одной compiled invocation: opaque node bytes + site + локальный
/// ordinal. Не зависит от adapter/WASM config fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledInvocationIdV1 {
    node: Vec<u8>,
    site_id: NumericalSiteIdV1,
    ordinal: u32,
}

impl CompiledInvocationIdV1 {
    /// Opaque node ID bytes (core не интерпретирует строку клиента).
    pub fn node_bytes(&self) -> &[u8] {
        &self.node
    }

    /// Site invocation.
    pub fn site_id(&self) -> NumericalSiteIdV1 {
        self.site_id
    }

    /// Локальный occurrence ordinal внутри пары `(node, site)`.
    pub fn ordinal(&self) -> u32 {
        self.ordinal
    }

    /// Canonical length-prefixed identity bytes (versioned контракт).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        push_len_prefixed(&mut buffer, INVOCATION_ID_DOMAIN_V1);
        buffer.extend_from_slice(&NUMERICAL_PLAN_SCHEMA_VERSION_V1.to_le_bytes());
        push_len_prefixed(&mut buffer, &self.node);
        push_len_prefixed(&mut buffer, self.site_id.key().as_bytes());
        buffer.extend_from_slice(&self.ordinal.to_le_bytes());
        buffer
    }
}

/// Одна compiled invocation: identity + site + typed mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledNumericalInvocationV1 {
    /// Identity invocation.
    pub invocation_id: CompiledInvocationIdV1,
    /// Зарегистрированный site.
    pub site_id: NumericalSiteIdV1,
    /// Typed execution mode.
    pub mode: NumericalExecutionModeV1,
}

/// Переносимый drift-checksum canonical plan-проекции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumericalPlanChecksumV1(u32);

impl NumericalPlanChecksumV1 {
    /// FNV-1a-32 canonical preimage.
    pub fn from_preimage(preimage: &[u8]) -> Self {
        Self(crate::hash::fnv1a_32(preimage))
    }

    /// Каноническая 8-hex запись (lowercase).
    pub fn hex(self) -> String {
        format!("{:08x}", self.0)
    }
}

/// Типизированная ошибка компиляции плана — fail closed, не runtime fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalPlanErrorV1 {
    /// Invocation ссылается на незарегистрированный site.
    UnknownSite {
        /// Незарегистрированный site.
        site_id: NumericalSiteIdV1,
    },
    /// Запрошенный compatibility release не зарегистрирован для site.
    UnregisteredCompatibilityRelease {
        /// Site invocation.
        site_id: NumericalSiteIdV1,
        /// Незарегистрированный release.
        release_id: NumericalCompatibilityReleaseIdV1,
    },
    /// Дубликат identity `(node, site, ordinal)`.
    DuplicateInvocation {
        /// Дублированная identity.
        invocation_id: CompiledInvocationIdV1,
    },
}

impl core::fmt::Display for NumericalPlanErrorV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownSite { site_id } => {
                write!(f, "site {} отсутствует в registry V1", site_id.key())
            }
            Self::UnregisteredCompatibilityRelease {
                site_id,
                release_id,
            } => write!(
                f,
                "release {} не зарегистрирован для site {}",
                release_id.key(),
                site_id.key()
            ),
            Self::DuplicateInvocation { invocation_id } => write!(
                f,
                "дубликат numerical invocation: site {} ordinal {}",
                invocation_id.site_id().key(),
                invocation_id.ordinal()
            ),
        }
    }
}

/// Canonical derived plan: invocations в порядке `invocation_id bytes` и
/// drift-checksum. Не участвует в resolve/frame path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledNumericalPlanV1 {
    /// Версия plan-схемы.
    pub schema_version: u32,
    invocations: Vec<CompiledNumericalInvocationV1>,
    /// Drift-checksum canonical проекции.
    pub checksum: NumericalPlanChecksumV1,
}

impl CompiledNumericalPlanV1 {
    /// Invocations в каноническом порядке проекции.
    pub fn invocations(&self) -> &[CompiledNumericalInvocationV1] {
        &self.invocations
    }

    /// Canonical checksum preimage: domain + schema + отсортированные
    /// invocations (identity bytes, site, mode tag, release key; пустой release
    /// кодируется явно нулевой длиной).
    pub fn canonical_checksum_preimage(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        push_len_prefixed(&mut buffer, PLAN_CHECKSUM_DOMAIN_V1);
        buffer.extend_from_slice(&self.schema_version.to_le_bytes());
        buffer.extend_from_slice(&(self.invocations.len() as u32).to_le_bytes());
        for invocation in &self.invocations {
            push_len_prefixed(&mut buffer, &invocation.invocation_id.canonical_bytes());
            push_len_prefixed(&mut buffer, invocation.site_id.key().as_bytes());
            push_len_prefixed(&mut buffer, invocation.mode.tag().as_bytes());
            let release_key = match invocation.mode {
                NumericalExecutionModeV1::StableOnly => "",
                NumericalExecutionModeV1::ExplicitCompatibility { release_id } => release_id.key(),
            };
            push_len_prefixed(&mut buffer, release_key.as_bytes());
        }
        buffer
    }
}

/// Компилирует canonical plan из последовательности numerical occurrences
/// (`opaque node bytes`, site, mode) в порядке production-деклараций.
///
/// Ordinal назначается локально внутри пары `(node, site)` в порядке
/// поступления; сортировка проекции происходит ПОСЛЕ назначения ordinal,
/// поэтому canonical порядок не влияет на identity.
///
/// # Errors
///
/// Типизированная [`NumericalPlanErrorV1`]: незарегистрированный site/release
/// либо дубликат identity.
pub fn compile_numerical_plan_v1<'a>(
    occurrences: impl IntoIterator<Item = (&'a [u8], NumericalSiteIdV1, NumericalExecutionModeV1)>,
) -> Result<CompiledNumericalPlanV1, NumericalPlanErrorV1> {
    let mut ordinals: BTreeMap<(Vec<u8>, &'static str), u32> = BTreeMap::new();
    let mut invocations: Vec<CompiledNumericalInvocationV1> = Vec::new();
    for (node, site_id, mode) in occurrences {
        let row = registry_row(site_id).ok_or(NumericalPlanErrorV1::UnknownSite { site_id })?;
        if let NumericalExecutionModeV1::ExplicitCompatibility { release_id } = mode {
            if !row.compatibility_releases.contains(&release_id) {
                return Err(NumericalPlanErrorV1::UnregisteredCompatibilityRelease {
                    site_id,
                    release_id,
                });
            }
        }
        let slot = ordinals.entry((node.to_vec(), site_id.key())).or_insert(0);
        let invocation_id = CompiledInvocationIdV1 {
            node: node.to_vec(),
            site_id,
            ordinal: *slot,
        };
        *slot += 1;
        invocations.push(CompiledNumericalInvocationV1 {
            invocation_id,
            site_id,
            mode,
        });
    }
    // Canonical порядок проекции: identity bytes → site key.
    // Ключ считается один раз на invocation (identity bytes уже содержат
    // site key и ordinal, поэтому вторичный тайбрейкер был бы избыточен).
    invocations.sort_by_cached_key(|invocation| invocation.invocation_id.canonical_bytes());
    if let Some(pair) = invocations
        .windows(2)
        .find(|pair| pair[0].invocation_id == pair[1].invocation_id)
    {
        // Ordinal назначается билдером, поэтому дубликат недостижим; проверка
        // остаётся fail-closed контрактом против будущих обходных билдеров.
        return Err(NumericalPlanErrorV1::DuplicateInvocation {
            invocation_id: pair[0].invocation_id.clone(),
        });
    }
    let mut plan = CompiledNumericalPlanV1 {
        schema_version: NUMERICAL_PLAN_SCHEMA_VERSION_V1,
        invocations,
        checksum: NumericalPlanChecksumV1::from_preimage(&[]),
    };
    plan.checksum = NumericalPlanChecksumV1::from_preimage(&plan.canonical_checksum_preimage());
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    const SITE: NumericalSiteIdV1 = NumericalSiteIdV1::GlowTargetOrMaximumV1;
    const RELEASE: NumericalCompatibilityReleaseIdV1 =
        NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1;

    fn stable() -> NumericalExecutionModeV1 {
        NumericalExecutionModeV1::StableOnly
    }

    fn compatibility() -> NumericalExecutionModeV1 {
        NumericalExecutionModeV1::ExplicitCompatibility {
            release_id: RELEASE,
        }
    }

    /// Смешанные stable/compatibility invocations одного site сосуществуют
    /// без глобального профиля; ordinal локален внутри `(node, site)`.
    #[test]
    fn mixed_modes_coexist_and_ordinals_are_local() {
        let plan = compile_numerical_plan_v1([
            (b"glow-a".as_slice(), SITE, stable()),
            (b"glow-b".as_slice(), SITE, compatibility()),
            // Synthetic повтор того же (node, site): ordinals 0,1.
            (b"glow-a".as_slice(), SITE, compatibility()),
        ])
        .unwrap();
        assert_eq!(plan.invocations().len(), 3);
        let ordinals_a: Vec<u32> = plan
            .invocations()
            .iter()
            .filter(|inv| inv.invocation_id.node_bytes() == b"glow-a")
            .map(|inv| inv.invocation_id.ordinal())
            .collect();
        assert_eq!(ordinals_a, [0, 1]);
        let ordinal_b: Vec<u32> = plan
            .invocations()
            .iter()
            .filter(|inv| inv.invocation_id.node_bytes() == b"glow-b")
            .map(|inv| inv.invocation_id.ordinal())
            .collect();
        assert_eq!(ordinal_b, [0]);
    }

    /// Перестановка деклараций и вставка другого node не меняют существующие
    /// IDs; canonical проекция/checksum совпадают для A=[z,a] и B=[a,z].
    #[test]
    fn declaration_permutation_preserves_ids_and_canonical_projection() {
        let a = compile_numerical_plan_v1([
            (b"z".as_slice(), SITE, stable()),
            (b"a".as_slice(), SITE, compatibility()),
        ])
        .unwrap();
        let b = compile_numerical_plan_v1([
            (b"a".as_slice(), SITE, compatibility()),
            (b"z".as_slice(), SITE, stable()),
        ])
        .unwrap();
        assert_eq!(a, b);
        assert_eq!(a.checksum, b.checksum);

        // Вставка третьего node не меняет прежние identity bytes.
        let extended = compile_numerical_plan_v1([
            (b"z".as_slice(), SITE, stable()),
            (b"m".as_slice(), SITE, stable()),
            (b"a".as_slice(), SITE, compatibility()),
        ])
        .unwrap();
        let ids = |plan: &CompiledNumericalPlanV1, node: &[u8]| {
            plan.invocations()
                .iter()
                .find(|inv| inv.invocation_id.node_bytes() == node)
                .map(|inv| inv.invocation_id.canonical_bytes())
                .unwrap()
        };
        assert_eq!(ids(&a, b"a"), ids(&extended, b"a"));
        assert_eq!(ids(&a, b"z"), ids(&extended, b"z"));
        assert_ne!(a.checksum, extended.checksum);
    }

    /// Rename opaque node закономерно меняет identity, но не mode semantics;
    /// mode/release mutation меняет проекцию/checksum.
    #[test]
    fn rename_changes_identity_and_mode_mutation_changes_checksum() {
        let original = compile_numerical_plan_v1([(b"glow".as_slice(), SITE, stable())]).unwrap();
        let renamed = compile_numerical_plan_v1([(b"halo".as_slice(), SITE, stable())]).unwrap();
        assert_ne!(
            original.invocations()[0].invocation_id,
            renamed.invocations()[0].invocation_id
        );
        assert_eq!(
            original.invocations()[0].mode,
            renamed.invocations()[0].mode
        );

        let mode_flipped =
            compile_numerical_plan_v1([(b"glow".as_slice(), SITE, compatibility())]).unwrap();
        assert_ne!(original.checksum, mode_flipped.checksum);
    }

    /// Замороженный byte-вектор canonical encoding (versioned контракт v1).
    ///
    /// Encoding — внешний контракт (его независимо переигрывают adapter-оракулы),
    /// поэтому дрейф даже СЕМАНТИЧЕСКИ эквивалентного поля (например, потеря
    /// mode-tag, компенсируемая release-ключом) обязан менять schema version,
    /// а не проходить молча. Вектор сгенерирован из этой же функции при
    /// заморозке v1; изменение допустимо только вместе с bump
    /// NUMERICAL_PLAN_SCHEMA_VERSION_V1 и новым вектором.
    #[test]
    fn canonical_plan_preimage_is_a_frozen_versioned_vector() {
        let plan = compile_numerical_plan_v1([
            (b"a".as_slice(), SITE, stable()),
            (b"z".as_slice(), SITE, compatibility()),
        ])
        .unwrap();
        let preimage = plan.canonical_checksum_preimage();
        let mut hex = String::with_capacity(preimage.len() * 2);
        for byte in preimage {
            write!(&mut hex, "{byte:02x}").expect("writing to String cannot fail");
        }
        let frozen = "1b0000006c6162636f6c6f72732e6e756d65726963616c2d706c616e2e763101000000020000004f000000210000006c6162636f6c6f72732e6e756d65726963616c2d696e766f636174696f6e2e763101000000010000006119000000676c6f772d7461726765742d6f722d6d6178696d756d2d76310000000019000000676c6f772d7461726765742d6f722d6d6178696d756d2d76310b000000737461626c652d6f6e6c79000000004f000000210000006c6162636f6c6f72732e6e756d65726963616c2d696e766f636174696f6e2e763101000000010000007a19000000676c6f772d7461726765742d6f722d6d6178696d756d2d76310000000019000000676c6f772d7461726765742d6f722d6d6178696d756d2d7631160000006578706c696369742d636f6d7061746962696c69747926000000676c6f772d63616d31362d7563732d6a7072696d652d7461726765742d6f722d6d61782d7631";
        assert_eq!(hex, frozen, "canonical plan encoding v1 заморожен");
        assert_eq!(plan.checksum.hex(), "49e5b6b7");
    }

    /// Единственный зарегистрированный release компилируется; негативная
    /// ветвь `UnregisteredCompatibilityRelease` сейчас НЕДОСТИЖИМА снаружи
    /// (в enum один вариант, и он зарегистрирован) — она остаётся vacuous до
    /// появления второго release/site и обязана получить настоящий негативный
    /// тест вместе с ним (#291).
    #[test]
    fn single_registered_release_compiles_and_negative_branch_is_vacuous_for_now() {
        // Единственный зарегистрированный release Glow — проверяем контракт
        // через registry: сам вызов с ним обязан проходить.
        assert!(compile_numerical_plan_v1([(b"glow".as_slice(), SITE, compatibility())]).is_ok());
    }
}
