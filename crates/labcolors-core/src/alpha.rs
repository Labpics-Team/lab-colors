//! Альфа-аналог солидного цвета: прямой и обратный ход straight-alpha
//! композита в ГАММА-КОДИРОВАННОМ sRGB.
//!
//! Кодированный домен заземлён измерением: модель `c = α·t + (1−α)·b`
//! воспроизводит все 12 семантических Figma-якорей движка
//! (`reference/labui-figma-structure.md` §3–§4, воспроизводимо
//! `cargo run -p labcolors-core --example figma_anchor_provenance`). Для
//! эмиссии контракт дополнительно фиксирует **encoded-sRGB8 source-over
//! reference**: арифметика идёт на байтах `0..255` с одним итоговым round.
//! Совпадение конкретного renderer зависит от его color-management профиля и
//! проверяется отдельно; линейный свет здесь только для последующей колориметрии.
//!
//! # Зачем обратный ход
//!
//! Движок решает роли СОЛИДАМИ (контраст-корректными на данном фоне). Альфа-
//! аналог роли — пара `(tint, α)`, чей композит на том же фоне равен солиду:
//!
//! ```text
//! t = (c − (1−α)·b) / α        (по каналам, кодированные значения)
//! ```
//!
//! В вещественной алгебре continuous encoded-sRGB композит инверсии равен
//! солиду тождественно. Реализация на `binary64` честно отделена: ошибка
//! round-trip ограничена выведенной оценкой `8·ε/α` и проверяется тестом
//! `inversion_identity_respects_derived_binary64_error_bound`. После эмиссии тинт уже
//! лежит на sRGB8-сетке; побайтовый контракт ниже доказывается и проверяется
//! отдельно, а не приписывается непрерывной функции.
//! На ином фоне композит другой — это и есть смысл альфы (адаптация к
//! подложке), гарантия формулируется для фона, на котором решён солид.
//!
//! # Разрешимость и границы квантования
//!
//! Тинт обязан лежать в гамуте `[0,1]³`. Поканальная алгебра нижней границы α:
//!
//! ```text
//! t ≥ 0  ⇔  α ≥ (b − c) / b        (канал с c < b; при b = 0 недостижимо, если c > 0 — но тогда c > b)
//! t ≤ 1  ⇔  α ≥ (c − b) / (1 − b)  (канал с c > b; при b = 1 симметрично)
//! ```
//!
//! [`min_alpha_encoded`] начинает с максимума этих алгебраических границ, затем
//! возвращает первый `binary64`, на котором строгая binary64-инверсия реально
//! лежит в `[0,1]³`. [`invert_composite_encoded`] может также принять более
//! раннюю граничную пару только если повторный binary64-композит побитно равен
//! входному solid; глобального epsilon и безусловного clamp нет. Это не минимальная α
//! дискретного sRGB8-композита: `#010000` над чёрным уже округляется из белого
//! красного канала при `α=0.5/255`, тогда как strict-binary64 пол равен `1/255`.
//!
//! Эмиссионный путь НЕ использует continuous-пол как суррогат byte-grid пола.
//! После квантования solid/background он сначала исчерпывающе решает три
//! независимых одноканальных диапазона тинта на запрошенной alpha. Если решения
//! нет, lower-bound по упорядоченным битам `f64` находит первый `binary64`,
//! проходящий ТОТ ЖЕ [`composite_over_srgb8`]; predecessor обязан не проходить.
//! Поэтому округлительно разрешимые пары вроде `#FF0000 @ 0.12 → #1F0000`
//! сохраняют запрошенную прозрачность, а `alphaCoerced` не врёт о деградации.
//! Весь одноканальный домен из 65 536 `(S, B)` проверяет и точный композит, и
//! минимальность фактической alpha.
//!
//! Это не обещает восстановить исходный тинт: солид из 8-битного hex несёт
//! ошибку ≤ 0.5/255, которую инверсия масштабирует в `1/α` раз. Граница
//! `0.5/(255·α)` запинена тестом `quantisation_error_bound_is_honoured`.

use crate::spaces::srgb::{hex_from_srgb_encoded, srgb_encoded_from_hex};

#[cfg(test)]
pub(crate) fn reset_source_over_evaluation_count() {
    crate::composition::reset_source_over_evaluation_count();
}

#[cfg(test)]
pub(crate) fn source_over_evaluation_count() -> usize {
    crate::composition::source_over_evaluation_count()
}

/// Валидный кодированный канал/цвет: конечный и в `[0,1]` — домен всех
/// функций модуля (hex-обёртки гарантируют его по построению, byte/255).
fn is_encoded_rgb(v: [f64; 3]) -> bool {
    v.into_iter()
        .all(|x| x.is_finite() && (0.0..=1.0).contains(&x))
}

/// Непрерывная алгебра straight-alpha до квантования. Она нужна инверсии и
/// материалам; путь эмитируемого 8-битного пикселя использует
/// [`composite_over_srgb8`], чтобы нормализация `byte/255` не сдвигала half-tie.
pub(crate) fn composite_over_encoded_unchecked(
    tint: [f64; 3],
    alpha: f64,
    bg: [f64; 3],
) -> [f64; 3] {
    [
        alpha * tint[0] + (1.0 - alpha) * bg[0],
        alpha * tint[1] + (1.0 - alpha) * bg[1],
        alpha * tint[2] + (1.0 - alpha) * bg[2],
    ]
}

/// Прямой непрерывный ход: `α·tint + (1−α)·bg` по кодированным каналам.
///
/// Это алгебра над `[0,1]`, не финальное округление sRGB8-reference. Для
/// эмитированных sRGB8-цветов используй [`composite_over_srgb8`].
///
/// # Errors
///
/// `Err`, если канал не конечен/вне `[0,1]` либо `alpha` не конечна/вне
/// `[0,1]`. Публичная граница не зажимает мусор и одинакова в debug/release.
pub fn composite_over_encoded(
    tint: [f64; 3],
    alpha: f64,
    bg: [f64; 3],
) -> Result<[f64; 3], String> {
    if !is_encoded_rgb(tint) {
        return Err(format!("tint вне конечного encoded-sRGB [0,1]: {tint:?}"));
    }
    if !is_encoded_rgb(bg) {
        return Err(format!("bg вне конечного encoded-sRGB [0,1]: {bg:?}"));
    }
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(format!("alpha вне конечного [0,1]: {alpha}"));
    }
    Ok(composite_over_encoded_unchecked(tint, alpha, bg))
}

