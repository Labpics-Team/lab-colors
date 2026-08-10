#![cfg(any(test, target_arch = "wasm32"))]

//! Закрытый production consumer минимального declarative Program.
//!
//! Rust-модуль намеренно приватен. Единственная внешняя поверхность существует
//! только в отдельном single-threaded `wasm32` artifact как фиксированные ABI-v1
//! request/result buffers и синхронный `run`; общий Program authoring API наружу
//! не проецируется.

use crate::Srgb8;
use crate::family_artifact::FamilyArtifactBundleV2;
use crate::program::attachment::handoff::{
    HandoffAttachmentV1, HandoffPointSinkHostErrorV1, HandoffPointSinkHostIntentV1,
    HandoffPointSinkHostV1, HandoffPointSinkOutputIdV1, handoff_point_sink,
};
use crate::program::attachment::{
    AuthoredPointEmissionBindingV1, AuthoredPointPresentationBindingV1, ExternalDisposeErrorV1,
};
use crate::program::{
    AppearanceContextV1, ConstraintIdV1, DraftV1, FinitePaintDomainV1, OccurrenceIdV1,
    OpacityInputIdV1, OutputSlotIdV1, PaintIdV1, PaintValueV1, PresentationRootIdV1, ScenarioV1,
    SourceIdV1, SurfaceIdV1, SurfaceInputPortIdV1, SurroundV1, TargetCandidateIdV1,
    TargetCandidateV1, TargetIdV1, UpdateV1,
};
use crate::program_session::{
    JointCandidateStateV1, TargetCandidateChoiceV1, TargetCandidateId as CoreTargetCandidateId,
    TargetId as CoreTargetId,
};
use crate::relation::DirectedRelationV1;
use crate::selection_release::{
    SelectionCandidateKeyV1, SelectionReleaseV1, admit_selection_release_v1,
    materialise_joint_selection_v1,
};

const BYTE_WIDTH: usize = 1;
const U16_WIDTH: usize = 2;
const U32_WIDTH: usize = 4;
const U64_WIDTH: usize = 8;
const F64_WIDTH: usize = U64_WIDTH;
const RGB_WIDTH: usize = 3;
const IDENTITY_WIDTH: usize = 32;
const MAGIC_WIDTH: usize = 4;
const HEADER_WIDTH: usize = MAGIC_WIDTH + U16_WIDTH + U16_WIDTH;
const FILL_CANDIDATE_COUNT: usize = 3;
const LABEL_CANDIDATE_COUNT: usize = 2;
const JOINT_STATE_COUNT: usize = FILL_CANDIDATE_COUNT * LABEL_CANDIDATE_COUNT;
const SCENARIO_COUNT: usize = 2;
const SELECTION_KEY_WIDTH: usize = 16;
const TARGET_CANDIDATE_WIDTH: usize = U32_WIDTH + RGB_WIDTH + U64_WIDTH;
const JOINT_STATE_WIDTH: usize = SELECTION_KEY_WIDTH + U32_WIDTH + U32_WIDTH;
const APPEARANCE_WIDTH: usize = F64_WIDTH + F64_WIDTH + BYTE_WIDTH;
const SCENARIO_WIDTH: usize = U32_WIDTH + RGB_WIDTH;

const PRIVATE_FIXTURE_REQUEST_V1_LEN: usize = HEADER_WIDTH
    + RGB_WIDTH
    + FILL_CANDIDATE_COUNT * TARGET_CANDIDATE_WIDTH
    + LABEL_CANDIDATE_COUNT * TARGET_CANDIDATE_WIDTH
    + JOINT_STATE_COUNT * JOINT_STATE_WIDTH
    + U64_WIDTH
    + F64_WIDTH
    + APPEARANCE_WIDTH
    + SCENARIO_COUNT * SCENARIO_WIDTH
    + RGB_WIDTH
    + U32_WIDTH
    + U64_WIDTH
    + U32_WIDTH;
const PRIVATE_FIXTURE_RESULT_V1_LEN: usize = HEADER_WIDTH
    + U32_WIDTH
    + U32_WIDTH
    + U32_WIDTH
    + RGB_WIDTH
    + U64_WIDTH
    + IDENTITY_WIDTH
    + IDENTITY_WIDTH;
const _: () = assert!(PRIVATE_FIXTURE_REQUEST_V1_LEN <= u16::MAX as usize);
const _: () = assert!(PRIVATE_FIXTURE_RESULT_V1_LEN <= u16::MAX as usize);

const PRIVATE_FIXTURE_REQUEST_V1_MAGIC: [u8; MAGIC_WIDTH] = *b"LCFQ";
const PRIVATE_FIXTURE_RESULT_V1_MAGIC: [u8; MAGIC_WIDTH] = *b"LCFR";
const PRIVATE_FIXTURE_ABI_VERSION_V1: u16 = 1;

const BRAND_SOURCE: SourceIdV1 = SourceIdV1::new(1);
const BRAND_REFERENCE_TARGET: TargetIdV1 = TargetIdV1::new(2);
const FILL_TARGET: TargetIdV1 = TargetIdV1::new(3);
const LABEL_TARGET: TargetIdV1 = TargetIdV1::new(4);
const FILL_PAINT: PaintIdV1 = PaintIdV1::new(5);
const LABEL_SOLID_PAINT: PaintIdV1 = PaintIdV1::new(6);
const LABEL_OPACITY_PAINT: PaintIdV1 = PaintIdV1::new(7);
const LABEL_OPACITY_INPUT: OpacityInputIdV1 = OpacityInputIdV1::new(8);
const PAGE_SURFACE_INPUT: SurfaceInputPortIdV1 = SurfaceInputPortIdV1::new(9);
const PAGE_SURFACE: SurfaceIdV1 = SurfaceIdV1::new(10);
const FILL_SURFACE: SurfaceIdV1 = SurfaceIdV1::new(11);
const FILL_ON_PAGE: OccurrenceIdV1 = OccurrenceIdV1::new(12);
const LABEL_ON_FILL: OccurrenceIdV1 = OccurrenceIdV1::new(13);
const PRESENTATION_ROOT: PresentationRootIdV1 = PresentationRootIdV1::new(14);
const INTRINSIC_RELATION: ConstraintIdV1 = ConstraintIdV1::new(15);
const FINAL_VISIBLE_IDENTITY: ConstraintIdV1 = ConstraintIdV1::new(16);
const OUTPUT: OutputSlotIdV1 = OutputSlotIdV1::new(17);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrivateFixtureErrorV1 {
    InvalidMagic,
    UnsupportedVersion,
    InvalidLength,
    InvalidAuthoredData,
    SelectionReleaseRejected,
    ProgramCompileRejected,
    AttachmentRejected,
    UpdateRejected,
    MissingCertifiedOutput,
    MultipleCertifiedOutputs,
    MissingSelectionReleaseIdentity,
    InternalInvariant,
    Busy,
    AlreadyActive,
    InvalidLifecycle,
    InvalidDisposeToken,
    DisposeNotConfirmed,
    MissingSelectedState,
}

