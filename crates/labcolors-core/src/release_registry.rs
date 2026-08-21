//! Minimal machine-readable registry for the F0 color-model releases.
//!
//! Registry descriptors contain only facts fixed by the implemented code:
//! context consumption, admitted frame/domain, coordinate units, achromatic
//! law, formula/reference identity and typed release dependencies.  The absence
//! of a difference calibration is an explicit keyless row, not a placeholder
//! calibration.  No empirical applicability, validation, uncertainty or
//! observer-study data is inferred here.

use crate::lcs_occurrence::{
    ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1, AdmittedSrgb8TristimulusBindingV1,
    AppearanceContextSchemaReleaseId, CAM16_UCS_VIEW_RELEASE_V1, CAM16_VIEW_RELEASE_V1,
    Cam16UcsViewReleaseId, Cam16ViewReleaseId, ColorimetricFrameId, IEC_SRGB_D65_XYZ_FRAME_V1,
    OKLAB_VIEW_RELEASE_V1, OklabViewReleaseId,
};
use crate::output_projection::{
    CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1, CssOklchHueSerializationReleaseIdV1,
    CssOklchNumberEncodingReleaseIdV1, DifferenceCalibrationReleaseIdV1, OKLCH_VIEW_RELEASE_V1,
    OklchViewReleaseId, OutputGamutTreatmentV1, OutputProjectionReleaseIdV1,
};

pub(crate) const RELEASE_REGISTRY_SCHEMA_VERSION_V1: u16 = 1;

// Registered rows append a fixed-width 15-byte descriptor after the release
// key. The byte order is documented by `RegisteredReleaseDescriptorV1` below.
const RELEASE_REGISTRY_CANONICAL_BYTES_V1: &[u8] = concat!(
    "labcolors.release-registry.canonical-binary.v1\0",
    "\0\x01\0\x06",
    "\x01\x01\0\x26cam16-li-et-al-2017-cie-248-forward-v1",
    "\x01\x02\x01\x01\x01\x02\x02\x02\0\0\0\0\0\0\0",
    "\x01\x01\0\x26cam16-ucs-li-et-al-2017-rectangular-v1",
    "\x01\x02\x01\x01\x04\x05\x05\x05\x03\x01\0\0\0\0\0",
    "\x01\x01\0\x24oklab-ottosson-2021-01-25-xyz-d65-v1",
    "\x01\x01\0\x01\x01\x01\x01\x01\0\0\0\0\0\0\0",
    "\x01\x01\0\x27polar-from-ottosson-2021-01-25-oklab-v1",
    "\x01\x01\0\x01\x02\x03\x03\x03\x01\0\x01\0\0\0\0",
    "\x02\0\0\0",
    "\x03\x01\0\x3acss-color-4-oklch-d65-from-modeled-iec61966-srgb8-solid-v1",
    "\x01\x03\0\x01\x03\x04\x04\x04\x02\x01\x01\x01\x01\x01\x01",
)
.as_bytes();

const RELEASE_REGISTRY_FNV1A32_V1: u32 = 540_606_852;

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
    Cam16UcsView(Cam16UcsViewReleaseId),
    OklabView(OklabViewReleaseId),
    OklchView(OklchViewReleaseId),
    CssColor4OklchD65FromModeledSrgb8Solid(OutputProjectionReleaseIdV1),
}

