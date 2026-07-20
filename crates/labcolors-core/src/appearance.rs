//! Приватный point render-граф.
//!
//! Граф владеет только физической топологией. Paint материализуется без
//! подложки; Occurrence — единственное место, где Paint применяется к Surface;
//! `surfaceFrom` лишь даёт видимому результату occurrence новую Surface-
//! идентичность. Клиентский словарь, recipe-роли и perception-утверждения сюда
//! не входят.
//!
//! Единственная операция композиции — версионированный exact-композитор
//! [`crate::composition`]. `Opacity` только умножает straight
//! alpha уже материализованного Paint и никогда не композитит промежуточный
//! результат.
//!
//! Code-owned adapter представлен sealed borrowed IR и исполняется тем же
//! evaluator-ом, что результат декларативной компиляции. Структурное равенство
//! статического IR результату compiler-а закреплено proof-тестом; production-
//! артефакт не содержит admission/topology compiler.

#[cfg(test)]
use std::collections::BTreeSet;

use crate::Srgb8;
pub(crate) use crate::composition::CompositionProfileV1;

/// Непрозрачный handle цветового входа. Число — только идентичность.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ColorInputId(u32);

impl ColorInputId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Непрозрачный handle наблюдаемого point-входа поверхности.
///
/// Он намеренно не взаимозаменяем с [`ColorInputId`]: authored Paint source и
/// runtime backdrop имеют разные lifecycle и admission contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SurfaceInputPortId(u32);

impl SurfaceInputPortId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Непрозрачный handle входа straight alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct OpacityInputId(u32);

impl OpacityInputId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Непрозрачный handle Paint-программы.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PaintId(u32);

impl PaintId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Непрозрачный handle наблюдаемой поверхности.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SurfaceId(u32);

impl SurfaceId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Непрозрачный handle применения Paint к Surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct OccurrenceId(u32);

impl OccurrenceId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Структурная identity статической физической программы. Она описывает
/// topology/opcode/profile, а не числовые handles декларации или client ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysicalProgramIdentityV1 {
    SolidOpacityOverSurfaceEncodedSrgb8V1,
}

/// Routing внутри одной compiled point-программы отделён от физического
/// source-over proof. Эти code-owned handles не являются client-authored ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProgramOccurrenceBindingV1 {
    occurrence: OccurrenceId,
    subject: PaintId,
    backdrop_surface: SurfaceId,
}

impl ProgramOccurrenceBindingV1 {
    #[cfg(test)]
    pub(crate) const fn occurrence(self) -> OccurrenceId {
        self.occurrence
    }

    #[cfg(test)]
    pub(crate) const fn subject(self) -> PaintId {
        self.subject
    }

    #[cfg(test)]
    pub(crate) const fn backdrop_surface(self) -> SurfaceId {
        self.backdrop_surface
    }
}

/// Paint-конструкторы point-домена. Ни один вариант не знает Surface.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaintSpec {
    /// Непрозрачный encoded-sRGB8 Paint из цветового входа.
    Solid { id: PaintId, color: ColorInputId },
    /// Модуляция straight alpha существующего Paint.
    ///
    /// Рёбра задают порядок операций: узел вычисляет одно binary64-произведение
    /// alpha источника и связанного scalar. Перегруппировка создаёт другую
    /// численную программу; алгебраическая ассоциативность f64 не заявляется.
    /// Композиции на этом шаге нет.
    Opacity {
        id: PaintId,
        source: PaintId,
        opacity: OpacityInputId,
    },
}

#[cfg(test)]
impl PaintSpec {
    fn id(&self) -> PaintId {
        match self {
            Self::Solid { id, .. } | Self::Opacity { id, .. } => *id,
        }
    }
}

/// Surface либо приходит извне как point-вход, либо является видимым
/// результатом объявленного occurrence.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SurfaceSpec {
    Input {
        id: SurfaceId,
        port: SurfaceInputPortId,
    },
    FromOccurrence {
        id: SurfaceId,
        occurrence: OccurrenceId,
    },
}

#[cfg(test)]
impl SurfaceSpec {
    fn id(&self) -> SurfaceId {
        match self {
            Self::Input { id, .. } | Self::FromOccurrence { id, .. } => *id,
        }
    }
}

/// Единственная canonical application Paint к backdrop Surface.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OccurrenceSpec {
    pub(crate) id: OccurrenceId,
    pub(crate) subject: PaintId,
    pub(crate) against: SurfaceId,
    pub(crate) profile: CompositionProfileV1,
}

/// Ошибки AOT-компиляции декларации. Compiler принадлежит proof-поверхности и
/// не входит в production-артефакт.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompileError {
    DuplicateColorInput {
        input: ColorInputId,
    },
    DuplicateOpacityInput {
        input: OpacityInputId,
    },
    DuplicateSurfaceInputPort {
        input: SurfaceInputPortId,
    },
    DuplicatePaint {
        paint: PaintId,
    },
    DuplicateSurface {
        surface: SurfaceId,
    },
    DuplicateOccurrence {
        occurrence: OccurrenceId,
    },
    MissingPaintColorInput {
        paint: PaintId,
        input: ColorInputId,
    },
    MissingPaintSource {
        paint: PaintId,
        source: PaintId,
    },
    MissingPaintOpacityInput {
        paint: PaintId,
        input: OpacityInputId,
    },
    MissingSurfaceInputPort {
        surface: SurfaceId,
        input: SurfaceInputPortId,
    },
    MissingSurfaceOccurrence {
        surface: SurfaceId,
        occurrence: OccurrenceId,
    },
    MissingOccurrencePaint {
        occurrence: OccurrenceId,
        paint: PaintId,
    },
    MissingOccurrenceBackdrop {
        occurrence: OccurrenceId,
        surface: SurfaceId,
    },
    /// Только реальные участники циклов, без зависимых от них узлов.
    PaintCycle {
        paints: Vec<PaintId>,
    },
    /// Только реальные участники циклов, разложенные по typed ID-пространствам.
    RenderCycle {
        surfaces: Vec<SurfaceId>,
        occurrences: Vec<OccurrenceId>,
    },
}

