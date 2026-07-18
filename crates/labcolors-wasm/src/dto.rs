//! Plain-data results of a resolve, independent of any JS representation.
//!
//! These are the binding's *output boundary*: pure Rust structs the engine
//! заполняет из уже допущенного именованного набора, ничего не зная о
//! wasm-bindgen или `js_sys`. Адаптерный слой ([`crate::lib`]) проецирует их в
//! JS-объект. Отсутствие фреймворка делает движок тестируемым нативным
//! `cargo test` и держит стрелку зависимостей внутрь. Имена ролей здесь не
//! перечисляются: каждый ключ принадлежит клиенту.

/// Полный результат резолва одного фона под одной темой. Существует только
/// после того, как ВЕСЬ именованный набор атомарно прошёл допуск.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTheme {
    /// ИСХОДНЫЙ клиентский ключ темы из словаря конфига, под которым решён
    /// набор (результат сохраняет имя клиента, не физический пресет).
    pub theme: String,
    /// The normalised background hex the set was resolved against.
    pub background: String,
    /// One entry per role the core returned, in the core's deterministic order.
    /// The CSS variable name is `--lab-{key}`; the key is `entry.role_key`.
    pub roles: Vec<RoleEntry>,
}

/// One role's outcome, keyed by its stable role key.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleEntry {
    /// The client-owned role key — the CSS-variable stem.
    pub role_key: String,
    /// What the role resolved to.
    pub outcome: RoleOutcome,
}

/// The terminal outcome union for one role, mirroring the core's `Resolved`
/// without leaking the core type across the boundary.
#[derive(Debug, Clone, PartialEq)]
pub enum RoleOutcome {
    /// A solved colour with its measured contrasts and degradation flags.
    Color(SolvedColor),
    /// Явный нулевой токен: цвета здесь нет — намеренно.
    None,
    /// A semi-transparent ladder / alpha-analog role: the emission is
    /// `oklch(L% C H / A)` and the browser composites it; the measured
    /// contrasts are those of the composite on the resolve background.
    Translucent(RgbaColor),
    /// Свечение (kind glow): screen-слои цвета источника +
    /// решённая интенсивность. Потребитель красит слои с
    /// `mix-blend-mode: screen`; `--lab-<role>` несёт halo, `--lab-<role>-core`
    /// — слой пересвета, `--lab-<role>-alpha` — интенсивность числом.
    Glow(GlowColor),
    /// Stable Glow terminal result: no sound numerical decision, no CSS vars.
    GlowIndeterminate(GlowIndeterminateColor),
    /// Двухслойный материал (kind material; whitepaper, «Точечные композиции»): тинт `01` (с выведенной α) +
    /// опаковая база `02`, обе — один тон. `--lab-<role>-01` несёт
    /// `oklch(<tone> / α)`, `--lab-<role>-02` и `--lab-<role>` — `oklch(<tone>)`
    /// (солид-канон/опаковая база); композит-гарантия читаемости — в полях.
    Material(MaterialColor),
    /// Допущенный локальный отказ роли: доказанная недостижимость или
    /// незавершённый bounded search. Rejected/unsupported/internal закрывают
    /// весь резолв и в этом варианте жить не могут.
    Failure {
        /// Core-owned semantic category.
        category: labcolors_core::RoleFailureCategory,
        /// Core-owned stable machine code.
        code: &'static str,
        /// A human-readable explanation (the core's `Display`).
        message: String,
    },
}

