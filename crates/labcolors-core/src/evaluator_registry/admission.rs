//! Admission gate for external readability profiles (APCA, future models).
//! No external profile enters the registry without passing this gate.
//!
//! DESIGN NOTE: This module is dependency-free. Hash verification accepts
//! pre-computed digests rather than computing them internally, preserving
//! core's zero-dependency invariant. The actual SHA-256 computation happens
//! at the call site (labcolors-wasm or test harness).
//!
//! WIRE NOTE: This type has no serde derives. If external profile descriptors
//! need wire serialization, add a mirror type in labcolors-wasm.

use super::metadata::EvaluatorApplicabilityV1;

/// Descriptor for an external readability profile seeking admission.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ExternalReadabilityProfileV1 {
    /// Canonical name (e.g., "APCA-0.0.98G").
    pub(crate) source: &'static str,
    /// SPDX license identifier of the profile specification.
    pub(crate) license_spdx: &'static str,
    /// Pre-computed SHA-256 digest of the canonical specification document bytes.
    /// Core does not compute hashes; the caller provides the verified digest.
    pub(crate) validation_hash: [u8; 32],
    /// Declared applicability domain.
    pub(crate) applicability: EvaluatorApplicabilityV1,
}

/// Outcome of the admission gate evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdmissionVerdict {
    /// Profile passed all checks and may be registered.
    Admitted,
    /// Profile failed one or more checks.
    Rejected(AdmissionRejection),
}

/// Specific reason for admission rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdmissionRejection {
    /// License is not in the admitted allowlist.
    LicenseNotAllowed { spdx: &'static str },
    /// Pre-computed hash does not match the expected digest.
    HashMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    /// Applicability domain is undeclared.
    ApplicabilityIncomplete,
    /// Source name is already registered in the session.
    SourceAlreadyRegistered,
}

/// Allowlist of SPDX identifiers permitted for external profiles.
const ADMITTED_LICENSES: &[&str] = &["MIT", "Apache-2.0", "CC-BY-4.0", "W3C"];

/// Evaluates whether an external readability profile should be admitted.
///
/// Checks are evaluated in priority order:
/// 1. License allowlist (rejects GPL, proprietary, etc.)
/// 2. Hash verification against caller-provided expected digest
/// 3. Applicability completeness (rejects "undeclared" domains)
/// 4. Duplicate source detection
///
/// The `expected_spec_hash` parameter is the authoritative digest that the
/// caller has independently verified. The profile's `validation_hash` is
/// compared against it. This keeps core free of cryptographic dependencies.
pub(crate) fn admit_external_profile(
    profile: &ExternalReadabilityProfileV1,
    expected_spec_hash: &[u8; 32],
    registered_sources: &[&str],
) -> AdmissionVerdict {
    if !ADMITTED_LICENSES.contains(&profile.license_spdx) {
        return AdmissionVerdict::Rejected(AdmissionRejection::LicenseNotAllowed {
            spdx: profile.license_spdx,
        });
    }

    if profile.validation_hash != *expected_spec_hash {
        return AdmissionVerdict::Rejected(AdmissionRejection::HashMismatch {
            expected: *expected_spec_hash,
            actual: profile.validation_hash,
        });
    }

    if profile.applicability.domain_description == "undeclared" {
        return AdmissionVerdict::Rejected(AdmissionRejection::ApplicabilityIncomplete);
    }

    if registered_sources.contains(&profile.source) {
        return AdmissionVerdict::Rejected(AdmissionRejection::SourceAlreadyRegistered);
    }

    AdmissionVerdict::Admitted
}

/// Container for admitted evaluators within a session scope.
/// External evaluators are stored here and invoked through a separate API,
/// never through core static dispatch match arms.
#[derive(Debug, Clone)]
pub(crate) struct EvaluatorRegistryV1 {
    admitted_sources: Vec<&'static str>,
}

impl EvaluatorRegistryV1 {
    /// Creates an empty registry.
    pub(crate) fn new() -> Self {
        Self {
            admitted_sources: Vec::new(),
        }
    }