impl RegisteredColorReleaseIdV1 {
    pub(crate) const fn class(self) -> ReleaseRegistryClassV1 {
        match self {
            Self::Cam16View(_)
            | Self::Cam16UcsView(_)
            | Self::OklabView(_)
            | Self::OklchView(_) => ReleaseRegistryClassV1::AppearanceView,
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
            Self::Cam16UcsView(Cam16UcsViewReleaseId::LiEtAl2017Cam16UcsV1) => {
                "cam16-ucs-li-et-al-2017-rectangular-v1"
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

/// Whether and how a release consumes the occurrence's appearance context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ReleaseContextRequirementV1 {
    NoAppearanceContextConsumptionV1,
    ConsumesAppearanceContextV1(AppearanceContextSchemaReleaseId),
    RetainsOccurrenceContextWithoutGeometryConsumptionV1,
}

impl ReleaseContextRequirementV1 {
    pub(crate) const fn schema_release(self) -> Option<AppearanceContextSchemaReleaseId> {
        match self {
            Self::ConsumesAppearanceContextV1(schema) => Some(schema),
            Self::NoAppearanceContextConsumptionV1
            | Self::RetainsOccurrenceContextWithoutGeometryConsumptionV1 => None,
        }
    }

    const fn canonical_tag(self) -> u8 {
        match self {
            Self::NoAppearanceContextConsumptionV1 => 1,
            Self::ConsumesAppearanceContextV1(
                AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
            ) => 2,
            Self::RetainsOccurrenceContextWithoutGeometryConsumptionV1 => 3,
        }
    }

    const fn schema_tag(self) -> u8 {
        match self.schema_release() {
            None => 0,
            Some(AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1) => 1,
        }
    }
}

/// Exact colorimetric frame admitted by every current registered release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RegistryAdmittedFrameV1 {
    Cie1931TwoDegreeXyzIecD65RelativeY1V1,
}

impl RegistryAdmittedFrameV1 {
    pub(crate) const fn frame(self) -> ColorimetricFrameId {
        match self {
            Self::Cie1931TwoDegreeXyzIecD65RelativeY1V1 => IEC_SRGB_D65_XYZ_FRAME_V1,
        }
    }

    const fn canonical_tag(self) -> u8 {
        match self {
            Self::Cie1931TwoDegreeXyzIecD65RelativeY1V1 => 1,
        }
    }
}

/// Code-admitted input domain; this carries no empirical applicability claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RegistryAdmittedDomainV1 {
    FiniteNonNegativeXyzStimulusV1,
    FiniteOklabRectangularViewV1,
    ModeledIec61966Srgb8OccurrenceV1,
    FiniteCam16ViewV1,
}

impl RegistryAdmittedDomainV1 {
    const fn canonical_tag(self) -> u8 {
        match self {
            Self::FiniteNonNegativeXyzStimulusV1 => 1,
            Self::FiniteOklabRectangularViewV1 => 2,
            Self::ModeledIec61966Srgb8OccurrenceV1 => 3,
            Self::FiniteCam16ViewV1 => 4,
        }
    }
}

/// Coordinate/output units fixed by the registered code release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RegistryCoordinateUnitsV1 {
    OklabCoordinatesUnitlessV1,
    Cam16CorrelatesUnitlessHueDegreesV1,
    OklchCoordinatesUnitlessHueDegreesV1,
    CssColor4OklchPercentLightnessNumericChromaHueDegreesV1,
    Cam16UcsJPrimeAPrimeBPrimeUnitlessV1,
}

impl RegistryCoordinateUnitsV1 {
    const fn canonical_tag(self) -> u8 {
        match self {
            Self::OklabCoordinatesUnitlessV1 => 1,
            Self::Cam16CorrelatesUnitlessHueDegreesV1 => 2,
            Self::OklchCoordinatesUnitlessHueDegreesV1 => 3,
            Self::CssColor4OklchPercentLightnessNumericChromaHueDegreesV1 => 4,
            Self::Cam16UcsJPrimeAPrimeBPrimeUnitlessV1 => 5,
        }
    }
}

/// Exact hue/achromatic behavior implemented by a release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RegistryAchromaticLawV1 {
    NoHueCoordinateV1,
    HueUndefinedExactlyWhenCam16MIsZeroV1,
    HueUndefinedExactlyWhenOklabAAndBAreZeroV1,
    ExactSourceGreyOrRectangularOriginSerializesHueZeroV1,
    RectangularChromaOriginExactlyWhenCam16MIsZeroV1,
}

