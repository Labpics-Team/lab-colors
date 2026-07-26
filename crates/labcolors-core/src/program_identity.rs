//! Устойчивый к коллизиям адрес исполняемого содержимого принятой Program.
//!
//! Paint/Surface/Occurrence способны образовывать двудольные графы
//! инцидентности, поэтому одного топологического хеша или уточнения разбиения
//! недостаточно. Модуль строит типизированный цветной граф, канонизирует его без
//! opaque ID и хеширует канонический прообраз.

use super::*;

const DOMAIN_V1: &[u8] = b"labcolors.program-content-identity.v1\0";
// Максимальный V1-цвет принадлежит Occurrence: теги вершины, композиции,
// контекста и frame, два binary64-параметра наблюдения и surround. Явная
// граница устраняет аллокацию на каждую вершину и требует пересмотра при
// расширении схемы вместо скрытого runtime-лимита.
const COLOR_CAPACITY: usize = 1 + 1 + 1 + 4 + 8 + 8 + 1;

mod release_tag {
    pub(super) const PROGRAM_SCHEMA_V1: u8 = 1;
    pub(super) const DECLARED_TOTAL_ORDER_V1: u8 = 1;
    pub(super) const FRESH_FULL_RECHECK_V1: u8 = 1;
    pub(super) const ATOMIC_OBSERVATION_GROUP_V1: u8 = 1;
    pub(super) const ENCODED_PAINT_EMISSION_V1: u8 = 1;
    pub(super) const MODELED_LCS_OCCURRENCE_V1: u8 = 1;

    pub(super) const IEC_SRGB8_D65_OUTPUT_PROFILE_V1: u8 = 1;
    pub(super) const IEC_SRGB8_TO_XYZ_D65_TRANSFORM_V1: u8 = 1;
    pub(super) const CIE1931_TWO_DEGREE_OBSERVER_V1: u8 = 1;
    pub(super) const IEC61966_D65_REFERENCE_WHITE_V1: u8 = 1;
    pub(super) const RELATIVE_Y1_SCALE_V1: u8 = 1;
    pub(super) const XYZ_FRAME_V1: u8 = 1;
    #[cfg(test)]
    pub(super) const MUTATION_SENTINEL_FRAME_V1: u8 = 2;
    pub(super) const CIECAM16_VIEWING_INPUTS_V1: u8 = 1;
    pub(super) const ENCODED_SRGB8_SOURCE_OVER_V1: u8 = 1;
    pub(super) const SURROUND_AVERAGE_V1: u8 = 1;
    pub(super) const SURROUND_DIM_V1: u8 = 2;
    pub(super) const SURROUND_DARK_V1: u8 = 3;

    pub(super) const EXACT_SRGB8_FAMILY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_IDENTITY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_RELEASE_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_CAPABILITY_V1: u8 = 1;
    #[cfg(test)]
    pub(super) const EXACT_SRGB8_IDENTITY_MUTATION_SENTINEL_V1: u8 = 2;
    #[cfg(test)]
    pub(super) const EXACT_SRGB8_RELEASE_MUTATION_SENTINEL_V1: u8 = 2;
    #[cfg(test)]
    pub(super) const EXACT_SRGB8_CAPABILITY_MUTATION_SENTINEL_V1: u8 = 2;
    pub(super) const WCAG22_SRGB8_FAMILY_V1: u8 = 2;
    pub(super) const WCAG22_SRGB8_IDENTITY_V1: u8 = 1;
    pub(super) const WCAG22_SRGB8_PROFILE_V1: u8 = 1;
    pub(super) const WCAG22_SRGB8_CAPABILITY_V1: u8 = 1;
    pub(super) const WCAG22_SC_1_4_3_TEXT_DEFAULT: u8 = 1;
    pub(super) const WCAG22_SC_1_4_3_TEXT_LARGE_SCALE: u8 = 2;
    pub(super) const WCAG22_SC_1_4_11_UI_COMPONENT_OR_STATE: u8 = 3;
    pub(super) const WCAG22_SC_1_4_11_GRAPHICAL_OBJECT: u8 = 4;
}

/// Устойчивый к коллизиям адрес канонизированного содержимого Program V1.
///
/// SHA-256 не делает адрес инъективным. Адрес не связывает пространства opaque
/// ID и не подтверждает владельца, поколение либо revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ProgramContentIdentityV1([u8; 32]);

impl ProgramContentIdentityV1 {
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct VertexColorV1 {
    len: u8,
    bytes: [u8; COLOR_CAPACITY],
}

impl VertexColorV1 {
    fn new(tag: u8) -> Self {
        let mut value = Self {
            len: 1,
            bytes: [0; COLOR_CAPACITY],
        };
        value.bytes[0] = tag;
        value
    }

    fn push_u8(&mut self, value: u8) -> Result<(), ProgramCompileError> {
        let index = usize::from(self.len);
        let slot = self
            .bytes
            .get_mut(index)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        *slot = value;
        self.len = self
            .len
            .checked_add(1)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        Ok(())
    }

    fn push_u64(&mut self, value: u64) -> Result<(), ProgramCompileError> {
        for byte in value.to_be_bytes() {
            self.push_u8(byte)?;
        }
        Ok(())
    }

