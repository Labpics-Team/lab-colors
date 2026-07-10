//! Конечный point-reference примитив для screen-рецепта glow.
//!
//! Модуль рассчитывает encoded-композит двух слоёв. Он не моделирует
//! физическое излучение, blur-поле или восприятие полного пространственного
//! эффекта: эти утверждения требуют render/context-контракта #221.
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
//! - на белом (1−bg)=0 слой является point-no-op именно по формуле screen;
//!   это численное свойство reference-профиля, не утверждение о физическом
//!   свете или неизвестном renderer.
//!
//! # Lab UI preset ступеней
//!
//! Текущие |ΔJ′| взяты из point-композитов клиентского стека теней Lab UI:
//! `subtle:=minor`, `base:=ambient`, `bloom:=major`. Это воспроизводимая
//! история preset, но не универсальный psychophysical law и не taxonomy
//! generic core; перенос в client profile отслеживается #221.
//!
//! # Анатомия (двухслойный bloom)
//!
//! В текущем клиентском recipe core — цвет с поднятой к белому светлотой, halo
//! — исходный цвет. Радиусы и blur здесь отсутствуют: названия обозначают
//! назначение слоёв у потребителя, а не измеренную геометрию.
//!
//! Core строится существующей policy [`crate::accent_balance`]: midpoint J′
//! задаёт seed Oklab-светлоты, chroma берётся на sRGB-границе данного hue.
//! После хроматического преобразования и sRGB8-квантования итоговый J′ не
//! объявляется равным seed — фактическая point-метрика core возвращается
//! отдельно. Это versioned recipe, а не оптимум, проверенный на наблюдателях.

use crate::lcs::LcsColor;
use crate::spaces::srgb::{decode_8bit, hex_from_srgb_encoded, srgb_encoded_from_hex, srgb_to_xyz};
use crate::spaces::vc::ViewingConditions;
use std::cmp::Ordering;

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

/// Версия конечного reference-домена, в котором решается point-композит glow.
///
/// Это не сертификат конкретного браузера или дисплея: профиль фиксирует
/// encoded sRGB8, оператор screen и CAM16-UCS J′ под переданными условиями
/// просмотра. Пространственный render-контракт живёт отдельно в #221.
pub const GLOW_REFERENCE_PROFILE: &str = "encoded-srgb8-screen-cam16ucs-jprime-v1";

/// Слой, по которому point-солвер держит целевой ΔJ′.
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

/// Итог проверки target по конечному reference-домену.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GlowTargetStatus {
    /// Найден первый по интенсивности sRGB8-state, держащий цель.
    Reached,
    /// Цель не держит ни один state; возвращён глобальный максимум |ΔJ′|, а
    /// при точном равенстве максимумов — первый state по alpha.
    Unreachable,
}

impl GlowTargetStatus {
    /// Стабильный wire-ключ.
    pub fn key(self) -> &'static str {
        match self {
            Self::Reached => "reached",
            Self::Unreachable => "unreachable",
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
    /// Целевой модуль ΔJ′ CAM16-UCS изолированного halo point-композита.
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
    // физическую применимость viewing context: такая граница принадлежит #230.
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
    Ok(())
}

/// Только J′ для горячего пути конечного солвера. Oklab-hue и M′ здесь не
/// участвуют в objective, поэтому полный `LcsColor` был бы лишней работой на
/// каждом из сотен sRGB8-state. Формула J′ остаётся тем же SSOT из `cam16`.
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

/// Screen-слой над непрозрачным фоном: `bg + α·G·(1−bg)` покомпонентно
/// в reference-домене encoded sRGB. Функция не утверждает, что неизвестный
/// renderer использует тот же compositing/output profile.
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

/// Изолированный point-замер одного screen-слоя в reference-профиле glow.
/// Пространственное перекрытие core/halo сюда намеренно не входит: без
/// геометрии и порядка слоёв его нельзя восстановить честно.
pub(crate) struct ScreenLayerMeasurement {
    pub(crate) composite_hex: String,
    pub(crate) achieved_dj: f64,
}

pub(crate) fn measure_screen_layer_at_alpha(
    tint_hex: &str,
    bg_hex: &str,
    alpha: f64,
    vc: &ViewingConditions,
) -> Result<ScreenLayerMeasurement, String> {
    validate_viewing_numerics(vc)?;
    let tint = srgb_encoded_from_hex(tint_hex)?;
    let bg = srgb_encoded_from_hex(bg_hex)?;
    let composite_hex = hex_from_srgb_encoded(screen_layer_over_encoded(tint, alpha, bg)?);
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
    })
}

