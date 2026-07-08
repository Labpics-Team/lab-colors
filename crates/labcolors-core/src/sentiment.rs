// oklab_hue_of: единая реализация формулы якорного оттенка живёт в палитре
// акцентов — сентименты потребляют её, не держат вторую копию физики.
#[cfg(test)]
use crate::accent::Accent;
use crate::accent::oklab_hue_of;
use crate::lcs::LcsColor;
use crate::neutral::NeutralCurve;
use crate::scale::{jp_to_oklab_l, max_chroma};
use crate::spaces::oklab::{oklab_to_srgb_linear, srgb_linear_to_oklab};
use crate::spaces::srgb::{
    hex_from_srgb, hex_from_srgb_encoded, srgb_encoded_from_hex, srgb_from_hex, srgb_to_xyz,
};
use crate::spaces::vc::ViewingConditions;

/// Перцептивный минимум разделения между оттенком сентимента и брендовым
/// оттенком, выраженный как **длина хорды в плоскости Oklab a/b** (не в
/// градусах).
///
/// # Почему хорда, а не угол
///
/// Issue #20: одинаковые угловые сдвиги хроматически не равноценны — поворот
/// на 20° при низкой хроме почти незаметен, тогда как при высокой хроме это
/// очевидная смена цвета. Фиксированный угловой порог поэтому создаёт
/// избыточное разделение в десатурированных зонах и недостаточное в
/// насыщенных. Перцептивно честный инвариант — постоянное *расстояние* в
/// плоскости (a, b), которое затем переводится в зонный угол.
///
/// # Деривация
///
/// ```text
/// S_PERC_MIN = 2 · C_rep_figma · sin(20° / 2)
/// ```
///
/// где:
/// - **C_rep_figma = 0.1978** — среднеарифметическое Oklab-хромы четырёх
///   якорей, взятых из Figma CONTENTS (коллекция «🔵 4.1 Primitives»,
///   Light-mode, обход переменных через figma-console, 2026-06-30):
///
///   | сентимент | hex Figma  | Oklab C |
///   |-----------|-----------|---------|
///   | Danger    | `#FF3B30` | 0.2321  |
///   | Warning   | `#FFA100` | 0.1717  |
///   | Success   | `#34C759` | 0.1944  |
///   | Info      | `#3E87FF` | 0.1931  |
///
///   `C_rep_figma = (0.2321 + 0.1717 + 0.1944 + 0.1931) / 4 = 0.1978`
///
/// - **20°** — нижний предел категориального восприятия оттенка.
///   ИЗ СТАТЬИ (Witzel & Gegenfurtner 2013 «Categorical sensitivity to color
///   differences», Journal of Vision 13(7):1, DOI 10.1167/13.7.1): категориальный
///   порог различения оттенка ~20° измерен в ИХ пространстве (hue-угол DKL, НЕ Oklab —
///   Oklab появился в 2020). ПРИНЯТО ДВИЖКОМ (приближение): этот порог перенесён в
///   Oklab-hue как ≈ 18–22° при типичной насыщенности; слой конверсии между
///   пространствами не из статьи, а принятое приближение движка.
///   Значение 20° — нижний предел этого диапазона (консервативный выбор
///   для разделения семантических категорий).
///
/// Итог: `2 × 0.1978 × sin(10°) ≈ 0.068_703_9`
// Выведено: 2 × C_rep_figma × sin(20°/2); C_rep_figma из Figma CONTENTS 2026-06-30;
// 20° — категориальный порог по Witzel & Gegenfurtner (2013) Journal of Vision 13(7):1 (DOI 10.1167/13.7.1).
// #[allow(dead_code)]: после удаления `s_min_deg` (Волна 1) единственные потребители —
// тесты (`s_perc_min_frozen`, derivation-identity), поэтому в lib-сборке const «мёртв».
#[allow(dead_code)]
const S_PERC_MIN: f64 = 0.068_703_9;

/// Sentiment categories. Each maps to a prototype hue expressed in
/// **Oklab hue degrees** (NOT HSB/HSL/sRGB hue). The resolved hue produced by
/// [`SentimentCurve`] is likewise an Oklab hue.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg(test)]
pub enum Sentiment {
    Danger,
    Warning,
    Success,
    Info,
}

#[cfg(test)]
impl Sentiment {
    /// Ideal hue for this sentiment — the **Oklab hue of its anchor colour**, in
    /// degrees (NOT HSB/HSL).
    ///
    /// The prototype is derived from a culturally-recognised anchor colour's
    /// actual Oklab hue ([`anchor_hex`](Self::anchor_hex)), not a hand-typed
    /// degree: the original hard-coded peaks were inconsistent with the anchors
    /// (Danger `18°` vs the true `28.7°`, Info `240°` vs `257°` — a hue-model
    /// mix-up that pulled Danger toward pink), while Oklab hue differs from HSB by
    /// 12–46° across the wheel, so a typed number is fragile. Deriving it removes
    /// the confusion at the source (the #65 fix, kept).
    fn prototype_hue(self) -> f64 {
        oklab_hue_of(self.anchor_hex())
    }

    /// Categorical hue floor (Oklab degrees) below which the sentiment loses its
    /// meaning — Warning must never slide into the red region it would otherwise
    /// share with Danger. Applied as a hard legality constraint, never a soft
    /// preference. This is the guarantee #65 dropped (and #66 inherited), whose
    /// loss let Warning resolve ~3.9° from Danger; restored here.
    fn hue_floor(self) -> Option<f64> {
        match self {
            Sentiment::Warning => Some(WARNING_HUE_FLOOR_DEG),
            _ => None,
        }
    }

    /// Семейство палитры акцентов, чей якорь несёт этот сентимент. Сентимент —
    /// это *семантическая роль* цвета (Danger, Warning, …), а его прототипный
    /// оттенок — это фиксированное семейство палитры ([`Accent`]). Отображение
    /// заземлено в Figma (`Labels/Danger/Primary` → `Accent/Red` и т.д.,
    /// коллекция «🔵 4.1 Primitives», Light-mode):
    ///
    /// | сентимент | семейство       | Figma-переменная  |
    /// |-----------|-----------------|-------------------|
    /// | Danger    | [`Accent::Red`]    | `Accent/Red`    |
    /// | Warning   | [`Accent::Orange`] | `Accent/Orange` |
    /// | Success   | [`Accent::Green`]  | `Accent/Green`  |
    /// | Info      | [`Accent::Blue`]   | `Accent/Blue`   |
    pub fn accent(self) -> Accent {
        match self {
            Sentiment::Danger => Accent::Red,
            Sentiment::Warning => Accent::Orange,
            Sentiment::Success => Accent::Green,
            Sentiment::Info => Accent::Blue,
        }
    }

    /// Якорный цвет сентимента, чей **Oklab-оттенок** используется как прототип.
    ///
    /// SSOT якорного hex — палитра акцентов ([`Accent::anchor_hex`]); сентимент
    /// лишь ссылается на своё семейство ([`accent`](Self::accent)), а не хранит
    /// собственную копию значения. Это устраняет дублирование hex между модулями
    /// (задача «акценты как данные, не 10 копий кода»): при изменении якоря в
    /// Figma правится ОДНА строка в `accent.rs`.
    ///
    /// Только Oklab-оттенок используется как прототип; хрома и светлота якоря
    /// не применяются — рампа строится из общей perceived-lightness лестницы на
    /// фиксированной доле граничной хромы гамута (см. [`SentimentCurve::at`]).
    fn anchor_hex(self) -> &'static str {
        self.accent().anchor_hex()
    }

    /// All four sentiment categories — the property-sweep surface for the tests.
    /// Currently consumed only by tests, so it is test-gated until the
    /// brand/sentiment table wiring (issue #59) consumes it.
    #[cfg(test)]
    pub(crate) const ALL: [Sentiment; 4] = [
        Sentiment::Danger,
        Sentiment::Warning,
        Sentiment::Success,
        Sentiment::Info,
    ];
}

// `DEFAULT_HARDNESS` (p-норма смещения ОТ бренда, модель Sticky Potential Well)
// УДАЛЁН Волной 1: закон категориальных зон убрал brand-displacement целиком —
// сентимент отдыхает на фокусе своей категории, жёсткости прижатия больше нет.
// Реестровая строка #20 удалена синхронно (docs/empirical-inventory.md): держать
// в реестре трекаемую константу удалённого механизма = ложь о живости кода.