/// Straight-alpha непрозрачного sRGB8-фона в шкале `0..255`, которую несут
/// каналы эмитированного CSS-цвета.
///
/// Округление выполняется один раз, после композиции. Это существенно на
/// half-tie: `(250/255)·0.122·255` может стать `30.499…`, хотя эталонная
/// byte-reference `250·0.122` равен `30.5` и по round-half-up даёт байт 31.
/// Binary64-операции выполняются как монотонная affine-форма
/// `bg + alpha*(tint-bg)` — официальный JS-runtime вызывает этот Core-профиль,
/// а не воспроизводит формулу отдельно. Expanded-форма запрещена: на ULP-швах
/// она способна дать последовательность PASS→FAIL→PASS при росте alpha.
///
/// # Errors
///
/// `Err`, если `alpha` не конечна или лежит вне `[0,1]`.
pub fn composite_over_srgb8(tint: [u8; 3], alpha: f64, bg: [u8; 3]) -> Result<[u8; 3], String> {
    crate::composition::source_over_srgb8(tint, alpha, bg)
}

/// Квантизация кодированного цвета в эмитируемые sRGB8-байты с доменной
/// проверкой — тот же контракт, которым композитор готовит свои входы.
/// `label` попадает в текст доменного отказа (`tint`/`bg` исторические).
pub(crate) fn encoded_to_srgb8(rgb: [f64; 3], label: &str) -> Result<[u8; 3], String> {
    if !is_encoded_rgb(rgb) {
        return Err(format!("{label} вне конечного encoded-sRGB [0,1]: {rgb:?}"));
    }
    Ok(rgb.map(|channel| (channel * 255.0).round() as u8))
}

/// Форматирование эмитируемых sRGB8-байт в `#RRGGBB` — единая точка формата
/// для композитора и потребителей его байтовых результатов (appearance-граф).
pub(crate) fn hex_from_srgb8(rgb: [u8; 3]) -> String {
    crate::Srgb8::new(rgb).to_hex()
}

/// Общий внутренний путь для semantic-метрик: вход уже квантован для эмиссии,
/// но повторная проверка не позволяет новому call site протащить NaN или clamp.
pub(crate) fn composite_srgb8_from_encoded(
    tint: [f64; 3],
    alpha: f64,
    bg: [f64; 3],
) -> Result<[u8; 3], String> {
    composite_over_srgb8(
        encoded_to_srgb8(tint, "tint")?,
        alpha,
        encoded_to_srgb8(bg, "bg")?,
    )
}

pub(crate) fn composite_hex_from_encoded(
    tint: [f64; 3],
    alpha: f64,
    bg: [f64; 3],
) -> Result<String, String> {
    composite_srgb8_from_encoded(tint, alpha, bg).map(hex_from_srgb8)
}

/// Обратный ход: тинт, чей композит с `alpha` на `bg` равен `solid`.
///
/// `None`, если вход вне домена (`solid`/`bg` не кодированные цвета `[0,1]³`
/// или не конечные), `alpha` не в `(0, 1]` (при α=0 инверсия вырождена —
/// композит не зависит от тинта), либо хотя бы один канал тинта выходит из
/// гамута `[0,1]`. Значение за границей канонизируется в 0/1 только при
/// побитном постусловии прямого binary64-хода; иначе возвращается `None`.
pub fn invert_composite_encoded(solid: [f64; 3], alpha: f64, bg: [f64; 3]) -> Option<[f64; 3]> {
    if !(is_encoded_rgb(solid) && is_encoded_rgb(bg) && alpha > 0.0 && alpha <= 1.0) {
        return None;
    }
    let mut tint = [0.0; 3];
    let mut projected_boundary = [false; 3];
    for c in 0..3 {
        let t = bg[c] + (solid[c] - bg[c]) / alpha;
        if (0.0..=1.0).contains(&t) {
            tint[c] = t;
        } else {
            // Никакого epsilon: пробуем только математическую границу gamut.
            // Она допустима лишь когда ТОТ ЖЕ binary64-прямой ход побитно
            // воспроизводит входной solid; иначе это была бы подмена цвета.
            tint[c] = if t < 0.0 { 0.0 } else { 1.0 };
            projected_boundary[c] = true;
        }
    }
    if projected_boundary.into_iter().any(|projected| projected) {
        let recomposed = composite_over_encoded_unchecked(tint, alpha, bg);
        if (0..3).any(|c| projected_boundary[c] && recomposed[c].to_bits() != solid[c].to_bits()) {
            return None;
        }
    }
    Some(tint)
}

/// Строгая binary64-инверсия уже проверенного домена.
///
/// Форма `bg + (solid-bg)/alpha` уменьшает отмену близких величин. Для каждого
/// знака `solid-bg` правильно округлённые IEEE-операции монотонны по alpha;
/// поэтому предикат попадания в `[0,1]` пригоден для exact lower-bound ниже.
fn invert_composite_strict(solid: [f64; 3], alpha: f64, bg: [f64; 3]) -> Option<[f64; 3]> {
    let mut tint = [0.0; 3];
    for c in 0..3 {
        let t = bg[c] + (solid[c] - bg[c]) / alpha;
        if !(0.0..=1.0).contains(&t) {
            return None;
        }
        tint[c] = t;
    }
    Some(tint)
}

/// Binary64-вычисление алгебраической границы непрерывной инверсии.
///
/// Это только стартовая точка поиска: округление деления может поставить её
/// как выше, так и ниже первого `binary64`, проходящего фактический строгий
/// предикат [`invert_composite_strict`].
fn analytic_alpha_candidate(solid: [f64; 3], bg: [f64; 3]) -> f64 {
    let mut lo = 0.0f64;
    for c in 0..3 {
        let (s, b) = (solid[c], bg[c]);
        let bound = if s < b {
            (b - s) / b
        } else if s > b {
            (s - b) / (1.0 - b)
        } else {
            0.0
        };
        lo = lo.max(bound);
    }
    lo.clamp(0.0, 1.0)
}

