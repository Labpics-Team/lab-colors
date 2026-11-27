//! Устойчивый к коллизиям адрес исполняемого содержимого принятой Program.
//!
//! Paint/Surface/Occurrence способны образовывать двудольные графы
//! инцидентности, поэтому одного топологического хеша или уточнения разбиения
//! недостаточно. Модуль строит типизированный цветной граф, канонизирует его без
//! opaque ID и хеширует канонический прообраз.

use super::*;
use crate::colored_graph::{EdgeV1, ordering};

#[path = "program_identity/observed_order.rs"]
mod observed_order;
pub(crate) use observed_order::{ObservedComponentOrder, PeerSearchGraph};

const DOMAIN_V9: &[u8] = b"labcolors.program-content-identity.v9\0";
const AUTHORED_COMPOSITION_DOMAIN_V1: &[u8] =
    b"labcolors.authored-composition-program.v1.component-joint-point.legacy-reference-ordered.full-recheck\0";
// V9 резервирует фиксированную protocol boundary под type/family tag и полный
// content digest. Каждый writer использует проверяемый push и отвергает
// расширение схемы вместо усечения данных или аллокации на вершину.
const HASH_BOUND_COLOR_BYTES_V9: usize = 1 + 1 + 32;
const COLOR_CAPACITY: usize = HASH_BOUND_COLOR_BYTES_V9;

mod release_tag {
    pub(super) const PROGRAM_SCHEMA_V9: u8 = 9;
    pub(super) const SELECTION_RELEASE_IDENTITY_V1: u8 = 1;
    pub(super) const DECLARED_TOTAL_ORDER_V1: u8 = 1;
    pub(super) const AUTHORED_COMPOSITION_V1: u8 = 2;
    pub(super) const FRESH_FULL_RECHECK_V1: u8 = 1;
    pub(super) const ATOMIC_OBSERVATION_GROUP_V1: u8 = 1;
    pub(super) const ENCODED_PAINT_OUTPUT_ROUTING_V1: u8 = 1;
    pub(super) const FINITE_ATOMIC_PAINT_CANDIDATE_V1: u8 = 1;
    pub(super) const MODELED_POINT_PRESENTATION_V1: u8 = 1;
    pub(super) const FIELD_EFFECT_GRAPH_V1: u8 = 1;
    pub(super) const POINT_ABSENCE_BYPASS_OWN_BACKDROP_V1: u8 = 1;
    #[cfg(test)]
    pub(super) const FAMILY_CERTIFICATE_VERTEX_V1: u8 = 1;
    pub(super) const FAMILY_SEMANTIC_RELEASE_VERTEX_V2: u8 = 2;
    #[cfg(test)]
    pub(super) const MODELED_LCS_OCCURRENCE_V1: u8 = 1;
    #[cfg(test)]
    pub(super) const MODELED_LCS_OCCURRENCE_MUTATION_SENTINEL_V1: u8 = 2;

    pub(super) const IEC_SRGB8_D65_OUTPUT_PROFILE_V1: u8 = 1;
    #[cfg(test)]
    pub(super) const IEC_SRGB8_TO_XYZ_D65_TRANSFORM_V1: u8 = 1;
    pub(super) const CIE1931_TWO_DEGREE_OBSERVER_V1: u8 = 1;
    pub(super) const IEC61966_D65_REFERENCE_WHITE_V1: u8 = 1;
    pub(super) const RELATIVE_Y1_SCALE_V1: u8 = 1;
    pub(super) const XYZ_FRAME_V1: u8 = 1;
    #[cfg(test)]
    pub(super) const MUTATION_SENTINEL_FRAME_V1: u8 = 2;
    pub(super) const CIECAM16_VIEWING_INPUTS_V1: u8 = 1;
    pub(super) const ENCODED_SRGB8_SOURCE_OVER_V1: u8 = 1;
    pub(super) const RASTER_SRGB8_SOURCE_OVER_V1: u8 = 2;
    pub(super) const SURROUND_AVERAGE_V1: u8 = 1;
    pub(super) const SURROUND_DIM_V1: u8 = 2;
    pub(super) const SURROUND_DARK_V1: u8 = 3;

    pub(super) const FIELD_WORKING_ENCODED_SRGB8_PREMULTIPLIED_V1: u8 = 1;
    pub(super) const FIELD_WORKING_LINEAR_SRGB_Q31_PREMULTIPLIED_V1: u8 = 2;
    pub(super) const FIELD_PRECISION_FIXED_Q32_V1: u8 = 1;
    pub(super) const FIELD_PRECISION_BINARY32_V1: u8 = 2;
    pub(super) const FIELD_QUANTIZATION_HALF_UP_SRGB8_V1: u8 = 1;
    pub(super) const FIELD_QUANTIZATION_TIES_EVEN_SRGB8_V1: u8 = 2;
    pub(super) const FIELD_CARRIER_PRESENT_V1: u8 = 1;
    pub(super) const FIELD_CARRIER_CONTRIBUTES_V1: u8 = 2;
    pub(super) const FIELD_CARRIER_SPATIAL_VARIATION_V1: u8 = 3;
    pub(super) const FIELD_GAUSSIAN_BINOMIAL_Q32_V1: u8 = 1;
    pub(super) const FIELD_GAUSSIAN_CLAMP_TO_EDGE_V1: u8 = 1;

    pub(super) const EXACT_SRGB8_FAMILY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_IDENTITY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_RELEASE_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_CAPABILITY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_INTRINSIC_UNARY_FAMILY_V1: u8 = 4;
    pub(super) const EXACT_SRGB8_INTRINSIC_UNARY_IDENTITY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_INTRINSIC_UNARY_RELEASE_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_INTRINSIC_UNARY_CAPABILITY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_RELATION_FAMILY_V1: u8 = 5;
    pub(super) const EXACT_SRGB8_RELATION_IDENTITY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_RELATION_RELEASE_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_RELATION_CAPABILITY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_DISTINCTION_RELATION_FAMILY_V1: u8 = 8;
    pub(super) const EXACT_SRGB8_DISTINCTION_RELATION_IDENTITY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_DISTINCTION_RELATION_RELEASE_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_DISTINCTION_RELATION_CAPABILITY_V1: u8 = 1;
    pub(super) const FAMILY_CATEGORY_RELATION_FAMILY_V1: u8 = 9;
    pub(super) const FAMILY_CATEGORY_RELATION_IDENTITY_V1: u8 = 1;
    pub(super) const FAMILY_CATEGORY_RELATION_RELEASE_V1: u8 = 1;
    pub(super) const FAMILY_CATEGORY_RELATION_CAPABILITY_V1: u8 = 1;
    pub(super) const VISIBLE_SEPARATION_FAMILY_V1: u8 = 10;
    pub(super) const PAIR_SEPARATION_FAMILY_V1: u8 = 12;
    pub(super) const NOMINAL_SRGB8_VISIBLE_FAMILY_V1: u8 = 11;
    pub(super) const VISIBLE_SEPARATION_IDENTITY_V1: u8 = 1;
    pub(super) const VISIBLE_SEPARATION_RELEASE_V1: u8 = 1;
    pub(super) const VISIBLE_SEPARATION_CAPABILITY_V1: u8 = 1;
    pub(super) const FAMILY_MEMBERSHIP_FAMILY_V1: u8 = 6;
    pub(super) const FAMILY_MEMBERSHIP_IDENTITY_V1: u8 = 1;
    #[cfg(test)]
    pub(super) const FAMILY_MEMBERSHIP_RELEASE_V1: u8 = 1;
    pub(super) const FAMILY_MEMBERSHIP_RELEASE_V2: u8 = 2;
    pub(super) const FAMILY_MEMBERSHIP_CAPABILITY_V1: u8 = 1;
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
    pub(super) const DECLARED_SRGB8_CLEAN_SET_FAMILY_V1: u8 = 3;
    #[cfg(test)]
    pub(super) const MODELED_LCS_PROBE_FAMILY_V1: u8 = 7;
}

/// Устойчивый к коллизиям адрес канонизированного содержимого Program V9.
///
/// SHA-256 не делает адрес инъективным. Адрес не связывает пространства opaque
/// ID и не подтверждает владельца, поколение либо revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ProgramContentIdentityV9([u8; 32]);

impl ProgramContentIdentityV9 {
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Derived from the compiled physical component, never from target names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CanonicalPreferenceV1 {
    Ordered(u64),
    JointPoint,
    LegacyReferenceOrdered(u64),
    JointReferencePeers,
}

/// Temporary compiler input, sorted by target ID for binding lookup only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ComputedTargetIdentityPreferenceV1 {
    pub(crate) target: TargetId,
    pub(crate) preference: CanonicalPreferenceV1,
}

/// Typed execution projections from one completed canonical form. The storage
/// and observation-schema indexes remain separate from these scheduling ranks.
/// Symmetric vertices retain separate bindings; their representative order is
/// not a semantic tie-breaker between signed per-use outcomes.
#[derive(Debug)]
pub(crate) struct CanonicalExecutionLayoutV1 {
    pub(crate) targets: Box<[TargetId]>,
    pub(crate) inputs: Box<[SurfaceInputPortId]>,
}

