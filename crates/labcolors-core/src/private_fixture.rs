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
    AppearanceContextV1, ConstraintIdV1, DraftV1, OccurrenceIdV1, OpacityInputIdV1, OutputSlotIdV1,
    PaintIdV1, PaintValueV1, PresentationRootIdV1, ScenarioV1, SourceIdV1, SurfaceIdV1,
    SurfaceInputPortIdV1, SurroundV1, TargetIdV1, UpdateV1,
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
const APPEARANCE_WIDTH: usize = F64_WIDTH + F64_WIDTH + BYTE_WIDTH;

const PRIVATE_FIXTURE_REQUEST_V1_LEN: usize =
    HEADER_WIDTH + RGB_WIDTH + F64_WIDTH + RGB_WIDTH + APPEARANCE_WIDTH + RGB_WIDTH + U32_WIDTH;
const PRIVATE_FIXTURE_RESULT_V1_LEN: usize =
    HEADER_WIDTH + U32_WIDTH + U32_WIDTH + RGB_WIDTH + U64_WIDTH + IDENTITY_WIDTH;
const _: () = assert!(PRIVATE_FIXTURE_REQUEST_V1_LEN <= u16::MAX as usize);
const _: () = assert!(PRIVATE_FIXTURE_RESULT_V1_LEN <= u16::MAX as usize);

const PRIVATE_FIXTURE_REQUEST_V1_MAGIC: [u8; MAGIC_WIDTH] = *b"LCFQ";
const PRIVATE_FIXTURE_RESULT_V1_MAGIC: [u8; MAGIC_WIDTH] = *b"LCFR";
const PRIVATE_FIXTURE_ABI_VERSION_V1: u16 = 1;

const AUTHORED_SOURCE: SourceIdV1 = SourceIdV1::new(1);
const FIXED_TARGET: TargetIdV1 = TargetIdV1::new(2);
const SOLID_PAINT: PaintIdV1 = PaintIdV1::new(3);
const OUTPUT_PAINT: PaintIdV1 = PaintIdV1::new(4);
const OPACITY_INPUT: OpacityInputIdV1 = OpacityInputIdV1::new(5);
const FROZEN_SURFACE_INPUT: SurfaceInputPortIdV1 = SurfaceInputPortIdV1::new(6);
const FROZEN_SURFACE: SurfaceIdV1 = SurfaceIdV1::new(7);
const OUTPUT_OCCURRENCE: OccurrenceIdV1 = OccurrenceIdV1::new(8);
const PRESENTATION_ROOT: PresentationRootIdV1 = PresentationRootIdV1::new(9);
const FINAL_VISIBLE_IDENTITY: ConstraintIdV1 = ConstraintIdV1::new(10);
const OUTPUT: OutputSlotIdV1 = OutputSlotIdV1::new(17);
// Static resolution still needs attachment-local provenance, but neither value
// is authored or observable through this artifact. PR E will introduce the
// runtime stream/revision contract instead of retrofitting semantics onto these
// private one-shot identities.
const STATIC_RESOLUTION_SCOPE: u32 = 1;
const STATIC_RESOLVE_REVISION: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrivateFixtureErrorV1 {
    InvalidMagic,
    UnsupportedVersion,
    InvalidLength,
    InvalidAuthoredData,
    ProgramCompileRejected,
    AttachmentRejected,
    UpdateRejected,
    MissingCertifiedOutput,
    MultipleCertifiedOutputs,
    InternalInvariant,
    Busy,
    AlreadyActive,
    InvalidLifecycle,
    InvalidDisposeToken,
    DisposeNotConfirmed,
}