/// Ошибки admission runtime bindings. Исполнение начинается только после
/// полной проверки, поэтому частичного результата нет.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BindingError {
    DuplicateColorBinding {
        input: ColorInputId,
    },
    DuplicateOpacityBinding {
        input: OpacityInputId,
    },
    DuplicateSurfaceInputBinding {
        input: SurfaceInputPortId,
    },
    MissingColorBinding {
        input: ColorInputId,
    },
    MissingOpacityBinding {
        input: OpacityInputId,
    },
    MissingSurfaceInputBinding {
        input: SurfaceInputPortId,
    },
    UnexpectedColorBinding {
        input: ColorInputId,
    },
    UnexpectedOpacityBinding {
        input: OpacityInputId,
    },
    UnexpectedSurfaceInputBinding {
        input: SurfaceInputPortId,
    },
    OpacityOutOfDomain {
        input: OpacityInputId,
        message: String,
    },
}

/// Единственный отказ sealed point-adapter-а: невалидная authored alpha.
/// Topology/bindings не представлены во входном типе и потому не могут дать
/// runtime-ошибку.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PointOpacityError {
    message: String,
}

impl PointOpacityError {
    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

/// Плоские декларации до атомарной компиляции. Порядок списков смысла не несёт.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AppearanceGraphSpec {
    color_inputs: Vec<ColorInputId>,
    surface_input_ports: Vec<SurfaceInputPortId>,
    opacity_inputs: Vec<OpacityInputId>,
    paints: Vec<PaintSpec>,
    surfaces: Vec<SurfaceSpec>,
    occurrences: Vec<OccurrenceSpec>,
}

#[cfg(test)]
impl AppearanceGraphSpec {
    pub(crate) fn new(
        color_inputs: Vec<ColorInputId>,
        surface_input_ports: Vec<SurfaceInputPortId>,
        opacity_inputs: Vec<OpacityInputId>,
        paints: Vec<PaintSpec>,
        surfaces: Vec<SurfaceSpec>,
        occurrences: Vec<OccurrenceSpec>,
    ) -> Self {
        Self {
            color_inputs,
            surface_input_ports,
            opacity_inputs,
            paints,
            surfaces,
            occurrences,
        }
    }