/// Слои свечения и решённая интенсивность (kind glow).
#[derive(Debug, Clone, PartialEq)]
pub struct GlowColor {
    /// Core-слой, предназначенный потребителем для меньшего blur, `#RRGGBB`;
    /// сам движок геометрию не моделирует.
    pub core_hex: String,
    /// Halo-слой, предназначенный потребителем для большего blur, `#RRGGBB`;
    /// в recipe v1 равен источнику.
    pub halo_hex: String,
    /// Интенсивность screen-слоя `(0, 1]`.
    pub alpha: f64,
    /// Каноническая CSS-запись той же alpha без повторного округления.
    pub alpha_css: String,
    /// Целевой |ΔJ′| изолированного halo-композита.
    pub target_dj: f64,
    /// Exact point-composite profile, независимо от target/max decision.
    pub composite_profile: labcolors_core::GlowCompositeProfileV1,
    /// Exact point-composite guarantee.
    pub composite_guarantee: labcolors_core::GlowCompositeGuaranteeV1,
    /// Версионированный алгоритм, построивший анатомию core/halo.
    pub layer_recipe_profile: labcolors_core::GlowLayerRecipeProfileV1,
    /// Диагностика внешнего вида изолированных point-слоёв Glow (не whole-effect
    /// сертификат). Обязательна, потому что
    /// `core_achieved_dj` реально вычисляется через CAM16-UCS J′.
    pub appearance_diagnostic_profile: labcolors_core::GlowDiagnosticProfileV1,
    /// Диагностическая модель, участвовавшая именно в выборе target/max. `None` у
    /// точного no-op профиля stable, который не выполняет выбор по внешнему виду.
    pub selection_diagnostic_profile: Option<labcolors_core::GlowDiagnosticProfileV1>,
    /// Атомарный исход решения (#292): доказанный stable exact no-op либо
    /// явный registered compatibility-алгоритм. Прежняя пара независимых полей
    /// profile/guarantee выводится из него boundary-проекциями, поэтому
    /// незаконное сочетание profile × guarantee непредставимо уже в DTO.
    pub decision_outcome: labcolors_core::glow::GlowDecisionOutcomeV1,
    /// Слой, по которому решалась цель.
    pub constraint_layer: labcolors_core::GlowConstraintLayer,
    /// Типизированный результат target-проверки.
    pub target_status: labcolors_core::GlowTargetStatus,
    /// Reference-композит изолированного halo.
    pub halo_composite_hex: String,
    /// Фактический |ΔJ′| изолированного halo-композита.
    pub halo_achieved_dj: f64,
    /// Reference-композит изолированного core с той же alpha.
    pub core_composite_hex: String,
    /// Фактический |ΔJ′| изолированного core-композита.
    pub core_achieved_dj: f64,
}

/// Stable Glow result without a selected semantic state.
#[derive(Debug, Clone, PartialEq)]
pub struct GlowIndeterminateColor {
    /// Canonical source anchor; not emitted as CSS without a decision.
    pub source_hex: String,
    /// Requested diagnostic target.
    pub target_dj: f64,
    /// Explicit stable profile.
    pub decision_profile: labcolors_core::GlowDecisionProfileV1,
    /// Registered branch-sensitive site.
    pub site_id: labcolors_core::NumericalSiteIdV1,
    /// Неразделимая причина вместе с sound interval, если он существует.
    pub evidence: labcolors_core::NumericalIndeterminacyV1,
    /// Layer whose target could not be classified.
    pub constraint_layer: labcolors_core::GlowConstraintLayer,
}