impl PrivateFixtureErrorV1 {
    const fn status(self) -> u32 {
        match self {
            Self::InvalidMagic => 1,
            Self::UnsupportedVersion => 2,
            Self::InvalidLength => 3,
            Self::InvalidAuthoredData => 4,
            Self::ProgramCompileRejected => 5,
            Self::AttachmentRejected => 6,
            Self::UpdateRejected => 7,
            Self::MissingCertifiedOutput => 8,
            Self::MultipleCertifiedOutputs => 9,
            Self::InternalInvariant => 10,
            Self::Busy => 11,
            Self::AlreadyActive => 12,
            Self::InvalidLifecycle => 13,
            Self::InvalidDisposeToken => 14,
            Self::DisposeNotConfirmed => 15,
        }
    }
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
struct AuthoredPrivateFixtureV1 {
    source: Srgb8,
    opacity: f64,
    frozen_backdrop: Srgb8,
    appearance: AuthoredAppearanceV1,
    expected_final_visible: Srgb8,
    sink_output: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CertifiedPrivateFixtureResultV1 {
    output: u32,
    sink_output: u32,
    paint_source: Srgb8,
    paint_opacity_bits: u64,
    content_identity: [u8; IDENTITY_WIDTH],
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

    let source = reader.read_rgb()?;
    let opacity = reader.read_f64()?;
    let frozen_backdrop = reader.read_rgb()?;
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
    let expected_final_visible = reader.read_rgb()?;
    let sink_output = reader.read_u32()?;
    reader.finish()?;

    Ok(AuthoredPrivateFixtureV1 {
        source,
        opacity,
        frozen_backdrop,
        appearance,
        expected_final_visible,
        sink_output,
    })
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
    PaintValueV1::try_new(authored.source, authored.opacity)
        .map_err(|_| PrivateFixtureErrorV1::InvalidAuthoredData)?;
    let context = AppearanceContextV1::try_new(
        authored.appearance.adapting_luminance_cd_m2,
        authored.appearance.background_luminance_ratio_yb_yw,
        authored.appearance.surround.into_program(),
    )
    .map_err(|_| PrivateFixtureErrorV1::InvalidAuthoredData)?;
    let mut draft = DraftV1::new();
    draft.push_source(AUTHORED_SOURCE, authored.source);
    draft.push_fixed_target(FIXED_TARGET, AUTHORED_SOURCE);
    draft.push_solid_paint(SOLID_PAINT, FIXED_TARGET);
    draft.push_opacity_input(OPACITY_INPUT, authored.opacity);
    draft.push_opacity_paint(OUTPUT_PAINT, SOLID_PAINT, OPACITY_INPUT);
    draft.push_surface_input_port(FROZEN_SURFACE_INPUT);
    draft.push_input_surface(FROZEN_SURFACE, FROZEN_SURFACE_INPUT);
    draft.push_source_over_occurrence(OUTPUT_OCCURRENCE, OUTPUT_PAINT, FROZEN_SURFACE, context);
    draft.push_point_presentation_root(PRESENTATION_ROOT, OUTPUT_OCCURRENCE);
    draft.push_point_presentation_target(PRESENTATION_ROOT, OUTPUT_OCCURRENCE);
    draft.push_exact_visible_unary_hard(
        FINAL_VISIBLE_IDENTITY,
        OUTPUT_OCCURRENCE,
        authored.expected_final_visible,
    );
    draft.push_output(OUTPUT, OUTPUT_PAINT);

    let owner = draft
        .compile()
        .map_err(|_| PrivateFixtureErrorV1::ProgramCompileRejected)?;
    let sink_output = HandoffPointSinkOutputIdV1::new(authored.sink_output);
    let emissions = [AuthoredPointEmissionBindingV1::new(OUTPUT, sink_output)];
    let presentations = [AuthoredPointPresentationBindingV1::new(
        OUTPUT,
        PRESENTATION_ROOT,
        OUTPUT_OCCURRENCE,
    )];
    let mut attachment = owner
        .attach_external(
            STATIC_RESOLUTION_SCOPE,
            &emissions,
            &presentations,
            FamilyArtifactBundleV2::empty(),
            handoff_point_sink(sink_output, host),
        )
        .map_err(|_| PrivateFixtureErrorV1::AttachmentRejected)?;

    let frozen_surface = [authored.frozen_backdrop];
    let scenarios = [ScenarioV1::new(1, &frozen_surface)];
    let commit = match attachment.update(UpdateV1::Observed {
        revision: STATIC_RESOLVE_REVISION,
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
    if certificate.selection_release_identity().is_some()
        || certificate.selected_state_index().is_some()
    {
        return Err(PrivateFixtureErrorV1::InternalInvariant);
    }
    let paint = render.paint();
    Ok(CertifiedPrivateFixtureResultV1 {
        output: render.output().value(),
        sink_output: render.sink_output().value(),
        paint_source: paint.source(),
        paint_opacity_bits: paint.opacity_bits(),
        content_identity: *certificate.content_identity().as_bytes(),
    })
}

/// Живые dispose-token'и кодируются в зарезервированный диапазон
/// [DISPOSE_TOKEN_BASE_V1, 2 * DISPOSE_TOKEN_BASE_V1 - 1], который не
/// пересекается ни с одним `PrivateFixtureErrorV1` status-кодом (1..=15), ни
/// со sentinel'ами Vacant `0` и Busy `u32::MAX`. Поэтому consumer однозначно
/// классифицирует u32 из `begin_dispose_v1`: значение вне живого диапазона —
/// это Vacant, Busy или типизированный status, но никогда живой token.
/// Generation начинается с 1 и растёт на каждый run; generation >= BASE
/// закодировать непересекающимся образом нельзя, поэтому такая проекция
/// fail-closed в типизированный status.
const DISPOSE_TOKEN_BASE_V1: u32 = 0x1000_0000;
const DISPOSE_TOKEN_ENCODED_END_V1: u32 = 2 * DISPOSE_TOKEN_BASE_V1 - 1;

const fn encode_dispose_token_v1(token: u32) -> u32 {
    if token >= DISPOSE_TOKEN_BASE_V1 {
        // Generation, которую нельзя закодировать непересекающимся образом, —
        // это нарушение инварианта; такой token не должен быть принят за
        // живой, поэтому проекция fail-closed в типизированный status.
        return PrivateFixtureErrorV1::InternalInvariant.status();
    }
    DISPOSE_TOKEN_BASE_V1 + token
}

/// Декодирует wire-token; `None` — для значения вне живого диапазона
/// (сырой generation, status-код или sentinel), чтобы abort/commit завершались
/// fail-closed `InvalidDisposeToken`, а не принимали поддельный или устаревший
/// token.
const fn decode_dispose_token_v1(token: u32) -> Option<u32> {
    match token {
        DISPOSE_TOKEN_BASE_V1..=DISPOSE_TOKEN_ENCODED_END_V1 => Some(token - DISPOSE_TOKEN_BASE_V1),
        _ => None,
    }
}

/// Проецирует исход `begin_dispose` в фиксированный wire-контракт v1: живой
/// token кодируется смещением в зарезервированный диапазон, Vacant
/// (`InvalidLifecycle`) — как зарезервированный sentinel `0` (generation
/// никогда не равен нулю), а любое инвариантное нарушение — как его
/// типизированный status, чтобы consumer не принял незаконный lifecycle за
/// отсутствие attachment и не спутал живой token со status-кодом.
const fn begin_dispose_status_v1(result: Result<u32, PrivateFixtureErrorV1>) -> u32 {
    match result {
        Ok(token) => encode_dispose_token_v1(token),
        Err(PrivateFixtureErrorV1::InvalidLifecycle) => 0,
        Err(error) => error.status(),
    }
}

const RESULT_MAGIC_OFFSET: usize = 0;
const RESULT_VERSION_OFFSET: usize = RESULT_MAGIC_OFFSET + MAGIC_WIDTH;
const RESULT_LENGTH_OFFSET: usize = RESULT_VERSION_OFFSET + U16_WIDTH;
const RESULT_OUTPUT_OFFSET: usize = RESULT_LENGTH_OFFSET + U16_WIDTH;
const RESULT_SINK_OUTPUT_OFFSET: usize = RESULT_OUTPUT_OFFSET + U32_WIDTH;
const RESULT_RGB_OFFSET: usize = RESULT_SINK_OUTPUT_OFFSET + U32_WIDTH;
const RESULT_OPACITY_OFFSET: usize = RESULT_RGB_OFFSET + RGB_WIDTH;
const RESULT_CONTENT_IDENTITY_OFFSET: usize = RESULT_OPACITY_OFFSET + U64_WIDTH;
const RESULT_END_OFFSET: usize = RESULT_CONTENT_IDENTITY_OFFSET + IDENTITY_WIDTH;
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
    bytes[RESULT_SINK_OUTPUT_OFFSET..RESULT_RGB_OFFSET]
        .copy_from_slice(&result.sink_output.to_le_bytes());
    bytes[RESULT_RGB_OFFSET..RESULT_OPACITY_OFFSET].copy_from_slice(&result.paint_source.bytes());
    bytes[RESULT_OPACITY_OFFSET..RESULT_CONTENT_IDENTITY_OFFSET]
        .copy_from_slice(&result.paint_opacity_bits.to_le_bytes());
    bytes[RESULT_CONTENT_IDENTITY_OFFSET..RESULT_END_OFFSET]
        .copy_from_slice(&result.content_identity);
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
        let Some(token) = decode_dispose_token_v1(token) else {
            return PrivateFixtureErrorV1::InvalidDisposeToken.status();
        };
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
        let Some(token) = decode_dispose_token_v1(token) else {
            return PrivateFixtureErrorV1::InvalidDisposeToken.status();
        };
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
        AuthoredPrivateFixtureV1 {
            source: Srgb8::new([64, 64, 64]),
            opacity: 0.5,
            frozen_backdrop: Srgb8::new([128, 128, 128]),
            appearance: AuthoredAppearanceV1 {
                adapting_luminance_cd_m2: 64.0,
                background_luminance_ratio_yb_yw: 0.2,
                surround: AuthoredSurroundV1::Dim,
            },
            expected_final_visible: Srgb8::new([96, 96, 96]),
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
        writer.write_rgb(authored.source).unwrap();
        writer.write_f64(authored.opacity).unwrap();
        writer.write_rgb(authored.frozen_backdrop).unwrap();
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
        writer.write_rgb(authored.expected_final_visible).unwrap();
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
            paint_source: reader.read_rgb().unwrap(),
            paint_opacity_bits: reader.read_u64().unwrap(),
            content_identity: reader.read_bytes::<IDENTITY_WIDTH>().unwrap(),
        };
        reader.finish().unwrap();
        result
    }

    struct RunDetailsV1 {
        status: u32,
        result: [u8; PRIVATE_FIXTURE_RESULT_V1_LEN],
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
                };
            }
        };
        let ExecutedPrivateFixtureV1 {
            attachment,
            projection,
        } = executed;
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
        RunDetailsV1 { status, result }
    }

    fn run(authored: AuthoredPrivateFixtureV1) -> (u32, [u8; PRIVATE_FIXTURE_RESULT_V1_LEN]) {
        let details = run_details(authored);
        (details.status, details.result)
    }

    #[test]
    fn static_fixture_certifies_fixed_paint_without_selection_authority() {
        let details = run_details(valid_authored());

        assert_eq!(details.status, 0);
        let result = decode_result_for_test(&details.result);
        assert_eq!(result.output, OUTPUT.value());
        assert_eq!(result.sink_output, 501);
        assert_eq!(result.paint_source, Srgb8::new([64, 64, 64]));
        assert_eq!(result.paint_opacity_bits, 0.5_f64.to_bits());
        assert_ne!(result.content_identity, [0; IDENTITY_WIDTH]);
    }

    #[test]
    fn static_fixture_fails_closed_when_final_materialized_render_violates_constraint() {
        let mut authored = valid_authored();
        authored.expected_final_visible = Srgb8::new([1, 1, 1]);

        let details = run_details(authored);

        assert_eq!(
            details.status,
            PrivateFixtureErrorV1::MissingCertifiedOutput.status()
        );
        assert_eq!(details.result, [0; PRIVATE_FIXTURE_RESULT_V1_LEN]);
    }

    #[test]
    fn fixed_wire_offsets_end_exactly_at_the_grammar_derived_lengths() {
        let request = encode_request_for_test(valid_authored());
        assert!(decode_request_v1(&request).is_ok());

        let certified = CertifiedPrivateFixtureResultV1 {
            output: 1,
            sink_output: 2,
            paint_source: Srgb8::new([3, 4, 5]),
            paint_opacity_bits: 1.0_f64.to_bits(),
            content_identity: [6; IDENTITY_WIDTH],
        };
        assert_eq!(encode_result_v1(certified).len(), RESULT_END_OFFSET);
    }

    #[test]
    fn production_entry_returns_only_the_attachment_certified_projection() {
        let details = run_details(valid_authored());
        assert_eq!(details.status, 0);
        let result = decode_result_for_test(&details.result);
        assert_eq!(result.output, OUTPUT.value());
        assert_eq!(result.sink_output, 501);
        assert_eq!(result.paint_source, Srgb8::new([64, 64, 64]));
        assert_eq!(result.paint_opacity_bits, 0.5_f64.to_bits());
        assert_ne!(result.content_identity, [0; IDENTITY_WIDTH]);
    }

    #[test]
    fn caller_authored_values_change_render_and_content_identity() {
        let (first_status, first) = run(valid_authored());
        assert_eq!(first_status, 0);
        let first = decode_result_for_test(&first);

        let mut changed = valid_authored();
        changed.source = Srgb8::new([192, 192, 192]);
        changed.expected_final_visible = Srgb8::new([160, 160, 160]);
        let (second_status, second) = run(changed);
        assert_eq!(second_status, 0);
        let second = decode_result_for_test(&second);

        assert_ne!(first.paint_source, second.paint_source);
        assert_ne!(first.content_identity, second.content_identity);
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
    fn authored_opacity_edge_is_necessary_for_any_certified_state() {
        let mut without_attenuation = valid_authored();
        without_attenuation.opacity = 1.0;
        let result = run_details(without_attenuation);

        assert_eq!(
            result.status,
            PrivateFixtureErrorV1::MissingCertifiedOutput.status()
        );
        assert_eq!(result.result, [0; PRIVATE_FIXTURE_RESULT_V1_LEN]);
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
    fn begin_dispose_status_v1_keeps_live_tokens_disjoint_from_error_statuses() {
        // Every error status code is a small integer; a generation equal to one
        // of them must never be returned in the same range, otherwise a
        // consumer that classifies by range would misread the token as a
        // typed failure (e.g. InternalInvariant == 12 after twelve run/dispose
        // cycles) and leave the attachment Disposing forever.
        for status in 1..=PrivateFixtureErrorV1::DisposeNotConfirmed.status() {
            assert_ne!(
                begin_dispose_status_v1(Ok(status)),
                status,
                "live token {status} collides with an error status code"
            );
        }
        assert_eq!(begin_dispose_status_v1(Ok(12)), DISPOSE_TOKEN_BASE_V1 + 12);
        assert_ne!(
            begin_dispose_status_v1(Ok(12)),
            PrivateFixtureErrorV1::InternalInvariant.status()
        );
        // The reserved sentinels stay untouched: Vacant maps to 0, Busy and
        // typed errors stay outside the live-token range.
        assert_eq!(
            begin_dispose_status_v1(Err(PrivateFixtureErrorV1::InvalidLifecycle)),
            0
        );
        assert_eq!(
            begin_dispose_status_v1(Err(PrivateFixtureErrorV1::Busy)),
            PrivateFixtureErrorV1::Busy.status()
        );
    }

    #[test]
    fn dispose_token_wire_decoding_fails_closed_outside_the_live_range() {
        assert_eq!(
            decode_dispose_token_v1(DISPOSE_TOKEN_BASE_V1 + 12),
            Some(12)
        );
        assert_eq!(decode_dispose_token_v1(DISPOSE_TOKEN_BASE_V1), Some(0));
        // A raw generation, a status code, the Vacant sentinel, and the Busy
        // sentinel are never valid wire tokens: abort/commit must reject them
        // instead of comparing them against the internal token.
        assert_eq!(decode_dispose_token_v1(12), None);
        assert_eq!(decode_dispose_token_v1(0), None);
        assert_eq!(decode_dispose_token_v1(u32::MAX), None);
        assert_eq!(
            decode_dispose_token_v1(PrivateFixtureErrorV1::Busy.status()),
            None
        );
    }

    #[test]
    fn begin_dispose_status_v1_preserves_the_documented_wire_sentinels() {
        // A live generation token is encoded above the error status range.
        assert_eq!(begin_dispose_status_v1(Ok(7)), DISPOSE_TOKEN_BASE_V1 + 7);
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
            decode_result_for_test(&certified_result).paint_source,
            Srgb8::new([64, 64, 64])
        );
    }
}