/// Нижняя граница легальности оттенка Warning (Oklab hue, градусы) —
/// ЗАДОКУМЕНТИРОВАННОЕ ИСКЛЮЧЕНИЕ, СОХРАНЁННОЕ Волной 1: пол, ниже которого
/// решённый Warning-оттенок не пускается (`is_legal_hue`), чтобы Warning не сползал
/// в красную (danger) зону. Это НЕ порог классификации red↔orange и НЕ середина
/// между канониками — это дуговая граница легальности категории.
///
/// Под законом категориальных зон (Волна 1) Warning ОТДЫХАЕТ на своём прототипе
/// (orange 68.61°), который ВЫШЕ пола, поэтому для labui-якоря пол — «спящий»
/// предохранитель: не кусается. Он срабатывает лишь если прототип оранжевого
/// якоря клиента окажется НИЖЕ 45° — тогда оттенок клампится вверх к 45°, оставаясь
/// в янтаре, а не соскальзывая в красный. Прежний brand-displacement / C¹-резолвер /
/// flip-ветка, вокруг которых зазор ≈0.527° калибровался, УБРАНЫ Волной 1; поэтому
/// пол — теперь чисто категориальный страж, а не шов непрерывности.
///
/// Почему НЕ точная граница red↔orange: такой чёткой границы не существует
/// (Berlin & Kay 1969; Kay & McDaniel 1978 — категориальные границы размыты),
/// поэтому это консервативный категориальный пол, а не порог категории.
///
/// Значение 45.0 — DECLARED-CALIBRATION (реестр). Единственный дом значения по
/// построению: фикстура labui (config/fixture.rs) ссылается на эту константу
/// напрямую, второго литерала не существует.
#[cfg(test)]
// SSOT-TRACKED — DECLARED-CALIBRATION (design-choice): консервативный категориальный пол между danger 28.66° и warning 68.61°, дремлющий для labui (прототип orange > пола). Прежняя «привязка выведена (натур. минимум = prototype − s_min = 45.528°)» относилась к brand-displacement, УБРАННОМУ Волной 1 — вывод снят.
pub(crate) const WARNING_HUE_FLOOR_DEG: f64 = 45.0;

/// Fraction of the in-gamut maximum chroma every sentiment colour carries at its
/// perceived-lightness-matched point — the single "strength" knob. `< 1` so a
/// sentiment sits just inside its gamut wall rather than on it (the edge can
/// read neon). Applied identically to every hue: there is no per-hue cap. See
/// [`SentimentCurve::hex_at`].
///
/// Терминал **(e) DESIGN-CHOICE** — генуинная свободная ручка «силы». Легальный
/// диапазон конфига **(0, 1]** (валидатор `CHROMA_FRACTION` в `config.rs`;
/// `>1` = за стеной гамута). Sensitivity (Волна 2, лок
/// `chroma_fraction_sensitivity_is_bounded`): свип [0.70, 1.0] на реальных
/// сентимент-якорях (danger/warning/success/info) даёт max ΔE_ok ≈ **0.0421**
/// (>1 JND) — НЕПРЕРЫВНЫЙ материальный дрейф (fraction прямо масштабирует хрому
/// `f · max_chroma`), значит честный (e), не (c). Реестровый дефолт `0.88` — для
/// клиентов без якорной калибровки; **labui ставит `1.0`** (чистая стена гамута:
/// его якорь danger `#FF3B30` сидит ВЫШЕ `0.88·C_max`, см. S-01). Протокол
/// калибровки: подобрать fraction так, чтобы самый насыщенный якорь клиента сел
/// чуть внутри стены гамута (не «неон», но и не тускло) — измерение по палитре
/// клиента, а не эксперимент с наблюдателями.
// SSOT-TRACKED — gamut-fraction chroma strength knob, терминал (e) design-choice (labui=1.0; max ΔE_ok 0.0421 по [0.70,1.0]), см. docs/empirical-inventory.md.
const CHROMA_FRACTION: f64 = 0.88;

#[derive(Debug, Clone)]
pub struct SentimentCurve {
    pub resolved_hue: f64,
    pub was_displaced: bool,
    pub displacement: f64,
    /// The neutral curve this sentiment rides — its lightness ladder and viewing
    /// conditions drive the shared perceived-lightness ramp.
    neutral: NeutralCurve,
}

impl SentimentCurve {
    /// Разрешить кривую сентимента на его прототипном оттенке под законом
    /// категориальных зон (Волна 1).
    ///
    /// `prototype_hue`, `brand_hue` и [`resolved_hue`](Self::resolved_hue) — все
    /// **Oklab-оттенки в градусах** (НЕ HSB/HSL/sRGB). Сентимент ОТДЫХАЕТ на своём
    /// прототипе (фокусе категории — Berlin & Kay 1969; Kay & McDaniel 1978,
    /// нечёткие категориальные зоны, DOI 10.1353/lan.1978.0035); брендовый оттенок
    /// его больше НЕ смещает — совпадение сентимента с брендом внутри категории
    /// легитимно, не ошибка. Увести оттенок с прототипа могут ТОЛЬКО
    /// задокументированные исключения: пол оттенка (`hue_floor`, держит Warning
    /// вне красной зоны Danger) и попарные зоны соседей (различимость сентиментов
    /// между собой). Здесь зоны пусты (`&[]`) — витринная кривая разводит лишь полом.
    ///
    /// `brand_hue` и `chroma_hex` СОХРАНЕНЫ в сигнатуре как контракт границы и
    /// валидируются (конечность / парсинг hex), но под новым законом оттенок от них
    /// НЕ зависит (brand-independent). Инвариант
    /// `prototype_hex_chroma_never_leaks_into_the_ramp` пинит, что `chroma_hex`
    /// не течёт в рампу.
    ///
    /// # Errors
    ///
    /// `Err`, если `brand_hue` не конечен, `chroma_hex` не парсится как sRGB, либо
    /// резолвер оттенка не нашёл легального кандидата (пустая дуга под полом). Цвет
    /// рампы — [`LcsColor`], строится лениво в [`at`](Self::at).
    pub fn new(
        brand_hue: f64,
        prototype_hue: f64,
        chroma_hex: &str,
        hue_floor: Option<f64>,
        neutral: &NeutralCurve,
    ) -> Result<Self, String> {
        // Контракт границы: brand/chroma больше не двигают оттенок, но остаются
        // валидируемым входом — мусор не должен тихо пройти. `chroma_hex` парсится
        // лишь для проверки и в рампу не течёт (пинит `..._never_leaks_into_the_ramp`).
        if !brand_hue.is_finite() {
            return Err(format!("brand_hue is not finite: {brand_hue}"));
        }
        srgb_from_hex(chroma_hex)?;

        // Правило закона зон: сентимент покоится на прототипе; пол — единственное
        // исключение, применяемое здесь (зоны соседей пусты для витринной кривой).
        let resolved_hue = resolve_sentiment_hue_among(hue_floor, prototype_hue, &[])?;

        // «Смещение» теперь — отклонение от прототипа, вызванное полом/соседями
        // (0 для покоящегося сентимента): флаг-репортёр, не ветка.
        let displacement = angular_distance(resolved_hue, prototype_hue);
        let was_displaced = displacement > 1e-6;

        // Рампа строится по запросу из нейтральной кривой и решённого оттенка
        // (закон хромы/светлоты в `at`/`hex_at` НЕ изменён — сменился лишь вход-оттенок).
        Ok(Self {
            resolved_hue,
            was_displaced,
            displacement,
            neutral: neutral.clone(),
        })
    }

    /// The sentiment colour at ramp position `t ∈ [0, 1]`. The four sentiments
    /// share one **perceived-lightness** (`j_hk`) ladder — the neutral grey's
    /// H-K lightness — and each hue is placed at that perceived lightness at a
    /// fixed fraction of the in-gamut maximum chroma. Equal `j_hk` means equal
    /// perceived brightness *and* equal contrast at every step (none out-shouts);
    /// max chroma means none is dull. One rule for every hue, no per-hue cap —
    /// the green "cap" of the old model falls out of the maths (a saturated green
    /// must sit at a lower base lightness to land on the same `j_hk`).
    pub fn at(&self, t: f64) -> LcsColor {
        let vc = self.neutral.vc();
        let hex = self.hex_at(t);
        LcsColor::from_hex_with_vc(&hex, vc).unwrap_or_else(|_| self.neutral.at(t))
    }

    /// `n` hex-строк рампы напрямую через `hex_at` — НЕ через
    /// трейтовый `ColorCurve::sample_hex`, который прогнал бы каждый цвет
    /// лишним hex→[`LcsColor`]→hex кругом (`at` уже строится из hex) и мог бы
    /// дрейфнуть на ±1 LSB. Прямой путь держит вывод байт-идентичным
    /// (`r3_byte_identity_tests`), поэтому этот inherent-метод сознательно
    /// оставлен и затеняет трейтовый дефолт только для sentiment.
    pub fn sample_hex(&self, n: usize) -> Vec<String> {
        if n == 0 {
            return Vec::new();
        }
        if n == 1 {
            return vec![self.hex_at(0.5)];
        }
        (0..n)
            .map(|i| self.hex_at(i as f64 / (n - 1) as f64))
            .collect()
    }