impl PrivateFixtureErrorV1 {
    const fn status(self) -> u32 {
        match self {
            Self::InvalidMagic => 1,
            Self::UnsupportedVersion => 2,
            Self::InvalidLength => 3,
            Self::InvalidAuthoredData => 4,
            Self::SelectionReleaseRejected => 5,
            Self::ProgramCompileRejected => 6,
            Self::AttachmentRejected => 7,
            Self::UpdateRejected => 8,
            Self::MissingCertifiedOutput => 9,
            Self::MultipleCertifiedOutputs => 10,
            Self::MissingSelectionReleaseIdentity => 11,
            Self::InternalInvariant => 12,
            Self::Busy => 13,
            Self::AlreadyActive => 14,
            Self::InvalidLifecycle => 15,
            Self::InvalidDisposeToken => 16,
            Self::DisposeNotConfirmed => 17,
            Self::MissingSelectedState => 18,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct AuthoredTargetCandidateV1 {
    id: u32,
    source: Srgb8,
    opacity_bits: u64,
}

#[derive(Debug, Clone, Copy)]
struct AuthoredJointStateV1 {
    key: [u8; SELECTION_KEY_WIDTH],
    fill_candidate: u32,
    label_candidate: u32,
}

#[derive(Debug, Clone, Copy)]
enum AuthoredSurroundV1 {
    Average,
    Dim,
    Dark,
}

impl AuthoredSurroundV1 {
    const fn into_program(self) -> SurroundV1 {
        match self {
            Self::Average => SurroundV1::Average,
            Self::Dim => SurroundV1::Dim,
            Self::Dark => SurroundV1::Dark,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct AuthoredAppearanceV1 {
    adapting_luminance_cd_m2: f64,
    background_luminance_ratio_yb_yw: f64,
    surround: AuthoredSurroundV1,
}

#[derive(Debug, Clone, Copy)]
struct AuthoredScenarioV1 {
    id: u32,
    page: Srgb8,
}

#[derive(Debug, Clone, Copy)]
struct AuthoredPrivateFixtureV1 {
    brand: Srgb8,
    fill_candidates: [AuthoredTargetCandidateV1; FILL_CANDIDATE_COUNT],
    label_candidates: [AuthoredTargetCandidateV1; LABEL_CANDIDATE_COUNT],
    joint_states: [AuthoredJointStateV1; JOINT_STATE_COUNT],
    selection_release_revision: u64,
    label_opacity: f64,
    appearance: AuthoredAppearanceV1,
    scenarios: [AuthoredScenarioV1; SCENARIO_COUNT],
    expected_final_visible: Srgb8,
    stream_id: u32,
    observation_revision: u64,
    sink_output: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CertifiedPrivateFixtureResultV1 {
    output: u32,
    sink_output: u32,
    selected_state_index: u32,
    paint_source: Srgb8,
    paint_opacity_bits: u64,
    content_identity: [u8; IDENTITY_WIDTH],
    selection_release_identity: [u8; IDENTITY_WIDTH],
}

struct WireReaderV1<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> WireReaderV1<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_bytes<const N: usize>(&mut self) -> Result<[u8; N], PrivateFixtureErrorV1> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or(PrivateFixtureErrorV1::InvalidLength)?;
        let source = self
            .bytes
            .get(self.offset..end)
            .ok_or(PrivateFixtureErrorV1::InvalidLength)?;
        let mut value = [0; N];
        value.copy_from_slice(source);
        self.offset = end;
        Ok(value)
    }

    fn read_u8(&mut self) -> Result<u8, PrivateFixtureErrorV1> {
        let [value] = self.read_bytes::<BYTE_WIDTH>()?;
        Ok(value)
    }

    fn read_u16(&mut self) -> Result<u16, PrivateFixtureErrorV1> {
        Ok(u16::from_le_bytes(self.read_bytes::<U16_WIDTH>()?))
    }

    fn read_u32(&mut self) -> Result<u32, PrivateFixtureErrorV1> {
        Ok(u32::from_le_bytes(self.read_bytes::<U32_WIDTH>()?))
    }

    fn read_u64(&mut self) -> Result<u64, PrivateFixtureErrorV1> {
        Ok(u64::from_le_bytes(self.read_bytes::<U64_WIDTH>()?))
    }

    fn read_f64(&mut self) -> Result<f64, PrivateFixtureErrorV1> {
        Ok(f64::from_bits(self.read_u64()?))
    }

    fn read_rgb(&mut self) -> Result<Srgb8, PrivateFixtureErrorV1> {
        Ok(Srgb8::new(self.read_bytes::<RGB_WIDTH>()?))
    }

    fn finish(self) -> Result<(), PrivateFixtureErrorV1> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(PrivateFixtureErrorV1::InvalidLength)
        }
    }
}

#[cfg(test)]
struct WireWriterV1<'a> {
    bytes: &'a mut [u8],
    offset: usize,
}

#[cfg(test)]
impl<'a> WireWriterV1<'a> {
    fn new(bytes: &'a mut [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn write_bytes<const N: usize>(&mut self, value: [u8; N]) -> Result<(), PrivateFixtureErrorV1> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or(PrivateFixtureErrorV1::InternalInvariant)?;
        let destination = self
            .bytes
            .get_mut(self.offset..end)
            .ok_or(PrivateFixtureErrorV1::InternalInvariant)?;
        destination.copy_from_slice(&value);
        self.offset = end;
        Ok(())
    }

    #[cfg(test)]
    fn write_u8(&mut self, value: u8) -> Result<(), PrivateFixtureErrorV1> {
        self.write_bytes([value])
    }

    fn write_u16(&mut self, value: u16) -> Result<(), PrivateFixtureErrorV1> {
        self.write_bytes(value.to_le_bytes())
    }

    fn write_u32(&mut self, value: u32) -> Result<(), PrivateFixtureErrorV1> {
        self.write_bytes(value.to_le_bytes())
    }

    fn write_u64(&mut self, value: u64) -> Result<(), PrivateFixtureErrorV1> {
        self.write_bytes(value.to_le_bytes())
    }

    #[cfg(test)]
    fn write_f64(&mut self, value: f64) -> Result<(), PrivateFixtureErrorV1> {
        self.write_u64(value.to_bits())
    }

    fn write_rgb(&mut self, value: Srgb8) -> Result<(), PrivateFixtureErrorV1> {
        self.write_bytes(value.bytes())
    }

    fn finish(self) -> Result<(), PrivateFixtureErrorV1> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(PrivateFixtureErrorV1::InternalInvariant)
        }
    }
}

fn wire_len_u16(value: usize) -> Result<u16, PrivateFixtureErrorV1> {
    u16::try_from(value).map_err(|_| PrivateFixtureErrorV1::InternalInvariant)
}

fn decode_request_v1(
    bytes: &[u8; PRIVATE_FIXTURE_REQUEST_V1_LEN],
) -> Result<AuthoredPrivateFixtureV1, PrivateFixtureErrorV1> {
    let mut reader = WireReaderV1::new(bytes);
    if reader.read_bytes::<MAGIC_WIDTH>()? != PRIVATE_FIXTURE_REQUEST_V1_MAGIC {
        return Err(PrivateFixtureErrorV1::InvalidMagic);
    }
    if reader.read_u16()? != PRIVATE_FIXTURE_ABI_VERSION_V1 {
        return Err(PrivateFixtureErrorV1::UnsupportedVersion);
    }
    if reader.read_u16()? != wire_len_u16(PRIVATE_FIXTURE_REQUEST_V1_LEN)? {
        return Err(PrivateFixtureErrorV1::InvalidLength);
    }

    let brand = reader.read_rgb()?;
    let fill_candidates = read_fill_candidates(&mut reader)?;
    let label_candidates = read_label_candidates(&mut reader)?;
    let joint_states = read_joint_states(&mut reader)?;
    let selection_release_revision = reader.read_u64()?;
    let label_opacity = reader.read_f64()?;
    let appearance = AuthoredAppearanceV1 {
        adapting_luminance_cd_m2: reader.read_f64()?,
        background_luminance_ratio_yb_yw: reader.read_f64()?,
        surround: match reader.read_u8()? {
            0 => AuthoredSurroundV1::Average,
            1 => AuthoredSurroundV1::Dim,
            2 => AuthoredSurroundV1::Dark,
            _ => return Err(PrivateFixtureErrorV1::InvalidAuthoredData),
        },
    };
    let scenarios = read_scenarios(&mut reader)?;
    let expected_final_visible = reader.read_rgb()?;
    let stream_id = reader.read_u32()?;
    let observation_revision = reader.read_u64()?;
    let sink_output = reader.read_u32()?;
    reader.finish()?;

    Ok(AuthoredPrivateFixtureV1 {
        brand,
        fill_candidates,
        label_candidates,
        joint_states,
        selection_release_revision,
        label_opacity,
        appearance,
        scenarios,
        expected_final_visible,
        stream_id,
        observation_revision,
        sink_output,
    })
}

fn read_target_candidate(
    reader: &mut WireReaderV1<'_>,
) -> Result<AuthoredTargetCandidateV1, PrivateFixtureErrorV1> {
    Ok(AuthoredTargetCandidateV1 {
        id: reader.read_u32()?,
        source: reader.read_rgb()?,
        opacity_bits: reader.read_u64()?,
    })
}

fn read_fill_candidates(
    reader: &mut WireReaderV1<'_>,
) -> Result<[AuthoredTargetCandidateV1; FILL_CANDIDATE_COUNT], PrivateFixtureErrorV1> {
    Ok([
        read_target_candidate(reader)?,
        read_target_candidate(reader)?,
        read_target_candidate(reader)?,
    ])
}

fn read_label_candidates(
    reader: &mut WireReaderV1<'_>,
) -> Result<[AuthoredTargetCandidateV1; LABEL_CANDIDATE_COUNT], PrivateFixtureErrorV1> {
    Ok([
        read_target_candidate(reader)?,
        read_target_candidate(reader)?,
    ])
}

fn read_joint_states(
    reader: &mut WireReaderV1<'_>,
) -> Result<[AuthoredJointStateV1; JOINT_STATE_COUNT], PrivateFixtureErrorV1> {
    Ok([
        AuthoredJointStateV1 {
            key: reader.read_bytes::<SELECTION_KEY_WIDTH>()?,
            fill_candidate: reader.read_u32()?,
            label_candidate: reader.read_u32()?,
        },
        AuthoredJointStateV1 {
            key: reader.read_bytes::<SELECTION_KEY_WIDTH>()?,
            fill_candidate: reader.read_u32()?,
            label_candidate: reader.read_u32()?,
        },
        AuthoredJointStateV1 {
            key: reader.read_bytes::<SELECTION_KEY_WIDTH>()?,
            fill_candidate: reader.read_u32()?,
            label_candidate: reader.read_u32()?,
        },
        AuthoredJointStateV1 {
            key: reader.read_bytes::<SELECTION_KEY_WIDTH>()?,
            fill_candidate: reader.read_u32()?,
            label_candidate: reader.read_u32()?,
        },
        AuthoredJointStateV1 {
            key: reader.read_bytes::<SELECTION_KEY_WIDTH>()?,
            fill_candidate: reader.read_u32()?,
            label_candidate: reader.read_u32()?,
        },
        AuthoredJointStateV1 {
            key: reader.read_bytes::<SELECTION_KEY_WIDTH>()?,
            fill_candidate: reader.read_u32()?,
            label_candidate: reader.read_u32()?,
        },
    ])
}

fn read_scenarios(
    reader: &mut WireReaderV1<'_>,
) -> Result<[AuthoredScenarioV1; SCENARIO_COUNT], PrivateFixtureErrorV1> {
    Ok([
        AuthoredScenarioV1 {
            id: reader.read_u32()?,
            page: reader.read_rgb()?,
        },
        AuthoredScenarioV1 {
            id: reader.read_u32()?,
            page: reader.read_rgb()?,
        },
    ])
}

fn target_domain_v1(
    candidates: impl IntoIterator<Item = AuthoredTargetCandidateV1>,
) -> Result<FinitePaintDomainV1, PrivateFixtureErrorV1> {
    FinitePaintDomainV1::try_new(
        candidates
            .into_iter()
            .map(|candidate| {
                Ok(TargetCandidateV1::new(
                    TargetCandidateIdV1::new(candidate.id),
                    PaintValueV1::try_new(candidate.source, f64::from_bits(candidate.opacity_bits))
                        .map_err(|_| PrivateFixtureErrorV1::InvalidAuthoredData)?,
                ))
            })
            .collect::<Result<Vec<_>, PrivateFixtureErrorV1>>()?,
    )
    .map_err(|_| PrivateFixtureErrorV1::InvalidAuthoredData)
}

