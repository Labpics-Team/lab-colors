//! Полный машинный домен выбора, не доказательство человеческой чистоты.
use super::*;

#[kani::proof]
#[kani::unwind(33)]
fn selection_admits_exactly_declared_modeled_point_release() {
    let release: [u8; 32] = kani::any();
    let scope = match kani::any::<u8>() {
        0 => CleanConventionScopeV1::ModeledSrgb8Point,
        1 => CleanConventionScopeV1::WholeField,
        _ => CleanConventionScopeV1::ObservedPoint,
    };
    let admission = match kani::any::<u8>() {
        0 => CleanConventionAdmissionKindV1::DeclaredPackagePolicyCandidate,
        1 => CleanConventionAdmissionKindV1::ProductionAuto,
        _ => CleanConventionAdmissionKindV1::HumanAction,
    };
    let exact_release = release == EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2;
    let exact_scope = scope == CleanConventionScopeV1::ModeledSrgb8Point;
    let exact_admission =
        admission == CleanConventionAdmissionKindV1::DeclaredPackagePolicyCandidate;
    let result = CleanConventionSelectionV1::select(release, scope, admission);
    assert!(
        result.is_ok() == (exact_release && exact_scope && exact_admission),
        "CC selection must admit exactly the declared modeled point release"
    );
    if let Ok(selection) = result {
        assert!(
            selection.release() == release
                && selection.scope() == scope
                && selection.admission() == admission,
            "CC selection must preserve release scope and admission"
        );
    }
    kani::cover!(result.is_ok(), "declared point remains reachable");
    kani::cover!(
        matches!(result, Err(CleanConventionErrorV1::UnsupportedRelease)),
        "foreign release rejected"
    );
    kani::cover!(
        matches!(result, Err(CleanConventionErrorV1::UnsupportedScope)),
        "unsupported scope rejected"
    );
    kani::cover!(
        matches!(result, Err(CleanConventionErrorV1::UnsupportedAdmission)),
        "unearned admission rejected"
    );
}
