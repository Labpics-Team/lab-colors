//! Конечный точечный reference-примитив для screen-рецепта glow.
//!
//! Модуль рассчитывает encoded-композит двух слоёв. Он не моделирует физическое
//! излучение, поле размытия или восприятие полного пространственного эффекта:
//! эти утверждения требуют контракта рендеринга и контекста #221.
//!
//! # Модель
//!
//! Слой свечения — цвет G с непрозрачностью α и `mix-blend-mode: screen`
//! над непрозрачным фоном. CSS Compositing 1 (blend → simple alpha
//! compositing) в объявленном reference-домене encoded sRGB даёт
//! покомпонентно:
//!
//! ```text
//! result = (1−α)·bg + α·screen(G, bg)
//!        = bg + α·G·(1−bg)          [screen(G,bg) = G + bg − G·bg]
//! ```
//!
//! Свойства по построению (не проверкой):
//! - **никогда не темнит**: α·G·(1−bg) ≥ 0 — вырождение «светлый тинт темнит
//!   светлый фон» у нормальной альфы здесь невозможно конструкцией;
//! - **монотонно и ЛИНЕЙНО по α** на канал. Из этого НЕ следует монотонность
//!   CAM16 J': солвер поэтому перебирает конечное множество реально эмитируемых
//!   sRGB8-композитов, а не предполагает форму перцептивной функции;
//! - на белом (1−bg)=0 слой является точечным no-op именно по формуле screen;
//!   это численное свойство reference-профиля, не утверждение о физическом
//!   свете или неизвестном рендерере.
//!
//! # Ступени пресета Lab UI
//!
//! Текущие |ΔJ′| взяты из точечных композитов клиентского стека теней Lab UI:
//! `subtle:=minor`, `base:=ambient`, `bloom:=major`. Это воспроизводимая
//! история пресета, но не универсальный психофизический закон и не таксономия
//! обобщённого core; перенос в клиентский профиль отслеживается #221.
//!
//! # Анатомия (двухслойный bloom)
//!
//! В текущем клиентском recipe core — цвет с поднятой к белому светлотой, halo
//! — исходный цвет. Радиусы и размытие здесь отсутствуют: названия обозначают
//! назначение слоёв у потребителя, а не измеренную геометрию.
//!
//! Для хроматического источника core строится существующей политикой
//! [`crate::accent_balance`]: midpoint J′ задаёт начальную Oklab-светлоту, chroma
//! берётся на sRGB-границе данного hue. Точная sRGB8-нейтраль остаётся
//! нейтральной: у неё нет hue, который можно было бы честно «усилить».
//! После хроматического преобразования и sRGB8-квантования итоговый J′ не
//! объявляется равным начальному значению — фактическая точечная метрика core
//! возвращается отдельно. Это версионированный recipe, а не оптимум,
//! проверенный на наблюдателях.

use crate::lcs::LcsColor;
use crate::numerical_plan::NumericalExecutionModeV1;
use crate::numerics::{
    LegacyPlatformDependentV1, NumericalCompatibilityReleaseIdV1, NumericalDecisionEvidenceV1,
    NumericalDecisionV1, NumericalIndeterminacyV1, NumericalSiteIdV1, ReferenceProfileIdV1,
    mint_bit_exact_evidence, registry_row,
};
use crate::spaces::oklab::oklab_to_srgb_linear;
use crate::spaces::srgb::{
    decode_8bit, hex_from_srgb, hex_from_srgb_encoded, srgb_encoded_from_hex, srgb_to_xyz,
};
use crate::spaces::vc::ViewingConditions;

/// Контрактный шаг glow-subtle (|ΔJ'| композита от фона): зеркало
/// стек-композита fx-shadow-minor на светлом якоре labui.
// SSOT-TRACKED — зеркальная деривация от владельческих альф теней.
pub const GLOW_SUBTLE_DJ: f64 = 0.8563;
/// Контрактный шаг glow-base: зеркало стек-композита fx-shadow-ambient.
// SSOT-TRACKED — зеркальная деривация от владельческих альф теней.
pub const GLOW_BASE_DJ: f64 = 2.3006;
/// Контрактный шаг glow-bloom: зеркало стек-композита fx-shadow-major.
// SSOT-TRACKED — зеркальная деривация от владельческих альф теней.
pub const GLOW_BLOOM_DJ: f64 = 13.3251;

/// Версия конечного reference-домена точного точечного композита glow.
///
/// Это не сертификат конкретного браузера, дисплея или решения CAM16 и не
/// пространственного поля: профиль фиксирует только encoded sRGB8 + screen.
pub const GLOW_COMPOSITE_PROFILE: &str = "encoded-srgb8-screen-v1";

/// Диагностическая appearance-модель, используемая legacy-выбором и замерами
/// полного двухслойного результата Glow. Она не является гарантией численного
/// решения и не усиливает сертификат композита.
pub const GLOW_DIAGNOSTIC_PROFILE: &str = "cam16-ucs-jprime-li2017-v1";

/// Версия recipe, строящего core-слой из начального CAM16-UCS J′ и Oklab cusp.
///
/// Профиль идентифицирует только алгоритм построения слоя. Он не является
/// гарантией выбора, appearance-сертификатом или утверждением о пространстве
/// либо браузере.
pub const GLOW_LAYER_RECIPE_PROFILE: &str = "cam16-jprime-oklab-cusp-v1";

/// Типизированный идентификатор профиля точного композитинга.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GlowCompositeProfileV1 {
    /// Точечный screen-композит в encoded sRGB8.
    EncodedSrgb8ScreenV1,
}

impl GlowCompositeProfileV1 {
    /// Стабильный wire-ключ сертификата.
    pub fn key(self) -> &'static str {
        match self {
            Self::EncodedSrgb8ScreenV1 => GLOW_COMPOSITE_PROFILE,
        }
    }
}

/// Гарантия точного сертификата композитинга, независимая от appearance-решения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GlowCompositeGuaranteeV1 {
    /// Фактический encoded-sRGB8 state и идентичность эмитированной binary64 alpha.
    BitExact,
}

impl GlowCompositeGuaranteeV1 {
    /// Стабильный wire-ключ.
    pub fn key(self) -> &'static str {
        match self {
            Self::BitExact => "bit-exact",
        }
    }
}

/// Идентификатор appearance-диагностики. Значение не является гарантией решения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GlowDiagnosticProfileV1 {
    /// Текущая транскрипция CAM16-UCS J′ из Li et al. 2017.
    Cam16UcsJPrimeLi2017V1,
}

impl GlowDiagnosticProfileV1 {
    /// Стабильный wire-ключ.
    pub fn key(self) -> &'static str {
        match self {
            Self::Cam16UcsJPrimeLi2017V1 => GLOW_DIAGNOSTIC_PROFILE,
        }
    }
}

/// Типизированный идентификатор текущего двухслойного recipe Glow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GlowLayerRecipeProfileV1 {
    /// Начальный midpoint CAM16-UCS J′, переведённый в Oklab lightness;
    /// хроматический core берёт ограниченную cusp chroma текущего Oklab hue,
    /// halo равен источнику.
    Cam16JPrimeOklabCuspV1,
}

impl GlowLayerRecipeProfileV1 {
    /// Стабильный wire-ключ recipe.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Cam16JPrimeOklabCuspV1 => GLOW_LAYER_RECIPE_PROFILE,
        }
    }
}

/// Явно выбранный профиль численного решения. `Default` намеренно отсутствует:
/// legacy-путь никогда не должен появляться через fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GlowDecisionProfileV1 {
    /// Стабильный контракт: без sound bound семантическая ветвь не выбирается.
    StableV1,
    /// Прежний зависящий от CAM16/libm выбор, сохранённый только явно.
    LegacyPlatformDependentV1,
}

impl GlowDecisionProfileV1 {
    /// Стабильный config/wire-ключ.
    pub const fn key(self) -> &'static str {
        match self {
            Self::StableV1 => "stable-v1",
            Self::LegacyPlatformDependentV1 => "legacy-platform-dependent-v1",
        }
    }

    /// Разбирает обязательный config-ключ; неизвестный профиль не нормализуется.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "stable-v1" => Ok(Self::StableV1),
            "legacy-platform-dependent-v1" => Ok(Self::LegacyPlatformDependentV1),
            other => Err(other.to_string()),
        }
    }

    /// Migration adapter (#292): прежние config/wire-ключи `stable-v1 |
    /// legacy-platform-dependent-v1` отображаются в generic typed execution
    /// mode. Они НЕ становятся новым общим profile enum.
    pub fn execution_mode(self) -> NumericalExecutionModeV1 {
        match self {
            Self::StableV1 => NumericalExecutionModeV1::StableOnly,
            Self::LegacyPlatformDependentV1 => NumericalExecutionModeV1::ExplicitCompatibility {
                release_id: NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1,
            },
        }
    }

    /// Обратная проекция generic mode в boundary-ключ (adapter-сторона).
    pub fn from_execution_mode(mode: NumericalExecutionModeV1) -> Self {
        match mode {
            NumericalExecutionModeV1::StableOnly => Self::StableV1,
            // Точный release: будущий чужой release не должен молча
            // проецироваться в Glow-профиль — компилятор потребует решения.
            NumericalExecutionModeV1::ExplicitCompatibility {
                release_id: NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1,
            } => Self::LegacyPlatformDependentV1,
        }
    }
}

/// Атомарный исход решения Glow ПОСЛЕ выбора состояния: доказанный stable
/// точный no-op либо явный registered compatibility-алгоритм. Незаконная
/// комбинация (stable + legacy provenance и т. п.) непредставима типами;
/// cross-product независимых полей profile/guarantee удалён (#292).
///
/// Genuine evidence from another registered site cannot be relabelled as a
/// Glow outcome outside Core:
///
/// ```compile_fail,E0639
/// use labcolors_core::GlowDecisionOutcomeV1;
/// use labcolors_core::wcag22::{
///     Wcag22AssessmentV1, Wcag22CriterionV1, evaluate_wcag22_srgb8,
/// };
///
/// let wcag = evaluate_wcag22_srgb8(
///     [0, 0, 0],
///     [255, 255, 255],
///     Wcag22CriterionV1::Sc143TextDefault,
/// ).unwrap();
/// let Wcag22AssessmentV1::Evaluated { evidence, .. } = wcag else {
///     unreachable!()
/// };
/// let _forged = GlowDecisionOutcomeV1::StableExactNoop { evidence };
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum GlowDecisionOutcomeV1 {
    /// Stable exact no-op: решение доказано запечатанным BitExact-evidence.
    #[non_exhaustive]
    StableExactNoop {
        /// Запечатанное registry-owned evidence.
        evidence: NumericalDecisionEvidenceV1,
    },
    /// Явно выбранный зарегистрированный прежний алгоритм.
    #[non_exhaustive]
    Compatibility {
        /// Registered release, реально исполнивший invocation.
        release_id: NumericalCompatibilityReleaseIdV1,
        /// Класс происхождения (не заменяет release identity).
        provenance: LegacyPlatformDependentV1,
    },
}