#[derive(Debug)]
pub(crate) struct CompiledProgramIdentityV1 {
    pub(crate) content: ProgramOwnerIdentityV1,
    pub(crate) execution: Option<CanonicalExecutionLayoutV1>,
    pub(crate) peer_order: Option<ProgramIdentityGraphV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct VertexColorV1 {
    // Длина предшествует payload: сравнение ключей сохраняет порядок V9
    // и выполняется одной операцией над фиксированным массивом.
    bytes: [u8; COLOR_CAPACITY + 1],
}

impl VertexColorV1 {
    fn new(tag: u8) -> Self {
        let mut value = Self {
            bytes: [0; COLOR_CAPACITY + 1],
        };
        value.bytes[0] = 1;
        value.bytes[1] = tag;
        value
    }

    fn len(&self) -> u8 {
        self.bytes[0]
    }

    fn push_u8(&mut self, value: u8) -> Result<(), ProgramCompileError> {
        self.push_bytes(&[value])
    }

    fn push_bytes(&mut self, values: &[u8]) -> Result<(), ProgramCompileError> {
        let start = usize::from(self.len()) + 1;
        let end = start
            .checked_add(values.len())
            .ok_or(ProgramCompileError::InternalInvariant)?;
        self.bytes
            .get_mut(start..end)
            .ok_or(ProgramCompileError::InternalInvariant)?
            .copy_from_slice(values);
        self.bytes[0] =
            u8::try_from(end - 1).map_err(|_| ProgramCompileError::InternalInvariant)?;
        Ok(())
    }

    fn push_u64(&mut self, value: u64) -> Result<(), ProgramCompileError> {
        self.push_bytes(&value.to_be_bytes())
    }

    fn push_srgb8(&mut self, value: Srgb8) -> Result<(), ProgramCompileError> {
        self.push_bytes(&value.bytes())
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[1..=usize::from(self.len())]
    }

    const fn tag(&self) -> u8 {
        self.bytes[1]
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
    pub(super) const PRESENTATION_ROOT: u8 = 20;
    pub(super) const PRESENTATION_TARGET: u8 = 21;
    pub(super) const FAMILY: u8 = 22;
    pub(super) const FIELD_INPUT_PREMULTIPLIED_RGBA8: u8 = 23;
    pub(super) const FIELD_INPUT_ENCODED_SRGB8_ALPHA: u8 = 24;
    pub(super) const FIELD_INPUT_OPAQUE_SRGB8: u8 = 25;
    pub(super) const FIELD_OPERATOR_GAUSSIAN_Q32: u8 = 26;
    pub(super) const FIELD_OPERATOR_SOURCE_OVER: u8 = 27;
    pub(super) const FIELD_OPERATOR_SCREEN: u8 = 28;
    pub(super) const FIELD_OPERATOR_LIGHTER: u8 = 29;
    pub(super) const TARGET_COMPUTED: u8 = 30;
    pub(super) const TARGET_COLOR: u8 = 31;
    pub(super) const TONAL_HIERARCHY_ENDPOINT: u8 = 32;
}

macro_rules! declare_edge_roles_v1 {
    ($($name:ident = $value:literal),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        #[repr(u8)]
        enum EdgeRoleV1 {
            $($name = $value),+
        }

        #[cfg(test)]
        const EDGE_ROLES_V1: &[EdgeRoleV1] = &[$(EdgeRoleV1::$name),+];
    };
}

declare_edge_roles_v1! {
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
    VisibleUnary = 12,
    OutputPaint = 13,
    SelectionState = 14,
    StateChoice = 15,
    ChoiceTarget = 16,
    ChoiceCandidate = 17,
    PresentationRootTerminal = 18,
    PresentationTargetRoot = 19,
    PresentationTargetOccurrence = 20,
    ConstraintPresentationTarget = 21,
    IntrinsicUnary = 22,
    IntrinsicReference = 23,
    IntrinsicCandidate = 24,
    VisibleReference = 25,
    VisibleCandidate = 26,
    ObservationGroup = 27,
    ConstraintFamily = 28,
    FieldInputObservationGroup = 29,
    FieldGaussianSource = 30,
    FieldSourceOverSource = 31,
    FieldSourceOverDestination = 32,
    FieldScreenSource = 33,
    FieldScreenBackdrop = 34,
    FieldLighterSource = 35,
    FieldLighterDestination = 36,
    FieldOutputOperator = 37,
    TargetToneInput = 38,
    TargetAppearance = 39,
    ComputedRelationMember = 40,
    ComputedRelationOccurrence = 41,
}

type CanonicalGraphV1 = crate::colored_graph::ColoredGraph<VertexColorV1>;

#[derive(Debug)]
pub(crate) struct ProgramIdentityGraphV1 {
    hard: Vec<(ConstraintId, usize)>,
    graph: CanonicalGraphV1,
    computed_targets: Vec<(TargetId, usize)>,
    inputs: Vec<(SurfaceInputPortId, usize)>,
}

impl ProgramIdentityGraphV1 {
    fn execution_layout(
        &mut self,
        order: &[usize],
    ) -> Result<CanonicalExecutionLayoutV1, ProgramCompileError> {
        let vertex_count = self.graph.vertex_count();
        if order.len() != vertex_count {
            return Err(ProgramCompileError::InternalInvariant);
        }
        let mut rank_of = Vec::new();
        rank_of
            .try_reserve_exact(vertex_count)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        rank_of.resize(vertex_count, usize::MAX);
        for (rank, &vertex) in order.iter().enumerate() {
            let slot = rank_of
                .get_mut(vertex)
                .ok_or(ProgramCompileError::InternalInvariant)?;
            if *slot != usize::MAX {
                return Err(ProgramCompileError::InternalInvariant);
            }
            *slot = rank;
        }
        fn project<Key: Copy>(
            bindings: &mut [(Key, usize)],
            rank_of: &[usize],
        ) -> Result<Box<[Key]>, ProgramCompileError> {
            if bindings.iter().any(|&(_, vertex)| vertex >= rank_of.len()) {
                return Err(ProgramCompileError::InternalInvariant);
            }
            ordering::sort_by_key(bindings, |&(_, vertex)| rank_of[vertex]);
            let mut keys = Vec::new();
            keys.try_reserve_exact(bindings.len())
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            keys.extend(bindings.iter().map(|&(key, _)| key));
            Ok(keys.into_boxed_slice())
        }
        Ok(CanonicalExecutionLayoutV1 {
            targets: project(&mut self.computed_targets, &rank_of)?,
            inputs: project(&mut self.inputs, &rank_of)?,
        })
    }
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
        self.edges.push(EdgeV1 {
            from,
            to,
            role: role as u8,
        });
        Ok(())
    }

    fn finish(self) -> Result<CanonicalGraphV1, ProgramCompileError> {
        crate::colored_graph::ColoredGraph::new(self.colors, self.edges)
            .map_err(canonical_graph_error)
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
        ordering::sort_by_key(&mut self.values, |(key, _)| *key);
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
    program_root_color_for(false)
}

fn program_root_color_for(
    authored_composition: bool,
) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(vertex_tag::PROGRAM);
    // Эти теги связывают адрес с версиями исполняемых законов: схемой Program,
    // политикой выбора, финальной перепроверкой, атомарным наблюдением,
    // маршрутизацией кодированного `Paint` в output slot и моделируемым
    // представлением точки. Версии
    // производных возможностей связываются только с теми ограничениями,
    // которые их исполняют.
    for release in [
        release_tag::PROGRAM_SCHEMA_V9,
        if authored_composition {
            release_tag::AUTHORED_COMPOSITION_V1
        } else {
            release_tag::DECLARED_TOTAL_ORDER_V1
        },
        release_tag::FRESH_FULL_RECHECK_V1,
        release_tag::ATOMIC_OBSERVATION_GROUP_V1,
        release_tag::ENCODED_PAINT_OUTPUT_ROUTING_V1,
        release_tag::FINITE_ATOMIC_PAINT_CANDIDATE_V1,
        release_tag::MODELED_POINT_PRESENTATION_V1,
        release_tag::FIELD_EFFECT_GRAPH_V1,
    ] {
        color.push_u8(release)?;
    }
    Ok(color)
}

fn joint_selection_color(
    selection: &MaterialisedSelectionV1,
) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(vertex_tag::JOINT_SELECTION);
    color.push_u8(release_tag::SELECTION_RELEASE_IDENTITY_V1)?;
    color.push_bytes(selection.release_identity().as_bytes())?;
    Ok(color)
}

fn presentation_target_color(
    target: PointPresentationTargetV1,
) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(vertex_tag::PRESENTATION_TARGET);
    color.push_u8(match target.absence_release() {
        PointOccurrenceAbsenceReleaseV1::BypassOwnBackdropV1 => {
            release_tag::POINT_ABSENCE_BYPASS_OWN_BACKDROP_V1
        }
    })?;
    Ok(color)
}

fn write_signal(color: &mut VertexColorV1, signal: ColorSignal) -> Result<(), ProgramCompileError> {
    let profile = match signal.output_profile() {
        crate::lcs_occurrence::OutputProfileId::Iec61966Srgb8D65V1 => {
            release_tag::IEC_SRGB8_D65_OUTPUT_PROFILE_V1
        }
    };
    color.push_u8(profile)?;
    color.push_srgb8(signal.srgb8())
}

fn source_color(source: Source) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(vertex_tag::SOURCE);
    write_signal(&mut color, source.signal())?;
    Ok(color)
}

