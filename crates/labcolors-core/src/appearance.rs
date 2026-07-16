//! Приватный физический appearance-граф (#307): связный компонент
//! «input-слои → derived-поверхности → foreground occurrences».
//!
//! Граф владеет render-топологией, НЕ клиентским словарём: здесь нет имён
//! ролей, сентиментов и позиций лестницы — только непрозрачные typed handles и
//! физические байты. Любое поведение выводится из объявленных рёбер, никогда —
//! из значений ID (ID структурны и не участвуют в физике).
//!
//! Единственная физическая операция модуля — версионированный exact-композитор
//! [`crate::alpha::composite_over_srgb8`]
//! ([`CompositionProfileV1::EncodedSrgb8SourceOverV1`]). Второй композитор
//! запрещён: модуль связывает топологию с существующим SSOT, а не вводит новую
//! численную политику (ни одного нового production-числа, порога или epsilon).
//!
//! Жизненный цикл — две атомарные фазы:
//!
//! ```text
//! AppearanceGraphSpec::compile()   — валидация + канонизация + topo (без I/O)
//! CompiledAppearanceGraph::evaluate(bindings) — исполнение по topo, fail closed
//! ```
//!
//! Compile детерминирован и атомарен: при любой ошибке граф не публикуется
//! частично. Канонизация сортирует декларации по typed ID, поэтому физический
//! результат компонента не зависит от порядка деклараций при тех же
//! handles/рёбрах (инвариант закреплён тестом
//! `compile_is_independent_of_declaration_order_for_the_same_handles`).
//! Hash-map итерация как источник порядка не используется вовсе — все
//! коллекции здесь отсортированные `Vec`.

use std::collections::BTreeSet;

/// Непрозрачный handle цветового входа компонента. Значение — только
/// идентичность (структурная ссылка), не позиция и не приоритет.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ColorInputId(u32);