impl RegistryAchromaticLawV1 {
    const fn canonical_tag(self) -> u8 {
        match self {
            Self::NoHueCoordinateV1 => 1,
            Self::HueUndefinedExactlyWhenCam16MIsZeroV1 => 2,
            Self::HueUndefinedExactlyWhenOklabAAndBAreZeroV1 => 3,
            Self::ExactSourceGreyOrRectangularOriginSerializesHueZeroV1 => 4,
            Self::RectangularChromaOriginExactlyWhenCam16MIsZeroV1 => 5,
        }
    }
}

/// Formula/specification identity only, never empirical validation metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RegistryReferenceIdentityV1 {
    Ottosson20210125OklabXyzD65V1,
    LiEtAl2017Cie248Cam16ForwardV1,
    Ottosson20210125OklabPolarV1,
    CssColor4OklchD65V1,
    LiEtAl2017Cam16UcsCoordinatesV1,
}

impl RegistryReferenceIdentityV1 {
    const fn canonical_tag(self) -> u8 {
        match self {
            Self::Ottosson20210125OklabXyzD65V1 => 1,
            Self::LiEtAl2017Cie248Cam16ForwardV1 => 2,
            Self::Ottosson20210125OklabPolarV1 => 3,
            Self::CssColor4OklchD65V1 => 4,
            Self::LiEtAl2017Cam16UcsCoordinatesV1 => 5,
        }
    }
}

/// Every typed edge executed by the current CSS output projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct OutputProjectionDependencyGraphV1 {
    modeled_source_binding: AdmittedSrgb8TristimulusBindingV1,
    oklab_view: OklabViewReleaseId,
    oklch_view: OklchViewReleaseId,
    number_encoding: CssOklchNumberEncodingReleaseIdV1,
    hue_serialization: CssOklchHueSerializationReleaseIdV1,
    gamut_treatment: OutputGamutTreatmentV1,
}

impl OutputProjectionDependencyGraphV1 {
    const fn registered_v1() -> Self {
        Self {
            modeled_source_binding: ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1,
            oklab_view: OKLAB_VIEW_RELEASE_V1,
            oklch_view: OKLCH_VIEW_RELEASE_V1,
            number_encoding: CssOklchNumberEncodingReleaseIdV1::LPercent5C6Hue3V1,
            hue_serialization:
                CssOklchHueSerializationReleaseIdV1::ExactSourceGreyOrRectangularOriginToZeroV1,
            gamut_treatment: OutputGamutTreatmentV1::NoExplicitProjectionGamutMapV1,
        }
    }

    pub(crate) const fn modeled_source_binding(self) -> AdmittedSrgb8TristimulusBindingV1 {
        self.modeled_source_binding
    }

    pub(crate) const fn oklab_view(self) -> OklabViewReleaseId {
        self.oklab_view
    }

    pub(crate) const fn oklch_view(self) -> OklchViewReleaseId {
        self.oklch_view
    }

    pub(crate) const fn number_encoding(self) -> CssOklchNumberEncodingReleaseIdV1 {
        self.number_encoding
    }

    pub(crate) const fn hue_serialization(self) -> CssOklchHueSerializationReleaseIdV1 {
        self.hue_serialization
    }

    pub(crate) const fn gamut_treatment(self) -> OutputGamutTreatmentV1 {
        self.gamut_treatment
    }
}

/// Typed release-to-release prerequisites; no free-form dependency keys exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ReleaseDependencyGraphV1 {
    DirectV1,
    OklchPolarFromOklabV1(OklabViewReleaseId),
    Cam16UcsRectangularFromCam16V1(Cam16ViewReleaseId),
    CssColor4OklchD65V1(OutputProjectionDependencyGraphV1),
}

