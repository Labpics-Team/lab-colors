//! Общие helpers тестовой конфигурационной поверхности.
//!
//! Модуль существует только под `cfg(test)`: строковая форма ниже — оракул
//! characterization/golden-тестов, а не второй публичный формат эмиссии.

use crate::semantic::Resolved;

/// Каноническая строковая форма решённой роли для in-crate golden-гейтов.
pub(crate) fn resolved_repr(res: &Resolved) -> String {
    match res {
        Resolved::Color { solved, .. } => solved.hex().to_string(),
        Resolved::Translucent(r) => format!("rgba({},{})", r.tint_hex(), r.alpha()),
        Resolved::Glow(g) => format!("glow({},{},{:.4})", g.core_hex(), g.halo_hex(), g.alpha()),
        Resolved::GlowIndeterminate(_) => "GLOW_INDETERMINATE".to_string(),
        Resolved::Material(m) => format!("material({},{:.4})", m.tint_hex(), m.alpha()),
        Resolved::None => "none".to_string(),
        Resolved::Failure(failure) => crate::test_support::role_failure_repr(failure),
    }
}