impl ColorInputId {
    /// Собрать handle из сырого значения клиента графа.
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Непрозрачный handle входа непрозрачности (straight alpha).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct OpacityInputId(u32);

impl OpacityInputId {
    /// Собрать handle из сырого значения клиента графа.
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Непрозрачный handle поверхности (rendered surface node).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SurfaceId(u32);

impl SurfaceId {
    /// Собрать handle из сырого значения клиента графа.
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Непрозрачный handle foreground occurrence (наблюдение foreground против
/// конкретной отрисованной поверхности).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct OccurrenceId(u32);

impl OccurrenceId {
    /// Собрать handle из сырого значения клиента графа.
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Версионированный профиль композиции ребра. Часть identity сертификата:
/// exact-утверждение живёт только внутри объявленного профиля, а не
/// «в браузерах вообще».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompositionProfileV1 {
    /// Straight-alpha source-over в encoded-sRGB8 байтовом домене Lab Colors:
    /// `bg + α·(src − bg)` на каждый канал, ОДНО финальное округление
    /// ([`crate::alpha::composite_over_srgb8`]). Reference-exact внутри этого
    /// профиля; универсальный browser/color-management pipeline не обещается.
    EncodedSrgb8SourceOverV1,
}

/// Декларация поверхности: input-слой (цвет из bindings как есть) либо
/// source-over композит поверх другой поверхности.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SurfaceSpec {
    /// Поверхность-вход: цвет берётся из bindings без преобразований.
    Input {
        /// Handle поверхности.
        id: SurfaceId,
        /// Цветовой вход, чьи байты становятся поверхностью.
        color: ColorInputId,
    },
    /// Derived-поверхность: `source` при `opacity` поверх `backdrop`.
    SourceOver {
        /// Handle поверхности.
        id: SurfaceId,
        /// Цветовой вход верхнего слоя.
        source: ColorInputId,
        /// Вход непрозрачности верхнего слоя.
        opacity: OpacityInputId,
        /// Поверхность-подложка (ребро зависимости графа).
        backdrop: SurfaceId,
        /// Версионированный профиль композиции.
        profile: CompositionProfileV1,
    },
}

impl SurfaceSpec {
    /// Handle поверхности — ключ канонизации и зависимости.
    fn id(&self) -> SurfaceId {
        match self {
            SurfaceSpec::Input { id, .. } | SurfaceSpec::SourceOver { id, .. } => *id,
        }
    }
}

/// Декларация foreground occurrence: identity-источник foreground наблюдается
/// против конкретной отрисованной поверхности. Именно топология (`against`)
/// задаёт роль «foreground/фон», а не имя токена.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ForegroundOccurrenceSpec {
    /// Handle occurrence.
    pub(crate) id: OccurrenceId,
    /// Цветовой вход — идентичность foreground (то, ЧТО наблюдается).
    pub(crate) identity_source: ColorInputId,
    /// Поверхность, против которой foreground реально стоит.
    pub(crate) against: SurfaceId,
}

/// Типизированные ошибки compile/evaluate. Публичный (в пределах crate) вход
/// не паникует: каждый отказ структурирован и различим без парсинга строк.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum GraphError {
    /// Один handle цветового входа объявлен дважды.
    DuplicateColorInput { input: ColorInputId },
    /// Один handle входа непрозрачности объявлен дважды.
    DuplicateOpacityInput { input: OpacityInputId },
    /// Один handle поверхности объявлен дважды.
    DuplicateSurface { surface: SurfaceId },
    /// Один handle occurrence объявлен дважды.
    DuplicateOccurrence { occurrence: OccurrenceId },
    /// Поверхность ссылается на необъявленный цветовой вход.
    MissingSurfaceColorInput {
        surface: SurfaceId,
        input: ColorInputId,
    },
    /// Поверхность ссылается на необъявленный вход непрозрачности.
    MissingSurfaceOpacityInput {
        surface: SurfaceId,
        input: OpacityInputId,
    },
    /// Композит ссылается на необъявленную поверхность-подложку.
    MissingSurfaceBackdrop {
        surface: SurfaceId,
        backdrop: SurfaceId,
    },
    /// Occurrence ссылается на необъявленный identity-источник.
    MissingOccurrenceSource {
        occurrence: OccurrenceId,
        input: ColorInputId,
    },
    /// Occurrence наблюдается против необъявленной поверхности.
    MissingOccurrenceBackdrop {
        occurrence: OccurrenceId,
        surface: SurfaceId,
    },
    /// Поверхности образуют цикл зависимостей: перечислены (в каноническом
    /// возрастающем порядке) все поверхности, не вошедшие в топологический
    /// порядок — участники циклов и их потомки.
    SurfaceCycle { surfaces: Vec<SurfaceId> },
    /// Один цветовой вход связан значением дважды.
    DuplicateColorBinding { input: ColorInputId },
    /// Один вход непрозрачности связан значением дважды.
    DuplicateOpacityBinding { input: OpacityInputId },
    /// Объявленный цветовой вход не получил значения.
    MissingColorBinding { input: ColorInputId },
    /// Объявленный вход непрозрачности не получил значения.
    MissingOpacityBinding { input: OpacityInputId },
    /// Значение подано для необъявленного цветового входа.
    UnexpectedColorBinding { input: ColorInputId },
    /// Значение подано для необъявленного входа непрозрачности.
    UnexpectedOpacityBinding { input: OpacityInputId },
    /// Непрозрачность вне конечного `[0,1]` (NaN/±∞/за границами). `message` —
    /// доменная ошибка SSOT-валидатора [`crate::alpha`] дословно, чтобы
    /// потребитель мог сохранить прежний публичный текст отказа байт-в-байт.
    OpacityOutOfDomain {
        input: OpacityInputId,
        message: String,
    },
    /// Композитор отверг вход. После валидации bindings недостижимо (байты
    /// корректны по типу, α проверена), но паника на публично достижимом пути
    /// запрещена — отказ остаётся типизированным.
    CompositionFailed { surface: SurfaceId, message: String },
}

/// Спека компонента до компиляции: плоские списки деклараций. Порядок
/// деклараций НЕ несёт смысла — compile канонизирует его по typed ID.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AppearanceGraphSpec {
    color_inputs: Vec<ColorInputId>,
    opacity_inputs: Vec<OpacityInputId>,
    surfaces: Vec<SurfaceSpec>,
    occurrences: Vec<ForegroundOccurrenceSpec>,
}