impl GlowDecisionOutcomeV1 {
    /// Boundary-проекция прежнего wire-ключа guarantee
    /// (`bit-exact | legacy-platform-dependent-v1`) — migration adapter.
    pub fn guarantee_wire_key(&self) -> &'static str {
        match self {
            Self::StableExactNoop { evidence } => evidence.class_key(),
            Self::Compatibility { provenance, .. } => provenance.key(),
        }
    }

    /// Boundary-проекция прежнего client decision profile.
    pub fn decision_profile(&self) -> GlowDecisionProfileV1 {
        match self {
            Self::StableExactNoop { .. } => GlowDecisionProfileV1::StableV1,
            Self::Compatibility { .. } => GlowDecisionProfileV1::LegacyPlatformDependentV1,
        }
    }
}

/// Слой, по которому точечный солвер держит целевой ΔJ′.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GlowConstraintLayer {
    /// Внешний ореол; core измеряется отдельно и не выдаётся за ту же цель.
    Halo,
}

impl GlowConstraintLayer {
    /// Стабильный wire-ключ.
    pub fn key(self) -> &'static str {
        match self {
            Self::Halo => "halo",
        }
    }
}

/// Итог проверки цели по конечному reference-домену.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GlowTargetStatus {
    /// Стабильный точный no-op: все alpha дают байтовое состояние фона, поэтому
    /// положительная цель точно недостижима без appearance-выбора.
    ExactNoopUnreachable,
    /// Явный legacy-выбор CAM16/libm нашёл первое состояние, держащее цель.
    LegacyReached,
    /// Явный legacy-выбор CAM16/libm не нашёл цель и вернул максимум |ΔJ′|,
    /// а при равенстве максимумов — первое состояние по alpha.
    LegacyUnreachable,
}

impl GlowTargetStatus {
    /// Стабильный wire-ключ.
    pub fn key(self) -> &'static str {
        match self {
            Self::ExactNoopUnreachable => "exact-noop-unreachable",
            Self::LegacyReached => "legacy-reached",
            Self::LegacyUnreachable => "legacy-unreachable",
        }
    }
}

/// Ступень контрактного стека свечения (зеркальная деривация, см. шапку).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlowStep {
    /// subtle := зеркало fx-shadow-minor.
    Subtle,
    /// base := зеркало fx-shadow-ambient.
    Base,
    /// bloom := зеркало fx-shadow-major.
    Bloom,
}

impl GlowStep {
    /// Целевой модуль ΔJ′ CAM16-UCS изолированного точечного halo-композита.
    pub fn target_dj(self) -> f64 {
        match self {
            GlowStep::Subtle => GLOW_SUBTLE_DJ,
            GlowStep::Base => GLOW_BASE_DJ,
            GlowStep::Bloom => GLOW_BLOOM_DJ,
        }
    }

    /// Стабильный kebab-ключ ступени (граница конфига).
    pub fn key(self) -> &'static str {
        match self {
            GlowStep::Subtle => "subtle",
            GlowStep::Base => "base",
            GlowStep::Bloom => "bloom",
        }
    }

    /// Разбор kebab-ключа; неизвестная строка — ошибка вызывающего.
    ///
    /// # Errors
    ///
    /// `Err` с непринятой строкой.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "subtle" => Ok(GlowStep::Subtle),
            "base" => Ok(GlowStep::Base),
            "bloom" => Ok(GlowStep::Bloom),
            other => Err(other.to_string()),
        }
    }
}

fn validate_encoded_rgb(label: &str, rgb: [f64; 3]) -> Result<(), String> {
    for (channel, value) in rgb.into_iter().enumerate() {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(format!(
                "{label}[{channel}] вне конечного encoded-sRGB [0,1]: {value}"
            ));
        }
    }
    Ok(())
}

fn validate_viewing_numerics(vc: &ViewingConditions) -> Result<(), String> {
    // Это только числовая защита CAM16-пути. Она не доказывает полноту или
    // физическую применимость контекста просмотра: такая граница принадлежит #230.
    for (name, value) in [
        ("n", vc.n),
        ("aw", vc.aw),
        ("nbb", vc.nbb),
        ("ncb", vc.ncb),
        ("fl", vc.fl),
        ("z", vc.z),
        ("c", vc.c),
        ("nc", vc.nc),
        ("fl_pow_025", vc.fl_pow_025),
        ("t_inner", vc.t_inner),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(format!(
                "ViewingConditions.{name} вне конечного положительного домена: {value}"
            ));
        }
    }
    for (channel, value) in vc.rgb_d.into_iter().enumerate() {
        if !value.is_finite() || value <= 0.0 {
            return Err(format!(
                "ViewingConditions.rgb_d[{channel}] вне конечного положительного домена: {value}"
            ));
        }
    }
    // Эти поля задокументированы как производные и должны меняться только
    // вместе с исходником. `ViewingConditions` пока имеет публичные поля, так
    // что проверяем инвариант на внешней границе вместо доверия структуре.
    for (name, actual, expected) in [
        ("fl_pow_025", vc.fl_pow_025, vc.fl.powf(0.25)),
        (
            "t_inner",
            vc.t_inner,
            (1.64 - 0.29_f64.powf(vc.n)).powf(0.73),
        ),
    ] {
        if actual.to_bits() != expected.to_bits() {
            return Err(format!(
                "ViewingConditions.{name} не согласован с исходными полями: {actual} != {expected}"
            ));
        }
    }
    Ok(())
}

fn validate_lcs_numerics(label: &str, color: &LcsColor) -> Result<(), String> {
    for (name, value) in [
        ("jp", color.jp),
        ("h_ok", color.h_ok),
        ("s", color.s),
        ("h_cam", color.h_cam()),
    ] {
        if !value.is_finite() {
            return Err(format!("{label}.{name} не конечен: {value}"));
        }
    }
    Ok(())
}

/// Только J′ для горячего пути конечного солвера. Oklab-hue и M′ здесь не
/// участвуют в целевой функции, поэтому полный `LcsColor` был бы лишней работой
/// на каждом из сотен состояний sRGB8. Формула J′ остаётся тем же SSOT из `cam16`.
fn jp_from_srgb8(bytes: [u8; 3], vc: &ViewingConditions) -> Result<f64, String> {
    let rgb = bytes.map(decode_8bit);
    let (j, _, _) = crate::spaces::cam16::forward(srgb_to_xyz(rgb), vc);
    let jp = crate::spaces::cam16::ucs_j(j);
    if jp.is_finite() {
        Ok(jp)
    } else {
        Err(format!(
            "условия просмотра дали неконечный J′ для {}",
            composite_hex(bytes)
        ))
    }
}

fn jp_from_hex(hex: &str, vc: &ViewingConditions) -> Result<f64, String> {
    let encoded = srgb_encoded_from_hex(hex)?;
    jp_from_srgb8(encoded_bytes(encoded), vc)
}

/// Непрерывный screen-слой над непрозрачным фоном: `bg + α·G·(1−bg)`
/// покомпонентно в encoded sRGB. Это алгебра до финального округления sRGB8; для
/// эмитируемого reference-пикселя используй [`screen_layer_over_srgb8`].
///
/// # Errors
///
/// `Err`, если α или компонент входного encoded-sRGB не конечен либо лежит
/// вне `[0, 1]`. Граница не зажимает мусор: иначе вызывающий получил бы другой
/// цвет без явного сигнала.
pub fn screen_layer_over_encoded(
    glow: [f64; 3],
    alpha: f64,
    bg: [f64; 3],
) -> Result<[f64; 3], String> {
    validate_encoded_rgb("glow", glow)?;
    validate_encoded_rgb("bg", bg)?;
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(format!("alpha вне конечного [0,1]: {alpha}"));
    }
    Ok([
        bg[0] + alpha * glow[0] * (1.0 - bg[0]),
        bg[1] + alpha * glow[1] * (1.0 - bg[1]),
        bg[2] + alpha * glow[2] * (1.0 - bg[2]),
    ])
}

/// Один канал конечного encoded-sRGB8 screen reference-профиля.
fn screen_channel_over_srgb8(glow: u8, alpha: f64, bg: u8) -> u8 {
    (f64::from(bg) + alpha * f64::from(glow) * f64::from(u8::MAX - bg) / f64::from(u8::MAX)).round()
        as u8
}

/// Screen-слой в конечном reference-домене encoded-sRGB8.
///
/// Формула вычисляется прямо в шкале байтов и округляется ровно один раз:
/// `round(bg + α·glow·(255−bg)/255)`. Указанный слева направо порядок
/// binary64-операций является частью reference-профиля и совпадает с JS-
/// проверкой официального пакета. Нормализация `byte/255` перед обратным
/// умножением способна сдвинуть граничное значение половинного округления на
/// соседний LSB.
///
/// # Errors
///
/// `Err`, если `alpha` не конечна или лежит вне `[0,1]`.
pub fn screen_layer_over_srgb8(glow: [u8; 3], alpha: f64, bg: [u8; 3]) -> Result<[u8; 3], String> {
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(format!("alpha вне конечного [0,1]: {alpha}"));
    }
    Ok(core::array::from_fn(|channel| {
        screen_channel_over_srgb8(glow[channel], alpha, bg[channel])
    }))
}

/// Точный сертификат одного изолированного screen-композита encoded-sRGB8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlowCompositeCertificateV1 {
    tint_srgb8: [u8; 3],
    background_srgb8: [u8; 3],
    alpha_bits: u64,
    alpha_css: String,
    composite_srgb8: [u8; 3],
}

