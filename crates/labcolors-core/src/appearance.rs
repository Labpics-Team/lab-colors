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
//! статического IR результату compiler-а закреплено proof-тестом. Compiler входит
//! в production Core: любой внутренний lowerer собирает тот же нейтральный
//! Paint/Surface/Occurrence DAG, не добавляя в физику словарь клиента.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "production physical-graph compiler lands before its Core lowerer consumer"
    )
)]

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::Srgb8;
pub(crate) use crate::composition::CompositionProfileV1;

/// Непрозрачный handle атомарного Paint-входа. Число — только идентичность.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PaintInputId(u32);

impl PaintInputId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Непрозрачный handle наблюдаемого point-входа поверхности.
///
/// Он намеренно не взаимозаменяем с [`PaintInputId`]: authored Paint и
/// runtime backdrop имеют разные lifecycle и admission contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SurfaceInputPortId(u32);

impl SurfaceInputPortId {
    /// Construct one client-owned opaque surface-input identity.
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Exact transport value. It has identity semantics only.
    pub(crate) const fn value(self) -> u32 {
        self.0
    }
}

/// Непрозрачный handle входа straight alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct OpacityInputId(u32);

impl OpacityInputId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub(crate) const fn value(self) -> u32 {
        self.0
    }
}

/// Непрозрачный handle Paint-программы.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PaintId(u32);

impl PaintId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub(crate) const fn value(self) -> u32 {
        self.0
    }
}

/// Непрозрачный handle наблюдаемой поверхности.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SurfaceId(u32);

impl SurfaceId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub(crate) const fn value(self) -> u32 {
        self.0
    }
}

/// Непрозрачный handle применения Paint к Surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct OccurrenceId(u32);

impl OccurrenceId {
    /// Construct one client-owned opaque occurrence identity.
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Exact transport value. It has identity semantics only.
    pub(crate) const fn value(self) -> u32 {
        self.0
    }
}

/// Структурная identity статической физической программы. Она описывает
/// topology/opcode/profile, а не числовые handles декларации или client ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysicalProgramIdentityV1 {
    InputOpacityOverSurfaceEncodedSrgb8V1,
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
    pub(crate) const fn occurrence(self) -> OccurrenceId {
        self.occurrence
    }

    pub(crate) const fn subject(self) -> PaintId {
        self.subject
    }

    pub(crate) const fn backdrop_surface(self) -> SurfaceId {
        self.backdrop_surface
    }
}

/// Paint-конструкторы point-домена. Ни один вариант не знает Surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaintSpec {
    /// Атомарное encoded-sRGB8 + straight-alpha значение Paint-входа.
    Input { id: PaintId, input: PaintInputId },
    /// Модуляция straight alpha существующего Paint.
    ///
    /// Связанный scalar является множителем, а не абсолютным новым alpha.
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

impl PaintSpec {
    fn id(&self) -> PaintId {
        match self {
            Self::Input { id, .. } | Self::Opacity { id, .. } => *id,
        }
    }
}

/// Surface либо приходит извне как point-вход, либо является видимым
/// результатом объявленного occurrence.
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

impl SurfaceSpec {
    fn id(&self) -> SurfaceId {
        match self {
            Self::Input { id, .. } | Self::FromOccurrence { id, .. } => *id,
        }
    }
}

/// Единственная canonical application Paint к backdrop Surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OccurrenceSpec {
    pub(crate) id: OccurrenceId,
    pub(crate) subject: PaintId,
    pub(crate) against: SurfaceId,
    pub(crate) profile: CompositionProfileV1,
}