impl AppearanceGraphSpec {
    /// Собрать спеку из деклараций. Валидации здесь нет намеренно: единственная
    /// точка отказа — атомарный [`compile`](Self::compile).
    pub(crate) fn new(
        color_inputs: Vec<ColorInputId>,
        opacity_inputs: Vec<OpacityInputId>,
        surfaces: Vec<SurfaceSpec>,
        occurrences: Vec<ForegroundOccurrenceSpec>,
    ) -> Self {
        Self {
            color_inputs,
            opacity_inputs,
            surfaces,
            occurrences,
        }
    }

    /// Детерминированная атомарная компиляция: дубликаты → ссылки → циклы.
    ///
    /// Порядок проверок фиксирован и не зависит от порядка деклараций (все
    /// проверки идут по канонически отсортированным копиям): при нескольких
    /// дефектах сообщается дефект с наименьшим typed ID первого нарушенного
    /// класса. Частичный граф при ошибке не публикуется.
    ///
    /// # Errors
    ///
    /// Типизированный [`GraphError`] соответствующего класса.
    pub(crate) fn compile(&self) -> Result<CompiledAppearanceGraph, GraphError> {
        // Канонизация: сортировка по typed ID. Дубликаты после сортировки
        // смежны — детекция order-independent по построению.
        let mut color_inputs = self.color_inputs.clone();
        color_inputs.sort_unstable();
        if let Some(w) = color_inputs.windows(2).find(|w| w[0] == w[1]) {
            return Err(GraphError::DuplicateColorInput { input: w[0] });
        }

        let mut opacity_inputs = self.opacity_inputs.clone();
        opacity_inputs.sort_unstable();
        if let Some(w) = opacity_inputs.windows(2).find(|w| w[0] == w[1]) {
            return Err(GraphError::DuplicateOpacityInput { input: w[0] });
        }

        let mut surfaces = self.surfaces.clone();
        surfaces.sort_unstable_by_key(SurfaceSpec::id);
        if let Some(w) = surfaces.windows(2).find(|w| w[0].id() == w[1].id()) {
            return Err(GraphError::DuplicateSurface { surface: w[0].id() });
        }

        let mut occurrences = self.occurrences.clone();
        occurrences.sort_unstable_by_key(|o| o.id);
        if let Some(w) = occurrences.windows(2).find(|w| w[0].id == w[1].id) {
            return Err(GraphError::DuplicateOccurrence {
                occurrence: w[0].id,
            });
        }

        // Ссылочная целостность: каждое ребро указывает на объявленный узел.
        // Отсутствие ссылки — ошибка структуры, не runtime-вопрос.
        let has_color = |id: ColorInputId| color_inputs.binary_search(&id).is_ok();
        let has_opacity = |id: OpacityInputId| opacity_inputs.binary_search(&id).is_ok();
        let surface_index =
            |id: SurfaceId| surfaces.binary_search_by_key(&id, SurfaceSpec::id).ok();

        for spec in &surfaces {
            match *spec {
                SurfaceSpec::Input { id, color } => {
                    if !has_color(color) {
                        return Err(GraphError::MissingSurfaceColorInput {
                            surface: id,
                            input: color,
                        });
                    }
                }
                SurfaceSpec::SourceOver {
                    id,
                    source,
                    opacity,
                    backdrop,
                    profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
                } => {
                    if !has_color(source) {
                        return Err(GraphError::MissingSurfaceColorInput {
                            surface: id,
                            input: source,
                        });
                    }
                    if !has_opacity(opacity) {
                        return Err(GraphError::MissingSurfaceOpacityInput {
                            surface: id,
                            input: opacity,
                        });
                    }
                    if surface_index(backdrop).is_none() {
                        return Err(GraphError::MissingSurfaceBackdrop {
                            surface: id,
                            backdrop,
                        });
                    }
                }
            }
        }

        for occurrence in &occurrences {
            if !has_color(occurrence.identity_source) {
                return Err(GraphError::MissingOccurrenceSource {
                    occurrence: occurrence.id,
                    input: occurrence.identity_source,
                });
            }
            if surface_index(occurrence.against).is_none() {
                return Err(GraphError::MissingOccurrenceBackdrop {
                    occurrence: occurrence.id,
                    surface: occurrence.against,
                });
            }
        }

        // Топологический порядок (Кан) с готовым множеством в BTreeSet:
        // из готовых всегда берётся наименьший SurfaceId, поэтому порядок
        // канонический без какого-либо произвольного iteration limit —
        // алгоритм завершается на любом входе за |V| шагов.
        let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); surfaces.len()];
        let mut pending_deps: Vec<usize> = vec![0; surfaces.len()];
        for (index, spec) in surfaces.iter().enumerate() {
            if let SurfaceSpec::SourceOver { backdrop, .. } = spec {
                let backdrop_index = surface_index(*backdrop)
                    .unwrap_or_else(|| unreachable!("ссылки проверены выше"));
                dependents[backdrop_index].push(index);
                pending_deps[index] += 1;
            }
        }
        let mut ready: BTreeSet<SurfaceId> = surfaces
            .iter()
            .enumerate()
            .filter(|(index, _)| pending_deps[*index] == 0)
            .map(|(_, spec)| spec.id())
            .collect();
        let mut topo: Vec<usize> = Vec::with_capacity(surfaces.len());
        while let Some(id) = ready.pop_first() {
            let index =
                surface_index(id).unwrap_or_else(|| unreachable!("id из собственного множества"));
            topo.push(index);
            for &dependent in &dependents[index] {
                pending_deps[dependent] -= 1;
                if pending_deps[dependent] == 0 {
                    ready.insert(surfaces[dependent].id());
                }
            }
        }
        if topo.len() != surfaces.len() {
            let mut cycle: Vec<SurfaceId> = surfaces
                .iter()
                .enumerate()
                .filter(|(index, _)| pending_deps[*index] > 0)
                .map(|(_, spec)| spec.id())
                .collect();
            cycle.sort_unstable();
            return Err(GraphError::SurfaceCycle { surfaces: cycle });
        }

        Ok(CompiledAppearanceGraph {
            color_inputs,
            opacity_inputs,
            surfaces,
            occurrences,
            topo,
        })
    }
}