    /// The hex at ramp position `t` — the colour [`at`](Self::at) builds, without
    /// the round-trip through [`LcsColor`].
    fn hex_at(&self, t: f64) -> String {
        let vc = self.neutral.vc();
        let h = self.resolved_hue;
        // The shared ladder is the neutral grey's *perceived* (H-K) lightness at
        // `t` — a grey has no chroma, so its `j_hk` is just its CAM16 lightness.
        let l_grey = jp_to_oklab_l(self.neutral.at(t).jp, vc);
        let target_jhk = jhk_at(l_grey, 0.0, h, vc);
        // Place this hue at that perceived lightness, at a fixed fraction of the
        // gamut-edge chroma. Identical rule for every hue; a saturated hue lands
        // at a lower base lightness (its H-K boost is what makes `j_hk` match).
        let l = l_for_jhk(target_jhk, h, vc);
        let c = CHROMA_FRACTION * max_chroma(l, h);
        oklab_lc_to_hex(l, c, h)
    }
}

impl crate::curve::ColorCurve for SentimentCurve {
    fn at(&self, t: f64) -> LcsColor {
        self.at(t)
    }

    fn vc(&self) -> &ViewingConditions {
        self.neutral.vc()
    }
}

/// Test-only convenience over the built-in [`Sentiment`] witness: maps a category
/// to its prototype hue (from its anchor) and `hue_floor`, and calls the enum-free
/// public API. Keeps the byte-identity oracles terse without leaking the showcase
/// enum into the production signature (ADR-0001 PR-c).
///
/// The category's prototype HUE comes from its anchor
/// ([`prototype_hue`](Sentiment::prototype_hue)); `chroma_hex` is validated but,
/// under the categorical-zone law, no longer influences the resolved hue — the
/// invariant `prototype_hex_chroma_never_leaks_into_the_ramp` pins that.
#[cfg(test)]
impl SentimentCurve {
    pub(crate) fn from_sentiment(
        sentiment: Sentiment,
        brand_hue: f64,
        chroma_hex: &str,
        neutral: &NeutralCurve,
    ) -> Result<Self, String> {
        Self::new(
            brand_hue,
            sentiment.prototype_hue(),
            chroma_hex,
            sentiment.hue_floor(),
            neutral,
        )
    }
}

/// Разрешить оттенок сентимента под законом категориальных зон (Волна 1).
///
/// ПРАВИЛО: сентимент ОТДЫХАЕТ на своём прототипе (фокусе категории). Брендовый
/// оттенок в разведении больше НЕ участвует — литература (Berlin & Kay 1969;
/// Kay & McDaniel 1978, нечёткие категориальные зоны, DOI 10.1353/lan.1978.0035;
/// Regier, Kay & Cook 2005, PNAS 102(23):8386 — фокусы стабильны, границы размыты)
/// не даёт основания форсировать сепарацию от бренда: только держать оттенок ВНУТРИ
/// своей категории. Поэтому совпадение сентимента с брендовым оттенком внутри его
/// зоны легитимно, а не ошибка (решение владельца).
///
/// ИСКЛЮЧЕНИЯ — единственное, что уводит оттенок с прототипа: пол оттенка
/// (`hue_floor` — держит Warning вне красной зоны Danger) и попарные зоны соседей
/// (`zones` — различимость сентиментов между собой). Оба СОХРАНЕНЫ Волной 1.
/// Если прототип уже легален под полом и зонами — возвращается он сам (ПОКОЙ, само
/// правило); иначе сканируется ближайший легальный оттенок ([`legalize_hue_among`],
/// задокументированное исключение: сдвиг полом/соседом).
///
/// `prototype` и результат — **Oklab-оттенки в градусах**.
///
/// # Errors
///
/// `Err`, если `prototype` не конечен, `hue_floor` вне домена `[0, 360)`, либо
/// легальная дуга под полом и зонами геометрически пуста — никогда тихий мусор.
pub(crate) fn resolve_sentiment_hue_among(
    hue_floor: Option<f64>,
    prototype: f64,
    zones: &[NeighborZone],
) -> Result<f64, String> {
    // Домен входа: NaN/inf в normalize_hue/скане не завершились бы осмысленно —
    // честный Err вместо тихого NaN-оттенка вниз по физике.
    if !prototype.is_finite() {
        return Err(format!("prototype вне домена (конечный): {prototype}"));
    }
    check_hue_floor_domain(hue_floor)?;

    // ПОКОЙ (правило): прототип легален под полом и зонами — сентимент отдыхает
    // на фокусе своей категории, без всякого участия бренда.
    if is_legal_hue_among(prototype, hue_floor, zones) {
        return Ok(normalize_hue(prototype));
    }
    // ИСКЛЮЧЕНИЕ: пол/сосед блокируют прототип — ближайший легальный оттенок.
    legalize_hue_among(prototype, hue_floor, zones)
}

/// Занятая соседним сентиментом зона оттенка: центр (решённый оттенок соседа)
/// и минимальный угловой отступ, выведенный инверсией хорды `s_perc_min` при
/// СРЕДНЕЙ хроме пары (тот же закон, что `s_min_deg_from_chord` для конфига —
/// попарная различимость сентиментов, аудит 2026-07-03, находка S-02). СОХРАНЕНО
/// Волной 1: brand-разведение убрано, но зоны соседей — задокументированное
/// исключение, держащее сентименты различимыми МЕЖДУ СОБОЙ.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NeighborZone {
    pub(crate) hue_deg: f64,
    pub(crate) min_sep_deg: f64,
}

/// Прижать оттенок к ближайшему легальному под полом И попарными зонами соседей.
///
/// Легальный кандидат возвращается как есть; иначе скан наружу мелким шагом
/// возвращает ближайший легальный оттенок. Шаг 0.05° — ниже углового разрешения
/// 8-битного квантования hex. Если легального оттенка на всём круге нет (пол и
/// зоны не оставляют места) — `Err`, а не тихое нарушение инварианта.
fn legalize_hue_among(
    candidate: f64,
    floor: Option<f64>,
    zones: &[NeighborZone],
) -> Result<f64, String> {
    if is_legal_hue_among(candidate, floor, zones) {
        return Ok(normalize_hue(candidate));
    }

    let mut step = 0.05_f64;
    while step <= 360.0 {
        for cand in [
            normalize_hue(candidate + step),
            normalize_hue(candidate - step),
        ] {
            if is_legal_hue_among(cand, floor, zones) {
                return Ok(cand);
            }
        }
        step += 0.05;
    }

    Err(format!(
        "no legal hue exists for floor={floor:?}, zones={zones:?}: the floor and the \
         neighbour zones leave no room on the hue circle"
    ))
}

/// Оттенок легален, если он на уровне пола или выше (где пол задан). Разведение от
/// бренда Волной 1 УБРАНО — единственное исключение здесь категориальный пол.
fn is_legal_hue(h: f64, floor: Option<f64>) -> bool {
    if let Some(f) = floor
        && normalize_hue(h) < f
    {
        return false;
    }
    true
}

/// [`is_legal_hue`] + попарные зоны соседей (различимость сентиментов между собой).
fn is_legal_hue_among(h: f64, floor: Option<f64>, zones: &[NeighborZone]) -> bool {
    is_legal_hue(h, floor)
        && zones
            .iter()
            .all(|z| angular_distance(h, z.hue_deg) >= z.min_sep_deg - 1e-9)
}

/// Signed shortest delta from `from` to `h` in (-180, 180].
/// Домен-гард пола оттенка: единая проверка для обоих публичных входов
/// (валидатор конфига и солвер) — дублированный блок разошёлся бы тихо,
/// ровно как разошлись бы независимые литералы одного домена.
fn check_hue_floor_domain(hue_floor: Option<f64>) -> Result<(), String> {
    if let Some(floor) = hue_floor
        && !(floor.is_finite()
            && (HUE_DOMAIN_MIN_INCLUSIVE..HUE_DOMAIN_MAX_EXCLUSIVE).contains(&floor))
    {
        return Err(format!("hue_floor вне домена [0, 360): {floor}"));
    }
    Ok(())
}

fn normalize_hue(h: f64) -> f64 {
    h.rem_euclid(360.0)
}

pub(crate) fn angular_distance(a: f64, b: f64) -> f64 {
    let diff = (a - b).rem_euclid(360.0);
    if diff > 180.0 { 360.0 - diff } else { diff }
}