fn family_color(family: &FamilyDeclarationV2) -> Result<VertexColorV1, ProgramCompileError> {
    let semantic = family.semantic();
    let mut color = VertexColorV1::new(vertex_tag::FAMILY);
    color.push_u8(release_tag::FAMILY_SEMANTIC_RELEASE_VERTEX_V2)?;
    color.push_bytes(semantic.as_bytes())?;
    Ok(color)
}

fn candidate_color(candidate: TargetCandidateV1) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(vertex_tag::CANDIDATE);
    let value = candidate.value();
    write_signal(&mut color, ColorSignal::from_srgb8(value.source()))?;
    color.push_u64(value.opacity_bits())?;
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
        CompositionProfile::RasterSrgb8SourceOverV1 => release_tag::RASTER_SRGB8_SOURCE_OVER_V1,
    })?;
    write_context(&mut color, occurrence.context())?;
    Ok(color)
}

fn field_input_color(input: FieldInputDeclarationV1) -> VertexColorV1 {
    VertexColorV1::new(match input.kind() {
        crate::field_effect::FieldInputKindV1::PremultipliedRgba8V1 => {
            vertex_tag::FIELD_INPUT_PREMULTIPLIED_RGBA8
        }
        crate::field_effect::FieldInputKindV1::EncodedSrgb8AlphaV1 => {
            vertex_tag::FIELD_INPUT_ENCODED_SRGB8_ALPHA
        }
        crate::field_effect::FieldInputKindV1::OpaqueSrgb8V1 => {
            vertex_tag::FIELD_INPUT_OPAQUE_SRGB8
        }
    })
}

fn field_operator_color(
    operator: FieldOperatorDeclarationV1,
) -> Result<VertexColorV1, ProgramCompileError> {
    use crate::field_effect::{
        CarrierIntentV1, FieldExecutionProfileV1, FieldPrecisionV1, FieldQuantizationV1,
        FieldWorkingSpaceV1, GaussianEdgeModeV1, GaussianKernelProfileV1,
    };

    let mut color = VertexColorV1::new(match operator.execution() {
        FieldExecutionProfileV1::GaussianBlur { .. } => vertex_tag::FIELD_OPERATOR_GAUSSIAN_Q32,
        FieldExecutionProfileV1::PremultipliedSourceOver { .. } => {
            vertex_tag::FIELD_OPERATOR_SOURCE_OVER
        }
        FieldExecutionProfileV1::EncodedSrgb8ScreenOpaqueBackdrop { .. } => {
            vertex_tag::FIELD_OPERATOR_SCREEN
        }
        FieldExecutionProfileV1::PorterDuffLighter { .. } => vertex_tag::FIELD_OPERATOR_LIGHTER,
    });
    color.push_u8(match operator.working_space() {
        FieldWorkingSpaceV1::EncodedSrgb8PremultipliedV1 => {
            release_tag::FIELD_WORKING_ENCODED_SRGB8_PREMULTIPLIED_V1
        }
        FieldWorkingSpaceV1::LinearSrgbQ31PremultipliedV1 => {
            release_tag::FIELD_WORKING_LINEAR_SRGB_Q31_PREMULTIPLIED_V1
        }
    })?;
    color.push_u8(match operator.precision() {
        FieldPrecisionV1::FixedQ32V1 => release_tag::FIELD_PRECISION_FIXED_Q32_V1,
        FieldPrecisionV1::Binary32V1 => release_tag::FIELD_PRECISION_BINARY32_V1,
    })?;
    color.push_u8(match operator.quantization() {
        FieldQuantizationV1::RoundHalfUpSrgb8V1 => release_tag::FIELD_QUANTIZATION_HALF_UP_SRGB8_V1,
        FieldQuantizationV1::RoundTiesToEvenSrgb8V1 => {
            release_tag::FIELD_QUANTIZATION_TIES_EVEN_SRGB8_V1
        }
    })?;
    color.push_u8(match operator.carrier_intent() {
        CarrierIntentV1::Present => release_tag::FIELD_CARRIER_PRESENT_V1,
        CarrierIntentV1::Contributes => release_tag::FIELD_CARRIER_CONTRIBUTES_V1,
        CarrierIntentV1::SpatialVariation => release_tag::FIELD_CARRIER_SPATIAL_VARIATION_V1,
    })?;
    if let FieldExecutionProfileV1::GaussianBlur {
        kernel,
        css_radius_px,
        edge_mode,
        ..
    } = operator.execution()
    {
        color.push_u8(match kernel {
            GaussianKernelProfileV1::BinomialGaussianQ32V1 => {
                release_tag::FIELD_GAUSSIAN_BINOMIAL_Q32_V1
            }
        })?;
        color.push_u64(u64::from(css_radius_px))?;
        color.push_u8(match edge_mode {
            GaussianEdgeModeV1::ClampToEdgeV1 => release_tag::FIELD_GAUSSIAN_CLAMP_TO_EDGE_V1,
        })?;
    }
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
        ProgramConstraintContentV1::NominalSrgb8Visible { release } => {
            // The distinct family binds visible membership rather than owned
            // contribution. The full existing classifier release remains pinned.
            color.push_u8(release_tag::NOMINAL_SRGB8_VISIBLE_FAMILY_V1)?;
            color.push_bytes(&release)?;
        }
        ProgramConstraintContentV1::PairSeparation { bounds } => {
            color.push_u8(release_tag::PAIR_SEPARATION_FAMILY_V1)?;
            color.push_u64(bounds.minimum().to_bits())?;
            color.push_u64(bounds.maximum().to_bits())?;
        }
        ProgramConstraintContentV1::VisibleSeparation {
            identity,
            release,
            capability,
            bounds,
        } => {
            color.push_u8(release_tag::VISIBLE_SEPARATION_FAMILY_V1)?;
            color.push_u8(match identity {
                crate::constraints::VisibleSeparationIdentityV1 => {
                    release_tag::VISIBLE_SEPARATION_IDENTITY_V1
                }
            })?;
            color.push_u8(match release {
                crate::constraints::VisibleSeparationReleaseV1 => {
                    release_tag::VISIBLE_SEPARATION_RELEASE_V1
                }
            })?;
            color.push_u8(match capability {
                crate::constraints::VisibleSeparationCapabilityV1 => {
                    release_tag::VISIBLE_SEPARATION_CAPABILITY_V1
                }
            })?;
            color.push_u64(bounds.minimum().to_bits())?;
            color.push_u64(bounds.maximum().to_bits())?;
        }
        ProgramConstraintContentV1::ExactSrgb8 {
            identity,
            release,
            capability,
            expected,
            applicability: _,
            uncertainty: _,
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
            applicability: _,
            uncertainty: _,
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
        ProgramConstraintContentV1::ExactSrgb8IntrinsicUnary {
            identity,
            release,
            capability,
            expected,
            applicability: _,
            uncertainty: _,
        } => {
            color.push_u8(release_tag::EXACT_SRGB8_INTRINSIC_UNARY_FAMILY_V1)?;
            color.push_u8(match identity {
                crate::constraints::ExactSrgb8IntrinsicUnaryIdentityV1::SourceEqualityV1 => {
                    release_tag::EXACT_SRGB8_INTRINSIC_UNARY_IDENTITY_V1
                }
            })?;
            color.push_u8(match release {
                crate::constraints::ExactSrgb8IntrinsicUnaryReleaseV1::V1 => {
                    release_tag::EXACT_SRGB8_INTRINSIC_UNARY_RELEASE_V1
                }
            })?;
            color.push_u8(match capability {
                crate::constraints::ExactSrgb8IntrinsicUnaryCapabilityV1::EncodedSrgb8PaintSourceV1 => {
                    release_tag::EXACT_SRGB8_INTRINSIC_UNARY_CAPABILITY_V1
                }
            })?;
            color.push_srgb8(expected)?;
        }
        ProgramConstraintContentV1::FamilyMembership {
            identity,
            release,
            capability,
            applicability: _,
            uncertainty: _,
        } => {
            color.push_u8(release_tag::FAMILY_MEMBERSHIP_FAMILY_V1)?;
            color.push_u8(match identity {
                crate::constraints::FamilyMembershipIdentityV1::ExactImageMembershipV1 => {
                    release_tag::FAMILY_MEMBERSHIP_IDENTITY_V1
                }
            })?;
            color.push_u8(match release {
                crate::constraints::FamilyMembershipReleaseV2::V2 => {
                    release_tag::FAMILY_MEMBERSHIP_RELEASE_V2
                }
            })?;
            color.push_u8(match capability {
                crate::constraints::FamilyMembershipCapabilityV1::Iec61966Srgb8D65V1 => {
                    release_tag::FAMILY_MEMBERSHIP_CAPABILITY_V1
                }
            })?;
        }
        ProgramConstraintContentV1::ExactSrgb8Relation {
            identity,
            release,
            capability,
            applicability: _,
            uncertainty: _,
        } => {
            color.push_u8(release_tag::EXACT_SRGB8_RELATION_FAMILY_V1)?;
            color.push_u8(match identity {
                crate::constraints::ExactSrgb8RelationIdentityV1::HomogeneousEndpointEqualityV1 => {
                    release_tag::EXACT_SRGB8_RELATION_IDENTITY_V1
                }
            })?;
            color.push_u8(match release {
                crate::constraints::ExactSrgb8RelationReleaseV1::V1 => {
                    release_tag::EXACT_SRGB8_RELATION_RELEASE_V1
                }
            })?;
            color.push_u8(match capability {
                crate::constraints::ExactSrgb8RelationCapabilityV1::HomogeneousEncodedSrgb8PairV1 => {
                    release_tag::EXACT_SRGB8_RELATION_CAPABILITY_V1
                }
            })?;
        }
        ProgramConstraintContentV1::ExactSrgb8DistinctionRelation {
            identity,
            release,
            capability,
            applicability: _,
            uncertainty: _,
        } => {
            color.push_u8(release_tag::EXACT_SRGB8_DISTINCTION_RELATION_FAMILY_V1)?;
            color.push_u8(match identity {
                crate::constraints::ExactSrgb8DistinctionIdentityV1::HomogeneousEndpointInequalityV1 => {
                    release_tag::EXACT_SRGB8_DISTINCTION_RELATION_IDENTITY_V1
                }
            })?;
            color.push_u8(match release {
                crate::constraints::ExactSrgb8DistinctionReleaseV1::V1 => {
                    release_tag::EXACT_SRGB8_DISTINCTION_RELATION_RELEASE_V1
                }
            })?;
            color.push_u8(match capability {
                crate::constraints::ExactSrgb8DistinctionCapabilityV1::HomogeneousEncodedSrgb8PairV1 => {
                    release_tag::EXACT_SRGB8_DISTINCTION_RELATION_CAPABILITY_V1
                }
            })?;
        }
        ProgramConstraintContentV1::FamilyCategoryRelation {
            identity,
            release,
            capability,
            applicability: _,
            uncertainty: _,
        } => {
            color.push_u8(release_tag::FAMILY_CATEGORY_RELATION_FAMILY_V1)?;
            color.push_u8(match identity {
                crate::constraints::FamilyCategoryRelationIdentityV1::HomogeneousEndpointCategoryMembershipV1 => {
                    release_tag::FAMILY_CATEGORY_RELATION_IDENTITY_V1
                }
            })?;
            color.push_u8(match release {
                crate::constraints::FamilyCategoryRelationReleaseV1::V1 => {
                    release_tag::FAMILY_CATEGORY_RELATION_RELEASE_V1
                }
            })?;
            color.push_u8(match capability {
                crate::constraints::FamilyCategoryRelationCapabilityV1::HomogeneousIec61966Srgb8D65PairV1 => {
                    release_tag::FAMILY_CATEGORY_RELATION_CAPABILITY_V1
                }
            })?;
        }
        #[cfg(test)]
        ProgramConstraintContentV1::ModeledLcsProbe {
            release,
            applicability: _,
            uncertainty: _,
        } => {
            color.push_u8(release_tag::MODELED_LCS_PROBE_FAMILY_V1)?;
            color.push_u8(match release.modeled_lcs_release() {
                crate::lcs_occurrence::ModeledLcsOccurrenceReleaseId::V1 => {
                    release_tag::MODELED_LCS_OCCURRENCE_V1
                }
                crate::lcs_occurrence::ModeledLcsOccurrenceReleaseId::MutationSentinelV1 => {
                    release_tag::MODELED_LCS_OCCURRENCE_MUTATION_SENTINEL_V1
                }
            })?;
            color.push_u8(match release.transform_release() {
                crate::lcs_occurrence::ColorimetricTransformReleaseId::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1 => {
                    release_tag::IEC_SRGB8_TO_XYZ_D65_TRANSFORM_V1
                }
            })?;
        }
        #[cfg(test)]
        ProgramConstraintContentV1::FinalRecheckMutantExactSrgb8 {
            expected,
            applicability: _,
            uncertainty: _,
        } => {
            for tag in [0xFE_u8, 1, 1, 1] {
                color.push_u8(tag)?;
            }
            color.push_srgb8(expected)?;
        }
    }
    Ok(color)
}