    fn push_srgb8(&mut self, value: Srgb8) -> Result<(), ProgramCompileError> {
        for byte in value.bytes() {
            self.push_u8(byte)?;
        }
        Ok(())
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

mod vertex_tag {
    pub(super) const PROGRAM: u8 = 1;
    pub(super) const SOURCE: u8 = 2;
    pub(super) const TARGET_FIXED: u8 = 3;
    pub(super) const TARGET_FINITE: u8 = 4;
    pub(super) const CANDIDATE: u8 = 5;
    pub(super) const OPACITY: u8 = 6;
    pub(super) const PAINT_SOLID: u8 = 7;
    pub(super) const PAINT_OPACITY: u8 = 8;
    pub(super) const OBSERVATION_GROUP: u8 = 9;
    pub(super) const SURFACE_INPUT_PORT: u8 = 10;
    pub(super) const SURFACE_INPUT: u8 = 11;
    pub(super) const SURFACE_FROM_OCCURRENCE: u8 = 12;
    pub(super) const OCCURRENCE: u8 = 13;
    pub(super) const CONSTRAINT_HARD: u8 = 14;
    pub(super) const CONSTRAINT_REPORT_ONLY: u8 = 15;
    pub(super) const OUTPUT: u8 = 16;
    pub(super) const JOINT_SELECTION: u8 = 17;
    pub(super) const JOINT_STATE: u8 = 18;
    pub(super) const JOINT_CHOICE: u8 = 19;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum EdgeRoleV1 {
    ProgramMember = 1,
    TargetSource = 2,
    TargetCandidate = 3,
    SolidTarget = 4,
    OpacitySourcePaint = 5,
    OpacityInput = 6,
    ObservationGroupPort = 7,
    InputSurfacePort = 8,
    DerivedSurfaceOccurrence = 9,
    OccurrenceSubjectPaint = 10,
    OccurrenceBackdropSurface = 11,
    ConstraintOccurrence = 12,
    OutputPaint = 13,
    SelectionState = 14,
    StateChoice = 15,
    ChoiceTarget = 16,
    ChoiceCandidate = 17,
}

#[derive(Debug, Clone, Copy)]
struct EdgeV1 {
    from: usize,
    to: usize,
    role: EdgeRoleV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ArcV1 {
    direction: u8,
    role: EdgeRoleV1,
    neighbour: usize,
}

struct CanonicalGraphV1 {
    colors: Vec<VertexColorV1>,
    adjacency: Vec<Vec<ArcV1>>,
    edge_count: usize,
}

struct GraphBuilderV1 {
    colors: Vec<VertexColorV1>,
    edges: Vec<EdgeV1>,
    root: usize,
}

impl GraphBuilderV1 {
    fn new(root: VertexColorV1) -> Result<Self, ProgramCompileError> {
        let mut colors = Vec::new();
        colors
            .try_reserve_exact(1)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        colors.push(root);
        Ok(Self {
            colors,
            edges: Vec::new(),
            root: 0,
        })
    }

    fn add_member(&mut self, color: VertexColorV1) -> Result<usize, ProgramCompileError> {
        self.colors
            .try_reserve(1)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        let index = self.colors.len();
        self.colors.push(color);
        self.add_edge(self.root, index, EdgeRoleV1::ProgramMember)?;
        Ok(index)
    }

    fn add_edge(
        &mut self,
        from: usize,
        to: usize,
        role: EdgeRoleV1,
    ) -> Result<(), ProgramCompileError> {
        if from >= self.colors.len() || to >= self.colors.len() {
            return Err(ProgramCompileError::InternalInvariant);
        }
        self.edges
            .try_reserve(1)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        self.edges.push(EdgeV1 { from, to, role });
        Ok(())
    }

    fn finish(self) -> Result<CanonicalGraphV1, ProgramCompileError> {
        let mut degrees = Vec::new();
        degrees
            .try_reserve_exact(self.colors.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        degrees.resize(self.colors.len(), 0_usize);
        for edge in &self.edges {
            degrees[edge.from] = degrees[edge.from]
                .checked_add(1)
                .ok_or(ProgramCompileError::ResourceExhausted)?;
            degrees[edge.to] = degrees[edge.to]
                .checked_add(1)
                .ok_or(ProgramCompileError::ResourceExhausted)?;
        }

        let mut adjacency = Vec::new();
        adjacency
            .try_reserve_exact(self.colors.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        for degree in degrees {
            let mut arcs = Vec::new();
            arcs.try_reserve_exact(degree)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            adjacency.push(arcs);
        }
        for edge in &self.edges {
            adjacency[edge.from].push(ArcV1 {
                direction: 0,
                role: edge.role,
                neighbour: edge.to,
            });
            adjacency[edge.to].push(ArcV1 {
                direction: 1,
                role: edge.role,
                neighbour: edge.from,
            });
        }
        for arcs in &mut adjacency {
            arcs.sort_unstable();
        }
        Ok(CanonicalGraphV1 {
            colors: self.colors,
            adjacency,
            edge_count: self.edges.len(),
        })
    }
}

struct IdIndexV1<Key> {
    values: Vec<(Key, usize)>,
}

impl<Key> IdIndexV1<Key>
where
    Key: Copy + Ord,
{
    fn new() -> Self {
        Self { values: Vec::new() }
    }

    fn insert(&mut self, key: Key, value: usize) -> Result<(), ProgramCompileError> {
        self.values
            .try_reserve(1)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        self.values.push((key, value));
        Ok(())
    }

    fn finish(&mut self) -> Result<(), ProgramCompileError> {
        self.values.sort_unstable_by_key(|(key, _)| *key);
        if self.values.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(ProgramCompileError::InternalInvariant);
        }
        Ok(())
    }

    fn get(&self, key: Key) -> Result<usize, ProgramCompileError> {
        let index = self
            .values
            .binary_search_by_key(&key, |(candidate, _)| *candidate)
            .map_err(|_| ProgramCompileError::InternalInvariant)?;
        Ok(self.values[index].1)
    }
}

fn program_root_color() -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(vertex_tag::PROGRAM);
    // Эти теги связывают адрес с версиями исполняемых законов: схемой Program,
    // total-order selection, финальной перепроверкой, атомарным наблюдением,
    // encoded Paint emission и формированием modeled LCS.
    for release in [
        release_tag::PROGRAM_SCHEMA_V1,
        release_tag::DECLARED_TOTAL_ORDER_V1,
        release_tag::FRESH_FULL_RECHECK_V1,
        release_tag::ATOMIC_OBSERVATION_GROUP_V1,
        release_tag::ENCODED_PAINT_EMISSION_V1,
        release_tag::MODELED_LCS_OCCURRENCE_V1,
    ] {
        color.push_u8(release)?;
    }
    Ok(color)
}

fn write_signal(color: &mut VertexColorV1, signal: ColorSignal) -> Result<(), ProgramCompileError> {
    let profile = match signal.output_profile() {
        crate::lcs_occurrence::OutputProfileId::Iec61966Srgb8D65V1 => {
            release_tag::IEC_SRGB8_D65_OUTPUT_PROFILE_V1
        }
    };
    color.push_u8(profile)?;
    color.push_u8(match crate::lcs_occurrence::ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1
        .transform_release()
    {
        crate::lcs_occurrence::ColorimetricTransformReleaseId::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1 => {
            release_tag::IEC_SRGB8_TO_XYZ_D65_TRANSFORM_V1
        }
    })?;
    color.push_srgb8(signal.srgb8())
}

fn source_color(source: Source) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(vertex_tag::SOURCE);
    write_signal(&mut color, source.signal())?;
    Ok(color)
}

fn candidate_color(candidate: TargetCandidateV1) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(vertex_tag::CANDIDATE);
    write_signal(&mut color, candidate.signal())?;
    Ok(color)
}

fn opacity_color(input: OpacityInput) -> Result<VertexColorV1, ProgramCompileError> {
    let admitted = crate::composition::AdmittedOpacityV1::new(input.value())
        .map_err(|_| ProgramCompileError::InternalInvariant)?;
    let mut color = VertexColorV1::new(vertex_tag::OPACITY);
    color.push_u64(admitted.bits())?;
    Ok(color)
}

fn write_context(
    color: &mut VertexColorV1,
    context: AppearanceContextId,
) -> Result<(), ProgramCompileError> {
    color.push_u8(match context.schema_release() {
        crate::lcs_occurrence::AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1 => {
            release_tag::CIECAM16_VIEWING_INPUTS_V1
        }
    })?;
    let frame = context.frame();
    color.push_u8(match frame.observer() {
        crate::lcs_occurrence::ObserverProfileId::Cie1931TwoDegreeV1 => {
            release_tag::CIE1931_TWO_DEGREE_OBSERVER_V1
        }
    })?;
    color.push_u8(match frame.reference_white() {
        crate::lcs_occurrence::ReferenceWhiteId::Iec61966D65ChromaticityV1 => {
            release_tag::IEC61966_D65_REFERENCE_WHITE_V1
        }
    })?;
    color.push_u8(match frame.scale() {
        crate::lcs_occurrence::TristimulusScale::RelativeY1 => release_tag::RELATIVE_Y1_SCALE_V1,
    })?;
    color.push_u8(match frame.release() {
        crate::lcs_occurrence::ColorimetricFrameReleaseId::XyzV1 => release_tag::XYZ_FRAME_V1,
        #[cfg(test)]
        crate::lcs_occurrence::ColorimetricFrameReleaseId::MutationSentinelV1 => {
            release_tag::MUTATION_SENTINEL_FRAME_V1
        }
    })?;
    color.push_u64(context.adapting_luminance_cd_m2().to_bits())?;
    color.push_u64(context.background_luminance_ratio().to_bits())?;
    color.push_u8(match context.surround_profile() {
        crate::lcs_occurrence::SurroundProfileId::AverageV1 => release_tag::SURROUND_AVERAGE_V1,
        crate::lcs_occurrence::SurroundProfileId::DimV1 => release_tag::SURROUND_DIM_V1,
        crate::lcs_occurrence::SurroundProfileId::DarkV1 => release_tag::SURROUND_DARK_V1,
    })?;
    Ok(())
}

fn occurrence_color(occurrence: Occurrence) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(vertex_tag::OCCURRENCE);
    color.push_u8(match occurrence.composition() {
        CompositionProfile::EncodedSrgb8SourceOverV1 => release_tag::ENCODED_SRGB8_SOURCE_OVER_V1,
    })?;
    write_context(&mut color, occurrence.context())?;
    Ok(color)
}

fn wcag_criterion_tag(criterion: Wcag22CriterionV1) -> u8 {
    match criterion {
        Wcag22CriterionV1::Sc143TextDefault => release_tag::WCAG22_SC_1_4_3_TEXT_DEFAULT,
        Wcag22CriterionV1::Sc143TextLargeScale => release_tag::WCAG22_SC_1_4_3_TEXT_LARGE_SCALE,
        Wcag22CriterionV1::Sc1411UiComponentOrState => {
            release_tag::WCAG22_SC_1_4_11_UI_COMPONENT_OR_STATE
        }
        Wcag22CriterionV1::Sc1411GraphicalObject => release_tag::WCAG22_SC_1_4_11_GRAPHICAL_OBJECT,
    }
}

fn constraint_color(
    mode_tag: u8,
    content: ProgramConstraintContentV1,
) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(mode_tag);
    match content {
        ProgramConstraintContentV1::ExactSrgb8 {
            identity,
            release,
            capability,
            expected,
        } => {
            color.push_u8(release_tag::EXACT_SRGB8_FAMILY_V1)?;
            color.push_u8(match identity {
                crate::constraints::ExactConstraintIdentityV1::FinalSrgb8IdentityV1 => {
                    release_tag::EXACT_SRGB8_IDENTITY_V1
                }
                #[cfg(test)]
                crate::constraints::ExactConstraintIdentityV1::MutationSentinelV1 => {
                    release_tag::EXACT_SRGB8_IDENTITY_MUTATION_SENTINEL_V1
                }
            })?;
            color.push_u8(match release {
                crate::constraints::ExactIdentityReleaseV1::V1 => {
                    release_tag::EXACT_SRGB8_RELEASE_V1
                }
                #[cfg(test)]
                crate::constraints::ExactIdentityReleaseV1::MutationSentinelV1 => {
                    release_tag::EXACT_SRGB8_RELEASE_MUTATION_SENTINEL_V1
                }
            })?;
            color.push_u8(match capability {
                crate::constraints::ExactIdentityCapabilityV1::FinalOccurrenceSrgb8IdentityV1 => {
                    release_tag::EXACT_SRGB8_CAPABILITY_V1
                }
                #[cfg(test)]
                crate::constraints::ExactIdentityCapabilityV1::MutationSentinelV1 => {
                    release_tag::EXACT_SRGB8_CAPABILITY_MUTATION_SENTINEL_V1
                }
            })?;
            color.push_srgb8(expected)?;
        }
        ProgramConstraintContentV1::Wcag22Srgb8 {
            identity,
            release,
            capability,
            criterion,
        } => {
            color.push_u8(release_tag::WCAG22_SRGB8_FAMILY_V1)?;
            color.push_u8(match identity {
                crate::constraints::Wcag22Srgb8EvaluatorIdentityV1 => {
                    release_tag::WCAG22_SRGB8_IDENTITY_V1
                }
            })?;
            color.push_u8(match release {
                crate::wcag22::Wcag22ProfileIdV1::Wcag22Srgb8ContrastV1 => {
                    release_tag::WCAG22_SRGB8_PROFILE_V1
                }
            })?;
            color.push_u8(match capability {
                crate::constraints::Wcag22Srgb8CapabilityV1 => {
                    release_tag::WCAG22_SRGB8_CAPABILITY_V1
                }
            })?;
            color.push_u8(wcag_criterion_tag(criterion))?;
        }
        #[cfg(test)]
        ProgramConstraintContentV1::FinalRecheckMutantExactSrgb8 { expected } => {
            for tag in [0xFE_u8, 1, 1, 1] {
                color.push_u8(tag)?;
            }
            color.push_srgb8(expected)?;
        }
    }
    Ok(color)
}

fn build_graph<Evaluation>(
    program: &Program<Evaluation>,
) -> Result<CanonicalGraphV1, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let mut graph = GraphBuilderV1::new(program_root_color()?)?;
    let mut sources = IdIndexV1::new();
    let mut targets = IdIndexV1::new();
    let mut candidates = IdIndexV1::new();
    let mut opacities = IdIndexV1::new();
    let mut paints = IdIndexV1::new();
    let mut ports = IdIndexV1::new();
    let mut surfaces = IdIndexV1::new();
    let mut occurrences = IdIndexV1::new();