    /// Канонизировать декларации, проверить каждое typed-ребро и построить два
    /// детерминированных topo: Paint DAG и совместный Surface/Occurrence DAG.
    pub(crate) fn compile(self) -> Result<CompiledAppearanceGraph, CompileError> {
        let Self {
            mut color_inputs,
            mut surface_input_ports,
            mut opacity_inputs,
            mut paints,
            mut surfaces,
            mut occurrences,
        } = self;

        color_inputs.sort_unstable();
        if let Some(duplicate) = adjacent_duplicate(&color_inputs) {
            return Err(CompileError::DuplicateColorInput { input: duplicate });
        }

        surface_input_ports.sort_unstable();
        if let Some(duplicate) = adjacent_duplicate(&surface_input_ports) {
            return Err(CompileError::DuplicateSurfaceInputPort { input: duplicate });
        }

        opacity_inputs.sort_unstable();
        if let Some(duplicate) = adjacent_duplicate(&opacity_inputs) {
            return Err(CompileError::DuplicateOpacityInput { input: duplicate });
        }

        paints.sort_unstable_by_key(PaintSpec::id);
        if let Some(duplicate) = paints
            .windows(2)
            .find(|window| window[0].id() == window[1].id())
        {
            return Err(CompileError::DuplicatePaint {
                paint: duplicate[0].id(),
            });
        }

        surfaces.sort_unstable_by_key(SurfaceSpec::id);
        if let Some(duplicate) = surfaces
            .windows(2)
            .find(|window| window[0].id() == window[1].id())
        {
            return Err(CompileError::DuplicateSurface {
                surface: duplicate[0].id(),
            });
        }

        occurrences.sort_unstable_by_key(|occurrence| occurrence.id);
        if let Some(duplicate) = occurrences
            .windows(2)
            .find(|window| window[0].id == window[1].id)
        {
            return Err(CompileError::DuplicateOccurrence {
                occurrence: duplicate[0].id,
            });
        }

        let has_color = |id: ColorInputId| color_inputs.binary_search(&id).is_ok();
        let has_surface_input =
            |id: SurfaceInputPortId| surface_input_ports.binary_search(&id).is_ok();
        let has_opacity = |id: OpacityInputId| opacity_inputs.binary_search(&id).is_ok();
        let paint_index = |id: PaintId| paints.binary_search_by_key(&id, PaintSpec::id).ok();
        let surface_index =
            |id: SurfaceId| surfaces.binary_search_by_key(&id, SurfaceSpec::id).ok();
        let occurrence_index = |id: OccurrenceId| {
            occurrences
                .binary_search_by_key(&id, |occurrence| occurrence.id)
                .ok()
        };

        for paint in &paints {
            match *paint {
                PaintSpec::Solid { id, color } => {
                    if !has_color(color) {
                        return Err(CompileError::MissingPaintColorInput {
                            paint: id,
                            input: color,
                        });
                    }
                }
                PaintSpec::Opacity {
                    id,
                    source,
                    opacity,
                } => {
                    if paint_index(source).is_none() {
                        return Err(CompileError::MissingPaintSource { paint: id, source });
                    }
                    if !has_opacity(opacity) {
                        return Err(CompileError::MissingPaintOpacityInput {
                            paint: id,
                            input: opacity,
                        });
                    }
                }
            }
        }

        for surface in &surfaces {
            match *surface {
                SurfaceSpec::Input { id, port } => {
                    if !has_surface_input(port) {
                        return Err(CompileError::MissingSurfaceInputPort {
                            surface: id,
                            input: port,
                        });
                    }
                }
                SurfaceSpec::FromOccurrence { id, occurrence } => {
                    if occurrence_index(occurrence).is_none() {
                        return Err(CompileError::MissingSurfaceOccurrence {
                            surface: id,
                            occurrence,
                        });
                    }
                }
            }
        }

        for occurrence in &occurrences {
            if paint_index(occurrence.subject).is_none() {
                return Err(CompileError::MissingOccurrencePaint {
                    occurrence: occurrence.id,
                    paint: occurrence.subject,
                });
            }
            if surface_index(occurrence.against).is_none() {
                return Err(CompileError::MissingOccurrenceBackdrop {
                    occurrence: occurrence.id,
                    surface: occurrence.against,
                });
            }
        }

        let paint_dependencies: Vec<Option<usize>> = paints
            .iter()
            .map(|paint| match *paint {
                PaintSpec::Solid { .. } => None,
                PaintSpec::Opacity { source, .. } => paint_index(source),
            })
            .collect();
        let paint_keys: Vec<PaintId> = paints.iter().map(PaintSpec::id).collect();
        let paint_topo =
            canonical_functional_topology(&paint_keys, &paint_dependencies).map_err(|members| {
                CompileError::PaintCycle {
                    paints: members.into_iter().map(|index| paint_keys[index]).collect(),
                }
            })?;

        let surface_count = surfaces.len();
        let mut render_keys: Vec<RenderKey> = surfaces
            .iter()
            .map(|surface| RenderKey::Surface(surface.id()))
            .collect();
        render_keys.extend(
            occurrences
                .iter()
                .map(|occurrence| RenderKey::Occurrence(occurrence.id)),
        );
        let mut render_dependencies: Vec<Option<usize>> = surfaces
            .iter()
            .map(|surface| match *surface {
                SurfaceSpec::Input { .. } => None,
                SurfaceSpec::FromOccurrence { occurrence, .. } => {
                    occurrence_index(occurrence).map(|index| surface_count + index)
                }
            })
            .collect();
        render_dependencies.extend(
            occurrences
                .iter()
                .map(|occurrence| surface_index(occurrence.against)),
        );
        let render_topo_indices = canonical_functional_topology(&render_keys, &render_dependencies)
            .map_err(|members| {
                let mut cyclic_surfaces = Vec::new();
                let mut cyclic_occurrences = Vec::new();
                for index in members {
                    if index < surface_count {
                        cyclic_surfaces.push(surfaces[index].id());
                    } else {
                        cyclic_occurrences.push(occurrences[index - surface_count].id);
                    }
                }
                cyclic_surfaces.sort_unstable();
                cyclic_occurrences.sort_unstable();
                CompileError::RenderCycle {
                    surfaces: cyclic_surfaces,
                    occurrences: cyclic_occurrences,
                }
            })?;
        let render_topo = render_topo_indices
            .into_iter()
            .map(|index| {
                if index < surface_count {
                    RenderNode::Surface(index)
                } else {
                    RenderNode::Occurrence(index - surface_count)
                }
            })
            .collect();

        let compiled_paints = paints
            .iter()
            .map(|paint| match *paint {
                PaintSpec::Solid { id, color } => CompiledPaintSpec::Solid { id, color },
                PaintSpec::Opacity {
                    id,
                    source,
                    opacity,
                } => CompiledPaintSpec::Opacity {
                    id,
                    source: paint_index(source)
                        .unwrap_or_else(|| unreachable!("paint links were validated")),
                    opacity,
                },
            })
            .collect();
        let compiled_surfaces = surfaces
            .iter()
            .map(|surface| match *surface {
                SurfaceSpec::Input { id, port } => CompiledSurfaceSpec::Input { id, port },
                SurfaceSpec::FromOccurrence { id, occurrence } => {
                    CompiledSurfaceSpec::FromOccurrence {
                        id,
                        occurrence: occurrence_index(occurrence)
                            .unwrap_or_else(|| unreachable!("occurrence links were validated")),
                    }
                }
            })
            .collect();
        let compiled_occurrences = occurrences
            .iter()
            .map(|occurrence| CompiledOccurrenceSpec {
                id: occurrence.id,
                subject_id: occurrence.subject,
                subject: paint_index(occurrence.subject)
                    .unwrap_or_else(|| unreachable!("paint links were validated")),
                against_id: occurrence.against,
                against: surface_index(occurrence.against)
                    .unwrap_or_else(|| unreachable!("surface links were validated")),
                profile: occurrence.profile,
            })
            .collect();

        Ok(CompiledAppearanceGraph {
            color_inputs,
            surface_input_ports,
            opacity_inputs,
            paints: compiled_paints,
            surfaces: compiled_surfaces,
            occurrences: compiled_occurrences,
            paint_topo,
            render_topo,
        })
    }
}

#[cfg(test)]
fn adjacent_duplicate<T: Copy + Eq>(sorted: &[T]) -> Option<T> {
    sorted
        .windows(2)
        .find(|window| window[0] == window[1])
        .map(|window| window[0])
}