/// Рациональная стенка квантования α. Целые здесь принципиальны: двоичная
/// аппроксимация десятичной границы не должна решать, какой байт существует.
#[derive(Debug, Clone, Copy)]
struct AlphaWall {
    numerator: u64,
    denominator: u64,
}

impl AlphaWall {
    const ZERO: Self = Self {
        numerator: 0,
        denominator: 1,
    };
    const ONE: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    fn cmp(self, other: Self) -> Ordering {
        (self.numerator * other.denominator).cmp(&(other.numerator * self.denominator))
    }

    fn same_value(self, other: Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }

    fn as_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }

    /// Центр интервала максимизирует запас до обеих стенок. Это канонизация
    /// представления, а не утверждение о перцептивно «лучшей» непрозрачности.
    fn midpoint(self, other: Self) -> f64 {
        if self.same_value(other) {
            return self.as_f64();
        }
        let numerator = self.numerator * other.denominator + other.numerator * self.denominator;
        let denominator = 2 * self.denominator * other.denominator;
        numerator as f64 / denominator as f64
    }
}

#[derive(Debug, Clone, Copy)]
struct QuantisedComposite {
    bytes: [u8; 3],
    lower: AlphaWall,
    upper: AlphaWall,
}

fn encoded_bytes(rgb: [f64; 3]) -> [u8; 3] {
    rgb.map(|channel| (channel * 255.0).round() as u8)
}

fn composite_hex(bytes: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", bytes[0], bytes[1], bytes[2])
}

/// Поток всех достижимых sRGB8-композитов без промежуточных коллекций.
///
/// На каждом канале стенки строго возрастают, поэтому три уже упорядоченных
/// потока достаточно слить тремя курсорами. Общая сортировка не добавляет
/// математической информации, зато затягивает в WASM аллокатор и универсальную
/// сортировку. Сравнение и группировка остаются рациональными: решение о байте
/// никогда не зависит от погрешности binary64.
struct QuantisedComposites {
    background: [u8; 3],
    slopes: [u64; 3],
    next_values: [u16; 3],
    bytes: [u8; 3],
    lower: AlphaWall,
    finished: bool,
}

impl QuantisedComposites {
    fn new(glow: [u8; 3], background: [u8; 3]) -> Self {
        let slopes = core::array::from_fn(|channel| {
            u64::from(glow[channel]) * (255 - u64::from(background[channel]))
        });
        let next_values = background.map(|byte| u16::from(byte) + 1);
        Self {
            background,
            slopes,
            next_values,
            bytes: background,
            lower: AlphaWall::ZERO,
            finished: false,
        }
    }

    /// На канале `B + α·G·(255−B)/255` новый байт `v` начинается ровно на
    /// `255·(2(v−B)−1) / (2·G·(255−B))`. Все множители ограничены 8 битами,
    /// поэтому точное перекрёстное сравнение стенок помещается в `u64`.
    fn next_wall(&self, channel: usize) -> Option<AlphaWall> {
        let slope = self.slopes[channel];
        let value = self.next_values[channel];
        if slope == 0 || value > u16::from(u8::MAX) {
            return None;
        }

        let base = u64::from(self.background[channel]);
        let delta = u64::from(value) - base;
        let wall = AlphaWall {
            numerator: 255 * (2 * delta - 1),
            denominator: 2 * slope,
        };
        (wall.cmp(AlphaWall::ONE) != Ordering::Greater).then_some(wall)
    }