/// Первый `binary64`, на котором строгая binary64-инверсия `solid` над `bg`
/// возвращает тинт в `[0,1]³`. Для `solid == bg` по определению равен 0.
/// Предыдущее представимое число обязательно не проходит тот же предикат.
/// Это точный дискретный минимум реализации, но не точный вещественный инфимум:
/// округление операций способно сдвинуть его относительно алгебраической
/// границы в обе стороны.
///
/// Это также не минимальная α на sRGB8-сетке: итоговый round иногда делает
/// целевой байт достижимым раньше.
/// Например, `solid=1/255`, `bg=0` даёт здесь `1/255`, хотя tint byte 255 при
/// `α=0.5/255` уже композитится в byte 1.
///
/// `None` при входе вне домена (не кодированный цвет `[0,1]³` / не конечный) —
/// молчаливый ответ на мусор был бы ложным обещанием разрешимости.
pub fn min_alpha_encoded(solid: [f64; 3], bg: [f64; 3]) -> Option<f64> {
    if !is_encoded_rgb(solid) || !is_encoded_rgb(bg) {
        return None;
    }
    if solid == bg {
        return Some(0.0);
    }
    let analytic = analytic_alpha_candidate(solid, bg);
    // Для двух разных цветов хотя бы один канал различается. В валидном
    // `[0,1]`-домене соответствующий знаменатель строго положителен, поэтому и
    // его поканальная граница, и максимум границ обязаны быть больше нуля.
    debug_assert!(
        analytic > 0.0,
        "solid != bg обязан давать положительную analytic alpha: solid={solid:?}, bg={bg:?}, analytic={analytic}"
    );

    // Положительные finite f64 упорядочены unsigned-битами. Сначала
    // экспоненциально находим ближайшую пару «не проходит / проходит» вокруг
    // аналитического кандидата; так обычный случай занимает несколько проб,
    // но ответ не зависит от эвристического лимита числа ULP.
    let analytic_bits = analytic.to_bits();
    let one_bits = 1.0f64.to_bits();
    let (mut failing_bits, mut passing_bits) =
        if invert_composite_strict(solid, analytic, bg).is_some() {
            let mut passing = analytic_bits;
            let mut step = 1_u64;
            loop {
                let candidate = passing.saturating_sub(step);
                if invert_composite_strict(solid, f64::from_bits(candidate), bg).is_none() {
                    break (candidate, passing);
                }
                passing = candidate;
                step = step.saturating_mul(2);
            }
        } else {
            let mut failing = analytic_bits;
            let mut step = 1_u64;
            loop {
                let candidate = failing.saturating_add(step).min(one_bits);
                if invert_composite_strict(solid, f64::from_bits(candidate), bg).is_some() {
                    break (failing, candidate);
                }
                failing = candidate;
                step = step.saturating_mul(2);
            }
        };

    // Монотонность строгой формы превращает найденный интервал в обычный
    // exact lower-bound по целочисленным битам — epsilon и float-midpoint не
    // участвуют.
    while passing_bits - failing_bits > 1 {
        let mid_bits = failing_bits + (passing_bits - failing_bits) / 2;
        let mid = f64::from_bits(mid_bits);
        if invert_composite_strict(solid, mid, bg).is_some() {
            passing_bits = mid_bits;
        } else {
            failing_bits = mid_bits;
        }
    }
    Some(f64::from_bits(passing_bits))
}

/// Hex-обёртка прямого хода: композит `tint_hex @ alpha` над `bg_hex`,
/// квантованный до 8-битного hex — канонический source-over reference-композит.
///
/// # Errors
///
/// `Err` при невалидном hex на любом входе либо неконечной/внедиапазонной
/// `alpha`; ни одна сборка не подменяет такой ввод clamp-результатом.
pub fn composite_hex(tint_hex: &str, alpha: f64, bg_hex: &str) -> Result<String, String> {
    let tint = srgb_encoded_from_hex(tint_hex)?;
    let bg = srgb_encoded_from_hex(bg_hex)?;
    composite_hex_from_encoded(tint, alpha, bg)
}

/// Hex-обёртка обратного хода: тинт для `solid_hex @ alpha` над `bg_hex`,
/// квантованный до hex; `Ok(None)` при неразрешимости в гамуте.
///
/// # Errors
///
/// `Err` при невалидном hex на любом входе либо при неконечной/внедиапазонной
/// `alpha`. `Ok(None)` зарезервирован для корректного домена: нулевой α или
/// отсутствия тинта в continuous encoded-sRGB gamut.
pub fn invert_composite_hex(
    solid_hex: &str,
    alpha: f64,
    bg_hex: &str,
) -> Result<Option<String>, String> {
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(format!("alpha вне конечного [0,1]: {alpha}"));
    }
    let solid = srgb_encoded_from_hex(solid_hex)?;
    let bg = srgb_encoded_from_hex(bg_hex)?;
    Ok(invert_composite_encoded(solid, alpha, bg).map(hex_from_srgb_encoded))
}

/// Hex-обёртка strict-binary64 пола [`min_alpha_encoded`]. Значение не следует
/// интерпретировать как вещественный инфимум или минимальную α дискретного
/// sRGB8-композита.
///
/// # Errors
///
/// `Err` при невалидном hex на любом входе.
pub fn min_alpha_hex(solid_hex: &str, bg_hex: &str) -> Result<f64, String> {
    Ok(min_alpha_encoded(
        srgb_encoded_from_hex(solid_hex)?,
        srgb_encoded_from_hex(bg_hex)?,
    )
    .expect("hex-вход всегда в домене byte/255 — None недостижим по построению"))
}

/// Непрерывный альфа-аналог солида: тинт + ФАКТИЧЕСКАЯ α.
///
/// Продуктовый слой поверх строгого закона: потребитель всегда получает
/// пригодный ответ без подмены цели клампом. Вещественная формула сохраняет
/// заданный цвет, а ошибка её binary64-вычисления ограничена доказанной выше
/// оценкой. Побайтовая эмиссия в hex/semantic использует отдельный проверяющий
/// sRGB8-путь с точным постусловием по байтам.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlphaAnalog {
    /// Кодированный тинт `[0,1]³`.
    pub tint: [f64; 3],
    /// Фактическая α: запрошенная, если она разрешима, иначе strict-binary64
    /// пол (не вещественный инфимум и не минимальная byte-grid α).
    pub alpha: f64,
}