/// Скомпилированный граф: канонические декларации + детерминированный topo.
/// Равенство скомпилированных графов означает равную физику: любые два
/// объявления с теми же handles/рёбрами компилируются в идентичное значение.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledAppearanceGraph {
    /// Отсортированные объявленные цветовые входы.
    color_inputs: Vec<ColorInputId>,
    /// Отсортированные объявленные входы непрозрачности.
    opacity_inputs: Vec<OpacityInputId>,
    /// Поверхности в каноническом порядке (по id).
    surfaces: Vec<SurfaceSpec>,
    /// Occurrences в каноническом порядке (по id).
    occurrences: Vec<ForegroundOccurrenceSpec>,
    /// Индексы `surfaces` в порядке исполнения (канонический Кан).
    topo: Vec<usize>,
}

/// Значения входов на один evaluate: цвета — финальные sRGB8-байты, альфы —
/// binary64. Дубликаты/пропуски/лишние значения отвергает `evaluate`
/// (конструктор непадающий — единая точка отказа).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AppearanceBindings {
    colors: Vec<(ColorInputId, [u8; 3])>,
    opacities: Vec<(OpacityInputId, f64)>,
}

impl AppearanceBindings {
    /// Собрать значения входов. Валидация — в [`CompiledAppearanceGraph::evaluate`].
    pub(crate) fn new(
        colors: Vec<(ColorInputId, [u8; 3])>,
        opacities: Vec<(OpacityInputId, f64)>,
    ) -> Self {
        Self { colors, opacities }
    }
}