/// Категориальный порог оттенка `S_PERC_MIN` (длина хорды Oklab a/b),
/// пересчитанный из хром сентимент-якорей конфига по закону
/// `2·C_rep·sin(20°/2)`, где `C_rep` — среднее хром.
///
/// `20°` — нижний предел категориального восприятия (Witzel & Gegenfurtner 2013,
/// Journal of Vision 13(7):1, DOI 10.1167/13.7.1). При labui-якорях (хромы Red/Orange/Green/Blue) результат
/// совпадает с замороженной константой `S_PERC_MIN` (`0.068_703_9`,
/// деривационная идентичность — тестом, допуск 1e-4): формула остаётся законом
/// при произвольных якорях клиента, а сегодняшнее значение — её частный случай.
///
/// Пустой срез хром даёт `0.0` (нет сентиментов — нет порога разделения).
pub fn s_perc_min_from_chromas(chromas: &[f64]) -> f64 {
    if chromas.is_empty() {
        return 0.0;
    }
    let c_rep = chromas.iter().sum::<f64>() / chromas.len() as f64;
    // Хорда длины 2·C·sin(Δh/2) при Δh = 20° — тот же категориальный порог
    // (Witzel & Gegenfurtner 2013), что в деривации [`S_PERC_MIN`]; инлайн
    // (не именованная const), т.к. это derivation-identity вход, не новый
    // POLICY-литерал — provenance держит doc [`S_PERC_MIN`].
    2.0 * c_rep * (20.0_f64.to_radians() / 2.0).sin()
}

/// Замороженное значение `S_PERC_MIN` (для теста деривационной идентичности).
/// Возвращается функцией (не `const`), чтобы не заводить второй POLICY-литерал в
/// аудите реестра — это тот же derivation-identity, что [`S_PERC_MIN`].
/// `#[cfg(test)]`: единственный потребитель — тест `config::tests` (не прод-API).
#[cfg(test)]
pub(crate) fn s_perc_min_frozen() -> f64 {
    S_PERC_MIN
}

/// Технический порог числовой определённости оттенка: ниже него atan2 в
/// [`oklab_hue_of`] математически не определён (не перцептивная величина —
/// защита от произвольного 0°, не политика). Дом константы — здесь, рядом с
/// законом «нет носителя оттенка → нет разведения»; конфиг-гарды
/// (`crate::config`) ссылаются сюда же.
///
/// Провенанс ε: минимум ненулевой Oklab-хромы 8-битного цвета ≈ 1.1e-3
/// (#FEFFFF; #808081 ≈ 1.5e-3), f64-шум конвейера sRGB→Oklab ≲ 1e-12;
/// 1e-7 лежит между ними с запасом ≥4 порядка в обе стороны — не может
/// переклассифицировать ни один представимый цвет.
// SSOT-TRACKED — арифметика представимости (деривация закрыта, внешнего стандарта не существует): границы в docs/empirical-inventory.md.
pub(crate) const ACHROMATIC_CHROMA_EPS: f64 = 1e-7;

/// Канонический домен угла оттенка (градусы): `[0, 360)` — угол по модулю
/// 360°, где 360° ≡ 0°. Единые границы для `hue_floor`/`hue_override`
/// валидатора конфига и гардов солвера: два независимых литерала одного
/// домена в цветовом коде — класс тихого расхождения пределов.
// SSOT-TRACKED — определение домена: угол по модулю 360°, не перцептивная политика (реестр, строка 39).
pub(crate) const HUE_DOMAIN_MIN_INCLUSIVE: f64 = 0.0;
/// Верхняя граница канонического домена оттенка (исключительно; 360° ≡ 0°).
// SSOT-TRACKED — определение домена: полный оборот окружности, не перцептивная политика (реестр, строка 40).
pub(crate) const HUE_DOMAIN_MAX_EXCLUSIVE: f64 = 360.0;

/// Config-facing сентимент-солид: якорь семейства, чей оттенок разрешён законом
/// категориальных зон (Волна 1), при СОХРАНЁННЫХ светлоте и хроме якоря (под
/// анти-неоновым потолком `chroma_fraction`).
///
/// Оттенок семейства ОТДЫХАЕТ на своём прототипе (фокусе категории) — бренд его
/// больше НЕ смещает (см. `resolve_sentiment_hue_among`). Для покоящегося
/// сентимента солид воспроизводит СЫРОЙ якорь семейства (деривационная
/// идентичность). Оттенок уводит с прототипа только пол (`hue_floor`).
///
/// # Errors
///
/// `Err`, если якорь невалиден, `hue_floor` вне домена `[0, 360)`,
/// `chroma_fraction` вне `(0, 1]`, либо легальный оттенок под полом геометрически
/// пуст (см. `resolve_sentiment_hue_among`).
pub fn resolve_config_sentiment_solid(
    family_anchor_hex: &str,
    chroma_fraction: f64,
    hue_floor: Option<f64>,
) -> Result<String, String> {
    resolve_config_sentiment_solid_among(family_anchor_hex, chroma_fraction, hue_floor, &[])
}

/// [`resolve_config_sentiment_solid`] с попарными зонами соседей (различимость
/// сентиментов между собой, находка S-02 аудита 2026-07-03). Пустой список зон —
/// чистое правило «покой на прототипе, сдвиг только полом».
pub(crate) fn resolve_config_sentiment_solid_among(
    family_anchor_hex: &str,
    chroma_fraction: f64,
    hue_floor: Option<f64>,
    zones: &[NeighborZone],
) -> Result<String, String> {
    // chroma_fraction — АНТИ-НЕОНОВЫЙ ПОТОЛОК:
    //   c_тинта = min(c_якоря, chroma_fraction · C_max(L, h_решённый)).
    // Якорь внутри потолка воспроизводится байт-в-байт (доля 1.0 — чистая стена
    // гамута); потолок кусается лишь на неоново-насыщенном якоре ИЛИ когда
    // сдвинутый полом оттенок имеет более узкий гамут. Усечение идёт по оси хромы
    // (оттенок сохранён — прежний канальный клип sRGB искажал оттенок).
    //
    // Публичная граница: прямой вызов мог бы протащить недоменную ручку прямо в
    // oklab_lc_to_hex тихим неверным hex.
    check_hue_floor_domain(hue_floor)?;
    // Тот же домен, что у конфиг-валидатора ((0, 1]): не пускаем NaN/0/1.5 в потолок хромы.
    if !(chroma_fraction.is_finite() && chroma_fraction > 0.0 && chroma_fraction <= 1.0) {
        return Err(format!(
            "chroma_fraction вне домена (0 < f ≤ 1): {chroma_fraction}"
        ));
    }
    let anchor_lab = srgb_linear_to_oklab(srgb_from_hex(family_anchor_hex)?);
    let l_anchor = anchor_lab[0];
    let c_anchor = (anchor_lab[1].powi(2) + anchor_lab[2].powi(2)).sqrt();
    // Ахроматичный якорь не несёт оттенка (prototype = atan2(0,0) — числовой
    // произвол): нет носителя оттенка → нет разведения, солид = сырой якорь
    // (байт-в-байт, нормализованный через encoded-roundtrip).
    if c_anchor < ACHROMATIC_CHROMA_EPS {
        return Ok(hex_from_srgb_encoded(srgb_encoded_from_hex(
            family_anchor_hex,
        )?));
    }
    // Правило: прототип = оттенок якоря; сентимент покоится на нём, пол/зоны —
    // единственные исключения. Brand-сепарация (и её сатурационная ветвь) убраны.
    let prototype = oklab_hue_of(family_anchor_hex);
    let resolved_hue = resolve_sentiment_hue_among(hue_floor, prototype, zones)?;
    // Солид на исходной светлоте якоря и его хроме под анти-неоновым потолком,
    // на решённом оттенке.
    let c = capped_chroma(c_anchor, chroma_fraction, l_anchor, resolved_hue);
    Ok(oklab_lc_to_hex(l_anchor, c, resolved_hue))
}

/// Анти-неоновый потолок хромы тинта: `min(c_якоря, f · C_max(L, h))` — хрома
/// якоря сохраняется, пока не упирается в долю гамутного максимума на
/// РЕШЁННОМ оттенке; усечение по оси хромы держит оттенок (в отличие от
/// канального клипа sRGB).
fn capped_chroma(c_anchor: f64, fraction: f64, l_ok: f64, h_deg: f64) -> f64 {
    c_anchor.min(fraction * crate::scale::max_chroma(l_ok, h_deg))
}