impl GlowCompositeCertificateV1 {
    /// Reference-профиль точной арифметики.
    pub fn profile(&self) -> GlowCompositeProfileV1 {
        GlowCompositeProfileV1::EncodedSrgb8ScreenV1
    }

    /// Точная гарантия не зависит от диагностики или выбора CAM16.
    pub fn guarantee(&self) -> GlowCompositeGuaranteeV1 {
        GlowCompositeGuaranteeV1::BitExact
    }

    /// Эмитируемая тройка байтов tint.
    pub fn tint_srgb8(&self) -> [u8; 3] {
        self.tint_srgb8
    }

    /// Тройка байтов фона reference-запроса.
    pub fn background_srgb8(&self) -> [u8; 3] {
        self.background_srgb8
    }

    /// Точная binary64-идентичность alpha.
    pub fn alpha_bits(&self) -> u64 {
        self.alpha_bits
    }

    /// Каноническая CSS-запись той же alpha.
    pub fn alpha_css(&self) -> &str {
        &self.alpha_css
    }

    /// Точная тройка байтов композита.
    pub fn composite_srgb8(&self) -> [u8; 3] {
        self.composite_srgb8
    }
}

fn composite_certificate(
    tint_srgb8: [u8; 3],
    background_srgb8: [u8; 3],
    alpha: f64,
    alpha_css: String,
    composite_srgb8: [u8; 3],
) -> GlowCompositeCertificateV1 {
    GlowCompositeCertificateV1 {
        tint_srgb8,
        background_srgb8,
        alpha_bits: alpha.to_bits(),
        alpha_css,
        composite_srgb8,
    }
}

/// Изолированный точечный замер одного screen-слоя в reference-профиле glow.
/// Пространственное перекрытие core/halo сюда намеренно не входит: без
/// геометрии и порядка слоёв его нельзя восстановить честно.
pub(crate) struct ScreenLayerMeasurement {
    pub(crate) composite_hex: String,
    pub(crate) achieved_dj: f64,
    pub(crate) certificate: GlowCompositeCertificateV1,
}

pub(crate) fn measure_screen_layer_at_alpha(
    tint_hex: &str,
    bg_hex: &str,
    alpha: f64,
    vc: &ViewingConditions,
) -> Result<ScreenLayerMeasurement, String> {
    validate_viewing_numerics(vc)?;
    let tint = encoded_bytes(srgb_encoded_from_hex(tint_hex)?);
    let bg = encoded_bytes(srgb_encoded_from_hex(bg_hex)?);
    let composite_srgb8 = screen_layer_over_srgb8(tint, alpha, bg)?;
    let composite_hex = composite_hex(composite_srgb8);
    let alpha_css = crate::css_alpha_value(alpha)?;
    let bg_jp = jp_from_hex(bg_hex, vc)?;
    let composite_jp = jp_from_hex(&composite_hex, vc)?;
    let achieved_dj = (composite_jp - bg_jp).abs();
    if !achieved_dj.is_finite() {
        return Err(format!(
            "условия просмотра дали неконечный ΔJ' для {composite_hex}"
        ));
    }
    Ok(ScreenLayerMeasurement {
        composite_hex,
        achieved_dj,
        certificate: composite_certificate(tint, bg, alpha, alpha_css, composite_srgb8),
    })
}

const ALPHA_ZERO_BITS: u64 = 0.0_f64.to_bits();
const ALPHA_ONE_BITS: u64 = 1.0_f64.to_bits();
const MAX_QUANTISED_COMPOSITE_STATES: u16 = 766;

#[derive(Debug, Clone, Copy)]
struct QuantisedComposite {
    bytes: [u8; 3],
    lower_bits: u64,
    upper_bits: u64,
    upper_inclusive: bool,
}

impl QuantisedComposite {
    /// Выбирает внутреннюю representable alpha около численного центра state.
    /// Если midpoint округлился на исключённую верхнюю границу, берётся её
    /// непосредственный predecessor; singleton при alpha=1 остаётся допустим.
    fn canonical_alpha(self) -> f64 {
        let lower = f64::from_bits(self.lower_bits);
        let upper = f64::from_bits(self.upper_bits);
        if self.lower_bits == self.upper_bits {
            debug_assert!(self.upper_inclusive);
            return lower;
        }

        let midpoint = lower + (upper - lower) * 0.5;
        let mut bits = midpoint.to_bits().max(self.lower_bits);
        if self.upper_inclusive {
            bits = bits.min(self.upper_bits);
        } else if bits >= self.upper_bits {
            bits = self.upper_bits - 1;
        }
        f64::from_bits(bits)
    }
}

fn encoded_bytes(rgb: [f64; 3]) -> [u8; 3] {
    rgb.map(|channel| (channel * 255.0).round() as u8)
}

fn composite_hex(bytes: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", bytes[0], bytes[1], bytes[2])
}

/// Поток всех состояний, достижимых representable binary64-alpha в объявленном
/// operation order [`screen_layer_over_srgb8`].
///
/// Алгебраически равные рациональные стенки нельзя склеивать: разные
/// факторизации `G·(255−B)` могут пересечь half-tie на разных `f64`. Поэтому
/// каждый channel transition находит первый passing binary64 точным lower-bound
/// по упорядоченным положительным битам. Каждый переход увеличивает хотя бы один
/// байт, значит состояний не больше `1 + 3·255 = 766`; память остаётся O(1).
struct QuantisedComposites {
    glow: [u8; 3],
    background: [u8; 3],
    final_bytes: [u8; 3],
    bytes: [u8; 3],
    next_boundaries: [Option<u64>; 3],
    lower_bits: u64,
    emitted_states: u16,
    finished: bool,
}

/// Binary64-округление рациональной half-wall, используемое только как seed
/// экспоненциального поиска. Фактическую границу всегда определяет compositor.
fn boundary_seed_bits(glow: u8, background: u8, next_value: u8) -> u64 {
    let background = f64::from(background);
    let numerator = (f64::from(next_value) - background - 0.5) * f64::from(u8::MAX);
    let denominator = f64::from(glow) * (f64::from(u8::MAX) - background);
    debug_assert!(denominator > 0.0);
    (numerator / denominator).clamp(0.0, 1.0).to_bits()
}

impl QuantisedComposites {
    fn new(glow: [u8; 3], background: [u8; 3]) -> Self {
        let final_bytes = core::array::from_fn(|channel| {
            screen_channel_over_srgb8(glow[channel], 1.0, background[channel])
        });
        let mut stream = Self {
            glow,
            background,
            final_bytes,
            bytes: background,
            next_boundaries: [None; 3],
            lower_bits: ALPHA_ZERO_BITS,
            emitted_states: 0,
            finished: false,
        };
        for channel in 0..3 {
            stream.next_boundaries[channel] = stream.next_boundary(channel);
        }
        stream
    }

    fn next_boundary(&self, channel: usize) -> Option<u64> {
        let next_value = self.bytes[channel].checked_add(1)?;
        if self.final_bytes[channel] < next_value {
            return None;
        }

        debug_assert!(
            screen_channel_over_srgb8(
                self.glow[channel],
                f64::from_bits(self.lower_bits),
                self.background[channel],
            ) < next_value,
            "текущий state уже пересёк искомую channel boundary"
        );
        let passes = |bits| {
            screen_channel_over_srgb8(
                self.glow[channel],
                f64::from_bits(bits),
                self.background[channel],
            ) >= next_value
        };

        // Рациональная half-wall служит только seed, а не определением state:
        // фиксированный binary64 operation order может сдвинуть first-passing
        // alpha относительно этого seed. Экспоненциальный поиск обязательно находит
        // bracket между уже известными failing lower и passing 1, после чего
        // обычный lower-bound остаётся точным независимо от качества seed.
        let seed = boundary_seed_bits(self.glow[channel], self.background[channel], next_value)
            .clamp(self.lower_bits.saturating_add(1), ALPHA_ONE_BITS);

        let (mut failing, mut passing);
        if passes(seed) {
            passing = seed;
            let mut stride = 1_u64;
            loop {
                let probe = passing.saturating_sub(stride).max(self.lower_bits);
                if !passes(probe) {
                    failing = probe;
                    break;
                }
                passing = probe;
                stride = stride.saturating_mul(2);
            }
        } else {
            failing = seed;
            let mut stride = 1_u64;
            loop {
                let probe = failing.saturating_add(stride).min(ALPHA_ONE_BITS);
                if passes(probe) {
                    passing = probe;
                    break;
                }
                failing = probe;
                stride = stride.saturating_mul(2);
            }
        }

        while passing - failing > 1 {
            let middle = failing + (passing - failing) / 2;
            if passes(middle) {
                passing = middle;
            } else {
                failing = middle;
            }
        }
        Some(passing)
    }

    fn emit(&mut self, state: QuantisedComposite) -> Result<Option<QuantisedComposite>, String> {
        self.emitted_states += 1;
        if self.emitted_states > MAX_QUANTISED_COMPOSITE_STATES {
            return Err(format!(
                "binary64 Glow stream превысил доказанную границу {MAX_QUANTISED_COMPOSITE_STATES}"
            ));
        }
        Ok(Some(state))
    }

    /// Возвращает очередной интервал `[lower, upper)`; последний интервал
    /// замкнут справа и заканчивается на `1`. Вместе поглощаются только
    /// transition с одним и тем же первым passing `f64`, а не одинаковые
    /// rational real-number walls.
    fn next_state(&mut self) -> Result<Option<QuantisedComposite>, String> {
        if self.finished {
            return Ok(None);
        }

        let upper = self.next_boundaries.into_iter().flatten().min();

        let Some(upper) = upper else {
            self.finished = true;
            return self.emit(QuantisedComposite {
                bytes: self.bytes,
                lower_bits: self.lower_bits,
                upper_bits: ALPHA_ONE_BITS,
                upper_inclusive: true,
            });
        };

        let state = QuantisedComposite {
            bytes: self.bytes,
            lower_bits: self.lower_bits,
            upper_bits: upper,
            upper_inclusive: false,
        };
        let previous = self.bytes;
        self.bytes = screen_layer_over_srgb8(self.glow, f64::from_bits(upper), self.background)?;
        self.lower_bits = upper;
        for (channel, previous_byte) in previous.into_iter().enumerate() {
            if self.bytes[channel] < previous_byte {
                return Err(format!(
                    "binary64 screen stream уменьшил канал {channel}: {} -> {}",
                    previous_byte, self.bytes[channel]
                ));
            }
            if self.bytes[channel] > previous_byte {
                self.next_boundaries[channel] = self.next_boundary(channel);
            } else if self.next_boundaries[channel] == Some(upper) {
                return Err(format!(
                    "channel {channel} не изменился на собственной first-passing boundary"
                ));
            }
        }
        self.emit(state)
    }
}