/// Topo для functional dependency graph: каждый узел имеет не более одной
/// зависимости. При цикле возвращает только его реальные узлы, а не весь
/// заблокированный Kahn-остаток.
#[cfg(test)]
fn canonical_functional_topology<K: Copy + Ord>(
    keys: &[K],
    dependencies: &[Option<usize>],
) -> Result<Vec<usize>, Vec<usize>> {
    debug_assert_eq!(keys.len(), dependencies.len());
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); keys.len()];
    let mut pending: Vec<usize> = vec![0; keys.len()];
    for (node, dependency) in dependencies.iter().enumerate() {
        if let Some(dependency) = dependency {
            dependents[*dependency].push(node);
            pending[node] = 1;
        }
    }
    let mut ready: BTreeSet<(K, usize)> = keys
        .iter()
        .copied()
        .enumerate()
        .filter(|(index, _)| pending[*index] == 0)
        .map(|(index, key)| (key, index))
        .collect();
    let mut topo = Vec::with_capacity(keys.len());
    while let Some((_, node)) = ready.pop_first() {
        topo.push(node);
        for &dependent in &dependents[node] {
            pending[dependent] -= 1;
            if pending[dependent] == 0 {
                ready.insert((keys[dependent], dependent));
            }
        }
    }
    if topo.len() == keys.len() {
        Ok(topo)
    } else {
        Err(functional_cycle_members(dependencies))
    }
}