/// Перевести целевую хорду разделения `chord` в угол оттенка (градусы) при
/// хроме `zone_chroma` — инверсия `chord = 2·C·sin(Δh/2)` для произвольной хорды
/// (конфиг-`S_PERC_MIN`); питает попарные зоны соседей (`NeighborZone`).
///
/// При `chord ≥ 2·zone_chroma` порог недостижим НИ ОДНИМ углом (хорда
/// окружности радиуса C ограничена диаметром 2C): возвращается ровно 180° —
/// маркер сатурации. Смысл: пара так приглушена, что категориальная хорда
/// недостижима при ЛЮБОМ разведении оттенка — пара перцептивно НЕРАЗДЕЛИМА.
/// Единственный потребитель (конфиг-фаза-2 попарных зон,
/// `sentiment_solid_for_mode`) обязан ПРОПУСТИТЬ такую зону: угловой отступ
/// 180° требует точного антипода (мера нуль), скан `legalize_hue` (шаг 0.05°)
/// его не находит — вышел бы ложный «пустая дуга». Раз пара неразделима,
/// отступ бессмыслен, и зона не накладывает ограничения (приглушённые
/// сентименты и так у серой оси — не флип-риск). Случай реален: приглушённый
/// якорь тёмной темы при хромных соседях.
///
/// # Errors
///
/// `Err` на неконечных/отрицательных входах и `zone_chroma ≤ 0` — вызывающий
/// обязан отсечь ахроматичную зону гардом [`ACHROMATIC_CHROMA_EPS`] до
/// инверсии хорды (asin от NaN-отношения дал бы NaN-градусы дальше по физике).
pub(crate) fn s_min_deg_from_chord(chord: f64, zone_chroma: f64) -> Result<f64, String> {
    if !(chord.is_finite() && zone_chroma.is_finite() && chord >= 0.0 && zone_chroma > 0.0) {
        return Err(format!(
            "инверсия хорды вне домена: chord={chord}, zone_chroma={zone_chroma}"
        ));
    }
    let ratio = chord / (2.0 * zone_chroma);
    if ratio >= 1.0 {
        return Ok(180.0);
    }
    Ok(2.0 * ratio.asin().to_degrees())
}

/// The in-gamut sRGB hex at Oklab `(L, C, h)`, channels clamped to `[0, 1]`.
fn oklab_lc_to_hex(l_ok: f64, c: f64, h_ok: f64) -> String {
    let a = c * h_ok.to_radians().cos();
    let b = c * h_ok.to_radians().sin();
    let rgb = oklab_to_srgb_linear([l_ok, a, b]);
    hex_from_srgb([
        rgb[0].clamp(0.0, 1.0),
        rgb[1].clamp(0.0, 1.0),
        rgb[2].clamp(0.0, 1.0),
    ])
}

/// The Helmholtz–Kohlrausch perceived lightness (`j_hk`) of the Oklab colour
/// `(l_ok, c, h_ok)` under `vc`. This is the H-K-corrected lightness the LCS
/// contrast pipeline already uses (`lpc::j_hk_from_xyz`): a saturated colour's
/// perceived lightness is boosted above its measured luminance. A grey (`c == 0`)
/// has no boost, so its `j_hk` is just its CAM16 lightness.
fn jhk_at(l_ok: f64, c: f64, h_ok: f64, vc: &ViewingConditions) -> f64 {
    let a = c * h_ok.to_radians().cos();
    let b = c * h_ok.to_radians().sin();
    let rgb = oklab_to_srgb_linear([l_ok, a, b]);
    let rgb = [
        rgb[0].clamp(0.0, 1.0),
        rgb[1].clamp(0.0, 1.0),
        rgb[2].clamp(0.0, 1.0),
    ];
    crate::lpc::j_hk_from_xyz(srgb_to_xyz(rgb), vc)
}