impl ReleaseDependencyGraphV1 {
    /// Fixed seven-byte tagged union. The graph tag determines its payload:
    ///
    /// - direct: six zero bytes;
    /// - Oklch: `[0, Oklab, 0, 0, 0, 0]`;
    /// - CSS output: `[source-binding, Oklab, Oklch, number, hue, gamut]`;
    /// - CAM16-UCS: `[CAM16, 0, 0, 0, 0, 0]`.
    const fn canonical_fields(self) -> [u8; 7] {
        match self {
            Self::DirectV1 => [0; 7],
            Self::OklchPolarFromOklabV1(OklabViewReleaseId::Ottosson20210125XyzD65V1) => {
                [1, 0, 1, 0, 0, 0, 0]
            }
            Self::Cam16UcsRectangularFromCam16V1(
                Cam16ViewReleaseId::LiEtAl2017Cie248ForwardV1,
            ) => [3, 1, 0, 0, 0, 0, 0],
            Self::CssColor4OklchD65V1(graph) => [
                2,
                match graph.modeled_source_binding {
                    AdmittedSrgb8TristimulusBindingV1::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1 => 1,
                },
                match graph.oklab_view {
                    OklabViewReleaseId::Ottosson20210125XyzD65V1 => 1,
                },
                match graph.oklch_view {
                    OklchViewReleaseId::PolarFromOttosson20210125OklabV1 => 1,
                },
                match graph.number_encoding {
                    CssOklchNumberEncodingReleaseIdV1::LPercent5C6Hue3V1 => 1,
                },
                match graph.hue_serialization {
                    CssOklchHueSerializationReleaseIdV1::ExactSourceGreyOrRectangularOriginToZeroV1 => 1,
                },
                match graph.gamut_treatment {
                    OutputGamutTreatmentV1::NoExplicitProjectionGamutMapV1 => 1,
                    OutputGamutTreatmentV1::HardClipWithOutOfGamutFlagV1 => 2,
                },
            ],
        }
    }
}

/// Complete code-truth descriptor for one registered release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RegisteredReleaseDescriptorV1 {
    release: RegisteredColorReleaseIdV1,
    context_requirement: ReleaseContextRequirementV1,
    admitted_frame: RegistryAdmittedFrameV1,
    admitted_domain: RegistryAdmittedDomainV1,
    coordinate_units: RegistryCoordinateUnitsV1,
    achromatic_law: RegistryAchromaticLawV1,
    reference_identity: RegistryReferenceIdentityV1,
    dependencies: ReleaseDependencyGraphV1,
}

impl RegisteredReleaseDescriptorV1 {
    pub(crate) const fn release(self) -> RegisteredColorReleaseIdV1 {
        self.release
    }

    pub(crate) const fn context_requirement(self) -> ReleaseContextRequirementV1 {
        self.context_requirement
    }

    pub(crate) const fn admitted_frame(self) -> RegistryAdmittedFrameV1 {
        self.admitted_frame
    }

    pub(crate) const fn admitted_domain(self) -> RegistryAdmittedDomainV1 {
        self.admitted_domain
    }

    pub(crate) const fn coordinate_units(self) -> RegistryCoordinateUnitsV1 {
        self.coordinate_units
    }

    pub(crate) const fn achromatic_law(self) -> RegistryAchromaticLawV1 {
        self.achromatic_law
    }

    pub(crate) const fn reference_identity(self) -> RegistryReferenceIdentityV1 {
        self.reference_identity
    }

    pub(crate) const fn dependencies(self) -> ReleaseDependencyGraphV1 {
        self.dependencies
    }

    /// Fixed-width descriptor bytes:
    ///
    /// `schema, context, context-schema, frame, domain, units, achromatic,
    /// reference, dependency-graph, source-binding, Oklab, Oklch, number,
    /// hue-policy, gamut-policy`.
    pub(crate) const fn canonical_fields(self) -> [u8; 15] {
        let dependencies = self.dependencies.canonical_fields();
        [
            1,
            self.context_requirement.canonical_tag(),
            self.context_requirement.schema_tag(),
            self.admitted_frame.canonical_tag(),
            self.admitted_domain.canonical_tag(),
            self.coordinate_units.canonical_tag(),
            self.achromatic_law.canonical_tag(),
            self.reference_identity.canonical_tag(),
            dependencies[0],
            dependencies[1],
            dependencies[2],
            dependencies[3],
            dependencies[4],
            dependencies[5],
            dependencies[6],
        ]
    }
}