/// Ошибки атомарной AOT-компиляции физической декларации.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompileError {
    DuplicatePaintInput {
        input: PaintInputId,
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
    MissingPaintInput {
        paint: PaintId,
        input: PaintInputId,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BindingError {
    DuplicatePaintInputBinding {
        input: PaintInputId,
    },
    DuplicateOpacityBinding {
        input: OpacityInputId,
    },
    DuplicateSurfaceInputBinding {
        input: SurfaceInputPortId,
    },
    MissingPaintInputBinding {
        input: PaintInputId,
    },
    MissingOpacityBinding {
        input: OpacityInputId,
    },
    MissingSurfaceInputBinding {
        input: SurfaceInputPortId,
    },
    UnexpectedPaintInputBinding {
        input: PaintInputId,
    },
    UnexpectedOpacityBinding {
        input: OpacityInputId,
    },
    UnexpectedSurfaceInputBinding {
        input: SurfaceInputPortId,
    },
    OpacityOutOfDomain {
        input: OpacityInputId,
        reason: crate::composition::OpacityAdmissionErrorV1,
    },
    /// Bindings were admitted against a different exact typed input schema.
    IncompatibleAdmittedBindings,
    /// Scratch belongs to a different physical graph shape. Reusing storage is
    /// allowed only when every typed output domain has the same cardinality.
    IncompatibleWorkspace,
    /// A fallible allocation needed to prepare bindings, scratch or an owned
    /// result could not be satisfied. No numeric policy limit is implied.
    ResourceExhausted,
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
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AppearanceGraphSpec {
    paint_inputs: Vec<PaintInputId>,
    surface_input_ports: Vec<SurfaceInputPortId>,
    opacity_inputs: Vec<OpacityInputId>,
    paints: Vec<PaintSpec>,
    surfaces: Vec<SurfaceSpec>,
    occurrences: Vec<OccurrenceSpec>,
}

impl AppearanceGraphSpec {
    pub(crate) fn new(
        paint_inputs: Vec<PaintInputId>,
        surface_input_ports: Vec<SurfaceInputPortId>,
        opacity_inputs: Vec<OpacityInputId>,
        paints: Vec<PaintSpec>,
        surfaces: Vec<SurfaceSpec>,
        occurrences: Vec<OccurrenceSpec>,
    ) -> Self {
        Self {
            paint_inputs,
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
            mut paint_inputs,
            mut surface_input_ports,
            mut opacity_inputs,
            mut paints,
            mut surfaces,
            mut occurrences,
        } = self;

        paint_inputs.sort_unstable();
        if let Some(duplicate) = adjacent_duplicate(&paint_inputs) {
            return Err(CompileError::DuplicatePaintInput { input: duplicate });
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

        let has_paint_input = |id: PaintInputId| paint_inputs.binary_search(&id).is_ok();
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
                PaintSpec::Input { id, input } => {
                    if !has_paint_input(input) {
                        return Err(CompileError::MissingPaintInput { paint: id, input });
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
                PaintSpec::Input { .. } => None,
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
                PaintSpec::Input { id, input } => CompiledPaintSpec::Input { id, input },
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
            instance: Arc::new(()),
            paint_inputs,
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

fn adjacent_duplicate<T: Copy + Eq>(sorted: &[T]) -> Option<T> {
    sorted
        .windows(2)
        .find(|window| window[0] == window[1])
        .map(|window| window[0])
}

/// Topo для functional dependency graph: каждый узел имеет не более одной
/// зависимости. При цикле возвращает только его реальные узлы, а не весь
/// заблокированный Kahn-остаток.
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
    Input {
        id: PaintId,
        input: PaintInputId,
    },
    Opacity {
        id: PaintId,
        source: usize,
        opacity: OpacityInputId,
    },
}

impl CompiledPaintSpec {
    const fn id(&self) -> PaintId {
        match self {
            Self::Input { id, .. } | Self::Opacity { id, .. } => *id,
        }
    }
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

/// Cold-bound canonical Paint position for allocation-free repeated lookup.
///
/// Both fields are private to this module: callers can obtain a slot only from
/// a compiled graph and cannot forge a raw ordinal. The retained nominal ID is
/// checked again by every evaluation view before the ordinal is dereferenced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompiledPaintSlotV1 {
    index: usize,
    id: PaintId,
}

/// Cold-bound canonical Paint-input position used by finite Program targets.
/// The nominal ID is retained and rechecked on every overwrite. This rejects
/// stale or mismatched index/ID pairs, but does not claim graph-instance
/// identity when two graphs have the same canonical input at that position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompiledPaintInputSlotV1 {
    index: usize,
    id: PaintInputId,
}

/// Cold-bound canonical Occurrence position for allocation-free repeated
/// lookup. As with [`CompiledPaintSlotV1`], construction remains sealed inside
/// the compiled appearance graph and every use revalidates the exact ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompiledOccurrenceSlotV1 {
    index: usize,
    id: OccurrenceId,
}

/// Версионированное правило контрфактического отсутствия для одной
/// моделируемой целевой точки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointOccurrenceAbsenceReleaseV1 {
    /// Заменить результат целевого `Occurrence` его уже вычисленной подложкой,
    /// затем повторить все неизменённые downstream-`Occurrence` и квантование.
    BypassOwnBackdropV1,
}

/// Созданное компилятором полномочие терминального корня. Один `OccurrenceId`
/// не разрешает утверждать финальный моделируемый результат.
#[derive(Debug, Clone)]
pub(crate) struct CompiledPointPresentationRootV1 {
    graph_instance: Arc<()>,
    terminal: CompiledOccurrenceSlotV1,
}

impl CompiledPointPresentationRootV1 {
    pub(crate) const fn terminal(&self) -> OccurrenceId {
        self.terminal.id
    }
}

/// Однозначный путь предков от цели к корню, доказанный на холодной границе
/// компилятора.
#[derive(Debug, Clone)]
pub(crate) struct CompiledPointPresentationPathV1 {
    graph_instance: Arc<()>,
    target: OccurrenceId,
    root: OccurrenceId,
    occurrences: Box<[CompiledOccurrenceSlotV1]>,
}

impl CompiledPointPresentationPathV1 {
    pub(crate) const fn target(&self) -> OccurrenceId {
        self.target
    }

    pub(crate) const fn root(&self) -> OccurrenceId {
        self.root
    }

    pub(crate) const fn len(&self) -> usize {
        self.occurrences.len()
    }

    pub(crate) fn belongs_to(&self, graph: &CompiledAppearanceGraph) -> bool {
        Arc::ptr_eq(&self.graph_instance, &graph.instance)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointPresentationPathErrorV1 {
    MissingRoot,
    MissingTarget,
    RootConsumedDownstream,
    TargetOutsideRootAncestry,
    IncompatibleRoot,
    ResourceExhausted,
    InternalInvariant,
}

/// Типизированный отказ до начала точного контрфакта точки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointOccurrenceAbsenceReplayErrorV1 {
    /// В плоском буфере вызывающей стороны нет места для полного пересчёта.
    InsufficientCapacity,
    /// Результат вычисления и созданный компилятором путь принадлежат разным графам.
    IncompatibleEvaluation,
}

/// Канонический compiled IR с индексными ссылками: после проверки bindings
/// исполнение самих Paint/Surface/Occurrence узлов линейно по их числу.
///
/// Graph намеренно не реализует `Clone` и value equality: его instance-token
/// является compiler-owned authority, а не частью сравнимого содержимого.
#[derive(Debug)]
pub(crate) struct CompiledAppearanceGraph {
    instance: Arc<()>,
    paint_inputs: Vec<PaintInputId>,
    surface_input_ports: Vec<SurfaceInputPortId>,
    opacity_inputs: Vec<OpacityInputId>,
    paints: Vec<CompiledPaintSpec>,
    surfaces: Vec<CompiledSurfaceSpec>,
    occurrences: Vec<CompiledOccurrenceSpec>,
    paint_topo: Vec<usize>,
    render_topo: Vec<RenderNode>,
}

/// Borrowed runtime-представление уже проверенного compiled IR. Оно отделяет
/// исполнение от compiler-а, а статическим внутренним adapter-ам — исполнять
/// заранее доказанную топологию без повторной компиляции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompiledInputSchema<'a> {
    paint_inputs: &'a [PaintInputId],
    surface_input_ports: &'a [SurfaceInputPortId],
    opacity_inputs: &'a [OpacityInputId],
}

impl<'a> CompiledInputSchema<'a> {
    const fn new(
        paint_inputs: &'a [PaintInputId],
        surface_input_ports: &'a [SurfaceInputPortId],
        opacity_inputs: &'a [OpacityInputId],
    ) -> Self {
        Self {
            paint_inputs,
            surface_input_ports,
            opacity_inputs,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompiledAppearanceProgram<'a> {
    paint_inputs: &'a [PaintInputId],
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
            paint_inputs: inputs.paint_inputs,
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

const POINT_PAINT_INPUT: PaintInputId = PaintInputId::new(0);
const POINT_CONTEXT: SurfaceInputPortId = SurfaceInputPortId::new(0);
const POINT_OPACITY: OpacityInputId = OpacityInputId::new(0);
const POINT_SOLID_PAINT: PaintId = PaintId::new(0);
const POINT_OPACITY_PAINT: PaintId = PaintId::new(1);
const POINT_CONTEXT_SURFACE: SurfaceId = SurfaceId::new(0);
const POINT_DERIVED_SURFACE: SurfaceId = SurfaceId::new(1);
const POINT_OCCURRENCE: OccurrenceId = OccurrenceId::new(0);

const POINT_PAINT_INPUTS: [PaintInputId; 1] = [POINT_PAINT_INPUT];
const POINT_SURFACE_INPUT_PORTS: [SurfaceInputPortId; 1] = [POINT_CONTEXT];
const POINT_OPACITY_INPUTS: [OpacityInputId; 1] = [POINT_OPACITY];
const POINT_PAINTS: [CompiledPaintSpec; 2] = [
    CompiledPaintSpec::Input {
        id: POINT_SOLID_PAINT,
        input: POINT_PAINT_INPUT,
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
            &POINT_PAINT_INPUTS,
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
        PhysicalProgramIdentityV1::InputOpacityOverSurfaceEncodedSrgb8V1
    }

    pub(crate) const fn composition_profile() -> CompositionProfileV1 {
        CompositionProfileV1::EncodedSrgb8SourceOverV1
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
                debug_assert_eq!(id, POINT_PAINT_INPUT);
                EncodedPointPaintValueV1::opaque(Srgb8::new(source))
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
        POINT_PAINT_INPUTS.to_vec(),
        POINT_SURFACE_INPUT_PORTS.to_vec(),
        POINT_OPACITY_INPUTS.to_vec(),
        vec![
            PaintSpec::Input {
                id: POINT_SOLID_PAINT,
                input: POINT_PAINT_INPUT,
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
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AppearanceBindings {
    paint_inputs: Vec<(PaintInputId, EncodedPointPaintValueV1)>,
    surfaces: Vec<(SurfaceInputPortId, Srgb8)>,
    opacities: Vec<(OpacityInputId, f64)>,
}

impl AppearanceBindings {
    pub(crate) fn new(
        mut paint_inputs: Vec<(PaintInputId, EncodedPointPaintValueV1)>,
        mut surfaces: Vec<(SurfaceInputPortId, Srgb8)>,
        mut opacities: Vec<(OpacityInputId, f64)>,
    ) -> Self {
        paint_inputs.sort_unstable_by_key(|(id, _)| *id);
        surfaces.sort_unstable_by_key(|(id, _)| *id);
        opacities.sort_unstable_by_key(|(id, _)| *id);
        Self {
            paint_inputs,
            surfaces,
            opacities,
        }
    }
}

/// Один раз полностью проверенные runtime bindings в typed physical domain.
///
/// IDs остаются рядом со значениями: это позволяет fail-closed отвергнуть
/// случайное применение bindings к другому compiled input schema. Значения
/// alpha уже представлены [`crate::composition::AdmittedOpacityV1`], поэтому
/// steady-state evaluate не повторяет numeric admission.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AdmittedAppearanceBindings {
    paint_inputs: Vec<(PaintInputId, EncodedPointPaintValueV1)>,
    surfaces: Vec<(SurfaceInputPortId, Srgb8)>,
    opacities: Vec<(OpacityInputId, crate::composition::AdmittedOpacityV1)>,
}

impl AdmittedAppearanceBindings {
    /// Fallibly duplicate one fully admitted value for an independent Session.
    ///
    /// An ordinary [`Clone`] can abort the process on allocation failure. The
    /// runtime attachment boundary uses this method so resource exhaustion is
    /// returned before a partially prepared Session can escape.
    pub(crate) fn try_clone_v1(&self) -> Result<Self, BindingError> {
        fn copy_vec<T: Copy>(source: &[T]) -> Result<Vec<T>, BindingError> {
            let mut copied = Vec::new();
            copied
                .try_reserve_exact(source.len())
                .map_err(|_| BindingError::ResourceExhausted)?;
            copied.extend_from_slice(source);
            Ok(copied)
        }

        Ok(Self {
            paint_inputs: copy_vec(&self.paint_inputs)?,
            surfaces: copy_vec(&self.surfaces)?,
            opacities: copy_vec(&self.opacities)?,
        })
    }

    /// Overwrite the complete canonical Surface-input slice from one borrowed
    /// value source.
    ///
    /// The exact typed schema is checked in full before `value_at` can run or
    /// any admitted value can change. After that O(N) preflight, `value_at` is
    /// called exactly once per canonical input and every destination value is
    /// overwritten exactly once. Neither pass needs lookup or allocation.
    pub(crate) fn overwrite_surface_inputs_canonical(
        &mut self,
        expected_inputs: impl IntoIterator<Item = SurfaceInputPortId>,
        mut value_at: impl FnMut(usize) -> Srgb8,
    ) -> Result<(), BindingError> {
        if !expected_inputs
            .into_iter()
            .eq(self.surfaces.iter().map(|(input, _)| *input))
        {
            return Err(BindingError::IncompatibleAdmittedBindings);
        }

        for (index, (_, value)) in self.surfaces.iter_mut().enumerate() {
            *value = value_at(index);
        }
        Ok(())
    }

    /// Overwrite one prebound finite-target input without lookup or allocation.
    /// The canonical index and nominal ID must both match before mutation.
    pub(crate) fn overwrite_paint_input_at(
        &mut self,
        slot: CompiledPaintInputSlotV1,
        value: EncodedPointPaintValueV1,
    ) -> Result<(), BindingError> {
        let Some((bound, destination)) = self.paint_inputs.get_mut(slot.index) else {
            return Err(BindingError::IncompatibleAdmittedBindings);
        };
        if *bound != slot.id {
            return Err(BindingError::IncompatibleAdmittedBindings);
        }
        *destination = value;
        Ok(())
    }

    /// Borrow the admitted Surface-input slice in its exact canonical order.
    pub(crate) fn surface_inputs_canonical(
        &self,
    ) -> impl ExactSizeIterator<Item = (SurfaceInputPortId, Srgb8)> + '_ {
        self.surfaces.iter().copied()
    }

    #[cfg(test)]
    pub(crate) fn opacity_bits(&self, input: OpacityInputId) -> Option<u64> {
        self.opacities
            .binary_search_by_key(&input, |(bound, _)| *bound)
            .ok()
            .map(|index| self.opacities[index].1.bits())
    }

    fn matches_schema(&self, schema: CompiledInputSchema<'_>) -> bool {
        schema.paint_inputs.len() == self.paint_inputs.len()
            && schema.surface_input_ports.len() == self.surfaces.len()
            && schema.opacity_inputs.len() == self.opacities.len()
            && schema
                .paint_inputs
                .iter()
                .zip(&self.paint_inputs)
                .all(|(declared, (bound, _))| declared == bound)
            && schema
                .surface_input_ports
                .iter()
                .zip(&self.surfaces)
                .all(|(declared, (bound, _))| declared == bound)
            && schema
                .opacity_inputs
                .iter()
                .zip(&self.opacities)
                .all(|(declared, (bound, _))| declared == bound)
    }
}

/// ID-free атомарное значение encoded point Paint.
///
/// Источник и straight alpha изменяются только вместе: это не позволяет solver-у
/// создать не объявленную клиентом декартову комбинацию двух независимых осей.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EncodedPointPaintValueV1 {
    source: Srgb8,
    opacity: crate::composition::AdmittedOpacityV1,
}

impl EncodedPointPaintValueV1 {
    pub(crate) const fn from_admitted(
        source: Srgb8,
        opacity: crate::composition::AdmittedOpacityV1,
    ) -> Self {
        Self { source, opacity }
    }

    pub(crate) const fn opaque(source: Srgb8) -> Self {
        Self::from_admitted(source, crate::composition::AdmittedOpacityV1::OPAQUE)
    }

    pub(crate) const fn source(self) -> Srgb8 {
        self.source
    }

    pub(crate) const fn opacity(self) -> crate::composition::AdmittedOpacityV1 {
        self.opacity
    }

    pub(crate) const fn opacity_bits(self) -> u64 {
        self.opacity.bits()
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
    value: EncodedPointPaintValueV1,
}

impl EncodedPointPaintV1 {
    pub(crate) const fn from_value(id: PaintId, value: EncodedPointPaintValueV1) -> Self {
        Self { id, value }
    }

    pub(crate) const fn id(self) -> PaintId {
        self.id
    }

    pub(crate) const fn source(self) -> Srgb8 {
        self.value.source()
    }

    pub(crate) const fn opacity(self) -> crate::composition::AdmittedOpacityV1 {
        self.value.opacity()
    }

    pub(crate) const fn opacity_bits(self) -> u64 {
        self.value.opacity_bits()
    }

    pub(crate) const fn value(self) -> EncodedPointPaintValueV1 {
        self.value
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
    fn compose(
        profile: CompositionProfileV1,
        subject_rgb: [u8; 3],
        subject_opacity: crate::composition::AdmittedOpacityV1,
        backdrop_rgb: [u8; 3],
    ) -> Self {
        Self {
            profile,
            subject_rgb,
            subject_opacity,
            backdrop_rgb,
            output_rgb: profile.composite(subject_rgb, subject_opacity, backdrop_rgb),
        }
    }

    /// Replay the exact code-owned composition law certified by this value.
    #[cfg(test)]
    pub(crate) fn replay(&self) -> [u8; 3] {
        self.profile
            .composite(self.subject_rgb, self.subject_opacity, self.backdrop_rgb)
    }

    pub(crate) const fn profile(&self) -> CompositionProfileV1 {
        self.profile
    }

    pub(crate) const fn subject_rgb(&self) -> [u8; 3] {
        self.subject_rgb
    }

    pub(crate) const fn subject_opacity_bits(&self) -> u64 {
        self.subject_opacity.bits()
    }

    const fn subject_opacity(&self) -> crate::composition::AdmittedOpacityV1 {
        self.subject_opacity
    }

    pub(crate) const fn backdrop_rgb(&self) -> [u8; 3] {
        self.backdrop_rgb
    }

    pub(crate) const fn output_rgb(&self) -> [u8; 3] {
        self.output_rgb
    }
}

/// Точный финальный домен точки одного смоделированного `Occurrence` после
/// полного пересчёта оставшегося пути представления к корню.
///
/// `Empty` означает отсутствие вклада `Occurrence` в байты именно выбранного
/// корня этого пересчёта. Это ничего не утверждает о других терминальных корнях
/// и не является положительным свидетельством о восприятии или качестве.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactFinalOwnedPointDomainV1 {
    Empty,
    Singleton { visible: [u8; 3] },
}

impl ExactFinalOwnedPointDomainV1 {
    fn from_roots(normal: [u8; 3], counterfactual: [u8; 3]) -> Self {
        if normal == counterfactual {
            Self::Empty
        } else {
            Self::Singleton { visible: normal }
        }
    }
}

/// Один точный шаг версионированной интервенции отсутствия.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointOccurrenceAbsenceStepV1 {
    Removed {
        occurrence: OccurrenceId,
        normal: SourceOverCertificateV1,
    },
    Propagated {
        occurrence: OccurrenceId,
        normal: SourceOverCertificateV1,
        counterfactual: SourceOverCertificateV1,
    },
}

impl PointOccurrenceAbsenceStepV1 {
    pub(crate) const fn occurrence(self) -> OccurrenceId {
        match self {
            Self::Removed { occurrence, .. } | Self::Propagated { occurrence, .. } => occurrence,
        }
    }

    pub(crate) const fn normal(self) -> SourceOverCertificateV1 {
        match self {
            Self::Removed { normal, .. } | Self::Propagated { normal, .. } => normal,
        }
    }

    pub(crate) const fn counterfactual_output(self) -> [u8; 3] {
        match self {
            Self::Removed { normal, .. } => normal.backdrop_rgb(),
            Self::Propagated { counterfactual, .. } => counterfactual.output_rgb(),
        }
    }
}

/// Нейтральная сводка непустой истории пересчёта. В отличие от Replay она не
/// выдаёт право на происхождение шагов: authority остаётся у вычисления либо
/// у владеющего ими revision-bound отчёта.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointOccurrenceAbsenceSummaryV1 {
    first: PointOccurrenceAbsenceStepV1,
    last: PointOccurrenceAbsenceStepV1,
}

impl PointOccurrenceAbsenceSummaryV1 {
    pub(crate) fn from_nonempty_steps(steps: &[PointOccurrenceAbsenceStepV1]) -> Option<Self> {
        Some(Self {
            first: steps.first().copied()?,
            last: steps.last().copied()?,
        })
    }

    pub(crate) const fn target(self) -> OccurrenceId {
        self.first.occurrence()
    }

    pub(crate) const fn root(self) -> OccurrenceId {
        self.last.occurrence()
    }

    pub(crate) const fn normal_root(self) -> [u8; 3] {
        self.last.normal().output_rgb()
    }

    pub(crate) const fn counterfactual_root(self) -> [u8; 3] {
        self.last.counterfactual_output()
    }

    pub(crate) fn domain(self) -> ExactFinalOwnedPointDomainV1 {
        ExactFinalOwnedPointDomainV1::from_roots(self.normal_root(), self.counterfactual_root())
    }
}

/// Заимствованный результат одного точного контрфактического пересчёта.
///
/// Результат связывает версию интервенции и профили композиции шагов, но сам по
/// себе не связывает ревизию, идентичность `Program` или сценарий и потому не
/// является причинным сертификатом.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PointOccurrenceAbsenceReplayV1<'steps> {
    release: PointOccurrenceAbsenceReleaseV1,
    /// Срез непуст по построению: compiler-minted path всегда содержит хотя бы
    /// корень, а другого конструктора результата в модуле нет.
    steps: &'steps [PointOccurrenceAbsenceStepV1],
}

impl PointOccurrenceAbsenceReplayV1<'_> {
    fn summary(&self) -> PointOccurrenceAbsenceSummaryV1 {
        PointOccurrenceAbsenceSummaryV1::from_nonempty_steps(self.steps)
            .unwrap_or_else(|| unreachable!("созданный компилятором пересчёт непуст"))
    }

    pub(crate) fn target(&self) -> OccurrenceId {
        self.summary().target()
    }

    pub(crate) fn root(&self) -> OccurrenceId {
        self.summary().root()
    }

    pub(crate) const fn release(&self) -> PointOccurrenceAbsenceReleaseV1 {
        self.release
    }

    pub(crate) fn normal_root(&self) -> [u8; 3] {
        self.summary().normal_root()
    }

    pub(crate) fn counterfactual_root(&self) -> [u8; 3] {
        self.summary().counterfactual_root()
    }

    pub(crate) fn domain(&self) -> ExactFinalOwnedPointDomainV1 {
        self.summary().domain()
    }

    pub(crate) const fn steps(&self) -> &[PointOccurrenceAbsenceStepV1] {
        self.steps
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
    pub(crate) const fn program_occurrence(self) -> ProgramOccurrenceBindingV1 {
        self.program_occurrence
    }

    pub(crate) const fn occurrence(self) -> SourceOverCertificateV1 {
        self.occurrence
    }

    pub(crate) const fn occurrence_ref(&self) -> &SourceOverCertificateV1 {
        &self.occurrence
    }
}

/// Полный атомарный результат evaluate в каноническом typed-ID порядке.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppearanceEvaluation {
    paints: Vec<EncodedPointPaintV1>,
    surfaces: Vec<(SurfaceId, Srgb8)>,
    occurrences: Vec<ResolvedOccurrence>,
}

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

/// Reusable scratch для одного compiled physical graph.
///
/// После первого fallible sizing повторные evaluate того же shape только
/// очищают slots; capacity и backing allocations остаются неизменными.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AppearanceWorkspaceShape {
    paints: usize,
    surfaces: usize,
    occurrences: usize,
}

impl AppearanceWorkspaceShape {
    const fn of(program: CompiledAppearanceProgram<'_>) -> Self {
        Self {
            paints: program.paints.len(),
            surfaces: program.surfaces.len(),
            occurrences: program.occurrences.len(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct AppearanceWorkspace {
    shape: AppearanceWorkspaceShape,
    paints: Vec<Option<EncodedPointPaintV1>>,
    surfaces: Vec<Option<Srgb8>>,
    occurrences: Vec<Option<ResolvedOccurrence>>,
}

impl AppearanceWorkspace {
    fn for_program(program: CompiledAppearanceProgram<'_>) -> Result<Self, BindingError> {
        let shape = AppearanceWorkspaceShape::of(program);
        let mut workspace = Self {
            shape,
            paints: Vec::new(),
            surfaces: Vec::new(),
            occurrences: Vec::new(),
        };
        initialise_workspace_slots(&mut workspace.paints, shape.paints)?;
        initialise_workspace_slots(&mut workspace.surfaces, shape.surfaces)?;
        initialise_workspace_slots(&mut workspace.occurrences, shape.occurrences)?;
        Ok(workspace)
    }

    fn prepare(&mut self, program: CompiledAppearanceProgram<'_>) -> Result<(), BindingError> {
        if self.shape != AppearanceWorkspaceShape::of(program) {
            return Err(BindingError::IncompatibleWorkspace);
        }
        self.paints.fill(None);
        self.surfaces.fill(None);
        self.occurrences.fill(None);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn storage_signature(&self) -> [(usize, usize); 3] {
        [
            (self.paints.as_ptr() as usize, self.paints.capacity()),
            (self.surfaces.as_ptr() as usize, self.surfaces.capacity()),
            (
                self.occurrences.as_ptr() as usize,
                self.occurrences.capacity(),
            ),
        ]
    }
}

fn initialise_workspace_slots<T: Clone>(
    slots: &mut Vec<Option<T>>,
    required_len: usize,
) -> Result<(), BindingError> {
    slots
        .try_reserve_exact(required_len)
        .map_err(|_| BindingError::ResourceExhausted)?;
    slots.resize(required_len, None);
    Ok(())
}

/// Borrowed allocation-free result of one workspace evaluation.
///
/// The mutable workspace borrow prevents another evaluate from invalidating
/// these values while a consumer is still reading them.
#[derive(Debug)]
pub(crate) struct AppearanceEvaluationView<'program, 'workspace> {
    graph_instance: &'program Arc<()>,
    program: CompiledAppearanceProgram<'program>,
    workspace: &'workspace AppearanceWorkspace,
}

impl AppearanceEvaluationView<'_, '_> {
    pub(crate) fn paint(&self, id: PaintId) -> Option<&EncodedPointPaintV1> {
        let index = self
            .program
            .paints
            .binary_search_by_key(&id, CompiledPaintSpec::id)
            .ok()?;
        self.workspace.paints[index].as_ref()
    }

    /// Resolve a compiler-minted Paint slot in constant time.
    ///
    /// A slot from a graph whose canonical ordinal names another Paint is
    /// rejected before returning workspace data. No fallback ID lookup occurs.
    pub(crate) fn paint_at(&self, slot: CompiledPaintSlotV1) -> Option<&EncodedPointPaintV1> {
        let spec = self.program.paints.get(slot.index)?;
        if spec.id() != slot.id {
            return None;
        }
        let paint = self.workspace.paints.get(slot.index)?.as_ref()?;
        (paint.id == slot.id).then_some(paint)
    }

    pub(crate) fn surface_rgb(&self, id: SurfaceId) -> Option<[u8; 3]> {
        let index = self
            .program
            .surfaces
            .binary_search_by_key(&id, CompiledSurfaceSpec::id)
            .ok()?;
        self.workspace.surfaces[index].map(Srgb8::bytes)
    }

    pub(crate) fn occurrence(&self, id: OccurrenceId) -> Option<&ResolvedOccurrence> {
        let index = self
            .program
            .occurrences
            .binary_search_by_key(&id, |occurrence| occurrence.id)
            .ok()?;
        self.workspace.occurrences[index].as_ref()
    }

    /// Resolve a compiler-minted Occurrence slot in constant time, retaining
    /// exact nominal identity as the fail-closed cross-graph check.
    pub(crate) fn occurrence_at(
        &self,
        slot: CompiledOccurrenceSlotV1,
    ) -> Option<&ResolvedOccurrence> {
        let spec = self.program.occurrences.get(slot.index)?;
        if spec.id != slot.id {
            return None;
        }
        let occurrence = self.workspace.occurrences.get(slot.index)?.as_ref()?;
        (occurrence.id == slot.id).then_some(occurrence)
    }

    pub(crate) fn occurrences(&self) -> impl ExactSizeIterator<Item = &ResolvedOccurrence> + '_ {
        self.workspace.occurrences.iter().map(|occurrence| {
            occurrence
                .as_ref()
                .unwrap_or_else(|| unreachable!("render topo covers every Occurrence"))
        })
    }

    /// Пересчитывает доказанную компилятором цепочку точки по версионированному
    /// правилу отсутствия без аллокации и частичного результата.
    ///
    /// Новые шаги дописываются в конец `steps`, а результат заимствует только
    /// добавленный диапазон. При переиспользовании scratch вызывающая сторона
    /// очищает или укорачивает его; недостаточная свободная ёмкость отвергается
    /// до композиции и до изменения буфера.
    pub(crate) fn replay_point_occurrence_absence_into<'steps>(
        &self,
        path: &CompiledPointPresentationPathV1,
        release: PointOccurrenceAbsenceReleaseV1,
        steps: &'steps mut Vec<PointOccurrenceAbsenceStepV1>,
    ) -> Result<PointOccurrenceAbsenceReplayV1<'steps>, PointOccurrenceAbsenceReplayErrorV1> {
        if !Arc::ptr_eq(self.graph_instance, &path.graph_instance) {
            return Err(PointOccurrenceAbsenceReplayErrorV1::IncompatibleEvaluation);
        }
        // Новый вариант release обязан сломать компиляцию здесь, а не молча
        // получить семантику единственного текущего правила.
        let PointOccurrenceAbsenceReleaseV1::BypassOwnBackdropV1 = release;
        if steps.capacity().saturating_sub(steps.len()) < path.occurrences.len() {
            return Err(PointOccurrenceAbsenceReplayErrorV1::InsufficientCapacity);
        }
        let start = steps.len();
        let mut counterfactual_previous = None;
        for (path_index, slot) in path.occurrences.iter().copied().enumerate() {
            let occurrence = self.occurrence_at(slot).unwrap_or_else(|| {
                unreachable!("путь того же графа содержит канонические позиции")
            });
            let normal = *occurrence.certificate();
            let counterfactual_output = if path_index == 0 {
                let absent_output = normal.backdrop_rgb();
                steps.push(PointOccurrenceAbsenceStepV1::Removed {
                    occurrence: slot.id,
                    normal,
                });
                absent_output
            } else {
                let counterfactual_backdrop = counterfactual_previous
                    .unwrap_or_else(|| unreachable!("каждый пересчёт начинается с удалённой цели"));
                let counterfactual = SourceOverCertificateV1::compose(
                    normal.profile(),
                    normal.subject_rgb(),
                    normal.subject_opacity(),
                    counterfactual_backdrop,
                );
                steps.push(PointOccurrenceAbsenceStepV1::Propagated {
                    occurrence: slot.id,
                    normal,
                    counterfactual,
                });
                counterfactual.output_rgb()
            };
            counterfactual_previous = Some(counterfactual_output);
        }
        let end = steps.len();
        Ok(PointOccurrenceAbsenceReplayV1 {
            release,
            steps: &steps[start..end],
        })
    }

    fn try_to_owned(&self) -> Result<AppearanceEvaluation, BindingError> {
        let mut paints = Vec::new();
        paints
            .try_reserve_exact(self.workspace.paints.len())
            .map_err(|_| BindingError::ResourceExhausted)?;
        for paint in &self.workspace.paints {
            paints.push(paint.unwrap_or_else(|| unreachable!("Paint topo covers every node")));
        }

        let mut surfaces = Vec::new();
        surfaces
            .try_reserve_exact(self.workspace.surfaces.len())
            .map_err(|_| BindingError::ResourceExhausted)?;
        for (spec, value) in self.program.surfaces.iter().zip(&self.workspace.surfaces) {
            surfaces.push((
                spec.id(),
                value.unwrap_or_else(|| unreachable!("render topo covers every Surface")),
            ));
        }

        let mut occurrences = Vec::new();
        occurrences
            .try_reserve_exact(self.workspace.occurrences.len())
            .map_err(|_| BindingError::ResourceExhausted)?;
        for occurrence in &self.workspace.occurrences {
            occurrences.push(
                occurrence.unwrap_or_else(|| unreachable!("render topo covers every Occurrence")),
            );
        }

        Ok(AppearanceEvaluation {
            paints,
            surfaces,
            occurrences,
        })
    }
}

impl CompiledAppearanceGraph {
    fn program(&self) -> CompiledAppearanceProgram<'_> {
        CompiledAppearanceProgram::from_validated_parts(
            CompiledInputSchema::new(
                &self.paint_inputs,
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

    #[cfg(test)]
    fn matches_program(&self, program: CompiledAppearanceProgram<'_>) -> bool {
        self.program() == program
    }

    /// Bind one Paint input to its canonical compiled ordinal. Runtime target
    /// selection consumes the sealed slot instead of repeating an ID search.
    pub(crate) fn bind_paint_input(&self, id: PaintInputId) -> Option<CompiledPaintInputSlotV1> {
        let index = self.paint_inputs.binary_search(&id).ok()?;
        Some(CompiledPaintInputSlotV1 { index, id })
    }

    /// Bind one Paint identity to its canonical compiled ordinal.
    ///
    /// This is the only cold lookup. Repeated evaluations consume the sealed
    /// slot through [`AppearanceEvaluationView::paint_at`] without searching.
    pub(crate) fn bind_paint(&self, id: PaintId) -> Option<CompiledPaintSlotV1> {
        let index = self
            .paints
            .binary_search_by_key(&id, CompiledPaintSpec::id)
            .ok()?;
        Some(CompiledPaintSlotV1 { index, id })
    }

    /// Bind one Occurrence identity to its canonical compiled ordinal.
    pub(crate) fn bind_occurrence(&self, id: OccurrenceId) -> Option<CompiledOccurrenceSlotV1> {
        let index = self
            .occurrences
            .binary_search_by_key(&id, |occurrence| occurrence.id)
            .ok()?;
        Some(CompiledOccurrenceSlotV1 { index, id })
    }

    /// Находит объявленный subject Paint одного канонического Occurrence.
    ///
    /// Cold compiler lookup не читает вычисленные значения и не создаёт
    /// runtime-состояние. Hot path сохраняет полученный Paint ID в закрытом
    /// compiled binding вместо повторного поиска.
    pub(crate) fn occurrence_subject(&self, id: OccurrenceId) -> Option<PaintId> {
        let index = self
            .occurrences
            .binary_search_by_key(&id, |occurrence| occurrence.id)
            .ok()?;
        Some(self.occurrences[index].subject_id)
    }

    /// Создаёт полномочие только для `Occurrence`, который не потребляется
    /// другим `Occurrence` этого точечного графа: промежуточный слой нельзя
    /// принять за финальный моделируемый результат.
    pub(crate) fn compile_point_presentation_root(
        &self,
        terminal: OccurrenceId,
    ) -> Result<CompiledPointPresentationRootV1, PointPresentationPathErrorV1> {
        let terminal = self
            .bind_occurrence(terminal)
            .ok_or(PointPresentationPathErrorV1::MissingRoot)?;
        let consumed = self.occurrences.iter().any(|occurrence| {
            matches!(
                self.surfaces.get(occurrence.against),
                Some(CompiledSurfaceSpec::FromOccurrence {
                    occurrence: source_occurrence,
                    ..
                }) if *source_occurrence == terminal.index
            )
        });
        if consumed {
            return Err(PointPresentationPathErrorV1::RootConsumedDownstream);
        }
        Ok(CompiledPointPresentationRootV1 {
            graph_instance: Arc::clone(&self.instance),
            terminal,
        })
    }

    /// Доказывает принадлежность цели однозначной цепочке предков подложки
    /// выбранного корня.
    pub(crate) fn compile_point_presentation_path(
        &self,
        target: OccurrenceId,
        root: &CompiledPointPresentationRootV1,
    ) -> Result<CompiledPointPresentationPathV1, PointPresentationPathErrorV1> {
        if !Arc::ptr_eq(&self.instance, &root.graph_instance) {
            return Err(PointPresentationPathErrorV1::IncompatibleRoot);
        }
        let target_slot = self
            .bind_occurrence(target)
            .ok_or(PointPresentationPathErrorV1::MissingTarget)?;
        let root_slot = root.terminal;
        let mut reverse = Vec::new();
        reverse
            .try_reserve_exact(self.occurrences.len())
            .map_err(|_| PointPresentationPathErrorV1::ResourceExhausted)?;

        // `compile()` отклоняет `RenderCycle` при построении канонической
        // функциональной топологии. Каждый переход идёт к единственному предку,
        // поэтому обход завершается не более чем за `occurrences.len()` узлов;
        // та же граница позволяет безопасно зарезервировать точный объём.
        let mut current = root_slot.index;
        loop {
            let spec = self
                .occurrences
                .get(current)
                .ok_or(PointPresentationPathErrorV1::InternalInvariant)?;
            reverse.push(CompiledOccurrenceSlotV1 {
                index: current,
                id: spec.id,
            });
            if current == target_slot.index {
                break;
            }
            let surface = self
                .surfaces
                .get(spec.against)
                .ok_or(PointPresentationPathErrorV1::InternalInvariant)?;
            current = match surface {
                CompiledSurfaceSpec::FromOccurrence { occurrence, .. } => *occurrence,
                CompiledSurfaceSpec::Input { .. } => {
                    return Err(PointPresentationPathErrorV1::TargetOutsideRootAncestry);
                }
            };
        }
        reverse.reverse();
        Ok(CompiledPointPresentationPathV1 {
            graph_instance: Arc::clone(&self.instance),
            target,
            root: root_slot.id,
            occurrences: reverse.into_boxed_slice(),
        })
    }

    /// Canonical client-owned occurrence identities emitted by this program.
    pub(crate) fn occurrence_ids(&self) -> impl ExactSizeIterator<Item = OccurrenceId> + '_ {
        self.occurrences.iter().map(|occurrence| occurrence.id)
    }

    /// Canonical physical Surface-input schema accepted by this program.
    pub(crate) fn surface_input_ports(
        &self,
    ) -> impl ExactSizeIterator<Item = SurfaceInputPortId> + '_ {
        self.surface_input_ports.iter().copied()
    }

    /// Проверить полный typed schema и один раз понизить authored alpha в
    /// admitted physical values. Результат можно клонировать для независимых
    /// runtime callers без повторной numeric admission.
    pub(crate) fn admit_bindings(
        &self,
        bindings: &AppearanceBindings,
    ) -> Result<AdmittedAppearanceBindings, BindingError> {
        self.program().admit_bindings(bindings)
    }

    /// Fallible one-time allocation of scratch for this exact physical shape.
    pub(crate) fn new_workspace(&self) -> Result<AppearanceWorkspace, BindingError> {
        AppearanceWorkspace::for_program(self.program())
    }

    /// Allocation-free steady-state execution over already admitted bindings.
    pub(crate) fn evaluate_admitted_into<'workspace>(
        &self,
        bindings: &AdmittedAppearanceBindings,
        workspace: &'workspace mut AppearanceWorkspace,
    ) -> Result<AppearanceEvaluationView<'_, 'workspace>, BindingError> {
        self.program()
            .evaluate_admitted_into(&self.instance, bindings, workspace)
    }

    /// Cold convenience внутри Core для callers, которым нужен owned result.
    /// Hot Session обязан хранить admitted bindings и workspace между вызовами.
    pub(crate) fn evaluate(
        &self,
        bindings: &AppearanceBindings,
    ) -> Result<AppearanceEvaluation, BindingError> {
        let admitted = self.admit_bindings(bindings)?;
        let mut workspace = self.new_workspace()?;
        self.evaluate_admitted_into(&admitted, &mut workspace)?
            .try_to_owned()
    }
}

impl<'program> CompiledAppearanceProgram<'program> {
    const fn input_schema(self) -> CompiledInputSchema<'program> {
        CompiledInputSchema::new(
            self.paint_inputs,
            self.surface_input_ports,
            self.opacity_inputs,
        )
    }

    fn admit_bindings(
        &self,
        bindings: &AppearanceBindings,
    ) -> Result<AdmittedAppearanceBindings, BindingError> {
        let paint_inputs = &bindings.paint_inputs;
        if let Some(window) = paint_inputs
            .windows(2)
            .find(|window| window[0].0 == window[1].0)
        {
            return Err(BindingError::DuplicatePaintInputBinding { input: window[0].0 });
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

        for declared in self.paint_inputs {
            if paint_inputs
                .binary_search_by_key(declared, |(id, _)| *id)
                .is_err()
            {
                return Err(BindingError::MissingPaintInputBinding { input: *declared });
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
        for (bound, _) in paint_inputs {
            if self.paint_inputs.binary_search(bound).is_err() {
                return Err(BindingError::UnexpectedPaintInputBinding { input: *bound });
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

        let mut admitted_paint_inputs = Vec::new();
        admitted_paint_inputs
            .try_reserve_exact(paint_inputs.len())
            .map_err(|_| BindingError::ResourceExhausted)?;
        admitted_paint_inputs.extend(paint_inputs.iter().copied());

        let mut admitted_surfaces = Vec::new();
        admitted_surfaces
            .try_reserve_exact(surfaces.len())
            .map_err(|_| BindingError::ResourceExhausted)?;
        admitted_surfaces.extend(surfaces.iter().copied());

        let mut admitted_opacities = Vec::new();
        admitted_opacities
            .try_reserve_exact(opacities.len())
            .map_err(|_| BindingError::ResourceExhausted)?;
        for (input, alpha) in opacities {
            let value = crate::composition::AdmittedOpacityV1::new(*alpha).map_err(|reason| {
                BindingError::OpacityOutOfDomain {
                    input: *input,
                    reason,
                }
            })?;
            admitted_opacities.push((*input, value));
        }

        Ok(AdmittedAppearanceBindings {
            paint_inputs: admitted_paint_inputs,
            surfaces: admitted_surfaces,
            opacities: admitted_opacities,
        })
    }

    fn evaluate_admitted_into<'workspace>(
        self,
        graph_instance: &'program Arc<()>,
        bindings: &AdmittedAppearanceBindings,
        workspace: &'workspace mut AppearanceWorkspace,
    ) -> Result<AppearanceEvaluationView<'program, 'workspace>, BindingError> {
        if !bindings.matches_schema(self.input_schema()) {
            return Err(BindingError::IncompatibleAdmittedBindings);
        }
        workspace.prepare(self)?;

        let paint_inputs = &bindings.paint_inputs;
        let surfaces = &bindings.surfaces;
        let opacities = &bindings.opacities;

        let paint_input_value = |id: PaintInputId| -> EncodedPointPaintValueV1 {
            let index = paint_inputs
                .binary_search_by_key(&id, |(bound, _)| *bound)
                .unwrap_or_else(|_| unreachable!("bindings were matched before evaluation"));
            paint_inputs[index].1
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
            opacities[index].1
        };

        self.execute_into(
            paint_input_value,
            surface_value,
            opacity_value,
            &mut workspace.paints,
            &mut workspace.surfaces,
            &mut workspace.occurrences,
        );

        Ok(AppearanceEvaluationView {
            graph_instance,
            program: self,
            workspace,
        })
    }

    /// Единственное исполнение compiled IR. Scratch принадлежит caller-у:
    /// static adapter использует stack arrays, generic admission —
    /// владеющие buffers. Алгоритм и сертификат при этом общие.
    fn execute_into<P, S, O>(
        &self,
        paint_input_value: P,
        surface_value: S,
        opacity_value: O,
        resolved_paints: &mut [Option<EncodedPointPaintV1>],
        resolved_surfaces: &mut [Option<Srgb8>],
        resolved_occurrences: &mut [Option<ResolvedOccurrence>],
    ) where
        P: Fn(PaintInputId) -> EncodedPointPaintValueV1,
        S: Fn(SurfaceInputPortId) -> Srgb8,
        O: Fn(OpacityInputId) -> crate::composition::AdmittedOpacityV1,
    {
        debug_assert_eq!(resolved_paints.len(), self.paints.len());
        debug_assert_eq!(resolved_surfaces.len(), self.surfaces.len());
        debug_assert_eq!(resolved_occurrences.len(), self.occurrences.len());

        for &index in self.paint_topo {
            let paint = match self.paints[index] {
                CompiledPaintSpec::Input { id, input } => {
                    EncodedPointPaintV1::from_value(id, paint_input_value(input))
                }
                CompiledPaintSpec::Opacity {
                    id,
                    source,
                    opacity,
                } => {
                    // Валидированный `paint_topo` всегда материализует source
                    // раньше зависимого Opacity-узла.
                    let source = resolved_paints[source].unwrap_or_else(|| unreachable!());
                    let value = source.value();
                    EncodedPointPaintV1::from_value(
                        id,
                        EncodedPointPaintValueV1::from_admitted(
                            value.source(),
                            value.opacity().multiply(opacity_value(opacity)),
                        ),
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
                    let certificate = SourceOverCertificateV1::compose(
                        spec.profile,
                        subject.source().bytes(),
                        subject.opacity(),
                        backdrop.bytes(),
                    );
                    let visible = certificate.output_rgb();
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