/// Итеративный functional-cycle detector: O(V), без риска переполнить стек на
/// большом входе и без ложного включения деревьев, ведущих в цикл.
#[cfg(test)]
fn functional_cycle_members(dependencies: &[Option<usize>]) -> Vec<usize> {
    const UNSEEN: u8 = 0;
    const ACTIVE: u8 = 1;
    const DONE: u8 = 2;

    let mut state = vec![UNSEEN; dependencies.len()];
    let mut position = vec![usize::MAX; dependencies.len()];
    let mut cycles = Vec::new();
    for start in 0..dependencies.len() {
        if state[start] != UNSEEN {
            continue;
        }
        let mut path = Vec::new();
        let mut current = Some(start);
        while let Some(node) = current {
            match state[node] {
                UNSEEN => {
                    state[node] = ACTIVE;
                    position[node] = path.len();
                    path.push(node);
                    current = dependencies[node];
                }
                ACTIVE => {
                    let cycle_start = position[node];
                    if cycle_start != usize::MAX {
                        cycles.extend_from_slice(&path[cycle_start..]);
                    }
                    break;
                }
                DONE => break,
                _ => unreachable!("cycle detector has a closed state set"),
            }
        }
        for node in path {
            state[node] = DONE;
            position[node] = usize::MAX;
        }
    }
    cycles.sort_unstable();
    cycles.dedup();
    cycles
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg(test)]
enum RenderKey {
    Surface(SurfaceId),
    Occurrence(OccurrenceId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderNode {
    Surface(usize),
    Occurrence(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CompiledPaintSpec {
    Solid {
        id: PaintId,
        color: ColorInputId,
    },
    Opacity {
        id: PaintId,
        source: usize,
        opacity: OpacityInputId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CompiledSurfaceSpec {
    Input {
        id: SurfaceId,
        port: SurfaceInputPortId,
    },
    FromOccurrence {
        id: SurfaceId,
        occurrence: usize,
    },
}

#[cfg(test)]
impl CompiledSurfaceSpec {
    fn id(&self) -> SurfaceId {
        match self {
            Self::Input { id, .. } | Self::FromOccurrence { id, .. } => *id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompiledOccurrenceSpec {
    id: OccurrenceId,
    subject_id: PaintId,
    subject: usize,
    against_id: SurfaceId,
    against: usize,
    profile: CompositionProfileV1,
}

/// Канонический compiled IR с индексными ссылками: после проверки bindings
/// исполнение самих Paint/Surface/Occurrence узлов линейно по их числу.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledAppearanceGraph {
    color_inputs: Vec<ColorInputId>,
    surface_input_ports: Vec<SurfaceInputPortId>,
    opacity_inputs: Vec<OpacityInputId>,
    paints: Vec<CompiledPaintSpec>,
    surfaces: Vec<CompiledSurfaceSpec>,
    occurrences: Vec<CompiledOccurrenceSpec>,
    paint_topo: Vec<usize>,
    render_topo: Vec<RenderNode>,
}

/// Borrowed runtime-представление уже проверенного compiled IR. Оно отделяет
/// исполнение от compiler-а и позволяет статическим внутренним adapter-ам не
/// тащить admission/topology machinery в конечный binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompiledInputSchema<'a> {
    color_inputs: &'a [ColorInputId],
    surface_input_ports: &'a [SurfaceInputPortId],
    opacity_inputs: &'a [OpacityInputId],
}

impl<'a> CompiledInputSchema<'a> {
    const fn new(
        color_inputs: &'a [ColorInputId],
        surface_input_ports: &'a [SurfaceInputPortId],
        opacity_inputs: &'a [OpacityInputId],
    ) -> Self {
        Self {
            color_inputs,
            surface_input_ports,
            opacity_inputs,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompiledAppearanceProgram<'a> {
    color_inputs: &'a [ColorInputId],
    surface_input_ports: &'a [SurfaceInputPortId],
    opacity_inputs: &'a [OpacityInputId],
    paints: &'a [CompiledPaintSpec],
    surfaces: &'a [CompiledSurfaceSpec],
    occurrences: &'a [CompiledOccurrenceSpec],
    paint_topo: &'a [usize],
    render_topo: &'a [RenderNode],
}

impl<'a> CompiledAppearanceProgram<'a> {
    /// Создаёт view только из IR, уже доказанно эквивалентного результату
    /// `AppearanceGraphSpec::compile`. Static adapter обязан защищать это
    /// равенство characterization-тестом.
    const fn from_validated_parts(
        inputs: CompiledInputSchema<'a>,
        paints: &'a [CompiledPaintSpec],
        surfaces: &'a [CompiledSurfaceSpec],
        occurrences: &'a [CompiledOccurrenceSpec],
        paint_topo: &'a [usize],
        render_topo: &'a [RenderNode],
    ) -> Self {
        Self {
            color_inputs: inputs.color_inputs,
            surface_input_ports: inputs.surface_input_ports,
            opacity_inputs: inputs.opacity_inputs,
            paints,
            surfaces,
            occurrences,
            paint_topo,
            render_topo,
        }
    }
}

const POINT_SOURCE: ColorInputId = ColorInputId::new(0);
const POINT_CONTEXT: SurfaceInputPortId = SurfaceInputPortId::new(0);
const POINT_OPACITY: OpacityInputId = OpacityInputId::new(0);
const POINT_SOLID_PAINT: PaintId = PaintId::new(0);
const POINT_OPACITY_PAINT: PaintId = PaintId::new(1);
const POINT_CONTEXT_SURFACE: SurfaceId = SurfaceId::new(0);
const POINT_DERIVED_SURFACE: SurfaceId = SurfaceId::new(1);
const POINT_OCCURRENCE: OccurrenceId = OccurrenceId::new(0);

const POINT_COLOR_INPUTS: [ColorInputId; 1] = [POINT_SOURCE];
const POINT_SURFACE_INPUT_PORTS: [SurfaceInputPortId; 1] = [POINT_CONTEXT];
const POINT_OPACITY_INPUTS: [OpacityInputId; 1] = [POINT_OPACITY];
const POINT_PAINTS: [CompiledPaintSpec; 2] = [
    CompiledPaintSpec::Solid {
        id: POINT_SOLID_PAINT,
        color: POINT_SOURCE,
    },
    CompiledPaintSpec::Opacity {
        id: POINT_OPACITY_PAINT,
        source: 0,
        opacity: POINT_OPACITY,
    },
];
const POINT_SURFACES: [CompiledSurfaceSpec; 2] = [
    CompiledSurfaceSpec::Input {
        id: POINT_CONTEXT_SURFACE,
        port: POINT_CONTEXT,
    },
    CompiledSurfaceSpec::FromOccurrence {
        id: POINT_DERIVED_SURFACE,
        occurrence: 0,
    },
];
const POINT_OCCURRENCES: [CompiledOccurrenceSpec; 1] = [CompiledOccurrenceSpec {
    id: POINT_OCCURRENCE,
    subject_id: POINT_OPACITY_PAINT,
    subject: 1,
    against_id: POINT_CONTEXT_SURFACE,
    against: 0,
    profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
}];
const POINT_PAINT_TOPO: [usize; 2] = [0, 1];
const POINT_RENDER_TOPO: [RenderNode; 3] = [
    RenderNode::Surface(0),
    RenderNode::Occurrence(0),
    RenderNode::Surface(1),
];
const POINT_OPACITY_OVER_SURFACE_V1: CompiledAppearanceProgram<'static> =
    CompiledAppearanceProgram::from_validated_parts(
        CompiledInputSchema::new(
            &POINT_COLOR_INPUTS,
            &POINT_SURFACE_INPUT_PORTS,
            &POINT_OPACITY_INPUTS,
        ),
        &POINT_PAINTS,
        &POINT_SURFACES,
        &POINT_OCCURRENCES,
        &POINT_PAINT_TOPO,
        &POINT_RENDER_TOPO,
    );

/// Sealed adapter минимального point-program. Он принимает только физические
/// значения; topology и индексные ссылки нельзя собрать за пределами модуля.
pub(crate) struct PointOpacityOverSurfaceV1;

impl PointOpacityOverSurfaceV1 {
    pub(crate) const fn physical_identity() -> PhysicalProgramIdentityV1 {
        PhysicalProgramIdentityV1::SolidOpacityOverSurfaceEncodedSrgb8V1
    }

    pub(crate) fn evaluate(
        source: [u8; 3],
        opacity: f64,
        backdrop: [u8; 3],
    ) -> Result<ResolvedOccurrence, PointOpacityError> {
        let opacity =
            crate::composition::AdmittedOpacityV1::new(opacity).map_err(|_| PointOpacityError {
                message: format!("alpha вне конечного [0,1]: {opacity}"),
            })?;
        Ok(Self::evaluate_value(source, opacity, backdrop))
    }

    /// Исполнение после typed admission alpha. Этим входом final recheck
    /// исключает невозможную повторную numeric validation и stringly error.
    pub(crate) fn evaluate_admitted(
        source: [u8; 3],
        opacity: crate::composition::AdmittedOpacityV1,
        backdrop: [u8; 3],
    ) -> ResolvedOccurrence {
        Self::evaluate_value(source, opacity, backdrop)
    }

    fn evaluate_value(
        source: [u8; 3],
        opacity: crate::composition::AdmittedOpacityV1,
        backdrop: [u8; 3],
    ) -> ResolvedOccurrence {
        let mut paints = [None; 2];
        let mut surfaces = [None; 2];
        let mut occurrences = [None; 1];
        POINT_OPACITY_OVER_SURFACE_V1.execute_into(
            |id| {
                debug_assert_eq!(id, POINT_SOURCE);
                Srgb8::new(source)
            },
            |id| {
                debug_assert_eq!(id, POINT_CONTEXT);
                Srgb8::new(backdrop)
            },
            |id| {
                debug_assert_eq!(id, POINT_OPACITY);
                opacity
            },
            &mut paints,
            &mut surfaces,
            &mut occurrences,
        );
        occurrences[0].unwrap_or_else(|| unreachable!())
    }
}

#[cfg(test)]
pub(crate) fn point_opacity_over_surface_declarative_spec() -> AppearanceGraphSpec {
    AppearanceGraphSpec::new(
        POINT_COLOR_INPUTS.to_vec(),
        POINT_SURFACE_INPUT_PORTS.to_vec(),
        POINT_OPACITY_INPUTS.to_vec(),
        vec![
            PaintSpec::Solid {
                id: POINT_SOLID_PAINT,
                color: POINT_SOURCE,
            },
            PaintSpec::Opacity {
                id: POINT_OPACITY_PAINT,
                source: POINT_SOLID_PAINT,
                opacity: POINT_OPACITY,
            },
        ],
        vec![
            SurfaceSpec::Input {
                id: POINT_CONTEXT_SURFACE,
                port: POINT_CONTEXT,
            },
            SurfaceSpec::FromOccurrence {
                id: POINT_DERIVED_SURFACE,
                occurrence: POINT_OCCURRENCE,
            },
        ],
        vec![OccurrenceSpec {
            id: POINT_OCCURRENCE,
            subject: POINT_OPACITY_PAINT,
            against: POINT_CONTEXT_SURFACE,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        }],
    )
}

#[cfg(test)]
pub(crate) fn point_program_matches(compiled: &CompiledAppearanceGraph) -> bool {
    compiled.matches_program(POINT_OPACITY_OVER_SURFACE_V1)
}

/// Runtime bindings одного атомарного evaluate.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AppearanceBindings {
    colors: Vec<(ColorInputId, Srgb8)>,
    surfaces: Vec<(SurfaceInputPortId, Srgb8)>,
    opacities: Vec<(OpacityInputId, f64)>,
}

#[cfg(test)]
impl AppearanceBindings {
    pub(crate) fn new(
        mut colors: Vec<(ColorInputId, Srgb8)>,
        mut surfaces: Vec<(SurfaceInputPortId, Srgb8)>,
        mut opacities: Vec<(OpacityInputId, f64)>,
    ) -> Self {
        colors.sort_unstable_by_key(|(id, _)| *id);
        surfaces.sort_unstable_by_key(|(id, _)| *id);
        opacities.sort_unstable_by_key(|(id, _)| *id);
        Self {
            colors,
            surfaces,
            opacities,
        }
    }
}

/// Материализованный encoded point Paint вне зависимости от стадии владения.
///
/// Graph materialization и downstream recheck разделяют это одно физическое
/// значение. Тип alpha делает повторную admission перед каждым occurrence
/// невозможной; его биты без потерь переходят в certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EncodedPointPaintV1 {
    id: PaintId,
    source: Srgb8,
    opacity: crate::composition::AdmittedOpacityV1,
}

impl EncodedPointPaintV1 {
    pub(crate) const fn from_admitted(
        id: PaintId,
        source: Srgb8,
        opacity: crate::composition::AdmittedOpacityV1,
    ) -> Self {
        Self {
            id,
            source,
            opacity,
        }
    }

    pub(crate) const fn id(self) -> PaintId {
        self.id
    }

    pub(crate) const fn source(self) -> Srgb8 {
        self.source
    }

    pub(crate) const fn opacity(self) -> crate::composition::AdmittedOpacityV1 {
        self.opacity
    }

    #[cfg(test)]
    pub(crate) const fn opacity_bits(self) -> u64 {
        self.opacity.bits()
    }
}

/// Replayable exact point-composite certificate. Он доказывает только
/// заявленную математическую операцию, не readability и не browser pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceOverCertificateV1 {
    profile: CompositionProfileV1,
    subject_rgb: [u8; 3],
    subject_opacity: crate::composition::AdmittedOpacityV1,
    backdrop_rgb: [u8; 3],
    output_rgb: [u8; 3],
}

impl SourceOverCertificateV1 {
    #[cfg(test)]
    pub(crate) fn replay(&self) -> [u8; 3] {
        self.profile
            .composite(self.subject_rgb, self.subject_opacity, self.backdrop_rgb)
    }

    #[cfg(test)]
    pub(crate) fn profile(&self) -> CompositionProfileV1 {
        self.profile
    }

    pub(crate) fn subject_rgb(&self) -> [u8; 3] {
        self.subject_rgb
    }

    pub(crate) fn subject_opacity_bits(&self) -> u64 {
        self.subject_opacity.bits()
    }

    pub(crate) fn backdrop_rgb(&self) -> [u8; 3] {
        self.backdrop_rgb
    }

    pub(crate) fn output_rgb(&self) -> [u8; 3] {
        self.output_rgb
    }
}

/// Разрешённое применение Paint к Surface. Сертификат структурно принадлежит
/// occurrence; `surfaceFrom` второго сертификата не создаёт.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolvedOccurrence {
    id: OccurrenceId,
    subject: PaintId,
    against: SurfaceId,
    backdrop: [u8; 3],
    visible: [u8; 3],
    certificate: SourceOverCertificateV1,
}

impl ResolvedOccurrence {
    #[cfg(test)]
    pub(crate) fn id(&self) -> OccurrenceId {
        self.id
    }

    #[cfg(test)]
    pub(crate) fn subject(&self) -> PaintId {
        self.subject
    }

    #[cfg(test)]
    pub(crate) fn against(&self) -> SurfaceId {
        self.against
    }

    #[cfg(test)]
    pub(crate) fn backdrop(&self) -> [u8; 3] {
        self.backdrop
    }

    pub(crate) fn visible(&self) -> [u8; 3] {
        self.visible
    }

    pub(crate) fn modeled_srgb8_point(&self) -> ModeledSrgb8PointOccurrence {
        ModeledSrgb8PointOccurrence {
            visible: self.visible,
            backdrop: self.backdrop,
        }
    }

    pub(crate) fn visible_point_binding(&self) -> VisiblePointBindingV1 {
        VisiblePointBindingV1 {
            program_occurrence: self.program_occurrence_binding(),
            occurrence: self.certificate,
        }
    }

    pub(crate) const fn program_occurrence_binding(&self) -> ProgramOccurrenceBindingV1 {
        ProgramOccurrenceBindingV1 {
            occurrence: self.id,
            subject: self.subject,
            backdrop_surface: self.against,
        }
    }

    pub(crate) fn certificate(&self) -> &SourceOverCertificateV1 {
        &self.certificate
    }
}

/// Owned point-target для evaluator-ов финального видимого результата.
/// Ссылки на occurrence/certificate здесь нет: evaluator структурно не может
/// подменить скомпозитированный stimulus authored source-цветом.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ModeledSrgb8PointOccurrence {
    visible: [u8; 3],
    backdrop: [u8; 3],
}

impl ModeledSrgb8PointOccurrence {
    pub(crate) fn visible(self) -> [u8; 3] {
        self.visible
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "shipped exact evaluator reads visible; backdrop is consumed by the test-private WCAG adapter"
        )
    )]
    pub(crate) fn backdrop(self) -> [u8; 3] {
        self.backdrop
    }
}

/// Physical identity modeled point-occurrence, связанная с exact source-over
/// proof. Assessment не может пережить смену Paint/Surface/alpha лишь потому,
/// что финальные байты случайно совпали.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisiblePointBindingV1 {
    program_occurrence: ProgramOccurrenceBindingV1,
    occurrence: SourceOverCertificateV1,
}

impl VisiblePointBindingV1 {
    pub(crate) fn program_occurrence(self) -> ProgramOccurrenceBindingV1 {
        self.program_occurrence
    }