fn declared_srgb8_clean_set_constraint_color(
    mode_tag: u8,
) -> Result<VertexColorV1, ProgramCompileError> {
    declared_srgb8_clean_set_constraint_color_for_release(
        mode_tag,
        crate::clean_set::EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2,
    )
}

#[cfg(test)]
fn clean_set_final_recheck_mutant_release_v1() -> [u8; 32] {
    let mut release = crate::clean_set::EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2;
    // Инъекция меняет один заранее объявленный бит: тест доказывает, что
    // final-recheck связывает новый release, а не случайно другой дайджест.
    release[0] ^= 1;
    release
}

fn declared_srgb8_clean_set_constraint_color_for_release(
    mode_tag: u8,
    release: [u8; 32],
) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(mode_tag);
    color.push_u8(release_tag::DECLARED_SRGB8_CLEAN_SET_FAMILY_V1)?;
    color.push_bytes(&release)?;
    Ok(color)
}

fn presentation_target_vertex(
    targets: &[(PointPresentationTargetV1, usize)],
    target: PointPresentationTargetV1,
) -> Result<usize, ProgramCompileError> {
    let key = (target.root(), target.occurrence());
    let index = targets
        .binary_search_by_key(&key, |(candidate, _)| {
            (candidate.root(), candidate.occurrence())
        })
        .map_err(|_| ProgramCompileError::InternalInvariant)?;
    let (candidate, vertex) = targets[index];
    if candidate != target {
        return Err(ProgramCompileError::InternalInvariant);
    }
    Ok(vertex)
}

fn field_value_vertex(
    inputs: &IdIndexV1<crate::field_effect::FieldInputPortIdV1>,
    operators: &IdIndexV1<crate::field_effect::FieldOperatorInstanceIdV1>,
    value: crate::field_effect::FieldValueRefV1,
) -> Result<usize, ProgramCompileError> {
    match value {
        crate::field_effect::FieldValueRefV1::Input(input) => inputs.get(input),
        crate::field_effect::FieldValueRefV1::Operator(operator) => operators.get(operator),
    }
}

struct ConstraintGraphBindingContextV1<'a, Evaluation> {
    evaluator: &'a Evaluation,
    presentation_targets: &'a [(PointPresentationTargetV1, usize)],
    targets: &'a IdIndexV1<TargetId>,
    families: &'a IdIndexV1<FamilyId>,
    occurrences: &'a IdIndexV1<OccurrenceId>,
    observation_group: usize,
}

fn add_constraint_graph_binding<Evaluation>(
    graph: &mut GraphBuilderV1,
    context: &ConstraintGraphBindingContextV1<'_, Evaluation>,
    mode_tag: u8,
    body: &ProgramConstraintBodyV1<ProgramConstraintInvocationOf<Evaluation>>,
) -> Result<usize, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    let color = match body {
        ProgramConstraintBodyV1::ComputedVisibleRelation { .. } => {
            let mut color = VertexColorV1::new(mode_tag);
            color.push_bytes(
                crate::sha256::digest(b"preserve-tonal-hierarchy.cancelled-background-ratio.v1")
                    .as_bytes(),
            )?;
            color
        }
        ProgramConstraintBodyV1::VisibleUnary { invocation, .. } => {
            constraint_color(mode_tag, context.evaluator.constraint_content(*invocation))?
        }
        ProgramConstraintBodyV1::IntrinsicUnary { invocation, .. } => {
            constraint_color(mode_tag, invocation.content())?
        }
        ProgramConstraintBodyV1::IntrinsicRelation { invocation, .. }
        | ProgramConstraintBodyV1::VisibleRelation { invocation, .. } => {
            constraint_color(mode_tag, invocation.content())?
        }
        ProgramConstraintBodyV1::DeclaredSrgb8CleanSet { .. } => {
            declared_srgb8_clean_set_constraint_color(mode_tag)?
        }
        #[cfg(test)]
        ProgramConstraintBodyV1::DeclaredSrgb8CleanSetFinalRecheckMutant { .. } => {
            declared_srgb8_clean_set_constraint_color_for_release(
                mode_tag,
                clean_set_final_recheck_mutant_release_v1(),
            )?
        }
    };
    let vertex = graph.add_member(color)?;
    match body {
        ProgramConstraintBodyV1::ComputedVisibleRelation { occurrences, .. } => {
            for (ordinal, occurrence) in occurrences.iter().enumerate() {
                let mut color = VertexColorV1::new(vertex_tag::TONAL_HIERARCHY_ENDPOINT);
                color.push_u64(ordinal as u64)?;
                let member = graph.add_member(color)?;
                graph.add_edge(vertex, member, EdgeRoleV1::ComputedRelationMember)?;
                graph.add_edge(
                    member,
                    context.occurrences.get(*occurrence)?,
                    EdgeRoleV1::ComputedRelationOccurrence,
                )?;
            }
        }
        ProgramConstraintBodyV1::VisibleUnary { occurrence, .. } => {
            graph.add_edge(
                vertex,
                context.occurrences.get(*occurrence)?,
                EdgeRoleV1::VisibleUnary,
            )?;
        }
        ProgramConstraintBodyV1::IntrinsicUnary { target, invocation } => {
            graph.add_edge(
                vertex,
                context.targets.get(*target)?,
                EdgeRoleV1::IntrinsicUnary,
            )?;
            if let CoreIntrinsicUnaryInvocationV1::FamilyMembership { family } = invocation {
                graph.add_edge(
                    vertex,
                    context.families.get(*family)?,
                    EdgeRoleV1::ConstraintFamily,
                )?;
            }
        }
        ProgramConstraintBodyV1::IntrinsicRelation {
            relation,
            invocation,
        } => {
            graph.add_edge(
                vertex,
                context.targets.get(relation.reference())?,
                EdgeRoleV1::IntrinsicReference,
            )?;
            for candidate in relation.candidates().iter().copied() {
                graph.add_edge(
                    vertex,
                    context.targets.get(candidate)?,
                    EdgeRoleV1::IntrinsicCandidate,
                )?;
            }
            if let CoreRelationInvocationV1::FamilyCategory { family } = invocation {
                graph.add_edge(
                    vertex,
                    context.families.get(*family)?,
                    EdgeRoleV1::ConstraintFamily,
                )?;
            }
        }
        ProgramConstraintBodyV1::VisibleRelation {
            relation,
            invocation,
        } => {
            graph.add_edge(
                vertex,
                context.occurrences.get(relation.reference())?,
                if matches!(invocation, CoreRelationInvocationV1::PairSeparation(_)) {
                    EdgeRoleV1::VisibleCandidate
                } else {
                    EdgeRoleV1::VisibleReference
                },
            )?;
            for candidate in relation.candidates().iter().copied() {
                graph.add_edge(
                    vertex,
                    context.occurrences.get(candidate)?,
                    EdgeRoleV1::VisibleCandidate,
                )?;
            }
            if let CoreRelationInvocationV1::FamilyCategory { family } = invocation {
                graph.add_edge(
                    vertex,
                    context.families.get(*family)?,
                    EdgeRoleV1::ConstraintFamily,
                )?;
            }
        }
        ProgramConstraintBodyV1::DeclaredSrgb8CleanSet { target } => {
            graph.add_edge(
                vertex,
                presentation_target_vertex(context.presentation_targets, *target)?,
                EdgeRoleV1::ConstraintPresentationTarget,
            )?;
        }
        #[cfg(test)]
        ProgramConstraintBodyV1::DeclaredSrgb8CleanSetFinalRecheckMutant { target } => {
            graph.add_edge(
                vertex,
                presentation_target_vertex(context.presentation_targets, *target)?,
                EdgeRoleV1::ConstraintPresentationTarget,
            )?;
        }
    }
    graph.add_edge(
        vertex,
        context.observation_group,
        EdgeRoleV1::ObservationGroup,
    )?;
    Ok(vertex)
}