    /// Возвращает очередной интервал `[lower, upper)`; последний интервал
    /// замкнут справа и заканчивается на `1`. Стенки с одинаковым рациональным
    /// значением поглощаются вместе, как в прежней сортированной реализации.
    fn next_state(&mut self) -> Result<Option<QuantisedComposite>, String> {
        if self.finished {
            return Ok(None);
        }

        let upper = (0..3)
            .filter_map(|channel| self.next_wall(channel).map(|wall| (wall, channel)))
            .min_by(|(left_wall, left_channel), (right_wall, right_channel)| {
                left_wall
                    .cmp(*right_wall)
                    .then_with(|| left_channel.cmp(right_channel))
            })
            .map(|(wall, _)| wall);

        let Some(upper) = upper else {
            self.finished = true;
            return Ok(Some(QuantisedComposite {
                bytes: self.bytes,
                lower: self.lower,
                upper: AlphaWall::ONE,
            }));
        };

        let state = QuantisedComposite {
            bytes: self.bytes,
            lower: self.lower,
            upper,
        };
        for channel in 0..3 {
            if self
                .next_wall(channel)
                .is_some_and(|wall| wall.same_value(upper))
            {
                self.bytes[channel] = self.bytes[channel].checked_add(1).ok_or_else(|| {
                    format!("внутренняя стенка переполнила sRGB8-канал {channel}")
                })?;
                self.next_values[channel] += 1;
            }
        }
        self.lower = upper;
        Ok(Some(state))
    }
}

/// Тестовый сборщик сохраняет удобный независимый оракул, но `Vec` и сортировка
/// не входят в production-WASM.
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

/// Результат glow-солвера (ADR-0002 «честный результат», закон 2).
#[derive(Debug, Clone, PartialEq)]
pub struct GlowSolve {
    /// Каноническая интенсивность слоя `(0, 1]`: центр устойчивого интервала
    /// выбранного sRGB8-композита. Это не «оптимальный вкус», а максимальный
    /// численный запас до ближайшей границы квантования.
    alpha: f64,
    /// Каноническая CSS-запись той же alpha; хранится вместе с числом, чтобы
    /// downstream не вводил собственную политику округления.
    alpha_css: String,
    /// Запрошенный |ΔJ′| point-композита halo.
    target_dj: f64,
    /// Фактический |ΔJ'| композита от фона, замерен на эмитируемом hex.
    achieved_dj: f64,
    /// Композит `screen(tint, α)` над фоном, `#RRGGBB`.
    composite_hex: String,
    /// Деградация: ни один достижимый sRGB8-композит не держит цель (например,
    /// над белым фоном слой является point-no-op в этом reference-профиле);
    /// возвращён глобальный максимум |ΔJ'| с честным флагом, НЕ ошибка и НЕ
    /// молчание.
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

    /// Типизированный результат target-проверки.
    pub fn status(&self) -> GlowTargetStatus {
        self.status
    }

    /// Геттер совместимости прежнего boolean-контракта.
    pub fn degraded(&self) -> bool {
        self.status == GlowTargetStatus::Unreachable
    }

    /// Слой, по которому решалась цель.
    pub fn constraint_layer(&self) -> GlowConstraintLayer {
        GlowConstraintLayer::Halo
    }

    /// Версия конечного домена расчёта.
    pub fn reference_profile(&self) -> &'static str {
        GLOW_REFERENCE_PROFILE
    }
}