    /// Returns the list of currently admitted source names.
    pub(crate) fn registered_sources(&self) -> &[&str] {
        &self.admitted_sources
    }

    /// Attempts to admit a profile. On success, records the source name.
    pub(crate) fn try_admit(
        &mut self,
        profile: &ExternalReadabilityProfileV1,
        expected_spec_hash: &[u8; 32],
    ) -> AdmissionVerdict {
        let verdict = admit_external_profile(profile, expected_spec_hash, &self.admitted_sources);
        if verdict == AdmissionVerdict::Admitted {
            self.admitted_sources.push(profile.source);
        }
        verdict
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_profile(source: &'static str) -> ExternalReadabilityProfileV1 {
        ExternalReadabilityProfileV1 {
            source,
            license_spdx: "MIT",
            validation_hash: [0xAA; 32],
            applicability: EvaluatorApplicabilityV1 {
                domain_description: "test domain",
                validation_sample_size: Some(1000),
                confidence_interval_95: Some(0.05),
            },
        }
    }

    #[test]
    fn admission_rejects_gpl_license() {
        let profile = ExternalReadabilityProfileV1 {
            license_spdx: "GPL-3.0",
            ..valid_profile("test-gpl")
        };
        let verdict = admit_external_profile(&profile, &[0xAA; 32], &[]);
        assert_eq!(
            verdict,
            AdmissionVerdict::Rejected(AdmissionRejection::LicenseNotAllowed { spdx: "GPL-3.0" }),
        );
    }

    #[test]
    fn admission_rejects_undeclared_applicability() {
        let profile = ExternalReadabilityProfileV1 {
            applicability: EvaluatorApplicabilityV1::undeclared(),
            ..valid_profile("test-undeclared")
        };
        let verdict = admit_external_profile(&profile, &[0xAA; 32], &[]);
        assert_eq!(
            verdict,
            AdmissionVerdict::Rejected(AdmissionRejection::ApplicabilityIncomplete),
        );
    }

    #[test]
    fn admission_rejects_duplicate_source() {
        let profile = valid_profile("APCA-0.0.98G");
        let verdict = admit_external_profile(&profile, &[0xAA; 32], &["APCA-0.0.98G"]);
        assert_eq!(
            verdict,
            AdmissionVerdict::Rejected(AdmissionRejection::SourceAlreadyRegistered),
        );
    }

    #[test]
    fn admission_rejects_hash_mismatch() {
        let profile = valid_profile("test-hash");
        let verdict = admit_external_profile(&profile, &[0xBB; 32], &[]);
        assert_eq!(
            verdict,
            AdmissionVerdict::Rejected(AdmissionRejection::HashMismatch {
                expected: [0xBB; 32],
                actual: [0xAA; 32],
            }),
        );
    }

    #[test]
    fn admission_admits_valid_profile() {
        let profile = valid_profile("APCA-0.0.98G-valid");
        let verdict = admit_external_profile(&profile, &[0xAA; 32], &[]);
        assert_eq!(verdict, AdmissionVerdict::Admitted);
    }

    #[test]
    fn registry_tracks_admitted_sources() {
        let mut registry = EvaluatorRegistryV1::new();
        let profile = valid_profile("test-source");
        let verdict = registry.try_admit(&profile, &[0xAA; 32]);
        assert_eq!(verdict, AdmissionVerdict::Admitted);
        assert_eq!(registry.registered_sources(), &["test-source"]);
    }

    #[test]
    fn registry_rejects_duplicate_via_try_admit() {
        let mut registry = EvaluatorRegistryV1::new();
        let profile = valid_profile("dup-source");
        assert_eq!(
            registry.try_admit(&profile, &[0xAA; 32]),
            AdmissionVerdict::Admitted
        );
        assert_eq!(
            registry.try_admit(&profile, &[0xAA; 32]),
            AdmissionVerdict::Rejected(AdmissionRejection::SourceAlreadyRegistered),
        );
    }
}