/// Счётчики фактически исполненных узлов — доказательство исполнения рёбер
/// (анти-вакуум: тест сверяет счётчики, а не только совпадение результата).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExecutionTrace {
    /// Исполненных input-поверхностей.
    pub(crate) input_surfaces: usize,
    /// Исполненных source-over рёбер.
    pub(crate) source_over_edges: usize,
    /// Собранных foreground occurrences.
    pub(crate) foreground_occurrences: usize,
}

/// Replayable-сертификат одной exact source-over операции: все входы и выход
/// в точных представлениях (байты и `to_bits` альфы). Не содержит и не может
/// содержать `Pass` про читаемость/восприятие — это сертификат композиции,
/// а не perception-утверждение.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceOverCertificateV1 {
    /// Identity версионированного профиля операции.
    pub(crate) profile: CompositionProfileV1,
    /// Поверхность, чей результат сертифицирован.
    pub(crate) surface: SurfaceId,
    /// Handle источника верхнего слоя.
    pub(crate) source_input: ColorInputId,
    /// Байты источника.
    pub(crate) source_rgb: [u8; 3],
    /// Handle поверхности-подложки.
    pub(crate) backdrop_surface: SurfaceId,
    /// Финальные байты подложки.
    pub(crate) backdrop_rgb: [u8; 3],
    /// Handle входа непрозрачности.
    pub(crate) opacity_input: OpacityInputId,
    /// Точные биты binary64-альфы (без потери представления).
    pub(crate) opacity_bits: u64,
    /// Финальные байты результата.
    pub(crate) output_rgb: [u8; 3],
}

impl SourceOverCertificateV1 {
    /// Независимо повторить операцию из данных сертификата.
    ///
    /// Часть proof-контракта модуля (§6.3 ТЗ #307): потребляется
    /// доказательствами (replay-тесты), production-путь один evaluate
    /// не дублирует — отсюда allow вне test-сборки.
    ///
    /// # Errors
    ///
    /// Доменная ошибка SSOT-композитора, если сертификат собран из
    /// невалидных данных (у честно выданного сертификата недостижимо).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn replay(&self) -> Result<[u8; 3], String> {
        match self.profile {
            CompositionProfileV1::EncodedSrgb8SourceOverV1 => crate::alpha::composite_over_srgb8(
                self.source_rgb,
                f64::from_bits(self.opacity_bits),
                self.backdrop_rgb,
            ),
        }
    }
}

/// Разрешённый foreground occurrence: identity-источник, его байты и финальные
/// байты поверхности, против которой foreground реально стоит.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolvedOccurrence {
    /// Handle occurrence.
    pub(crate) id: OccurrenceId,
    /// Объявленный identity-источник (ребро идентичности, не копия байт).
    pub(crate) identity_source: ColorInputId,
    /// Байты identity-источника из bindings.
    pub(crate) source: [u8; 3],
    /// Поверхность наблюдения (объявленная топологией).
    pub(crate) against: SurfaceId,
    /// Финальные вычисленные байты этой поверхности.
    pub(crate) backdrop: [u8; 3],
}

/// Результат одного evaluate: байты каждой поверхности, occurrences,
/// сертификаты exact-операций и счётчики исполнения. Все коллекции — в
/// каноническом порядке, поэтому результат сравним значением.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AppearanceEvaluation {
    /// `(поверхность, финальные байты)` в каноническом порядке (по id).
    surfaces: Vec<(SurfaceId, [u8; 3])>,
    /// Occurrences в каноническом порядке (по id).
    occurrences: Vec<ResolvedOccurrence>,
    /// Сертификаты source-over рёбер в порядке исполнения (канонический topo).
    certificates: Vec<SourceOverCertificateV1>,
    /// Счётчики фактического исполнения.
    trace: ExecutionTrace,
}

