//! Minimal machine-readable registry for the F0 color-model releases.
//!
//! Registry rows name only code releases that already exist.  The absence of a
//! difference calibration is an explicit row, not a placeholder calibration.
//! No applicability, empirical validation, uncertainty or observer-study data
//! is inferred here.

use crate::lcs_occurrence::{
    CAM16_VIEW_RELEASE_V1, Cam16ViewReleaseId, OKLAB_VIEW_RELEASE_V1, OklabViewReleaseId,
};
use crate::output_projection::{
    CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1, DifferenceCalibrationReleaseIdV1,
    OKLCH_VIEW_RELEASE_V1, OklchViewReleaseId, OutputProjectionReleaseIdV1,
};

pub(crate) const RELEASE_REGISTRY_SCHEMA_VERSION_V1: u16 = 1;

const RELEASE_REGISTRY_CANONICAL_BYTES_V1: &[u8] = concat!(
    "labcolors.release-registry.canonical-binary.v1\0",
    "\0\x01\0\x05",
    "\x01\x01\0\x26cam16-li-et-al-2017-cie-248-forward-v1",
    "\x01\x01\0\x24oklab-ottosson-2021-01-25-xyz-d65-v1",
    "\x01\x01\0\x27polar-from-ottosson-2021-01-25-oklab-v1",
    "\x02\0\0\0",
    "\x03\x01\0\x3acss-color-4-oklch-d65-from-modeled-iec61966-srgb8-solid-v1",
)
.as_bytes();

const RELEASE_REGISTRY_FNV1A32_V1: u32 = 3_103_457_152;

/// The disjoint kinds whose availability is reported by this registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ReleaseRegistryClassV1 {
    AppearanceView,
    DifferenceCalibration,
    OutputProjection,
}

impl ReleaseRegistryClassV1 {
    pub(crate) const fn canonical_tag(self) -> u8 {
        match self {
            Self::AppearanceView => 1,
            Self::DifferenceCalibration => 2,
            Self::OutputProjection => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ReleaseRegistryAvailabilityV1 {
    Unavailable,
    Registered,
}

impl ReleaseRegistryAvailabilityV1 {
    pub(crate) const fn canonical_tag(self) -> u8 {
        match self {
            Self::Unavailable => 0,
            Self::Registered => 1,
        }
    }
}

/// A nominal link to one release implemented by the current F0 code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RegisteredColorReleaseIdV1 {
    Cam16View(Cam16ViewReleaseId),
    OklabView(OklabViewReleaseId),
    OklchView(OklchViewReleaseId),
    CssColor4OklchD65FromModeledSrgb8Solid(OutputProjectionReleaseIdV1),
}

impl RegisteredColorReleaseIdV1 {
    pub(crate) const fn class(self) -> ReleaseRegistryClassV1 {
        match self {
            Self::Cam16View(_) | Self::OklabView(_) | Self::OklchView(_) => {
                ReleaseRegistryClassV1::AppearanceView
            }
            Self::CssColor4OklchD65FromModeledSrgb8Solid(_) => {
                ReleaseRegistryClassV1::OutputProjection
            }
        }
    }

    /// Stable ASCII key for the exact formula/operation-order release.
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::Cam16View(Cam16ViewReleaseId::LiEtAl2017Cie248ForwardV1) => {
                "cam16-li-et-al-2017-cie-248-forward-v1"
            }
            Self::OklabView(OklabViewReleaseId::Ottosson20210125XyzD65V1) => {
                "oklab-ottosson-2021-01-25-xyz-d65-v1"
            }
            Self::OklchView(OklchViewReleaseId::PolarFromOttosson20210125OklabV1) => {
                "polar-from-ottosson-2021-01-25-oklab-v1"
            }
            Self::CssColor4OklchD65FromModeledSrgb8Solid(release) => release.key(),
        }
    }
}

/// One closed registry row.
///
/// The unavailable variant carries no release identifier, so it cannot be
/// mistaken for an admitted difference formula.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ReleaseRegistryRecordV1 {
    Registered(RegisteredColorReleaseIdV1),
    DifferenceCalibrationUnavailable,
}