    for source in &program.sources {
        sources.insert(source.id(), graph.add_member(source_color(*source)?)?)?;
    }
    for target in &program.targets {
        let target_color = match target.domain() {
            TargetDomainV1::Fixed => VertexColorV1::new(vertex_tag::TARGET_FIXED),
            TargetDomainV1::Finite(_) => VertexColorV1::new(vertex_tag::TARGET_FINITE),
        };
        let target_vertex = graph.add_member(target_color)?;
        targets.insert(target.id(), target_vertex)?;
        if let TargetDomainV1::Finite(domain) = target.domain() {
            for candidate in domain {
                let vertex = graph.add_member(candidate_color(*candidate)?)?;
                candidates.insert((target.id(), candidate.id()), vertex)?;
            }
        }
    }
    for opacity in &program.opacities {
        opacities.insert(opacity.id(), graph.add_member(opacity_color(*opacity)?)?)?;
    }
    for paint in &program.paints {
        let (id, tag) = match paint {
            Paint::Solid { id, .. } => (*id, vertex_tag::PAINT_SOLID),
            Paint::Opacity { id, .. } => (*id, vertex_tag::PAINT_OPACITY),
        };
        paints.insert(id, graph.add_member(VertexColorV1::new(tag))?)?;
    }

    let group = graph.add_member(VertexColorV1::new(vertex_tag::OBSERVATION_GROUP))?;
    for port in &program.observation_group.surface_input_ports {
        ports.insert(
            *port,
            graph.add_member(VertexColorV1::new(vertex_tag::SURFACE_INPUT_PORT))?,
        )?;
    }
    for surface in &program.surfaces {
        let (id, tag) = match surface {
            Surface::Input { id, .. } => (*id, vertex_tag::SURFACE_INPUT),
            Surface::FromOccurrence { id, .. } => (*id, vertex_tag::SURFACE_FROM_OCCURRENCE),
        };
        surfaces.insert(id, graph.add_member(VertexColorV1::new(tag))?)?;
    }
    for occurrence in &program.occurrences {
        occurrences.insert(
            occurrence.id(),
            graph.add_member(occurrence_color(*occurrence)?)?,
        )?;
    }

