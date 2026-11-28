//! Вычисляемый selection существующего Program. Политика не знает смысл ID.
//! Области сокращаются до полного выбора; физика остаётся у Program.

use super::*;
use crate::appearance::SourceOverCertificateV1;
use crate::color_intent::{ColorIdentityDomainV1, ToneCurveV1};
use crate::program_wire::{ProgramSelectionComponentV1, ProgramSelectionModeV1};
use std::cmp::Ordering;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ComputedIdentityV1(pub(super) [u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssignmentDomainV1 {
    Cube { lower: [u8; 3], upper: [u8; 3] },
    Gray { lower: u8, upper: u8 },
}

impl AssignmentDomainV1 {
    fn bounds(self) -> ([u8; 3], [u8; 3]) {
        match self {
            Self::Cube { lower, upper } => (lower, upper),
            Self::Gray { lower, upper } => ([lower; 3], [upper; 3]),
        }
    }

    fn lower_score(self, anchor: [f64; 3]) -> f64 {
        let (lower, upper) = self.bounds();
        let (lower, upper) = crate::spaces::oklab::srgb8_box_oklab_bounds(lower, upper);
        let nearest = std::array::from_fn(|i| {
            if anchor[i] < lower[i] {
                lower[i]
            } else if anchor[i] > upper[i] {
                upper[i]
            } else {
                anchor[i]
            }
        });
        squared_distance(nearest, anchor)
    }

    fn split(self) -> (Self, Self) {
        match self {
            Self::Gray { lower, upper } => {
                let middle = lower + (upper - lower) / 2;
                (
                    Self::Gray {
                        lower,
                        upper: middle,
                    },
                    Self::Gray {
                        lower: middle + 1,
                        upper,
                    },
                )
            }
            Self::Cube { lower, upper } => {
                let mut axis = 0;
                for i in 1..3 {
                    if upper[i] - lower[i] > upper[axis] - lower[axis] {
                        axis = i;
                    }
                }
                let middle = lower[axis] + (upper[axis] - lower[axis]) / 2;
                let mut left_upper = upper;
                let mut right_lower = lower;
                left_upper[axis] = middle;
                right_lower[axis] = middle + 1;
                (
                    Self::Cube {
                        lower,
                        upper: left_upper,
                    },
                    Self::Cube {
                        lower: right_lower,
                        upper,
                    },
                )
            }
        }
    }

    // Целочисленное деление пополам требует не более ceil(log2 длины) шагов на ось.
    fn split_depth(self) -> usize {
        let axis_depth = |lower, upper| {
            (u16::from(upper) - u16::from(lower) + 1)
                .next_power_of_two()
                .trailing_zeros() as usize
        };
        match self {
            Self::Cube { lower, upper } => (0..3).map(|i| axis_depth(lower[i], upper[i])).sum(),
            Self::Gray { lower, upper } => axis_depth(lower, upper),
        }
    }

    fn cardinality(self) -> Option<u64> {
        match self {
            Self::Cube { lower, upper } => (0..3).try_fold(1_u64, |count, i| {
                count.checked_mul(u64::from(upper[i].checked_sub(lower[i])?) + 1)
            }),
            Self::Gray { lower, upper } => Some(u64::from(upper.checked_sub(lower)?) + 1),
        }
    }

    fn value(self, ordinal: u64) -> Srgb8 {
        match self {
            Self::Gray { lower, .. } => Srgb8::new([lower + ordinal as u8; 3]),
            Self::Cube { lower, upper } => {
                let blue = u64::from(upper[2]) - u64::from(lower[2]) + 1;
                let green = u64::from(upper[1]) - u64::from(lower[1]) + 1;
                Srgb8::new([
                    lower[0] + (ordinal / (blue * green)) as u8,
                    lower[1] + ((ordinal / blue) % green) as u8,
                    lower[2] + (ordinal % blue) as u8,
                ])
            }
        }
    }

    fn contains(self, rgb: Srgb8) -> bool {
        let [r, g, b] = rgb.bytes();
        match self {
            Self::Gray { lower, upper } => r == g && g == b && lower <= r && r <= upper,
            Self::Cube { lower, upper } => rgb
                .bytes()
                .iter()
                .enumerate()
                .all(|(i, value)| lower[i] <= *value && *value <= upper[i]),
        }
    }

    fn gray_intersection(self) -> Option<Self> {
        let (lower, upper) = self.bounds();
        let lower = lower[0].max(lower[1]).max(lower[2]);
        let upper = upper[0].min(upper[1]).min(upper[2]);
        (lower <= upper).then_some(Self::Gray { lower, upper })
    }

    pub(super) fn identity_bytes(self) -> [u8; 7] {
        match self {
            Self::Cube { lower, upper } => [
                0, lower[0], lower[1], lower[2], upper[0], upper[1], upper[2],
            ],
            Self::Gray { lower, upper } => [1, lower, lower, lower, upper, upper, upper],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComputedTargetV1 {
    domain: AssignmentDomainV1,
    anchor: Srgb8,
    max_delta_bits: u64,
    preference: u64,
    intent: Option<ColorUseIntentV1>,
    reference_appearance: bool,
}

/// Геометрия авторского характера без знания имён ролей. Explicit appearance
/// проверяет visible; reference appearance сохраняет identity исходного материала.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ColorUseIntentV1 {
    pub(crate) identity: ColorIdentityDomainV1,
    pub(crate) appearance: Option<OccurrenceId>,
    pub(crate) tone: Option<ToneCurveV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComputedAdmissionErrorV1 {
    InvalidDomain,
    InvalidTolerance,
    MixedSelection,
    InvalidPreferenceOrder,
    InvalidColorIntent,
    MissingColorAppearance,
    MissingToneInput,
    InvalidReferenceComposition,
    InvalidTonalHierarchy,
}

// Предел одной связанной задачи синхронного профиля; не клиентская политика.
pub(crate) const EXECUTION_NODE_LIMIT_V1: u64 = 65_536;

pub(super) fn execution_profile_identity(budget: u64) -> [u8; 32] {
    const DOMAIN: &[u8] =
        b"labcolors.assignment-execution.v5.nominal-visible-boxes.independent-components.anytime.direct-oklab-f64.lex-rgb.region-dfs.leaf64.region-plus-point-budget.full-recheck\0";
    execution_identity_with_budget(DOMAIN, budget)
}

fn execution_identity_with_budget(domain: &[u8], budget: u64) -> [u8; 32] {
    let mut hash = crate::sha256::Hasher::new();
    hash.update(domain);
    hash.update(&budget.to_le_bytes());
    hash.update(contrast_bounds::IDENTITY);
    hash.update(crate::spaces::oklab::SRGB8_OKLAB_PROJECTION_IDENTITY);
    *hash.finalize().as_bytes()
}

impl ComputedTargetV1 {
    fn try_clone_v1(&self) -> Result<Self, ProgramCompileError> {
        let intent = self
            .intent
            .as_ref()
            .map(|intent| -> Result<_, ProgramCompileError> {
                Ok(ColorUseIntentV1 {
                    identity: intent.identity,
                    appearance: intent.appearance,
                    tone: intent
                        .tone
                        .as_ref()
                        .map(ToneCurveV1::try_clone_v1)
                        .transpose()
                        .map_err(|_| ProgramCompileError::ResourceExhausted)?,
                })
            })
            .transpose()?;
        Ok(Self {
            domain: self.domain,
            anchor: self.anchor,
            max_delta_bits: self.max_delta_bits,
            preference: self.preference,
            intent,
            reference_appearance: self.reference_appearance,
        })
    }

    pub(crate) fn declared(domain: AssignmentDomainV1, anchor: Srgb8, max_delta: f64) -> Self {
        Self {
            domain,
            anchor,
            max_delta_bits: if max_delta == 0.0 {
                0
            } else {
                max_delta.to_bits()
            },
            preference: u64::MAX,
            intent: None,
            reference_appearance: false,
        }
    }

    pub(crate) fn color(anchor: Srgb8, max_delta: f64, intent: ColorUseIntentV1) -> Self {
        let domain = if intent.appearance.is_none() && intent.identity.neutral() {
            AssignmentDomainV1::Gray {
                lower: 0,
                upper: 255,
            }
        } else {
            AssignmentDomainV1::Cube {
                lower: [0; 3],
                upper: [255; 3],
            }
        };
        let mut value = Self::declared(domain, anchor, max_delta);
        value.intent = Some(intent);
        value
    }

    pub(crate) fn set_reference_appearance(&mut self) {
        self.reference_appearance = true;
    }

    pub(crate) fn color_intent(&self) -> Option<&ColorUseIntentV1> {
        self.intent.as_ref()
    }

    /// Оpaque IDs связывает canonical graph; здесь только собственные данные.
    pub(crate) fn color_identity_hash(&self) -> Result<[u8; 32], ProgramCompileError> {
        self.color_identity_hash_for(identity::CanonicalPreferenceV1::Ordered(self.preference))
    }

    pub(crate) fn color_identity_hash_for(
        &self,
        preference: identity::CanonicalPreferenceV1,
    ) -> Result<[u8; 32], ProgramCompileError> {
        let intent = self
            .intent
            .as_ref()
            .ok_or(ProgramCompileError::InternalInvariant)?;
        let stops = intent.tone.as_ref().map_or(0, |tone| tone.stops().len());
        let size = 27_usize
            .checked_add(46 + 1 + 4)
            .and_then(|n| stops.checked_mul(16).and_then(|s| n.checked_add(s)))
            .ok_or(ProgramCompileError::ResourceExhausted)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        self.append_identity_bytes_for(&mut bytes, preference);
        bytes.extend_from_slice(&intent.identity.identity_bytes());
        bytes.push(if self.reference_appearance {
            2
        } else {
            u8::from(intent.appearance.is_some())
        });
        if self.reference_appearance {
            bytes.push(u8::from(intent.identity.force_neutral()));
        }
        bytes.extend_from_slice(
            &u32::try_from(stops)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?
                .to_le_bytes(),
        );
        if let Some(tone) = &intent.tone {
            for stop in tone.stops() {
                for component in stop {
                    bytes.extend_from_slice(&component.to_bits().to_le_bytes());
                }
            }
        }
        Ok(*crate::sha256::digest(&bytes).as_bytes())
    }

    // Фабрика недоступна старым wire/SDK и private-fixture ABI.
    #[cfg(test)]
    fn new(
        domain: AssignmentDomainV1,
        anchor: Srgb8,
        max_delta: f64,
        preference: u64,
    ) -> Result<Self, ComputedAdmissionErrorV1> {
        domain
            .cardinality()
            .ok_or(ComputedAdmissionErrorV1::InvalidDomain)?;
        if !max_delta.is_finite() || max_delta < 0.0 || !(max_delta * max_delta).is_finite() {
            return Err(ComputedAdmissionErrorV1::InvalidTolerance);
        }
        Ok(Self {
            domain,
            anchor,
            max_delta_bits: if max_delta == 0.0 {
                0
            } else {
                max_delta.to_bits()
            },
            preference,
            intent: None,
            reference_appearance: false,
        })
    }

    pub(super) const fn anchor(&self) -> Srgb8 {
        self.anchor
    }

    pub(super) fn identity_bytes(&self) -> [u8; 26] {
        let mut bytes = [0; 26];
        bytes[..7].copy_from_slice(&self.domain.identity_bytes());
        bytes[7..10].copy_from_slice(&self.anchor.bytes());
        bytes[10..18].copy_from_slice(&self.max_delta_bits.to_le_bytes());
        bytes[18..].copy_from_slice(&self.preference.to_le_bytes());
        bytes
    }

    pub(crate) fn identity_bytes_for(
        &self,
        preference: identity::CanonicalPreferenceV1,
    ) -> Result<Vec<u8>, ProgramCompileError> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(27)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        self.append_identity_bytes_for(&mut bytes, preference);
        Ok(bytes)
    }

    fn append_identity_bytes_for(
        &self,
        bytes: &mut Vec<u8>,
        preference: identity::CanonicalPreferenceV1,
    ) {
        let mut value = self.identity_bytes();
        match preference {
            identity::CanonicalPreferenceV1::Ordered(rank) => {
                value[18..].copy_from_slice(&rank.to_le_bytes());
                bytes.extend_from_slice(&value);
            }
            identity::CanonicalPreferenceV1::JointPoint => {
                bytes.extend_from_slice(&value[..18]);
                bytes.push(1);
            }
            identity::CanonicalPreferenceV1::JointReferencePeers => {
                bytes.extend_from_slice(&value[..18]);
                bytes.push(3);
            }
            identity::CanonicalPreferenceV1::LegacyReferenceOrdered(rank) => {
                value[18..].copy_from_slice(&rank.to_le_bytes());
                bytes.extend_from_slice(&value);
                bytes.push(2);
            }
        }
    }
}

struct CompiledComputedTargetV1 {
    id: TargetId,
    binding: CompiledPaintInputSlotV1,
    declaration: ComputedTargetV1,
    // Source recourse stays anchored before compositing. Reference compilation
    // updates only the effective appearance used by visible-space conditions.
    source_anchor_lab: [f64; 3],
    anchor_lab: [f64; 3],
    effective_anchor: Srgb8,
    reference_backdrop: Option<Srgb8>,
    independent_backdrop: bool,
    appearance: Option<CompiledOccurrenceSlotV1>,
    appearance_inputs: Box<[usize]>,
    tone_input: Option<usize>,
    opaque: bool,
}

/// The declared preference is separate from source identity and maxChange.
/// Each numeric interpreter resolves the same origin in its own model.
enum CompiledPreferredIntentV1<'a> {
    Point {
        anchor: PreferredPointAnchorV1,
        tone: Option<(&'a ToneCurveV1, usize)>,
    },
    ImplicitReference,
}

#[derive(Clone, Copy)]
enum PreferredPointAnchorV1 {
    AuthoredSource,
    ReferenceVisible,
}

struct EffectiveColorIntentV1<'a> {
    identity: ColorIdentityDomainV1,
    appearance: Option<OccurrenceId>,
    tone: Option<&'a ToneCurveV1>,
}

impl CompiledComputedTargetV1 {
    fn preferred_intent(&self) -> CompiledPreferredIntentV1<'_> {
        let tone = self
            .declaration
            .intent
            .as_ref()
            .and_then(|intent| intent.tone.as_ref().zip(self.tone_input));
        if self.declaration.reference_appearance && tone.is_none() {
            CompiledPreferredIntentV1::ImplicitReference
        } else {
            CompiledPreferredIntentV1::Point {
                anchor: if self.declaration.reference_appearance {
                    PreferredPointAnchorV1::ReferenceVisible
                } else {
                    PreferredPointAnchorV1::AuthoredSource
                },
                tone,
            }
        }
    }

    fn effective_intent(&self) -> Option<EffectiveColorIntentV1<'_>> {
        let authored = self.declaration.intent.as_ref()?;
        Some(EffectiveColorIntentV1 {
            identity: authored.identity,
            appearance: authored.appearance,
            tone: authored.tone.as_ref(),
        })
    }
    fn effective_anchor(&self) -> Srgb8 {
        self.effective_anchor
    }

    // Reference composition describes the use's appearance. It never changes
    // the material's authored source identity, even when compositing cancels
    // its hue. The older explicit-appearance mode keeps its visible contract.
    fn identity_point(&self, source: Srgb8, visible: Srgb8) -> Srgb8 {
        if self.declaration.reference_appearance {
            source
        } else {
            visible
        }
    }

    fn search_domain(&self) -> AssignmentDomainV1 {
        if (self.opaque || self.declaration.reference_appearance)
            && self
                .effective_intent()
                .is_some_and(|intent| intent.identity.neutral())
        {
            // Source identity remains neutral through a transparent use.
            // Explicit visible identity narrows the source only if opaque.
            self.declaration
                .domain
                .gray_intersection()
                .unwrap_or(self.declaration.domain)
        } else {
            self.declaration.domain
        }
    }
}

impl ComputedSelectionV1 {
    /// Constraint factors retain compiled constraint order in both phases.
    /// Objectives occupy the tail of the hard-factor array.
    pub(super) fn constraint_inputs(&self, constraint: usize) -> Option<&[usize]> {
        let count = self.factors.len().checked_sub(self.targets.len())?;
        for factors in [&self.factors[..count], self.report_only_factors.as_ref()] {
            if let Ok(index) =
                factors.binary_search_by_key(&Some(constraint), |factor| factor.constraint())
            {
                return Some(&factors[index].inputs);
            }
        }
        None
    }

    pub(super) fn identity_preferences(
        &self,
        partition: &components::ComponentPartitionV1,
    ) -> Result<Vec<identity::ComputedTargetIdentityPreferenceV1>, ProgramCompileError> {
        let mut result = Vec::new();
        result
            .try_reserve_exact(self.targets.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        for component in &partition.groups {
            for (rank, &index) in component.targets.iter().enumerate() {
                let target = &self.targets[index];
                let preference = match component.mode {
                    ProgramSelectionModeV1::OrderedV1 => {
                        identity::CanonicalPreferenceV1::Ordered(target.declaration.preference)
                    }
                    ProgramSelectionModeV1::JointPointV1 => {
                        identity::CanonicalPreferenceV1::JointPoint
                    }
                    ProgramSelectionModeV1::JointReferencePeersV1 => {
                        identity::CanonicalPreferenceV1::JointReferencePeers
                    }
                    ProgramSelectionModeV1::OrderedReferenceV1 => {
                        identity::CanonicalPreferenceV1::LegacyReferenceOrdered(rank as u64)
                    }
                };
                result.push(identity::ComputedTargetIdentityPreferenceV1 {
                    target: target.id,
                    preference,
                });
            }
        }
        result.sort_unstable_by_key(|value| value.target);
        Ok(result)
    }

    /// Apply the canonical schedule to the existing partition. Dependencies are
    /// physical facts and do not change when target ordinals are permuted.
    pub(super) fn canonicalize_execution(
        &mut self,
        layout: &identity::CanonicalExecutionLayoutV1,
        partition: &mut components::ComponentPartitionV1,
    ) -> Result<(), ProgramCompileError> {
        let allocation = |_| ProgramCompileError::ResourceExhausted;
        let mut ranks = Vec::new();
        ranks
            .try_reserve_exact(layout.targets.len())
            .map_err(allocation)?;
        ranks.extend(
            layout
                .targets
                .iter()
                .enumerate()
                .map(|(rank, &id)| (id, rank)),
        );
        ranks.sort_unstable_by_key(|entry| entry.0);
        let mut target_ranks = Vec::new();
        target_ranks
            .try_reserve_exact(self.targets.len())
            .map_err(allocation)?;
        for target in &self.targets {
            let position = ranks
                .binary_search_by_key(&target.id, |entry| entry.0)
                .map_err(|_| ProgramCompileError::InternalInvariant)?;
            target_ranks.push(ranks[position].1);
        }
        let mut old_to_new =
            buffer(self.targets.len(), 0).map_err(|_| ProgramCompileError::ResourceExhausted)?;
        for group in &mut partition.groups {
            if group.mode.symmetric() {
                group
                    .targets
                    .sort_unstable_by_key(|&target| target_ranks[target]);
            }
            let &first = group
                .targets
                .first()
                .ok_or(ProgramCompileError::InternalInvariant)?;
            old_to_new[first] = group
                .targets
                .iter()
                .map(|&target| target_ranks[target])
                .min()
                .ok_or(ProgramCompileError::InternalInvariant)?;
        }
        partition
            .groups
            .sort_unstable_by_key(|group| old_to_new[group.targets[0]]);
        for (next, index) in partition
            .groups
            .iter_mut()
            .flat_map(|group| &mut group.targets)
            .enumerate()
        {
            old_to_new[*index] = next;
            *index = next;
        }
        for factor in partition
            .factors
            .iter_mut()
            .chain(&mut partition.report_only_factors)
        {
            for target in &mut factor.targets {
                *target = old_to_new[*target];
            }
        }
        // An objective travels with its target. Each swap places one target,
        // its physical objective, and its binding at their final ordinal.
        let first = partition
            .factors
            .len()
            .checked_sub(self.targets.len())
            .ok_or(ProgramCompileError::InternalInvariant)?;
        let objectives = &mut partition.factors[first..];
        for index in 0..self.targets.len() {
            while old_to_new[index] != index {
                let next = old_to_new[index];
                self.targets.swap(index, next);
                objectives.swap(index, next);
                old_to_new.swap(index, next);
            }
            objectives[index].kind = ComputedFactorKindV1::Objective(index);
        }
        Ok(())
    }

    fn objective_factor(&self, target: usize) -> Option<usize> {
        let index = self
            .factors
            .len()
            .checked_sub(self.targets.len())?
            .checked_add(target)?;
        (self.factors.get(index)?.kind == ComputedFactorKindV1::Objective(target)).then_some(index)
    }

    pub(super) fn compile_peer_schedules<T>(
        &mut self,
        topology: &identity::ProgramIdentityGraphV1,
        graph: &CompiledAppearanceGraph,
        constraints: &[CompiledPointConstraint<T>],
    ) -> Result<(), ProgramCompileError> {
        let mut ports = Vec::new();
        ports
            .try_reserve_exact(graph.surface_input_ports().len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        ports.extend(graph.surface_input_ports());
        for component in &mut self.components {
            if component.mode != ProgramSelectionModeV1::JointReferencePeersV1
                || component.inputs.len() < 2
            {
                continue;
            }
            component.peer_graph = topology.peer_component(
                component.targets.iter().map(|&i| (i, self.targets[i].id)),
                component.inputs.iter().map(|&i| (i, ports[i])),
                component
                    .constraints
                    .iter()
                    .map(|&i| (i, constraints[i].id)),
            )?;
        }
        Ok(())
    }

    pub(super) fn canonicalize_inputs(
        &mut self,
        layout: &identity::CanonicalExecutionLayoutV1,
        graph: &CompiledAppearanceGraph,
    ) -> Result<(), ProgramCompileError> {
        let mut ranks = Vec::new();
        ranks
            .try_reserve_exact(layout.inputs.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        ranks.extend(
            layout
                .inputs
                .iter()
                .enumerate()
                .map(|(rank, &id)| (id, rank)),
        );
        ranks.sort_unstable_by_key(|entry| entry.0);
        let mut schema_ranks = Vec::new();
        schema_ranks
            .try_reserve_exact(ranks.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        for id in graph.surface_input_ports() {
            let index = ranks
                .binary_search_by_key(&id, |entry| entry.0)
                .map_err(|_| ProgramCompileError::InternalInvariant)?;
            schema_ranks.push(ranks[index].1);
        }
        for component in &mut self.components {
            if component.mode.symmetric() {
                component
                    .inputs
                    .sort_unstable_by_key(|&input| schema_ranks[input]);
            }
        }
        Ok(())
    }

    pub(super) fn compile_reference(
        &mut self,
        reference: Option<&reference::ReferenceCompositionV1>,
    ) -> Result<(), ProgramCompileError> {
        for target in &mut self.targets {
            if !target.declaration.reference_appearance {
                continue;
            }
            let invalid = || {
                ProgramCompileError::InvalidComputedAssignment(
                    ComputedAdmissionErrorV1::InvalidReferenceComposition,
                )
            };
            let occurrence = target
                .declaration
                .intent
                .as_ref()
                .and_then(|intent| intent.appearance)
                .ok_or_else(invalid)?;
            let (visible, backdrop) = reference
                .and_then(|value| value.occurrence(occurrence))
                .ok_or_else(invalid)?;
            target.effective_anchor = visible;
            target.anchor_lab = lab(visible);
            target.reference_backdrop = Some(backdrop);
        }
        Ok(())
    }
}

pub(super) struct ComputedSelectionV1 {
    targets: Box<[CompiledComputedTargetV1]>,
    components: Box<[ComputedComponentV1]>,
    factors: Box<[ComputedFactorV1]>,
    report_only_factors: Box<[ComputedFactorV1]>,
    fixed_constraints: Box<[usize]>,
    budget: u64,
    total_budget: u64,
    execution_profile: [u8; 32],
    selection_components: Arc<[ProgramSelectionComponentV1]>,
    point_anchors: Option<Arc<contextual::reference_completion::CompiledPointAnchorsV1>>,
}

impl ComputedSelectionV1 {
    /// Each independently selected component, and each hard factor including
    /// fixed-only constraints, must read at most one varying physical input.
    pub(super) fn accepts_independent_fields(&self, fields: &[u32]) -> bool {
        let include = |first: &mut Option<usize>, inputs: &[usize]| {
            for &input in inputs {
                if fields.binary_search(&(input as u32)).is_ok() {
                    match *first {
                        None => *first = Some(input),
                        Some(previous) if previous == input => {}
                        Some(_) => return false,
                    }
                }
            }
            true
        };
        for component in &self.components {
            if !include(&mut None, &component.inputs) {
                return false;
            }
        }
        for factor in &self.factors {
            if !include(&mut None, &factor.inputs) {
                return false;
            }
        }
        for factor in &self.report_only_factors {
            if !include(&mut None, &factor.inputs) {
                return false;
            }
        }
        true
    }
}

/// Ascending indices into the compiled target schedule: authored preference
/// for ordered selection, canonical execution order for Point composition.
struct ComputedComponentV1 {
    targets: Box<[usize]>,
    constraints: Box<[usize]>,
    inputs: Box<[usize]>,
    physical: CompiledAppearanceConeV1,
    mode: ProgramSelectionModeV1,
    point_equivalence: Option<PointEquivalenceV1>,
    peer_graph: Option<identity::PeerSearchGraph>,
}

/// A substitution proof, not graph symmetry: every grouped source has the
/// same local feasible set and Point preference for every value of the rest.
struct PointEquivalenceV1 {
    representatives: Box<[usize]>,
    variables: Box<[usize]>,
    constraints: Box<[usize]>,
}

impl ComputedComponentV1 {
    fn point_representative(&self, local: usize) -> usize {
        self.point_equivalence
            .as_ref()
            .map_or(local, |equivalence| equivalence.representatives[local])
    }

    fn point_variable_count(&self) -> usize {
        self.point_equivalence
            .as_ref()
            .map_or(self.targets.len(), |equivalence| {
                equivalence.variables.len()
            })
    }

    fn point_variable(&self, ordinal: usize) -> usize {
        self.point_equivalence
            .as_ref()
            .map_or(ordinal, |equivalence| equivalence.variables[ordinal])
    }

    fn point_constraints(&self) -> &[usize] {
        self.point_equivalence
            .as_ref()
            .map_or(&self.constraints, |equivalence| &equivalence.constraints)
    }
}

/// The same compiled cone supports both initial and conditional independence.
struct ComputedFactorV1 {
    targets: Box<[usize]>,
    inputs: Box<[usize]>,
    kind: ComputedFactorKindV1,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ComputedFactorKindV1 {
    Constraint(usize),
    Objective(usize),
}

impl ComputedFactorV1 {
    fn constraint(&self) -> Option<usize> {
        match self.kind {
            ComputedFactorKindV1::Constraint(index) => Some(index),
            ComputedFactorKindV1::Objective(_) => None,
        }
    }
}

/// Borrowed projections keep one owner of target declarations and bindings.
#[derive(Clone, Copy)]
struct ComputedTargetsV1<'a> {
    all: &'a [CompiledComputedTargetV1],
    indices: Option<&'a [usize]>,
}

impl<'a> ComputedTargetsV1<'a> {
    fn new(selection: &'a ComputedSelectionV1, component: Option<&'a ComputedComponentV1>) -> Self {
        Self {
            all: &selection.targets,
            indices: component.map(|component| component.targets.as_ref()),
        }
    }

    fn len(self) -> usize {
        self.indices.map_or(self.all.len(), <[usize]>::len)
    }

    fn iter(
        self,
    ) -> impl ExactSizeIterator<Item = &'a CompiledComputedTargetV1> + DoubleEndedIterator {
        (0..self.len())
            .map(move |index| &self.all[self.indices.map_or(index, |indices| indices[index])])
    }
}

impl core::ops::Index<usize> for ComputedTargetsV1<'_> {
    type Output = CompiledComputedTargetV1;

    fn index(&self, index: usize) -> &Self::Output {
        &self.all[self.indices.map_or(index, |indices| indices[index])]
    }
}

pub(super) fn assign_preference(
    targets: &mut [Target],
    order: Option<&[TargetId]>,
) -> Result<(), ProgramCompileError> {
    let invalid = |error| ProgramCompileError::InvalidComputedAssignment(error);
    let computed = targets
        .iter()
        .filter(|target| matches!(target.intent, TargetIntentV1::Computed(_)))
        .count();
    let Some(order) = order else {
        return if computed == 0 {
            Ok(())
        } else {
            Err(invalid(ComputedAdmissionErrorV1::InvalidPreferenceOrder))
        };
    };
    if targets
        .iter()
        .any(|target| matches!(target.intent, TargetIntentV1::Finite(_)))
    {
        return Err(invalid(ComputedAdmissionErrorV1::MixedSelection));
    }
    if order.len() != computed {
        return Err(invalid(ComputedAdmissionErrorV1::InvalidPreferenceOrder));
    }
    for (ordinal, id) in order.iter().enumerate() {
        if order[..ordinal].contains(id) {
            return Err(invalid(ComputedAdmissionErrorV1::InvalidPreferenceOrder));
        }
        let Some(Target {
            intent: TargetIntentV1::Computed(value),
            ..
        }) = targets.iter_mut().find(|target| target.id == *id)
        else {
            return Err(invalid(ComputedAdmissionErrorV1::InvalidPreferenceOrder));
        };
        value.preference = ordinal as u64;
    }
    Ok(())
}

pub(super) fn compile_selection(
    graph: &CompiledAppearanceGraph,
    targets: &[Target],
    finite: Option<&MaterialisedSelectionV1>,
    budget: u64,
) -> Result<ComputedSelectionV1, ProgramCompileError> {
    let invalid = ProgramCompileError::InvalidComputedAssignment;
    if finite.is_some()
        || targets
            .iter()
            .any(|target| matches!(target.intent, TargetIntentV1::Finite(_)))
    {
        return Err(invalid(ComputedAdmissionErrorV1::MixedSelection));
    }
    let mut compiled = Vec::new();
    compiled
        .try_reserve_exact(targets.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for target in targets {
        let TargetIntentV1::Computed(declaration) = &target.intent else {
            continue;
        };
        declaration
            .domain
            .cardinality()
            .ok_or(invalid(ComputedAdmissionErrorV1::InvalidDomain))?;
        let delta = f64::from_bits(declaration.max_delta_bits);
        if !delta.is_finite() || delta < 0.0 || !(delta * delta).is_finite() {
            return Err(invalid(ComputedAdmissionErrorV1::InvalidTolerance));
        }
        if declaration
            .intent
            .as_ref()
            .is_some_and(|intent| intent.identity.source() != declaration.anchor)
        {
            return Err(invalid(ComputedAdmissionErrorV1::InvalidColorIntent));
        }
        let source_anchor_lab = lab(declaration.anchor);
        compiled.push(CompiledComputedTargetV1 {
            id: target.id,
            binding: graph
                .bind_paint_input(target_paint_input_id(target.id))
                .ok_or(ProgramCompileError::InternalInvariant)?,
            declaration: declaration.try_clone_v1()?,
            effective_anchor: declaration.anchor,
            reference_backdrop: None,
            independent_backdrop: false,
            appearance_inputs: Box::new([]),
            source_anchor_lab,
            anchor_lab: source_anchor_lab,
            appearance: declaration
                .intent
                .as_ref()
                .and_then(|v| v.appearance)
                .map(|id| {
                    graph
                        .bind_occurrence(id)
                        .ok_or(invalid(ComputedAdmissionErrorV1::MissingColorAppearance))
                })
                .transpose()?,
            tone_input: declaration
                .intent
                .as_ref()
                .and_then(|v| v.tone.as_ref())
                .map(|tone| {
                    graph
                        .surface_input_index(SurfaceInputPortId::new(tone.input()))
                        .ok_or(invalid(ComputedAdmissionErrorV1::MissingToneInput))
                })
                .transpose()?,
            opaque: true,
        });
        if let Some(appearance) = declaration.intent.as_ref().and_then(|v| v.appearance) {
            if graph.occurrence_source_input(appearance) != Some(target_paint_input_id(target.id)) {
                return Err(invalid(ComputedAdmissionErrorV1::InvalidColorIntent));
            }
        }
    }
    for index in 0..compiled.len() {
        loop {
            let preference = compiled[index].declaration.preference;
            if preference == index as u64 {
                break;
            }
            // Сначала проверяем u64: на wasm32 преобразование индекса усечёт старшие биты.
            if preference >= compiled.len() as u64
                || compiled[preference as usize].declaration.preference == preference
            {
                return Err(invalid(ComputedAdmissionErrorV1::InvalidPreferenceOrder));
            }
            // Каждый обмен окончательно ставит один ранг вместе со всей записью на его место.
            compiled.swap(index, preference as usize);
        }
    }
    let contextual = compiled
        .iter()
        .any(|target| target.declaration.intent.is_some());
    let reference = compiled
        .iter()
        .any(|target| target.declaration.reference_appearance);
    Ok(ComputedSelectionV1 {
        targets: compiled.into_boxed_slice(),
        components: Box::new([]),
        factors: Box::new([]),
        report_only_factors: Box::new([]),
        fixed_constraints: Box::new([]),
        budget,
        total_budget: budget,
        execution_profile: if reference {
            contextual::reference_execution_profile_identity(budget)
        } else if contextual {
            contextual::execution_profile_identity(budget)
        } else {
            execution_profile_identity(budget)
        },
        selection_components: Arc::from([]),
        point_anchors: None,
    })
}

fn lab(rgb: Srgb8) -> [f64; 3] {
    crate::spaces::oklab::srgb8_to_oklab(rgb)
}

fn squared_distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let [l, a, b] = std::array::from_fn::<_, 3, _>(|i| a[i] - b[i]);
    (l * l + a * a) + b * b
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionV1 {
    AuthoredExact,
    Complete,
    BudgetExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SearchEvidenceV1 {
    completion: CompletionV1,
    // Domain traversal can be complete even when feasible objective
    // profiles could not be ordered. This is separate from open regions.
    numerical_order_unresolved: bool,
    visited: u64,
    regions: u64,
    points: u64,
    endpoint_evaluations: u64,
    examined: u64,
    // Учёт незавершённых областей обхода не зависит от мощности произведения.
    // Каждая новая область расходует посещение: open_regions <= regions <= visited <= budget.
    open_regions: u64,
}

impl SearchEvidenceV1 {
    fn close_region(&mut self) -> Result<(), ()> {
        self.open_regions = self.open_regions.checked_sub(1).ok_or(())?;
        Ok(())
    }
}

struct SearchResultV1 {
    evidence: SearchEvidenceV1,
    values: Option<Vec<Srgb8>>,
    reference_selections:
        Option<std::sync::Arc<[crate::program_wire::ProgramReferenceSelectionV1]>>,
}

#[derive(Clone, Copy)]
struct RegionNodeV1 {
    depth: usize,
    domain: AssignmentDomainV1,
    lower_score: f64,
    ordinal: Option<u64>,
}

fn region_node(
    depth: usize,
    domain: AssignmentDomainV1,
    budget: u64,
    preference: [f64; 3],
    evidence: &mut SearchEvidenceV1,
) -> Option<RegionNodeV1> {
    if evidence.visited >= budget {
        return None;
    }
    evidence.visited += 1;
    evidence.regions += 1;
    evidence.open_regions += 1;
    Some(RegionNodeV1 {
        depth,
        domain,
        lower_score: domain.lower_score(preference),
        ordinal: None,
    })
}

#[derive(Clone, Copy)]
enum RegionConstraintV1 {
    TonalHierarchy {
        slot: CompiledOccurrenceSlotV1,
        weaker: CompiledOccurrenceSlotV1,
        required: f64,
        weaker_contrast: Option<Wcag22CriterionV1>,
    },
    Wcag {
        slot: CompiledOccurrenceSlotV1,
        criterion: Wcag22CriterionV1,
    },
    Exact {
        slot: CompiledOccurrenceSlotV1,
        expected: Srgb8,
    },
    NominalSrgb8Visible {
        slot: CompiledOccurrenceSlotV1,
    },
    Separation {
        slot: CompiledOccurrenceSlotV1,
        other: Option<CompiledOccurrenceSlotV1>,
        bounds: crate::constraints::VisibleSeparationBoundsV1,
    },
}

#[derive(Clone, Copy, Default)]
struct RegionPointV1 {
    visible: [u8; 3],
    backdrop: [u8; 3],
}

#[derive(Clone, Copy)]
struct RegionConstraintPointV1 {
    subject: SourceOverCertificateV1,
    comparison: [u8; 3],
    weaker: [u8; 3],
}

impl RegionConstraintV1 {
    fn point(
        self,
        evaluation: &AppearanceEvaluationView<'_, '_>,
    ) -> Option<RegionConstraintPointV1> {
        let slot = match self {
            Self::Wcag { slot, .. }
            | Self::Exact { slot, .. }
            | Self::NominalSrgb8Visible { slot }
            | Self::Separation { slot, .. }
            | Self::TonalHierarchy { slot, .. } => slot,
        };
        let point = evaluation.occurrence_at(slot)?;
        let weaker = match self {
            Self::TonalHierarchy { weaker, .. } => evaluation.occurrence_at(weaker)?.visible(),
            _ => [0; 3],
        };
        Some(RegionConstraintPointV1 {
            subject: *point.certificate(),
            comparison: match self {
                Self::Separation {
                    other: Some(peer), ..
                } => evaluation.occurrence_at(peer)?.visible(),
                _ => point.certificate().backdrop_rgb(),
            },
            weaker,
        })
    }
    fn may_pass(
        self,
        lower: RegionConstraintPointV1,
        upper: RegionConstraintPointV1,
    ) -> Result<bool, ()> {
        let visible = (lower.subject.output_rgb(), upper.subject.output_rgb());
        let backdrop = (lower.comparison, upper.comparison);
        match self {
            Self::Wcag { criterion, .. } => {
                contrast_bounds::source_over_may_pass(lower.subject, upper.subject, criterion)
            }
            Self::Exact { expected, .. } => Ok(expected
                .bytes()
                .iter()
                .enumerate()
                .all(|(i, value)| visible.0[i] <= *value && *value <= visible.1[i])),
            Self::NominalSrgb8Visible { .. } => {
                Ok(crate::clean_set::ExactNominalSrgb8CleanSetV2
                    .intersects_box(visible.0, visible.1))
            }
            Self::Separation { bounds, .. } => {
                let [minimum, maximum] =
                    crate::spaces::oklab::srgb8_boxes_distance_bounds(visible, backdrop);
                Ok(minimum <= bounds.maximum() && maximum >= bounds.minimum())
            }
            Self::TonalHierarchy {
                required,
                weaker_contrast,
                ..
            } => {
                let luminance = |lower, upper| {
                    [lower, upper].map(|rgb| {
                        crate::color_intent::reference_transport::luminance(Srgb8::new(rgb))
                    })
                };
                let stronger = luminance(visible.0, visible.1);
                let weaker = luminance(lower.weaker, upper.weaker);
                let background = luminance(backdrop.0, backdrop.1);
                if let Some(criterion) = weaker_contrast {
                    let limits =
                        crate::color_intent::reference_transport::wcag_weak_luminance_limits(
                            [Srgb8::new(backdrop.0), Srgb8::new(backdrop.1)],
                            criterion,
                        );
                    Ok(CompiledTonalHierarchyV1::pair_possible_with_weak_limits(
                        required, stronger, weaker, background, limits,
                    ))
                } else {
                    Ok(CompiledTonalHierarchyV1::pair_possible_ratio(
                        required, stronger, weaker, background,
                    ))
                }
            }
        }
    }
}

fn region_constraints<Evaluation: ProgramConstraintEvaluatorSetV1>(
    epoch: &ProgramEpochV1<Evaluation>,
    constraint_indices: Option<&[usize]>,
) -> Result<Vec<RegionConstraintV1>, ()> {
    let mut result = Vec::new();
    if !epoch.graph.supports_rgb_box_endpoints() {
        return Ok(result);
    }
    let count = constraint_indices.map_or(epoch.constraints.len(), <[usize]>::len);
    let capacity = (0..count).try_fold(0_usize, |total, ordinal| {
        let index = constraint_indices.map_or(ordinal, |indices| indices[ordinal]);
        let constraint = &epoch.constraints[index];
        let entries = if !ProgramEvaluationPhaseV1::Hard.includes(constraint.mode) {
            0
        } else {
            match &constraint.body {
                CompiledProgramConstraintBodyV1::ComputedVisibleRelation {
                    occurrences, ..
                } => occurrences.len().checked_sub(1).ok_or(())?,
                CompiledProgramConstraintBodyV1::VisibleUnary { .. }
                | CompiledProgramConstraintBodyV1::VisibleRelation {
                    invocation: CompiledCoreRelationInvocationV1::PairSeparation(_),
                    ..
                } => 1,
                _ => 0,
            }
        };
        total.checked_add(entries).ok_or(())
    })?;
    result.try_reserve_exact(capacity).map_err(|_| ())?;
    for ordinal in 0..count {
        let index = constraint_indices.map_or(ordinal, |indices| indices[ordinal]);
        let constraint = &epoch.constraints[index];
        if !ProgramEvaluationPhaseV1::Hard.includes(constraint.mode) {
            continue;
        }
        if let CompiledProgramConstraintBodyV1::VisibleRelation {
            reference,
            candidates,
            invocation: CompiledCoreRelationInvocationV1::PairSeparation(bounds),
        } = &constraint.body
        {
            let [peer] = candidates.as_ref() else {
                return Err(());
            };
            result.push(RegionConstraintV1::Separation {
                slot: reference.slot,
                other: Some(peer.slot),
                bounds: *bounds,
            });
        }
        if let CompiledProgramConstraintBodyV1::ComputedVisibleRelation {
            occurrences,
            hierarchy,
        } = &constraint.body
        {
            for (pair, &required) in occurrences.windows(2).zip(hierarchy.ratios()) {
                result.push(RegionConstraintV1::TonalHierarchy {
                    slot: pair[0].slot,
                    weaker: pair[1].slot,
                    required,
                    weaker_contrast: None,
                });
            }
        }
        let CompiledProgramConstraintBodyV1::VisibleUnary {
            slot, invocation, ..
        } = constraint.body
        else {
            continue;
        };
        match epoch.evaluator.constraint_content(invocation) {
            ProgramConstraintContentV1::Wcag22Srgb8 { criterion, .. } => {
                result.push(RegionConstraintV1::Wcag { slot, criterion })
            }
            ProgramConstraintContentV1::ExactSrgb8 { expected, .. } => {
                result.push(RegionConstraintV1::Exact { slot, expected })
            }
            ProgramConstraintContentV1::NominalSrgb8Visible {
                release: crate::clean_set::EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2,
            } => result.push(RegionConstraintV1::NominalSrgb8Visible { slot }),
            ProgramConstraintContentV1::VisibleSeparation { bounds, .. } => {
                result.push(RegionConstraintV1::Separation {
                    slot,
                    other: None,
                    bounds,
                })
            }
            _ => {}
        }
    }
    bind_weaker_contrast(&mut result);
    Ok(result)
}

/// A hierarchy pair and a WCAG requirement can share a stronger bound only
/// when they address the same sealed occurrence (including its backdrop).
/// Index the WCAG prefix in place, strongest criterion first for each slot.
/// Preparation is O(n + w log w + h log w) without a second allocation.
fn bind_weaker_contrast(constraints: &mut [RegionConstraintV1]) {
    if !constraints
        .iter()
        .any(|constraint| matches!(constraint, RegionConstraintV1::TonalHierarchy { .. }))
    {
        return;
    }
    let mut wcag_count = 0;
    for index in 0..constraints.len() {
        if matches!(constraints[index], RegionConstraintV1::Wcag { .. }) {
            constraints.swap(index, wcag_count);
            wcag_count += 1;
        }
    }
    if wcag_count == 0 {
        return;
    }
    let (wcag, other) = constraints.split_at_mut(wcag_count);
    wcag.sort_unstable_by_key(|constraint| match *constraint {
        RegionConstraintV1::Wcag { slot, criterion } => Some((
            slot,
            std::cmp::Reverse(crate::wcag22::twice_minimum_contrast(criterion)),
        )),
        _ => None,
    });
    for constraint in other {
        let RegionConstraintV1::TonalHierarchy {
            weaker,
            weaker_contrast,
            ..
        } = constraint
        else {
            continue;
        };
        let index = wcag.partition_point(|constraint| {
            matches!(*constraint, RegionConstraintV1::Wcag { slot, .. } if slot < *weaker)
        });
        *weaker_contrast = match wcag.get(index) {
            Some(RegionConstraintV1::Wcag { slot, criterion }) if slot == weaker => {
                Some(*criterion)
            }
            _ => None,
        };
    }
}

struct RegionPruningV1<'scope> {
    constraints: Vec<RegionConstraintV1>,
    lower: Vec<Option<RegionConstraintPointV1>>,
    endpoint_evaluations: u64,
    physical: &'scope CompiledAppearanceConeV1,
}

impl RegionPruningV1<'_> {
    fn may_pass<Evaluation: ProgramConstraintEvaluatorSetV1>(
        &mut self,
        targets: ComputedTargetsV1<'_>,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<Evaluation>,
        scenario_set: NonEmptyScenarioSetV1<'_>,
        node: RegionNodeV1,
        values: &[Srgb8],
    ) -> Result<bool, ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>> {
        if self.constraints.is_empty() {
            return Ok(true);
        }
        let schema = epoch.observation_group.schema.as_slice();
        for case_index in 0..scenario_set.len().get() {
            let observed = scenario_set
                .physical_values(case_index)
                .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
            if observed.len() != schema.len() {
                return Err(ProgramSessionEvaluationError::InternalInvariant);
            }
            for high in [false, true] {
                for (i, target) in targets.iter().enumerate() {
                    let value = if i < node.depth {
                        values[i]
                    } else {
                        let (lo, hi) = if i == node.depth {
                            node.domain
                        } else {
                            target.declaration.domain
                        }
                        .bounds();
                        Srgb8::new(if high { hi } else { lo })
                    };
                    runtime
                        .bindings
                        .overwrite_paint_input_at(
                            target.binding,
                            EncodedPointPaintValueV1::opaque(value),
                        )
                        .map_err(map_program_execution_binding_error)?;
                }
                let mut paints = epoch
                    .graph
                    .prepare_scoped_surface_cases(
                        self.physical,
                        runtime.bindings,
                        runtime.workspace,
                    )
                    .map_err(map_program_execution_binding_error)?;
                let evaluation = paints.evaluate_surface_inputs_canonical(|i| observed[i].srgb8());
                self.endpoint_evaluations += 1;
                for (index, constraint) in self.constraints.iter().enumerate() {
                    let point = constraint
                        .point(&evaluation)
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                    if !high {
                        self.lower[index] = Some(point);
                        continue;
                    }
                    let possible = constraint
                        .may_pass(
                            self.lower[index]
                                .ok_or(ProgramSessionEvaluationError::InternalInvariant)?,
                            point,
                        )
                        .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                    if !possible {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
thread_local! {
    static CORRUPT_FINAL_SOURCE: std::cell::Cell<Option<Srgb8>> = const { std::cell::Cell::new(None) };
}

fn buffer<T: Clone>(len: usize, value: T) -> Result<Vec<T>, ()> {
    let mut result = Vec::new();
    result.try_reserve_exact(len).map_err(|_| ())?;
    result.resize(len, value);
    Ok(result)
}

fn select_component<Evaluation>(
    selection: &ComputedSelectionV1,
    component: &ComputedComponentV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<Evaluation>,
    scenario_set: NonEmptyScenarioSetV1<'_>,
) -> Result<SearchResultV1, ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let targets = ComputedTargetsV1::new(selection, Some(component));
    let count = targets.len();
    let allocate = |_| ProgramSessionEvaluationError::ResourceExhausted;
    let mut values = buffer(count, Srgb8::new([0; 3])).map_err(allocate)?;
    let mut evidence = SearchEvidenceV1 {
        completion: CompletionV1::BudgetExhausted,
        numerical_order_unresolved: false,
        visited: 0,
        regions: 0,
        points: 0,
        endpoint_evaluations: 0,
        examined: 0,
        open_regions: 0,
    };
    if selection.budget == 0 {
        return Ok(SearchResultV1 {
            reference_selections: None,
            evidence,
            values: None,
        });
    }
    values
        .iter_mut()
        .zip(targets.iter())
        .for_each(|(value, target)| *value = target.declaration.anchor);
    evidence.visited += 1;
    evidence.points += 1;
    if targets
        .iter()
        .zip(&values)
        .all(|(target, &value)| target.declaration.domain.contains(value))
    {
        evidence.examined += 1;
        apply_targets::<Evaluation>(targets, runtime, &values)?;
        if !scan_program_candidate_scoped(
            runtime,
            epoch,
            scenario_set,
            0,
            ProgramEvaluationPhaseV1::Hard,
            ProgramCandidateCollectionV1::none(),
            ProgramCandidateScanV1::Constraints {
                indices: &component.constraints,
                physical: Some(&component.physical),
            },
        )? {
            evidence.completion = CompletionV1::AuthoredExact;
            return Ok(SearchResultV1 {
                reference_selections: None,
                evidence,
                values: Some(values),
            });
        }
    }
    let mut best_values = buffer(count, Srgb8::new([0; 3])).map_err(allocate)?;
    let mut scores = buffer(count, 0.0).map_err(allocate)?;
    let mut best_scores = buffer(count, 0.0).map_err(allocate)?;
    let constraints = region_constraints(epoch, Some(&component.constraints)).map_err(allocate)?;
    let lower = buffer(constraints.len(), None).map_err(allocate)?;
    let mut pruning = RegionPruningV1 {
        physical: &component.physical,
        constraints,
        lower,
        endpoint_evaluations: 0,
    };
    let mut stack = Vec::new();
    // DFS хранит не более одной второй ветви на каждый шаг деления и одного
    // продолжения листа на target. Граница следует длинам осей, не произведению.
    let stack_limit = targets
        .iter()
        .try_fold(1_usize, |capacity, target| {
            capacity.checked_add(target.declaration.domain.split_depth() + 1)
        })
        .ok_or(ProgramSessionEvaluationError::ResourceExhausted)?;
    stack
        .try_reserve_exact(stack_limit)
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    let mut has_best = false;
    let Some(root) = region_node(
        0,
        targets[0].declaration.domain,
        selection.budget,
        targets[0].anchor_lab,
        &mut evidence,
    ) else {
        return Ok(SearchResultV1 {
            reference_selections: None,
            evidence,
            values: has_best.then_some(best_values),
        });
    };
    stack.push(root);
    while let Some(mut node) = stack.pop() {
        let depth = node.depth;
        let target = &targets[depth];
        let volume = node
            .domain
            .cardinality()
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        if node.ordinal.is_none() {
            scores[depth] = node.lower_score;
            let delta = f64::from_bits(target.declaration.max_delta_bits);
            let worse = node.lower_score > delta * delta
                || has_best
                    && compare_scores(&scores[..=depth], &best_scores[..=depth])
                        == Ordering::Greater;
            let possible =
                !worse && pruning.may_pass(targets, runtime, epoch, scenario_set, node, &values)?;
            evidence.endpoint_evaluations = pruning.endpoint_evaluations;
            if !possible {
                evidence
                    .close_region()
                    .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                continue;
            }
            if volume > 64 {
                let (left, right) = node.domain.split();
                let Some(left) = region_node(
                    depth,
                    left,
                    selection.budget,
                    target.anchor_lab,
                    &mut evidence,
                ) else {
                    return Ok(SearchResultV1 {
                        reference_selections: None,
                        evidence,
                        values: has_best.then_some(best_values),
                    });
                };
                let Some(right) = region_node(
                    depth,
                    right,
                    selection.budget,
                    target.anchor_lab,
                    &mut evidence,
                ) else {
                    return Ok(SearchResultV1 {
                        reference_selections: None,
                        evidence,
                        values: has_best.then_some(best_values),
                    });
                };
                let (first, second) = if left.lower_score <= right.lower_score {
                    (left, right)
                } else {
                    (right, left)
                };
                // Обе непересекающиеся ветви уже приняты; теперь закрываем родителя.
                evidence
                    .close_region()
                    .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                stack.push(second);
                stack.push(first);
                continue;
            }
            node.ordinal = Some(0);
            stack.push(node);
            continue;
        }
        if evidence.visited == selection.budget {
            return Ok(SearchResultV1 {
                reference_selections: None,
                evidence,
                values: has_best.then_some(best_values),
            });
        }
        evidence.visited += 1;
        evidence.points += 1;
        let ordinal = node
            .ordinal
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        let value = node.domain.value(ordinal);
        if ordinal + 1 < volume {
            // Продолжение сохраняет открытое обязательство всей листовой области.
            stack.push(RegionNodeV1 {
                ordinal: Some(ordinal + 1),
                ..node
            });
        } else {
            evidence
                .close_region()
                .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
        }
        values[depth] = value;
        scores[depth] = squared_distance(lab(value), target.anchor_lab);
        let delta = f64::from_bits(target.declaration.max_delta_bits);
        let worse_prefix = has_best
            && compare_scores(&scores[..=depth], &best_scores[..=depth]) == Ordering::Greater;
        if scores[depth] > delta * delta || worse_prefix {
            continue;
        }
        if depth + 1 < count {
            let Some(next) = region_node(
                depth + 1,
                targets[depth + 1].declaration.domain,
                selection.budget,
                targets[depth + 1].anchor_lab,
                &mut evidence,
            ) else {
                return Ok(SearchResultV1 {
                    reference_selections: None,
                    evidence,
                    values: has_best.then_some(best_values),
                });
            };
            stack.push(next);
            continue;
        }
        evidence.examined += 1;
        apply_targets::<Evaluation>(targets, runtime, &values)?;
        if !scan_program_candidate_scoped(
            runtime,
            epoch,
            scenario_set,
            0,
            ProgramEvaluationPhaseV1::Hard,
            ProgramCandidateCollectionV1::none(),
            ProgramCandidateScanV1::Constraints {
                indices: &component.constraints,
                physical: Some(&component.physical),
            },
        )? && (!has_best
            || compare_scores(&scores, &best_scores).then_with(|| values.cmp(&best_values))
                == Ordering::Less)
        {
            best_values.copy_from_slice(&values);
            best_scores.copy_from_slice(&scores);
            has_best = true;
        }
    }
    // Пустой стек не доказывает полноту: потерянная ветвь оставит обязательство.
    if evidence.open_regions != 0 {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }
    evidence.completion = CompletionV1::Complete;
    Ok(SearchResultV1 {
        reference_selections: None,
        evidence,
        values: has_best.then_some(best_values),
    })
}

fn compare_scores(left: &[f64], right: &[f64]) -> Ordering {
    left.iter()
        .zip(right)
        .map(|(a, b)| a.total_cmp(b))
        .find(|order| *order != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}

fn apply<Evaluation>(
    selection: &ComputedSelectionV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    values: &[Srgb8],
) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    apply_targets::<Evaluation>(ComputedTargetsV1::new(selection, None), runtime, values)
}

fn apply_targets<Evaluation>(
    targets: ComputedTargetsV1<'_>,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    values: &[Srgb8],
) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    if values.len() != targets.len() {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }
    for (target, &value) in targets.iter().zip(values) {
        runtime
            .bindings
            .overwrite_paint_input_at(target.binding, EncodedPointPaintValueV1::opaque(value))
            .map_err(map_program_execution_binding_error)?;
    }
    Ok(())
}

pub(crate) struct ComputedStoppedV1 {
    pub(super) diagnostics: crate::program_diagnostics::ProgramDiagnosticsV1,
    pub(super) identity: ComputedIdentityV1,
    pub(super) observation: RevisionBoundObservationV1,
    search: SearchEvidenceV1,
    execution_profile: [u8; 32],
}

impl session_private::EvidenceSealed for ComputedStoppedV1 {}
impl ComputedStoppedV1 {
    pub(crate) const fn diagnostics(&self) -> &crate::program_diagnostics::ProgramDiagnosticsV1 {
        &self.diagnostics
    }
}
impl SessionEvidenceV1 for ComputedStoppedV1 {
    fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComputedOptimalityV1 {
    Exact,
    Unfinished,
}

pub(super) fn prepare_point_anchors<E: ProgramConstraintEvaluatorSetV1>(
    epoch: &mut ProgramEpochV1<E>,
) -> Result<(), ProgramCompileError> {
    let needed = matches!(&epoch.target_selection, CompiledTargetSelectionV1::Computed(selection) if selection.components.iter().any(|component| matches!(component.mode, ProgramSelectionModeV1::JointPointV1 | ProgramSelectionModeV1::JointReferencePeersV1)));
    if needed {
        let anchors = contextual::reference_completion::CompiledPointAnchorsV1::new(epoch)
            .map_err(|error| match error {
                contextual::reference_completion::ReferenceCompletionErrorV1::ResourceExhausted => {
                    ProgramCompileError::ResourceExhausted
                }
                _ => ProgramCompileError::InternalInvariant,
            })?;
        if let CompiledTargetSelectionV1::Computed(selection) = &mut epoch.target_selection {
            selection.point_anchors = Some(Arc::new(anchors));
        }
    }
    Ok(())
}

pub(crate) struct ComputedVerifiedV1<Evaluation: ProgramConstraintEvaluatorSetV1> {
    pub(super) identity: ComputedIdentityV1,
    pub(super) observation: RevisionBoundObservationV1,
    values: Box<[Srgb8]>,
    search: SearchEvidenceV1,
    execution_profile: [u8; 32],
    reference_selections:
        Option<std::sync::Arc<[crate::program_wire::ProgramReferenceSelectionV1]>>,
    components: std::sync::Arc<[components::VerifiedComponentV1]>,
    selection_components: Arc<[ProgramSelectionComponentV1]>,
    pub(super) arena: ProgramEvaluationArenaLeaseV1<Evaluation>,
}

impl<Evaluation: ProgramConstraintEvaluatorSetV1> ComputedVerifiedV1<Evaluation> {
    pub(crate) fn selection_components(&self) -> &Arc<[ProgramSelectionComponentV1]> {
        &self.selection_components
    }
    pub(crate) fn reference_selections(
        &self,
    ) -> Option<&std::sync::Arc<[crate::program_wire::ProgramReferenceSelectionV1]>> {
        self.reference_selections.as_ref()
    }
    pub(super) fn optimality(&self) -> ComputedOptimalityV1 {
        // Each verified component retains its own search proof through observation reuse.
        // Completing the byte domain suffices only for a discrete selection policy.
        if self
            .components
            .iter()
            .zip(self.selection_components.iter())
            .all(|(verified, component)| verified.proves_optimum(component.mode()))
        {
            ComputedOptimalityV1::Exact
        } else {
            ComputedOptimalityV1::Unfinished
        }
    }
}

impl<Evaluation: ProgramConstraintEvaluatorSetV1> session_private::EvidenceSealed
    for ComputedVerifiedV1<Evaluation>
{
}
impl<Evaluation: ProgramConstraintEvaluatorSetV1> SessionEvidenceV1
    for ComputedVerifiedV1<Evaluation>
{
    fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }
}

pub(super) fn evaluate<Evaluation>(
    plan: &mut ProgramSessionPlan<Evaluation>,
    epoch: &ProgramEpochV1<Evaluation>,
    observation: RevisionBoundObservationV1,
    previous: Option<&ComputedVerifiedV1<Evaluation>>,
) -> ProgramSessionEvaluationResult<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    let (
        ProgramOwnerIdentityV1::Computed(identity),
        CompiledTargetSelectionV1::Computed(selection),
    ) = (epoch.content_identity, &epoch.target_selection)
    else {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    };
    let mut arena = plan
        .evaluation_arenas
        .guard(observation.arena_slot())
        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
    let mut runtime = ProgramEvaluationRuntimeV1 {
        bindings: &mut plan.bindings,
        workspace: &mut plan.workspace,
        presentation_cache: &mut plan.presentation_cache,
        family_artifacts: &plan.family_artifacts,
    };
    let scenarios = NonEmptyScenarioSetV1::from_admitted(&observation)
        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
    let counts = prepare_program_evaluation_arena(epoch, scenarios, arena.storage_mut())?;
    let reusable = previous.filter(|previous| {
        previous.identity == identity
            && previous.execution_profile == selection.execution_profile
            && previous.values.len() == selection.targets.len()
            && previous.components.len() == selection.components.len()
            && previous
                .observation
                .shares_schema_backing_with(&epoch.observation_group.schema)
            && observation.shares_schema_backing_with(&epoch.observation_group.schema)
    });
    let unchanged = reusable.filter(|previous| {
        previous.observation.physical_case_count() == scenarios.len().get()
            && (0..scenarios.len().get()).all(|index| {
                previous.observation.physical_values(index) == observation.physical_values(index)
            })
    });
    let (selected, components) = if let Some(previous) = unchanged {
        // Only this stronger equality preserves global report case indexes.
        // All paths still restore the committed tuple and certify it below.
        let mut values = Vec::new();
        values
            .try_reserve_exact(previous.values.len())
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
        values.extend_from_slice(&previous.values);
        (
            SearchResultV1 {
                reference_selections: previous.reference_selections.clone(),
                evidence: previous.search,
                values: Some(values),
            },
            previous.components.clone(),
        )
    } else {
        let (selected, components) = components::select(
            selection,
            &mut runtime,
            epoch,
            scenarios,
            arena.storage_mut(),
            reusable,
            &mut plan.reuse_history,
            &mut plan.reference_completion,
            &observation,
        )?;
        (selected, components.into())
    };
    arena.storage_mut().clear();
    match selected.values {
        Some(values) => {
            apply::<Evaluation>(selection, &mut runtime, &values)?;
            if selection
                .targets
                .iter()
                .any(|target| target.declaration.intent.is_some())
            {
                contextual::recheck_intents(selection, &mut runtime, epoch, scenarios, &values)?;
            }
            #[cfg(test)]
            if let Some(corrupted) = CORRUPT_FINAL_SOURCE.with(|value| value.take()) {
                runtime
                    .bindings
                    .overwrite_paint_input_at(
                        selection.targets[0].binding,
                        EncodedPointPaintValueV1::opaque(corrupted),
                    )
                    .map_err(map_program_execution_binding_error)?;
            }
            let origin = CandidateOriginV1::Computed {
                identity,
                values: values.into_boxed_slice(),
            };
            let result = collect_program_candidate_into(
                &mut runtime,
                epoch,
                scenarios,
                origin,
                0,
                arena.storage_mut(),
                counts,
                None,
            )?;
            let ProgramEvaluationOutcomeV1::Verified { origin } = result else {
                return Err(ProgramSessionEvaluationError::InternalInvariant);
            };
            let CandidateOriginV1::Computed {
                identity: selected_identity,
                values,
            } = origin
            else {
                return Err(ProgramSessionEvaluationError::InternalInvariant);
            };
            // Допустимость другого RGB не доказывает обещанное preference.
            // После полного physical recheck проверяем тот же tuple в тех
            // же compiled slots, которые потребляла реальная композиция.
            if selected_identity != identity
                || selection
                    .targets
                    .iter()
                    .zip(values.iter())
                    .any(|(target, &value)| {
                        runtime.bindings.paint_input_at(target.binding)
                            != Some(EncodedPointPaintValueV1::opaque(value))
                    })
            {
                return Err(ProgramSessionEvaluationError::InternalInvariant);
            }
            Ok(SessionDecision::Verified(
                ProgramVerifiedOutcomeV1::Computed(ComputedVerifiedV1 {
                    identity,
                    execution_profile: selection.execution_profile,
                    observation,
                    values,
                    search: selected.evidence,
                    reference_selections: selected.reference_selections,
                    components,
                    selection_components: selection.selection_components.clone(),
                    arena: arena.into_lease(),
                }),
            ))
        }
        None => {
            let diagnostics = witness::capture(
                selection,
                &mut runtime,
                epoch,
                scenarios,
                arena.storage_mut(),
                selected.evidence,
            )?;
            let evidence = ComputedStoppedV1 {
                diagnostics,
                identity,
                execution_profile: selection.execution_profile,
                observation,
                search: selected.evidence,
            };
            if selected.evidence.completion == CompletionV1::Complete {
                Ok(SessionDecision::Violation(
                    ProgramViolationOutcomeV1::Computed(evidence),
                ))
            } else {
                Ok(SessionDecision::Incomplete(evidence))
            }
        }
    }
}

mod components;
mod contextual;
mod contrast_bounds;
pub(super) use components::{ComponentReuseHistoryV1, compile_components, partition_components};
pub(super) use contextual::reference_completion::ReferenceCompletionScratchV1;
#[cfg(test)]
pub(super) use contextual::reference_completion::tests::incoming_test_hook as reference_completion_incoming_test_hook;
mod witness;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod qualification_tests;

#[cfg(test)]
mod color_tests;

#[cfg(test)]
mod color_context_oracle;

#[cfg(test)]
mod reuse_tests;

#[cfg(test)]
mod component_tests;

#[cfg(test)]
mod refinement_tests;

#[cfg(test)]
mod reference_tests;