/// Тестовый сборщик сохраняет удобный независимый оракул, но `Vec` и сортировка
/// не входят в production-сборку WASM.
#[cfg(test)]
fn quantised_composites(
    glow: [u8; 3],
    background: [u8; 3],
) -> Result<Vec<QuantisedComposite>, String> {
    let mut stream = QuantisedComposites::new(glow, background);
    let mut states = Vec::new();
    while let Some(state) = stream.next_state()? {
        states.push(state);
    }
    Ok(states)
}

/// Результат glow-солвера с типизированным статусом проверки цели.
#[derive(Debug, Clone, PartialEq)]
pub struct GlowSolve {
    /// Каноническая интенсивность слоя `(0, 1]`: центр устойчивого интервала
    /// выбранного sRGB8-композита. Это не «оптимальный вкус», а максимальный
    /// численный запас до ближайшей границы квантования.
    alpha: f64,
    /// Каноническая CSS-запись той же alpha; хранится вместе с числом, чтобы
    /// нижележащий потребитель не вводил собственную политику округления.
    alpha_css: String,
    /// Запрошенный |ΔJ′| точечного композита halo.
    target_dj: f64,
    /// Фактический |ΔJ'| композита от фона, замерен на эмитируемом hex.
    achieved_dj: f64,
    /// Композит `screen(tint, α)` над фоном, `#RRGGBB`.
    composite_hex: String,
    /// Точное свидетельство композитинга, независимое от legacy appearance-решения.
    composite_certificate: GlowCompositeCertificateV1,
    /// Appearance-диагностика, реально использованная только для выбора.
    /// Точный no-op полностью её обходит.
    selection_diagnostic_profile: Option<GlowDiagnosticProfileV1>,
    /// Исход проверки цели с provenance. Точный no-op объявляет точную
    /// недостижимость по одному байтовому состоянию; явный legacy-путь объявляет
    /// достижение либо возвращает выбранный CAM16 максимум с legacy-маркером.
    status: GlowTargetStatus,
}

impl GlowSolve {
    /// Каноническая интенсивность, выбранная внутри устойчивого sRGB8-интервала.
    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    /// CSS-запись, восстанавливающая ту же binary64 alpha.
    pub fn alpha_css(&self) -> &str {
        &self.alpha_css
    }

    /// Запрошенный |ΔJ′| halo-композита.
    pub fn target_dj(&self) -> f64 {
        self.target_dj
    }

    /// Фактически достигнутый |ΔJ′| на возвращённом sRGB8-композите.
    pub fn achieved_dj(&self) -> f64 {
        self.achieved_dj
    }

    /// Возвращённый reference-композит halo.
    pub fn composite_hex(&self) -> &str {
        &self.composite_hex
    }

    /// Точный сертификат точечного композита выбранного состояния.
    pub fn composite_certificate(&self) -> &GlowCompositeCertificateV1 {
        &self.composite_certificate
    }

    /// Типизированный результат проверки цели.
    pub fn status(&self) -> GlowTargetStatus {
        self.status
    }

    /// Геттер совместимости прежнего булева контракта: цель недостижима как при
    /// точном no-op, так и при явном legacy-исходе с максимумом.
    pub fn degraded(&self) -> bool {
        matches!(
            self.status,
            GlowTargetStatus::ExactNoopUnreachable | GlowTargetStatus::LegacyUnreachable
        )
    }

    /// Слой, по которому решалась цель.
    pub fn constraint_layer(&self) -> GlowConstraintLayer {
        GlowConstraintLayer::Halo
    }

    /// Точный профиль композита; идентификатор диагностики выбора читается отдельно.
    pub fn composite_profile(&self) -> GlowCompositeProfileV1 {
        self.composite_certificate.profile()
    }

    /// Идентификатор диагностики выбора, только если точечный солвер действительно
    /// вызвал эту модель для выбора target/max.
    pub fn selection_diagnostic_profile(&self) -> Option<GlowDiagnosticProfileV1> {
        self.selection_diagnostic_profile
    }
}

struct ScreenPointInputs {
    glow: [u8; 3],
    background: [u8; 3],
    slopes: [u16; 3],
}

/// Разобрать точные точечные входы encoded-sRGB8 и вывести наклоны их каналов.
fn screen_point_inputs(glow_tint_hex: &str, bg_hex: &str) -> Result<ScreenPointInputs, String> {
    let glow_bytes = encoded_bytes(srgb_encoded_from_hex(glow_tint_hex)?);
    let bg_bytes = encoded_bytes(srgb_encoded_from_hex(bg_hex)?);
    let slopes = core::array::from_fn::<_, 3, _>(|channel| {
        u16::from(glow_bytes[channel]) * u16::from(u8::MAX - bg_bytes[channel])
    });
    Ok(ScreenPointInputs {
        glow: glow_bytes,
        background: bg_bytes,
        slopes,
    })
}

fn slopes_are_exact_srgb8_noop(slopes: [u16; 3]) -> bool {
    // Screen монотонен по alpha. Поэтому каждая alpha квантуется в фон тогда и
    // только тогда, когда endpoint при alpha=1 остаётся строго ниже первой
    // стенки половины LSB: G*(255-B)/255 < 1/2.
    slopes
        .into_iter()
        .all(|slope| 2 * u32::from(slope) < u32::from(u8::MAX))
}

/// Предикат точной повторной проверки стабильного точечного решения Glow.
///
/// Возвращает `true` тогда и только тогда, когда монотонный screen-endpoint
/// encoded-sRGB8 при alpha=1 остаётся ниже первой стенки половины LSB в каждом
/// канале. Сюда входят нулевые наклоны и ненулевые наклоны меньше LSB.
/// Appearance-модель и epsilon в проверке не участвуют.
///
/// # Errors
///
/// Возвращает `Err` для невалидного tint или фона вместо их нормализации.
pub fn screen_point_is_exact_noop(glow_tint_hex: &str, bg_hex: &str) -> Result<bool, String> {
    let inputs = screen_point_inputs(glow_tint_hex, bg_hex)?;
    Ok(slopes_are_exact_srgb8_noop(inputs.slopes))
}

/// Решить интенсивность screen-слоя с явным профилем численного решения.
///
/// `StableV1` не вызывает CAM16 для нетривиального участка target/max, пока для
/// него нет sound bound, и возвращает типизированный `Indeterminate`.
/// Единственный точный определённый стабильный случай — точечный no-op: все
/// alpha дают тот же фон, поэтому `ΔJ′ = 0` следует из равенства байтовых
/// состояний без appearance-математики. `LegacyPlatformDependentV1` явно
/// сохраняет прежний runtime и помечает его гарантию; неявная legacy-обёртка
/// отсутствует.
///
/// # Errors
///
/// `Err` — невалидный hex или target либо нарушенное внутреннее постусловие
/// legacy-пути.
pub fn solve_screen_alpha_for_dj(
    glow_tint_hex: &str,
    bg_hex: &str,
    target_dj: f64,
    mode: NumericalExecutionModeV1,
    vc: &ViewingConditions,
) -> Result<NumericalDecisionV1<GlowSolve>, String> {
    // #292: resolver исполняет typed mode, сохранённый в compiled invocation;
    // plan lookup/string policy selection в hot path отсутствуют.
    const SITE: NumericalSiteIdV1 = NumericalSiteIdV1::GlowTargetOrMaximumV1;
    match mode {
        NumericalExecutionModeV1::ExplicitCompatibility { release_id } => {
            // Fail closed: release обязан быть зарегистрирован для site —
            // незарегистрированный выбор является load-ошибкой, не fallback.
            let row = registry_row(SITE)
                .ok_or_else(|| format!("site {} отсутствует в registry V1", SITE.key()))?;
            if !row.compatibility_releases.contains(&release_id) {
                return Err(format!(
                    "release {} не зарегистрирован для site {}",
                    release_id.key(),
                    SITE.key()
                ));
            }
            Ok(NumericalDecisionV1::Compatibility {
                site_id: SITE,
                release_id,
                value: solve_screen_alpha_for_dj_legacy(glow_tint_hex, bg_hex, target_dj, vc)?,
                provenance: LegacyPlatformDependentV1,
            })
        }
        NumericalExecutionModeV1::StableOnly => {
            if !target_dj.is_finite() || target_dj <= 0.0 {
                return Err(format!("целевой шаг вне домена: {target_dj}"));
            }
            let ScreenPointInputs {
                glow: glow_bytes,
                background: bg_bytes,
                slopes,
            } = screen_point_inputs(glow_tint_hex, bg_hex)?;
            if slopes_are_exact_srgb8_noop(slopes) {
                // Любая alpha из [0,1] даёт тот же байтовый composite; 0.5 —
                // канонический средний представитель, а не измеренная величина.
                let alpha = 0.5;
                let alpha_css = crate::css_alpha_value(alpha)?;
                let composite_srgb8 = bg_bytes;
                return Ok(NumericalDecisionV1::Determinate {
                    site_id: SITE,
                    value: GlowSolve {
                        alpha,
                        alpha_css: alpha_css.clone(),
                        target_dj,
                        achieved_dj: 0.0,
                        composite_hex: composite_hex(composite_srgb8),
                        composite_certificate: composite_certificate(
                            glow_bytes,
                            bg_bytes,
                            alpha,
                            alpha_css,
                            composite_srgb8,
                        ),
                        selection_diagnostic_profile: None,
                        status: GlowTargetStatus::ExactNoopUnreachable,
                    },
                    // BitExact минтится registry-owned минтером: доказательство
                    // принадлежит точному байтовому screen-профилю.
                    evidence: mint_bit_exact_evidence(
                        SITE,
                        ReferenceProfileIdV1::EncodedSrgb8ScreenV1,
                    )?,
                });
            }
            // Нетривиальный target/max без sound bound: честный typed-отказ,
            // CAM16 selection не вызывается.
            Ok(NumericalDecisionV1::Indeterminate {
                site_id: SITE,
                evidence: NumericalIndeterminacyV1::SoundBoundUnavailable,
            })
        }
    }
}