    sources.finish()?;
    targets.finish()?;
    candidates.finish()?;
    opacities.finish()?;
    paints.finish()?;
    ports.finish()?;
    surfaces.finish()?;
    occurrences.finish()?;

    for target in &program.targets {
        let target_vertex = targets.get(target.id())?;
        graph.add_edge(
            target_vertex,
            sources.get(target.source())?,
            EdgeRoleV1::TargetSource,
        )?;
        if let TargetDomainV1::Finite(domain) = target.domain() {
            for candidate in domain {
                graph.add_edge(
                    target_vertex,
                    candidates.get((target.id(), candidate.id()))?,
                    EdgeRoleV1::TargetCandidate,
                )?;
            }
        }
    }
    for paint in &program.paints {
        match *paint {
            Paint::Solid { id, target } => graph.add_edge(
                paints.get(id)?,
                targets.get(target)?,
                EdgeRoleV1::SolidTarget,
            )?,
            Paint::Opacity {
                id,
                source,
                opacity,
            } => {
                graph.add_edge(
                    paints.get(id)?,
                    paints.get(source)?,
                    EdgeRoleV1::OpacitySourcePaint,
                )?;
                graph.add_edge(
                    paints.get(id)?,
                    opacities.get(opacity)?,
                    EdgeRoleV1::OpacityInput,
                )?;
            }
        }
    }
    for port in &program.observation_group.surface_input_ports {
        graph.add_edge(group, ports.get(*port)?, EdgeRoleV1::ObservationGroupPort)?;
    }
    for surface in &program.surfaces {
        match *surface {
            Surface::Input { id, input } => graph.add_edge(
                surfaces.get(id)?,
                ports.get(input)?,
                EdgeRoleV1::InputSurfacePort,
            )?,
            Surface::FromOccurrence { id, occurrence } => graph.add_edge(
                surfaces.get(id)?,
                occurrences.get(occurrence)?,
                EdgeRoleV1::DerivedSurfaceOccurrence,
            )?,
        }
    }
    for occurrence in &program.occurrences {
        graph.add_edge(
            occurrences.get(occurrence.id())?,
            paints.get(occurrence.subject())?,
            EdgeRoleV1::OccurrenceSubjectPaint,
        )?;
        graph.add_edge(
            occurrences.get(occurrence.id())?,
            surfaces.get(occurrence.against())?,
            EdgeRoleV1::OccurrenceBackdropSurface,
        )?;
    }

    for constraint in &program.constraints.hard {
        let color = constraint_color(
            vertex_tag::CONSTRAINT_HARD,
            program.evaluator.constraint_content(constraint.invocation),
        )?;
        let vertex = graph.add_member(color)?;
        graph.add_edge(
            vertex,
            occurrences.get(constraint.target)?,
            EdgeRoleV1::ConstraintOccurrence,
        )?;
    }
    for constraint in &program.constraints.report_only {
        let color = constraint_color(
            vertex_tag::CONSTRAINT_REPORT_ONLY,
            program.evaluator.constraint_content(constraint.invocation),
        )?;
        let vertex = graph.add_member(color)?;
        graph.add_edge(
            vertex,
            occurrences.get(constraint.target)?,
            EdgeRoleV1::ConstraintOccurrence,
        )?;
    }
    for output in &program.outputs {
        let vertex = graph.add_member(VertexColorV1::new(vertex_tag::OUTPUT))?;
        graph.add_edge(vertex, paints.get(output.paint())?, EdgeRoleV1::OutputPaint)?;
    }

    if let Some(selection) = &program.joint_selection {
        let selection_vertex = graph.add_member(VertexColorV1::new(vertex_tag::JOINT_SELECTION))?;
        for (state_index, state) in selection.states().iter().enumerate() {
            let state_index =
                u64::try_from(state_index).map_err(|_| ProgramCompileError::ResourceExhausted)?;
            let mut state_color = VertexColorV1::new(vertex_tag::JOINT_STATE);
            state_color.push_u64(state_index)?;
            let state_vertex = graph.add_member(state_color)?;
            graph.add_edge(selection_vertex, state_vertex, EdgeRoleV1::SelectionState)?;
            for choice in state.choices() {
                let choice_vertex =
                    graph.add_member(VertexColorV1::new(vertex_tag::JOINT_CHOICE))?;
                graph.add_edge(state_vertex, choice_vertex, EdgeRoleV1::StateChoice)?;
                graph.add_edge(
                    choice_vertex,
                    targets.get(choice.target())?,
                    EdgeRoleV1::ChoiceTarget,
                )?;
                graph.add_edge(
                    choice_vertex,
                    candidates.get((choice.target(), choice.candidate()))?,
                    EdgeRoleV1::ChoiceCandidate,
                )?;
            }
        }
    }

    graph.finish()
}

struct PartitionV1 {
    cells: Vec<Vec<usize>>,
}

impl PartitionV1 {
    fn initial(graph: &CanonicalGraphV1) -> Result<Self, ProgramCompileError> {
        let mut order = Vec::new();
        order
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        order.extend(0..graph.colors.len());
        order.sort_unstable_by(|left, right| {
            graph.colors[*left]
                .cmp(&graph.colors[*right])
                .then_with(|| left.cmp(right))
        });

        let mut cells = Vec::new();
        cells
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        let mut start = 0;
        while start < order.len() {
            let mut end = start + 1;
            while end < order.len() && graph.colors[order[start]] == graph.colors[order[end]] {
                end += 1;
            }
            cells.push(copy_vertices(&order[start..end])?);
            start = end;
        }
        Ok(Self { cells })
    }

    fn is_discrete(&self) -> bool {
        self.cells.iter().all(|cell| cell.len() == 1)
    }

    fn first_non_singleton(&self) -> Option<usize> {
        self.cells.iter().position(|cell| cell.len() > 1)
    }
}