const CAM16_VIEW_DESCRIPTOR_V1: RegisteredReleaseDescriptorV1 = RegisteredReleaseDescriptorV1 {
    release: RegisteredColorReleaseIdV1::Cam16View(CAM16_VIEW_RELEASE_V1),
    context_requirement: ReleaseContextRequirementV1::ConsumesAppearanceContextV1(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
    ),
    admitted_frame: RegistryAdmittedFrameV1::Cie1931TwoDegreeXyzIecD65RelativeY1V1,
    admitted_domain: RegistryAdmittedDomainV1::FiniteNonNegativeXyzStimulusV1,
    coordinate_units: RegistryCoordinateUnitsV1::Cam16CorrelatesUnitlessHueDegreesV1,
    achromatic_law: RegistryAchromaticLawV1::HueUndefinedExactlyWhenCam16MIsZeroV1,
    reference_identity: RegistryReferenceIdentityV1::LiEtAl2017Cie248Cam16ForwardV1,
    dependencies: ReleaseDependencyGraphV1::DirectV1,
};

const CAM16_UCS_VIEW_DESCRIPTOR_V1: RegisteredReleaseDescriptorV1 = RegisteredReleaseDescriptorV1 {
    release: RegisteredColorReleaseIdV1::Cam16UcsView(CAM16_UCS_VIEW_RELEASE_V1),
    context_requirement: ReleaseContextRequirementV1::ConsumesAppearanceContextV1(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
    ),
    admitted_frame: RegistryAdmittedFrameV1::Cie1931TwoDegreeXyzIecD65RelativeY1V1,
    admitted_domain: RegistryAdmittedDomainV1::FiniteCam16ViewV1,
    coordinate_units: RegistryCoordinateUnitsV1::Cam16UcsJPrimeAPrimeBPrimeUnitlessV1,
    achromatic_law: RegistryAchromaticLawV1::RectangularChromaOriginExactlyWhenCam16MIsZeroV1,
    reference_identity: RegistryReferenceIdentityV1::LiEtAl2017Cam16UcsCoordinatesV1,
    dependencies: ReleaseDependencyGraphV1::Cam16UcsRectangularFromCam16V1(CAM16_VIEW_RELEASE_V1),
};

const OKLAB_VIEW_DESCRIPTOR_V1: RegisteredReleaseDescriptorV1 = RegisteredReleaseDescriptorV1 {
    release: RegisteredColorReleaseIdV1::OklabView(OKLAB_VIEW_RELEASE_V1),
    context_requirement: ReleaseContextRequirementV1::NoAppearanceContextConsumptionV1,
    admitted_frame: RegistryAdmittedFrameV1::Cie1931TwoDegreeXyzIecD65RelativeY1V1,
    admitted_domain: RegistryAdmittedDomainV1::FiniteNonNegativeXyzStimulusV1,
    coordinate_units: RegistryCoordinateUnitsV1::OklabCoordinatesUnitlessV1,
    achromatic_law: RegistryAchromaticLawV1::NoHueCoordinateV1,
    reference_identity: RegistryReferenceIdentityV1::Ottosson20210125OklabXyzD65V1,
    dependencies: ReleaseDependencyGraphV1::DirectV1,
};

const OKLCH_VIEW_DESCRIPTOR_V1: RegisteredReleaseDescriptorV1 = RegisteredReleaseDescriptorV1 {
    release: RegisteredColorReleaseIdV1::OklchView(OKLCH_VIEW_RELEASE_V1),
    context_requirement: ReleaseContextRequirementV1::NoAppearanceContextConsumptionV1,
    admitted_frame: RegistryAdmittedFrameV1::Cie1931TwoDegreeXyzIecD65RelativeY1V1,
    admitted_domain: RegistryAdmittedDomainV1::FiniteOklabRectangularViewV1,
    coordinate_units: RegistryCoordinateUnitsV1::OklchCoordinatesUnitlessHueDegreesV1,
    achromatic_law: RegistryAchromaticLawV1::HueUndefinedExactlyWhenOklabAAndBAreZeroV1,
    reference_identity: RegistryReferenceIdentityV1::Ottosson20210125OklabPolarV1,
    dependencies: ReleaseDependencyGraphV1::OklchPolarFromOklabV1(OKLAB_VIEW_RELEASE_V1),
};