fn build_graph<Evaluation>(
    program: &Program<Evaluation>,
) -> Result<CanonicalGraphV1, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    build_identity_graph(program, None).map(|built| built.graph)
}

fn build_identity_graph<Evaluation>(
    program: &Program<Evaluation>,
    preferences: Option<&[ComputedTargetIdentityPreferenceV1]>,
) -> Result<ProgramIdentityGraphV1, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    if let Some(preferences) = preferences {
        let computed = program
            .targets
            .iter()
            .filter(|target| matches!(target.intent(), TargetIntentV1::Computed(_)))
            .count();
        if preferences.len() != computed
            || preferences
                .windows(2)
                .any(|pair| pair[0].target >= pair[1].target)
            || preferences
                .iter()
                .any(|entry| matches!(entry.preference, CanonicalPreferenceV1::Ordered(_)))
        {
            return Err(ProgramCompileError::InternalInvariant);
        }
    }
    let root = if preferences.is_some() {
        program_root_color_for(true)?
    } else {
        program_root_color()?
    };
    let mut graph = GraphBuilderV1::new(root)?;
    let mut sources = IdIndexV1::new();
    let mut targets = IdIndexV1::new();
    let mut computed_targets = IdIndexV1::new();
    let mut families = IdIndexV1::new();
    let mut candidates = IdIndexV1::new();
    let mut opacities = IdIndexV1::new();
    let mut paints = IdIndexV1::new();
    let mut ports = IdIndexV1::new();
    let mut surfaces = IdIndexV1::new();
    let mut occurrences = IdIndexV1::new();
    let mut field_inputs = IdIndexV1::new();
    let mut field_operators = IdIndexV1::new();
    let mut presentation_roots = IdIndexV1::new();
    let mut presentation_targets = Vec::new();
    presentation_targets
        .try_reserve_exact(program.presentation_targets.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;

    for source in &program.sources {
        sources.insert(source.id(), graph.add_member(source_color(*source)?)?)?;
    }
    for target in &program.targets {
        let target_id = target.id();
        let target_color = match target.intent() {
            TargetIntentV1::FixedSource(_) => VertexColorV1::new(vertex_tag::TARGET_FIXED),
            TargetIntentV1::Finite(_) => VertexColorV1::new(vertex_tag::TARGET_FINITE),
            TargetIntentV1::Computed(target) => {
                let preference = preferences
                    .map(|entries| {
                        entries
                            .binary_search_by_key(&target_id, |entry| entry.target)
                            .map(|index| entries[index].preference)
                            .map_err(|_| ProgramCompileError::InternalInvariant)
                    })
                    .transpose()?;
                let mut color = VertexColorV1::new(if target.color_intent().is_some() {
                    vertex_tag::TARGET_COLOR
                } else {
                    vertex_tag::TARGET_COMPUTED
                });
                if target.color_intent().is_some() {
                    color.push_bytes(&match preference {
                        Some(preference) => target.color_identity_hash_for(preference)?,
                        None => target.color_identity_hash()?,
                    })?;
                } else {
                    match preference {
                        Some(preference) => {
                            color.push_bytes(&target.identity_bytes_for(preference)?)?;
                        }
                        None => color.push_bytes(&target.identity_bytes())?,
                    }
                }
                color
            }
        };
        let target_vertex = graph.add_member(target_color)?;
        targets.insert(target.id(), target_vertex)?;
        if preferences.is_some() && matches!(target.intent(), TargetIntentV1::Computed(_)) {
            computed_targets.insert(target.id(), target_vertex)?;
        }
        if let TargetIntentV1::Finite(domain) = target.intent() {
            for candidate in domain.candidates() {
                let vertex = graph.add_member(candidate_color(*candidate)?)?;
                candidates.insert((target.id(), candidate.id()), vertex)?;
            }
        }
    }
    for family in &program.families {
        families.insert(family.id(), graph.add_member(family_color(family)?)?)?;
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
        let mut color = VertexColorV1::new(vertex_tag::SURFACE_INPUT_PORT);
        if let Some(inputs) = &program.reference_inputs {
            let index = inputs
                .binary_search_by_key(port, |entry| entry.0)
                .map_err(|_| ProgramCompileError::InternalInvariant)?;
            color.push_bytes(b"reference.v1")?;
            color.push_bytes(&inputs[index].1.bytes())?;
        }
        ports.insert(*port, graph.add_member(color)?)?;
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
    for input in program.field.inputs() {
        field_inputs.insert(input.id(), graph.add_member(field_input_color(*input))?)?;
    }
    for operator in program.field.operators() {
        field_operators.insert(
            operator.id(),
            graph.add_member(field_operator_color(*operator)?)?,
        )?;
    }
    for root in &program.presentation_roots {
        presentation_roots.insert(
            root.id(),
            graph.add_member(VertexColorV1::new(vertex_tag::PRESENTATION_ROOT))?,
        )?;
    }
    // `prepare_program` вызывает этот закрытый этап вычисления identity только
    // после сортировки пар `(root, occurrence)` и отклонения дубликатов в
    // `compile_point_presentations`. Повторная дедупликация здесь скрыла бы
    // ошибку декларации и разделила правило admission между двумя источниками истины.
    for target in &program.presentation_targets {
        presentation_targets.push((
            *target,
            graph.add_member(presentation_target_color(*target)?)?,
        ));
    }

    sources.finish()?;
    targets.finish()?;
    families.finish()?;
    candidates.finish()?;
    opacities.finish()?;
    paints.finish()?;
    ports.finish()?;
    surfaces.finish()?;
    occurrences.finish()?;
    field_inputs.finish()?;
    field_operators.finish()?;
    presentation_roots.finish()?;

    for target in &program.targets {
        let target_vertex = targets.get(target.id())?;
        if let TargetIntentV1::Computed(value) = target.intent() {
            if let Some(intent) = value.color_intent() {
                if let Some(tone) = &intent.tone {
                    graph.add_edge(
                        target_vertex,
                        ports.get(SurfaceInputPortId::new(tone.input()))?,
                        EdgeRoleV1::TargetToneInput,
                    )?;
                }
                if let Some(appearance) = intent.appearance {
                    graph.add_edge(
                        target_vertex,
                        occurrences.get(appearance)?,
                        EdgeRoleV1::TargetAppearance,
                    )?;
                }
            }
        }
        if let TargetIntentV1::FixedSource(source) = target.intent() {
            graph.add_edge(
                target_vertex,
                sources.get(*source)?,
                EdgeRoleV1::TargetSource,
            )?;
        }
        if let TargetIntentV1::Finite(domain) = target.intent() {
            for candidate in domain.candidates() {
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
    for input in program.field.inputs() {
        graph.add_edge(
            group,
            field_inputs.get(input.id())?,
            EdgeRoleV1::FieldInputObservationGroup,
        )?;
    }
    for operator in program.field.operators() {
        use crate::field_effect::FieldExecutionProfileV1;

        let vertex = field_operators.get(operator.id())?;
        match operator.execution() {
            FieldExecutionProfileV1::GaussianBlur { source, .. } => graph.add_edge(
                vertex,
                field_value_vertex(&field_inputs, &field_operators, source)?,
                EdgeRoleV1::FieldGaussianSource,
            )?,
            FieldExecutionProfileV1::PremultipliedSourceOver {
                source,
                destination,
            } => {
                graph.add_edge(
                    vertex,
                    field_value_vertex(&field_inputs, &field_operators, source)?,
                    EdgeRoleV1::FieldSourceOverSource,
                )?;
                graph.add_edge(
                    vertex,
                    field_value_vertex(&field_inputs, &field_operators, destination)?,
                    EdgeRoleV1::FieldSourceOverDestination,
                )?;
            }
            FieldExecutionProfileV1::EncodedSrgb8ScreenOpaqueBackdrop { source, backdrop } => {
                graph.add_edge(
                    vertex,
                    field_value_vertex(&field_inputs, &field_operators, source)?,
                    EdgeRoleV1::FieldScreenSource,
                )?;
                graph.add_edge(
                    vertex,
                    field_value_vertex(&field_inputs, &field_operators, backdrop)?,
                    EdgeRoleV1::FieldScreenBackdrop,
                )?;
            }
            FieldExecutionProfileV1::PorterDuffLighter {
                source,
                destination,
            } => {
                graph.add_edge(
                    vertex,
                    field_value_vertex(&field_inputs, &field_operators, source)?,
                    EdgeRoleV1::FieldLighterSource,
                )?;
                graph.add_edge(
                    vertex,
                    field_value_vertex(&field_inputs, &field_operators, destination)?,
                    EdgeRoleV1::FieldLighterDestination,
                )?;
            }
        }
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
    for root in &program.presentation_roots {
        graph.add_edge(
            presentation_roots.get(root.id())?,
            occurrences.get(root.terminal())?,
            EdgeRoleV1::PresentationRootTerminal,
        )?;
    }
    for (target, vertex) in &presentation_targets {
        graph.add_edge(
            *vertex,
            presentation_roots.get(target.root())?,
            EdgeRoleV1::PresentationTargetRoot,
        )?;
        graph.add_edge(
            *vertex,
            occurrences.get(target.occurrence())?,
            EdgeRoleV1::PresentationTargetOccurrence,
        )?;
    }

    let constraint_context = ConstraintGraphBindingContextV1 {
        evaluator: &program.evaluator,
        presentation_targets: &presentation_targets,
        targets: &targets,
        families: &families,
        occurrences: &occurrences,
        observation_group: group,
    };
    let peers = preferences.is_some_and(|items| {
        items
            .iter()
            .any(|item| item.preference == CanonicalPreferenceV1::JointReferencePeers)
    });
    let mut hard = Vec::new();
    if peers {
        hard.try_reserve_exact(program.constraints.hard.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    }
    for constraint in &program.constraints.hard {
        let vertex = add_constraint_graph_binding(
            &mut graph,
            &constraint_context,
            vertex_tag::CONSTRAINT_HARD,
            constraint.body(),
        )?;
        if peers {
            hard.push((constraint.id(), vertex));
        }
    }
    hard.sort_unstable_by_key(|entry| entry.0);
    for constraint in &program.constraints.report_only {
        add_constraint_graph_binding(
            &mut graph,
            &constraint_context,
            vertex_tag::CONSTRAINT_REPORT_ONLY,
            constraint.body(),
        )?;
    }
    for output in &program.outputs {
        let vertex = graph.add_member(VertexColorV1::new(vertex_tag::OUTPUT))?;
        graph.add_edge(vertex, paints.get(output.paint())?, EdgeRoleV1::OutputPaint)?;
    }
    for output in &program.field_outputs {
        let vertex = graph.add_member(VertexColorV1::new(vertex_tag::OUTPUT))?;
        graph.add_edge(
            vertex,
            field_operators.get(output.operator())?,
            EdgeRoleV1::FieldOutputOperator,
        )?;
    }

    if let Some(selection) = &program.joint_selection {
        let selection_vertex = graph.add_member(joint_selection_color(selection)?)?;
        for (state_index, state) in selection.order().states().iter().enumerate() {
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

    Ok(ProgramIdentityGraphV1 {
        hard,
        graph: graph.finish()?,
        computed_targets: computed_targets.values,
        inputs: ports.values,
    })
}

#[cfg(test)]
pub(crate) fn graph_schema_for_test<Evaluation>(
    program: &Program<Evaluation>,
) -> Result<(Vec<u8>, Vec<u8>), ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let graph = build_graph(program)?;
    let mut vertex_tags = Vec::new();
    vertex_tags
        .try_reserve_exact(graph.colors().len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    vertex_tags.extend(graph.colors().iter().map(VertexColorV1::tag));
    ordering::sort(&mut vertex_tags);
    vertex_tags.dedup();

    let mut edge_roles = Vec::new();
    edge_roles
        .try_reserve_exact(graph.edge_count())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    edge_roles.extend(graph.edge_roles());
    ordering::sort(&mut edge_roles);
    edge_roles.dedup();
    Ok((vertex_tags, edge_roles))
}

#[cfg(test)]
pub(crate) const fn edge_role_count_for_test() -> usize {
    EDGE_ROLES_V1.len()
}

#[cfg(test)]
pub(crate) fn content_identity_with_clean_set_release_for_test<Evaluation>(
    program: &Program<Evaluation>,
    release: [u8; 32],
) -> Result<ProgramContentIdentityV9, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    // Pin the historical semantic input without adding an obsolete release to
    // executable Program declarations. Every other graph byte stays current.
    let mut graph = build_graph(program)?;
    let mut replaced = false;
    for mode in [
        vertex_tag::CONSTRAINT_HARD,
        vertex_tag::CONSTRAINT_REPORT_ONLY,
    ] {
        let current = declared_srgb8_clean_set_constraint_color(mode)?;
        for color in graph.colors_mut() {
            if *color == current {
                *color = declared_srgb8_clean_set_constraint_color_for_release(mode, release)?;
                replaced = true;
            }
        }
    }
    if !replaced {
        return Err(ProgramCompileError::InternalInvariant);
    }
    Ok(ProgramContentIdentityV9(
        *crate::sha256::digest(&canonical_preimage(&graph)?).as_bytes(),
    ))
}

impl AsRef<[u8]> for VertexColorV1 {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

fn canonical_graph_error(error: crate::colored_graph::CanonicalGraphError) -> ProgramCompileError {
    match error {
        crate::colored_graph::CanonicalGraphError::ResourceExhausted => {
            ProgramCompileError::ResourceExhausted
        }
        crate::colored_graph::CanonicalGraphError::InternalInvariant => {
            ProgramCompileError::InternalInvariant
        }
    }
}

fn canonical_preimage(graph: &CanonicalGraphV1) -> Result<Vec<u8>, ProgramCompileError> {
    crate::colored_graph::canonical_preimage(graph, DOMAIN_V9).map_err(canonical_graph_error)
}

pub(super) fn compile_program_content_identity_v9<Evaluation>(
    program: &Program<Evaluation>,
) -> Result<ProgramContentIdentityV9, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    if program.assignment.is_some()
        || program
            .targets
            .iter()
            .any(|target| matches!(target.intent(), TargetIntentV1::Computed(_)))
    {
        return Err(ProgramCompileError::InternalInvariant);
    }
    let graph = build_graph(program)?;
    let preimage = canonical_preimage(&graph)?;
    let digest = crate::sha256::digest(&preimage);
    Ok(ProgramContentIdentityV9(*digest.as_bytes()))
}

pub(super) fn compile_owner_identity<Evaluation>(
    program: &Program<Evaluation>,
) -> Result<ProgramOwnerIdentityV1, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    if program
        .assignment
        .as_ref()
        .is_some_and(ComputedSelectionPolicyV1::authored_composition)
    {
        return Err(ProgramCompileError::InternalInvariant);
    }
    if program.assignment.is_some() {
        return compile_computed_identity(program).map(ProgramOwnerIdentityV1::Computed);
    }
    compile_program_content_identity_v9(program).map(ProgramOwnerIdentityV1::Legacy)
}

pub(crate) fn compile_owner_identity_and_layout<Evaluation>(
    program: &Program<Evaluation>,
    preferences: &[ComputedTargetIdentityPreferenceV1],
) -> Result<CompiledProgramIdentityV1, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let authored_composition = program
        .assignment
        .as_ref()
        .is_some_and(ComputedSelectionPolicyV1::authored_composition);
    if !authored_composition {
        return Ok(CompiledProgramIdentityV1 {
            content: compile_owner_identity(program)?,
            execution: None,
            peer_order: None,
        });
    }
    let mut built = build_identity_graph(program, Some(preferences))?;
    let form = crate::colored_graph::canonical_form(&built.graph, AUTHORED_COMPOSITION_DOMAIN_V1)
        .map_err(canonical_graph_error)?;
    let content = ProgramOwnerIdentityV1::Computed(computed_assignment::ComputedIdentityV1(
        *crate::sha256::digest(&form.preimage).as_bytes(),
    ));
    // Retain only the leaf binding while projecting typed keys. The potentially
    // much larger serialization is no longer needed after its content digest.
    drop(form.preimage);
    let execution = built.execution_layout(&form.order)?;
    let peer_order = (!built.hard.is_empty()).then(|| {
        built.inputs.sort_unstable_by_key(|entry| entry.0);
        built.computed_targets.sort_unstable_by_key(|entry| entry.0);
        built
    });
    Ok(CompiledProgramIdentityV1 {
        content,
        execution: Some(execution),
        peer_order,
    })
}

fn compile_computed_identity<Evaluation>(
    program: &Program<Evaluation>,
) -> Result<computed_assignment::ComputedIdentityV1, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let graph = build_graph(program)?;
    let canonical = canonical_preimage(&graph)?;
    // Semantic identity связывает объявленную политику и граф. Предел работы
    // принадлежит отдельному execution profile; старый V9 preimage не изменён.
    let domain = b"labcolors.computed-program.v3.anytime.direct-oklab-f64.lex-rgb.full-recheck\0";
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(domain.len() + canonical.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(&canonical);
    Ok(computed_assignment::ComputedIdentityV1(
        *crate::sha256::digest(&bytes).as_bytes(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout_program(
        target_ids: [u32; 2],
        input_ids: [u32; 2],
        reverse: bool,
        authored: bool,
    ) -> Program<Wcag22Srgb8V1> {
        use crate::color_intent::{ColorIdentityDomainV1, IdentityOptionsV1};
        use crate::lcs_occurrence::{
            AdaptingLuminanceCdM2, AppearanceContextSchemaReleaseId, BackgroundLuminanceRatio,
            IEC_SRGB_D65_XYZ_FRAME_V1, SurroundProfileId,
        };
        let context = AppearanceContextId::from_inputs(
            AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
            IEC_SRGB_D65_XYZ_FRAME_V1,
            AdaptingLuminanceCdM2::try_new(64.0).expect("64 cd/m² is a valid adapting luminance"),
            BackgroundLuminanceRatio::try_new(0.2).expect("0.2 is a valid background ratio"),
            SurroundProfileId::AverageV1,
        );
        let mut targets = Vec::new();
        let mut paints = Vec::new();
        let mut surfaces = Vec::new();
        let mut occurrences = Vec::new();
        let mut outputs = Vec::new();
        for index in 0..2 {
            let source = Srgb8::new([64 + 128 * index as u8; 3]);
            let occurrence = OccurrenceId::new(50 + index as u32);
            let paint = PaintId::new(30 + index as u32);
            let surface = SurfaceId::new(40 + index as u32);
            let target = TargetId::new(target_ids[index]);
            targets.push(Target::new(
                target,
                TargetIntentV1::Computed(computed_assignment::ComputedTargetV1::color(
                    source,
                    1.0,
                    computed_assignment::ColorUseIntentV1 {
                        identity: ColorIdentityDomainV1::new(
                            source,
                            false,
                            IdentityOptionsV1::default(),
                        )
                        .expect("default identity admits the source color"),
                        appearance: Some(occurrence),
                        tone: None,
                    },
                )),
            ));
            paints.push(Paint::Solid { id: paint, target });
            surfaces.push(Surface::Input {
                id: surface,
                input: SurfaceInputPortId::new(input_ids[index]),
            });
            occurrences.push(Occurrence::new(
                occurrence,
                paint,
                surface,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context,
            ));
            outputs.push(OutputBinding::new(
                OutputSlotId::new(60 + index as u32),
                paint,
            ));
        }
        let mut inputs = input_ids.map(SurfaceInputPortId::new).to_vec();
        let mut order = target_ids.map(TargetId::new).to_vec();
        if reverse {
            targets.reverse();
            paints.reverse();
            surfaces.reverse();
            occurrences.reverse();
            outputs.reverse();
            inputs.reverse();
            order.reverse();
        }
        computed_assignment::assign_preference(&mut targets, Some(&order))
            .expect("fixture lists every adjustable target once");
        let mut program = Program::new(
            vec![],
            targets,
            ObservationGroup::new(ObservationGroupId::new(1), inputs),
            vec![],
            paints,
            surfaces,
            occurrences,
            ConstraintSet::new(vec![], vec![]),
            outputs,
            Wcag22Srgb8V1,
        );
        program.assignment = Some(if authored {
            ComputedSelectionPolicyV1::AuthoredComposition {
                legacy_reference_order: order,
            }
        } else {
            ComputedSelectionPolicyV1::Ordered(order)
        });
        program
    }

    fn layout_preferences(
        program: &Program<Wcag22Srgb8V1>,
        legacy: bool,
    ) -> Vec<ComputedTargetIdentityPreferenceV1> {
        let mut result: Vec<_> = program
            .assignment
            .as_ref()
            .expect("authored program declares a selection policy")
            .legacy_order()
            .iter()
            .enumerate()
            .map(|(rank, &target)| ComputedTargetIdentityPreferenceV1 {
                target,
                preference: if legacy {
                    CanonicalPreferenceV1::LegacyReferenceOrdered(rank as u64)
                } else {
                    CanonicalPreferenceV1::JointPoint
                },
            })
            .collect();
        result.sort_unstable_by_key(|entry| entry.target);
        result
    }

    #[test]
    fn authored_layout_ignores_chronology_and_binds_typed_renamed_targets_and_ports() {
        let baseline = layout_program([10, 20], [70, 80], false, true);
        let reversed = layout_program([10, 20], [70, 80], true, true);
        let renamed = layout_program([91, 7], [99, 3], true, true);
        let compile = |program: &Program<Wcag22Srgb8V1>| {
            compile_owner_identity_and_layout(program, &layout_preferences(program, false))
                .expect("fixture has a complete preference binding")
        };
        let baseline = compile(&baseline);
        let reversed = compile(&reversed);
        let renamed = compile(&renamed);
        assert_eq!(baseline.content, reversed.content);
        assert_eq!(baseline.content, renamed.content);
        let baseline = baseline
            .execution
            .expect("authored composition supplies a physical execution layout");
        let reversed = reversed
            .execution
            .expect("authored composition supplies a physical execution layout");
        let renamed = renamed
            .execution
            .expect("authored composition supplies a physical execution layout");
        assert_eq!(baseline.targets, reversed.targets);
        assert_eq!(baseline.inputs, reversed.inputs);
        let target_roles = |values: &[TargetId], ids: [u32; 2]| {
            values
                .iter()
                .map(|id| {
                    ids.map(TargetId::new)
                        .iter()
                        .position(|other| other == id)
                        .expect("compiled layout contains exactly the declared handles")
                })
                .collect::<Vec<_>>()
        };
        let input_roles = |values: &[SurfaceInputPortId], ids: [u32; 2]| {
            values
                .iter()
                .map(|id| {
                    ids.map(SurfaceInputPortId::new)
                        .iter()
                        .position(|other| other == id)
                        .expect("compiled layout contains exactly the declared handles")
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            target_roles(&baseline.targets, [10, 20]),
            target_roles(&renamed.targets, [91, 7])
        );
        assert_eq!(
            input_roles(&baseline.inputs, [70, 80]),
            input_roles(&renamed.inputs, [99, 3])
        );
    }

    #[test]
    fn ordered_and_legacy_reference_identities_keep_their_declared_preference() {
        let ordered = layout_program([10, 20], [70, 80], false, false);
        let reordered = layout_program([10, 20], [70, 80], true, false);
        let compiled = compile_owner_identity_and_layout(&ordered, &[])
            .expect("fixture has a complete preference binding");
        assert!(compiled.execution.is_none());
        assert_eq!(
            compiled.content,
            compile_owner_identity(&ordered).expect("fixture has a canonical program identity")
        );
        assert_ne!(
            compiled.content,
            compile_owner_identity(&reordered).expect("fixture has a canonical program identity")
        );
        let legacy = layout_program([10, 20], [70, 80], false, true);
        let reordered_legacy = layout_program([10, 20], [70, 80], true, true);
        let identity = |program: &Program<Wcag22Srgb8V1>| {
            compile_owner_identity_and_layout(program, &layout_preferences(program, true))
                .expect("fixture has a complete preference binding")
                .content
        };
        assert_ne!(identity(&legacy), identity(&reordered_legacy));
        assert_ne!(compiled.content, identity(&legacy));
        assert_ne!(
            identity(&legacy),
            compile_owner_identity_and_layout(&legacy, &layout_preferences(&legacy, false))
                .expect("fixture has a complete preference binding")
                .content,
        );
    }

    #[test]
    fn authored_identity_requires_a_complete_unique_typed_preference_binding() {
        let program = layout_program([10, 20], [70, 80], false, true);
        let valid = layout_preferences(&program, false);
        let mut duplicate = valid.clone();
        duplicate[1].target = duplicate[0].target;
        let mut absent = valid.clone();
        absent[1].target = TargetId::new(21);
        let mut unordered = valid.clone();
        unordered.reverse();
        let mut wrong_policy = valid.clone();
        wrong_policy[0].preference = CanonicalPreferenceV1::Ordered(0);
        for invalid in [
            vec![],
            valid[..1].to_vec(),
            duplicate,
            absent,
            unordered,
            wrong_policy,
        ] {
            assert_eq!(
                compile_owner_identity_and_layout(&program, &invalid).err(),
                Some(ProgramCompileError::InternalInvariant),
            );
        }
    }

    #[test]
    fn fixed_only_authored_composition_retains_its_policy_and_input_layout() {
        let mut program = layout_program([10, 20], [70, 80], false, true);
        for (index, target) in program.targets.iter_mut().enumerate() {
            let source = SourceId::new(600 + index as u32);
            let rgb = Srgb8::new([64 + 128 * index as u8; 3]);
            program
                .sources
                .push(Source::new(source, ColorSignal::from_srgb8(rgb)));
            target.intent = TargetIntentV1::FixedSource(source);
        }
        program.assignment = Some(ComputedSelectionPolicyV1::AuthoredComposition {
            legacy_reference_order: vec![],
        });
        let authored = compile_owner_identity_and_layout(&program, &[])
            .expect("fixture has a complete preference binding");
        let layout = authored
            .execution
            .expect("authored composition supplies a physical execution layout");
        assert!(layout.targets.is_empty());
        assert_eq!(layout.inputs.len(), 2);
        program.assignment = Some(ComputedSelectionPolicyV1::Ordered(vec![]));
        assert_ne!(
            authored.content,
            compile_owner_identity(&program).expect("fixture has a canonical program identity")
        );
    }

    #[test]
    fn joint_selection_vertex_binds_tag_and_exact_release_identity_bytes() {
        let key = crate::selection_release::SelectionCandidateKeyV1::new(
            b"only".to_vec().into_boxed_slice(),
        );
        let release = crate::selection_release::SelectionReleaseV1::new(
            9,
            vec![vec![key.clone()].into_boxed_slice()].into_boxed_slice(),
        );
        let admitted = crate::selection_release::admit_selection_release_v1(release)
            .expect("single-key release must admit");
        let materialised = crate::selection_release::materialise_joint_selection_v1(
            &admitted,
            &[(
                JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(
                    TargetId::new(1),
                    TargetCandidateId::new(2),
                )]),
                key,
            )],
        )
        .expect("single binding must materialise");

        let color = joint_selection_color(&materialised).expect("identity color must fit");

        assert_eq!(color.as_slice().len(), 1 + 1 + 32);
        assert_eq!(color.as_slice()[0], vertex_tag::JOINT_SELECTION);
        assert_eq!(
            color.as_slice()[1],
            release_tag::SELECTION_RELEASE_IDENTITY_V1,
        );
        assert_eq!(
            &color.as_slice()[2..],
            materialised.release_identity().as_bytes(),
        );
    }

    #[test]
    fn family_vertex_codec_binds_semantic_release_without_opaque_id_or_artifact_receipt() {
        assert_eq!(release_tag::FAMILY_CERTIFICATE_VERTEX_V1, 1);
        assert_eq!(release_tag::FAMILY_MEMBERSHIP_RELEASE_V1, 1);
        assert_eq!(release_tag::FAMILY_SEMANTIC_RELEASE_VERTEX_V2, 2);
        assert_eq!(release_tag::FAMILY_MEMBERSHIP_RELEASE_V2, 2);
        let expected = crate::family::SemanticFamilyReleaseIdV2::from_digest([0xA5; 32]);
        let family = FamilyDeclarationV2::new(FamilyId::new(u32::MAX), expected);

        let color = family_color(&family).unwrap();

        assert_eq!(color.as_slice()[0], vertex_tag::FAMILY);
        assert_eq!(
            color.as_slice()[1],
            release_tag::FAMILY_SEMANTIC_RELEASE_VERTEX_V2
        );
        assert_eq!(&color.as_slice()[2..], expected.as_bytes());
    }

    #[test]
    fn clean_set_final_recheck_mutant_release_changes_exactly_one_declared_bit() {
        let source = crate::clean_set::EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2;
        let mutant = clean_set_final_recheck_mutant_release_v1();

        assert_eq!(mutant[0], source[0] ^ 1);
        assert_eq!(&mutant[1..], &source[1..]);
    }

    #[test]
    fn declared_clean_set_constraint_color_binds_every_release_digest_byte() {
        let release = crate::clean_set::EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2;
        let baseline = declared_srgb8_clean_set_constraint_color_for_release(
            vertex_tag::CONSTRAINT_HARD,
            release,
        )
        .unwrap();

        for byte_index in 0..release.len() {
            let mut mutant = release;
            mutant[byte_index] ^= 1;
            assert_ne!(
                baseline,
                declared_srgb8_clean_set_constraint_color_for_release(
                    vertex_tag::CONSTRAINT_HARD,
                    mutant,
                )
                .unwrap(),
                "release digest byte {byte_index} escaped identity",
            );
        }
    }
    #[test]
    fn nominal_visible_identity_separates_owned_domain_and_binds_all_release_bytes()
    -> Result<(), String> {
        let release = crate::clean_set::EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2;
        let baseline = constraint_color(
            vertex_tag::CONSTRAINT_HARD,
            ProgramConstraintContentV1::NominalSrgb8Visible { release },
        )
        .map_err(|error| format!("visible identity: {error:?}"))?;
        assert_ne!(
            baseline,
            declared_srgb8_clean_set_constraint_color_for_release(
                vertex_tag::CONSTRAINT_HARD,
                release,
            )
            .map_err(|error| format!("owned identity: {error:?}"))?,
            "visible membership and owned-domain membership have different obligations",
        );
        for byte_index in 0..release.len() {
            let mut mutant = release;
            mutant[byte_index] ^= 1;
            assert_ne!(
                baseline,
                constraint_color(
                    vertex_tag::CONSTRAINT_HARD,
                    ProgramConstraintContentV1::NominalSrgb8Visible { release: mutant },
                )
                .map_err(|error| format!("mutated identity byte {byte_index}: {error:?}"))?,
                "nominal release digest byte {byte_index} escaped identity",
            );
        }
        Ok(())
    }

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
            applicability,
            uncertainty,
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
                applicability,
                uncertainty,
            },
            ProgramConstraintContentV1::ExactSrgb8 {
                identity,
                release: crate::constraints::ExactIdentityReleaseV1::MutationSentinelV1,
                capability,
                expected,
                applicability,
                uncertainty,
            },
            ProgramConstraintContentV1::ExactSrgb8 {
                identity,
                release,
                capability: crate::constraints::ExactIdentityCapabilityV1::MutationSentinelV1,
                expected,
                applicability,
                uncertainty,
            },
        ] {
            assert_ne!(
                constraint_color(vertex_tag::CONSTRAINT_HARD, mutant).unwrap(),
                baseline
            );
        }
    }

    #[test]
    fn modeled_lcs_release_mutation_affects_only_lcs_aware_constraint_content() {
        fn mutate_modeled_lcs_release(
            content: ProgramConstraintContentV1,
        ) -> ProgramConstraintContentV1 {
            match content {
                ProgramConstraintContentV1::ModeledLcsProbe {
                    release,
                    applicability,
                    uncertainty,
                } => ProgramConstraintContentV1::ModeledLcsProbe {
                    release: release.with_modeled_lcs_release_for_test(
                        crate::lcs_occurrence::ModeledLcsOccurrenceReleaseId::MutationSentinelV1,
                    ),
                    applicability,
                    uncertainty,
                },
                encoded_only => encoded_only,
            }
        }

        let exact = crate::constraints::ExactSrgb8IdentityV1
            .program_constraint_content_v1(Srgb8::new([0x12, 0x34, 0x56]));
        let wcag = crate::constraints::Wcag22Srgb8V1
            .program_constraint_content_v1(Wcag22CriterionV1::Sc143TextDefault);
        for encoded_only in [exact, wcag] {
            assert_eq!(
                constraint_color(vertex_tag::CONSTRAINT_HARD, encoded_only).unwrap(),
                constraint_color(
                    vertex_tag::CONSTRAINT_HARD,
                    mutate_modeled_lcs_release(encoded_only),
                )
                .unwrap(),
                "an encoded-only descriptor has no LCS release capability to mutate",
            );
        }

        let lcs = crate::constraints::LcsProbeProgramEvaluatorV1.program_constraint_content_v1();
        assert_ne!(
            constraint_color(vertex_tag::CONSTRAINT_HARD, lcs).unwrap(),
            constraint_color(vertex_tag::CONSTRAINT_HARD, mutate_modeled_lcs_release(lcs),)
                .unwrap(),
            "an LCS-aware descriptor must bind the modeled occurrence release",
        );
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
        ordering::sort(&mut colors);
        colors.dedup();
        assert_eq!(colors.len(), 4);
    }

    #[test]
    fn fixed_color_key_preserves_length_first_order_and_bounds() -> Result<(), ProgramCompileError>
    {
        let mut colors = Vec::new();
        for length in 0..COLOR_CAPACITY {
            for tag in [0, 1, 128, 255] {
                let mut color = VertexColorV1::new(tag);
                let payload = vec![tag ^ length as u8; length];
                color.push_bytes(&payload)?;
                assert_eq!(usize::from(color.len()), length + 1);
                assert_eq!(color.as_slice()[0], tag);
                assert_eq!(&color.as_slice()[1..], payload);
                colors.push(color);
            }
        }
        for left in &colors {
            for right in &colors {
                assert_eq!(
                    left.cmp(right),
                    (left.len(), left.as_slice()).cmp(&(right.len(), right.as_slice()))
                );
            }
        }
        let mut full = *colors
            .last()
            .ok_or(ProgramCompileError::InternalInvariant)?;
        let before = full;
        assert_eq!(full.push_u8(1), Err(ProgramCompileError::InternalInvariant));
        assert_eq!(
            full.push_u64(1),
            Err(ProgramCompileError::InternalInvariant)
        );
        assert_eq!(full, before);
        Ok(())
    }
}