fn copy_vertices(source: &[usize]) -> Result<Vec<usize>, ProgramCompileError> {
    let mut copied = Vec::new();
    copied
        .try_reserve_exact(source.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    copied.extend_from_slice(source);
    Ok(copied)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct RefinementAtomV1 {
    direction: u8,
    role: EdgeRoleV1,
    neighbour_cell: usize,
}

struct RefinementRecordV1 {
    vertex: usize,
    signature: Vec<RefinementAtomV1>,
}

fn refine_partition(
    graph: &CanonicalGraphV1,
    mut partition: PartitionV1,
) -> Result<PartitionV1, ProgramCompileError> {
    loop {
        let mut cell_of = Vec::new();
        cell_of
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        cell_of.resize(graph.colors.len(), usize::MAX);
        for (cell_index, cell) in partition.cells.iter().enumerate() {
            for &vertex in cell {
                let slot = cell_of
                    .get_mut(vertex)
                    .ok_or(ProgramCompileError::InternalInvariant)?;
                if *slot != usize::MAX {
                    return Err(ProgramCompileError::InternalInvariant);
                }
                *slot = cell_index;
            }
        }
        if cell_of.contains(&usize::MAX) {
            return Err(ProgramCompileError::InternalInvariant);
        }

        let mut next_cells = Vec::new();
        next_cells
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        for cell in &partition.cells {
            let mut records = Vec::new();
            records
                .try_reserve_exact(cell.len())
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            for &vertex in cell {
                let arcs = graph
                    .adjacency
                    .get(vertex)
                    .ok_or(ProgramCompileError::InternalInvariant)?;
                let mut signature = Vec::new();
                signature
                    .try_reserve_exact(arcs.len())
                    .map_err(|_| ProgramCompileError::ResourceExhausted)?;
                for arc in arcs {
                    signature.push(RefinementAtomV1 {
                        direction: arc.direction,
                        role: arc.role,
                        neighbour_cell: cell_of[arc.neighbour],
                    });
                }
                signature.sort_unstable();
                records.push(RefinementRecordV1 { vertex, signature });
            }
            records.sort_unstable_by(|left, right| left.signature.cmp(&right.signature));

            let mut start = 0;
            while start < records.len() {
                let mut end = start + 1;
                while end < records.len() && records[start].signature == records[end].signature {
                    end += 1;
                }
                let mut split = Vec::new();
                split
                    .try_reserve_exact(end - start)
                    .map_err(|_| ProgramCompileError::ResourceExhausted)?;
                split.extend(records[start..end].iter().map(|record| record.vertex));
                next_cells.push(split);
                start = end;
            }
        }

        if next_cells.len() == partition.cells.len() {
            partition.cells = next_cells;
            return Ok(partition);
        }
        partition.cells = next_cells;
    }
}

fn individualize(
    partition: &PartitionV1,
    cell_index: usize,
    vertex: usize,
) -> Result<PartitionV1, ProgramCompileError> {
    let selected = partition
        .cells
        .get(cell_index)
        .ok_or(ProgramCompileError::InternalInvariant)?;
    if selected.len() < 2 || !selected.contains(&vertex) {
        return Err(ProgramCompileError::InternalInvariant);
    }

    let mut cells = Vec::new();
    cells
        .try_reserve_exact(
            partition
                .cells
                .len()
                .checked_add(1)
                .ok_or(ProgramCompileError::ResourceExhausted)?,
        )
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for (index, cell) in partition.cells.iter().enumerate() {
        if index != cell_index {
            cells.push(copy_vertices(cell)?);
            continue;
        }
        let mut singleton = Vec::new();
        singleton
            .try_reserve_exact(1)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        singleton.push(vertex);
        cells.push(singleton);

        let mut remainder = Vec::new();
        remainder
            .try_reserve_exact(cell.len() - 1)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        remainder.extend(
            cell.iter()
                .copied()
                .filter(|candidate| *candidate != vertex),
        );
        cells.push(remainder);
    }
    Ok(PartitionV1 { cells })
}

struct SearchFrameV1 {
    partition: PartitionV1,
    branch_cell: Option<usize>,
    candidates: Vec<usize>,
    explored_candidates: Vec<usize>,
    next_candidate: usize,
    leaf_pending: bool,
}

impl SearchFrameV1 {
    fn new(partition: PartitionV1) -> Result<Self, ProgramCompileError> {
        let branch_cell = partition.first_non_singleton();
        let candidates = match branch_cell {
            Some(index) => copy_vertices(&partition.cells[index])?,
            None => Vec::new(),
        };
        Ok(Self {
            leaf_pending: branch_cell.is_none(),
            partition,
            branch_cell,
            candidates,
            explored_candidates: Vec::new(),
            next_candidate: 0,
        })
    }
}

fn push_u64_bytes(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn usize_as_u64(value: usize) -> Result<u64, ProgramCompileError> {
    u64::try_from(value).map_err(|_| ProgramCompileError::ResourceExhausted)
}

struct SerializedLeafV1 {
    preimage: Vec<u8>,
    order: Vec<usize>,
}

fn serialize_leaf(
    graph: &CanonicalGraphV1,
    partition: &PartitionV1,
) -> Result<SerializedLeafV1, ProgramCompileError> {
    if !partition.is_discrete() || partition.cells.len() != graph.colors.len() {
        return Err(ProgramCompileError::InternalInvariant);
    }
    let mut order = Vec::new();
    order
        .try_reserve_exact(graph.colors.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    let mut label_of = Vec::new();
    label_of
        .try_reserve_exact(graph.colors.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    label_of.resize(graph.colors.len(), usize::MAX);
    for (label, cell) in partition.cells.iter().enumerate() {
        let [vertex] = cell.as_slice() else {
            return Err(ProgramCompileError::InternalInvariant);
        };
        label_of[*vertex] = label;
        order.push(*vertex);
    }

    let edge_bytes = graph
        .edge_count
        .checked_mul(9)
        .ok_or(ProgramCompileError::ResourceExhausted)?;
    let color_bytes = graph.colors.iter().try_fold(0_usize, |total, color| {
        total
            .checked_add(16)
            .and_then(|value| value.checked_add(color.as_slice().len()))
            .ok_or(ProgramCompileError::ResourceExhausted)
    })?;
    let capacity = DOMAIN_V1
        .len()
        .checked_add(16)
        .and_then(|value| value.checked_add(color_bytes))
        .and_then(|value| value.checked_add(edge_bytes))
        .ok_or(ProgramCompileError::ResourceExhausted)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(capacity)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    output.extend_from_slice(DOMAIN_V1);
    push_u64_bytes(&mut output, usize_as_u64(graph.colors.len())?);
    push_u64_bytes(&mut output, usize_as_u64(graph.edge_count)?);

    for cell in &partition.cells {
        let vertex = cell[0];
        let color = graph.colors[vertex];
        push_u64_bytes(&mut output, u64::from(color.len));
        output.extend_from_slice(color.as_slice());

        let outgoing_count = graph.adjacency[vertex]
            .iter()
            .filter(|arc| arc.direction == 0)
            .count();
        push_u64_bytes(&mut output, usize_as_u64(outgoing_count)?);
        let mut outgoing = Vec::new();
        outgoing
            .try_reserve_exact(outgoing_count)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        outgoing.extend(
            graph.adjacency[vertex]
                .iter()
                .filter(|arc| arc.direction == 0)
                .map(|arc| (arc.role, label_of[arc.neighbour])),
        );
        outgoing.sort_unstable();
        for (role, target) in outgoing {
            output.push(role as u8);
            push_u64_bytes(&mut output, usize_as_u64(target)?);
        }
    }
    Ok(SerializedLeafV1 {
        preimage: output,
        order,
    })
}

fn equal_leaf_automorphism(
    canonical_order: &[usize],
    equal_order: &[usize],
) -> Result<Vec<usize>, ProgramCompileError> {
    if canonical_order.len() != equal_order.len() {
        return Err(ProgramCompileError::InternalInvariant);
    }
    let mut permutation = Vec::new();
    permutation
        .try_reserve_exact(canonical_order.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    permutation.resize(canonical_order.len(), usize::MAX);
    for (&from, &to) in canonical_order.iter().zip(equal_order) {
        let slot = permutation
            .get_mut(from)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        if *slot != usize::MAX || to >= canonical_order.len() {
            return Err(ProgramCompileError::InternalInvariant);
        }
        *slot = to;
    }
    if permutation.contains(&usize::MAX) {
        return Err(ProgramCompileError::InternalInvariant);
    }
    Ok(permutation)
}

fn automorphism_preserves_partition(
    permutation: &[usize],
    cell_of: &[usize],
) -> Result<bool, ProgramCompileError> {
    if permutation.len() != cell_of.len() {
        return Err(ProgramCompileError::InternalInvariant);
    }
    for (vertex, &image) in permutation.iter().enumerate() {
        let image_cell = cell_of
            .get(image)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        if cell_of[vertex] != *image_cell {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Проверяет, лежит ли `candidate` в орбите уже исследованной ветви при
/// автоморфизмах, стабилизирующих текущее упорядоченное разбиение. Отсечение
/// точное: отображение сохраняется лишь после совпадения полных сериализаций
/// листьев, доказывающего автоморфизм графа.
fn candidate_is_in_explored_orbit(
    partition: &PartitionV1,
    explored: &[usize],
    candidate: usize,
    automorphisms: &[Vec<usize>],
) -> Result<bool, ProgramCompileError> {
    if explored.is_empty() || automorphisms.is_empty() {
        return Ok(false);
    }
    let vertex_count = automorphisms[0].len();
    let mut cell_of = Vec::new();
    cell_of
        .try_reserve_exact(vertex_count)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    cell_of.resize(vertex_count, usize::MAX);
    for (cell_index, cell) in partition.cells.iter().enumerate() {
        for &vertex in cell {
            let slot = cell_of
                .get_mut(vertex)
                .ok_or(ProgramCompileError::InternalInvariant)?;
            if *slot != usize::MAX {
                return Err(ProgramCompileError::InternalInvariant);
            }
            *slot = cell_index;
        }
    }
    if cell_of.contains(&usize::MAX) || candidate >= vertex_count {
        return Err(ProgramCompileError::InternalInvariant);
    }

    let mut stabilizers = Vec::new();
    stabilizers
        .try_reserve_exact(automorphisms.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for (index, permutation) in automorphisms.iter().enumerate() {
        if automorphism_preserves_partition(permutation, &cell_of)? {
            stabilizers.push(index);
        }
    }
    if stabilizers.is_empty() {
        return Ok(false);
    }

    let mut seen = Vec::new();
    seen.try_reserve_exact(vertex_count)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    seen.resize(vertex_count, false);
    let mut queue = Vec::new();
    queue
        .try_reserve_exact(vertex_count)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for &representative in explored {
        let slot = seen
            .get_mut(representative)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        if !*slot {
            *slot = true;
            queue.push(representative);
        }
    }
    let mut cursor = 0;
    while cursor < queue.len() {
        let vertex = queue[cursor];
        cursor += 1;
        for &index in &stabilizers {
            let image = automorphisms[index][vertex];
            let slot = seen
                .get_mut(image)
                .ok_or(ProgramCompileError::InternalInvariant)?;
            if !*slot {
                *slot = true;
                queue.push(image);
            }
        }
    }
    Ok(seen[candidate])
}

/// Для одноцветных вершин с одинаковыми полными списками инцидентности
/// транспозиция сохраняет все типизированные рёбра. Это точное дешёвое
/// отсечение обрабатывает повторы до выделения общей перестановки.
fn candidate_is_exact_twin(
    graph: &CanonicalGraphV1,
    explored: &[usize],
    candidate: usize,
) -> Result<bool, ProgramCompileError> {
    let candidate_color = graph
        .colors
        .get(candidate)
        .ok_or(ProgramCompileError::InternalInvariant)?;
    let candidate_arcs = graph
        .adjacency
        .get(candidate)
        .ok_or(ProgramCompileError::InternalInvariant)?;
    for &representative in explored {
        if graph
            .colors
            .get(representative)
            .ok_or(ProgramCompileError::InternalInvariant)?
            == candidate_color
            && graph
                .adjacency
                .get(representative)
                .ok_or(ProgramCompileError::InternalInvariant)?
                == candidate_arcs
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn canonical_search_impl(
    graph: &CanonicalGraphV1,
    mut remaining_branch_expansions: Option<usize>,
) -> Result<(Vec<u8>, usize), ProgramCompileError> {
    let initial = refine_partition(graph, PartitionV1::initial(graph)?)?;
    let mut stack = Vec::new();
    stack
        .try_reserve_exact(graph.colors.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    stack.push(SearchFrameV1::new(initial)?);
    let mut best: Option<SerializedLeafV1> = None;
    let mut automorphisms: Vec<Vec<usize>> = Vec::new();
    let mut leaf_count = 0_usize;

    while !stack.is_empty() {
        let leaf = stack
            .last()
            .map(|frame| frame.leaf_pending)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        if leaf {
            let candidate = {
                let frame = stack
                    .last_mut()
                    .ok_or(ProgramCompileError::InternalInvariant)?;
                frame.leaf_pending = false;
                serialize_leaf(graph, &frame.partition)?
            };
            stack.pop();
            // Диагностический счётчик не участвует в admission: насыщение не
            // влияет на выбранный прообраз даже у недостижимо большого дерева.
            leaf_count = leaf_count.saturating_add(1);
            match &best {
                None => best = Some(candidate),
                Some(current) if candidate.preimage < current.preimage => best = Some(candidate),
                Some(current) if candidate.preimage == current.preimage => {
                    let permutation = equal_leaf_automorphism(&current.order, &candidate.order)?;
                    // Отсечение не требуется для корректности. Храним не более
                    // V доказанных автоморфизмов: при длине V это ограничивает
                    // память O(V²), а остальные ветви исследуются полностью.
                    if permutation.iter().enumerate().any(|(from, to)| from != *to)
                        && automorphisms.len() < graph.colors.len()
                        && !automorphisms.contains(&permutation)
                    {
                        automorphisms
                            .try_reserve(1)
                            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
                        automorphisms.push(permutation);
                    }
                }
                Some(_) => {}
            }
            continue;
        }

        let child = {
            let frame = stack
                .last_mut()
                .ok_or(ProgramCompileError::InternalInvariant)?;
            let mut selected = None;
            while frame.next_candidate < frame.candidates.len() {
                let candidate = frame.candidates[frame.next_candidate];
                frame.next_candidate += 1;
                if candidate_is_exact_twin(graph, &frame.explored_candidates, candidate)?
                    || candidate_is_in_explored_orbit(
                        &frame.partition,
                        &frame.explored_candidates,
                        candidate,
                        &automorphisms,
                    )?
                {
                    continue;
                }
                frame
                    .explored_candidates
                    .try_reserve(1)
                    .map_err(|_| ProgramCompileError::ResourceExhausted)?;
                frame.explored_candidates.push(candidate);
                selected = Some(candidate);
                break;
            }
            match selected {
                Some(candidate) => {
                    if let Some(remaining) = &mut remaining_branch_expansions {
                        *remaining = remaining
                            .checked_sub(1)
                            .ok_or(ProgramCompileError::ResourceExhausted)?;
                    }
                    let cell = frame
                        .branch_cell
                        .ok_or(ProgramCompileError::InternalInvariant)?;
                    Some(refine_partition(
                        graph,
                        individualize(&frame.partition, cell, candidate)?,
                    )?)
                }
                None => None,
            }
        };
        match child {
            Some(partition) => {
                stack
                    .try_reserve(1)
                    .map_err(|_| ProgramCompileError::ResourceExhausted)?;
                stack.push(SearchFrameV1::new(partition)?);
            }
            None => {
                stack.pop();
            }
        }
    }
    let best = best.ok_or(ProgramCompileError::InternalInvariant)?;
    Ok((best.preimage, leaf_count))
}

fn canonical_search(graph: &CanonicalGraphV1) -> Result<(Vec<u8>, usize), ProgramCompileError> {
    // Число шагов поиска не инвариантно к изоморфизму: после opaque-
    // переименования автоморфизм может обнаружиться другой ветвью. Поэтому
    // динамический лимит сделал бы допуск зависимым от client-owned ID. Точный
    // поиск возвращает только полный прообраз либо типизированный отказ.
    canonical_search_impl(graph, None)
}

#[cfg(test)]
fn canonical_search_with_test_fuel(
    graph: &CanonicalGraphV1,
    test_fuel: usize,
) -> Result<(Vec<u8>, usize), ProgramCompileError> {
    // Только тестовая инъекция отказа для проверки атомарности. Это не политика
    // допуска: история поиска не инвариантна к alpha-переименованию.
    canonical_search_impl(graph, Some(test_fuel))
}

fn canonical_preimage(graph: &CanonicalGraphV1) -> Result<Vec<u8>, ProgramCompileError> {
    canonical_search(graph).map(|(preimage, _)| preimage)
}

pub(super) fn compile_program_content_identity_v1<Evaluation>(
    program: &Program<Evaluation>,
) -> Result<ProgramContentIdentityV1, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let graph = build_graph(program)?;
    let preimage = canonical_preimage(&graph)?;
    let digest = crate::sha256::digest(&preimage);
    Ok(ProgramContentIdentityV1(*digest.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn evaluator_descriptor_and_certificate_metadata_have_one_source_of_truth() {
        let evaluator = crate::constraints::ExactSrgb8IdentityV1;
        let expected = Srgb8::new([0x12, 0x34, 0x56]);
        let content = evaluator.program_constraint_content_v1(expected);
        let ProgramConstraintContentV1::ExactSrgb8 {
            identity,
            release,
            capability,
            expected: described_expected,
        } = content
        else {
            panic!("exact evaluator must describe its own exact invocation");
        };
        assert_eq!(
            identity,
            <crate::constraints::ExactSrgb8IdentityV1 as Evaluator<ProgramPointTargetV1>>::identity(
                &evaluator
            )
        );
        assert_eq!(
            release,
            <crate::constraints::ExactSrgb8IdentityV1 as Evaluator<ProgramPointTargetV1>>::release(
                &evaluator
            )
        );
        assert_eq!(
            capability,
            <crate::constraints::ExactSrgb8IdentityV1 as Evaluator<
                ProgramPointTargetV1,
            >>::capability(&evaluator)
        );
        assert_eq!(described_expected, expected);

        let baseline = constraint_color(vertex_tag::CONSTRAINT_HARD, content).unwrap();
        for mutant in [
            ProgramConstraintContentV1::ExactSrgb8 {
                identity: crate::constraints::ExactConstraintIdentityV1::MutationSentinelV1,
                release,
                capability,
                expected,
            },
            ProgramConstraintContentV1::ExactSrgb8 {
                identity,
                release: crate::constraints::ExactIdentityReleaseV1::MutationSentinelV1,
                capability,
                expected,
            },
            ProgramConstraintContentV1::ExactSrgb8 {
                identity,
                release,
                capability: crate::constraints::ExactIdentityCapabilityV1::MutationSentinelV1,
                expected,
            },
        ] {
            assert_ne!(
                constraint_color(vertex_tag::CONSTRAINT_HARD, mutant).unwrap(),
                baseline
            );
        }
    }

    fn context_color(context: AppearanceContextId) -> VertexColorV1 {
        let mut color = VertexColorV1::new(vertex_tag::OCCURRENCE);
        write_context(&mut color, context).unwrap();
        color
    }

    #[test]
    fn every_appearance_context_coordinate_and_frame_release_is_content_bound() {
        use crate::lcs_occurrence::{
            AdaptingLuminanceCdM2, AppearanceContextSchemaReleaseId, BackgroundLuminanceRatio,
            IEC_SRGB_D65_XYZ_FRAME_V1, MUTATION_SENTINEL_XYZ_FRAME_V1, SurroundProfileId,
        };

        let make = |frame, adapting_luminance, background_ratio, surround| {
            AppearanceContextId::from_inputs(
                AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
                frame,
                AdaptingLuminanceCdM2::try_new(adapting_luminance).unwrap(),
                BackgroundLuminanceRatio::try_new(background_ratio).unwrap(),
                surround,
            )
        };
        let baseline = context_color(make(
            IEC_SRGB_D65_XYZ_FRAME_V1,
            64.0,
            0.2,
            SurroundProfileId::AverageV1,
        ));
        for mutant in [
            make(
                IEC_SRGB_D65_XYZ_FRAME_V1,
                32.0,
                0.2,
                SurroundProfileId::AverageV1,
            ),
            make(
                IEC_SRGB_D65_XYZ_FRAME_V1,
                64.0,
                0.1,
                SurroundProfileId::AverageV1,
            ),
            make(
                IEC_SRGB_D65_XYZ_FRAME_V1,
                64.0,
                0.2,
                SurroundProfileId::DimV1,
            ),
            make(
                MUTATION_SENTINEL_XYZ_FRAME_V1,
                64.0,
                0.2,
                SurroundProfileId::AverageV1,
            ),
        ] {
            assert_ne!(context_color(mutant), baseline);
        }
    }

    #[test]
    fn every_wcag_criterion_has_distinct_constraint_content() {
        let evaluator = crate::constraints::Wcag22Srgb8V1;
        let mut colors = Vec::new();
        for criterion in [
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22CriterionV1::Sc143TextLargeScale,
            Wcag22CriterionV1::Sc1411UiComponentOrState,
            Wcag22CriterionV1::Sc1411GraphicalObject,
        ] {
            colors.push(
                constraint_color(
                    vertex_tag::CONSTRAINT_HARD,
                    evaluator.program_constraint_content_v1(criterion),
                )
                .unwrap(),
            );
        }
        colors.sort_unstable();
        colors.dedup();
        assert_eq!(colors.len(), 4);
    }

    fn mapping_preserves_graph(
        left: &CanonicalGraphV1,
        right: &CanonicalGraphV1,
        mapping: &[usize],
    ) -> bool {
        if left.colors.len() != right.colors.len() || left.edge_count != right.edge_count {
            return false;
        }
        for (vertex, &image) in mapping.iter().enumerate() {
            if left.colors[vertex] != right.colors[image] {
                return false;
            }
            let mut left_arcs = left.adjacency[vertex]
                .iter()
                .map(|arc| (arc.direction, arc.role, mapping[arc.neighbour]))
                .collect::<Vec<_>>();
            let mut right_arcs = right.adjacency[image]
                .iter()
                .map(|arc| (arc.direction, arc.role, arc.neighbour))
                .collect::<Vec<_>>();
            left_arcs.sort_unstable();
            right_arcs.sort_unstable();
            if left_arcs != right_arcs {
                return false;
            }
        }
        true
    }

    fn visit_mappings(
        left: &CanonicalGraphV1,
        right: &CanonicalGraphV1,
        mapping: &mut [usize],
        cursor: usize,
    ) -> bool {
        if cursor == mapping.len() {
            return mapping_preserves_graph(left, right, mapping);
        }
        for candidate in cursor..mapping.len() {
            mapping.swap(cursor, candidate);
            let color_matches = left.colors[cursor] == right.colors[mapping[cursor]];
            if color_matches && visit_mappings(left, right, mapping, cursor + 1) {
                mapping.swap(cursor, candidate);
                return true;
            }
            mapping.swap(cursor, candidate);
        }
        false
    }

    fn brute_force_isomorphic(left: &CanonicalGraphV1, right: &CanonicalGraphV1) -> bool {
        if left.colors.len() != right.colors.len() {
            return false;
        }
        let mut mapping = (0..left.colors.len()).collect::<Vec<_>>();
        visit_mappings(left, right, &mut mapping, 0)
    }

    fn tiny_bipartite_graph(edge_mask: u8) -> CanonicalGraphV1 {
        let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM)).unwrap();
        let sources = [(); 2].map(|()| {
            graph
                .add_member(VertexColorV1::new(vertex_tag::SOURCE))
                .unwrap()
        });
        let targets = [(); 2].map(|()| {
            graph
                .add_member(VertexColorV1::new(vertex_tag::TARGET_FIXED))
                .unwrap()
        });
        for (bit, (target, source)) in [
            (targets[0], sources[0]),
            (targets[0], sources[1]),
            (targets[1], sources[0]),
            (targets[1], sources[1]),
        ]
        .into_iter()
        .enumerate()
        {
            if edge_mask & (1 << bit) != 0 {
                graph
                    .add_edge(target, source, EdgeRoleV1::TargetSource)
                    .unwrap();
            }
        }
        graph.finish().unwrap()
    }

    #[test]
    fn canonicalizer_matches_an_independent_tiny_isomorphism_oracle() {
        let graphs = (0..16).map(tiny_bipartite_graph).collect::<Vec<_>>();
        let preimages = graphs
            .iter()
            .map(|graph| canonical_preimage(graph).unwrap())
            .collect::<Vec<_>>();

        for left in 0..graphs.len() {
            for right in 0..graphs.len() {
                assert_eq!(
                    preimages[left] == preimages[right],
                    brute_force_isomorphic(&graphs[left], &graphs[right]),
                    "tiny bipartite masks {left:#06b} and {right:#06b}"
                );
            }
        }
    }

    #[test]
    fn exact_automorphism_pruning_prevents_factorial_symmetric_search() {
        const SYMMETRIC_VERTICES: usize = 12;
        let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM)).unwrap();
        for _ in 0..SYMMETRIC_VERTICES {
            graph
                .add_member(VertexColorV1::new(vertex_tag::SOURCE))
                .unwrap();
        }
        let graph = graph.finish().unwrap();

        let (_, leaves) = canonical_search(&graph).unwrap();

        assert_eq!(leaves, 1, "exact twin pruning visited extra leaves");
    }

    #[test]
    fn exhausted_fault_injection_fuel_never_returns_a_partial_preimage() {
        let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM)).unwrap();
        graph
            .add_member(VertexColorV1::new(vertex_tag::SOURCE))
            .unwrap();
        graph
            .add_member(VertexColorV1::new(vertex_tag::SOURCE))
            .unwrap();
        let graph = graph.finish().unwrap();

        assert_eq!(
            canonical_search_with_test_fuel(&graph, 0).unwrap_err(),
            ProgramCompileError::ResourceExhausted
        );
    }

    fn relabelled_budget_graph(permutation: [usize; 9]) -> CanonicalGraphV1 {
        const EDGES: [(usize, usize); 16] = [
            (1, 2),
            (2, 1),
            (3, 4),
            (8, 1),
            (4, 3),
            (1, 8),
            (6, 4),
            (6, 7),
            (7, 6),
            (5, 6),
            (5, 3),
            (8, 2),
            (7, 5),
            (4, 7),
            (3, 5),
            (2, 8),
        ];

        let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM)).unwrap();
        for _ in 1..permutation.len() {
            graph
                .add_member(VertexColorV1::new(vertex_tag::SOURCE))
                .unwrap();
        }
        for (from, to) in EDGES {
            graph
                .add_edge(permutation[from], permutation[to], EdgeRoleV1::TargetSource)
                .unwrap();
        }
        graph.finish().unwrap()
    }

    fn permutation_from_keys<const N: usize>(keys: [u64; N]) -> Vec<usize> {
        let mut images = (0..N).collect::<Vec<_>>();
        images.sort_unstable_by_key(|index| (keys[*index], *index));
        let mut permutation = vec![0];
        permutation.extend(images.into_iter().map(|index| index + 1));
        permutation
    }

    fn small_directed_graph(edge_mask: u16, permutation: &[usize]) -> CanonicalGraphV1 {
        let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM)).unwrap();
        for _ in 1..permutation.len() {
            graph
                .add_member(VertexColorV1::new(vertex_tag::SOURCE))
                .unwrap();
        }
        let mut bit = 0;
        for from in 1..permutation.len() {
            for to in 1..permutation.len() {
                if from != to && edge_mask & (1 << bit) != 0 {
                    graph
                        .add_edge(permutation[from], permutation[to], EdgeRoleV1::TargetSource)
                        .unwrap();
                }
                if from != to {
                    bit += 1;
                }
            }
        }
        graph.finish().unwrap()
    }

    #[test]
    fn exact_preimage_is_invariant_under_opaque_relabelling() {
        let canonical = relabelled_budget_graph([0, 1, 2, 3, 4, 5, 6, 7, 8]);
        let renamed = relabelled_budget_graph([0, 1, 2, 6, 3, 5, 4, 7, 8]);

        let (canonical, _) = canonical_search(&canonical).unwrap();
        let (renamed, _) = canonical_search(&renamed).unwrap();

        assert_eq!(canonical, renamed);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn hostile_graph_preimage_is_invariant_for_generated_bijections(
            keys in proptest::array::uniform8(any::<u64>()),
        ) {
            let permutation = permutation_from_keys(keys);
            let permutation: [usize; 9] = permutation.try_into().unwrap();
            let baseline = relabelled_budget_graph([0, 1, 2, 3, 4, 5, 6, 7, 8]);
            let renamed = relabelled_budget_graph(permutation);

            prop_assert_eq!(
                canonical_search(&baseline).unwrap().0,
                canonical_search(&renamed).unwrap().0,
            );
        }

        #[test]
        fn small_role_directed_graph_preimage_is_invariant_for_generated_bijections(
            edge_mask in 0_u16..=0x0fff,
            keys in proptest::array::uniform4(any::<u64>()),
        ) {
            let baseline = small_directed_graph(edge_mask, &[0, 1, 2, 3, 4]);
            let renamed = small_directed_graph(edge_mask, &permutation_from_keys(keys));

            prop_assert_eq!(
                canonical_search(&baseline).unwrap().0,
                canonical_search(&renamed).unwrap().0,
            );
        }
    }
}