const OUTPUT_PROJECTION_DESCRIPTOR_V1: RegisteredReleaseDescriptorV1 =
    RegisteredReleaseDescriptorV1 {
        release: RegisteredColorReleaseIdV1::CssColor4OklchD65FromModeledSrgb8Solid(
            CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1,
        ),
        context_requirement:
            ReleaseContextRequirementV1::RetainsOccurrenceContextWithoutGeometryConsumptionV1,
        admitted_frame: RegistryAdmittedFrameV1::Cie1931TwoDegreeXyzIecD65RelativeY1V1,
        admitted_domain: RegistryAdmittedDomainV1::ModeledIec61966Srgb8OccurrenceV1,
        coordinate_units:
            RegistryCoordinateUnitsV1::CssColor4OklchPercentLightnessNumericChromaHueDegreesV1,
        achromatic_law:
            RegistryAchromaticLawV1::ExactSourceGreyOrRectangularOriginSerializesHueZeroV1,
        reference_identity: RegistryReferenceIdentityV1::CssColor4OklchD65V1,
        dependencies: ReleaseDependencyGraphV1::CssColor4OklchD65V1(
            OutputProjectionDependencyGraphV1::registered_v1(),
        ),
    };

/// One closed registry row.
///
/// The unavailable variant carries no descriptor or release identifier, so it
/// cannot be mistaken for an admitted difference formula.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ReleaseRegistryRecordV1 {
    Registered(RegisteredReleaseDescriptorV1),
    DifferenceCalibrationUnavailable,
}

impl ReleaseRegistryRecordV1 {
    pub(crate) const fn class(self) -> ReleaseRegistryClassV1 {
        match self {
            Self::Registered(descriptor) => descriptor.release().class(),
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
            Self::Registered(descriptor) => Some(descriptor.release()),
            Self::DifferenceCalibrationUnavailable => None,
        }
    }

    pub(crate) const fn descriptor(self) -> Option<RegisteredReleaseDescriptorV1> {
        match self {
            Self::Registered(descriptor) => Some(descriptor),
            Self::DifferenceCalibrationUnavailable => None,
        }
    }
}

// Canonical order is class tag, then release-key bytes. An unavailable class
// has exactly one keyless and descriptor-less row.
const RELEASE_REGISTRY_RECORDS_V1: [ReleaseRegistryRecordV1; 6] = [
    ReleaseRegistryRecordV1::Registered(CAM16_VIEW_DESCRIPTOR_V1),
    ReleaseRegistryRecordV1::Registered(CAM16_UCS_VIEW_DESCRIPTOR_V1),
    ReleaseRegistryRecordV1::Registered(OKLAB_VIEW_DESCRIPTOR_V1),
    ReleaseRegistryRecordV1::Registered(OKLCH_VIEW_DESCRIPTOR_V1),
    ReleaseRegistryRecordV1::DifferenceCalibrationUnavailable,
    ReleaseRegistryRecordV1::Registered(OUTPUT_PROJECTION_DESCRIPTOR_V1),
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
/// declared canonical order. Every row starts with
/// `class:u8 || availability:u8 || key_length:u16be || key:utf8`. A registered
/// row then carries the 15 descriptor bytes documented by
/// [`RegisteredReleaseDescriptorV1::canonical_fields`]. The unavailable
/// difference row has a zero key length and no descriptor bytes.
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

pub(crate) const fn release_registry_digest_v1() -> ReleaseRegistryDigestV1 {
    ReleaseRegistryDigestV1 {
        algorithm: ReleaseRegistryDigestAlgorithmV1::Fnv1a32V1,
        value: RELEASE_REGISTRY_FNV1A32_V1,
    }
}