/// Решить интенсивность screen-слоя под целевой численный модуль ΔJ′ CAM16-UCS.
///
/// Канальные переходы screen выводятся как рациональные стенки округления до
/// sRGB8. Солвер проверяет все достижимые композиты в порядке α и потому не
/// предполагает монотонность CAM16 J'. Если цель недостижима, возвращается
/// глобальный максимум |ΔJ'| среди достижимых состояний; равные максимумы
/// детерминированно разрешаются первым state по alpha.
///
/// # Errors
///
/// `Err` — программный мусор (закон 3 ADR-0002): невалидный hex, неконечная
/// или неположительная цель либо нечисловой результат переданных условий
/// просмотра.
pub fn solve_screen_alpha_for_dj(
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
    let slopes = core::array::from_fn::<_, 3, _>(|channel| {
        u16::from(glow_bytes[channel]) * u16::from(u8::MAX - bg_bytes[channel])
    });
    if slopes == [0; 3] {
        // Весь α-интервал отображается в один и тот же sRGB8-state. CAM16
        // forward ничего не добавит: ΔJ′ одного цвета с самим собой точно 0.
        let alpha = AlphaWall::ZERO.midpoint(AlphaWall::ONE);
        return Ok(GlowSolve {
            alpha,
            alpha_css: crate::css_alpha_value(alpha)?,
            target_dj,
            achieved_dj: 0.0,
            composite_hex: composite_hex(bg_bytes),
            status: GlowTargetStatus::Unreachable,
        });
    }
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
        (state, achieved_dj, GlowTargetStatus::Reached)
    } else {
        let (state, achieved_dj) =
            best.ok_or_else(|| "конечное множество композитов оказалось пустым".to_string())?;
        (state, achieved_dj, GlowTargetStatus::Unreachable)
    };
    let composite_hex = composite_hex(state.bytes);
    let alpha = state.lower.midpoint(state.upper);

    // Канонический CSS-сериализатор хранит ту же binary64 alpha. Production-проверка
    // повторно композитит исходное число; строковый parse-round-trip проверяют
    // граничные и межъязыковые тесты, не затягивая dec2flt-парсер в WASM.
    let alpha_css = crate::css_alpha_value(alpha)?;
    let roundtrip_hex = hex_from_srgb_encoded(screen_layer_over_encoded(glow, alpha, bg)?);
    if roundtrip_hex != composite_hex {
        return Err(format!(
            "выбранная alpha не воспроизвела state: ожидался {composite_hex}, получен {roundtrip_hex}"
        ));
    }

    Ok(GlowSolve {
        alpha,
        alpha_css,
        target_dj,
        achieved_dj,
        composite_hex,
        status,
    })
}