/// Явный зависящий от CAM16/libm legacy-путь target/max.
///
/// Канальные переходы screen находятся как first-passing binary64 границы того
/// же operation order, который исполняет публичный compositor. Солвер проверяет
/// все достижимые композиты в порядке α и потому не предполагает монотонность
/// CAM16 J'. Если цель недостижима, возвращается глобальный максимум |ΔJ'| среди
/// достижимых состояний; равные максимумы детерминированно разрешаются первым
/// состоянием по alpha.
///
/// # Errors
///
/// `Err` — невалидный hex, неконечная или неположительная цель либо нечисловой
/// результат переданных условий просмотра.
fn solve_screen_alpha_for_dj_legacy(
    glow_tint_hex: &str,
    bg_hex: &str,
    target_dj: f64,
    vc: &ViewingConditions,
) -> Result<GlowSolve, String> {
    if !target_dj.is_finite() || target_dj <= 0.0 {
        return Err(format!("целевой шаг вне домена: {target_dj}"));
    }
    validate_viewing_numerics(vc)?;
    let glow = srgb_encoded_from_hex(glow_tint_hex)?;
    let bg = srgb_encoded_from_hex(bg_hex)?;
    let glow_bytes = encoded_bytes(glow);
    let bg_bytes = encoded_bytes(bg);
    let bg_jp = jp_from_srgb8(bg_bytes, vc)?;

    let measured = |bytes: [u8; 3]| -> Result<f64, String> {
        let jp = jp_from_srgb8(bytes, vc)?;
        let dj = (jp - bg_jp).abs();
        if !dj.is_finite() {
            return Err(format!(
                "условия просмотра дали неконечный ΔJ' для {}",
                composite_hex(bytes)
            ));
        }
        Ok(dj)
    };

    let mut states = QuantisedComposites::new(glow_bytes, bg_bytes);
    let mut best: Option<(QuantisedComposite, f64)> = None;
    let mut selected = None;

    while let Some(state) = states.next_state()? {
        let achieved_dj = measured(state.bytes)?;
        if achieved_dj >= target_dj {
            selected = Some((state, achieved_dj));
            break;
        }
        if best
            .as_ref()
            .is_none_or(|(_, best_dj)| achieved_dj > *best_dj)
        {
            best = Some((state, achieved_dj));
        }
    }

    let (state, achieved_dj, status) = if let Some(selected) = selected {
        let (state, achieved_dj) = selected;
        (state, achieved_dj, GlowTargetStatus::LegacyReached)
    } else {
        let (state, achieved_dj) =
            best.ok_or_else(|| "конечное множество композитов оказалось пустым".to_string())?;
        (state, achieved_dj, GlowTargetStatus::LegacyUnreachable)
    };
    let selected_hex = composite_hex(state.bytes);
    let alpha = state.canonical_alpha();

    // Канонический CSS-сериализатор хранит ту же binary64 alpha. Production-
    // проверка повторно композитит исходное число; обратное чтение строки
    // проверяют граничные и межъязыковые тесты, не затягивая dec2flt-парсер в WASM.
    let alpha_css = crate::css_alpha_value(alpha)?;
    let roundtrip_srgb8 = screen_layer_over_srgb8(glow_bytes, alpha, bg_bytes)?;
    let roundtrip_hex = composite_hex(roundtrip_srgb8);
    if roundtrip_hex != selected_hex {
        return Err(format!(
            "выбранная alpha не воспроизвела state: ожидался {selected_hex}, получен {roundtrip_hex}"
        ));
    }

    Ok(GlowSolve {
        alpha,
        alpha_css: alpha_css.clone(),
        target_dj,
        achieved_dj,
        composite_hex: selected_hex,
        composite_certificate: composite_certificate(
            glow_bytes,
            bg_bytes,
            alpha,
            alpha_css,
            roundtrip_srgb8,
        ),
        selection_diagnostic_profile: Some(GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1),
        status,
    })
}