/// Двухслойный материал (kind material): тон + выведенная α + вердикт гарантии.
///
/// Тинт `01`, база `02` и солид-канон равны тону по построению (композит `T` над
/// `T` есть `T`), поэтому один `tone_hex`. `alpha` — повторно проверенная верхняя
/// граница платформенно охарактеризованного поиска; численное свидетельство
/// лежит рядом.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialColor {
    /// Тон `#RRGGBB`: тинт `01`, база `02` и солид-канон одновременно.
    pub tone_hex: String,
    /// Выбранная альфа тинта `01`, `[0, 1]`.
    pub alpha: f64,
    /// Худший WCAG-контраст коммит-полюса по коридору `[чёрный, белый]`.
    pub worst_contrast: f64,
    /// Платформенно охарактеризованное свидетельство выбора границы alpha.
    pub alpha_guarantee: labcolors_core::MaterialAlphaGuaranteeV1,
    /// Типизированный исход проверки пола; от него выведен булев псевдоним совместимости.
    pub alpha_status: labcolors_core::MaterialAlphaStatusV1,
    /// Запрошенный WCAG-пол (4.5 / 3.0); держится только при `Satisfied`.
    pub floor: f64,
    /// Коммит-полюс поверхности белый (`true`, тёмный тон) или чёрный (`false`).
    pub pole_white: bool,
    /// Фактический |ΔJ'| тона-базы от фона резолва — различимость поверхности.
    pub achieved_dj: f64,
    /// Целевой |ΔJ'| тона не попал в бюджет ограниченного обхода; возвращённый
    /// кандидат имеет минимальную ошибку среди просмотренных, не среди всего
    /// гамута.
    pub tone_compressed: bool,
    /// Солид-канон отличим от фона резолва на 8-битной сетке дисплея.
    pub distinct: bool,
}

/// A semi-transparent emission and the contrasts its composite achieves.
#[derive(Debug, Clone, PartialEq)]
pub struct RgbaColor {
    /// The tint as `#RRGGBB` — the colour the emitted `oklch(… / A)` carries.
    pub tint_hex: String,
    /// The alpha the emission carries, `(0, 1]`.
    pub alpha: f64,
    /// The solid the tint composites to on the resolve background.
    pub composite_hex: String,
    /// The signed score of the frozen SAPC-shaped candidate curve for the composite.
    pub composite_lc: f64,
    /// The WCAG 2.1 ratio of the composite.
    pub composite_wcag: f64,
    /// `true`, если запрошенная alpha не допускает ни одного byte-тинта,
    /// воспроизводящего цель в affine encoded-sRGB8 reference, и потому поднята
    /// до первого проходящего `binary64`. Композит остаётся побайтно равен
    /// целевому солиду; меняется только явно возвращённая `alpha`. У прямой
    /// лестницы всегда `false`.
    pub alpha_coerced: bool,
    /// `true` when a solid family border (`border-<family>-strong`, M2 ch5c) was
    /// darkened along the family curve to meet the AA UI floor (3:1), because the
    /// raw family tint did not clear it on this background — an honest, flagged
    /// minimal legal shift along the declared family curve. Final bytes remain
    /// the representation truth; this flag does not claim perceptual hue/chroma
    /// preservation. `false` for a direct ladder emission and legal family solids.
    pub floor_coerced: bool,
}

/// A resolved colour with its Ys candidate score and WCAG ratio.
#[derive(Debug, Clone, PartialEq)]
pub struct SolvedColor {
    /// The colour as `#RRGGBB`.
    pub hex: String,
    /// The signed Ys candidate score from the frozen SAPC-shaped curve against the background;
    /// not LPC/readability evidence.
    pub lc: f64,
    /// The WCAG 2.1 ratio (1–21) against the background.
    pub wcag_ratio: f64,
    /// `true`, если точная цель роли не сохранена: legal floor/иерархия сжали
    /// contrast-target либо ограниченный dJ′-поиск вернул лучший из просмотренных
    /// кандидатов. Глобальный оптимум не заявляется.
    pub compressed: bool,
    /// Честный замер |ΔJ'| на отданном hex для dJ'-ролей (симметрия с glow);
    /// `None` у contrast-score ролей (их переходная координата — `Lc`).
    pub achieved_dj: Option<f64>,
    /// `true` when the WCAG legal floor overrode the Ys candidate-score target.
    pub floor_override: bool,
    /// Минимальное отношение WCAG из контракта роли (`AaText` → 4.5,
    /// `AaUi` → 3.0) либо `None`, если пола нет. Solve проверяет финальную
    /// эмитированную пару. Default runtime не удерживает пол на каждом
    /// промежуточном кадре; `strict` использует охарактеризованный clamp, но не
    /// является сертификатом.
    pub legal_floor: Option<f64>,
}