/// The Oklab lightness whose **gamut-edge** colour at hue `h_ok` has perceived
/// lightness `target` (`j_hk`). Bisection: at the gamut edge `j_hk` rises with
/// `l_ok` (a lighter base is perceived lighter even after the saturation boost),
/// so the root is unique. This is what places a saturated hue at a *lower* base
/// lightness so its H-K boost lands it on the shared perceived-lightness ladder.
fn l_for_jhk(target: f64, h_ok: f64, vc: &ViewingConditions) -> f64 {
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        if jhk_at(mid, max_chroma(mid, h_ok), h_ok, vc) > target {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    0.5 * (lo + hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::ColorCurve;

    fn neutral() -> NeutralCurve {
        NeutralCurve::new("#FFFFFF", "#787880", "#101012").unwrap()
    }

    #[test]
    fn prototype_is_the_anchor_oklab_hue() {
        // Прототип читается с Oklab-оттенка якорного цвета, а не вводится вручную —
        // это исключает дрейф между цветовыми моделями (баг, из-за которого
        // Danger уходил в розовый).
        for s in Sentiment::ALL {
            let want = oklab_hue_of(s.anchor_hex());
            assert!(
                (s.prototype_hue() - want).abs() < 1e-9,
                "{s:?}: prototype {} != anchor Oklab hue {want}",
                s.prototype_hue()
            );
        }
    }

    /// Якорные цвета заземлены в Figma CONTENTS (коллекция «🔵 4.1 Primitives»,
    /// Light-mode, обход узлов через figma-console, дата: 2026-06-30).
    ///
    /// Доказательство из Figma:
    ///   Labels/Danger/Primary  → Accent/Red   = #FF3B30  (Oklab h=28.66°)
    ///   Labels/Warning/Primary → Accent/Orange = #FFA100  (Oklab h=68.61°)
    ///   Labels/Success/Primary → Accent/Green  = #34C759  (Oklab h=147.44°)
    ///   Labels/Info/Primary    → Accent/Blue   = #3E87FF  (Oklab h=259.89°)
    ///
    /// Тест зафиксирует любое отклонение якорного Oklab-оттенка от Figma-примитивов.
    /// Используемый допуск < 0.001° — меньше разрешения 8-бит квантования.
    #[test]
    fn anchor_hues_match_figma_primitives_light_mode() {
        // Figma CONTENTS: коллекция «4.1 Primitives», Light-mode, обход переменных.
        // Значения верифицированы через figma-console figma_execute (2026-06-30).
        let expected: &[(&str, f64)] = &[
            // (figma_hex,        expected_oklab_hue_deg)
            ("#FF3B30", 28.6592),  // Accent/Red   → Danger
            ("#FFA100", 68.6070),  // Accent/Orange → Warning
            ("#34C759", 147.4439), // Accent/Green  → Success
            ("#3E87FF", 259.8918), // Accent/Blue   → Info
        ];
        let sentiments = [
            Sentiment::Danger,
            Sentiment::Warning,
            Sentiment::Success,
            Sentiment::Info,
        ];
        for ((figma_hex, want_hue), s) in expected.iter().zip(sentiments) {
            // anchor_hex() ДОЛЖЕН совпадать точно с Figma-примитивом
            assert_eq!(
                s.anchor_hex(),
                *figma_hex,
                "{s:?}: anchor_hex() дрейфанул от заземлённого Figma-примитива"
            );
            let actual = s.prototype_hue();
            // prototype_hue() ДОЛЖЕН совпадать с Oklab-оттенком Figma-примитива
            assert!(
                (actual - want_hue).abs() < 0.001,
                "{s:?}: prototype_hue() = {actual:.4}° != Figma-оттенок {want_hue}° \
                 (якорный hex Figma: {figma_hex})"
            );
        }
    }

    /// Пин контракта дедупликации, достижимый на уровне значений: (1) маппинг
    /// сентимент→семейство заземлён Figma (Labels/<Sentiment>/Primary →
    /// Accent/<Family>) и запинен явно; (2) якорь сентимента обязан быть равен
    /// якорю его семейства — две поверхности не могут разойтись. Появление
    /// локальной копии hex этот тест не видит в момент появления (значения
    /// равны), но ловит при ПЕРВОМ расхождении копий (правка одной таблицы без
    /// другой) — раньше расхождение было бы тихим.
    #[test]
    fn sentiment_delegates_anchor_to_its_accent_family() {
        use crate::accent::Accent;
        let mapping = [
            (Sentiment::Danger, Accent::Red),
            (Sentiment::Warning, Accent::Orange),
            (Sentiment::Success, Accent::Green),
            (Sentiment::Info, Accent::Blue),
        ];
        for (s, family) in mapping {
            assert_eq!(
                s.accent(),
                family,
                "{s:?}: маппинг сентимент→семейство разошёлся с Figma-заземлением"
            );
            assert_eq!(
                s.anchor_hex(),
                s.accent().anchor_hex(),
                "{s:?}: anchor_hex() не делегирует в палитру (появилась локальная копия)"
            );
        }
    }

    #[test]
    fn sample_hex_has_requested_length_and_valid_hex() {
        let n = neutral();
        let sc = SentimentCurve::from_sentiment(Sentiment::Danger, 33.5, "#FF2E2E", &n).unwrap();
        for k in [0usize, 1, 2, 10, 13] {
            let v = sc.sample_hex(k);
            assert_eq!(v.len(), k, "sample_hex({k}) length");
            for h in &v {
                assert!(srgb_from_hex(h).is_ok(), "invalid hex {h}");
            }
        }
    }

    /// The H-K perceived lightness of a rendered hex — the same `j_hk` the ramp
    /// matches across hues.
    fn jhk_hex(hex: &str, vc: &ViewingConditions) -> f64 {
        crate::lpc::j_hk_from_xyz(srgb_to_xyz(srgb_from_hex(hex).unwrap()), vc)
    }

    #[test]
    fn all_sentiments_share_one_perceived_lightness_ladder() {
        // The coherence invariant of the unified law: at every ramp step the four
        // sentiments sit at the SAME perceived (H-K) lightness — the neutral
        // grey's `j_hk` — so none out-shouts and all share one contrast level
        // ("одноуровневый по контрасту и светлоте"). The green warm-budget cap
        // used to approximate this by hand for one hue; now it holds for every
        // hue, by construction, for any brand. Swept across brands; the small
        // tolerance only absorbs 8-bit quantisation of the emitted hex.
        let n = neutral();
        let vc = n.vc();
        for brand in (0..360).step_by(13).map(|d| d as f64) {
            let curves: Vec<_> = Sentiment::ALL
                .into_iter()
                .map(|s| SentimentCurve::from_sentiment(s, brand, s.anchor_hex(), &n).unwrap())
                .collect();
            for i in 0..=10 {
                let t = i as f64 / 10.0;
                // The ladder target: the neutral grey's perceived lightness here.
                let target = jhk_at(jp_to_oklab_l(n.at(t).jp, vc), 0.0, 0.0, vc);
                for (s, curve) in Sentiment::ALL.into_iter().zip(&curves) {
                    let got = jhk_hex(&curve.sample_hex(11)[i], vc);
                    assert!(
                        (got - target).abs() < 1.6,
                        "{s:?} brand {brand} step {i}: j_hk {got:.2} off ladder {target:.2}"
                    );
                }
            }
        }
    }

    // УДАЛЕНЫ Волной 1 (премиса — brand-separation, которой закон больше нет):
    //   `resolved_hue_clears_the_brand_by_s_min` — инвариант «оттенок ≥ s_min от
    //     бренда» снят: бренд оттенок больше не отталкивает.
    //   `success_slides_to_teal_not_yellow_when_a_green_brand_encroaches` — премиса
    //     «бренд у зелёного толкает Success в тил» снята: Success отдыхает на
    //     прототипе, бренд его не двигает.

    #[test]
    fn ramp_lightness_is_monotone_dark() {
        let n = neutral();
        for s in Sentiment::ALL {
            let r = SentimentCurve::from_sentiment(s, 33.5, s.anchor_hex(), &n)
                .unwrap()
                .sample(13);
            for w in r.windows(2) {
                assert!(
                    w[1].jp <= w[0].jp + 1e-6,
                    "{s:?}: lightness not monotone ({} -> {})",
                    w[0].jp,
                    w[1].jp
                );
            }
        }
    }

    #[test]
    fn rejects_non_finite_brand_hue() {
        let n = neutral();
        assert!(
            SentimentCurve::from_sentiment(Sentiment::Danger, f64::NAN, "#FF2E2E", &n).is_err()
        );
    }

    #[test]
    fn every_hue_carries_the_chroma_fraction_so_nothing_is_dull() {
        // Nothing dull: at each mid step every sentiment — INCLUDING green, which
        // the old warm-budget cap muted to ~0.79 of its own ceiling — sits at
        // (near) `CHROMA_FRACTION` of the gamut-edge chroma for its rendered
        // lightness. The 0.80 floor (target 0.88) is loose enough to absorb 8-bit
        // quantisation while still proving green is no longer capped down. Swept
        // across brands; checked on the mid steps where the gamut has chroma to give.
        let n = neutral();
        for brand in (0..360).step_by(29).map(|d| d as f64) {
            for s in Sentiment::ALL {
                let curve = SentimentCurve::from_sentiment(s, brand, s.anchor_hex(), &n).unwrap();
                let h = curve.resolved_hue;
                for i in 3..=7 {
                    let hex = curve.sample_hex(11)[i].clone();
                    let lab = srgb_linear_to_oklab(srgb_from_hex(&hex).unwrap());
                    let l_r = lab[0];
                    let c_r = (lab[1].powi(2) + lab[2].powi(2)).sqrt();
                    let c_max = max_chroma(l_r, h);
                    assert!(
                        c_r >= 0.80 * c_max,
                        "{s:?} brand {brand} step {i}: chroma {c_r:.3} dull \
                         (< 0.80 of gamut max {c_max:.3})"
                    );
                }
            }
        }
    }

    #[test]
    fn prototype_hex_chroma_never_leaks_into_the_ramp() {
        // Честный API: под законом категориальных зон (Волна 1) `chroma_hex`
        // ВАЛИДИРУЕТСЯ, но в оттенок/рампу НЕ течёт — сентимент отдыхает на
        // прототипе независимо от хромы прототипного hex. Насыщенный и приглушённый
        // прототип одного оттенка обязаны дать БАЙТ-В-БАЙТ идентичную рампу.
        let n = neutral();
        let s = Sentiment::Danger;
        let brand = normalize_hue(s.prototype_hue() + 180.0); // бренд игнорируется законом
        let saturated = SentimentCurve::from_sentiment(s, brand, "#FF3B30", &n).unwrap();
        let muted = SentimentCurve::from_sentiment(s, brand, "#B36A65", &n).unwrap(); // тот же красный, хрома ~вдвое ниже
        for t in [0.2, 0.5, 0.8] {
            assert_eq!(
                saturated.hex_at(t),
                muted.hex_at(t),
                "хрома прототипа просочилась в рампу (t={t})"
            );
        }
    }

    #[test]
    fn warning_floor_enforced_full_circle() {
        // Восстановлена защита (#65 её убрала, #66 унаследовал уязвимость):
        // Warning никогда не должен опускаться ниже своего категориального
        // порога в красную зону, при ЛЮБОМ брендовом оттенке на круге.
        // prototype_hex = Figma Accent/Orange (#FFA100, 2026-06-30).
        let n = neutral();
        let mut brand = 0.0;
        while brand < 360.0 {
            let h = SentimentCurve::from_sentiment(Sentiment::Warning, brand, "#FFA100", &n)
                .unwrap()
                .resolved_hue;
            assert!(
                normalize_hue(h) >= 45.0 - 1e-6,
                "Warning resolved {h:.2}° is below the 45° floor at brand {brand}"
            );
            brand += 0.25;
        }
    }

    #[test]
    fn warning_floor_is_a_dormant_guard_below_the_orange_prototype() {
        // РЕФРЕЙМ Волны 1 (прежний вывод «натуральный минимум = prototype − s_min»
        // относился к УБРАННОМУ brand-displacement и снят). Под законом
        // категориальных зон Warning ОТДЫХАЕТ на своём прототипе (orange). Пол 45° —
        // задокументированное исключение, СОХРАНЁННОЕ, но для labui-якоря «спящее»:
        // прототип orange лежит ВЫШЕ пола, поэтому пол не кусается и Warning садится
        // на фокус, а не клампится. Страж пинит именно отношение floor < prototype:
        // если оранжевый якорь клиента опустится ниже пола, Warning клампанётся вверх
        // к 45° (останется в янтаре, не в красном), но для эталонного паспорта пол дремлет.
        let proto_hue = Sentiment::Warning.prototype_hue();
        assert!(
            proto_hue > WARNING_HUE_FLOOR_DEG,
            "прототип Warning {proto_hue:.3}° обязан лежать ВЫШЕ пола \
             {WARNING_HUE_FLOOR_DEG}° — иначе пол перестаёт быть спящим для labui-якоря"
        );
        // Нетавтологичный пин запаса (orange proto 68.607° − пол 45.0 = 23.607°):
        // ловит дрейф в обе стороны (сдвиг orange-прототипа или значения пола).
        let margin = proto_hue - WARNING_HUE_FLOOR_DEG;
        assert!(
            (margin - 23.607).abs() < 0.05,
            "запас прототипа над полом {margin:.4}° ушёл от ≈23.607° — сдвинулся \
             orange-прототип или пол; осмыслить провенанс, не подгонять"
        );
    }

    #[test]
    fn warning_stays_distinguishable_from_danger_full_circle() {
        // Доказанный дефект машинным тестом: с picker на основе membership-field
        // Warning мог резолвиться в 3.9° от Danger (перцептивно один цвет) при
        // brand≈56°. Smooth-resolver + floor держат чёткий зазор везде.
        // prototype_hex: Warning = Figma #FFA100; Danger = Figma #FF3B30.
        let n = neutral();
        let mut brand = 0.0;
        let mut worst = f64::INFINITY;
        while brand < 360.0 {
            let w = SentimentCurve::from_sentiment(Sentiment::Warning, brand, "#FFA100", &n)
                .unwrap()
                .resolved_hue;
            let d = SentimentCurve::from_sentiment(Sentiment::Danger, brand, "#FF3B30", &n)
                .unwrap()
                .resolved_hue;
            worst = worst.min(angular_distance(w, d));
            brand += 0.25;
        }
        assert!(
            worst >= 10.0,
            "Warning↔Danger closest approach {worst:.2}° (must stay >= 10° apart)"
        );
    }

    // УДАЛЁН Волной 1: `resolved_hue_is_smooth_between_its_two_seams` — страж
    // C¹-непрерывности p-норм-резолвера по бренду. Резолвер удалён (сентимент
    // отдыхает на прототипе, от бренда не зависит), «швов» больше нет — тест стал
    // беспредметным.

    #[test]
    fn legalize_hue_errs_when_zones_leave_no_legal_arc() {
        // Ветка ошибки обязана выдать `Err`, не тихо нарушить инвариант. Под законом
        // зон единственный способ опустошить дугу — попарные зоны соседей: зона с
        // отступом > 180° недостижима ни одним оттенком (max angular_distance = 180°),
        // поэтому легальных нет.
        let cover = [NeighborZone {
            hue_deg: 0.0,
            min_sep_deg: 181.0,
        }];
        let r = legalize_hue_among(0.0, None, &cover);
        assert!(
            r.is_err(),
            "ожидался Err, когда зона соседа не оставляет легального оттенка, получено {r:?}"
        );

        // Санити: ослабить зону — легальный оттенок снова есть (тот же вход иначе),
        // значит Err выше — коллизия ограничений, не баг.
        let ok = [NeighborZone {
            hue_deg: 0.0,
            min_sep_deg: 10.0,
        }];
        assert!(legalize_hue_among(0.0, None, &ok).is_ok());
    }

    /// Деривационная идентичность: S_PERC_MIN = 2 × C_rep_figma × sin(20°/2),
    /// где C_rep_figma — средняя Oklab-хрома четырёх якорей из Figma CONTENTS
    /// (коллекция «🔵 4.1 Primitives», Light-mode, 2026-06-30).
    /// Порог 20° — нижний предел категориального восприятия оттенка по
    /// Witzel & Gegenfurtner (2013), Journal of Vision 13(7):1 (DOI 10.1167/13.7.1).
    /// Допуск 1e-4: хромы зафиксированы до 4 знаков, итоговая погрешность
    /// деривации существенно меньше — допуск исключает реальный дрейф константы.
    #[test]
    fn s_perc_min_derivation_identity() {
        // Oklab C якорей из Figma CONTENTS, 2026-06-30:
        let c_figma = [0.2321_f64, 0.1717_f64, 0.1944_f64, 0.1931_f64];
        let c_rep = c_figma.iter().sum::<f64>() / c_figma.len() as f64;
        // Геометрическая деривация: 2 × C_rep × sin(20°/2)
        // Источник порога 20°: Witzel & Gegenfurtner (2013), Journal of Vision 13(7):1 (DOI 10.1167/13.7.1)
        let derived = 2.0 * c_rep * (10.0_f64.to_radians()).sin();
        assert!(
            (S_PERC_MIN - derived).abs() < 1e-4,
            "S_PERC_MIN = {S_PERC_MIN:.7} != выведено {derived:.7} \
             (разница {:.7} >= 1e-4; значение должно совпадать с Figma-деривацией)",
            (S_PERC_MIN - derived).abs()
        );
    }

    /// Инверсия хорды: достижимый порог строго внутри (0, 180°); сатурация
    /// (chord ≥ 2C) — маркер ровно 180°; мусор-входы — честный Err, не
    /// NaN-градусы дальше по физике.
    #[test]
    fn chord_inversion_saturates_and_rejects_garbage() {
        let deg = s_min_deg_from_chord(0.05, 0.1).expect("достижимая хорда");
        assert!(deg > 0.0 && deg < 180.0, "0 < {deg} < 180");
        // Ровно диаметр и выше — маркер сатурации.
        assert_eq!(s_min_deg_from_chord(0.2, 0.1).unwrap(), 180.0);
        assert_eq!(s_min_deg_from_chord(0.5, 0.1).unwrap(), 180.0);
        for (chord, zone) in [
            (f64::NAN, 0.1),
            (0.1, f64::NAN),
            (f64::INFINITY, 0.1),
            (-0.1, 0.1),
            (0.1, 0.0),
            (0.1, -0.1),
        ] {
            assert!(
                s_min_deg_from_chord(chord, zone).is_err(),
                "({chord}, {zone}) обязана быть отвергнута"
            );
        }
    }

    // УДАЛЁН Волной 1: `saturated_chord_resolves_to_diametric_hue` — сатурационная
    // ветвь (chord ≥ 2C → диаметраль от бренда) была частью brand-separation и
    // вырезана целиком; её больше не существует.

    /// Публичная граница `resolve_config_sentiment_solid` (новая сигнатура):
    /// недоменный `hue_floor` или `chroma_fraction` — честный Err, не тихий hex.
    #[test]
    fn config_sentiment_public_boundary_rejects_garbage_angles() {
        for bad_floor in [f64::NAN, -1.0, 360.0, f64::INFINITY] {
            assert!(
                resolve_config_sentiment_solid("#FF3B30", 1.0, Some(bad_floor)).is_err(),
                "hue_floor={bad_floor} обязан быть отвергнут"
            );
        }
        for bad_fraction in [f64::NAN, 0.0, -0.1, 1.5, f64::INFINITY] {
            assert!(
                resolve_config_sentiment_solid("#FF3B30", bad_fraction, None).is_err(),
                "chroma_fraction={bad_fraction} обязан быть отвергнут"
            );
        }
        // Валидный вход резолвится.
        assert!(resolve_config_sentiment_solid("#FF3B30", 1.0, None).is_ok());
    }

    /// Ахроматичный якорь семейства не несёт оттенка: разведения нет — солид равен
    /// сырому якорю. Fast path НЕ обходит валидацию ручек: недоменные
    /// chroma_fraction/hue_floor отвергаются и на сером якоре (валидация ДО раннего
    /// возврата — мусор не «везёт» в зависимости от цвета).
    #[test]
    fn achromatic_family_anchor_returns_raw_anchor() {
        let solid =
            resolve_config_sentiment_solid("#808080", 1.0, None).expect("серый якорь легален");
        assert_eq!(solid, "#808080", "серый якорь возвращается байт-в-байт");

        for bad_fraction in [0.0, f64::NAN, -0.01, 1.5, f64::INFINITY] {
            assert!(
                resolve_config_sentiment_solid("#808080", bad_fraction, None).is_err(),
                "chroma_fraction={bad_fraction} обязан отвергаться и на сером"
            );
        }
        assert!(
            resolve_config_sentiment_solid("#808080", 1.0, Some(-1.0)).is_err(),
            "недоменный hue_floor обязан отвергаться и на сером"
        );
    }

    /// Домен нового резолвера `resolve_sentiment_hue_among`: неконечный `prototype`
    /// и недоменный `hue_floor` — честный Err, не NaN-оттенок вниз по физике.
    #[test]
    fn resolve_sentiment_hue_rejects_garbage_domain() {
        assert!(resolve_sentiment_hue_among(None, f64::NAN, &[]).is_err());
        assert!(resolve_sentiment_hue_among(None, f64::INFINITY, &[]).is_err());
        assert!(resolve_sentiment_hue_among(Some(360.0), 68.0, &[]).is_err());
        assert!(resolve_sentiment_hue_among(Some(f64::NAN), 68.0, &[]).is_err());
        assert!(resolve_sentiment_hue_among(Some(-1.0), 68.0, &[]).is_err());
        // Валидный вход резолвится и возвращает нормализованный прототип (покой).
        let h = resolve_sentiment_hue_among(None, 68.0, &[]).expect("валидный");
        assert!((h - 68.0).abs() < 1e-9, "покой на прототипе: {h}");
    }

    /// АРБИТР того, что СОХРАНЁННАЯ машинерия зон соседей ЖИВА (достижима через
    /// пол), а не мёртвый код. Сценарий: якорь семейства (red, прототип ≈ 28.66°)
    /// лежит НИЖЕ своего пола (40°), поэтому клампится ВВЕРХ к полу — и там
    /// сталкивается с зоной покоящегося соседа (45°/±10°). Зона обязана оттолкнуть
    /// решённый оттенок на ≥ min_sep, доказывая, что зоны реально применяются.
    #[test]
    fn floor_clamp_collision_triggers_neighbor_separation() {
        let anchor = "#FF3B30"; // red, Oklab hue ≈ 28.66° — ниже пола 40°
        let floor = Some(40.0);
        let neighbor_hue = 45.0;
        let min_sep = 10.0;
        let zones = [NeighborZone {
            hue_deg: neighbor_hue,
            min_sep_deg: min_sep,
        }];

        // Без зоны: чистый кламп к полу (40°) — сосед не мешает.
        let bare = resolve_config_sentiment_solid_among(anchor, 1.0, floor, &[])
            .expect("клампится к полу");
        let bare_hue = oklab_hue_of(&bare);
        assert!(
            (bare_hue - 40.0).abs() < 1.5,
            "без зоны red клампится к полу 40°, получено {bare_hue:.2}°"
        );

        // С зоной у 45°/±10°: пол ставит кандидата в её ±10° коридор → зона
        // отталкивает решённый оттенок на ≥ min_sep. Доказывает, что зоны применяются.
        let with_zone = resolve_config_sentiment_solid_among(anchor, 1.0, floor, &zones)
            .expect("резолвится с зоной");
        let with_hue = oklab_hue_of(&with_zone);
        assert!(
            with_hue >= 40.0 - 1e-6,
            "решённый оттенок {with_hue:.2}° обязан оставаться над полом 40°"
        );
        // hex-эмиссия квантует угол на доли градуса — допуск 0.5°.
        let sep_with = angular_distance(with_hue, neighbor_hue);
        assert!(
            sep_with >= min_sep - 0.5,
            "зона соседа у {neighbor_hue}° обязана оттолкнуть оттенок на ≥ {min_sep}°, \
             получено {sep_with:.2}° (решённый {with_hue:.2}°)"
        );
        // И решённый С зоной ДАЛЬШЕ от соседа, чем без зоны (зона реально подвинула).
        let sep_bare = angular_distance(bare_hue, neighbor_hue);
        assert!(
            sep_with > sep_bare,
            "зона обязана УВЕЛИЧИТЬ отступ от соседа (без зоны {sep_bare:.2}°, с зоной {sep_with:.2}°)"
        );
    }
}

// Научные локи (a) DERIVABLE (волна science/constants-objectivization). Значения
// НЕ меняются — тесты предъявляют вывод из представимости/определения окружности.
#[cfg(test)]
mod derivator_locks {
    use super::{
        ACHROMATIC_CHROMA_EPS, HUE_DOMAIN_MAX_EXCLUSIVE, HUE_DOMAIN_MIN_INCLUSIVE, normalize_hue,
    };
    use crate::spaces::oklab::srgb_linear_to_oklab;
    use crate::spaces::srgb::srgb_gamma_inv;

    fn oklab_chroma(rgb: [u8; 3]) -> f64 {
        let e = [
            rgb[0] as f64 / 255.0,
            rgb[1] as f64 / 255.0,
            rgb[2] as f64 / 255.0,
        ];
        let l = [
            srgb_gamma_inv(e[0]),
            srgb_gamma_inv(e[1]),
            srgb_gamma_inv(e[2]),
        ];
        let lab = srgb_linear_to_oklab(l);
        lab[1].hypot(lab[2])
    }

    /// (a) ACHROMATIC_CHROMA_EPS выведен из АРИФМЕТИКИ ПРЕДСТАВИМОСТИ: строго зажат
    /// между двумя ИЗМЕРЕННЫМИ границами конвейера sRGB->Oklab — потолком f64-шума
    /// истинно ахроматических (R=G=B) цветов снизу и полом минимальной ненулевой
    /// хромы представимого 8-битного цвета сверху.
    ///
    /// КОРРЕКЦИЯ ПРОВЕНАНСА: замер даёт потолок шума ~3.7e-8 (НЕ <=1e-12, как
    /// ранее заявлял реестр) — EPS сидит лишь ~2.7x ВЫШЕ потолка шума. Запас >=3
    /// порядка держится ТОЛЬКО сверху (мин. представимая хрома ~1.06e-3). Обе
    /// границы соблюдены, EPS не переклассифицирует ни один представимый цвет.
    #[test]
    fn achromatic_eps_between_f64_noise_and_min_representable_chroma() {
        let mut noise = 0.0f64;
        for i in 0u16..=255 {
            noise = noise.max(oklab_chroma([i as u8, i as u8, i as u8]));
        }
        let mut min_nz = f64::INFINITY;
        for i in 0u16..=254 {
            let i = i as u8;
            for c in [[i, i, i + 1], [i, i + 1, i + 1], [i, i + 1, i]] {
                let ch = oklab_chroma(c);
                if ch > 0.0 {
                    min_nz = min_nz.min(ch);
                }
            }
        }
        assert!(
            noise < ACHROMATIC_CHROMA_EPS,
            "потолок f64-шума серых {noise:.3e} ниже EPS={ACHROMATIC_CHROMA_EPS:.0e}"
        );
        assert!(
            (1e-8..1e-7).contains(&noise),
            "замеренный потолок шума {noise:.3e} вне полосы [1e-8,1e-7) — провенанс изменился"
        );
        assert!(
            min_nz > ACHROMATIC_CHROMA_EPS * 1e3,
            "мин. представимая ненулевая хрома {min_nz:.3e} >= 3 порядков выше EPS"
        );
    }

    /// (a) HUE_DOMAIN_{MIN_INCLUSIVE,MAX_EXCLUSIVE} — ОПРЕДЕЛЕНИЕ угла по модулю
    /// 360 (не политика): normalize_hue = rem_euclid(360) имеет кодомен ровно
    /// [0, 360). Границы выводятся из кодомена нормализатора.
    #[test]
    fn hue_domain_is_the_circle_modulus_codomain() {
        assert_eq!(HUE_DOMAIN_MIN_INCLUSIVE, 0.0);
        assert_eq!(HUE_DOMAIN_MAX_EXCLUSIVE, 360.0);
        for k in -5..=5 {
            for step in 0..360 {
                let h = k as f64 * 360.0 + step as f64 + 0.5;
                let n = normalize_hue(h);
                assert!(
                    (HUE_DOMAIN_MIN_INCLUSIVE..HUE_DOMAIN_MAX_EXCLUSIVE).contains(&n),
                    "normalize_hue({h})={n} вне домена"
                );
            }
        }
        assert_eq!(normalize_hue(360.0), HUE_DOMAIN_MIN_INCLUSIVE);
        assert!((normalize_hue(-1.0) - 359.0).abs() < 1e-9);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Волна 2 «объективизация» — терминал (e) для CHROMA_FRACTION.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod wave2_e_locks {
    use super::{CHROMA_FRACTION, oklab_lc_to_hex};
    use crate::scale::max_chroma;
    use crate::spaces::oklab::srgb_linear_to_oklab;
    use crate::spaces::srgb::srgb_from_hex;

    fn de_ok_hex(a: &str, b: &str) -> f64 {
        let la = srgb_linear_to_oklab(srgb_from_hex(a).unwrap());
        let lb = srgb_linear_to_oklab(srgb_from_hex(b).unwrap());
        ((la[0] - lb[0]).powi(2) + (la[1] - lb[1]).powi(2) + (la[2] - lb[2]).powi(2)).sqrt()
    }

    /// (e) DESIGN-CHOICE sensitivity-лок для `CHROMA_FRACTION`. Свип [0.70, 1.0]
    /// на реальных сентимент-якорях labui (danger/warning/success/info): непрерывный
    /// МАТЕРИАЛЬНЫЙ дрейф (fraction прямо масштабирует хрому), значит (e), не (c).
    /// КУСАЕТСЯ: value-пин `== 0.88` падает на любой мутации.
    #[test]
    fn chroma_fraction_sensitivity_is_bounded() {
        assert_eq!(
            CHROMA_FRACTION, 0.88,
            "реестровый дефолт доли хромы сентимента"
        );
        let anchors = ["#FF3B30", "#FF9008", "#34C759", "#3E87FF"];
        let mut max_de = 0.0_f64;
        for hex in anchors {
            let lab = srgb_linear_to_oklab(srgb_from_hex(hex).unwrap());
            let l = lab[0];
            let h = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);
            let base = oklab_lc_to_hex(l, CHROMA_FRACTION * max_chroma(l, h), h);
            for f in [0.70_f64, 0.80, 0.85, 0.92, 0.96, 1.0] {
                max_de = max_de.max(de_ok_hex(
                    &base,
                    &oklab_lc_to_hex(l, f * max_chroma(l, h), h),
                ));
            }
        }
        assert!(
            (0.025..0.06).contains(&max_de),
            "max ΔE_ok по [0.70,1.0] {max_de:.4} вне замеренного [0.025, 0.06) — ручка материальна (e)"
        );
        eprintln!(
            "WAVE2 CHROMA_FRACTION (e): max ΔE_ok[0.70,1.0]={max_de:.4} (материальна → (e), не (c); labui=1.0)"
        );
    }
}