/// Версионированный двухслойный recipe от источника: `(core_hex, halo_hex)`.
///
/// В recipe v1 halo буквально равен источнику. Для core арифметическая середина
/// J′ источника и 100 задаёт начальную Oklab-светлоту. У хроматического источника
/// chroma — sRGB-граница его Oklab hue через
/// [`crate::accent_balance::accent_balanced`]; у точного sRGB8-нейтраля chroma
/// остаётся нулевой, потому его численный hue не несёт цветового смысла.
/// Фактический J′ эмитированного core измеряется вызывающим кодом: он не обязан
/// совпасть с начальным значением после смены координат и sRGB8-квантования.
/// Это recipe, а не наблюдательная модель красоты или геометрии свечения.
pub fn glow_layers_from_source(
    source_hex: &str,
    vc: &ViewingConditions,
) -> Result<(String, String), String> {
    validate_viewing_numerics(vc)?;
    let source_encoded = srgb_encoded_from_hex(source_hex)?;
    let source_bytes = encoded_bytes(source_encoded);
    let canonical_source_hex = hex_from_srgb_encoded(source_encoded);
    let src = LcsColor::from_hex_with_vc(&canonical_source_hex, vc)?;
    validate_lcs_numerics("source", &src)?;
    let jp_core = (src.jp + 100.0) * 0.5;
    if !jp_core.is_finite() {
        return Err(format!("core J′ seed не конечен: {jp_core}"));
    }
    let l_core = crate::scale::jp_to_oklab_l(jp_core, vc);
    if !l_core.is_finite() || !(0.0..=1.0).contains(&l_core) {
        return Err(format!("core Oklab L вне конечного [0,1]: {l_core}"));
    }

    let core_rgb = if source_bytes[0] == source_bytes[1] && source_bytes[1] == source_bytes[2] {
        // У точного sRGB8-нейтраля оттенка нет. Число atan2 от матричного шума
        // нельзя превращать в насыщенный core: hue-отсутствие сохраняется.
        oklab_to_srgb_linear([l_core, 0.0, 0.0])
    } else {
        let balanced = crate::accent_balance::accent_balanced(l_core, src.h_ok, vc);
        validate_lcs_numerics("core", &balanced.color)?;
        for (name, value) in [
            ("l_ok", balanced.l_ok),
            ("c_ok", balanced.c_ok),
            ("hue_deg", balanced.hue_deg),
        ] {
            if !value.is_finite() {
                return Err(format!("balanced core {name} не конечен: {value}"));
            }
        }
        let hue = balanced.hue_deg.to_radians();
        oklab_to_srgb_linear([
            balanced.l_ok,
            balanced.c_ok * hue.cos(),
            balanced.c_ok * hue.sin(),
        ])
    };
    if core_rgb.into_iter().any(|channel| !channel.is_finite()) {
        return Err(format!("core linear-sRGB не конечен: {core_rgb:?}"));
    }
    let core_hex = hex_from_srgb(core_rgb);
    Ok((core_hex, canonical_source_hex))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_and_recipe_keys_preserve_their_provenance() {
        assert_eq!(
            GlowTargetStatus::ExactNoopUnreachable.key(),
            "exact-noop-unreachable"
        );
        assert_eq!(GlowTargetStatus::LegacyReached.key(), "legacy-reached");
        assert_eq!(
            GlowTargetStatus::LegacyUnreachable.key(),
            "legacy-unreachable"
        );
        assert_eq!(
            GlowLayerRecipeProfileV1::Cam16JPrimeOklabCuspV1.key(),
            "cam16-jprime-oklab-cusp-v1"
        );
    }

    fn solve_legacy(
        tint: &str,
        background: &str,
        target_dj: f64,
        vc: &ViewingConditions,
    ) -> Result<GlowSolve, String> {
        match solve_screen_alpha_for_dj(
            tint,
            background,
            target_dj,
            GlowDecisionProfileV1::LegacyPlatformDependentV1.execution_mode(),
            vc,
        )? {
            NumericalDecisionV1::Compatibility {
                value,
                release_id: NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1,
                provenance: LegacyPlatformDependentV1,
                ..
            } => Ok(value),
            other => Err(format!("explicit compatibility mode дал {other:?}")),
        }
    }

    #[test]
    fn stable_profile_returns_bound_unavailable_indeterminate() {
        let vc = ViewingConditions::srgb();
        let decision = solve_screen_alpha_for_dj(
            "#C0B2FA",
            "#000000",
            GLOW_BASE_DJ,
            GlowDecisionProfileV1::StableV1.execution_mode(),
            &vc,
        )
        .expect("валидный запрос обязан дать typed numerical decision");
        assert!(matches!(
            decision,
            NumericalDecisionV1::Indeterminate {
                site_id: NumericalSiteIdV1::GlowTargetOrMaximumV1,
                evidence: NumericalIndeterminacyV1::SoundBoundUnavailable,
            }
        ));
    }

    #[test]
    fn stable_indeterminate_path_does_not_consult_cam16_viewing_conditions() {
        let mut invalid_vc = ViewingConditions::srgb();
        invalid_vc.n = f64::NAN;

        let stable = solve_screen_alpha_for_dj(
            "#C0B2FA",
            "#000000",
            GLOW_BASE_DJ,
            GlowDecisionProfileV1::StableV1.execution_mode(),
            &invalid_vc,
        )
        .expect("stable-v1 must stop before the unbounded CAM16 site");
        assert!(matches!(
            stable,
            NumericalDecisionV1::Indeterminate {
                site_id: NumericalSiteIdV1::GlowTargetOrMaximumV1,
                evidence: NumericalIndeterminacyV1::SoundBoundUnavailable,
            }
        ));

        assert!(
            solve_screen_alpha_for_dj(
                "#C0B2FA",
                "#000000",
                GLOW_BASE_DJ,
                GlowDecisionProfileV1::LegacyPlatformDependentV1.execution_mode(),
                &invalid_vc,
            )
            .is_err()
        );
    }

    #[test]
    fn stable_point_noop_is_determinate_from_exact_bytes_only() {
        let mut invalid_vc = ViewingConditions::srgb();
        invalid_vc.n = f64::NAN;
        let decision = solve_screen_alpha_for_dj(
            "#4A8FFF",
            "#FFFFFF",
            GLOW_BASE_DJ,
            GlowDecisionProfileV1::StableV1.execution_mode(),
            &invalid_vc,
        )
        .expect("white point screen composite is exact and does not need CAM16");

        let NumericalDecisionV1::Determinate {
            value,
            evidence: NumericalDecisionEvidenceV1::BitExact { .. },
            ..
        } = decision
        else {
            panic!("exact point no-op must be a BitExact determinate decision");
        };
        assert_eq!(value.status(), GlowTargetStatus::ExactNoopUnreachable);
        assert!(value.selection_diagnostic_profile().is_none());
        assert_eq!(value.achieved_dj().to_bits(), 0.0_f64.to_bits());
        assert_eq!(value.composite_hex(), "#FFFFFF");
        assert_eq!(value.composite_certificate().composite_srgb8(), [255; 3]);
    }

    #[test]
    fn stable_point_noop_follows_exact_endpoint_law_not_white_special_case() {
        let mut invalid_vc = ViewingConditions::srgb();
        invalid_vc.n = f64::NAN;
        for (tint, background) in [
            ("#000000", "#123456"),
            // Каждый канал имеет G*(255-B)=0, но ни один операнд не является
            // однородным endpoint: R/B используют G=0, а G использует B=255.
            ("#00FF00", "#12FF34"),
            // Ненулевой красный наклон 1 остаётся ниже точной стенки половины LSB.
            ("#010000", "#FE0000"),
        ] {
            assert!(screen_point_is_exact_noop(tint, background).unwrap());
            let decision = solve_screen_alpha_for_dj(
                tint,
                background,
                GLOW_BASE_DJ,
                GlowDecisionProfileV1::StableV1.execution_mode(),
                &invalid_vc,
            )
            .expect("quantised screen no-op is exact and must not consult CAM16");
            assert!(matches!(
                decision,
                NumericalDecisionV1::Determinate {
                    value,
                    evidence: NumericalDecisionEvidenceV1::BitExact { .. },
                    ..
                } if value.status() == GlowTargetStatus::ExactNoopUnreachable
                    && value.achieved_dj().to_bits() == 0.0_f64.to_bits()
                    && value.composite_hex() == background
                    && value.selection_diagnostic_profile().is_none()
            ));
        }
        assert!(screen_point_is_exact_noop("#7F0000", "#FE0000").unwrap());
        assert!(!screen_point_is_exact_noop("#800000", "#FE0000").unwrap());
        assert!(screen_point_is_exact_noop("not-a-colour", "#123456").is_err());
    }

    #[test]
    fn exact_noop_predicate_matches_every_one_channel_endpoint() {
        for glow in 0..=u8::MAX {
            for background in 0..=u8::MAX {
                let tint_hex = format!("#{glow:02X}0000");
                let bg_hex = format!("#{background:02X}0000");
                let endpoint =
                    screen_layer_over_srgb8([glow, 0, 0], 1.0, [background, 0, 0]).unwrap();
                assert_eq!(
                    screen_point_is_exact_noop(&tint_hex, &bg_hex).unwrap(),
                    endpoint == [background, 0, 0],
                    "glow={glow}, background={background}"
                );
            }
        }
    }

    /// Независимый оракул не использует production cursor/boundary helper. Для
    /// каждого следующего channel byte он отдельно делает lower-bound по всему
    /// диапазону положительных `f64` через публичный композитор, затем объединяет
    /// найденные bit-boundaries и удаляет только соседние дубликаты hex.
    fn oracle_states(
        tint_hex: &str,
        bg_hex: &str,
        vc: &ViewingConditions,
    ) -> Vec<(f64, String, f64)> {
        let tint = srgb_encoded_from_hex(tint_hex).unwrap();
        let bg = srgb_encoded_from_hex(bg_hex).unwrap();
        let tint_bytes = encoded_bytes(tint);
        let bg_bytes = encoded_bytes(bg);
        let mut boundaries = vec![ALPHA_ZERO_BITS, ALPHA_ONE_BITS];

        for channel in 0..3 {
            let final_value = screen_layer_over_srgb8(tint_bytes, 1.0, bg_bytes).unwrap()[channel];
            for value in (u16::from(bg_bytes[channel]) + 1)..=u16::from(final_value) {
                let mut failing = ALPHA_ZERO_BITS;
                let mut passing = ALPHA_ONE_BITS;
                while passing - failing > 1 {
                    let middle = failing + (passing - failing) / 2;
                    let output =
                        screen_layer_over_srgb8(tint_bytes, f64::from_bits(middle), bg_bytes)
                            .unwrap()[channel];
                    if output >= value as u8 {
                        passing = middle;
                    } else {
                        failing = middle;
                    }
                }
                boundaries.push(passing);
            }
        }
        boundaries.sort_unstable();
        boundaries.dedup();

        let bg_jp = LcsColor::from_hex_with_vc(bg_hex, vc).unwrap().jp;
        let mut states: Vec<(f64, String, f64)> = Vec::new();
        for boundary in boundaries {
            let alpha = f64::from_bits(boundary);
            let hex = composite_hex(screen_layer_over_srgb8(tint_bytes, alpha, bg_bytes).unwrap());
            if states
                .last()
                .is_some_and(|(_, previous, _)| previous == &hex)
            {
                continue;
            }
            let jp = LcsColor::from_hex_with_vc(&hex, vc).unwrap().jp;
            states.push((alpha, hex, (jp - bg_jp).abs()));
        }
        states
    }

    /// Screen никогда не темнит — свойство конструкции, sweep-замок.
    #[test]
    fn screen_never_darkens_any_channel() {
        let tints = ["#3E87FF", "#FF3B30", "#FFFFFF", "#101012"];
        let bgs = ["#101012", "#1C1C1E", "#808080", "#F7F8FA", "#FFFFFF"];
        for tint_hex in tints {
            let tint = srgb_encoded_from_hex(tint_hex).unwrap();
            for bg_hex in bgs {
                let bg = srgb_encoded_from_hex(bg_hex).unwrap();
                for i in 0..=20 {
                    let a = f64::from(i) / 20.0;
                    let out = screen_layer_over_encoded(tint, a, bg).unwrap();
                    for ch in 0..3 {
                        assert!(
                            out[ch] >= bg[ch] - 1e-12,
                            "screen затемнил канал {ch}: tint {tint_hex} @ {a} на {bg_hex}"
                        );
                        assert!(out[ch] <= 1.0 + 1e-12, "канал {ch} вне гамута");
                    }
                }
            }
        }
    }

    /// Публичная числовая граница отвергает мусор, а не паникует в debug и не
    /// превращает его в другой цвет через clamp.
    #[test]
    fn screen_rejects_every_non_finite_or_out_of_domain_component() {
        let valid = [0.25, 0.5, 0.75];
        for alpha in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.1] {
            assert!(screen_layer_over_encoded(valid, alpha, valid).is_err());
            assert!(screen_layer_over_srgb8([1, 2, 3], alpha, [4, 5, 6]).is_err());
        }
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.1] {
            let mut rgb = valid;
            rgb[1] = bad;
            assert!(screen_layer_over_encoded(rgb, 0.5, valid).is_err());
            assert!(screen_layer_over_encoded(valid, 0.5, rgb).is_err());
        }
        assert!(screen_layer_over_encoded(valid, 0.0, valid).is_ok());
        assert!(screen_layer_over_encoded(valid, 1.0, valid).is_ok());
        assert!(screen_layer_over_srgb8([1, 2, 3], 0.0, [4, 5, 6]).is_ok());
        assert!(screen_layer_over_srgb8([1, 2, 3], 1.0, [4, 5, 6]).is_ok());
    }

    #[test]
    fn hot_jp_path_is_bit_identical_to_full_lcs_forward() {
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            for byte in 0_u16..=255 {
                let hex = format!("#{byte:02X}{byte:02X}{byte:02X}");
                let lean = jp_from_hex(&hex, &vc).unwrap();
                let full = LcsColor::from_hex_with_vc(&hex, &vc).unwrap().jp;
                assert_eq!(lean.to_bits(), full.to_bits(), "{hex}");
            }
            for hex in ["#007AFF", "#FF3B30", "#34C759", "#FFD000", "#3E87FF"] {
                let lean = jp_from_hex(hex, &vc).unwrap();
                let full = LcsColor::from_hex_with_vc(hex, &vc).unwrap().jp;
                assert_eq!(lean.to_bits(), full.to_bits(), "{hex}");
            }
        }
    }

    /// Точка `x.5` принадлежит новому байту, а каждый state начинается ровно на
    /// первом passing `f64` объявленного operation order.
    #[test]
    fn binary64_boundaries_follow_round_half_up_and_are_reproducible() {
        let at_half = composite_hex(screen_layer_over_srgb8([1, 0, 0], 0.5, [0; 3]).unwrap());
        let just_below_half = f64::from_bits(0.5_f64.to_bits() - 1);
        let below =
            composite_hex(screen_layer_over_srgb8([1, 0, 0], just_below_half, [0; 3]).unwrap());
        assert_eq!(below, "#000000");
        assert_eq!(at_half, "#010000", "round(0.5) обязан выбрать верхний байт");

        let states = quantised_composites([1, 0, 0], [0; 3]).unwrap();
        assert_eq!(states.len(), 2);
        assert_eq!(states[0].bytes, [0; 3]);
        assert_eq!(states[0].upper_bits, 0.5_f64.to_bits());
        assert!(!states[0].upper_inclusive);
        assert_eq!(states[1].bytes, [1, 0, 0]);
        assert_eq!(states[1].lower_bits, states[0].upper_bits);
        assert!(states[1].upper_inclusive);

        let white_states = quantised_composites([255; 3], [0; 3]).unwrap();
        assert_eq!(white_states.len(), 256);
        let first_boundary = white_states[0].upper_bits;
        assert_eq!(white_states[1].lower_bits, first_boundary);
        assert_eq!(
            screen_layer_over_srgb8([255; 3], f64::from_bits(first_boundary - 1), [0; 3]).unwrap(),
            [0; 3]
        );
        assert_eq!(
            screen_layer_over_srgb8([255; 3], f64::from_bits(first_boundary), [0; 3]).unwrap(),
            [1; 3]
        );
        assert_eq!(white_states[1].bytes, [1; 3]);
    }

    #[test]
    fn rationally_equal_walls_keep_distinct_binary64_seam_states() {
        // Обе алгебраические стенки равны 255/508, но зафиксированный порядок
        // binary64-операций переводит зелёный канал на 127 ULP раньше красного.
        // Поэтому промежуточный RGB-state достижим и не может быть поглощён
        // группировкой по равенству рациональных дробей.
        let states = quantised_composites([1, 2, 0], [1, 128, 0]).unwrap();
        let seam_index = states
            .iter()
            .position(|state| state.bytes == [1, 129, 0])
            .expect("потерян достижимый binary64 seam-state #018100");
        let seam = states[seam_index];
        assert_eq!(seam.lower_bits, 4_602_696_549_879_841_156);
        assert_eq!(seam.upper_bits, 4_602_696_549_879_841_283);
        assert_eq!(seam.upper_bits - seam.lower_bits, 127);
        assert_eq!(states[seam_index + 1].bytes, [2, 129, 0]);
    }

    #[test]
    fn rational_seed_is_a_stable_performance_hint_not_a_state_definition() {
        for (glow, background, next_value, expected_bits) in [
            (1, 1, 2, 0x3fe0_1020_4081_0204),
            (2, 128, 129, 0x3fe0_1020_4081_0204),
            (33, 131, 132, 0x3f9f_e7f9_fe7f_9fe8),
            (11, 0, 8, 0x3fe5_d174_5d17_45d1),
            (255, 0, 255, 0x3fef_efef_efef_eff0),
        ] {
            assert_eq!(
                boundary_seed_bits(glow, background, next_value),
                expected_bits,
                "glow={glow}, background={background}, next={next_value}",
            );
        }
    }

    #[test]
    fn canonical_alpha_pins_the_midpoint_of_the_actual_binary64_partition() {
        let state = quantised_composites([255, 59, 48], [0; 3])
            .unwrap()
            .into_iter()
            .find(|state| state.bytes == [3, 1, 1])
            .expect("reference state #030101");
        assert_eq!(state.lower_bits, 0x3f85_5555_5555_5555);
        assert_eq!(state.upper_bits, 0x3f8c_1c1c_1c1c_1c1c);
        assert_eq!(state.canonical_alpha().to_bits(), 0x3f88_b8b8_b8b8_b8b8);

        // Если representable state содержит только lower, численный midpoint
        // half-ULP округляется к чётной исключённой upper boundary. Канон обязан
        // вернуть её непосредственного predecessor, то есть единственный state.
        let lower_bits = 0.5_f64.to_bits() + 1;
        let singleton = QuantisedComposite {
            bytes: [0; 3],
            lower_bits,
            upper_bits: lower_bits + 1,
            upper_inclusive: false,
        };
        assert_eq!(singleton.canonical_alpha().to_bits(), lower_bits);
    }

    /// Каждый transition увеличивает хотя бы один из трёх байтов. Поэтому их
    /// суммарно не больше `3·255`, а states — не больше 766. Fixtures включают
    /// длинный поток и ULP-seam, который ломал прежнюю rational grouping.
    #[test]
    fn binary64_stream_respects_the_766_state_bound() {
        for (glow, background) in [
            ([255, 255, 254], [0, 1, 1]),
            ([1, 2, 0], [1, 128, 0]),
            ([255; 3], [0; 3]),
        ] {
            let states = quantised_composites(glow, background).unwrap();
            assert!(
                states.len() <= usize::from(MAX_QUANTISED_COMPOSITE_STATES),
                "glow={glow:?}, background={background:?}, states={}",
                states.len()
            );
            for pair in states.windows(2) {
                assert_eq!(pair[0].upper_bits, pair[1].lower_bits);
                assert!(!pair[0].upper_inclusive);
                assert!(
                    pair[1]
                        .bytes
                        .into_iter()
                        .zip(pair[0].bytes)
                        .all(|(next, previous)| next >= previous)
                );
            }
            assert!(states.last().unwrap().upper_inclusive);
        }

        let solved = solve_legacy(
            "#FFFFFE",
            "#000101",
            101.0,
            &ViewingConditions::dim_surround(),
        )
        .unwrap();
        assert_eq!(solved.status(), GlowTargetStatus::LegacyUnreachable);
        assert!(solved.achieved_dj() < 101.0);
    }

    /// Известная десятичная alpha проверяется независимой целочисленной
    /// формулой на всей одноканальной области. Это ловит нормализацию
    /// `byte/255 → *255`, которая на `250 × 0.122` теряет точную половину.
    #[test]
    fn byte_reference_screen_alpha_0122_matches_exact_rational_for_all_channel_pairs() {
        const ALPHA_NUMERATOR: u64 = 61;
        const ALPHA_DENOMINATOR: u64 = 500;
        const BYTE_MAX: u64 = 255;
        let denominator = ALPHA_DENOMINATOR * BYTE_MAX;

        for glow in 0_u16..=255 {
            for bg in 0_u16..=255 {
                let glow = u64::from(glow);
                let bg = u64::from(bg);
                let numerator = bg * denominator + ALPHA_NUMERATOR * glow * (BYTE_MAX - bg);
                let expected = ((2 * numerator + denominator) / (2 * denominator)) as u8;
                let actual = screen_layer_over_srgb8([glow as u8, 0, 0], 0.122, [bg as u8, 0, 0])
                    .unwrap()[0];
                assert_eq!(actual, expected, "glow={glow}, bg={bg}");
            }
        }

        assert_eq!(
            screen_layer_over_srgb8([192, 178, 250], 0.122, [0; 3]).unwrap(),
            [23, 22, 31]
        );
        let measurement =
            measure_screen_layer_at_alpha("#C0B2FA", "#000000", 0.122, &ViewingConditions::srgb())
                .unwrap();
        assert_eq!(measurement.composite_hex, "#17161F");
    }

    /// На этой границе точное вещественное сравнение binary64 и фиксированный
    /// порядок binary64-операций дают разные классификации. Reference намеренно
    /// фиксирует второй вариант — тот же, который выполняет JS-потребитель;
    /// солвер выбирает внутренние точки интервалов и перепроверяет результат.
    #[test]
    fn byte_screen_pins_reference_operation_order_at_float_wall() {
        let rounded_rational_wall = 0.501_968_503_937_007_9_f64;
        let first_passing = f64::from_bits(rounded_rational_wall.to_bits() - 1);
        let below_first = f64::from_bits(first_passing.to_bits() - 1);
        assert_eq!(
            screen_layer_over_srgb8([1, 0, 0], below_first, [1, 0, 0]).unwrap()[0],
            1
        );
        assert_eq!(
            screen_layer_over_srgb8([1, 0, 0], first_passing, [1, 0, 0]).unwrap()[0],
            2
        );
        assert_eq!(
            screen_layer_over_srgb8([1, 0, 0], rounded_rational_wall, [1, 0, 0],).unwrap()[0],
            2
        );
    }

    /// Исчерпывающая теорема для одного канала: first-passing binary64
    /// partition совпадает с публичной формулой для всех 65 536 пар
    /// `(background, glow)`.
    #[test]
    fn every_binary64_channel_partition_matches_the_public_compositor() {
        for background in 0_u16..=255 {
            for glow in 0_u16..=255 {
                let b = background as u8;
                let g = glow as u8;
                let states = quantised_composites([g, 0, 0], [b, 0, 0]).unwrap();
                let final_byte = screen_layer_over_srgb8([g, 0, 0], 1.0, [b, 0, 0]).unwrap()[0];

                assert_eq!(states.first().unwrap().bytes[0], b);
                assert_eq!(states.last().unwrap().bytes[0], final_byte);
                assert_eq!(states.len(), usize::from(final_byte - b) + 1);

                for (index, state) in states.iter().enumerate() {
                    assert!(state.lower_bits <= state.upper_bits);
                    if let Some(next) = states.get(index + 1) {
                        assert_eq!(state.upper_bits, next.lower_bits);
                        assert!(!state.upper_inclusive);
                        assert_eq!(next.bytes[0], state.bytes[0] + 1);
                        assert_eq!(
                            screen_layer_over_srgb8(
                                [g, 0, 0],
                                f64::from_bits(next.lower_bits),
                                [b, 0, 0],
                            )
                            .unwrap()[0],
                            next.bytes[0],
                            "B={b}, G={g}, state={index}: lower boundary must pass",
                        );
                        assert_eq!(
                            screen_layer_over_srgb8(
                                [g, 0, 0],
                                f64::from_bits(next.lower_bits - 1),
                                [b, 0, 0],
                            )
                            .unwrap()[0],
                            state.bytes[0],
                            "B={b}, G={g}, state={index}: predecessor must fail",
                        );
                    }

                    let alpha = state.canonical_alpha();
                    let out = screen_layer_over_srgb8([g, 0, 0], alpha, [b, 0, 0]).unwrap();
                    assert_eq!(
                        out[0], state.bytes[0],
                        "B={b}, G={g}, state={index}, alpha={alpha}"
                    );
                }
            }
        }
    }

    /// Солвер достигает контрактной цели на тёмном фоне (среда свечения).
    #[test]
    fn solver_hits_targets_on_dark() {
        let vc = ViewingConditions::dim_surround();
        for target in [GLOW_SUBTLE_DJ, GLOW_BASE_DJ, GLOW_BLOOM_DJ] {
            let g = solve_legacy("#3E87FF", "#101012", target, &vc).unwrap();
            assert!(!g.degraded(), "цель {target} недостижима на #101012");
            assert_eq!(g.status(), GlowTargetStatus::LegacyReached);
            assert_eq!(
                g.selection_diagnostic_profile(),
                Some(GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1)
            );
            // Первый достижимый sRGB8-композит обязан держать цель. Его
            // минимальность независимо проверяется sweep-оракулом ниже.
            assert!(
                g.achieved_dj() >= target,
                "цель {target}: достигнуто {:.4}",
                g.achieved_dj()
            );
            assert!(g.alpha() > 0.0 && g.alpha() <= 1.0);
        }
    }

    /// Регрессия реальной границы квантования: альфа, округлённая отдельно от
    /// рассчитанного hex, не имеет права незаметно вернуть предыдущий байт.
    #[test]
    fn serialised_alpha_keeps_the_quantised_target() {
        let vc = ViewingConditions::dim_surround();
        let tint = encoded_bytes(srgb_encoded_from_hex("#4A8FFF").unwrap());
        let bg = encoded_bytes(srgb_encoded_from_hex("#101012").unwrap());
        let bg_jp = LcsColor::from_hex_with_vc("#101012", &vc).unwrap().jp;
        let measured_at = |alpha: f64| {
            let hex = composite_hex(screen_layer_over_srgb8(tint, alpha, bg).unwrap());
            let jp = LcsColor::from_hex_with_vc(&hex, &vc).unwrap().jp;
            ((jp - bg_jp).abs(), hex)
        };

        // Прежняя бисекция сходилась к рациональной стенке красного канала
        // 1275/35372. Формат DTO с четырьмя знаками превращал её в 0.0360.
        let legacy_boundary_alpha = 1_275.0 / 35_372.0;
        let legacy_css = format!("{legacy_boundary_alpha:.4}");
        assert_eq!(legacy_css, "0.0360");
        let (rounded_dj, rounded_hex) = measured_at(legacy_css.parse().unwrap());
        assert_eq!(rounded_hex, "#12151B");
        assert!(
            rounded_dj < GLOW_BASE_DJ,
            "контрпример обязан лежать ниже цели: {rounded_dj}"
        );

        let solved = solve_legacy("#4A8FFF", "#101012", GLOW_BASE_DJ, &vc).unwrap();
        let alpha_css = solved.alpha_css();
        let serialised = alpha_css.parse::<f64>().unwrap();
        assert_eq!(serialised.to_bits(), solved.alpha().to_bits());
        let (serialised_dj, serialised_hex) = measured_at(serialised);
        assert_eq!(serialised_hex, solved.composite_hex());
        assert!(
            serialised_dj >= GLOW_BASE_DJ,
            "эмитируемая alpha={alpha_css} дала {serialised_hex} с ΔJ'={serialised_dj}, ниже цели {}",
            GLOW_BASE_DJ
        );
    }

    /// Матрица свойств сверяет минимальный успешный state и глобальный
    /// максимум при деградации с независимым оракулом. В ней есть нулевой тинт,
    /// белый фон и разные surround — ветви, где предположение `alpha=1` особенно
    /// легко скрывает ошибку выбора.
    #[test]
    fn solver_matches_independent_finite_state_oracle() {
        let vcs = [ViewingConditions::srgb(), ViewingConditions::dim_surround()];
        for tint_hex in ["#000000", "#010200", "#4A8FFF", "#FF3B30", "#FFFFFF"] {
            for bg_hex in ["#000000", "#018000", "#101012", "#808080", "#FFFFFF"] {
                for vc in vcs {
                    let oracle = oracle_states(tint_hex, bg_hex, &vc);
                    assert!(!oracle.is_empty());
                    let mut global_best = &oracle[0];
                    for state in &oracle[1..] {
                        if state.2 > global_best.2 {
                            global_best = state;
                        }
                    }

                    for target in [0.01, 0.32, 1.0, 10.0, global_best.2 + 1.0] {
                        let solved = solve_legacy(tint_hex, bg_hex, target, &vc).unwrap();
                        let expected = oracle.iter().find(|state| state.2 >= target);
                        let (expected_state, degraded) = match expected {
                            Some(state) => (state, false),
                            None => (global_best, true),
                        };
                        assert_eq!(
                            solved.composite_hex(),
                            expected_state.1,
                            "tint={tint_hex}, bg={bg_hex}, target={target}"
                        );
                        assert_eq!(solved.achieved_dj().to_bits(), expected_state.2.to_bits());
                        assert_eq!(solved.degraded(), degraded);

                        let emitted_alpha = solved.alpha_css().parse::<f64>().unwrap();
                        assert_eq!(
                            emitted_alpha.to_bits(),
                            solved.alpha().to_bits(),
                            "CSS round-trip обязан сохранять binary64: tint={tint_hex}, bg={bg_hex}, target={target}"
                        );
                        let recomposed = composite_hex(
                            screen_layer_over_srgb8(
                                encoded_bytes(srgb_encoded_from_hex(tint_hex).unwrap()),
                                emitted_alpha,
                                encoded_bytes(srgb_encoded_from_hex(bg_hex).unwrap()),
                            )
                            .unwrap(),
                        );
                        assert_eq!(recomposed, solved.composite_hex());

                        let certificate = solved.composite_certificate();
                        assert_eq!(certificate.profile().key(), GLOW_COMPOSITE_PROFILE);
                        assert_eq!(certificate.guarantee(), GlowCompositeGuaranteeV1::BitExact);
                        assert_eq!(
                            certificate.tint_srgb8(),
                            encoded_bytes(srgb_encoded_from_hex(tint_hex).unwrap())
                        );
                        assert_eq!(
                            certificate.background_srgb8(),
                            encoded_bytes(srgb_encoded_from_hex(bg_hex).unwrap())
                        );
                        assert_eq!(certificate.alpha_bits(), emitted_alpha.to_bits());
                        assert_eq!(certificate.alpha_css(), solved.alpha_css());
                        assert_eq!(
                            composite_hex(certificate.composite_srgb8()),
                            solved.composite_hex()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn solver_rejects_non_finite_and_non_positive_targets() {
        let vc = ViewingConditions::dim_surround();
        for target in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0] {
            assert!(solve_legacy("#4A8FFF", "#101012", target, &vc).is_err());
            assert!(
                solve_screen_alpha_for_dj(
                    "#4A8FFF",
                    "#101012",
                    target,
                    GlowDecisionProfileV1::StableV1.execution_mode(),
                    &vc,
                )
                .is_err(),
                "stable profile accepted invalid target {target}"
            );
        }

        let mut invalid_vc = vc;
        invalid_vc.n = f64::NAN;
        assert!(solve_legacy("#4A8FFF", "#101012", GLOW_BASE_DJ, &invalid_vc).is_err());
    }

    /// Ступени контракта строго возрастают и по цели, и по решённой α.
    #[test]
    fn glow_stack_is_strictly_progressive() {
        const { assert!(GLOW_SUBTLE_DJ < GLOW_BASE_DJ && GLOW_BASE_DJ < GLOW_BLOOM_DJ) };
        let vc = ViewingConditions::dim_surround();
        let mut prev = 0.0;
        for target in [GLOW_SUBTLE_DJ, GLOW_BASE_DJ, GLOW_BLOOM_DJ] {
            let g = solve_legacy("#FF3B30", "#101012", target, &vc).unwrap();
            assert!(
                g.alpha() > prev,
                "α стека не прогрессивна: {} после {prev}",
                g.alpha()
            );
            prev = g.alpha();
        }
    }

    /// На белом фоне screen-слой является point-no-op по объявленной формуле;
    /// недостижимость поэтому должна быть явной, а не замаскированной ошибкой.
    #[test]
    fn glow_on_white_degrades_honestly() {
        let vc = ViewingConditions::srgb();
        let g = solve_legacy("#3E87FF", "#FFFFFF", GLOW_BASE_DJ, &vc).unwrap();
        assert!(g.degraded(), "над белым screen обязан быть point-no-op");
        assert_eq!(g.status(), GlowTargetStatus::LegacyUnreachable);
        assert!(g.achieved_dj() < GLOW_BASE_DJ);
        assert_eq!(g.composite_hex(), "#FFFFFF", "screen над белым — тождество");
        assert_eq!(
            g.alpha(),
            0.5,
            "единственный state канонизируется центром [0,1]"
        );
    }

    /// Recipe v1: core светлее источника, использует его Oklab hue и
    /// gamut-boundary chroma без тихого клипа. Тест фиксирует алгоритм, а не
    /// проверенный на наблюдателях закон оптимального свечения.
    #[test]
    fn core_is_balanced_overexposed_source() {
        let vc = ViewingConditions::dim_surround();
        let (core_hex, halo_hex) = glow_layers_from_source("#FF3B30", &vc).unwrap();
        assert_eq!(halo_hex, "#FF3B30", "halo — сам источник");
        let src = LcsColor::from_hex_with_vc("#FF3B30", &vc).unwrap();
        let core = LcsColor::from_hex_with_vc(&core_hex, &vc).unwrap();
        assert!(core.jp > src.jp, "core светлее источника (пересвет)");
        let dh = (core.h_ok - src.h_ok + 180.0).rem_euclid(360.0) - 180.0;
        assert!(dh.abs() < 6.0, "оттенок унаследован: Δh = {dh:.2}°");

        // Хрома баланса = стена гамута ⇒ эмиссия в гамуте, без тихого клипа
        // (прежняя ×0.5-доля могла запросить недостижимую красочность и молча
        // срезаться в to_hex). Round-trip красочности центра стабилен.
        let core_reparsed = LcsColor::from_hex_with_vc(&core_hex, &vc).unwrap();
        assert!(
            (core_reparsed.mp() - core.mp()).abs() < 0.5,
            "центр в гамуте: round-trip красочности стабилен (без тихого клипа)"
        );
    }

    #[test]
    fn layer_recipe_canonicalises_the_public_source_hex() {
        let vc = ViewingConditions::srgb();
        let (core_hex, halo_hex) = glow_layers_from_source("ff3b30", &vc).unwrap();
        assert_eq!(halo_hex, "#FF3B30");
        assert!(core_hex.starts_with('#'));
        assert_eq!(core_hex.len(), 7);
    }

    /// У sRGB8-серого нет определённого hue. Матричный шум Oklab не должен
    /// превращаться в насыщенный core ни на одном байте и ни в одной штатной VC.
    #[test]
    fn exact_achromatic_sources_never_receive_an_invented_core_hue() {
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            for byte in u8::MIN..=u8::MAX {
                let source = format!("#{byte:02X}{byte:02X}{byte:02X}");
                let (core, halo) = glow_layers_from_source(&source, &vc).unwrap();
                let [r, g, b] = encoded_bytes(srgb_encoded_from_hex(&core).unwrap());
                assert_eq!(halo, source);
                assert_eq!(r, g, "{source} дал цветной core {core}");
                assert_eq!(g, b, "{source} дал цветной core {core}");
            }
        }
    }

    /// Публичные поля VC позволяют внешнему коду создать несогласованное
    /// производное состояние. Такое состояние раньше протаскивало NaN через
    /// CAM16 и маскировало его квантизацией в правдоподобный `#000000`.
    #[test]
    fn layer_recipe_rejects_numerically_degenerate_viewing_conditions() {
        let mut vc = ViewingConditions::srgb();
        vc.fl = 1.0e308;
        vc.fl_pow_025 = 1.0e308;
        assert!(glow_layers_from_source("#4A8FFF", &vc).is_err());
    }
}