/// Версионированный двухслойный recipe от источника: `(core_hex, halo_hex)`.
///
/// В recipe v1 halo буквально равен источнику. Для core арифметическая середина
/// J′ источника и 100 задаёт seed Oklab-светлоты, а chroma — sRGB-границу его
/// Oklab hue через [`crate::accent_balance::accent_balanced`]. Фактический J′
/// эмитированного core измеряется вызывающим кодом: он не обязан совпасть с seed
/// после смены координат и sRGB8-квантования. Это recipe, не наблюдательная
/// модель красоты или геометрии свечения.
pub fn glow_layers_from_source(
    source_hex: &str,
    vc: &ViewingConditions,
) -> Result<(String, String), String> {
    validate_viewing_numerics(vc)?;
    let source_encoded = srgb_encoded_from_hex(source_hex)?;
    let canonical_source_hex = hex_from_srgb_encoded(source_encoded);
    let src = LcsColor::from_hex_with_vc(&canonical_source_hex, vc)?;
    let jp_core = (src.jp + 100.0) * 0.5;
    let l_core = crate::scale::jp_to_oklab_l(jp_core, vc);
    let core = crate::accent_balance::accent_balanced(l_core, src.h_ok, vc).color;
    let core_hex = core.to_hex_with_vc(vc);
    Ok((core_hex, canonical_source_hex))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Независимый оракул не меняет байты по событиям production-алгоритма:
    /// он строит открытые промежутки обычной формулой, пробует их середины через
    /// публичный композитор и удаляет только соседние дубликаты hex.
    fn oracle_states(
        tint_hex: &str,
        bg_hex: &str,
        vc: &ViewingConditions,
    ) -> Vec<(f64, String, f64)> {
        let tint = srgb_encoded_from_hex(tint_hex).unwrap();
        let bg = srgb_encoded_from_hex(bg_hex).unwrap();
        let tint_bytes = encoded_bytes(tint);
        let bg_bytes = encoded_bytes(bg);
        let mut walls = vec![0.0, 1.0];

        for channel in 0..3 {
            let slope = f64::from(tint_bytes[channel]) * f64::from(u8::MAX - bg_bytes[channel]);
            if slope == 0.0 {
                continue;
            }
            for value in (u16::from(bg_bytes[channel]) + 1)..=u16::from(u8::MAX) {
                let wall = 255.0 * (f64::from(value) - f64::from(bg_bytes[channel]) - 0.5) / slope;
                if wall > 1.0 {
                    break;
                }
                walls.push(wall);
            }
        }
        walls.sort_unstable_by(f64::total_cmp);
        walls.dedup();

        let bg_jp = LcsColor::from_hex_with_vc(bg_hex, vc).unwrap().jp;
        let mut states: Vec<(f64, String, f64)> = Vec::new();
        for window in walls.windows(2) {
            if window[0] == window[1] {
                continue;
            }
            let alpha = window[0] + (window[1] - window[0]) * 0.5;
            let hex = hex_from_srgb_encoded(screen_layer_over_encoded(tint, alpha, bg).unwrap());
            if states
                .last()
                .is_some_and(|(_, previous, _)| previous == &hex)
            {
                continue;
            }
            let jp = LcsColor::from_hex_with_vc(&hex, vc).unwrap().jp;
            states.push((alpha, hex, (jp - bg_jp).abs()));
        }
        // Отдельный endpoint не даёт оракулу молча потерять состояние, которое
        // теоретически могло бы существовать только при полностью непрозрачном слое.
        let alpha = 1.0;
        let hex = hex_from_srgb_encoded(screen_layer_over_encoded(tint, alpha, bg).unwrap());
        if states
            .last()
            .is_none_or(|(_, previous, _)| previous != &hex)
        {
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
        }
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.1] {
            let mut rgb = valid;
            rgb[1] = bad;
            assert!(screen_layer_over_encoded(rgb, 0.5, valid).is_err());
            assert!(screen_layer_over_encoded(valid, 0.5, rgb).is_err());
        }
        assert!(screen_layer_over_encoded(valid, 0.0, valid).is_ok());
        assert!(screen_layer_over_encoded(valid, 1.0, valid).is_ok());
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

    /// Точка `x.5` принадлежит новому байту. Проверка на представимой стенке
    /// отделяет это правило от погрешности binary64, а `1/510` ниже проверяет
    /// рациональную группировку непредставимой стенки трёх каналов.
    #[test]
    fn exact_walls_follow_round_half_up_without_binary_boundary_assumptions() {
        let red = [1.0 / 255.0, 0.0, 0.0];
        let black = [0.0; 3];
        let at_half = hex_from_srgb_encoded(screen_layer_over_encoded(red, 0.5, black).unwrap());
        let just_below_half = f64::from_bits(0.5_f64.to_bits() - 1);
        let below =
            hex_from_srgb_encoded(screen_layer_over_encoded(red, just_below_half, black).unwrap());
        assert_eq!(below, "#000000");
        assert_eq!(at_half, "#010000", "round(0.5) обязан выбрать верхний байт");

        let states = quantised_composites([1, 0, 0], [0; 3]).unwrap();
        assert_eq!(states.len(), 2);
        assert_eq!(states[0].bytes, [0; 3]);
        assert!(states[0].upper.same_value(AlphaWall {
            numerator: 1,
            denominator: 2,
        }));
        assert_eq!(states[1].bytes, [1, 0, 0]);
        assert!(states[1].lower.same_value(states[0].upper));

        let white_states = quantised_composites([255; 3], [0; 3]).unwrap();
        assert_eq!(white_states.len(), 256);
        let first_wall = AlphaWall {
            numerator: 1,
            denominator: 510,
        };
        assert!(white_states[0].upper.same_value(first_wall));
        assert!(white_states[1].lower.same_value(first_wall));
        assert_eq!(white_states[1].bytes, [1; 3]);
    }

    /// Исчерпывающая теорема для одного канала: рациональное разбиение обязано
    /// совпадать с публичной формулой для всех 65 536 пар `(background, glow)`.
    /// Три канала независимы по определению screen, поэтому этот перебор
    /// закрывает полный класс стенок без случайной выборки RGB-троек.
    #[test]
    fn every_rational_channel_partition_matches_the_public_compositor() {
        for background in 0_u16..=255 {
            for glow in 0_u16..=255 {
                let b = background as u8;
                let g = glow as u8;
                let states = quantised_composites([g, 0, 0], [b, 0, 0]).unwrap();
                let at_one = screen_layer_over_encoded(
                    [f64::from(g) / 255.0, 0.0, 0.0],
                    1.0,
                    [f64::from(b) / 255.0, 0.0, 0.0],
                )
                .unwrap();
                let final_byte = (at_one[0] * 255.0).round() as u8;

                assert_eq!(states.first().unwrap().bytes[0], b);
                assert_eq!(states.last().unwrap().bytes[0], final_byte);
                assert_eq!(states.len(), usize::from(final_byte - b) + 1);

                for (index, state) in states.iter().enumerate() {
                    assert!(state.lower.cmp(state.upper) != Ordering::Greater);
                    if let Some(next) = states.get(index + 1) {
                        assert!(state.upper.same_value(next.lower));
                        assert_eq!(next.bytes[0], state.bytes[0] + 1);
                    }

                    let alpha = state.lower.midpoint(state.upper);
                    let out = screen_layer_over_encoded(
                        [f64::from(g) / 255.0, 0.0, 0.0],
                        alpha,
                        [f64::from(b) / 255.0, 0.0, 0.0],
                    )
                    .unwrap();
                    assert_eq!(
                        (out[0] * 255.0).round() as u8,
                        state.bytes[0],
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
            let g = solve_screen_alpha_for_dj("#3E87FF", "#101012", target, &vc).unwrap();
            assert!(!g.degraded(), "цель {target} недостижима на #101012");
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
        let tint = srgb_encoded_from_hex("#4A8FFF").unwrap();
        let bg = srgb_encoded_from_hex("#101012").unwrap();
        let bg_jp = LcsColor::from_hex_with_vc("#101012", &vc).unwrap().jp;
        let measured_at = |alpha: f64| {
            let hex = hex_from_srgb_encoded(screen_layer_over_encoded(tint, alpha, bg).unwrap());
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

        let solved = solve_screen_alpha_for_dj("#4A8FFF", "#101012", GLOW_BASE_DJ, &vc).unwrap();
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
        for tint_hex in ["#000000", "#4A8FFF", "#FF3B30", "#FFFFFF"] {
            for bg_hex in ["#000000", "#101012", "#808080", "#FFFFFF"] {
                for vc in vcs {
                    let oracle = oracle_states(tint_hex, bg_hex, &vc);
                    assert!(!oracle.is_empty());
                    let mut global_best = &oracle[0];
                    for state in &oracle[1..] {
                        if state.2 > global_best.2 {
                            global_best = state;
                        }
                    }

                    for target in [0.01, 1.0, 10.0, global_best.2 + 1.0] {
                        let solved =
                            solve_screen_alpha_for_dj(tint_hex, bg_hex, target, &vc).unwrap();
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
                        let recomposed = hex_from_srgb_encoded(
                            screen_layer_over_encoded(
                                srgb_encoded_from_hex(tint_hex).unwrap(),
                                emitted_alpha,
                                srgb_encoded_from_hex(bg_hex).unwrap(),
                            )
                            .unwrap(),
                        );
                        assert_eq!(recomposed, solved.composite_hex());
                    }
                }
            }
        }
    }

    #[test]
    fn solver_rejects_non_finite_and_non_positive_targets() {
        let vc = ViewingConditions::dim_surround();
        for target in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0] {
            assert!(solve_screen_alpha_for_dj("#4A8FFF", "#101012", target, &vc).is_err());
        }

        let mut invalid_vc = vc;
        invalid_vc.n = f64::NAN;
        assert!(
            solve_screen_alpha_for_dj("#4A8FFF", "#101012", GLOW_BASE_DJ, &invalid_vc).is_err()
        );
    }

    /// Ступени контракта строго возрастают и по цели, и по решённой α.
    #[test]
    fn glow_stack_is_strictly_progressive() {
        const { assert!(GLOW_SUBTLE_DJ < GLOW_BASE_DJ && GLOW_BASE_DJ < GLOW_BLOOM_DJ) };
        let vc = ViewingConditions::dim_surround();
        let mut prev = 0.0;
        for target in [GLOW_SUBTLE_DJ, GLOW_BASE_DJ, GLOW_BLOOM_DJ] {
            let g = solve_screen_alpha_for_dj("#FF3B30", "#101012", target, &vc).unwrap();
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
        let g = solve_screen_alpha_for_dj("#3E87FF", "#FFFFFF", GLOW_BASE_DJ, &vc).unwrap();
        assert!(g.degraded(), "над белым screen обязан быть point-no-op");
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
}