impl AppearanceEvaluation {
    /// Финальные байты поверхности, если она объявлена. Инспекционный API
    /// (проверяется доказательствами; production читает occurrence).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn surface_rgb(&self, id: SurfaceId) -> Option<[u8; 3]> {
        self.surfaces
            .binary_search_by_key(&id, |(surface, _)| *surface)
            .ok()
            .map(|index| self.surfaces[index].1)
    }

    /// Разрешённый occurrence, если он объявлен.
    pub(crate) fn occurrence(&self, id: OccurrenceId) -> Option<&ResolvedOccurrence> {
        self.occurrences
            .binary_search_by_key(&id, |occurrence| occurrence.id)
            .ok()
            .map(|index| &self.occurrences[index])
    }

    /// Сертификаты exact-операций этого evaluate (порядок исполнения).
    /// Proof-контракт (§6.3 ТЗ #307) — потребляется replay-доказательствами.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn certificates(&self) -> &[SourceOverCertificateV1] {
        &self.certificates
    }

    /// Счётчики фактического исполнения — анти-вакуумный proof-контракт.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn trace(&self) -> ExecutionTrace {
        self.trace
    }
}

impl CompiledAppearanceGraph {
    /// Исполнить граф на значениях входов: строго по compiled topo, только
    /// через SSOT-композитор, fail closed на любом дефекте bindings.
    ///
    /// Порядок валидации детерминирован: дубликаты → пропуски → лишние →
    /// домен α (везде наименьший typed ID первого нарушенного класса).
    ///
    /// # Errors
    ///
    /// Типизированный [`GraphError`]; частичный результат не публикуется.
    pub(crate) fn evaluate(
        &self,
        bindings: &AppearanceBindings,
    ) -> Result<AppearanceEvaluation, GraphError> {
        // Канонизация значений: сортировка по handle, смежные дубликаты.
        let mut colors = bindings.colors.clone();
        colors.sort_unstable_by_key(|(id, _)| *id);
        if let Some(w) = colors.windows(2).find(|w| w[0].0 == w[1].0) {
            return Err(GraphError::DuplicateColorBinding { input: w[0].0 });
        }
        let mut opacities = bindings.opacities.clone();
        opacities.sort_unstable_by_key(|(id, _)| *id);
        if let Some(w) = opacities.windows(2).find(|w| w[0].0 == w[1].0) {
            return Err(GraphError::DuplicateOpacityBinding { input: w[0].0 });
        }

        // Точное соответствие объявлениям: и пропуск, и лишнее значение —
        // отказ (молчаливые дефолты запрещены контрактом продукта).
        for declared in &self.color_inputs {
            if colors
                .binary_search_by_key(declared, |(id, _)| *id)
                .is_err()
            {
                return Err(GraphError::MissingColorBinding { input: *declared });
            }
        }
        for declared in &self.opacity_inputs {
            if opacities
                .binary_search_by_key(declared, |(id, _)| *id)
                .is_err()
            {
                return Err(GraphError::MissingOpacityBinding { input: *declared });
            }
        }
        for (bound, _) in &colors {
            if self.color_inputs.binary_search(bound).is_err() {
                return Err(GraphError::UnexpectedColorBinding { input: *bound });
            }
        }
        for (bound, _) in &opacities {
            if self.opacity_inputs.binary_search(bound).is_err() {
                return Err(GraphError::UnexpectedOpacityBinding { input: *bound });
            }
        }

        // Домен α — SSOT-валидатор композитора, чтобы текст доменного отказа
        // был единым во всём продукте (сообщение переносится дословно).
        for (input, alpha) in &opacities {
            if let Err(message) = crate::alpha::validate_alpha(*alpha) {
                return Err(GraphError::OpacityOutOfDomain {
                    input: *input,
                    message,
                });
            }
        }

        let color_value = |id: ColorInputId| -> [u8; 3] {
            let index = colors
                .binary_search_by_key(&id, |(bound, _)| *bound)
                .unwrap_or_else(|_| unreachable!("соответствие bindings проверено выше"));
            colors[index].1
        };
        let opacity_value = |id: OpacityInputId| -> f64 {
            let index = opacities
                .binary_search_by_key(&id, |(bound, _)| *bound)
                .unwrap_or_else(|_| unreachable!("соответствие bindings проверено выше"));
            opacities[index].1
        };

        // Исполнение строго по compiled topo: подложка каждого source-over
        // вычислена раньше по построению порядка.
        let mut resolved: Vec<Option<[u8; 3]>> = vec![None; self.surfaces.len()];
        let mut certificates: Vec<SourceOverCertificateV1> = Vec::new();
        let mut trace = ExecutionTrace {
            input_surfaces: 0,
            source_over_edges: 0,
            foreground_occurrences: 0,
        };
        for &index in &self.topo {
            match self.surfaces[index] {
                SurfaceSpec::Input { color, .. } => {
                    trace.input_surfaces += 1;
                    resolved[index] = Some(color_value(color));
                }
                SurfaceSpec::SourceOver {
                    id,
                    source,
                    opacity,
                    backdrop,
                    profile: profile @ CompositionProfileV1::EncodedSrgb8SourceOverV1,
                } => {
                    let backdrop_index = self
                        .surfaces
                        .binary_search_by_key(&backdrop, SurfaceSpec::id)
                        .unwrap_or_else(|_| unreachable!("ссылки проверены компиляцией"));
                    let backdrop_rgb = resolved[backdrop_index]
                        .unwrap_or_else(|| unreachable!("подложка раньше в topo по построению"));
                    let source_rgb = color_value(source);
                    let alpha = opacity_value(opacity);
                    // ЕДИНСТВЕННАЯ операция композиции модуля — SSOT-композитор.
                    let output_rgb =
                        crate::alpha::composite_over_srgb8(source_rgb, alpha, backdrop_rgb)
                            .map_err(|message| GraphError::CompositionFailed {
                                surface: id,
                                message,
                            })?;
                    trace.source_over_edges += 1;
                    certificates.push(SourceOverCertificateV1 {
                        profile,
                        surface: id,
                        source_input: source,
                        source_rgb,
                        backdrop_surface: backdrop,
                        backdrop_rgb,
                        opacity_input: opacity,
                        opacity_bits: alpha.to_bits(),
                        output_rgb,
                    });
                    resolved[index] = Some(output_rgb);
                }
            }
        }

        let surfaces: Vec<(SurfaceId, [u8; 3])> = self
            .surfaces
            .iter()
            .zip(&resolved)
            .map(|(spec, bytes)| {
                let bytes =
                    bytes.unwrap_or_else(|| unreachable!("topo покрывает каждую поверхность"));
                (spec.id(), bytes)
            })
            .collect();

        let occurrences: Vec<ResolvedOccurrence> = self
            .occurrences
            .iter()
            .map(|spec| {
                let backdrop_index = self
                    .surfaces
                    .binary_search_by_key(&spec.against, SurfaceSpec::id)
                    .unwrap_or_else(|_| unreachable!("ссылки проверены компиляцией"));
                let backdrop = resolved[backdrop_index]
                    .unwrap_or_else(|| unreachable!("topo покрывает каждую поверхность"));
                trace.foreground_occurrences += 1;
                ResolvedOccurrence {
                    id: spec.id,
                    identity_source: spec.identity_source,
                    source: color_value(spec.identity_source),
                    against: spec.against,
                    backdrop,
                }
            })
            .collect();

        Ok(AppearanceEvaluation {
            surfaces,
            occurrences,
            certificates,
            trace,
        })
    }
}