    pub(crate) fn occurrence(self) -> SourceOverCertificateV1 {
        self.occurrence
    }

    pub(crate) const fn occurrence_ref(&self) -> &SourceOverCertificateV1 {
        &self.occurrence
    }
}

/// Полный атомарный результат evaluate в каноническом typed-ID порядке.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppearanceEvaluation {
    paints: Vec<EncodedPointPaintV1>,
    surfaces: Vec<(SurfaceId, Srgb8)>,
    occurrences: Vec<ResolvedOccurrence>,
}

#[cfg(test)]
impl AppearanceEvaluation {
    pub(crate) fn paint(&self, id: PaintId) -> Option<&EncodedPointPaintV1> {
        self.paints
            .binary_search_by_key(&id, |paint| paint.id)
            .ok()
            .map(|index| &self.paints[index])
    }

    pub(crate) fn surface_rgb(&self, id: SurfaceId) -> Option<[u8; 3]> {
        self.surfaces
            .binary_search_by_key(&id, |(surface, _)| *surface)
            .ok()
            .map(|index| self.surfaces[index].1.bytes())
    }

    pub(crate) fn occurrence(&self, id: OccurrenceId) -> Option<&ResolvedOccurrence> {
        self.occurrences
            .binary_search_by_key(&id, |occurrence| occurrence.id)
            .ok()
            .map(|index| &self.occurrences[index])
    }
}

