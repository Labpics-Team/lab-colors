//! Общие fail-closed helpers in-crate тестов.
//!
//! Production-адаптеры имеют собственные типизированные границы. Этот модуль
//! существует только под `cfg(test)`, чтобы golden/property тесты не стирали
//! разные причины терминального отказа в один правдоподобный sentinel.

use crate::{SolveFailure, SolveFailureCategory};

/// Точная стабильная форма публичного failure; внутренний инвариант не имеет
/// публичной формы и обязан уронить тест.
pub(crate) fn failure_repr(failure: &SolveFailure) -> String {
    let boundary = failure
        .boundary()
        .unwrap_or_else(|| panic!("internal solve invariant leaked into test output: {failure}"));
    format!(
        "FAILURE({},{})",
        boundary.category().as_str(),
        boundary.code()
    )
}

/// Проверить допустимый отказ валидного sRGB `resolve_set`/`resolve_named_set`.
/// `unreachable` доказывает отсутствие решения, а `unresolved` честно сообщает
/// лишь об исчерпании объявленного поиска; `unsupported` здесь означает дрейф
/// фиксированной capability, но остаётся честным исходом общего `solve` API.
pub(crate) fn valid_srgb_set_failure_repr(failure: &SolveFailure) -> String {
    let boundary = failure
        .boundary()
        .unwrap_or_else(|| panic!("internal solve invariant leaked from valid resolve: {failure}"));
    assert!(
        matches!(
            boundary.category(),
            SolveFailureCategory::Unreachable | SolveFailureCategory::Unresolved
        ),
        "valid sRGB set resolve returned {}/{}: {failure}",
        boundary.category().as_str(),
        boundary.code()
    );
    failure_repr(failure)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_projection_preserves_public_code_and_rejects_internal_drift() {
        let below = SolveFailure::BelowContrastFloor { target: 1.0 };
        let floor = SolveFailure::FloorUnreachable {
            floor: 4.5,
            max_ratio: 3.0,
        };
        assert_eq!(
            failure_repr(&below),
            "FAILURE(unreachable,below_contrast_floor)"
        );
        assert_eq!(
            failure_repr(&floor),
            "FAILURE(unreachable,floor_unreachable)"
        );
        assert_ne!(failure_repr(&below), failure_repr(&floor));

        let internal = SolveFailure::InternalInvariant("test drift".to_string());
        assert!(std::panic::catch_unwind(|| failure_repr(&internal)).is_err());
    }

    #[test]
    fn valid_srgb_set_projection_accepts_only_unreachable_or_unresolved() {
        let unresolved = SolveFailure::BoundedSearchExhausted {
            target: 50.0,
            closest_examined: 48.0,
        };
        assert_eq!(
            valid_srgb_set_failure_repr(&unresolved),
            "FAILURE(unresolved,bounded_search_exhausted)"
        );

        for invalid in [
            SolveFailure::InvalidInput("bad".to_string()),
            SolveFailure::GamutUnsupported,
            SolveFailure::InternalInvariant("drift".to_string()),
        ] {
            assert!(
                std::panic::catch_unwind(|| valid_srgb_set_failure_repr(&invalid)).is_err(),
                "valid sRGB set resolve must reject {invalid:?}"
            );
        }
    }
}