fn materialise_selection_v1(
    authored: &AuthoredPrivateFixtureV1,
) -> Result<crate::selection_release::MaterialisedSelectionV1, PrivateFixtureErrorV1> {
    let rank_groups = authored
        .joint_states
        .iter()
        .map(|state| {
            vec![SelectionCandidateKeyV1::new(
                state.key.to_vec().into_boxed_slice(),
            )]
            .into_boxed_slice()
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let release = admit_selection_release_v1(SelectionReleaseV1::new(
        authored.selection_release_revision,
        rank_groups,
    ))
    .map_err(|_| PrivateFixtureErrorV1::SelectionReleaseRejected)?;

    let bindings = authored
        .joint_states
        .iter()
        .map(|state| {
            (
                JointCandidateStateV1::new(vec![
                    TargetCandidateChoiceV1::new(
                        CoreTargetId::new(FILL_TARGET.value()),
                        CoreTargetCandidateId::new(state.fill_candidate),
                    ),
                    TargetCandidateChoiceV1::new(
                        CoreTargetId::new(LABEL_TARGET.value()),
                        CoreTargetCandidateId::new(state.label_candidate),
                    ),
                ]),
                SelectionCandidateKeyV1::new(state.key.to_vec().into_boxed_slice()),
            )
        })
        .collect::<Vec<_>>();
    materialise_joint_selection_v1(&release, &bindings)
        .map_err(|_| PrivateFixtureErrorV1::SelectionReleaseRejected)
}

struct ExecutedPrivateFixtureV1<H>
where
    H: HandoffPointSinkHostV1,
{
    attachment: HandoffAttachmentV1<H>,
    projection: Result<CertifiedPrivateFixtureResultV1, PrivateFixtureErrorV1>,
}

fn execute_private_fixture_v1<H>(
    authored: AuthoredPrivateFixtureV1,
    host: H,
) -> Result<ExecutedPrivateFixtureV1<H>, PrivateFixtureErrorV1>
where
    H: HandoffPointSinkHostV1,
{
    let fill_domain = target_domain_v1(authored.fill_candidates)?;
    let label_domain = target_domain_v1(authored.label_candidates)?;
    let selection = materialise_selection_v1(&authored)?;
    let context = AppearanceContextV1::try_new(
        authored.appearance.adapting_luminance_cd_m2,
        authored.appearance.background_luminance_ratio_yb_yw,
        authored.appearance.surround.into_program(),
    )
    .map_err(|_| PrivateFixtureErrorV1::InvalidAuthoredData)?;
    let relation = DirectedRelationV1::try_new(BRAND_REFERENCE_TARGET, vec![LABEL_TARGET])
        .map_err(|_| PrivateFixtureErrorV1::InternalInvariant)?;

    let mut draft = DraftV1::new();
    draft.push_source(BRAND_SOURCE, authored.brand);
    draft.push_fixed_target(BRAND_REFERENCE_TARGET, BRAND_SOURCE);
    draft.push_finite_target(FILL_TARGET, fill_domain);
    draft.push_finite_target(LABEL_TARGET, label_domain);
    draft
        .set_materialised_joint_selection(selection)
        .map_err(|_| PrivateFixtureErrorV1::InvalidAuthoredData)?;
    draft.push_solid_paint(FILL_PAINT, FILL_TARGET);
    draft.push_solid_paint(LABEL_SOLID_PAINT, LABEL_TARGET);
    draft.push_opacity_input(LABEL_OPACITY_INPUT, authored.label_opacity);
    draft.push_opacity_paint(LABEL_OPACITY_PAINT, LABEL_SOLID_PAINT, LABEL_OPACITY_INPUT);
    draft.push_surface_input_port(PAGE_SURFACE_INPUT);
    draft.push_input_surface(PAGE_SURFACE, PAGE_SURFACE_INPUT);
    draft.push_source_over_occurrence(FILL_ON_PAGE, FILL_PAINT, PAGE_SURFACE, context);
    draft.push_occurrence_surface(FILL_SURFACE, FILL_ON_PAGE);
    draft.push_source_over_occurrence(LABEL_ON_FILL, LABEL_OPACITY_PAINT, FILL_SURFACE, context);
    draft.push_point_presentation_root(PRESENTATION_ROOT, LABEL_ON_FILL);
    draft.push_point_presentation_target(PRESENTATION_ROOT, LABEL_ON_FILL);
    draft.push_exact_intrinsic_relation_hard(INTRINSIC_RELATION, relation);
    draft.push_exact_visible_unary_hard(
        FINAL_VISIBLE_IDENTITY,
        LABEL_ON_FILL,
        authored.expected_final_visible,
    );
    draft.push_output(OUTPUT, LABEL_OPACITY_PAINT);

    let owner = draft
        .compile()
        .map_err(|_| PrivateFixtureErrorV1::ProgramCompileRejected)?;
    let sink_output = HandoffPointSinkOutputIdV1::new(authored.sink_output);
    let emissions = [AuthoredPointEmissionBindingV1::new(OUTPUT, sink_output)];
    let presentations = [AuthoredPointPresentationBindingV1::new(
        OUTPUT,
        PRESENTATION_ROOT,
        LABEL_ON_FILL,
    )];
    let mut attachment = owner
        .attach_external(
            authored.stream_id,
            &emissions,
            &presentations,
            FamilyArtifactBundleV2::empty(),
            handoff_point_sink(sink_output, host),
        )
        .map_err(|_| PrivateFixtureErrorV1::AttachmentRejected)?;

    let [first_scenario, second_scenario] = authored.scenarios;
    let first_surface = [first_scenario.page];
    let second_surface = [second_scenario.page];
    let scenarios = [
        ScenarioV1::new(first_scenario.id, &first_surface),
        ScenarioV1::new(second_scenario.id, &second_surface),
    ];
    let commit = match attachment.update(UpdateV1::Observed {
        revision: authored.observation_revision,
        scenarios: &scenarios,
    }) {
        Ok(commit) => commit,
        Err(_) => {
            return Ok(ExecutedPrivateFixtureV1 {
                attachment,
                projection: Err(PrivateFixtureErrorV1::UpdateRejected),
            });
        }
    };
    let projection = project_attachment_commit_v1(commit);

    Ok(ExecutedPrivateFixtureV1 {
        attachment,
        projection,
    })
}

fn project_attachment_commit_v1(
    commit: crate::program::attachment::AttachmentCommitV1<'_, HandoffPointSinkOutputIdV1>,
) -> Result<CertifiedPrivateFixtureResultV1, PrivateFixtureErrorV1> {
    let mut renders = commit.render_outputs();
    let render = renders
        .next()
        .ok_or(PrivateFixtureErrorV1::MissingCertifiedOutput)?;
    if renders.next().is_some() {
        return Err(PrivateFixtureErrorV1::MultipleCertifiedOutputs);
    }
    project_certified_render_v1(render)
}

fn project_certified_render_v1(
    render: crate::program::attachment::AttachedRenderOutputV1<'_, HandoffPointSinkOutputIdV1>,
) -> Result<CertifiedPrivateFixtureResultV1, PrivateFixtureErrorV1> {
    let certificate = render.certificate();
    let selection_release_identity = certificate
        .selection_release_identity()
        .ok_or(PrivateFixtureErrorV1::MissingSelectionReleaseIdentity)?;
    let selected_state_index = certificate
        .selected_state_index()
        .ok_or(PrivateFixtureErrorV1::MissingSelectedState)
        .and_then(|index| {
            u32::try_from(index).map_err(|_| PrivateFixtureErrorV1::InternalInvariant)
        })?;
    let paint = render.paint();
    Ok(CertifiedPrivateFixtureResultV1 {
        output: render.output().value(),
        sink_output: render.sink_output().value(),
        selected_state_index,
        paint_source: paint.source(),
        paint_opacity_bits: paint.opacity_bits(),
        content_identity: *certificate.content_identity().as_bytes(),
        selection_release_identity: *selection_release_identity.as_bytes(),
    })
}

/// Проецирует исход `begin_dispose` в фиксированный wire-контракт v1: живой
/// token возвращается как есть, Vacant (`InvalidLifecycle`) — как зарезервированный
/// sentinel `0` (generation никогда не равен нулю), а любое инвариантное
/// нарушение — как его типизированный status, чтобы consumer не принял
/// незаконный lifecycle за отсутствие attachment.
const fn begin_dispose_status_v1(result: Result<u32, PrivateFixtureErrorV1>) -> u32 {
    match result {
        Ok(token) => token,
        Err(PrivateFixtureErrorV1::InvalidLifecycle) => 0,
        Err(error) => error.status(),
    }
}

const RESULT_MAGIC_OFFSET: usize = 0;
const RESULT_VERSION_OFFSET: usize = RESULT_MAGIC_OFFSET + MAGIC_WIDTH;
const RESULT_LENGTH_OFFSET: usize = RESULT_VERSION_OFFSET + U16_WIDTH;
const RESULT_OUTPUT_OFFSET: usize = RESULT_LENGTH_OFFSET + U16_WIDTH;
const RESULT_SINK_OUTPUT_OFFSET: usize = RESULT_OUTPUT_OFFSET + U32_WIDTH;
const RESULT_SELECTED_STATE_OFFSET: usize = RESULT_SINK_OUTPUT_OFFSET + U32_WIDTH;
const RESULT_RGB_OFFSET: usize = RESULT_SELECTED_STATE_OFFSET + U32_WIDTH;
const RESULT_OPACITY_OFFSET: usize = RESULT_RGB_OFFSET + RGB_WIDTH;
const RESULT_CONTENT_IDENTITY_OFFSET: usize = RESULT_OPACITY_OFFSET + U64_WIDTH;
const RESULT_RELEASE_IDENTITY_OFFSET: usize = RESULT_CONTENT_IDENTITY_OFFSET + IDENTITY_WIDTH;
const RESULT_END_OFFSET: usize = RESULT_RELEASE_IDENTITY_OFFSET + IDENTITY_WIDTH;
const _: () = assert!(RESULT_END_OFFSET == PRIVATE_FIXTURE_RESULT_V1_LEN);

fn encode_result_v1(
    result: CertifiedPrivateFixtureResultV1,
) -> [u8; PRIVATE_FIXTURE_RESULT_V1_LEN] {
    let mut bytes = [0; PRIVATE_FIXTURE_RESULT_V1_LEN];
    bytes[RESULT_MAGIC_OFFSET..RESULT_VERSION_OFFSET]
        .copy_from_slice(&PRIVATE_FIXTURE_RESULT_V1_MAGIC);
    bytes[RESULT_VERSION_OFFSET..RESULT_LENGTH_OFFSET]
        .copy_from_slice(&PRIVATE_FIXTURE_ABI_VERSION_V1.to_le_bytes());
    bytes[RESULT_LENGTH_OFFSET..RESULT_OUTPUT_OFFSET]
        .copy_from_slice(&(PRIVATE_FIXTURE_RESULT_V1_LEN as u16).to_le_bytes());
    bytes[RESULT_OUTPUT_OFFSET..RESULT_SINK_OUTPUT_OFFSET]
        .copy_from_slice(&result.output.to_le_bytes());
    bytes[RESULT_SINK_OUTPUT_OFFSET..RESULT_SELECTED_STATE_OFFSET]
        .copy_from_slice(&result.sink_output.to_le_bytes());
    bytes[RESULT_SELECTED_STATE_OFFSET..RESULT_RGB_OFFSET]
        .copy_from_slice(&result.selected_state_index.to_le_bytes());
    bytes[RESULT_RGB_OFFSET..RESULT_OPACITY_OFFSET].copy_from_slice(&result.paint_source.bytes());
    bytes[RESULT_OPACITY_OFFSET..RESULT_CONTENT_IDENTITY_OFFSET]
        .copy_from_slice(&result.paint_opacity_bits.to_le_bytes());
    bytes[RESULT_CONTENT_IDENTITY_OFFSET..RESULT_RELEASE_IDENTITY_OFFSET]
        .copy_from_slice(&result.content_identity);
    bytes[RESULT_RELEASE_IDENTITY_OFFSET..RESULT_END_OFFSET]
        .copy_from_slice(&result.selection_release_identity);
    bytes
}

enum PrivateFixtureLifecycleV1<H>
where
    H: HandoffPointSinkHostV1,
{
    Vacant,
    Running,
    Active {
        generation: u32,
        attachment: HandoffAttachmentV1<H>,
    },
    Disposing {
        generation: u32,
        token: u32,
        attachment: Option<HandoffAttachmentV1<H>>,
    },
}

struct PrivateFixtureInstanceV1<H>
where
    H: HandoffPointSinkHostV1,
{
    lifecycle: PrivateFixtureLifecycleV1<H>,
    next_generation: u32,
}

impl<H> PrivateFixtureInstanceV1<H>
where
    H: HandoffPointSinkHostV1,
{
    const fn new() -> Self {
        Self {
            lifecycle: PrivateFixtureLifecycleV1::Vacant,
            next_generation: 1,
        }
    }

    fn begin_run(&mut self) -> Result<u32, PrivateFixtureErrorV1> {
        if !matches!(self.lifecycle, PrivateFixtureLifecycleV1::Vacant) {
            return Err(PrivateFixtureErrorV1::AlreadyActive);
        }
        let generation = self.next_generation;
        self.next_generation = generation
            .checked_add(1)
            .ok_or(PrivateFixtureErrorV1::InternalInvariant)?;
        self.lifecycle = PrivateFixtureLifecycleV1::Running;
        Ok(generation)
    }

    fn fail_run(&mut self) -> Result<(), PrivateFixtureErrorV1> {
        if !matches!(self.lifecycle, PrivateFixtureLifecycleV1::Running) {
            return Err(PrivateFixtureErrorV1::InternalInvariant);
        }
        self.lifecycle = PrivateFixtureLifecycleV1::Vacant;
        Ok(())
    }

    fn complete_run(
        &mut self,
        generation: u32,
        executed: ExecutedPrivateFixtureV1<H>,
    ) -> Result<Result<CertifiedPrivateFixtureResultV1, PrivateFixtureErrorV1>, PrivateFixtureErrorV1>
    {
        if !matches!(self.lifecycle, PrivateFixtureLifecycleV1::Running) {
            return Err(PrivateFixtureErrorV1::InternalInvariant);
        }
        self.lifecycle = PrivateFixtureLifecycleV1::Active {
            generation,
            attachment: executed.attachment,
        };
        Ok(executed.projection)
    }

    fn begin_dispose(&mut self) -> Result<u32, PrivateFixtureErrorV1> {
        let previous = core::mem::replace(&mut self.lifecycle, PrivateFixtureLifecycleV1::Running);
        match previous {
            PrivateFixtureLifecycleV1::Active {
                generation,
                attachment,
            } => {
                let token = generation;
                self.lifecycle = PrivateFixtureLifecycleV1::Disposing {
                    generation,
                    token,
                    attachment: Some(attachment),
                };
                Ok(token)
            }
            // Vacant is the one legitimate "no active attachment" outcome; the
            // ABI maps it to the reserved 0 sentinel (generations never yield a
            // zero token). Any other state means the caller raced a live run or
            // a disposal, which must stay typed and fail closed, never Vacant.
            PrivateFixtureLifecycleV1::Vacant => {
                self.lifecycle = PrivateFixtureLifecycleV1::Vacant;
                Err(PrivateFixtureErrorV1::InvalidLifecycle)
            }
            other => {
                self.lifecycle = other;
                Err(PrivateFixtureErrorV1::InternalInvariant)
            }
        }
    }

    fn abort_dispose(&mut self, token: u32) -> Result<(), PrivateFixtureErrorV1> {
        let previous = core::mem::replace(&mut self.lifecycle, PrivateFixtureLifecycleV1::Running);
        match previous {
            PrivateFixtureLifecycleV1::Disposing {
                generation,
                token: expected,
                attachment,
            } if token == expected => match attachment {
                Some(attachment) => {
                    self.lifecycle = PrivateFixtureLifecycleV1::Active {
                        generation,
                        attachment,
                    };
                    Ok(())
                }
                None => {
                    self.lifecycle = PrivateFixtureLifecycleV1::Disposing {
                        generation,
                        token: expected,
                        attachment: None,
                    };
                    Err(PrivateFixtureErrorV1::InternalInvariant)
                }
            },
            other => {
                self.lifecycle = other;
                Err(PrivateFixtureErrorV1::InvalidDisposeToken)
            }
        }
    }

    fn commit_dispose(
        &mut self,
        token: u32,
        confirm: impl FnOnce(u32, u32) -> Result<(), PrivateFixtureErrorV1>,
    ) -> Result<(), PrivateFixtureErrorV1> {
        let result = match &mut self.lifecycle {
            PrivateFixtureLifecycleV1::Disposing {
                generation,
                token: expected,
                attachment,
            } if token == *expected => {
                HandoffAttachmentV1::<H>::confirm_and_consume_external_dispose(attachment, || {
                    confirm(*generation, token)
                })
                .map_err(|error| match error {
                    ExternalDisposeErrorV1::Confirmation(error) => error,
                    ExternalDisposeErrorV1::MissingAttachment => {
                        PrivateFixtureErrorV1::InternalInvariant
                    }
                })
            }
            _ => Err(PrivateFixtureErrorV1::InvalidDisposeToken),
        };
        match result {
            Ok(()) => {
                self.lifecycle = PrivateFixtureLifecycleV1::Vacant;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
}

fn run_request_v1<H>(
    request: &[u8; PRIVATE_FIXTURE_REQUEST_V1_LEN],
    result: &mut [u8; PRIVATE_FIXTURE_RESULT_V1_LEN],
    instance: &mut PrivateFixtureInstanceV1<H>,
    make_host: impl FnOnce(u32) -> H,
) -> u32
where
    H: HandoffPointSinkHostV1,
{
    result.fill(0);
    let authored = match decode_request_v1(request) {
        Ok(authored) => authored,
        Err(error) => return error.status(),
    };
    let generation = match instance.begin_run() {
        Ok(generation) => generation,
        Err(error) => return error.status(),
    };
    let executed = match execute_private_fixture_v1(authored, make_host(generation)) {
        Ok(executed) => executed,
        Err(error) => {
            return match instance.fail_run() {
                Ok(()) => error.status(),
                Err(invariant) => invariant.status(),
            };
        }
    };
    match instance.complete_run(generation, executed) {
        Ok(Ok(certified)) => {
            *result = encode_result_v1(certified);
            0
        }
        Ok(Err(error)) | Err(error) => error.status(),
    }
}

struct PrivateFixtureAbiGateV1 {
    busy: core::sync::atomic::AtomicBool,
}

impl PrivateFixtureAbiGateV1 {
    const fn new() -> Self {
        Self {
            busy: core::sync::atomic::AtomicBool::new(false),
        }
    }

    fn try_enter(&self) -> Option<PrivateFixtureAbiGuardV1<'_>> {
        self.busy
            .compare_exchange(
                false,
                true,
                core::sync::atomic::Ordering::Acquire,
                core::sync::atomic::Ordering::Relaxed,
            )
            .ok()
            .map(|_| PrivateFixtureAbiGuardV1 { gate: self })
    }
}

struct PrivateFixtureAbiGuardV1<'gate> {
    gate: &'gate PrivateFixtureAbiGateV1,
}

impl Drop for PrivateFixtureAbiGuardV1<'_> {
    fn drop(&mut self) {
        self.gate
            .busy
            .store(false, core::sync::atomic::Ordering::Release);
    }
}

/// Executes the fixed-buffer entry without holding Rust references to buffers
/// that remain writable through exported linear-memory pointers.
///
/// # Safety
///
/// `request`, `result`, and `instance` must be valid, properly aligned,
/// pairwise-disjoint pointers for this call. `instance` must not be reachable by
/// the host callback except through a nested call to this function with the
/// same `gate`; that nested call returns Busy before dereferencing any pointer.
/// The host may read or overwrite `request` and `result` synchronously.
unsafe fn run_fixed_buffer_entry_v1<H>(
    gate: &PrivateFixtureAbiGateV1,
    request: *mut [u8; PRIVATE_FIXTURE_REQUEST_V1_LEN],
    result: *mut [u8; PRIVATE_FIXTURE_RESULT_V1_LEN],
    instance: *mut PrivateFixtureInstanceV1<H>,
    make_host: impl FnOnce(u32) -> H,
) -> u32
where
    H: HandoffPointSinkHostV1,
{
    let Some(_guard) = gate.try_enter() else {
        return PrivateFixtureErrorV1::Busy.status();
    };

    // SAFETY: the caller supplies valid disjoint backing cells. Copying and
    // clearing happen before any host call, and no reference to either backing
    // cell exists while `run_request_v1` may synchronously enter the host.
    let request_snapshot = unsafe { request.read() };
    unsafe { result.write([0; PRIVATE_FIXTURE_RESULT_V1_LEN]) };
    let mut staged_result = [0; PRIVATE_FIXTURE_RESULT_V1_LEN];
    // SAFETY: INSTANCE is not exported and the gate was acquired before this
    // dereference. A nested call returns Busy before reaching this line.
    let status = unsafe {
        run_request_v1(
            &request_snapshot,
            &mut staged_result,
            &mut *instance,
            make_host,
        )
    };
    // SAFETY: all synchronous host callbacks have returned; publishing the
    // staged Copy replaces any hostile/stale writes to the exported result.
    unsafe { result.write(staged_result) };
    status
}

#[cfg(all(target_arch = "wasm32", not(labcolors_private_fixture_unshared_v1)))]
compile_error!("the private fixture fixed-buffer ABI requires explicit unshared-WASM admission");

#[cfg(all(target_arch = "wasm32", labcolors_private_fixture_unshared_v1))]
mod wasm_abi {
    use core::cell::UnsafeCell;

    use super::*;

    /// The sole return value which means the host atomically completed publish.
    const HOST_INSTALL_SUCCESS_V1: u32 = 0x4c43_0001;
    /// The sole return value which proves the same generation has a tombstone.
    const HOST_DISPOSE_CONFIRMED_V1: u32 = 0x4c43_0002;
    const DISPOSE_BEGIN_BUSY_V1: u32 = u32::MAX;

    #[link(wasm_import_module = "labcolors_private_fixture_host_v1")]
    unsafe extern "C" {
        #[link_name = "labcolors_private_fixture_host_install_v1"]
        fn host_install_v1(
            generation: u32,
            operation: u32,
            revision_low: u32,
            revision_high: u32,
            expected_low: u32,
            expected_high: u32,
            desired_low: u32,
            desired_high: u32,
            output: u32,
            sink_output: u32,
            css_ptr: *const u8,
            css_len: u32,
        ) -> u32;

        #[link_name = "labcolors_private_fixture_host_confirm_disposed_v1"]
        fn host_confirm_disposed_v1(generation: u32, token: u32) -> u32;
    }

    struct WasmHostPointSinkV1 {
        generation: u32,
        css: String,
    }

    impl HandoffPointSinkHostV1 for WasmHostPointSinkV1 {
        fn try_install(
            &mut self,
            intent: HandoffPointSinkHostIntentV1,
        ) -> Result<(), HandoffPointSinkHostErrorV1> {
            let (output, sink_output) = match intent.point() {
                Some((output, sink_output, paint)) => {
                    let [red, green, blue] = paint.source().bytes();
                    let alpha = crate::css_alpha_value(f64::from_bits(paint.opacity_bits()))
                        .map_err(|_| HandoffPointSinkHostErrorV1::Protocol)?;
                    self.css = format!("rgba({red},{green},{blue},{alpha})");
                    (output.value(), sink_output.value())
                }
                None => {
                    self.css.clear();
                    (0, 0)
                }
            };
            let (revision_low, revision_high) = split_u64_v1(intent.revision());
            let (expected_low, expected_high) = split_u64_v1(intent.expected_sequence());
            let (desired_low, desired_high) = split_u64_v1(intent.desired_sequence());
            let css_len =
                u32::try_from(self.css.len()).map_err(|_| HandoffPointSinkHostErrorV1::Protocol)?;
            // SAFETY: the separate artifact loader supplies this exact scalar
            // ABI, catches exceptions/thenables, and returns the one documented
            // success magic only after atomic lease.publish has completed.
            let status = unsafe {
                host_install_v1(
                    self.generation,
                    intent.operation(),
                    revision_low,
                    revision_high,
                    expected_low,
                    expected_high,
                    desired_low,
                    desired_high,
                    output,
                    sink_output,
                    self.css.as_ptr(),
                    css_len,
                )
            };
            match status {
                HOST_INSTALL_SUCCESS_V1 => Ok(()),
                0 => Err(HandoffPointSinkHostErrorV1::Rejected),
                _ => Err(HandoffPointSinkHostErrorV1::Protocol),
            }
        }
    }

    const fn split_u64_v1(value: u64) -> (u32, u32) {
        (value as u32, (value >> 32) as u32)
    }

    struct SingleThreadWasmCellV1<T>(UnsafeCell<T>);

    impl<T> SingleThreadWasmCellV1<T> {
        const fn new(value: T) -> Self {
            Self(UnsafeCell::new(value))
        }
    }

    // SAFETY: only the canonical private build admits this module, and its
    // artifact validator rejects imported/shared memory. INSTANCE mutation is
    // guarded; exported request/result cells are accessed as raw backing and
    // the run entry never holds references to them across a synchronous host
    // callback.
    unsafe impl<T> Sync for SingleThreadWasmCellV1<T> {}

    static ABI_GATE_V1: PrivateFixtureAbiGateV1 = PrivateFixtureAbiGateV1::new();
    static REQUEST_V1: SingleThreadWasmCellV1<[u8; PRIVATE_FIXTURE_REQUEST_V1_LEN]> =
        SingleThreadWasmCellV1::new([0; PRIVATE_FIXTURE_REQUEST_V1_LEN]);
    static RESULT_V1: SingleThreadWasmCellV1<[u8; PRIVATE_FIXTURE_RESULT_V1_LEN]> =
        SingleThreadWasmCellV1::new([0; PRIVATE_FIXTURE_RESULT_V1_LEN]);
    static INSTANCE_V1: SingleThreadWasmCellV1<PrivateFixtureInstanceV1<WasmHostPointSinkV1>> =
        SingleThreadWasmCellV1::new(PrivateFixtureInstanceV1::new());

    /// Возвращает адрес принадлежащего instance request-buffer ABI v1.
    #[doc(hidden)]
    #[unsafe(export_name = "labcolors_private_fixture_request_v1_ptr")]
    pub extern "C" fn request_v1_ptr() -> *mut u8 {
        REQUEST_V1.0.get().cast::<u8>()
    }

    /// Возвращает точный размер request-buffer ABI v1.
    #[doc(hidden)]
    #[unsafe(export_name = "labcolors_private_fixture_request_v1_len")]
    pub extern "C" fn request_v1_len() -> u32 {
        PRIVATE_FIXTURE_REQUEST_V1_LEN as u32
    }

    /// Возвращает read-only адрес принадлежащего instance result-buffer ABI v1.
    #[doc(hidden)]
    #[unsafe(export_name = "labcolors_private_fixture_result_v1_ptr")]
    pub extern "C" fn result_v1_ptr() -> *const u8 {
        RESULT_V1.0.get().cast::<u8>().cast_const()
    }

    /// Возвращает точный размер result-buffer ABI v1.
    #[doc(hidden)]
    #[unsafe(export_name = "labcolors_private_fixture_result_v1_len")]
    pub extern "C" fn result_v1_len() -> u32 {
        PRIVATE_FIXTURE_RESULT_V1_LEN as u32
    }

    /// Обнуляет stale result и синхронно исполняет весь certified route.
    #[doc(hidden)]
    #[unsafe(export_name = "labcolors_private_fixture_run_v1")]
    pub extern "C" fn run_v1() -> u32 {
        // SAFETY: the three static cells are valid and pairwise distinct.
        // INSTANCE has no exported pointer; reentry uses this same gate.
        unsafe {
            run_fixed_buffer_entry_v1(
                &ABI_GATE_V1,
                REQUEST_V1.0.get(),
                RESULT_V1.0.get(),
                INSTANCE_V1.0.get(),
                |generation| WasmHostPointSinkV1 {
                    generation,
                    css: String::new(),
                },
            )
        }
    }

    #[doc(hidden)]
    #[unsafe(export_name = "labcolors_private_fixture_begin_dispose_v1")]
    pub extern "C" fn begin_dispose_v1() -> u32 {
        let Some(_guard) = ABI_GATE_V1.try_enter() else {
            return DISPOSE_BEGIN_BUSY_V1;
        };
        // SAFETY: the guard is held and this transition makes no host call.
        begin_dispose_status_v1(unsafe { &mut *INSTANCE_V1.0.get() }.begin_dispose())
    }

    #[doc(hidden)]
    #[unsafe(export_name = "labcolors_private_fixture_abort_dispose_v1")]
    pub extern "C" fn abort_dispose_v1(token: u32) -> u32 {
        let Some(_guard) = ABI_GATE_V1.try_enter() else {
            return PrivateFixtureErrorV1::Busy.status();
        };
        // SAFETY: the guard is held and this transition makes no host call.
        match unsafe { &mut *INSTANCE_V1.0.get() }.abort_dispose(token) {
            Ok(()) => 0,
            Err(error) => error.status(),
        }
    }

    #[doc(hidden)]
    #[unsafe(export_name = "labcolors_private_fixture_commit_dispose_v1")]
    pub extern "C" fn commit_dispose_v1(token: u32) -> u32 {
        let Some(_guard) = ABI_GATE_V1.try_enter() else {
            return PrivateFixtureErrorV1::Busy.status();
        };
        // SAFETY: the guard is held. Confirmation may re-enter, but nested ABI
        // calls fail the guard before borrowing instance state or buffers.
        let confirm = |generation, dispose_token| {
            // SAFETY: the loader catches exceptions and returns the exact magic
            // only for an already-inactive lease tombstoned at this generation.
            let status = unsafe { host_confirm_disposed_v1(generation, dispose_token) };
            if status == HOST_DISPOSE_CONFIRMED_V1 {
                Ok(())
            } else {
                Err(PrivateFixtureErrorV1::DisposeNotConfirmed)
            }
        };
        match unsafe { &mut *INSTANCE_V1.0.get() }.commit_dispose(token, confirm) {
            Ok(()) => 0,
            Err(error) => error.status(),
        }
    }
}

#[cfg(test)]
mod tests {
    use core::cell::{Cell, RefCell, UnsafeCell};
    use std::rc::Rc;

    use super::*;

    #[derive(Debug, Default)]
    struct HostOracleV1 {
        generation: Option<u32>,
        published: Option<HandoffPointSinkHostIntentV1>,
        installs: Vec<HandoffPointSinkHostIntentV1>,
        tombstone: Option<u32>,
        local_drop_count: usize,
        invalid_drop_count: usize,
        next_install_error: Option<HandoffPointSinkHostErrorV1>,
    }

    struct NativeFixtureHostV1 {
        generation: u32,
        oracle: Rc<RefCell<HostOracleV1>>,
        armed: bool,
    }

    impl HandoffPointSinkHostV1 for NativeFixtureHostV1 {
        fn try_install(
            &mut self,
            intent: HandoffPointSinkHostIntentV1,
        ) -> Result<(), HandoffPointSinkHostErrorV1> {
            self.armed = true;
            let mut oracle = self.oracle.borrow_mut();
            if oracle.generation != Some(self.generation) {
                return Err(HandoffPointSinkHostErrorV1::Rejected);
            }
            oracle.installs.push(intent);
            if let Some(error) = oracle.next_install_error.take() {
                return Err(error);
            }
            oracle.published = Some(intent);
            Ok(())
        }
    }

    impl Drop for NativeFixtureHostV1 {
        fn drop(&mut self) {
            if !self.armed {
                return;
            }
            let mut oracle = self.oracle.borrow_mut();
            oracle.local_drop_count += 1;
            if oracle.tombstone != Some(self.generation)
                || oracle.generation == Some(self.generation)
            {
                oracle.invalid_drop_count += 1;
            }
        }
    }

    fn native_host(generation: u32) -> (NativeFixtureHostV1, Rc<RefCell<HostOracleV1>>) {
        let oracle = Rc::new(RefCell::new(HostOracleV1 {
            generation: Some(generation),
            ..HostOracleV1::default()
        }));
        (
            NativeFixtureHostV1 {
                generation,
                oracle: Rc::clone(&oracle),
                armed: false,
            },
            oracle,
        )
    }

    fn mark_js_disposed(oracle: &Rc<RefCell<HostOracleV1>>, generation: u32) {
        let mut oracle = oracle.borrow_mut();
        assert_eq!(oracle.generation, Some(generation));
        oracle.generation = None;
        oracle.tombstone = Some(generation);
        oracle.published = None;
    }

    fn confirm_tombstone(
        oracle: &Rc<RefCell<HostOracleV1>>,
        generation: u32,
        token: u32,
    ) -> Result<(), PrivateFixtureErrorV1> {
        let oracle = oracle.borrow();
        if generation == token
            && oracle.tombstone == Some(generation)
            && oracle.generation != Some(generation)
        {
            Ok(())
        } else {
            Err(PrivateFixtureErrorV1::DisposeNotConfirmed)
        }
    }

    fn valid_authored() -> AuthoredPrivateFixtureV1 {
        let brand = Srgb8::new([64, 64, 64]);
        AuthoredPrivateFixtureV1 {
            brand,
            fill_candidates: [
                AuthoredTargetCandidateV1 {
                    id: 10,
                    source: Srgb8::new([0, 0, 0]),
                    opacity_bits: 0.5_f64.to_bits(),
                },
                AuthoredTargetCandidateV1 {
                    id: 11,
                    source: Srgb8::new([255, 255, 255]),
                    opacity_bits: 0.5_f64.to_bits(),
                },
                AuthoredTargetCandidateV1 {
                    id: 12,
                    source: Srgb8::new([128, 128, 128]),
                    opacity_bits: 1.0_f64.to_bits(),
                },
            ],
            label_candidates: [
                AuthoredTargetCandidateV1 {
                    id: 20,
                    source: brand,
                    opacity_bits: 1.0_f64.to_bits(),
                },
                AuthoredTargetCandidateV1 {
                    id: 21,
                    source: Srgb8::new([0, 0, 0]),
                    opacity_bits: 0.5_f64.to_bits(),
                },
            ],
            joint_states: [
                AuthoredJointStateV1 {
                    key: [0xA1; SELECTION_KEY_WIDTH],
                    fill_candidate: 12,
                    label_candidate: 21,
                },
                AuthoredJointStateV1 {
                    key: [0xA2; SELECTION_KEY_WIDTH],
                    fill_candidate: 10,
                    label_candidate: 20,
                },
                AuthoredJointStateV1 {
                    key: [0xB1; SELECTION_KEY_WIDTH],
                    fill_candidate: 11,
                    label_candidate: 20,
                },
                AuthoredJointStateV1 {
                    key: [0xB2; SELECTION_KEY_WIDTH],
                    fill_candidate: 12,
                    label_candidate: 20,
                },
                AuthoredJointStateV1 {
                    key: [0xC1; SELECTION_KEY_WIDTH],
                    fill_candidate: 10,
                    label_candidate: 21,
                },
                AuthoredJointStateV1 {
                    key: [0xC2; SELECTION_KEY_WIDTH],
                    fill_candidate: 11,
                    label_candidate: 21,
                },
            ],
            selection_release_revision: 7,
            label_opacity: 0.5,
            appearance: AuthoredAppearanceV1 {
                adapting_luminance_cd_m2: 64.0,
                background_luminance_ratio_yb_yw: 0.2,
                surround: AuthoredSurroundV1::Dim,
            },
            scenarios: [
                AuthoredScenarioV1 {
                    id: 101,
                    page: Srgb8::new([0, 0, 0]),
                },
                AuthoredScenarioV1 {
                    id: 102,
                    page: Srgb8::new([255, 255, 255]),
                },
            ],
            expected_final_visible: Srgb8::new([96, 96, 96]),
            stream_id: 301,
            observation_revision: 401,
            sink_output: 501,
        }
    }

    fn encode_request_for_test(
        authored: AuthoredPrivateFixtureV1,
    ) -> [u8; PRIVATE_FIXTURE_REQUEST_V1_LEN] {
        let mut bytes = [0; PRIVATE_FIXTURE_REQUEST_V1_LEN];
        let mut writer = WireWriterV1::new(&mut bytes);
        writer
            .write_bytes(PRIVATE_FIXTURE_REQUEST_V1_MAGIC)
            .unwrap();
        writer.write_u16(PRIVATE_FIXTURE_ABI_VERSION_V1).unwrap();
        writer
            .write_u16(wire_len_u16(PRIVATE_FIXTURE_REQUEST_V1_LEN).unwrap())
            .unwrap();
        writer.write_rgb(authored.brand).unwrap();
        for candidate in authored.fill_candidates {
            writer.write_u32(candidate.id).unwrap();
            writer.write_rgb(candidate.source).unwrap();
            writer.write_u64(candidate.opacity_bits).unwrap();
        }
        for candidate in authored.label_candidates {
            writer.write_u32(candidate.id).unwrap();
            writer.write_rgb(candidate.source).unwrap();
            writer.write_u64(candidate.opacity_bits).unwrap();
        }
        for state in authored.joint_states {
            writer.write_bytes(state.key).unwrap();
            writer.write_u32(state.fill_candidate).unwrap();
            writer.write_u32(state.label_candidate).unwrap();
        }
        writer
            .write_u64(authored.selection_release_revision)
            .unwrap();
        writer.write_f64(authored.label_opacity).unwrap();
        writer
            .write_f64(authored.appearance.adapting_luminance_cd_m2)
            .unwrap();
        writer
            .write_f64(authored.appearance.background_luminance_ratio_yb_yw)
            .unwrap();
        writer
            .write_u8(match authored.appearance.surround {
                AuthoredSurroundV1::Average => 0,
                AuthoredSurroundV1::Dim => 1,
                AuthoredSurroundV1::Dark => 2,
            })
            .unwrap();
        for scenario in authored.scenarios {
            writer.write_u32(scenario.id).unwrap();
            writer.write_rgb(scenario.page).unwrap();
        }
        writer.write_rgb(authored.expected_final_visible).unwrap();
        writer.write_u32(authored.stream_id).unwrap();
        writer.write_u64(authored.observation_revision).unwrap();
        writer.write_u32(authored.sink_output).unwrap();
        writer.finish().unwrap();
        bytes
    }

    fn decode_result_for_test(
        bytes: &[u8; PRIVATE_FIXTURE_RESULT_V1_LEN],
    ) -> CertifiedPrivateFixtureResultV1 {
        let mut reader = WireReaderV1::new(bytes);
        assert_eq!(
            reader.read_bytes::<MAGIC_WIDTH>().unwrap(),
            PRIVATE_FIXTURE_RESULT_V1_MAGIC
        );
        assert_eq!(reader.read_u16().unwrap(), PRIVATE_FIXTURE_ABI_VERSION_V1);
        assert_eq!(
            reader.read_u16().unwrap(),
            wire_len_u16(PRIVATE_FIXTURE_RESULT_V1_LEN).unwrap()
        );
        let result = CertifiedPrivateFixtureResultV1 {
            output: reader.read_u32().unwrap(),
            sink_output: reader.read_u32().unwrap(),
            selected_state_index: reader.read_u32().unwrap(),
            paint_source: reader.read_rgb().unwrap(),
            paint_opacity_bits: reader.read_u64().unwrap(),
            content_identity: reader.read_bytes::<IDENTITY_WIDTH>().unwrap(),
            selection_release_identity: reader.read_bytes::<IDENTITY_WIDTH>().unwrap(),
        };
        reader.finish().unwrap();
        result
    }

    struct RunDetailsV1 {
        status: u32,
        result: [u8; PRIVATE_FIXTURE_RESULT_V1_LEN],
        selected_state_index: Option<usize>,
    }

    fn run_details(authored: AuthoredPrivateFixtureV1) -> RunDetailsV1 {
        let generation = 1;
        let (host, oracle) = native_host(generation);
        let executed = match execute_private_fixture_v1(authored, host) {
            Ok(executed) => executed,
            Err(error) => {
                return RunDetailsV1 {
                    status: error.status(),
                    result: [0; PRIVATE_FIXTURE_RESULT_V1_LEN],
                    selected_state_index: None,
                };
            }
        };
        let ExecutedPrivateFixtureV1 {
            attachment,
            projection,
        } = executed;
        let selected_state_index = projection
            .as_ref()
            .ok()
            .map(|certified| certified.selected_state_index as usize);
        let (status, result) = match projection {
            Ok(certified) => (0, encode_result_v1(certified)),
            Err(error) => (error.status(), [0; PRIVATE_FIXTURE_RESULT_V1_LEN]),
        };
        mark_js_disposed(&oracle, generation);
        let mut attachment = Some(attachment);
        HandoffAttachmentV1::<NativeFixtureHostV1>::confirm_and_consume_external_dispose(
            &mut attachment,
            || confirm_tombstone(&oracle, generation, generation),
        )
        .unwrap_or_else(|_| panic!("the confirmation must bind the same writer epoch"));
        assert!(attachment.is_none());
        assert_eq!(oracle.borrow().invalid_drop_count, 0);
        RunDetailsV1 {
            status,
            result,
            selected_state_index,
        }
    }

    fn run(authored: AuthoredPrivateFixtureV1) -> (u32, [u8; PRIVATE_FIXTURE_RESULT_V1_LEN]) {
        let details = run_details(authored);
        (details.status, details.result)
    }

    fn selected_candidate_ids(
        authored: AuthoredPrivateFixtureV1,
        selected_state_index: Option<usize>,
    ) -> Option<(u32, u32)> {
        let selection = materialise_selection_v1(&authored).ok()?;
        let state = selection.order().states().get(selected_state_index?)?;
        let mut fill = None;
        let mut label = None;
        for choice in state.choices() {
            if choice.target().value() == FILL_TARGET.value() {
                fill = Some(choice.candidate().value());
            } else if choice.target().value() == LABEL_TARGET.value() {
                label = Some(choice.candidate().value());
            }
        }
        Some((fill?, label?))
    }

    #[test]
    fn fixed_wire_offsets_end_exactly_at_the_grammar_derived_lengths() {
        let request = encode_request_for_test(valid_authored());
        assert!(decode_request_v1(&request).is_ok());

        let certified = CertifiedPrivateFixtureResultV1 {
            output: 1,
            sink_output: 2,
            selected_state_index: 3,
            paint_source: Srgb8::new([3, 4, 5]),
            paint_opacity_bits: 1.0_f64.to_bits(),
            content_identity: [6; IDENTITY_WIDTH],
            selection_release_identity: [7; IDENTITY_WIDTH],
        };
        assert_eq!(encode_result_v1(certified).len(), RESULT_END_OFFSET);
    }

    #[test]
    fn production_entry_returns_only_the_attachment_certified_projection() {
        let details = run_details(valid_authored());
        assert_eq!(details.status, 0);
        assert_eq!(details.selected_state_index, Some(3));
        let result = decode_result_for_test(&details.result);
        assert_eq!(result.output, OUTPUT.value());
        assert_eq!(result.sink_output, 501);
        assert_eq!(result.selected_state_index, 3);
        assert_eq!(result.paint_source, Srgb8::new([64, 64, 64]));
        assert_eq!(result.paint_opacity_bits, 0.5_f64.to_bits());
        assert_ne!(result.content_identity, [0; IDENTITY_WIDTH]);
        assert_ne!(result.selection_release_identity, [0; IDENTITY_WIDTH]);
    }

    #[test]
    fn caller_authored_values_change_render_and_content_but_not_the_same_release() {
        let (first_status, first) = run(valid_authored());
        assert_eq!(first_status, 0);
        let first = decode_result_for_test(&first);

        let mut changed = valid_authored();
        changed.brand = Srgb8::new([192, 192, 192]);
        changed.label_candidates[0].source = changed.brand;
        changed.label_candidates[1].source = Srgb8::new([255, 255, 255]);
        changed.expected_final_visible = Srgb8::new([160, 160, 160]);
        let (second_status, second) = run(changed);
        assert_eq!(second_status, 0);
        let second = decode_result_for_test(&second);

        assert_ne!(first.paint_source, second.paint_source);
        assert_ne!(first.content_identity, second.content_identity);
        assert_eq!(
            first.selection_release_identity,
            second.selection_release_identity
        );
    }

    #[test]
    fn bad_header_zeroes_the_whole_stale_result_before_returning() {
        let mut request = encode_request_for_test(valid_authored());
        request[MAGIC_WIDTH] ^= 1;
        let mut result = [0xA5; PRIVATE_FIXTURE_RESULT_V1_LEN];
        let mut instance = PrivateFixtureInstanceV1::new();
        let (host, oracle) = native_host(1);
        let mut host = Some(host);
        let mut factory_calls = 0;
        let status = run_request_v1(&request, &mut result, &mut instance, |generation| {
            factory_calls += 1;
            assert_eq!(generation, 1);
            host.take().unwrap()
        });

        assert_eq!(status, PrivateFixtureErrorV1::UnsupportedVersion.status());
        assert_eq!(result, [0; PRIVATE_FIXTURE_RESULT_V1_LEN]);
        assert_eq!(factory_calls, 0);
        assert!(matches!(
            instance.lifecycle,
            PrivateFixtureLifecycleV1::Vacant
        ));
        assert!(oracle.borrow().installs.is_empty());
        assert_eq!(oracle.borrow().local_drop_count, 0);

        let declared_length_offset = MAGIC_WIDTH + U16_WIDTH;
        request = encode_request_for_test(valid_authored());
        request[declared_length_offset] ^= 1;
        result.fill(0xA5);
        let status = run_request_v1(&request, &mut result, &mut instance, |generation| {
            factory_calls += 1;
            assert_eq!(generation, 1);
            host.take().unwrap()
        });
        assert_eq!(status, PrivateFixtureErrorV1::InvalidLength.status());
        assert_eq!(result, [0; PRIVATE_FIXTURE_RESULT_V1_LEN]);
        assert_eq!(factory_calls, 0);
        assert!(matches!(
            instance.lifecycle,
            PrivateFixtureLifecycleV1::Vacant
        ));
        assert!(oracle.borrow().installs.is_empty());
        assert_eq!(oracle.borrow().local_drop_count, 0);
    }

    #[test]
    fn duplicate_opaque_candidate_key_is_rejected_by_selection_release_admission() {
        let mut authored = valid_authored();
        authored.joint_states[1].key = authored.joint_states[0].key;
        let (status, result) = run(authored);

        assert_eq!(
            status,
            PrivateFixtureErrorV1::SelectionReleaseRejected.status()
        );
        assert_eq!(result, [0; PRIVATE_FIXTURE_RESULT_V1_LEN]);
    }

    #[test]
    fn no_feasible_state_never_returns_or_leaves_a_render_claim() {
        let mut authored = valid_authored();
        authored.expected_final_visible = Srgb8::new([1, 1, 1]);
        let (status, result) = run(authored);

        assert_eq!(
            status,
            PrivateFixtureErrorV1::MissingCertifiedOutput.status()
        );
        assert_eq!(result, [0; PRIVATE_FIXTURE_RESULT_V1_LEN]);
    }

    #[test]
    fn each_observed_backdrop_is_independently_necessary_for_the_selected_state() {
        let authored = valid_authored();
        let both = run_details(authored);
        let mut black_twice = valid_authored();
        black_twice.scenarios[1].page = black_twice.scenarios[0].page;
        let black_only = run_details(black_twice);
        let mut white_twice = valid_authored();
        white_twice.scenarios[0].page = white_twice.scenarios[1].page;
        let white_only = run_details(white_twice);

        assert_eq!(both.status, 0);
        assert_eq!(both.selected_state_index, Some(3));
        assert_eq!(black_only.status, 0);
        assert_eq!(black_only.selected_state_index, Some(2));
        assert_eq!(white_only.status, 0);
        assert_eq!(white_only.selected_state_index, Some(1));
        assert_eq!(
            selected_candidate_ids(authored, both.selected_state_index),
            Some((12, 20))
        );
        assert_eq!(
            selected_candidate_ids(black_twice, black_only.selected_state_index),
            Some((11, 20))
        );
        assert_eq!(
            selected_candidate_ids(white_twice, white_only.selected_state_index),
            Some((10, 20))
        );
    }

    #[test]
    fn authored_opacity_edge_is_necessary_for_any_certified_state() {
        let mut without_attenuation = valid_authored();
        without_attenuation.label_opacity = 1.0;
        let result = run_details(without_attenuation);

        assert_eq!(
            result.status,
            PrivateFixtureErrorV1::MissingCertifiedOutput.status()
        );
        assert_eq!(result.result, [0; PRIVATE_FIXTURE_RESULT_V1_LEN]);
        assert_eq!(result.selected_state_index, None);
    }

    #[test]
    fn two_phase_dispose_retains_attachment_until_same_generation_tombstone() {
        let mut instance = PrivateFixtureInstanceV1::new();
        let generation = instance.begin_run().unwrap();
        let (host, oracle) = native_host(generation);
        let executed = execute_private_fixture_v1(valid_authored(), host).unwrap();
        assert!(instance.complete_run(generation, executed).unwrap().is_ok());

        let token = instance.begin_dispose().unwrap();
        let retained_address = match &instance.lifecycle {
            PrivateFixtureLifecycleV1::Disposing { attachment, .. } => {
                core::ptr::from_ref(attachment.as_ref().expect("disposing owns attachment"))
                    as usize
            }
            _ => panic!("begin must enter Disposing"),
        };
        assert_eq!(token, generation);
        assert_eq!(
            instance.begin_run(),
            Err(PrivateFixtureErrorV1::AlreadyActive)
        );
        assert_eq!(
            instance.commit_dispose(token, |generation, token| {
                confirm_tombstone(&oracle, generation, token)
            }),
            Err(PrivateFixtureErrorV1::DisposeNotConfirmed)
        );
        assert_eq!(oracle.borrow().local_drop_count, 0);
        assert_eq!(
            match &instance.lifecycle {
                PrivateFixtureLifecycleV1::Disposing { attachment, .. } => {
                    core::ptr::from_ref(attachment.as_ref().expect("disposing owns attachment"))
                        as usize
                }
                _ => panic!("failed confirmation must retain Disposing"),
            },
            retained_address,
        );
        assert_eq!(
            instance.abort_dispose(token.wrapping_add(1)),
            Err(PrivateFixtureErrorV1::InvalidDisposeToken)
        );
        instance.abort_dispose(token).unwrap();
        assert_eq!(oracle.borrow().local_drop_count, 0);

        let retry_token = instance.begin_dispose().unwrap();
        assert_eq!(retry_token, token);
        mark_js_disposed(&oracle, generation);
        instance
            .commit_dispose(retry_token, |generation, token| {
                confirm_tombstone(&oracle, generation, token)
            })
            .unwrap();
        let oracle = oracle.borrow();
        assert_eq!(oracle.local_drop_count, 1);
        assert_eq!(oracle.invalid_drop_count, 0);
    }

    #[test]
    fn panicking_dispose_confirmation_leaves_the_identical_disposing_state() {
        let mut instance = PrivateFixtureInstanceV1::new();
        let generation = instance.begin_run().unwrap();
        let (host, oracle) = native_host(generation);
        let executed = execute_private_fixture_v1(valid_authored(), host).unwrap();
        assert!(instance.complete_run(generation, executed).unwrap().is_ok());
        let token = instance.begin_dispose().unwrap();
        let retained_address = match &instance.lifecycle {
            PrivateFixtureLifecycleV1::Disposing { attachment, .. } => {
                core::ptr::from_ref(attachment.as_ref().expect("disposing owns attachment"))
                    as usize
            }
            _ => panic!("begin must enter Disposing"),
        };

        let trapped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = instance.commit_dispose(token, |_, _| {
                panic!("host confirmation trap");
            });
        }));
        assert!(trapped.is_err());
        assert_eq!(oracle.borrow().local_drop_count, 0);
        assert_eq!(
            match &instance.lifecycle {
                PrivateFixtureLifecycleV1::Disposing {
                    generation: retained_generation,
                    token: retained_token,
                    attachment,
                } => {
                    assert_eq!(*retained_generation, generation);
                    assert_eq!(*retained_token, token);
                    core::ptr::from_ref(attachment.as_ref().expect("disposing owns attachment"))
                        as usize
                }
                _ => panic!("a confirmation trap must retain Disposing"),
            },
            retained_address,
        );

        mark_js_disposed(&oracle, generation);
        instance
            .commit_dispose(token, |generation, token| {
                confirm_tombstone(&oracle, generation, token)
            })
            .unwrap();
        assert_eq!(oracle.borrow().invalid_drop_count, 0);
    }

    #[test]
    fn stale_tombstone_cannot_release_the_next_generation() {
        let mut instance = PrivateFixtureInstanceV1::new();
        let first_generation = instance.begin_run().unwrap();
        let (first_host, first_oracle) = native_host(first_generation);
        let first = execute_private_fixture_v1(valid_authored(), first_host).unwrap();
        assert!(
            instance
                .complete_run(first_generation, first)
                .unwrap()
                .is_ok()
        );
        let first_token = instance.begin_dispose().unwrap();
        mark_js_disposed(&first_oracle, first_generation);
        instance
            .commit_dispose(first_token, |generation, token| {
                confirm_tombstone(&first_oracle, generation, token)
            })
            .unwrap();

        let second_generation = instance.begin_run().unwrap();
        assert_ne!(second_generation, first_generation);
        let (second_host, second_oracle) = native_host(second_generation);
        let second = execute_private_fixture_v1(valid_authored(), second_host).unwrap();
        assert!(
            instance
                .complete_run(second_generation, second)
                .unwrap()
                .is_ok()
        );
        let second_token = instance.begin_dispose().unwrap();
        assert_eq!(
            instance.commit_dispose(second_token, |generation, token| {
                confirm_tombstone(&first_oracle, generation, token)
            }),
            Err(PrivateFixtureErrorV1::DisposeNotConfirmed)
        );
        assert_eq!(second_oracle.borrow().generation, Some(second_generation));
        assert_eq!(second_oracle.borrow().local_drop_count, 0);

        mark_js_disposed(&second_oracle, second_generation);
        instance
            .commit_dispose(second_token, |generation, token| {
                confirm_tombstone(&second_oracle, generation, token)
            })
            .unwrap();
        assert_eq!(second_oracle.borrow().invalid_drop_count, 0);
    }

    #[test]
    fn begin_dispose_distinguishes_vacant_from_illegal_lifecycle_states() {
        let mut instance = PrivateFixtureInstanceV1::new();

        // Vacant is the legitimate "nothing to dispose" state: the ABI maps it
        // to the documented 0 sentinel (a generation token is never 0).
        assert_eq!(
            instance.begin_dispose(),
            Err(PrivateFixtureErrorV1::InvalidLifecycle)
        );

        // A Running lifecycle is an internal protocol violation: no caller may
        // race a live run. It must be typed as an invariant, never as Vacant,
        // and the lifecycle must survive the failed transition.
        instance.begin_run().unwrap();
        assert_eq!(
            instance.begin_dispose(),
            Err(PrivateFixtureErrorV1::InternalInvariant)
        );
        assert!(matches!(
            instance.lifecycle,
            PrivateFixtureLifecycleV1::Running
        ));
        instance.fail_run().unwrap();

        // A Disposing lifecycle owns the attachment: a second begin_dispose is
        // an equally illegal state and must not consume or reclassify it.
        let generation = instance.begin_run().unwrap();
        let (host, oracle) = native_host(generation);
        let executed = execute_private_fixture_v1(valid_authored(), host).unwrap();
        assert!(instance.complete_run(generation, executed).unwrap().is_ok());
        let token = instance.begin_dispose().unwrap();
        let retained_address = match &instance.lifecycle {
            PrivateFixtureLifecycleV1::Disposing { attachment, .. } => {
                core::ptr::from_ref(attachment.as_ref().expect("disposing owns attachment"))
                    as usize
            }
            _ => panic!("begin must enter Disposing"),
        };
        assert_eq!(
            instance.begin_dispose(),
            Err(PrivateFixtureErrorV1::InternalInvariant)
        );
        match &instance.lifecycle {
            PrivateFixtureLifecycleV1::Disposing {
                generation: retained_generation,
                token: retained_token,
                attachment,
            } => {
                assert_eq!(*retained_generation, generation);
                assert_eq!(*retained_token, token);
                assert_eq!(
                    core::ptr::from_ref(attachment.as_ref().expect("attachment must be retained"))
                        as usize,
                    retained_address,
                    "a failed begin on Disposing must keep the same attachment",
                );
            }
            _ => panic!("a failed begin on Disposing must retain Disposing"),
        }
        assert_eq!(oracle.borrow().local_drop_count, 0);
    }

    #[test]
    fn begin_dispose_status_v1_preserves_the_documented_wire_sentinels() {
        // A live generation token is returned verbatim.
        assert_eq!(begin_dispose_status_v1(Ok(7)), 7);
        // Vacant keeps the reserved 0 sentinel: generations start at 1, so 0
        // can never be confused with a successful token.
        assert_eq!(
            begin_dispose_status_v1(Err(PrivateFixtureErrorV1::InvalidLifecycle)),
            0
        );
        // An invariant violation is distinguishable and fail-closed: the
        // consumer must never read it as "no active attachment".
        assert_eq!(
            begin_dispose_status_v1(Err(PrivateFixtureErrorV1::InternalInvariant)),
            PrivateFixtureErrorV1::InternalInvariant.status()
        );
        assert_ne!(
            begin_dispose_status_v1(Err(PrivateFixtureErrorV1::InternalInvariant)),
            0
        );
    }

    struct HostileFixedBufferHostV1 {
        gate: *const PrivateFixtureAbiGateV1,
        request: *mut [u8; PRIVATE_FIXTURE_REQUEST_V1_LEN],
        result: *mut [u8; PRIVATE_FIXTURE_RESULT_V1_LEN],
        instance: *mut PrivateFixtureInstanceV1<Self>,
        nested_status: Rc<Cell<Option<u32>>>,
    }

    impl HandoffPointSinkHostV1 for HostileFixedBufferHostV1 {
        fn try_install(
            &mut self,
            _intent: HandoffPointSinkHostIntentV1,
        ) -> Result<(), HandoffPointSinkHostErrorV1> {
            // SAFETY: the outer entry deliberately holds no references to its
            // exported-equivalent buffer cells while this callback runs.
            unsafe {
                self.request.write([0xCC; PRIVATE_FIXTURE_REQUEST_V1_LEN]);
                self.result.write([0xDD; PRIVATE_FIXTURE_RESULT_V1_LEN]);
            }
            // SAFETY: this uses the identical entry wrapper and gate. Busy is
            // returned before the aliased INSTANCE raw pointer is dereferenced.
            let nested = unsafe {
                run_fixed_buffer_entry_v1(
                    &*self.gate,
                    self.request,
                    self.result,
                    self.instance,
                    |_| panic!("a Busy nested call must not construct a host"),
                )
            };
            self.nested_status.set(Some(nested));
            Ok(())
        }
    }

    #[test]
    fn fixed_buffer_entry_isolates_host_mutation_and_nested_reentry() {
        let gate = PrivateFixtureAbiGateV1::new();
        let request = UnsafeCell::new(encode_request_for_test(valid_authored()));
        let result = UnsafeCell::new([0xA5; PRIVATE_FIXTURE_RESULT_V1_LEN]);
        let mut instance = PrivateFixtureInstanceV1::<HostileFixedBufferHostV1>::new();
        let instance_ptr = &raw mut instance;
        let nested_status = Rc::new(Cell::new(None));

        // SAFETY: local cells are valid, disjoint, and outlive the call. The
        // callback receives only their raw pointers and the same guarded entry.
        let status = unsafe {
            run_fixed_buffer_entry_v1(&gate, request.get(), result.get(), instance_ptr, |_| {
                HostileFixedBufferHostV1 {
                    gate: &raw const gate,
                    request: request.get(),
                    result: result.get(),
                    instance: instance_ptr,
                    nested_status: Rc::clone(&nested_status),
                }
            })
        };

        assert_eq!(status, 0);
        assert_eq!(
            nested_status.get(),
            Some(PrivateFixtureErrorV1::Busy.status())
        );
        // SAFETY: the synchronous entry and all callbacks have returned.
        let hostile_request = unsafe { request.get().read() };
        let certified_result = unsafe { result.get().read() };
        assert_eq!(
            hostile_request, [0xCC; PRIVATE_FIXTURE_REQUEST_V1_LEN],
            "the host mutation must hit only backing memory, not the snapshot",
        );
        assert_ne!(
            certified_result, [0xDD; PRIVATE_FIXTURE_RESULT_V1_LEN],
            "the staged result must overwrite the hostile backing write",
        );
        assert_eq!(
            decode_result_for_test(&certified_result).selected_state_index,
            3
        );
    }
}