#[cfg(test)]
impl CompiledAppearanceGraph {
    fn program(&self) -> CompiledAppearanceProgram<'_> {
        CompiledAppearanceProgram::from_validated_parts(
            CompiledInputSchema::new(
                &self.color_inputs,
                &self.surface_input_ports,
                &self.opacity_inputs,
            ),
            &self.paints,
            &self.surfaces,
            &self.occurrences,
            &self.paint_topo,
            &self.render_topo,
        )
    }

    fn matches_program(&self, program: CompiledAppearanceProgram<'_>) -> bool {
        self.program() == program
    }

    pub(crate) fn evaluate(
        &self,
        bindings: &AppearanceBindings,
    ) -> Result<AppearanceEvaluation, BindingError> {
        self.program().evaluate(bindings)
    }
}

impl CompiledAppearanceProgram<'_> {
    /// Проверить bindings, материализовать Paint DAG один раз и исполнить
    /// Surface/Occurrence DAG один раз. Частичный результат не возвращается.
    #[cfg(test)]
    pub(crate) fn evaluate(
        &self,
        bindings: &AppearanceBindings,
    ) -> Result<AppearanceEvaluation, BindingError> {
        let colors = &bindings.colors;
        if let Some(window) = colors.windows(2).find(|window| window[0].0 == window[1].0) {
            return Err(BindingError::DuplicateColorBinding { input: window[0].0 });
        }
        let opacities = &bindings.opacities;
        if let Some(window) = opacities
            .windows(2)
            .find(|window| window[0].0 == window[1].0)
        {
            return Err(BindingError::DuplicateOpacityBinding { input: window[0].0 });
        }
        let surfaces = &bindings.surfaces;
        if let Some(window) = surfaces
            .windows(2)
            .find(|window| window[0].0 == window[1].0)
        {
            return Err(BindingError::DuplicateSurfaceInputBinding { input: window[0].0 });
        }

        for declared in self.color_inputs {
            if colors
                .binary_search_by_key(declared, |(id, _)| *id)
                .is_err()
            {
                return Err(BindingError::MissingColorBinding { input: *declared });
            }
        }
        for declared in self.opacity_inputs {
            if opacities
                .binary_search_by_key(declared, |(id, _)| *id)
                .is_err()
            {
                return Err(BindingError::MissingOpacityBinding { input: *declared });
            }
        }
        for declared in self.surface_input_ports {
            if surfaces
                .binary_search_by_key(declared, |(id, _)| *id)
                .is_err()
            {
                return Err(BindingError::MissingSurfaceInputBinding { input: *declared });
            }
        }
        for (bound, _) in colors {
            if self.color_inputs.binary_search(bound).is_err() {
                return Err(BindingError::UnexpectedColorBinding { input: *bound });
            }
        }
        for (bound, _) in opacities {
            if self.opacity_inputs.binary_search(bound).is_err() {
                return Err(BindingError::UnexpectedOpacityBinding { input: *bound });
            }
        }
        for (bound, _) in surfaces {
            if self.surface_input_ports.binary_search(bound).is_err() {
                return Err(BindingError::UnexpectedSurfaceInputBinding { input: *bound });
            }
        }
        for (input, alpha) in opacities {
            if let Err(message) = crate::composition::validate_alpha(*alpha) {
                return Err(BindingError::OpacityOutOfDomain {
                    input: *input,
                    message,
                });
            }
        }

        let color_value = |id: ColorInputId| -> Srgb8 {
            let index = colors
                .binary_search_by_key(&id, |(bound, _)| *bound)
                .unwrap_or_else(|_| unreachable!("bindings were matched before evaluation"));
            colors[index].1
        };
        let surface_value = |id: SurfaceInputPortId| -> Srgb8 {
            let index = surfaces
                .binary_search_by_key(&id, |(bound, _)| *bound)
                .unwrap_or_else(|_| unreachable!("bindings were matched before evaluation"));
            surfaces[index].1
        };
        let opacity_value = |id: OpacityInputId| -> crate::composition::AdmittedOpacityV1 {
            let index = opacities
                .binary_search_by_key(&id, |(bound, _)| *bound)
                .unwrap_or_else(|_| unreachable!("bindings were matched before evaluation"));
            crate::composition::AdmittedOpacityV1::new(opacities[index].1)
                .unwrap_or_else(|_| unreachable!("opacity bindings were admitted before execution"))
        };

        let mut resolved_paints: Vec<Option<EncodedPointPaintV1>> = vec![None; self.paints.len()];
        let mut resolved_surfaces: Vec<Option<Srgb8>> = vec![None; self.surfaces.len()];
        let mut resolved_occurrences: Vec<Option<ResolvedOccurrence>> =
            vec![None; self.occurrences.len()];
        self.execute_into(
            color_value,
            surface_value,
            opacity_value,
            &mut resolved_paints,
            &mut resolved_surfaces,
            &mut resolved_occurrences,
        );

        let paints = resolved_paints
            .into_iter()
            .map(|paint| paint.unwrap_or_else(|| unreachable!("Paint topo covers every node")))
            .collect();
        let surfaces = self
            .surfaces
            .iter()
            .zip(resolved_surfaces)
            .map(|(surface, value)| {
                (
                    surface.id(),
                    value.unwrap_or_else(|| unreachable!("render topo covers every Surface")),
                )
            })
            .collect();
        let occurrences = resolved_occurrences
            .into_iter()
            .map(|occurrence| {
                occurrence.unwrap_or_else(|| unreachable!("render topo covers every Occurrence"))
            })
            .collect();

        Ok(AppearanceEvaluation {
            paints,
            surfaces,
            occurrences,
        })
    }

    /// Единственное исполнение compiled IR. Scratch принадлежит caller-у:
    /// static adapter использует stack arrays, test-only generic admission —
    /// динамические buffers. Алгоритм и сертификат при этом общие.
    fn execute_into<C, S, O>(
        &self,
        color_value: C,
        surface_value: S,
        opacity_value: O,
        resolved_paints: &mut [Option<EncodedPointPaintV1>],
        resolved_surfaces: &mut [Option<Srgb8>],
        resolved_occurrences: &mut [Option<ResolvedOccurrence>],
    ) where
        C: Fn(ColorInputId) -> Srgb8,
        S: Fn(SurfaceInputPortId) -> Srgb8,
        O: Fn(OpacityInputId) -> crate::composition::AdmittedOpacityV1,
    {
        debug_assert_eq!(resolved_paints.len(), self.paints.len());
        debug_assert_eq!(resolved_surfaces.len(), self.surfaces.len());
        debug_assert_eq!(resolved_occurrences.len(), self.occurrences.len());

        for &index in self.paint_topo {
            let paint = match self.paints[index] {
                CompiledPaintSpec::Solid { id, color } => EncodedPointPaintV1::from_admitted(
                    id,
                    color_value(color),
                    crate::composition::AdmittedOpacityV1::OPAQUE,
                ),
                CompiledPaintSpec::Opacity {
                    id,
                    source,
                    opacity,
                } => {
                    // Валидированный `paint_topo` всегда материализует source
                    // раньше зависимого Opacity-узла.
                    let source = resolved_paints[source].unwrap_or_else(|| unreachable!());
                    EncodedPointPaintV1::from_admitted(
                        id,
                        source.source,
                        source.opacity.multiply(opacity_value(opacity)),
                    )
                }
            };
            resolved_paints[index] = Some(paint);
        }

        for node in self.render_topo {
            match *node {
                RenderNode::Surface(index) => {
                    let value = match self.surfaces[index] {
                        CompiledSurfaceSpec::Input { port, .. } => surface_value(port),
                        CompiledSurfaceSpec::FromOccurrence { occurrence, .. } => Srgb8::new(
                            resolved_occurrences[occurrence]
                                .unwrap_or_else(|| unreachable!())
                                .visible(),
                        ),
                    };
                    resolved_surfaces[index] = Some(value);
                }
                RenderNode::Occurrence(index) => {
                    let spec = &self.occurrences[index];
                    let subject = resolved_paints[spec.subject].unwrap_or_else(|| unreachable!());
                    let backdrop =
                        resolved_surfaces[spec.against].unwrap_or_else(|| unreachable!());
                    let visible = spec.profile.composite(
                        subject.source.bytes(),
                        subject.opacity,
                        backdrop.bytes(),
                    );
                    let certificate = SourceOverCertificateV1 {
                        profile: spec.profile,
                        subject_rgb: subject.source.bytes(),
                        subject_opacity: subject.opacity,
                        backdrop_rgb: backdrop.bytes(),
                        output_rgb: visible,
                    };
                    resolved_occurrences[index] = Some(ResolvedOccurrence {
                        id: spec.id,
                        subject: spec.subject_id,
                        against: spec.against_id,
                        backdrop: backdrop.bytes(),
                        visible,
                        certificate,
                    });
                }
            }
        }
    }
}