/// Продуктовый резолвер: ближайший ПРИЕМЛЕМЫЙ альфа-аналог вместо отказа.
///
/// «Приблизить» можно двумя способами, и только один честен: кламп тинта при
/// запрошенной α тихо сдвинул бы композит (система соврала бы о цвете —
/// запрещённая подмена), а подъём α до strict-binary64 пола
/// [`min_alpha_encoded`] сохраняет исходную цель вещественной инверсии; в
/// binary64 остаётся только ограниченная ошибка округления, а не произвольный
/// цветовой сдвиг. Двигается прозрачность, и
/// фактическая α возвращается явно ([`AlphaAnalog::alpha`]). Запрошенная α вне
/// `[0,1]` отвергается: clamp скрыл бы ошибку вызывающего кода. `α=0` входит в
/// библиотечный домен и поднимается до strict-binary64 пола, если solid
/// отличается от фона; при `solid == bg` возвращается вырожденная пара
/// `tint=bg, α=0`.
///
/// `None` — только на входе вне домена (цвет не конечен/не в `[0,1]³` либо
/// запрошенная α не конечна/не в `[0,1]`). Для валидного входа ответ существует
/// всегда (в худшем случае α=1, тинт=солид).
pub fn resolve_alpha_analog(
    solid: [f64; 3],
    requested_alpha: f64,
    bg: [f64; 3],
) -> Option<AlphaAnalog> {
    if !requested_alpha.is_finite() || !(0.0..=1.0).contains(&requested_alpha) {
        return None;
    }
    let floor = min_alpha_encoded(solid, bg)?; // None только на мусор-входах
    // Если запрошенная binary64-пара уже является точной обратной к нашему
    // прямому ходу (включая честную gamut-границу), не поднимаем прозрачность.
    if requested_alpha > 0.0 {
        if let Some(tint) = invert_composite_encoded(solid, requested_alpha, bg) {
            return Some(AlphaAnalog {
                tint,
                alpha: requested_alpha,
            });
        }
    }
    let alpha = requested_alpha.max(floor);
    // При α == floor == 0 солид равен фону: любой видимый эффект отсутствует,
    // тинт = фон (инверсия при α=0 вырожденна — отвечаем без неё).
    if alpha == 0.0 {
        return Some(AlphaAnalog { tint: bg, alpha });
    }
    let tint = invert_composite_encoded(solid, alpha, bg)
        .expect("α ≥ α_min по построению — инверсия разрешима");
    Some(AlphaAnalog { tint, alpha })
}

/// Hex-обёртка эмиссионного sRGB8-резолвера: `(tint_hex, фактическая α)`.
/// Возвращённая пара побайтно воспроизводит
/// `solid_hex` через [`composite_over_srgb8`]; постусловие проверено до возврата.
///
/// # Errors
///
/// `Err` при невалидном hex, неконечной/внедиапазонной запрошенной α либо при
/// нарушении точного sRGB8-постусловия (защитная ветка против численного дрейфа).
pub fn resolve_alpha_analog_hex(
    solid_hex: &str,
    requested_alpha: f64,
    bg_hex: &str,
) -> Result<(String, f64), String> {
    let target = crate::Srgb8::new(crate::srgb8::hex_bytes(solid_hex)?);
    let backdrop = crate::Srgb8::new(crate::srgb8::hex_bytes(bg_hex)?);
    let verified = crate::analog::resolve_verified(
        crate::analog::AuthoredAlphaBindingIdV1::Standalone,
        target,
        requested_alpha,
        backdrop,
    )
    .map_err(resolve_verified_error_message)?;
    Ok((verified.tint().to_hex(), verified.alpha()))
}

