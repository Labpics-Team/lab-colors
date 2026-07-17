//! Общие fail-closed helpers in-crate тестов.
//!
//! Production-адаптеры имеют собственные типизированные границы. Этот модуль
//! существует только под `cfg(test)`, чтобы golden/property тесты не стирали
//! разные причины терминального отказа в один правдоподобный sentinel.

use crate::RoleFailure;

/// Stable representation of an already-admitted role failure. Admission lives
/// in production; tests only format the typed category and core-owned code.
pub(crate) fn role_failure_repr(failure: &RoleFailure) -> String {
    format!(
        "FAILURE({},{})",
        failure.category().as_str(),
        failure.code()
    )
}