impl ReleaseRegistryRecordV1 {
    pub(crate) const fn class(self) -> ReleaseRegistryClassV1 {
        match self {
            Self::Registered(release) => release.class(),
            Self::DifferenceCalibrationUnavailable => ReleaseRegistryClassV1::DifferenceCalibration,
        }
    }

    pub(crate) const fn availability(self) -> ReleaseRegistryAvailabilityV1 {
        match self {
            Self::Registered(_) => ReleaseRegistryAvailabilityV1::Registered,
            Self::DifferenceCalibrationUnavailable => ReleaseRegistryAvailabilityV1::Unavailable,
        }
    }

    pub(crate) const fn release(self) -> Option<RegisteredColorReleaseIdV1> {
        match self {
            Self::Registered(release) => Some(release),
            Self::DifferenceCalibrationUnavailable => None,
        }
    }
}

// Canonical order is class tag, then release-key bytes.  An unavailable class
// has exactly one keyless row.
const RELEASE_REGISTRY_RECORDS_V1: [ReleaseRegistryRecordV1; 5] = [
    ReleaseRegistryRecordV1::Registered(RegisteredColorReleaseIdV1::Cam16View(
        CAM16_VIEW_RELEASE_V1,
    )),
    ReleaseRegistryRecordV1::Registered(RegisteredColorReleaseIdV1::OklabView(
        OKLAB_VIEW_RELEASE_V1,
    )),
    ReleaseRegistryRecordV1::Registered(RegisteredColorReleaseIdV1::OklchView(
        OKLCH_VIEW_RELEASE_V1,
    )),
    ReleaseRegistryRecordV1::DifferenceCalibrationUnavailable,
    ReleaseRegistryRecordV1::Registered(
        RegisteredColorReleaseIdV1::CssColor4OklchD65FromModeledSrgb8Solid(
            CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1,
        ),
    ),
];

pub(crate) const fn release_registry_records_v1() -> &'static [ReleaseRegistryRecordV1] {
    &RELEASE_REGISTRY_RECORDS_V1
}

/// The existing difference-release type is uninhabited in this registry.
pub(crate) const fn impossible_difference_calibration_release_v1(
    release: DifferenceCalibrationReleaseIdV1,
) -> ! {
    match release {}
}

/// Canonical binary encoding of the complete registry.
///
/// Layout is `magic || schema:u16be || rows:u16be`, followed by rows in the
/// declared canonical order.  Every row is
/// `class:u8 || availability:u8 || key_length:u16be || key:utf8`; unavailable
/// difference calibration has a zero key length.
pub(crate) const fn release_registry_canonical_bytes_v1() -> &'static [u8] {
    RELEASE_REGISTRY_CANONICAL_BYTES_V1
}

/// Explicit algorithm identity for the registry's non-cryptographic drift
/// sentinel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ReleaseRegistryDigestAlgorithmV1 {
    Fnv1a32V1,
}

impl ReleaseRegistryDigestAlgorithmV1 {
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::Fnv1a32V1 => "fnv1a-32-v1",
        }
    }
}

/// A deterministic drift sentinel, not cryptographic evidence or an
/// authenticity/content-identity claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ReleaseRegistryDigestV1 {
    algorithm: ReleaseRegistryDigestAlgorithmV1,
    value: u32,
}

impl ReleaseRegistryDigestV1 {
    pub(crate) const fn algorithm(self) -> ReleaseRegistryDigestAlgorithmV1 {
        self.algorithm
    }

    pub(crate) const fn value(self) -> u32 {
        self.value
    }
}

pub(crate) fn release_registry_digest_v1() -> ReleaseRegistryDigestV1 {
    ReleaseRegistryDigestV1 {
        algorithm: ReleaseRegistryDigestAlgorithmV1::Fnv1a32V1,
        value: RELEASE_REGISTRY_FNV1A32_V1,
    }
}