fn resolve_verified_error_message(error: crate::analog::ResolveVerifiedErrorV1) -> String {
    match error {
        crate::analog::ResolveVerifiedErrorV1::Proposal(error) => match error {
            crate::analog::AlphaAnalogProposalErrorV1::InvalidRequestedAlpha { bits } => {
                let requested_alpha = f64::from_bits(bits);
                format!("requested_alpha вне конечного [0,1]: {requested_alpha}")
            }
            crate::analog::AlphaAnalogProposalErrorV1::DerivedAlphaOutsideUnitInterval => {
                "выведенная alpha вышла из конечного [0,1]".to_owned()
            }
            crate::analog::AlphaAnalogProposalErrorV1::MissingTintAtFirstAlpha => {
                "первая sRGB8-alpha не дала допустимый byte-тинт".to_owned()
            }
        },
        crate::analog::ResolveVerifiedErrorV1::ConstraintViolation(witness) => format!(
            "alpha-analog не воспроизвёл sRGB8-цель: target={:?}, actual={:?}",
            witness.violation().target().bytes(),
            witness.violation().actual().bytes()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Живые Figma-пары нейтральной лестницы (`reference/labui-figma-structure.md`
    /// §2 — альфы и тинт, §4 — композиты; фоны Backgrounds/Neutral/Primary):
    /// (композит, α, фон). Тинт всех 12 пар — один: `#787880`.
    const TINT: &str = "#787880";
    const BG_LIGHT: &str = "#FFFFFF";
    const BG_DARK: &str = "#101012";
    const FIGMA_PAIRS: &[(&str, f64, &str)] = &[
        ("#E4E4E6", 0.20, BG_LIGHT), // Fills/Neutral/Primary light
        ("#35353A", 0.36, BG_DARK),  // Fills/Neutral/Primary dark
        ("#E9E9EB", 0.16, BG_LIGHT), // Fills/Neutral/Secondary light
        ("#313135", 0.32, BG_DARK),  // Fills/Neutral/Secondary dark
        ("#EFEFF0", 0.12, BG_LIGHT), // Fills/Neutral/Tertiary light
        ("#29292C", 0.24, BG_DARK),  // Fills/Neutral/Tertiary dark
        ("#F4F4F5", 0.08, BG_LIGHT), // Fills/Neutral/Quaternary light
        ("#212124", 0.16, BG_DARK),  // Fills/Neutral/Quaternary dark
        ("#E9E9EB", 0.16, BG_LIGHT), // Border/Neutral/Base light
        ("#252528", 0.20, BG_DARK),  // Border/Neutral/Base dark
        ("#F4F4F5", 0.08, BG_LIGHT), // Border/Neutral/Soft light
        ("#1C1C1F", 0.12, BG_DARK),  // Border/Neutral/Soft dark
    ];

    /// Прямой ход воспроизводит все 12 живых Figma-композитов побайтно —
    /// пространство закона (гамма-кодированный sRGB) подтверждено измерением,
    /// а не памятью.
    #[test]
    fn figma_neutral_ladder_pairs_roundtrip() {
        for (solid, alpha, bg) in FIGMA_PAIRS {
            let got = composite_hex(TINT, *alpha, bg).unwrap();
            assert_eq!(
                &got, solid,
                "композит {TINT}@{alpha} над {bg} разошёлся с Figma-композитом"
            );
        }
    }

    /// Контрпример на точной половине байта: source-over reference считает
    /// каналы в шкале 0..255, поэтому `250 × 0.122 = 30.5` даёт 31.
    /// Нормализация через `(250/255)×255` теряла tie и давала 30.
    #[test]
    fn emitted_composite_uses_byte_reference_arithmetic_at_half_tie() {
        assert_eq!(
            composite_hex("#C0B2FA", 0.122, "#000000").unwrap(),
            "#17161F"
        );
    }

    /// Порядок source-over обязан совпадать с `effective-bg.js`. Affine-
    /// reference вычисляет `5 + 0.1·(0−5) = 4.5` и округляет канал в 5.
    #[test]
    fn source_over_half_seam_matches_the_official_js_operation_order() {
        assert_eq!(composite_hex("#000505", 0.1, "#050505").unwrap(), "#050505");
        let (tint, actual) = resolve_alpha_analog_hex("#040505", 0.1, "#050505").unwrap();
        assert!(
            actual > 0.1,
            "запрошенная пара не воспроизводит красный байт 4"
        );
        assert_eq!(composite_hex(&tint, actual, "#050505").unwrap(), "#040505");
    }

    /// Expanded-форма давала PASS→FAIL→PASS на трёх соседних `binary64`, что
    /// делало обычный lower-bound ложным. Affine-reference монотонен.
    #[test]
    fn source_over_reference_is_monotone_across_the_known_ulp_seam() {
        let centre = 0.812_992_125_984_252_f64;
        let alphas = [
            f64::from_bits(centre.to_bits() - 1),
            centre,
            f64::from_bits(centre.to_bits() + 1),
        ];
        let outputs =
            alphas.map(|alpha| crate::composition::source_over_channel_srgb8(255, alpha, 1));
        assert!(
            outputs.windows(2).all(|pair| pair[0] <= pair[1]),
            "{outputs:?}"
        );
    }

    /// Теорема для всей одноканальной области известной α: эталон считается
    /// целыми как `(122·t + 878·b)/1000`, поэтому тест не повторяет f64-путь
    /// production-кода и ловит нормализацию через `/255·255` на всех half-tie.
    #[test]
    fn byte_reference_alpha_0122_matches_exact_rational_for_all_channel_pairs() {
        for tint in u8::MIN..=u8::MAX {
            for bg in u8::MIN..=u8::MAX {
                let got = composite_over_srgb8([tint, 0, 0], 0.122, [bg, 0, 0]).unwrap()[0];
                let numerator = 122_u32 * u32::from(tint) + 878_u32 * u32::from(bg);
                let expected = ((numerator + 500) / 1_000) as u8;
                assert_eq!(got, expected, "tint={tint}, bg={bg}");
            }
        }
    }

    /// Конечная sRGB8-теорема альфа-аналога: для каждой из 65 536 пар
    /// `(solid, bg)` и каждой представительной запрошенной α проверяющий
    /// production-резолвер обязан вернуть тинт, который публичный byte-
    /// композитор складывает ТОЧНО в исходный solid. `near_one` — соседнее
    /// представимое число перед `1.0`; оно закрывает минимальный запас до
    /// половины байта, где float-дрейф опаснее всего.
    #[test]
    fn srgb8_alpha_analog_is_exact_for_every_channel_pair() {
        let near_one = f64::from_bits(1.0_f64.to_bits() - 1);
        for requested_alpha in [0.0, 0.01, 0.122, 0.5, 0.9, near_one, 1.0] {
            for solid in u8::MIN..=u8::MAX {
                for bg in u8::MIN..=u8::MAX {
                    let target = crate::Srgb8::new([solid; 3]);
                    let backdrop = crate::Srgb8::new([bg; 3]);
                    let requested_is_feasible =
                        crate::analog::tint_at_alpha(target, requested_alpha, backdrop).is_some();
                    let verified = crate::analog::resolve_verified(
                        crate::analog::AuthoredAlphaBindingIdV1::Standalone,
                        target,
                        requested_alpha,
                        backdrop,
                    )
                    .unwrap_or_else(|error| {
                        panic!(
                            "solid={solid}, bg={bg}, requested={requested_alpha}: {}",
                            resolve_verified_error_message(error)
                        )
                    });
                    let tint = verified.tint().bytes();
                    let actual_alpha = verified.alpha();
                    let got = composite_over_srgb8(tint, actual_alpha, [bg; 3])
                        .expect("резолвер возвращает α в [0,1]");
                    assert_eq!(
                        got, [solid; 3],
                        "solid={solid}, bg={bg}, requested={requested_alpha}, actual={actual_alpha}, tint={tint:?}"
                    );
                    if requested_is_feasible {
                        assert_eq!(
                            actual_alpha.to_bits(),
                            requested_alpha.to_bits(),
                            "разрешимая requested alpha была изменена: solid={solid}, bg={bg}"
                        );
                    } else {
                        assert!(actual_alpha > requested_alpha);
                        let predecessor = f64::from_bits(actual_alpha.to_bits() - 1);
                        assert!(
                            crate::analog::tint_at_alpha(target, predecessor, backdrop).is_none(),
                            "actual alpha не минимальна: solid={solid}, bg={bg}, actual={actual_alpha}"
                        );
                    }
                }
            }
        }
    }

    /// Два контрпримера прежнего continuous-floor: byte-grid округление уже
    /// делает цель точной, поэтому прозрачность нельзя увеличивать.
    #[test]
    fn byte_grid_resolver_preserves_every_already_feasible_requested_alpha() {
        let tiny_red_alpha = 0.5 / 255.0;
        let (tiny_tint, tiny_actual) =
            resolve_alpha_analog_hex("#010000", tiny_red_alpha, "#000000").unwrap();
        assert_eq!(tiny_tint, "#FF0000");
        assert_eq!(tiny_actual.to_bits(), tiny_red_alpha.to_bits());
        assert_eq!(
            composite_hex(&tiny_tint, tiny_actual, "#000000").unwrap(),
            "#010000"
        );

        let (tint, actual) = resolve_alpha_analog_hex("#1F0000", 0.12, "#000000").unwrap();
        assert_eq!(tint, "#FF0000");
        assert_eq!(actual.to_bits(), 0.12_f64.to_bits());
        assert_eq!(composite_hex(&tint, actual, "#000000").unwrap(), "#1F0000");
    }

    /// Валидная, но заведомо слишком малая alpha не должна переполнять
    /// целочисленную реконструкцию ни в debug, ни в release. Она поднимается до
    /// первой разрешимой sRGB8-точки и сохраняет целевой байт.
    #[test]
    fn byte_grid_resolver_handles_tiny_normal_and_subnormal_alpha() {
        for requested in [2_f64.powi(-100), f64::from_bits(1)] {
            let (tint, actual) = resolve_alpha_analog_hex("#010000", requested, "#000000").unwrap();
            assert!(actual > requested);
            assert_eq!(composite_hex(&tint, actual, "#000000").unwrap(), "#010000");
            let predecessor = f64::from_bits(actual.to_bits() - 1);
            assert!(
                crate::analog::tint_at_alpha(
                    crate::Srgb8::new([1, 0, 0]),
                    predecessor,
                    crate::Srgb8::new([0; 3]),
                )
                .is_none()
            );
        }
    }

    /// Исчерпывающий одноканальный сертификат reference-предиката: первый
    /// `binary64` проходит, непосредственный predecessor — нет.
    #[test]
    fn byte_grid_alpha_floor_is_first_passing_for_every_channel_pair() {
        for solid in u8::MIN..=u8::MAX {
            for bg in u8::MIN..=u8::MAX {
                let target = [solid, bg, bg];
                let background = [bg; 3];
                let target = crate::Srgb8::new(target);
                let background = crate::Srgb8::new(background);
                let floor = crate::analog::first_alpha(target, background);
                assert!(
                    crate::analog::tint_at_alpha(target, floor, background).is_some(),
                    "solid={solid}, bg={bg}, floor={floor} не проходит"
                );
                if solid == bg {
                    assert_eq!(floor.to_bits(), 0.0_f64.to_bits());
                } else {
                    let predecessor = f64::from_bits(floor.to_bits() - 1);
                    assert!(
                        crate::analog::tint_at_alpha(target, predecessor, background).is_none(),
                        "solid={solid}, bg={bg}: predecessor={predecessor} тоже проходит"
                    );
                }
            }
        }
    }

    /// Контрпример прежнего смешения доменов: `0.25/255` квантуется в byte 0,
    /// но инверсия НЕквантованной цели при α=0.5 дала tint byte 1, чей
    /// production-композит равен 1. Эмиссионный путь обязан сначала квантовать
    /// цель, а глубинная проверка — отвергать заведомо неверную пару.
    #[test]
    fn srgb8_resolver_quantises_target_before_inversion_and_guard_rejects_drift() {
        let off_grid_solid = [0.25 / 255.0; 3];
        let target = crate::Srgb8::new(encoded_to_srgb8(off_grid_solid, "solid").unwrap());
        let backdrop = crate::Srgb8::new([0; 3]);
        let verified = crate::analog::resolve_verified(
            crate::analog::AuthoredAlphaBindingIdV1::Standalone,
            target,
            0.5,
            backdrop,
        )
        .expect("валидный домен всегда имеет конечный sRGB8-ответ");
        let tint = verified.tint().bytes();
        let alpha = verified.alpha();
        assert_eq!(tint, [0; 3]);
        assert_eq!(composite_over_srgb8(tint, alpha, [0; 3]).unwrap(), [0; 3]);
    }

    /// Обратный ход на живых парах: восстановленный тинт отклоняется от
    /// канонического не более чем на границу квантования 0.5/(255·α) на канал,
    /// а его композит воспроизводит опорный hex побайтно (что и является
    /// продуктовой гарантией: полупрозрачная пара красит ровно тот же цвет).
    #[test]
    fn inversion_recovers_figma_tint_within_quantisation_bound() {
        let true_tint = srgb_encoded_from_hex(TINT).unwrap();
        for (solid, alpha, bg) in FIGMA_PAIRS {
            let s = srgb_encoded_from_hex(solid).unwrap();
            let b = srgb_encoded_from_hex(bg).unwrap();
            let tint = invert_composite_encoded(s, *alpha, b)
                .unwrap_or_else(|| panic!("{solid}@{alpha}/{bg}: инверсия неразрешима"));
            let bound = 0.5 / (255.0 * alpha);
            for c in 0..3 {
                assert!(
                    (tint[c] - true_tint[c]).abs() <= bound + 1e-12,
                    "{solid}@{alpha}: канал {c} восстановлен с ошибкой {} > {bound}",
                    (tint[c] - true_tint[c]).abs()
                );
            }
            // Продуктовая гарантия проверяется production byte-композитором,
            // а не повторением непрерывной алгебры инверсии.
            let recomposed = composite_hex_from_encoded(tint, *alpha, b).unwrap();
            assert_eq!(&recomposed, solid, "{solid}@{alpha}: re-композит разошёлся");
        }
    }

    /// Алгебраическое тождество на непрерывных значениях с явной binary64-
    /// границей ошибки. Прямой ход содержит три базовые операции, обратный —
    /// ещё три; стандартная first-order оценка каждой правильно округлённой
    /// операции даёт консервативные `8·ε/α` на восстановленный канал.
    #[test]
    fn inversion_identity_respects_derived_binary64_error_bound() {
        let grid: Vec<f64> = (0..=10).map(|i| f64::from(i) / 10.0).collect();
        for &tr in &grid {
            for &tb in &grid {
                for &br in &grid {
                    let tint = [tr, 0.5, tb];
                    let bg = [br, 0.25, 0.9];
                    for alpha in [0.05, 0.2, 0.5, 0.85, 1.0] {
                        let solid = composite_over_encoded_unchecked(tint, alpha, bg);
                        let back = invert_composite_encoded(solid, alpha, bg).unwrap_or_else(|| {
                            panic!(
                                "прямой ход не обратим: tint={tint:?}, bg={bg:?}, alpha={alpha}, solid={solid:?}"
                            )
                        });
                        for c in 0..3 {
                            let error = (back[c] - tint[c]).abs();
                            let bound = 8.0 * f64::EPSILON / alpha;
                            assert!(error <= bound, "канал {c}: error={error} > {bound}");
                        }
                    }
                }
            }
        }
    }

    /// Полный одноканальный sRGB8-домен фиксирует точное определение пола:
    /// возвращённый `binary64` проходит строгую инверсию, а непосредственно
    /// предшествующий — нет. Проверка predecessor принципиальна: округлённый
    /// аналитический кандидат нередко проходит вместе с несколькими числами
    /// перед ним, поэтому простого `candidate is feasible` недостаточно.
    #[test]
    fn min_alpha_is_first_passing_binary64_for_every_srgb8_channel_pair() {
        for solid_byte in u8::MIN..=u8::MAX {
            for bg_byte in u8::MIN..=u8::MAX {
                let s = f64::from(solid_byte) / 255.0;
                let b = f64::from(bg_byte) / 255.0;
                let solid = [s, 0.25, 0.75];
                let bg = [b, 0.25, 0.75];
                let floor = min_alpha_encoded(solid, bg).expect("sRGB8 всегда в домене");

                if solid_byte == bg_byte {
                    assert_eq!(floor.to_bits(), 0.0_f64.to_bits());
                    continue;
                }

                assert!(
                    invert_composite_strict(solid, floor, bg).is_some(),
                    "solid={solid_byte}, bg={bg_byte}: floor={floor} не проходит"
                );
                let predecessor = f64::from_bits(floor.to_bits() - 1);
                assert!(
                    invert_composite_strict(solid, predecessor, bg).is_none(),
                    "solid={solid_byte}, bg={bg_byte}: predecessor={predecessor} тоже проходит; floor={floor} не минимален"
                );
            }
        }
    }

    /// `min_alpha_encoded` нельзя называть минимумом sRGB8-композита:
    /// финальное округление расширяет достижимое множество на половину кода.
    /// Контрпример также защищает смысл `alphaCoerced`: это сознательно
    /// отдельный strict-binary64 пол.
    #[test]
    fn strict_binary64_floor_is_not_the_byte_grid_minimum() {
        let solid = [1.0 / 255.0, 0.0, 0.0];
        let bg = [0.0; 3];
        let strict_floor = min_alpha_encoded(solid, bg).unwrap();
        let byte_alpha = 0.5 / 255.0;

        assert_eq!(strict_floor, 1.0 / 255.0);
        assert!(byte_alpha < strict_floor);
        assert_eq!(
            composite_over_srgb8([255, 0, 0], byte_alpha, [0; 3]).unwrap(),
            [1, 0, 0]
        );
        assert!(
            invert_composite_encoded(solid, byte_alpha, bg).is_none(),
            "byte достижим округлением, но continuous-тинт был бы вне gamut"
        );
    }

    /// Вырожденные α честно отвергаются, кламп не подменяет ответ.
    #[test]
    fn degenerate_alpha_is_rejected_not_clamped() {
        let s = [0.5, 0.5, 0.5];
        let b = [0.9, 0.9, 0.9];
        assert!(invert_composite_encoded(s, 0.0, b).is_none());
        assert!(invert_composite_encoded(s, -0.1, b).is_none());
        assert!(invert_composite_encoded(s, 1.1, b).is_none());
        // α=1: тинт == солид, тривиально разрешимо.
        assert_eq!(invert_composite_encoded(s, 1.0, b), Some(s));
    }

    /// Выход из gamut даже на доли триллионной не является «машинным шумом»:
    /// глобальный epsilon раньше превращал неразрешимую пару в ложный ответ.
    #[test]
    fn inversion_never_expands_gamut_with_epsilon() {
        let alpha = 1.0 - 5e-13;
        assert!(alpha < min_alpha_encoded([0.0; 3], [1.0; 3]).unwrap());
        assert!(invert_composite_encoded([0.0; 3], alpha, [1.0; 3]).is_none());
    }

    /// Непрерывный резолвер не подменяет целевой цвет: при разрешимой
    /// запрошенной α возвращает её саму; при неразрешимой — поднимает α ровно
    /// до α_min. Ошибка binary64-композита остаётся внутри доказанной оценки
    /// (двигается прозрачность, не целевой цвет). Побайтовый постусловный тест живёт отдельно в
    /// `srgb8_alpha_analog_is_exact_for_every_channel_pair`.
    #[test]
    fn resolver_moves_alpha_never_the_colour() {
        let grid: Vec<f64> = (0..=8).map(|i| f64::from(i) / 8.0).collect();
        for &s in &grid {
            for &b in &grid {
                let solid = [s, 0.4, 0.6];
                let bg = [b, 0.4, 0.6];
                let floor = min_alpha_encoded(solid, bg).expect("в домене");
                for requested in [0.0, 0.05, 0.3, 0.9, 1.0] {
                    let requested_is_exact =
                        requested > 0.0 && invert_composite_encoded(solid, requested, bg).is_some();
                    let a = resolve_alpha_analog(solid, requested, bg).expect("в домене");
                    // Фактическая α: запрошенная, если строгая или побитно-точная
                    // граничная инверсия разрешима; иначе ровно conservative floor.
                    let want = if requested_is_exact {
                        requested
                    } else {
                        requested.max(floor)
                    };
                    assert_eq!(
                        a.alpha.to_bits(),
                        want.to_bits(),
                        "solid={s},bg={b},req={requested}: α={} != {want}",
                        a.alpha
                    );
                    // Отклонение — только ограниченная ошибка binary64, а не
                    // результат клампа или смены целевого цвета.
                    let c = if a.alpha == 0.0 {
                        bg // вырожденный случай solid==bg
                    } else {
                        composite_over_encoded_unchecked(a.tint, a.alpha, bg)
                    };
                    for ch in 0..3 {
                        let error = (c[ch] - solid[ch]).abs();
                        let bound = 8.0 * f64::EPSILON;
                        assert!(
                            error <= bound,
                            "solid={s},bg={b},req={requested}: канал {ch}, error={error} > {bound}"
                        );
                    }
                }
            }
        }
    }

    /// Публичная hex-поверхность не пропускает мусорную α в release-алгебру.
    #[test]
    fn composite_hex_rejects_out_of_range_alpha() {
        for bad in [f64::NAN, -0.1, 1.1, f64::INFINITY] {
            assert!(
                composite_hex("#787880", bad, "#FFFFFF").is_err(),
                "α={bad} обязана быть отвергнута"
            );
        }
    }

    /// Обе публичные прямые границы одинаково отвергают мусор в debug/release;
    /// непрерывная алгебра не доступна как panic/clamp-лазейка.
    #[test]
    fn public_compositors_reject_invalid_numeric_inputs() {
        let valid = [0.25, 0.5, 0.75];
        for alpha in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.1] {
            assert!(composite_over_encoded(valid, alpha, valid).is_err());
            assert!(composite_over_srgb8([1, 2, 3], alpha, [4, 5, 6]).is_err());
        }
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.1] {
            let mut rgb = valid;
            rgb[0] = bad;
            assert!(composite_over_encoded(rgb, 0.5, valid).is_err());
            assert!(composite_over_encoded(valid, 0.5, rgb).is_err());
        }
    }

    /// Все три hex-обёртки публичной поверхности: roundtrip на живой Figma-паре,
    /// плюс честный Err (не паника) на невалидном hex — .expect в min_alpha_hex
    /// недостижим, парсинг падает раньше через `?`.
    #[test]
    fn hex_wrappers_roundtrip_and_reject_invalid_hex() {
        // Roundtrip: Fills/Neutral/Primary light (композит #E4E4E6 = #787880@0.20 над #FFFFFF).
        let tint = invert_composite_hex("#E4E4E6", 0.20, "#FFFFFF")
            .expect("валидный hex")
            .expect("α=0.20 разрешима");
        assert_eq!(composite_hex(&tint, 0.20, "#FFFFFF").unwrap(), "#E4E4E6");
        // min_alpha_hex: для равных цветов пол = 0; для контрастной пары > 0.
        assert_eq!(min_alpha_hex("#FFFFFF", "#FFFFFF").unwrap(), 0.0);
        assert!(min_alpha_hex("#101012", "#FFFFFF").unwrap() > 0.9);
        // Hex-путь: неразрешимая α поднимается, а итоговый sRGB8-композит
        // побайтно равен солиду.
        let (tint2, actual) = resolve_alpha_analog_hex("#101012", 0.05, "#FFFFFF")
            .expect("валидный домен всегда имеет конечный ответ");
        assert!(actual > 0.05, "α обязана подняться до разрешимой");
        assert_eq!(composite_hex(&tint2, actual, "#FFFFFF").unwrap(), "#101012");
        // Невалидный hex — Err на каждой обёртке (никаких паник).
        for f in [
            invert_composite_hex("ош", 0.5, "#FFFFFF").err(),
            min_alpha_hex("#12345", "#FFFFFF").err(),
            resolve_alpha_analog_hex("#GGGGGG", 0.5, "#FFFFFF").err(),
        ] {
            assert!(f.is_some(), "невалидный hex обязан дать Err");
        }

        // Ошибка вызывающего кода не должна выглядеть как физическая
        // неразрешимость корректной пары цветов.
        for bad in [
            f64::NAN,
            f64::NEG_INFINITY,
            f64::INFINITY,
            -f64::EPSILON,
            1.0 + f64::EPSILON,
        ] {
            assert!(
                invert_composite_hex("#808080", bad, "#FFFFFF").is_err(),
                "alpha={bad} обязана дать Err, а не Ok(None)"
            );
        }
        assert_eq!(
            invert_composite_hex("#808080", 0.0, "#FFFFFF").unwrap(),
            None,
            "нулевая alpha валидна, но обратный ход вырожден"
        );
    }

    /// Резолвер строго отвергает α вне `[0,1]`: ни NaN, ни конечное значение за
    /// границей не должны превращаться clamp-ом в правдоподобный ответ.
    #[test]
    fn resolver_rejects_every_out_of_domain_alpha() {
        let ok = [0.5, 0.5, 0.5];
        assert!(resolve_alpha_analog([1.5, 0.0, 0.0], 0.5, ok).is_none());
        for bad in [
            f64::NAN,
            f64::NEG_INFINITY,
            f64::INFINITY,
            -f64::EPSILON,
            1.0 + f64::EPSILON,
            -1.0,
            5.0,
        ] {
            assert!(
                resolve_alpha_analog(ok, bad, ok).is_none(),
                "requested α={bad} обязана быть отвергнута"
            );
            assert!(
                resolve_alpha_analog_hex("#808080", bad, "#808080").is_err(),
                "hex-граница обязана вернуть Err для α={bad}"
            );
        }
    }

    #[test]
    fn public_hex_boundary_stringifies_typed_proposal_failure() {
        let error = resolve_alpha_analog_hex("#000000", -0.25, "#FFFFFF")
            .expect_err("public hex boundary must preserve proposal rejection");
        assert!(error.contains("requested_alpha вне конечного [0,1]"));
    }

    #[test]
    fn public_string_boundary_omits_authored_routing_identity() {
        let message_for = |declaration_ordinal| {
            let error = crate::analog::ExactAlphaProgramV1::evaluate(
                crate::analog::AuthoredAlphaBindingIdV1::Named {
                    declaration_ordinal,
                },
                crate::Srgb8::new([0; 3]),
                crate::Srgb8::new([255; 3]),
                crate::composition::AdmittedOpacityV1::new(0.5).unwrap(),
                crate::Srgb8::new([0; 3]),
            )
            .expect_err("control candidate must violate exact identity");
            let witness = error;
            resolve_verified_error_message(
                crate::analog::ResolveVerifiedErrorV1::ConstraintViolation(witness),
            )
        };

        let first = message_for(2);
        let second = message_for(9);
        assert_eq!(first, second, "routing identity must remain typed-only");
        assert_eq!(
            first,
            "alpha-analog не воспроизвёл sRGB8-цель: target=[0, 0, 0], actual=[128, 128, 128]"
        );
        for forbidden in ["Standalone", "Named", "ordinal", "declaration_ordinal"] {
            assert!(!first.contains(forbidden));
        }
    }

    /// Домен ядра закреплён: внегамутные и неконечные входы отвергаются
    /// (молчаливый ответ на мусор был бы ложным обещанием разрешимости).
    #[test]
    fn out_of_domain_inputs_are_rejected() {
        let ok = [0.5, 0.5, 0.5];
        for bad in [
            [1.5, 0.5, 0.5],
            [-0.1, 0.5, 0.5],
            [f64::NAN, 0.5, 0.5],
            [f64::INFINITY, 0.5, 0.5],
        ] {
            assert!(
                invert_composite_encoded(bad, 0.5, ok).is_none(),
                "{bad:?} как solid"
            );
            assert!(
                invert_composite_encoded(ok, 0.5, bad).is_none(),
                "{bad:?} как bg"
            );
            assert!(
                min_alpha_encoded(bad, ok).is_none(),
                "{bad:?} как solid (min_alpha)"
            );
            assert!(
                min_alpha_encoded(ok, bad).is_none(),
                "{bad:?} как bg (min_alpha)"
            );
        }
        assert!(invert_composite_encoded(ok, f64::NAN, ok).is_none());
    }

    /// Граница квантования из модульной документации подтверждается на
    /// наихудшем сдвиге: солид, смещённый на пол-кода (0.5/255), после
    /// инверсии отклоняет тинт ровно на 0.5/(255·α).
    #[test]
    fn quantisation_error_bound_is_honoured() {
        let tint = [0.4, 0.6, 0.2];
        let bg = [1.0, 1.0, 1.0];
        for alpha in [0.08, 0.2, 0.5] {
            let solid = composite_over_encoded_unchecked(tint, alpha, bg);
            let shifted = [solid[0] + 0.5 / 255.0, solid[1], solid[2]];
            let back = invert_composite_encoded(shifted, alpha, bg)
                .expect("сдвиг на пол-кода не выбивает из гамута на этих значениях");
            let err = (back[0] - tint[0]).abs();
            let bound = 0.5 / (255.0 * alpha);
            assert!(
                (err - bound).abs() < 1e-9,
                "α={alpha}: ошибка {err} != границе {bound}"
            );
        }
    }
}
