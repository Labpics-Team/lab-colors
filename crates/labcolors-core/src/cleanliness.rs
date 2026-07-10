//! Замороженная совместимая поверхность исторического «Закона Грязи V1».
//!
//! Модуль сохраняет только старые функции и константы с прежней семантикой для
//! воспроизводимости conformance-векторов. Новый код не должен использовать их
//! как универсальную шкалу качества, человеческий verdict или основание для
//! автоматического изменения цвета.
//!
//! Screen-native анализ без `Clean/Dirty` находится в [`crate::color_quality`].

#[path = "cleanliness_legacy.rs"]
mod legacy_v1;

pub use legacy_v1::{
    B0, BW, C0, DefectContext, H_Y_DEG, JND, Theme, b_of, cusp_l_of, depth_mod, depth_term, drab,
    drab_in_context, hue_weight, muddiness_from_hex, muddiness_from_linear_srgb,
    muddiness_in_context, muddiness_oklch, n_pure, neutral_gate, raw_chromatic,
};
