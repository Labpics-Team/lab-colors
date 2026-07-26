//! Публичный декларативный контракт компиляции и исполнения цветовой программы.
//!
//! Клиент один раз описывает физический граф через [`DraftV1`]: исходные
//! сигналы, решаемые цели, Paint, Surface, их [`OccurrenceIdV1`], ограничения
//! и выходы. Идентификаторы непрозрачны: Core не выводит из них семантику.
//! [`DraftV1::compile`] проверяет граф целиком и возвращает [`OwnerV1`] —
//! единственного владельца конкретной скомпилированной эпохи.
//!
//! [`OwnerV1::instantiate`] создаёт потоковую [`SessionV1`]. На горячем пути
//! [`OwnerV1::update`] принимает физические сценарии в каноническом порядке
//! входов и атомарно возвращает [`ProjectionV1`] без повторного решения в
//! адаптере. Исторические доказательства принадлежат Session, но только тот же
//! Owner разрешает обновления и операции.
//!
//! Состояния проецируются однозначно:
//!
//! | Состояние | Операции |
//! |---|---|
//! | `Waiting` + `Empty` | нет |
//! | `Waiting` + допущенный `Unknown` | `Remove` для каждого выхода |
//! | `Ready` | `Set` для каждого выхода |
//! | `Stale` | `Remove` для каждого выхода |
//! | `Failed` | `Remove` для каждого выхода |
//!
//! Прошлый Verified-сертификат остаётся в evidence для диагностики, но не
//! разрешает эмиссию: он относится к прошлому наблюдению, а не к текущему
//! неизвестному или нарушающему контексту. Непустая сырая голова без текущего
//! Verified-сертификата также отзывает выходы: это закрывает передачу sink от
//! одной Session другой Session того же Owner.
//!
//! [`CertificateV1::Verified`] хранит выбранное состояние, все клетки
//! доказательства и сертифицированные выходы. [`CertificateV1::Conflict`]
//! хранит исчерпывающий конфликт по всем рассмотренным состояниям.
//! [`ContentIdentityV1`] идентифицирует каноническое содержание, но не даёт
//! полномочий живого [`OwnerV1`].

use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::slice;

use crate::Srgb8;
use crate::appearance::{OccurrenceId, OpacityInputId, PaintId, SurfaceId, SurfaceInputPortId};
use crate::composition::CompositionProfileV1;
use crate::constraints::{
    ApplicableWcag22EvaluationErrorV1, ExactSrgb8IdentityV1, ProgramVisiblePointBindingV1,
    ProgramVisiblePointPassEvidence, ProgramVisiblePointViolationEvidence, Wcag22Srgb8V1,
};
use crate::joint::FiniteJointOrderErrorV1;
use crate::lcs_occurrence::{
    AdaptingLuminanceCdM2, AdmittedSrgb8TristimulusBindingV1, AppearanceContextDomainErrorV1,
    AppearanceContextFieldV1 as CoreAppearanceContextFieldV1, AppearanceContextId,
    AppearanceContextSchemaReleaseId, BackgroundLuminanceRatio, ColorSignal, ColorSignalViewV1,
    ColorimetricFrameId, ColorimetricFrameReleaseId, IEC_SRGB_D65_XYZ_FRAME_V1,
    ModeledLcsOccurrenceFormationErrorV1, NumericDomainError, ObserverProfileId,
    OccurrenceFormationError, ReferenceWhiteId, SurroundProfileId,
    TristimulusComponentV1 as CoreTristimulusComponentV1, TristimulusDomainErrorV1,
    TristimulusSample, TristimulusScale,
};
use crate::numerics::NumericalDecisionEvidenceV1;
use crate::observation::{
    ObservationError, ObservationHeadViewV1, ObservationSchemaMismatchV1, ObservationStreamId,
    Revision, ScenarioId, SchemaOrderedScenarioSourceV1, UnknownReasonId,
};
use crate::program_session::{
    CompiledCoreProgramV1, CompositionProfile, ConstraintId, ConstraintInvocation,
    CoreProgramConstraintInvocationV1, CoreProgramDraftErrorV1, CoreProgramDraftV1,
    CoreProgramEvaluatorErrorV1, CoreProgramEvaluatorsV1, CoreProgramPassEvidenceV1,
    CoreProgramViolationEvidenceV1, DeclaredJointSelectionV1, JointCandidateStateV1, Occurrence,
    OpacityInput, OutputBinding, OutputSlotId, Paint, ProgramCompileError, ProgramConflictV1,
    ProgramConstraintCellV1, ProgramConstraintResultV1, ProgramContentIdentityV1, ProgramOutputV1,
    ProgramSessionEvaluationError, ProgramSessionInstantiateError, ProgramSessionPlan,
    ProgramVerifiedV1, Source, SourceId, Surface, Target, TargetCandidateChoiceV1,
    TargetCandidateId, TargetCandidateV1 as CoreTargetCandidateV1, TargetId,
};
use crate::session::{Session, SessionState, SessionUpdateError};
use crate::wcag22::{
    Wcag22ClientDeclaredNotApplicableV1, Wcag22CriterionV1, Wcag22EvaluationErrorV1,
    Wcag22LuminanceBoundsQ55V1, Wcag22ProfileIdV1, wcag22_profile_v1,
};

type CoreVerifiedV1 = ProgramVerifiedV1<CoreProgramEvaluatorsV1>;
type CoreConflictV1 = ProgramConflictV1<CoreProgramEvaluatorsV1>;
type CoreProgramPlanV1 = ProgramSessionPlan<CoreProgramEvaluatorsV1>;
type CoreProgramSessionV1 = Session<CoreProgramPlanV1>;
type CoreProgramStateV1 = SessionState<CoreVerifiedV1, CoreConflictV1>;
type CoreProgramPlanErrorV1 = ProgramSessionEvaluationError<CoreProgramEvaluatorErrorV1>;
type CoreProgramConstraintCellV1 = ProgramConstraintCellV1<CoreProgramEvaluatorsV1>;
type CoreExactPassEvidenceV1 = ProgramVisiblePointPassEvidence<ExactSrgb8IdentityV1>;
type CoreExactViolationEvidenceV1 = ProgramVisiblePointViolationEvidence<ExactSrgb8IdentityV1>;
type CoreWcag22PassEvidenceV1 = ProgramVisiblePointPassEvidence<Wcag22Srgb8V1>;
type CoreWcag22ViolationEvidenceV1 = ProgramVisiblePointViolationEvidence<Wcag22Srgb8V1>;

macro_rules! authored_id {
    ($doc:literal, $name:ident, $core:ty) => {
        #[doc = $doc]
        #[repr(transparent)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        #[must_use]
        pub struct $name($core);

        impl $name {
            /// Создаёт непрозрачный идентификатор из клиентского числового ключа.
            pub const fn new(value: u32) -> Self {
                Self(<$core>::new(value))
            }

            /// Возвращает исходный клиентский числовой ключ.
            pub const fn value(self) -> u32 {
                self.0.value()
            }

            const fn from_core(value: $core) -> Self {
                Self(value)
            }

            const fn into_core(self) -> $core {
                self.0
            }
        }

        impl core::hash::Hash for $name {
            fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                core::hash::Hash::hash(&self.value(), state);
            }
        }
    };
}

macro_rules! projected_id {
    ($doc:literal, $name:ident, $core:ty) => {
        #[doc = $doc]
        #[repr(transparent)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        #[must_use]
        pub struct $name($core);

        impl $name {
            const fn from_core(value: $core) -> Self {
                Self(value)
            }

            /// Возвращает числовой ключ сохранённой provenance.
            pub const fn value(self) -> u32 {
                self.0.value()
            }
        }

        impl core::hash::Hash for $name {
            fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                core::hash::Hash::hash(&self.value(), state);
            }
        }
    };
}

authored_id!(
    "Идентификатор объявленного исходного сигнала.",
    SourceIdV1,
    SourceId
);
authored_id!("Идентификатор решаемой цели.", TargetIdV1, TargetId);
authored_id!(
    "Идентификатор конечного кандидата одной цели.",
    TargetCandidateIdV1,
    TargetCandidateId
);
authored_id!(
    "Идентификатор объявленного входа прозрачности.",
    OpacityInputIdV1,
    OpacityInputId
);
authored_id!("Идентификатор узла Paint.", PaintIdV1, PaintId);
authored_id!(
    "Идентификатор входного порта динамической поверхности.",
    SurfaceInputPortIdV1,
    SurfaceInputPortId
);
authored_id!("Идентификатор узла Surface.", SurfaceIdV1, SurfaceId);
authored_id!(
    "Идентификатор физического наложения Paint на Surface.",
    OccurrenceIdV1,
    OccurrenceId
);
authored_id!(
    "Идентификатор проверяемого ограничения.",
    ConstraintIdV1,
    ConstraintId
);
authored_id!(
    "Идентификатор клиентского выходного слота.",
    OutputSlotIdV1,
    OutputSlotId
);
projected_id!(
    "Идентификатор потока, сохранённый в наблюдении.",
    StreamIdV1,
    ObservationStreamId
);
projected_id!(
    "Идентификатор сценария, сохранённый как provenance.",
    ScenarioIdV1,
    ScenarioId
);

/// Один физический кандидат конечной цели.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetCandidateV1(CoreTargetCandidateV1);

impl TargetCandidateV1 {
    /// Связывает непрозрачный ID кандидата с конкретным encoded sRGB8 сигналом.
    pub const fn new(id: TargetCandidateIdV1, source: Srgb8) -> Self {
        Self(CoreTargetCandidateV1::from_srgb8(id.into_core(), source))
    }
}

/// Выбор одного кандидата для одной цели в совместном состоянии.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JointChoiceV1(TargetCandidateChoiceV1);

impl JointChoiceV1 {
    /// Создаёт типизированную пару `цель → кандидат`.
    pub const fn new(target: TargetIdV1, candidate: TargetCandidateIdV1) -> Self {
        Self(TargetCandidateChoiceV1::new(
            target.into_core(),
            candidate.into_core(),
        ))
    }
}

/// Полное явно объявленное состояние всех конечных целей.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JointStateV1(JointCandidateStateV1);

impl JointStateV1 {
    /// Создаёт состояние из одного выбора для каждой конечной цели.
    pub fn new(choices: Vec<JointChoiceV1>) -> Self {
        Self(JointCandidateStateV1::new(
            choices.into_iter().map(|choice| choice.0).collect(),
        ))
    }
}

/// Зарегистрированный режим окружения CIECAM16.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurroundV1 {
    /// Среднее освещение окружения.
    Average,
    /// Приглушённое освещение окружения.
    Dim,
    /// Тёмное окружение.
    Dark,
}

impl SurroundV1 {
    const fn into_core(self) -> SurroundProfileId {
        match self {
            Self::Average => SurroundProfileId::AverageV1,
            Self::Dim => SurroundProfileId::DimV1,
            Self::Dark => SurroundProfileId::DarkV1,
        }
    }
}

/// Поле входного контекста восприятия, не прошедшее admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppearanceContextFieldV1 {
    /// Адаптирующая яркость в кд/м².
    AdaptingLuminanceCdM2,
    /// Безразмерное отношение фоновой яркости `Y_b/Y_w`.
    BackgroundLuminanceRatioYbYw,
}

/// Числовая причина отказа при формировании контекста восприятия.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericDomainErrorV1 {
    /// Значение не является конечным числом.
    NonFinite,
    /// Значение отрицательно.
    Negative,
    /// Значение должно быть строго положительным.
    NotPositive,
    /// Значение превышает единицу.
    AboveOne,
    /// Угол оттенка находится вне полуинтервала `[0°, 360°)`.
    HueOutOfRange,
}

/// Закрытая классификация отказа admission контекста восприятия.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppearanceContextErrorKindV1 {
    /// Клиентское значение находится вне объявленного домена.
    Domain,
    /// Нарушен внутренний инвариант закрытого преобразования.
    InternalInvariant,
}

/// Типизированный отказ admission контекста восприятия.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppearanceContextErrorV1 {
    kind: AppearanceContextErrorKindV1,
    field: Option<AppearanceContextFieldV1>,
    reason: Option<NumericDomainErrorV1>,
}

impl AppearanceContextErrorV1 {
    /// Возвращает класс отказа.
    pub const fn kind(self) -> AppearanceContextErrorKindV1 {
        self.kind
    }

    /// Возвращает отвергнутое поле, когда Core смог его локализовать.
    pub const fn field(self) -> Option<AppearanceContextFieldV1> {
        self.field
    }

    /// Возвращает точную числовую причину, если отказ относится к входному домену.
    pub const fn reason(self) -> Option<NumericDomainErrorV1> {
        self.reason
    }

    fn from_core(error: AppearanceContextDomainErrorV1) -> Self {
        let field = match error.field() {
            CoreAppearanceContextFieldV1::AdaptingLuminanceCdM2 => {
                AppearanceContextFieldV1::AdaptingLuminanceCdM2
            }
            CoreAppearanceContextFieldV1::BackgroundLuminanceRatio => {
                AppearanceContextFieldV1::BackgroundLuminanceRatioYbYw
            }
        };
        let reason = match error.reason() {
            NumericDomainError::NonFinite => Some(NumericDomainErrorV1::NonFinite),
            NumericDomainError::Negative => Some(NumericDomainErrorV1::Negative),
            NumericDomainError::NotPositive => Some(NumericDomainErrorV1::NotPositive),
            NumericDomainError::AboveOne => Some(NumericDomainErrorV1::AboveOne),
            NumericDomainError::HueOutOfRange => None,
        };
        match reason {
            Some(reason) => Self {
                kind: AppearanceContextErrorKindV1::Domain,
                field: Some(field),
                reason: Some(reason),
            },
            None => Self {
                kind: AppearanceContextErrorKindV1::InternalInvariant,
                field: Some(field),
                reason: None,
            },
        }
    }
}

/// Неизменяемый допущенный контекст восприятия.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppearanceContextV1(AppearanceContextId);

impl AppearanceContextV1 {
    const fn from_core(context: AppearanceContextId) -> Self {
        Self(context)
    }

    /// Допускает явные входы CIECAM16 для encoded sRGB8/D65.
    ///
    /// `background_luminance_ratio_yb_yw` — безразмерное `Y_b/Y_w` в `(0, 1]`,
    /// а не абсолютная яркость.
    pub fn try_new(
        adapting_luminance_cd_m2: f64,
        background_luminance_ratio_yb_yw: f64,
        surround: SurroundV1,
    ) -> Result<Self, AppearanceContextErrorV1> {
        let adapting_luminance_cd_m2 = AdaptingLuminanceCdM2::try_new(adapting_luminance_cd_m2)
            .map_err(AppearanceContextErrorV1::from_core)?;
        let background_luminance_ratio =
            BackgroundLuminanceRatio::try_new(background_luminance_ratio_yb_yw)
                .map_err(AppearanceContextErrorV1::from_core)?;
        Ok(Self(AppearanceContextId::from_inputs(
            AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
            IEC_SRGB_D65_XYZ_FRAME_V1,
            adapting_luminance_cd_m2,
            background_luminance_ratio,
            surround.into_core(),
        )))
    }

    /// Возвращает допущенную адаптирующую яркость в кд/м².
    pub fn adapting_luminance_cd_m2(self) -> f64 {
        self.0.adapting_luminance_cd_m2()
    }

    /// Возвращает допущенное безразмерное отношение `Y_b/Y_w`.
    pub fn background_luminance_ratio_yb_yw(self) -> f64 {
        self.0.background_luminance_ratio()
    }

    /// Возвращает зарегистрированный режим окружения.
    pub const fn surround(self) -> SurroundV1 {
        match self.0.surround_profile() {
            SurroundProfileId::AverageV1 => SurroundV1::Average,
            SurroundProfileId::DimV1 => SurroundV1::Dim,
            SurroundProfileId::DarkV1 => SurroundV1::Dark,
        }
    }
}

/// Закрытая классификация ошибки компиляции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileErrorKindV1 {
    /// Повторно объявлен исходный сигнал.
    DuplicateSource,
    /// Повторно объявлена цель.
    DuplicateTarget,
    /// В одной цели повторно объявлен ID кандидата.
    DuplicateTargetCandidate,
    /// Два кандидата одной цели задают одинаковый сигнал.
    DuplicateTargetCandidateSignal,
    /// Повторно объявлен вход прозрачности.
    DuplicateOpacityInput,
    /// Повторно объявлен входной порт поверхности.
    DuplicateSurfaceInputPort,
    /// Объявленный входной порт поверхности не используется.
    UnusedSurfaceInputPort,
    /// Один входной порт привязан к нескольким Surface.
    DuplicateSurfaceInputBinding,
    /// Повторно объявлен Paint.
    DuplicatePaint,
    /// Повторно объявлен Surface.
    DuplicateSurface,
    /// Повторно объявлен Occurrence.
    DuplicateOccurrence,
    /// Повторно объявлено ограничение.
    DuplicateConstraint,
    /// Повторно объявлен выходной слот.
    DuplicateOutputSlot,
    /// Цель ссылается на отсутствующий исходный сигнал.
    MissingTargetSource,
    /// Paint ссылается на отсутствующую цель.
    MissingPaintTarget,
    /// Paint ссылается на отсутствующий Paint.
    MissingPaintSource,
    /// Paint ссылается на отсутствующий вход прозрачности.
    MissingPaintOpacityInput,
    /// Surface ссылается на отсутствующий входной порт.
    MissingSurfaceInputPort,
    /// Surface ссылается на отсутствующий Occurrence.
    MissingSurfaceOccurrence,
    /// Occurrence ссылается на отсутствующий Paint.
    MissingOccurrencePaint,
    /// Occurrence ссылается на отсутствующий backdrop Surface.
    MissingOccurrenceBackdrop,
    /// Ограничение ссылается на отсутствующий Occurrence.
    MissingConstraintOccurrence,
    /// Выход ссылается на отсутствующий Paint.
    MissingOutputPaint,
    /// Граф Paint содержит цикл.
    PaintCycle,
    /// Граф рендера содержит цикл Surface/Occurrence.
    RenderCycle,
    /// Значение прозрачности находится вне `[0, 1]` или не конечно.
    OpacityOutOfDomain,
    /// Конечная цель не содержит кандидатов.
    EmptyTargetDomain,
    /// Конечная цель не участвует ни в одном ограничении.
    UnconstrainedTarget,
    /// Конечные цели образуют несвязанные компоненты.
    DisconnectedFiniteTargets,
    /// Выходной Paint не покрыт ни одним ограничением.
    UnassessedOutput,
    /// Для конечных целей не объявлен совместный порядок.
    MissingJointSelection,
    /// Совместный порядок объявлен без конечных целей.
    JointSelectionWithoutTargets,
    /// Состояние повторяет одну цель.
    JointStateDuplicateTarget,
    /// В состоянии отсутствует конечная цель.
    JointStateMissingTarget,
    /// Состояние ссылается на неизвестную цель.
    JointStateUnknownTarget,
    /// Состояние ссылается на неизвестного кандидата.
    JointStateUnknownCandidate,
    /// Совместный порядок не является полным конечным порядком.
    InvalidJointOrder,
    /// Не объявлено ни одного динамического входа поверхности.
    EmptySurfaceInputPortSet,
    /// Не объявлено ни одного физического Occurrence.
    EmptyOccurrenceSet,
    /// Не объявлено ни одного ограничения.
    EmptyConstraintSet,
    /// Не объявлено ни одного выхода.
    EmptyOutputSet,
    /// Для компиляции недостаточно памяти или адресного пространства.
    ResourceExhausted,
    /// Нарушен внутренний инвариант закрытого компилятора.
    InternalInvariant,
}

/// Типизированный ID узла, к которому относится ошибка компиляции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileErrorHandleV1 {
    /// Исходный сигнал.
    Source(SourceIdV1),
    /// Цель.
    Target(TargetIdV1),
    /// Кандидат цели.
    TargetCandidate(TargetCandidateIdV1),
    /// Вход прозрачности.
    OpacityInput(OpacityInputIdV1),
    /// Paint.
    Paint(PaintIdV1),
    /// Входной порт Surface.
    SurfaceInputPort(SurfaceInputPortIdV1),
    /// Surface.
    Surface(SurfaceIdV1),
    /// Occurrence.
    Occurrence(OccurrenceIdV1),
    /// Ограничение.
    Constraint(ConstraintIdV1),
    /// Выходной слот.
    OutputSlot(OutputSlotIdV1),
}

impl CompileErrorHandleV1 {
    /// Возвращает клиентский числовой ключ независимо от пространства ID.
    pub const fn value(self) -> u32 {
        match self {
            Self::Source(value) => value.value(),
            Self::Target(value) => value.value(),
            Self::TargetCandidate(value) => value.value(),
            Self::OpacityInput(value) => value.value(),
            Self::Paint(value) => value.value(),
            Self::SurfaceInputPort(value) => value.value(),
            Self::Surface(value) => value.value(),
            Self::Occurrence(value) => value.value(),
            Self::Constraint(value) => value.value(),
            Self::OutputSlot(value) => value.value(),
        }
    }
}

/// Точные участники одного цикла зависимостей Paint.
#[derive(Debug, PartialEq, Eq)]
pub struct PaintCycleV1 {
    paints: Vec<PaintId>,
}

impl PaintCycleV1 {
    /// Возвращает участников цикла в каноническом порядке диагностики.
    pub fn paints(&self) -> impl ExactSizeIterator<Item = PaintIdV1> + '_ {
        self.paints.iter().copied().map(PaintIdV1::from_core)
    }
}

/// Точные участники одного цикла рендера.
#[derive(Debug, PartialEq, Eq)]
pub struct RenderCycleV1 {
    surfaces: Vec<SurfaceId>,
    occurrences: Vec<OccurrenceId>,
}

impl RenderCycleV1 {
    /// Возвращает Surface-участников цикла.
    pub fn surfaces(&self) -> impl ExactSizeIterator<Item = SurfaceIdV1> + '_ {
        self.surfaces.iter().copied().map(SurfaceIdV1::from_core)
    }

    /// Возвращает Occurrence-участников цикла.
    pub fn occurrences(&self) -> impl ExactSizeIterator<Item = OccurrenceIdV1> + '_ {
        self.occurrences
            .iter()
            .copied()
            .map(OccurrenceIdV1::from_core)
    }
}

/// Точная причина отказа явно объявленного конечного совместного порядка.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JointOrderErrorV1 {
    /// Одно измерение не содержит кандидатов.
    EmptyDomain {
        /// Индекс пустого измерения.
        dimension: usize,
    },
    /// Декартова мощность измерений не представима в `usize`.
    CardinalityOverflow,
    /// Явный порядок не содержит состояний.
    EmptyOrder,
    /// Состояние содержит неверное число измерений.
    TupleArity {
        /// Индекс состояния.
        state: usize,
        /// Требуемое число измерений.
        expected: usize,
        /// Фактическое число измерений.
        actual: usize,
    },
    /// Ординал кандидата выходит за домен измерения.
    OrdinalOutOfDomain {
        /// Индекс состояния.
        state: usize,
        /// Индекс измерения.
        dimension: usize,
        /// Некорректный ординал.
        ordinal: usize,
        /// Мощность домена измерения.
        domain_len: usize,
    },
    /// Два состояния задают один и тот же кортеж.
    DuplicateTuple {
        /// Индекс первого состояния.
        first_state: usize,
        /// Индекс повторного состояния.
        duplicate_state: usize,
    },
    /// Порядок не покрывает полный декартов домен.
    IncompleteOrder {
        /// Требуемое число состояний.
        expected: usize,
        /// Фактическое число состояний.
        actual: usize,
    },
    /// Для проверки порядка недостаточно ресурсов.
    ResourceExhausted,
}

/// Атомарная и полная ошибка компиляции объявленной программы.
///
/// Enum авторитетен; [`Self::kind`], [`Self::primary_handle`] и
/// [`Self::related_handle`] — только удобные проекции полного payload.
#[derive(Debug, PartialEq, Eq)]
pub enum CompileErrorV1 {
    /// Повторно объявлен исходный сигнал.
    DuplicateSource {
        /// Повторный ID.
        source: SourceIdV1,
    },
    /// Повторно объявлена цель.
    DuplicateTarget {
        /// Повторный ID.
        target: TargetIdV1,
    },
    /// Цель ссылается на отсутствующий исходный сигнал.
    MissingTargetSource {
        /// Ошибочная цель.
        target: TargetIdV1,
        /// Отсутствующий исходный сигнал.
        source: SourceIdV1,
    },
    /// Повторно объявлен вход прозрачности.
    DuplicateOpacityInput {
        /// Повторный ID.
        input: OpacityInputIdV1,
    },
    /// Повторно объявлен входной порт поверхности.
    DuplicateSurfaceInputPort {
        /// Повторный ID.
        input: SurfaceInputPortIdV1,
    },
    /// Объявленный входной порт не используется.
    UnusedSurfaceInputPort {
        /// Неиспользуемый ID.
        input: SurfaceInputPortIdV1,
    },
    /// Один входной порт привязан к двум Surface.
    DuplicateSurfaceInputBinding {
        /// Повторно привязанный порт.
        input: SurfaceInputPortIdV1,
        /// Первая Surface.
        first: SurfaceIdV1,
        /// Повторная Surface.
        duplicate: SurfaceIdV1,
    },
    /// Повторно объявлен Paint.
    DuplicatePaint {
        /// Повторный ID.
        paint: PaintIdV1,
    },
    /// Повторно объявлен Surface.
    DuplicateSurface {
        /// Повторный ID.
        surface: SurfaceIdV1,
    },
    /// Повторно объявлен Occurrence.
    DuplicateOccurrence {
        /// Повторный ID.
        occurrence: OccurrenceIdV1,
    },
    /// Paint ссылается на отсутствующую цель.
    MissingPaintTarget {
        /// Ошибочный Paint.
        paint: PaintIdV1,
        /// Отсутствующая цель.
        target: TargetIdV1,
    },
    /// Paint ссылается на отсутствующий исходный Paint.
    MissingPaintSource {
        /// Ошибочный Paint.
        paint: PaintIdV1,
        /// Отсутствующий исходный Paint.
        source: PaintIdV1,
    },
    /// Paint ссылается на отсутствующий вход прозрачности.
    MissingPaintOpacityInput {
        /// Ошибочный Paint.
        paint: PaintIdV1,
        /// Отсутствующий вход.
        input: OpacityInputIdV1,
    },
    /// Surface ссылается на отсутствующий входной порт.
    MissingSurfaceInputPort {
        /// Ошибочная Surface.
        surface: SurfaceIdV1,
        /// Отсутствующий порт.
        input: SurfaceInputPortIdV1,
    },
    /// Surface ссылается на отсутствующий Occurrence.
    MissingSurfaceOccurrence {
        /// Ошибочная Surface.
        surface: SurfaceIdV1,
        /// Отсутствующий Occurrence.
        occurrence: OccurrenceIdV1,
    },
    /// Occurrence ссылается на отсутствующий Paint.
    MissingOccurrencePaint {
        /// Ошибочный Occurrence.
        occurrence: OccurrenceIdV1,
        /// Отсутствующий Paint.
        paint: PaintIdV1,
    },
    /// Occurrence ссылается на отсутствующий backdrop Surface.
    MissingOccurrenceBackdrop {
        /// Ошибочный Occurrence.
        occurrence: OccurrenceIdV1,
        /// Отсутствующая Surface.
        surface: SurfaceIdV1,
    },
    /// Обнаружен цикл зависимостей Paint.
    PaintCycle(PaintCycleV1),
    /// Обнаружен цикл Surface/Occurrence.
    RenderCycle(RenderCycleV1),
    /// Вход прозрачности не является конечным числом в `[0, 1]`.
    OpacityOutOfDomain {
        /// Ошибочный вход.
        input: OpacityInputIdV1,
    },
    /// Конечная цель не содержит кандидатов.
    EmptyTargetDomain {
        /// Пустая цель.
        target: TargetIdV1,
    },
    /// В одной цели повторно объявлен ID кандидата.
    DuplicateTargetCandidate {
        /// Цель кандидата.
        target: TargetIdV1,
        /// Повторный кандидат.
        candidate: TargetCandidateIdV1,
    },
    /// Два кандидата одной цели имеют одинаковый физический сигнал.
    DuplicateTargetCandidateSignal {
        /// Цель кандидатов.
        target: TargetIdV1,
        /// Первый кандидат.
        first: TargetCandidateIdV1,
        /// Повторный кандидат.
        duplicate: TargetCandidateIdV1,
        /// Совпавший encoded sRGB8 сигнал.
        encoded_srgb8: Srgb8,
    },
    /// Конечная цель не участвует ни в одном ограничении.
    UnconstrainedTarget {
        /// Неограниченная цель.
        target: TargetIdV1,
    },
    /// Конечные цели образуют несвязанные компоненты.
    DisconnectedFiniteTargets,
    /// Выходной Paint не покрыт ни одним ограничением.
    UnassessedOutput {
        /// Непроверенный выход.
        output: OutputSlotIdV1,
        /// Его Paint.
        paint: PaintIdV1,
    },
    /// Для конечных целей не объявлен совместный порядок.
    MissingJointSelection,
    /// Совместный порядок объявлен без конечных целей.
    JointSelectionWithoutTargets,
    /// Состояние повторяет одну цель.
    JointStateDuplicateTarget {
        /// Индекс состояния.
        state: usize,
        /// Повторная цель.
        target: TargetIdV1,
    },
    /// В состоянии отсутствует конечная цель.
    JointStateMissingTarget {
        /// Индекс состояния.
        state: usize,
        /// Отсутствующая цель.
        target: TargetIdV1,
    },
    /// Состояние ссылается на неизвестную цель.
    JointStateUnknownTarget {
        /// Индекс состояния.
        state: usize,
        /// Неизвестная цель.
        target: TargetIdV1,
    },
    /// Состояние ссылается на неизвестного кандидата.
    JointStateUnknownCandidate {
        /// Индекс состояния.
        state: usize,
        /// Цель кандидата.
        target: TargetIdV1,
        /// Неизвестный кандидат.
        candidate: TargetCandidateIdV1,
    },
    /// Явный совместный порядок не является полным конечным порядком.
    InvalidJointOrder(JointOrderErrorV1),
    /// Не объявлено ни одного динамического входа поверхности.
    EmptySurfaceInputPortSet,
    /// Не объявлено ни одного физического Occurrence.
    EmptyOccurrenceSet,
    /// Не объявлено ни одного ограничения.
    EmptyConstraintSet,
    /// Не объявлено ни одного выхода.
    EmptyOutputSet,
    /// Повторно объявлено ограничение.
    DuplicateConstraint {
        /// Повторный ID.
        constraint: ConstraintIdV1,
    },
    /// Ограничение ссылается на отсутствующий Occurrence.
    MissingConstraintOccurrence {
        /// Ошибочное ограничение.
        constraint: ConstraintIdV1,
        /// Отсутствующий Occurrence.
        occurrence: OccurrenceIdV1,
    },
    /// Повторно объявлен выходной слот.
    DuplicateOutputSlot {
        /// Повторный ID.
        output: OutputSlotIdV1,
    },
    /// Выход ссылается на отсутствующий Paint.
    MissingOutputPaint {
        /// Ошибочный выход.
        output: OutputSlotIdV1,
        /// Отсутствующий Paint.
        paint: PaintIdV1,
    },
    /// Для компиляции недостаточно ресурсов.
    ResourceExhausted,
    /// Нарушен внутренний инвариант закрытого компилятора.
    InternalInvariant,
}

impl CompileErrorV1 {
    /// Возвращает стабильный класс ошибки без потери полного payload.
    pub const fn kind(&self) -> CompileErrorKindV1 {
        use CompileErrorKindV1 as Kind;

        match self {
            Self::DuplicateSource { .. } => Kind::DuplicateSource,
            Self::DuplicateTarget { .. } => Kind::DuplicateTarget,
            Self::MissingTargetSource { .. } => Kind::MissingTargetSource,
            Self::DuplicateOpacityInput { .. } => Kind::DuplicateOpacityInput,
            Self::DuplicateSurfaceInputPort { .. } => Kind::DuplicateSurfaceInputPort,
            Self::UnusedSurfaceInputPort { .. } => Kind::UnusedSurfaceInputPort,
            Self::DuplicateSurfaceInputBinding { .. } => Kind::DuplicateSurfaceInputBinding,
            Self::DuplicatePaint { .. } => Kind::DuplicatePaint,
            Self::DuplicateSurface { .. } => Kind::DuplicateSurface,
            Self::DuplicateOccurrence { .. } => Kind::DuplicateOccurrence,
            Self::MissingPaintTarget { .. } => Kind::MissingPaintTarget,
            Self::MissingPaintSource { .. } => Kind::MissingPaintSource,
            Self::MissingPaintOpacityInput { .. } => Kind::MissingPaintOpacityInput,
            Self::MissingSurfaceInputPort { .. } => Kind::MissingSurfaceInputPort,
            Self::MissingSurfaceOccurrence { .. } => Kind::MissingSurfaceOccurrence,
            Self::MissingOccurrencePaint { .. } => Kind::MissingOccurrencePaint,
            Self::MissingOccurrenceBackdrop { .. } => Kind::MissingOccurrenceBackdrop,
            Self::PaintCycle(_) => Kind::PaintCycle,
            Self::RenderCycle(_) => Kind::RenderCycle,
            Self::OpacityOutOfDomain { .. } => Kind::OpacityOutOfDomain,
            Self::EmptyTargetDomain { .. } => Kind::EmptyTargetDomain,
            Self::DuplicateTargetCandidate { .. } => Kind::DuplicateTargetCandidate,
            Self::DuplicateTargetCandidateSignal { .. } => Kind::DuplicateTargetCandidateSignal,
            Self::UnconstrainedTarget { .. } => Kind::UnconstrainedTarget,
            Self::DisconnectedFiniteTargets => Kind::DisconnectedFiniteTargets,
            Self::UnassessedOutput { .. } => Kind::UnassessedOutput,
            Self::MissingJointSelection => Kind::MissingJointSelection,
            Self::JointSelectionWithoutTargets => Kind::JointSelectionWithoutTargets,
            Self::JointStateDuplicateTarget { .. } => Kind::JointStateDuplicateTarget,
            Self::JointStateMissingTarget { .. } => Kind::JointStateMissingTarget,
            Self::JointStateUnknownTarget { .. } => Kind::JointStateUnknownTarget,
            Self::JointStateUnknownCandidate { .. } => Kind::JointStateUnknownCandidate,
            Self::InvalidJointOrder(_) => Kind::InvalidJointOrder,
            Self::EmptySurfaceInputPortSet => Kind::EmptySurfaceInputPortSet,
            Self::EmptyOccurrenceSet => Kind::EmptyOccurrenceSet,
            Self::EmptyConstraintSet => Kind::EmptyConstraintSet,
            Self::EmptyOutputSet => Kind::EmptyOutputSet,
            Self::DuplicateConstraint { .. } => Kind::DuplicateConstraint,
            Self::MissingConstraintOccurrence { .. } => Kind::MissingConstraintOccurrence,
            Self::DuplicateOutputSlot { .. } => Kind::DuplicateOutputSlot,
            Self::MissingOutputPaint { .. } => Kind::MissingOutputPaint,
            Self::ResourceExhausted => Kind::ResourceExhausted,
            Self::InternalInvariant => Kind::InternalInvariant,
        }
    }

    /// Возвращает основной типизированный ID, если ошибка локализуема одним узлом.
    pub const fn primary_handle(&self) -> Option<CompileErrorHandleV1> {
        use CompileErrorHandleV1 as Handle;

        match self {
            Self::DuplicateSource { source } => Some(Handle::Source(*source)),
            Self::DuplicateTarget { target }
            | Self::EmptyTargetDomain { target }
            | Self::UnconstrainedTarget { target }
            | Self::JointStateDuplicateTarget { target, .. }
            | Self::JointStateMissingTarget { target, .. }
            | Self::JointStateUnknownTarget { target, .. }
            | Self::JointStateUnknownCandidate { target, .. }
            | Self::MissingTargetSource { target, .. }
            | Self::DuplicateTargetCandidate { target, .. }
            | Self::DuplicateTargetCandidateSignal { target, .. } => Some(Handle::Target(*target)),
            Self::DuplicateOpacityInput { input } | Self::OpacityOutOfDomain { input } => {
                Some(Handle::OpacityInput(*input))
            }
            Self::DuplicateSurfaceInputPort { input }
            | Self::UnusedSurfaceInputPort { input }
            | Self::DuplicateSurfaceInputBinding { input, .. } => {
                Some(Handle::SurfaceInputPort(*input))
            }
            Self::DuplicatePaint { paint }
            | Self::MissingPaintTarget { paint, .. }
            | Self::MissingPaintSource { paint, .. }
            | Self::MissingPaintOpacityInput { paint, .. } => Some(Handle::Paint(*paint)),
            Self::DuplicateSurface { surface }
            | Self::MissingSurfaceInputPort { surface, .. }
            | Self::MissingSurfaceOccurrence { surface, .. } => Some(Handle::Surface(*surface)),
            Self::DuplicateOccurrence { occurrence }
            | Self::MissingOccurrencePaint { occurrence, .. }
            | Self::MissingOccurrenceBackdrop { occurrence, .. } => {
                Some(Handle::Occurrence(*occurrence))
            }
            Self::UnassessedOutput { output, .. }
            | Self::DuplicateOutputSlot { output }
            | Self::MissingOutputPaint { output, .. } => Some(Handle::OutputSlot(*output)),
            Self::DuplicateConstraint { constraint }
            | Self::MissingConstraintOccurrence { constraint, .. } => {
                Some(Handle::Constraint(*constraint))
            }
            Self::PaintCycle(_)
            | Self::RenderCycle(_)
            | Self::DisconnectedFiniteTargets
            | Self::MissingJointSelection
            | Self::JointSelectionWithoutTargets
            | Self::InvalidJointOrder(_)
            | Self::EmptySurfaceInputPortSet
            | Self::EmptyOccurrenceSet
            | Self::EmptyConstraintSet
            | Self::EmptyOutputSet
            | Self::ResourceExhausted
            | Self::InternalInvariant => None,
        }
    }

    /// Возвращает связанный типизированный ID для ошибки отношения двух узлов.
    pub const fn related_handle(&self) -> Option<CompileErrorHandleV1> {
        use CompileErrorHandleV1 as Handle;

        match self {
            Self::MissingTargetSource { source, .. } => Some(Handle::Source(*source)),
            Self::DuplicateSurfaceInputBinding { duplicate, .. } => {
                Some(Handle::Surface(*duplicate))
            }
            Self::MissingPaintTarget { target, .. } => Some(Handle::Target(*target)),
            Self::MissingPaintSource { source, .. } => Some(Handle::Paint(*source)),
            Self::MissingPaintOpacityInput { input, .. } => Some(Handle::OpacityInput(*input)),
            Self::MissingSurfaceInputPort { input, .. } => Some(Handle::SurfaceInputPort(*input)),
            Self::MissingSurfaceOccurrence { occurrence, .. }
            | Self::MissingConstraintOccurrence { occurrence, .. } => {
                Some(Handle::Occurrence(*occurrence))
            }
            Self::MissingOccurrencePaint { paint, .. }
            | Self::UnassessedOutput { paint, .. }
            | Self::MissingOutputPaint { paint, .. } => Some(Handle::Paint(*paint)),
            Self::MissingOccurrenceBackdrop { surface, .. } => Some(Handle::Surface(*surface)),
            Self::DuplicateTargetCandidate { candidate, .. }
            | Self::DuplicateTargetCandidateSignal {
                duplicate: candidate,
                ..
            }
            | Self::JointStateUnknownCandidate { candidate, .. } => {
                Some(Handle::TargetCandidate(*candidate))
            }
            Self::DuplicateSource { .. }
            | Self::DuplicateTarget { .. }
            | Self::DuplicateOpacityInput { .. }
            | Self::DuplicateSurfaceInputPort { .. }
            | Self::UnusedSurfaceInputPort { .. }
            | Self::DuplicatePaint { .. }
            | Self::DuplicateSurface { .. }
            | Self::DuplicateOccurrence { .. }
            | Self::PaintCycle(_)
            | Self::RenderCycle(_)
            | Self::OpacityOutOfDomain { .. }
            | Self::EmptyTargetDomain { .. }
            | Self::UnconstrainedTarget { .. }
            | Self::DisconnectedFiniteTargets
            | Self::MissingJointSelection
            | Self::JointSelectionWithoutTargets
            | Self::JointStateDuplicateTarget { .. }
            | Self::JointStateMissingTarget { .. }
            | Self::JointStateUnknownTarget { .. }
            | Self::InvalidJointOrder(_)
            | Self::EmptySurfaceInputPortSet
            | Self::EmptyOccurrenceSet
            | Self::EmptyConstraintSet
            | Self::EmptyOutputSet
            | Self::DuplicateConstraint { .. }
            | Self::DuplicateOutputSlot { .. }
            | Self::ResourceExhausted
            | Self::InternalInvariant => None,
        }
    }
}

/// Холодный декларативный builder канонической Program IR.
#[must_use]
pub struct DraftV1 {
    inner: CoreProgramDraftV1,
}

/// Ошибка изменения Draft до компиляции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftErrorV1 {
    /// Совместный порядок уже объявлен и не может быть молча заменён.
    JointSelectionAlreadyDeclared,
}

impl DraftV1 {
    /// Создаёт пустой Draft.
    pub fn new() -> Self {
        Self {
            inner: CoreProgramDraftV1::new(),
        }
    }

    /// Объявляет неизменяемый исходный encoded sRGB8 сигнал.
    pub fn push_source(&mut self, id: SourceIdV1, source: Srgb8) -> &mut Self {
        self.inner
            .push_source(Source::new(id.into_core(), ColorSignal::from_srgb8(source)));
        self
    }

    /// Объявляет цель, физически равную исходному сигналу.
    pub fn push_fixed_target(&mut self, id: TargetIdV1, source: SourceIdV1) -> &mut Self {
        self.inner
            .push_target(Target::fixed(id.into_core(), source.into_core()));
        self
    }

    /// Объявляет решаемую цель с конечным набором физических кандидатов.
    pub fn push_finite_target(
        &mut self,
        id: TargetIdV1,
        source: SourceIdV1,
        candidates: Vec<TargetCandidateV1>,
    ) -> &mut Self {
        self.inner.push_target(Target::finite(
            id.into_core(),
            source.into_core(),
            candidates
                .into_iter()
                .map(|candidate| candidate.0)
                .collect(),
        ));
        self
    }

    /// Один раз задаёт полный порядок совместных состояний конечных целей.
    pub fn set_joint_selection(
        &mut self,
        states: Vec<JointStateV1>,
    ) -> Result<&mut Self, DraftErrorV1> {
        self.inner
            .set_joint_selection(DeclaredJointSelectionV1::new(
                states.into_iter().map(|state| state.0).collect(),
            ))
            .map_err(|error| match error {
                CoreProgramDraftErrorV1::JointSelectionAlreadyDeclared => {
                    DraftErrorV1::JointSelectionAlreadyDeclared
                }
            })?;
        Ok(self)
    }

    /// Объявляет один динамический вход поверхности.
    pub fn push_surface_input_port(&mut self, input: SurfaceInputPortIdV1) -> &mut Self {
        self.inner.push_surface_input_port(input.into_core());
        self
    }

    /// Объявляет числовой вход прозрачности; домен проверяется при компиляции.
    pub fn push_opacity_input(&mut self, id: OpacityInputIdV1, value: f64) -> &mut Self {
        self.inner
            .push_opacity_input(OpacityInput::new(id.into_core(), value));
        self
    }

    /// Объявляет непрозрачный Paint, связанный с целью.
    pub fn push_solid_paint(&mut self, id: PaintIdV1, target: TargetIdV1) -> &mut Self {
        self.inner.push_paint(Paint::Solid {
            id: id.into_core(),
            target: target.into_core(),
        });
        self
    }

    /// Объявляет Paint как прозрачную версию другого Paint.
    pub fn push_opacity_paint(
        &mut self,
        id: PaintIdV1,
        source: PaintIdV1,
        opacity: OpacityInputIdV1,
    ) -> &mut Self {
        self.inner.push_paint(Paint::Opacity {
            id: id.into_core(),
            source: source.into_core(),
            opacity: opacity.into_core(),
        });
        self
    }

    /// Объявляет Surface, значение которой поступает из runtime-сценария.
    pub fn push_input_surface(
        &mut self,
        id: SurfaceIdV1,
        input: SurfaceInputPortIdV1,
    ) -> &mut Self {
        self.inner.push_surface(Surface::Input {
            id: id.into_core(),
            input: input.into_core(),
        });
        self
    }

    /// Объявляет Surface как видимый результат другого Occurrence.
    pub fn push_occurrence_surface(
        &mut self,
        id: SurfaceIdV1,
        occurrence: OccurrenceIdV1,
    ) -> &mut Self {
        self.inner.push_surface(Surface::FromOccurrence {
            id: id.into_core(),
            occurrence: occurrence.into_core(),
        });
        self
    }

    /// Объявляет encoded-sRGB8 source-over Occurrence в явном контексте.
    pub fn push_source_over_occurrence(
        &mut self,
        id: OccurrenceIdV1,
        subject: PaintIdV1,
        against: SurfaceIdV1,
        context: AppearanceContextV1,
    ) -> &mut Self {
        self.inner.push_occurrence(Occurrence::new(
            id.into_core(),
            subject.into_core(),
            against.into_core(),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context.0,
        ));
        self
    }

    /// Добавляет обязательное точное сравнение видимого sRGB8 результата.
    pub fn push_exact_hard(
        &mut self,
        id: ConstraintIdV1,
        occurrence: OccurrenceIdV1,
        expected: Srgb8,
    ) -> &mut Self {
        self.inner.push_hard_constraint(ConstraintInvocation::hard(
            id.into_core(),
            occurrence.into_core(),
            CoreProgramConstraintInvocationV1::ExactSrgb8(expected),
        ));
        self
    }

    /// Добавляет диагностическое точное сравнение, не влияющее на выбор.
    pub fn push_exact_report_only(
        &mut self,
        id: ConstraintIdV1,
        occurrence: OccurrenceIdV1,
        expected: Srgb8,
    ) -> &mut Self {
        self.inner
            .push_report_constraint(ConstraintInvocation::report_only(
                id.into_core(),
                occurrence.into_core(),
                CoreProgramConstraintInvocationV1::ExactSrgb8(expected),
            ));
        self
    }

    /// Добавляет обязательный критерий WCAG 2.2 для видимого результата.
    pub fn push_wcag22_hard(
        &mut self,
        id: ConstraintIdV1,
        occurrence: OccurrenceIdV1,
        criterion: Wcag22CriterionV1,
    ) -> &mut Self {
        self.inner.push_hard_constraint(ConstraintInvocation::hard(
            id.into_core(),
            occurrence.into_core(),
            CoreProgramConstraintInvocationV1::Wcag22Srgb8(criterion),
        ));
        self
    }

    /// Добавляет диагностический критерий WCAG 2.2, не влияющий на выбор.
    pub fn push_wcag22_report_only(
        &mut self,
        id: ConstraintIdV1,
        occurrence: OccurrenceIdV1,
        criterion: Wcag22CriterionV1,
    ) -> &mut Self {
        self.inner
            .push_report_constraint(ConstraintInvocation::report_only(
                id.into_core(),
                occurrence.into_core(),
                CoreProgramConstraintInvocationV1::Wcag22Srgb8(criterion),
            ));
        self
    }

    /// Связывает клиентский выходной слот с итоговым Paint.
    pub fn push_output(&mut self, output: OutputSlotIdV1, paint: PaintIdV1) -> &mut Self {
        self.inner
            .push_output(OutputBinding::new(output.into_core(), paint.into_core()));
        self
    }

    /// Атомарно проверяет и компилирует весь граф.
    pub fn compile(self) -> Result<OwnerV1, CompileErrorV1> {
        let compiled = self.inner.compile().map_err(map_program_compile_error)?;
        Ok(OwnerV1::from_compiled(compiled))
    }
}

impl Default for DraftV1 {
    fn default() -> Self {
        Self::new()
    }
}

/// Непрозрачный сильный владелец одной точной скомпилированной Program.
///
/// Созданные им Session изменяются только через эту же аллокацию. Уничтожение
/// Owner отзывает обновления и операции, но исторические evidence остаются в
/// Session.
pub struct OwnerV1 {
    compiled: CompiledCoreProgramV1,
}

/// Верхние границы числа клеток в новом сертификате одного Observed-update.
///
/// Границы относятся только к текущим клеткам доказательства. Они не включают
/// сохранённый прошлый сертификат, observation/provenance, выходы, операции или
/// байты конкретного транспорта.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidenceCellBoundsV1 {
    verified_cells: usize,
    conflict_cells: usize,
}

impl EvidenceCellBoundsV1 {
    /// Максимум клеток успешного сертификата.
    pub const fn verified_cells(self) -> usize {
        self.verified_cells
    }

    /// Максимум клеток исчерпывающего конфликтного сертификата.
    pub const fn conflict_cells(self) -> usize {
        self.conflict_cells
    }
}

/// Закрытая причина невозможности вычислить границы сертификата.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceBoundsErrorV1 {
    /// Произведение числа сценариев, ограничений и состояний не помещается в
    /// адресное пространство платформы.
    CardinalityOverflow,
}

/// Отказ доступа из-за несовпадения точной owner-эпохи.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessErrorV1 {
    /// Session была создана другой аллокацией Owner.
    OwnerMismatch,
}

impl OwnerV1 {
    /// Внутренняя передача из канонического компилятора.
    pub(crate) const fn from_compiled(compiled: CompiledCoreProgramV1) -> Self {
        Self { compiled }
    }

    /// Возвращает каноническую identity скомпилированного содержания.
    ///
    /// Identity доступна до первого update, но не заменяет полномочия этой
    /// конкретной owner-эпохи.
    pub fn content_identity(&self) -> ContentIdentityV1 {
        ContentIdentityV1::from_core(self.compiled.content_identity())
    }

    /// Вычисляет верхние границы клеток для prospective Observed-update.
    ///
    /// `scenario_count` — число объявленных клиентом сценариев до admission.
    /// Core сам схлопывает физически одинаковые сценарии, поэтому фактический
    /// сертификат может быть короче. Нулевое значение разрешено только как
    /// чистый арифметический preflight; пустой Observed-update по-прежнему не
    /// допускается. Запрос не создаёт Session и не меняет состояние.
    pub fn evidence_cell_bounds(
        &self,
        scenario_count: usize,
    ) -> Result<EvidenceCellBoundsV1, EvidenceBoundsErrorV1> {
        self.compiled
            .evidence_cell_bounds(scenario_count)
            .map(|(verified_cells, conflict_cells)| EvidenceCellBoundsV1 {
                verified_cells,
                conflict_cells,
            })
            .ok_or(EvidenceBoundsErrorV1::CardinalityOverflow)
    }

    /// Число значений Surface в каждом schema-ordered сценарии.
    pub fn surface_input_port_count(&self) -> usize {
        self.compiled.surface_input_ports().len()
    }

    /// Канонический порядок входных портов для однократного binding на хосте.
    pub fn surface_input_ports(&self) -> impl ExactSizeIterator<Item = SurfaceInputPortIdV1> + '_ {
        self.compiled
            .surface_input_ports()
            .iter()
            .copied()
            .map(SurfaceInputPortIdV1::from_core)
    }

    /// Канонический порядок непрозрачных выходных слотов.
    pub fn output_slots(&self) -> impl ExactSizeIterator<Item = OutputSlotIdV1> + '_ {
        self.compiled
            .outputs()
            .map(|(slot, _paint)| OutputSlotIdV1::from_core(slot))
    }

    /// Проецирует операции только для Session этой точной owner-эпохи.
    ///
    /// Равенство [`ContentIdentityV1`] не даёт полномочий.
    pub fn project<'owner, 'session>(
        &'owner self,
        session: &'session SessionV1,
    ) -> Result<ProjectionV1<'owner, 'session>, AccessErrorV1> {
        if !self.compiled.owns_session(&session.session) {
            return Err(AccessErrorV1::OwnerMismatch);
        }
        Ok(ProjectionV1 {
            evidence: session.evidence(),
            owner: self,
            scope: BorrowScopeV1::new(self, session),
        })
    }

    /// Атомарно допускает update и возвращает его неизменяемую проекцию.
    ///
    /// Несовпадение Owner проверяется до admission, аллокаций и вычисления.
    pub fn update<'owner, 'session>(
        &'owner self,
        session: &'session mut SessionV1,
        update: UpdateV1<'_>,
    ) -> Result<ProjectionV1<'owner, 'session>, UpdateErrorV1> {
        if !self.compiled.owns_session(&session.session) {
            return Err(UpdateErrorV1::OwnerMismatch);
        }
        session.apply_update(update)?;
        Ok(ProjectionV1 {
            evidence: session.evidence(),
            owner: self,
            scope: BorrowScopeV1::new(self, session),
        })
    }

    /// Создаёт Session, привязанную к одному непрозрачному stream ID.
    pub fn instantiate(&self, stream_id: u32) -> Result<SessionV1, InstantiateErrorV1> {
        let stream = ObservationStreamId::new(stream_id);
        let session = self
            .compiled
            .instantiate(stream)
            .map_err(InstantiateErrorV1::from_core)?;
        Ok(SessionV1 {
            scenario_order_scratch: Vec::new(),
            session,
        })
    }
}

/// Один заимствованный физический сценарий в скомпилированном schema order.
///
/// ID сценария — непрозрачная provenance. `values` содержит ровно один encoded
/// sRGB8 на каждый [`OwnerV1::surface_input_ports`] в том же порядке.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioV1<'a> {
    scenario_id: u32,
    values: &'a [Srgb8],
}

impl<'a> ScenarioV1<'a> {
    /// Создаёт один одновременный физический кортеж.
    pub const fn new(scenario_id: u32, values: &'a [Srgb8]) -> Self {
        Self {
            scenario_id,
            values,
        }
    }
}

/// Одно revision-bound обновление; stream принадлежит Session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateV1<'a> {
    /// Согласованные физические сценарии в порядке скомпилированной схемы.
    Observed {
        /// Монотонная ревизия входного наблюдения.
        revision: u64,
        /// Одновременные сценарии физического контекста.
        scenarios: &'a [ScenarioV1<'a>],
    },
    /// Наблюдение явно недоступно; Core не изобретает фон.
    Unknown {
        /// Монотонная ревизия входного наблюдения.
        revision: u64,
        /// Непрозрачная клиентская причина недоступности.
        reason_id: u32,
    },
}

/// Непрозрачная изменяемая Session одной Program и одного stream.
pub struct SessionV1 {
    scenario_order_scratch: Vec<usize>,
    session: CoreProgramSessionV1,
}

impl SessionV1 {
    /// Возвращает исторические evidence без права на операции.
    pub fn evidence(&self) -> EvidenceViewV1<'_> {
        EvidenceViewV1 {
            session: &self.session,
        }
    }

    fn apply_update(&mut self, update: UpdateV1<'_>) -> Result<(), UpdateErrorV1> {
        match update {
            UpdateV1::Observed {
                revision,
                scenarios,
            } => {
                let source = ScenarioSourceV1(scenarios);
                self.session
                    .update_schema_ordered(
                        Revision::new(revision),
                        &source,
                        &mut self.scenario_order_scratch,
                    )
                    .map_err(map_session_update_error)?;
            }
            UpdateV1::Unknown {
                revision,
                reason_id,
            } => {
                self.session
                    .update_unknown(Revision::new(revision), UnknownReasonId::new(reason_id))
                    .map_err(map_session_update_error)?;
            }
        }
        Ok(())
    }
}

struct ScenarioSourceV1<'a>(&'a [ScenarioV1<'a>]);

impl SchemaOrderedScenarioSourceV1 for ScenarioSourceV1<'_> {
    fn scenario_count(&self) -> usize {
        self.0.len()
    }

    fn scenario_id(&self, scenario_index: usize) -> ScenarioId {
        ScenarioId::new(self.0[scenario_index].scenario_id)
    }

    fn value_count(&self, scenario_index: usize) -> usize {
        self.0[scenario_index].values.len()
    }

    fn value(&self, scenario_index: usize, binding_index: usize) -> Srgb8 {
        self.0[scenario_index].values[binding_index]
    }
}

/// Закрытая классификация lifecycle Session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateKindV1 {
    /// Допущенного вычислимого наблюдения ещё нет; сырая голова может быть `Unknown`.
    Waiting,
    /// Текущая ревизия сертифицирована.
    Ready,
    /// Новое наблюдение недоступно; прошлый сертификат сохранён для диагностики.
    Stale,
    /// Текущая ревизия имеет исчерпывающий конфликт.
    Failed,
}

/// Текущая сырая голова наблюдений независимо от evaluator lifecycle.
///
/// Непустая голова хранит stream provenance, но не полномочия на операции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationHeadV1 {
    /// Наблюдений ещё не было.
    Empty,
    /// Наблюдение явно недоступно.
    Unknown {
        /// Stream наблюдения.
        stream: StreamIdV1,
        /// Ревизия наблюдения.
        revision: u64,
        /// Непрозрачная причина недоступности.
        reason_id: u32,
    },
    /// Принят физический набор сценариев.
    Observed {
        /// Stream наблюдения.
        stream: StreamIdV1,
        /// Ревизия наблюдения.
        revision: u64,
    },
}

/// Заимствованное историческое evidence, принадлежащее Session.
#[derive(Clone, Copy)]
pub struct EvidenceViewV1<'a> {
    session: &'a CoreProgramSessionV1,
}

impl<'a> EvidenceViewV1<'a> {
    const fn state(self) -> &'a CoreProgramStateV1 {
        self.session.state()
    }

    /// Возвращает lifecycle-класс текущего состояния.
    pub const fn kind(self) -> StateKindV1 {
        match self.state() {
            SessionState::Waiting => StateKindV1::Waiting,
            SessionState::Ready { .. } => StateKindV1::Ready,
            SessionState::Stale { .. } => StateKindV1::Stale,
            SessionState::Failed { .. } => StateKindV1::Failed,
        }
    }

    /// Возвращает сырую голову наблюдений вместе с provenance.
    pub fn observation_head(self) -> ObservationHeadV1 {
        match self.session.raw_head() {
            ObservationHeadViewV1::Empty => ObservationHeadV1::Empty,
            ObservationHeadViewV1::Unknown(unknown) => ObservationHeadV1::Unknown {
                stream: StreamIdV1::from_core(unknown.stream()),
                revision: unknown.revision().value(),
                reason_id: unknown.reason().value(),
            },
            ObservationHeadViewV1::Observed(observation) => ObservationHeadV1::Observed {
                stream: StreamIdV1::from_core(observation.stream()),
                revision: observation.revision().value(),
            },
        }
    }

    /// Индекс cause-сертификата в [`Self::certificates`] для `Failed`.
    pub const fn cause_certificate_index(self) -> Option<usize> {
        match self.state() {
            SessionState::Failed { .. } => Some(0),
            SessionState::Waiting | SessionState::Ready { .. } | SessionState::Stale { .. } => None,
        }
    }

    /// Сертификаты в каноническом порядке одного снимка.
    pub fn certificates(
        self,
    ) -> impl ExactSizeIterator<Item = CertificateV1<'a>> + FusedIterator + 'a {
        let (first, second) = match self.state() {
            SessionState::Waiting => (None, None),
            SessionState::Ready { current } | SessionState::Stale { previous: current } => {
                (Some(CertificateV1::verified(current)), None)
            }
            SessionState::Failed { cause, previous } => (
                Some(CertificateV1::conflict(cause)),
                previous.as_ref().map(CertificateV1::verified),
            ),
        };
        CertificatesV1::new(first, second)
    }
}

/// Нулевой lifetime-маркер точной пары Owner и неизменяемого снимка Session.
#[derive(Clone, Copy)]
struct BorrowScopeV1<'owner, 'session> {
    _scope: PhantomData<(&'owner OwnerV1, &'session SessionV1)>,
}

impl<'owner, 'session> BorrowScopeV1<'owner, 'session> {
    const fn new(_owner: &'owner OwnerV1, _session: &'session SessionV1) -> Self {
        Self {
            _scope: PhantomData,
        }
    }
}

/// Проверенная Owner-and-snapshot проекция evidence и операций.
#[derive(Clone, Copy)]
pub struct ProjectionV1<'owner, 'session> {
    evidence: EvidenceViewV1<'session>,
    owner: &'owner OwnerV1,
    scope: BorrowScopeV1<'owner, 'session>,
}

impl<'owner, 'session> ProjectionV1<'owner, 'session> {
    /// Возвращает историческое evidence этого снимка.
    pub const fn evidence(self) -> EvidenceViewV1<'session> {
        self.evidence
    }

    /// Возвращает полную каноническую последовательность операций состояния.
    pub fn operations(
        self,
    ) -> impl ExactSizeIterator<Item = OperationV1<'owner, 'session>> + FusedIterator {
        let inner = match self.evidence.state() {
            SessionState::Waiting
                if matches!(
                    self.evidence.session.raw_head(),
                    ObservationHeadViewV1::Empty
                ) =>
            {
                OperationSourceV1::Empty
            }
            SessionState::Ready { current } => {
                debug_assert_eq!(current.outputs().len(), self.owner.compiled.output_count());
                debug_assert!(
                    current
                        .outputs()
                        .iter()
                        .enumerate()
                        .all(|(index, output)| self.owner.compiled.output_slot_at(index)
                            == Some(output.output()))
                );
                OperationSourceV1::Set {
                    outputs: current.outputs().iter(),
                    certificate: VerifiedCertificateV1 { inner: current },
                    scope: self.scope,
                }
            }
            // `Waiting + Empty` — единственное состояние без действия и без
            // полномочий на sink. После admission сырой головы любое состояние
            // без текущего Verified-доказательства подчиняется одному закону
            // отзыва. Так же fail-closed обрабатывается внутренне недостижимое
            // сегодня сочетание `Waiting + Observed`.
            SessionState::Waiting | SessionState::Stale { .. } | SessionState::Failed { .. } => {
                OperationSourceV1::Remove {
                    slots: OwnerOutputSlotsV1::new(&self.owner.compiled),
                    scope: self.scope,
                }
            }
        };
        OperationsV1 { inner }
    }
}

/// Collision-resistant адрес канонического физического содержания Program.
///
/// Identity не идентифицирует owner-эпоху и не даёт runtime-полномочий.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentIdentityV1([u8; 32]);

impl ContentIdentityV1 {
    const fn from_core(value: ProgramContentIdentityV1) -> Self {
        Self(*value.as_bytes())
    }

    /// Возвращает 256-битное каноническое представление identity.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Доказательство прохождения всех hard-клеток на полном physical support.
#[derive(Clone, Copy)]
pub struct VerifiedCertificateV1<'a> {
    inner: &'a CoreVerifiedV1,
}

impl<'a> VerifiedCertificateV1<'a> {
    /// Возвращает identity скомпилированного содержания.
    pub const fn content_identity(self) -> ContentIdentityV1 {
        ContentIdentityV1::from_core(self.inner.report().content_identity())
    }

    /// Возвращает точное наблюдение, на котором выдан сертификат.
    pub const fn observation(self) -> ObservationV1<'a> {
        ObservationV1 {
            inner: self.inner.report().observation(),
        }
    }

    /// Возвращает индекс выбранного состояния или `None` для fixed Program.
    pub const fn selected_state_index(self) -> Option<usize> {
        self.inner.selected_state_index()
    }

    /// Возвращает все `case × constraint` клетки выбранного состояния.
    pub fn cells(self) -> impl ExactSizeIterator<Item = VerifiedCellV1<'a>> + FusedIterator + 'a {
        self.inner
            .report()
            .cells()
            .iter()
            .map(VerifiedCellV1::from_core)
    }

    /// Возвращает все сертифицированные выходы в каноническом порядке.
    pub fn outputs(
        self,
    ) -> impl ExactSizeIterator<Item = CertifiedOutputV1<'a>> + FusedIterator + 'a {
        self.inner
            .outputs()
            .iter()
            .map(CertifiedOutputV1::from_core)
    }
}

/// Исчерпывающее доказательство, что каждое состояние нарушает hard-клетку.
#[derive(Clone, Copy)]
pub struct ConflictCertificateV1<'a> {
    inner: &'a CoreConflictV1,
}

impl<'a> ConflictCertificateV1<'a> {
    /// Возвращает identity скомпилированного содержания.
    pub const fn content_identity(self) -> ContentIdentityV1 {
        ContentIdentityV1::from_core(self.inner.report().content_identity())
    }

    /// Возвращает точное наблюдение, вызвавшее конфликт.
    pub const fn observation(self) -> ObservationV1<'a> {
        ObservationV1 {
            inner: self.inner.report().observation(),
        }
    }

    /// Возвращает число исчерпывающе рассмотренных состояний.
    pub const fn considered_state_count(self) -> usize {
        self.inner.considered_state_count()
    }

    /// Возвращает все `state × case × constraint` клетки конфликта.
    pub fn cells(self) -> impl ExactSizeIterator<Item = ConflictCellV1<'a>> + FusedIterator + 'a {
        self.inner
            .report()
            .cells()
            .iter()
            .map(ConflictCellV1::from_core)
    }
}

/// Закрытая заимствованная проекция одного Core-owned сертификата.
///
/// Сертификат заимствует только историю Session и может пережить Owner,
/// разрешивший исходную проекцию.
///
/// ```no_run
/// use labcolors_core::program::{
///     CertificateV1, OwnerV1, SessionV1,
/// };
///
/// fn retain_evidence<'session>(
///     owner: OwnerV1,
///     session: &'session SessionV1,
/// ) -> CertificateV1<'session> {
///     owner
///         .project(session)
///         .unwrap()
///         .evidence()
///         .certificates()
///         .next()
///         .unwrap()
/// }
/// ```
#[derive(Clone, Copy)]
pub enum CertificateV1<'a> {
    /// Все hard-клетки полного support прошли.
    Verified(VerifiedCertificateV1<'a>),
    /// Каждое рассмотренное состояние нарушает хотя бы одну hard-клетку.
    Conflict(ConflictCertificateV1<'a>),
}

impl<'a> CertificateV1<'a> {
    const fn verified(value: &'a CoreVerifiedV1) -> Self {
        Self::Verified(VerifiedCertificateV1 { inner: value })
    }

    const fn conflict(value: &'a CoreConflictV1) -> Self {
        Self::Conflict(ConflictCertificateV1 { inner: value })
    }

    /// Возвращает identity скомпилированного содержания.
    pub const fn content_identity(self) -> ContentIdentityV1 {
        match self {
            Self::Verified(value) => value.content_identity(),
            Self::Conflict(value) => value.content_identity(),
        }
    }

    /// Возвращает точное revision-bound наблюдение сертификата.
    pub const fn observation(self) -> ObservationV1<'a> {
        match self {
            Self::Verified(value) => value.observation(),
            Self::Conflict(value) => value.observation(),
        }
    }

    #[cfg(test)]
    pub(crate) fn observation_backing_ptr_for_test(self) -> *const () {
        self.observation().inner.backing_ptr_for_test()
    }
}

/// Точное revision-bound наблюдение, сохранённое сертификатом.
#[derive(Clone, Copy)]
pub struct ObservationV1<'a> {
    inner: &'a crate::observation::RevisionBoundObservationV1,
}

impl<'a> ObservationV1<'a> {
    /// Возвращает stream provenance наблюдения.
    pub const fn stream(self) -> StreamIdV1 {
        StreamIdV1::from_core(self.inner.stream())
    }

    /// Возвращает ревизию наблюдения.
    pub const fn revision(self) -> u64 {
        self.inner.revision().value()
    }

    /// Возвращает каноническую schema, общую для всех физических cases.
    ///
    /// Позиция `i` соответствует позиции `i` в [`PhysicalCaseV1::values`].
    pub fn surface_input_ports(
        self,
    ) -> impl ExactSizeIterator<Item = SurfaceInputPortIdV1> + FusedIterator + 'a {
        self.inner
            .schema()
            .iter()
            .copied()
            .map(SurfaceInputPortIdV1::from_core)
    }

    /// Возвращает канонические уникальные физические cases.
    ///
    /// Дубликаты значений схлопываются, а их ID сохраняются в provenance.
    pub fn physical_cases(
        self,
    ) -> impl ExactSizeIterator<Item = PhysicalCaseV1<'a>> + FusedIterator + 'a {
        (0..self.inner.physical_case_count()).map(move |index| PhysicalCaseV1 {
            observation: self.inner,
            index,
        })
    }
}

/// Закрытое семейство сигналов физического case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalV1 {
    /// Encoded sRGB8 в IEC 61966-2-1 с белой точкой D65.
    Iec61966Srgb8D65(Srgb8),
}

/// Один канонический физический case и его полная provenance.
#[derive(Clone, Copy)]
pub struct PhysicalCaseV1<'a> {
    observation: &'a crate::observation::RevisionBoundObservationV1,
    index: usize,
}

impl<'a> PhysicalCaseV1<'a> {
    /// Возвращает значения case в каноническом schema order.
    pub fn values(self) -> impl ExactSizeIterator<Item = SignalV1> + FusedIterator + 'a {
        self.observation
            .physical_values(self.index)
            .expect("physical case originates from the same observation")
            .iter()
            .copied()
            .map(|signal| match signal.view() {
                ColorSignalViewV1::Iec61966Srgb8D65(value) => SignalV1::Iec61966Srgb8D65(value),
            })
    }

    /// Возвращает все scenario ID, схлопнутые в этот физический case.
    pub fn provenance(self) -> impl ExactSizeIterator<Item = ScenarioIdV1> + FusedIterator + 'a {
        self.observation
            .provenance(self.index)
            .expect("physical case originates from the same observation")
            .iter()
            .copied()
            .map(ScenarioIdV1::from_core)
    }
}

/// Роль одной constraint-клетки в выборе.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintModeV1 {
    /// Нарушение запрещает состояние.
    Hard,
    /// Результат сохраняется, но не влияет на выбор.
    ReportOnly,
}

/// Одна клетка `case × constraint` выбранного или fixed состояния.
#[derive(Clone, Copy)]
pub struct VerifiedCellV1<'a> {
    inner: &'a CoreProgramConstraintCellV1,
}

impl<'a> VerifiedCellV1<'a> {
    const fn from_core(inner: &'a CoreProgramConstraintCellV1) -> Self {
        Self { inner }
    }

    /// Возвращает индекс физического case.
    pub const fn case_index(self) -> usize {
        self.inner.case_index()
    }

    /// Возвращает ID ограничения.
    pub const fn constraint(self) -> ConstraintIdV1 {
        ConstraintIdV1::from_core(self.inner.constraint())
    }

    /// Возвращает ID проверенного Occurrence.
    pub const fn occurrence(self) -> OccurrenceIdV1 {
        OccurrenceIdV1::from_core(self.inner.target())
    }

    /// Возвращает роль ограничения в выборе.
    pub const fn mode(self) -> ConstraintModeV1 {
        project_constraint_mode(self.inner)
    }

    /// Возвращает типизированное сохранённое evidence.
    pub fn assessment(self) -> AssessmentV1<'a> {
        project_assessment(self.inner)
    }
}

/// Одна исчерпывающая клетка `state × case × constraint` конфликта.
#[derive(Clone, Copy)]
pub struct ConflictCellV1<'a> {
    inner: &'a CoreProgramConstraintCellV1,
}

impl<'a> ConflictCellV1<'a> {
    const fn from_core(inner: &'a CoreProgramConstraintCellV1) -> Self {
        Self { inner }
    }

    /// Возвращает индекс рассмотренного состояния.
    pub const fn state_index(self) -> usize {
        self.inner.candidate_state_index()
    }

    /// Возвращает индекс физического case.
    pub const fn case_index(self) -> usize {
        self.inner.case_index()
    }

    /// Возвращает ID ограничения.
    pub const fn constraint(self) -> ConstraintIdV1 {
        ConstraintIdV1::from_core(self.inner.constraint())
    }

    /// Возвращает ID проверенного Occurrence.
    pub const fn occurrence(self) -> OccurrenceIdV1 {
        OccurrenceIdV1::from_core(self.inner.target())
    }

    /// Возвращает роль ограничения в выборе.
    pub const fn mode(self) -> ConstraintModeV1 {
        project_constraint_mode(self.inner)
    }

    /// Возвращает типизированное сохранённое evidence.
    pub fn assessment(self) -> AssessmentV1<'a> {
        project_assessment(self.inner)
    }
}

const fn project_constraint_mode(cell: &CoreProgramConstraintCellV1) -> ConstraintModeV1 {
    if cell.is_hard() {
        ConstraintModeV1::Hard
    } else {
        ConstraintModeV1::ReportOnly
    }
}

fn project_assessment(cell: &CoreProgramConstraintCellV1) -> AssessmentV1<'_> {
    match cell.result() {
        ProgramConstraintResultV1::Pass(CoreProgramPassEvidenceV1::ExactSrgb8(evidence)) => {
            AssessmentV1::ExactSrgb8(ExactSrgb8EvidenceV1 {
                inner: ExactSrgb8EvidenceRefV1::Pass(evidence),
            })
        }
        ProgramConstraintResultV1::Violation(CoreProgramViolationEvidenceV1::ExactSrgb8(
            evidence,
        )) => AssessmentV1::ExactSrgb8(ExactSrgb8EvidenceV1 {
            inner: ExactSrgb8EvidenceRefV1::Violation(evidence),
        }),
        ProgramConstraintResultV1::Pass(CoreProgramPassEvidenceV1::Wcag22Srgb8(evidence)) => {
            AssessmentV1::Wcag22Srgb8(Wcag22Srgb8EvidenceV1 {
                inner: Wcag22Srgb8EvidenceRefV1::Pass(evidence),
            })
        }
        ProgramConstraintResultV1::Violation(CoreProgramViolationEvidenceV1::Wcag22Srgb8(
            evidence,
        )) => AssessmentV1::Wcag22Srgb8(Wcag22Srgb8EvidenceV1 {
            inner: Wcag22Srgb8EvidenceRefV1::Violation(evidence),
        }),
    }
}

/// Закрытое семейство сохранённого evaluator evidence.
#[derive(Clone, Copy)]
pub enum AssessmentV1<'a> {
    /// Evidence точного сравнения encoded sRGB8.
    ExactSrgb8(ExactSrgb8EvidenceV1<'a>),
    /// Evidence применимого критерия WCAG 2.2.
    Wcag22Srgb8(Wcag22Srgb8EvidenceV1<'a>),
}

impl<'a> AssessmentV1<'a> {
    /// Возвращает несовместимый с противоположным исход классификатора.
    pub const fn verdict(self) -> VerdictV1 {
        match self {
            Self::ExactSrgb8(value) => value.verdict(),
            Self::Wcag22Srgb8(value) => value.verdict(),
        }
    }

    /// Возвращает общую физическую и моделированную привязку точки.
    pub fn binding(self) -> PointBindingV1<'a> {
        match self {
            Self::ExactSrgb8(value) => value.binding(),
            Self::Wcag22Srgb8(value) => value.binding(),
        }
    }
}

/// Несовместимые сохранённые исходы классификатора.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictV1 {
    /// Критерий доказан.
    Pass,
    /// Критерий доказанно нарушен.
    Violation,
}

#[derive(Clone, Copy)]
enum ExactSrgb8EvidenceRefV1<'a> {
    Pass(&'a CoreExactPassEvidenceV1),
    Violation(&'a CoreExactViolationEvidenceV1),
}

/// Evidence точного sRGB8 сравнения с физикой и моделированным контекстом.
#[derive(Clone, Copy)]
pub struct ExactSrgb8EvidenceV1<'a> {
    inner: ExactSrgb8EvidenceRefV1<'a>,
}

impl<'a> ExactSrgb8EvidenceV1<'a> {
    /// Возвращает сохранённый исход классификатора.
    pub const fn verdict(self) -> VerdictV1 {
        match self.inner {
            ExactSrgb8EvidenceRefV1::Pass(_) => VerdictV1::Pass,
            ExactSrgb8EvidenceRefV1::Violation(_) => VerdictV1::Violation,
        }
    }

    /// Возвращает ожидаемый encoded sRGB8 результат.
    pub fn expected(self) -> Srgb8 {
        match self.inner {
            ExactSrgb8EvidenceRefV1::Pass(value) => value.target(),
            ExactSrgb8EvidenceRefV1::Violation(value) => value.target(),
        }
    }

    /// Возвращает физическую и моделированную привязку точки.
    pub fn binding(self) -> PointBindingV1<'a> {
        let value = match self.inner {
            ExactSrgb8EvidenceRefV1::Pass(value) => value.binding(),
            ExactSrgb8EvidenceRefV1::Violation(value) => value.binding(),
        };
        PointBindingV1 { inner: value }
    }
}

#[derive(Clone, Copy)]
enum Wcag22Srgb8EvidenceRefV1<'a> {
    Pass(&'a CoreWcag22PassEvidenceV1),
    Violation(&'a CoreWcag22ViolationEvidenceV1),
}

/// WCAG 2.2 evidence вместе с физикой и моделированным контекстом.
#[derive(Clone, Copy)]
pub struct Wcag22Srgb8EvidenceV1<'a> {
    inner: Wcag22Srgb8EvidenceRefV1<'a>,
}

impl<'a> Wcag22Srgb8EvidenceV1<'a> {
    /// Возвращает сохранённый исход классификатора.
    pub const fn verdict(self) -> VerdictV1 {
        match self.inner {
            Wcag22Srgb8EvidenceRefV1::Pass(_) => VerdictV1::Pass,
            Wcag22Srgb8EvidenceRefV1::Violation(_) => VerdictV1::Violation,
        }
    }

    /// Возвращает версию применённого WCAG 2.2 профиля.
    pub fn profile_id(self) -> Wcag22ProfileIdV1 {
        match self.inner {
            Wcag22Srgb8EvidenceRefV1::Pass(value) => value.measurement().value().profile_id(),
            Wcag22Srgb8EvidenceRefV1::Violation(value) => value.measurement().value().profile_id(),
        }
    }

    /// Возвращает применённый критерий.
    pub fn criterion(self) -> Wcag22CriterionV1 {
        match self.inner {
            Wcag22Srgb8EvidenceRefV1::Pass(value) => value.measurement().value().criterion(),
            Wcag22Srgb8EvidenceRefV1::Violation(value) => value.measurement().value().criterion(),
        }
    }

    /// Возвращает сертифицированные границы яркости foreground.
    pub fn foreground_luminance(self) -> Wcag22LuminanceBoundsQ55V1 {
        let measurement = match self.inner {
            Wcag22Srgb8EvidenceRefV1::Pass(value) => value.measurement().value().measurement(),
            Wcag22Srgb8EvidenceRefV1::Violation(value) => value.measurement().value().measurement(),
        };
        measurement.foreground_luminance
    }

    /// Возвращает сертифицированные границы яркости background.
    pub fn background_luminance(self) -> Wcag22LuminanceBoundsQ55V1 {
        let measurement = match self.inner {
            Wcag22Srgb8EvidenceRefV1::Pass(value) => value.measurement().value().measurement(),
            Wcag22Srgb8EvidenceRefV1::Violation(value) => value.measurement().value().measurement(),
        };
        measurement.background_luminance
    }

    /// Возвращает числовое доказательство устойчивости решения.
    pub fn numerical_evidence(self) -> &'a NumericalDecisionEvidenceV1 {
        match self.inner {
            Wcag22Srgb8EvidenceRefV1::Pass(value) => value.measurement().value().evidence(),
            Wcag22Srgb8EvidenceRefV1::Violation(value) => value.measurement().value().evidence(),
        }
    }

    /// Возвращает физическую и моделированную привязку точки.
    pub fn binding(self) -> PointBindingV1<'a> {
        let value = match self.inner {
            Wcag22Srgb8EvidenceRefV1::Pass(value) => value.binding(),
            Wcag22Srgb8EvidenceRefV1::Violation(value) => value.binding(),
        };
        PointBindingV1 { inner: value }
    }
}

/// Общая привязка физической композиции и моделированного tristimulus/context.
#[derive(Clone, Copy)]
pub struct PointBindingV1<'a> {
    inner: &'a ProgramVisiblePointBindingV1,
}

impl<'a> PointBindingV1<'a> {
    /// Возвращает закрытый тип точной физической композиции.
    pub const fn physical(self) -> PhysicalPointV1<'a> {
        match self.inner.physical().occurrence().profile() {
            CompositionProfileV1::EncodedSrgb8SourceOverV1 => {
                PhysicalPointV1::EncodedSrgb8SourceOver(EncodedSrgb8SourceOverV1 {
                    inner: self.inner,
                })
            }
        }
    }

    /// Возвращает закрытый тип допущенного моделированного сигнала.
    pub const fn modeled(self) -> ModeledPointV1<'a> {
        match self.inner.modeled_lcs().provenance().binding() {
            AdmittedSrgb8TristimulusBindingV1::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1 => {
                ModeledPointV1::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1(
                    ModeledTristimulusV1 { inner: self.inner },
                )
            }
        }
    }
}

/// Закрытое семейство точной физической композиции.
#[derive(Clone, Copy)]
pub enum PhysicalPointV1<'a> {
    /// Encoded-sRGB8 source-over композиция.
    EncodedSrgb8SourceOver(EncodedSrgb8SourceOverV1<'a>),
}

/// Точная привязка одного encoded-sRGB8 source-over Occurrence.
#[derive(Clone, Copy)]
pub struct EncodedSrgb8SourceOverV1<'a> {
    inner: &'a ProgramVisiblePointBindingV1,
}

impl EncodedSrgb8SourceOverV1<'_> {
    /// Возвращает ID накладываемого Paint.
    pub const fn subject_paint(self) -> PaintIdV1 {
        PaintIdV1::from_core(self.inner.physical().program_occurrence().subject())
    }

    /// Возвращает ID backdrop Surface.
    pub const fn backdrop_surface(self) -> SurfaceIdV1 {
        SurfaceIdV1::from_core(
            self.inner
                .physical()
                .program_occurrence()
                .backdrop_surface(),
        )
    }

    /// Возвращает исходный encoded sRGB8 subject до композиции.
    pub const fn subject(self) -> Srgb8 {
        Srgb8::new(self.inner.physical().occurrence().subject_rgb())
    }

    /// Возвращает точную прозрачность subject в `[0, 1]`.
    pub const fn opacity(self) -> f64 {
        f64::from_bits(self.inner.physical().occurrence().subject_opacity_bits())
    }

    /// Возвращает observed encoded sRGB8 backdrop.
    pub const fn backdrop(self) -> Srgb8 {
        Srgb8::new(self.inner.physical().occurrence().backdrop_rgb())
    }

    /// Возвращает видимый encoded sRGB8 результат композиции.
    pub const fn visible(self) -> Srgb8 {
        Srgb8::new(self.inner.physical().occurrence().output_rgb())
    }
}

/// Закрытое семейство provenance моделированного tristimulus.
#[derive(Clone, Copy)]
pub enum ModeledPointV1<'a> {
    /// IEC sRGB8 → CIE 1931 2° XYZ D65 с относительным `Y=1`.
    Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1(ModeledTristimulusV1<'a>),
}

/// Допущенный моделированный tristimulus и его контекст восприятия.
#[derive(Clone, Copy)]
pub struct ModeledTristimulusV1<'a> {
    inner: &'a ProgramVisiblePointBindingV1,
}

impl ModeledTristimulusV1<'_> {
    /// Возвращает относительные координаты CIE XYZ.
    pub fn xyz(self) -> [f64; 3] {
        self.inner.modeled_lcs().derivation().sample().xyz()
    }

    /// Возвращает явный контекст, использованный при моделировании.
    pub const fn appearance_context(self) -> AppearanceContextV1 {
        AppearanceContextV1(self.inner.modeled_lcs().occurrence().context())
    }
}

/// Один Core-сертифицированный выходной Paint.
#[derive(Clone, Copy)]
pub struct CertifiedOutputV1<'a> {
    inner: &'a ProgramOutputV1,
}

impl<'a> CertifiedOutputV1<'a> {
    const fn from_core(inner: &'a ProgramOutputV1) -> Self {
        Self { inner }
    }

    /// Возвращает клиентский выходной слот.
    pub const fn output_slot(self) -> OutputSlotIdV1 {
        OutputSlotIdV1::from_core((*self.inner).output())
    }

    /// Возвращает ID сертифицированного Paint.
    pub const fn paint(self) -> PaintIdV1 {
        PaintIdV1::from_core((*self.inner).paint().id())
    }

    /// Возвращает исходный encoded sRGB8 сигнал Paint.
    pub const fn source(self) -> Srgb8 {
        (*self.inner).paint().source()
    }

    /// Возвращает сертифицированную прозрачность Paint.
    pub const fn opacity(self) -> f64 {
        (*self.inner).paint().opacity().value()
    }
}

/// Операция установки, структурно связанная с точным Verified-сертификатом.
#[derive(Clone, Copy)]
pub struct SetV1<'owner, 'session> {
    output: &'session ProgramOutputV1,
    certificate: VerifiedCertificateV1<'session>,
    _scope: BorrowScopeV1<'owner, 'session>,
}

impl<'session> SetV1<'_, 'session> {
    /// Возвращает изменяемый клиентский выходной слот.
    pub const fn output_slot(self) -> OutputSlotIdV1 {
        OutputSlotIdV1::from_core((*self.output).output())
    }

    /// Возвращает исходный encoded sRGB8 сигнал результата.
    pub const fn source(self) -> Srgb8 {
        (*self.output).paint().source()
    }

    /// Возвращает прозрачность результата.
    pub const fn opacity(self) -> f64 {
        (*self.output).paint().opacity().value()
    }

    /// Возвращает сертификат, разрешивший эту операцию.
    pub const fn certificate(self) -> VerifiedCertificateV1<'session> {
        self.certificate
    }
}

/// Операция удаления результата без сертификата для текущего контекста.
#[derive(Clone, Copy)]
pub struct RemoveV1<'owner, 'session> {
    output_slot: OutputSlotIdV1,
    _scope: BorrowScopeV1<'owner, 'session>,
}

impl RemoveV1<'_, '_> {
    /// Возвращает удаляемый клиентский выходной слот.
    pub const fn output_slot(self) -> OutputSlotIdV1 {
        self.output_slot
    }
}

/// Полное закрытое множество операций над непрозрачными выходными слотами.
///
/// Каждый payload заимствует точные Owner и снимок Session. Скопированные
/// slot/source/opacity — только данные: runtime обязан перепроверить живую
/// пару непосредственно перед одним атомарным sink commit.
///
/// ```compile_fail,E0515
/// use labcolors_core::program::{
///     OperationV1, OwnerV1, RemoveV1,
///     SessionV1,
/// };
///
/// fn escape_remove<'session>(
///     owner: OwnerV1,
///     session: &'session SessionV1,
/// ) -> RemoveV1<'session, 'session> {
///     match owner.project(session).unwrap().operations().next().unwrap() {
///         OperationV1::Remove(remove) => remove,
///         _ => panic!("fixture supplies Remove"),
///     }
/// }
/// ```
///
/// ```compile_fail,E0515
/// use labcolors_core::program::{
///     OperationV1, OwnerV1, RemoveV1,
/// };
///
/// fn escape_local_session<'owner>(
///     owner: &'owner OwnerV1,
/// ) -> RemoveV1<'owner, 'owner> {
///     let session = owner.instantiate(1).unwrap();
///     match owner.project(&session).unwrap().operations().next().unwrap() {
///         OperationV1::Remove(remove) => remove,
///         _ => panic!("fixture supplies Remove"),
///     }
/// }
/// ```
///
/// ```compile_fail,E0515
/// use labcolors_core::program::{
///     OperationV1, OwnerV1, SessionV1,
///     SetV1,
/// };
///
/// fn escape_set<'session>(
///     owner: OwnerV1,
///     session: &'session SessionV1,
/// ) -> SetV1<'session, 'session> {
///     match owner.project(session).unwrap().operations().next().unwrap() {
///         OperationV1::Set(set) => set,
///         _ => panic!("fixture supplies Set"),
///     }
/// }
/// ```
///
/// ```compile_fail,E0502
/// use labcolors_core::program::{
///     OperationV1, OwnerV1, SessionV1,
///     UpdateV1,
/// };
///
/// fn remove_blocks_session_mutation(
///     owner: &OwnerV1,
///     session: &mut SessionV1,
/// ) {
///     let remove = match owner.project(session).unwrap().operations().next().unwrap() {
///         OperationV1::Remove(remove) => remove,
///         _ => return,
///     };
///     let _second = owner.update(
///         session,
///         UpdateV1::Unknown {
///             revision: 2,
///             reason_id: 7,
///         },
///     );
///     let _slot = remove.output_slot();
/// }
/// ```
#[derive(Clone, Copy)]
pub enum OperationV1<'owner, 'session> {
    /// Установить сертифицированный результат.
    Set(SetV1<'owner, 'session>),
    /// Удалить результат, когда текущий контекст не сертифицирован.
    Remove(RemoveV1<'owner, 'session>),
}

struct CertificatesV1<'a> {
    values: [Option<CertificateV1<'a>>; 2],
    index: usize,
}

impl<'a> CertificatesV1<'a> {
    fn new(first: Option<CertificateV1<'a>>, second: Option<CertificateV1<'a>>) -> Self {
        Self {
            values: [first, second],
            index: 0,
        }
    }
}

impl<'a> Iterator for CertificatesV1<'a> {
    type Item = CertificateV1<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.values.len() {
            let value = self.values[self.index];
            self.index += 1;
            if value.is_some() {
                return value;
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.values[self.index..]
            .iter()
            .filter(|value| value.is_some())
            .count();
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for CertificatesV1<'_> {}
impl FusedIterator for CertificatesV1<'_> {}

struct OwnerOutputSlotsV1<'owner> {
    compiled: &'owner CompiledCoreProgramV1,
    index: usize,
    len: usize,
}

impl<'owner> OwnerOutputSlotsV1<'owner> {
    fn new(compiled: &'owner CompiledCoreProgramV1) -> Self {
        Self {
            compiled,
            index: 0,
            len: compiled.output_count(),
        }
    }
}

impl Iterator for OwnerOutputSlotsV1<'_> {
    type Item = OutputSlotIdV1;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.len {
            return None;
        }
        let output = self.compiled.output_slot_at(self.index)?;
        self.index += 1;
        Some(OutputSlotIdV1::from_core(output))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len - self.index;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for OwnerOutputSlotsV1<'_> {}
impl FusedIterator for OwnerOutputSlotsV1<'_> {}

enum OperationSourceV1<'owner, 'session> {
    Empty,
    Set {
        outputs: slice::Iter<'session, ProgramOutputV1>,
        certificate: VerifiedCertificateV1<'session>,
        scope: BorrowScopeV1<'owner, 'session>,
    },
    Remove {
        slots: OwnerOutputSlotsV1<'owner>,
        scope: BorrowScopeV1<'owner, 'session>,
    },
}

struct OperationsV1<'owner, 'session> {
    inner: OperationSourceV1<'owner, 'session>,
}

impl<'owner, 'session> Iterator for OperationsV1<'owner, 'session> {
    type Item = OperationV1<'owner, 'session>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            OperationSourceV1::Empty => None,
            OperationSourceV1::Set {
                outputs,
                certificate,
                scope,
            } => {
                let output = outputs.next()?;
                Some(OperationV1::Set(SetV1 {
                    output,
                    certificate: *certificate,
                    _scope: *scope,
                }))
            }
            OperationSourceV1::Remove { slots, scope } => Some(OperationV1::Remove(RemoveV1 {
                output_slot: slots.next()?,
                _scope: *scope,
            })),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = match &self.inner {
            OperationSourceV1::Empty => 0,
            OperationSourceV1::Set { outputs, .. } => outputs.len(),
            OperationSourceV1::Remove { slots, .. } => slots.len(),
        };
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for OperationsV1<'_, '_> {}
impl FusedIterator for OperationsV1<'_, '_> {}

/// Закрытая классификация ошибки создания Session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstantiateErrorKindV1 {
    /// Для создания Session недостаточно ресурсов.
    ResourceExhausted,
    /// Нарушен внутренний инвариант скомпилированной Program.
    InternalInvariant,
}

/// Непрозрачная ошибка создания Session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstantiateErrorV1 {
    kind: InstantiateErrorKindV1,
}

impl InstantiateErrorV1 {
    const fn new(kind: InstantiateErrorKindV1) -> Self {
        Self { kind }
    }

    fn from_core(error: ProgramSessionInstantiateError) -> Self {
        let kind = match error {
            ProgramSessionInstantiateError::ResourceExhausted => {
                InstantiateErrorKindV1::ResourceExhausted
            }
            ProgramSessionInstantiateError::InternalInvariant => {
                InstantiateErrorKindV1::InternalInvariant
            }
        };
        Self::new(kind)
    }

    /// Возвращает стабильный класс ошибки.
    pub const fn kind(self) -> InstantiateErrorKindV1 {
        self.kind
    }
}

impl From<InstantiateErrorKindV1> for InstantiateErrorV1 {
    fn from(kind: InstantiateErrorKindV1) -> Self {
        Self::new(kind)
    }
}

/// Закрытая классификация ошибки одного атомарного update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateErrorKindV1 {
    /// Session принадлежит другой точной owner-эпохе.
    OwnerMismatch,
    /// Наблюдение нарушает скомпилированную schema.
    InvalidObservation,
    /// Ревизия старше уже принятой.
    RevisionOutOfOrder,
    /// Та же ревизия содержит другой payload.
    RevisionConflict,
    /// Для admission или вычисления недостаточно ресурсов.
    ResourceExhausted,
    /// Зарегистрированный evaluator не смог выполнить оценку.
    EvaluationFailed,
    /// Нарушен внутренний инвариант.
    InternalInvariant,
}

/// Фаза update, в которой закончился ограниченный ресурс.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePhaseV1 {
    /// Admission и канонизация физического наблюдения.
    ObservationAdmission,
    /// Вычисление, поиск и финальная перепроверка Program.
    ProgramEvaluation,
}

/// Точный отказ зарегистрированного evaluator-а.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvaluatorFailureV1 {
    /// Отказ зарегистрированного WCAG 2.2 evaluator-а.
    Wcag22Srgb8 {
        /// Версия evaluator-а и его численного доказательства.
        profile_id: Wcag22ProfileIdV1,
        /// Точная исходная ошибка WCAG 2.2 без строковой переклассификации.
        source: Wcag22EvaluationErrorV1,
    },
}

/// Точная причина расхождения observation со скомпилированным binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationBindingFailureV1 {
    /// Скомпилированная schema не содержит ни одного входного порта.
    EmptyCompiledSurfaceInputSchema,
    /// Один входной порт повторён в скомпилированной schema.
    DuplicateCompiledSurfaceInputPort {
        /// Повторённый непрозрачный ID порта.
        input: SurfaceInputPortIdV1,
    },
    /// Update относится к другому observation stream.
    StreamMismatch {
        /// Stream, принадлежащий Session.
        expected: StreamIdV1,
        /// Stream отвергнутого update.
        actual: StreamIdV1,
    },
    /// Один входной порт повторён внутри физического сценария.
    DuplicateSurfaceInputBinding {
        /// Provenance ошибочного сценария.
        scenario: ScenarioIdV1,
        /// Повторённый входной порт.
        input: SurfaceInputPortIdV1,
    },
    /// В физическом сценарии отсутствует обязательный входной порт.
    MissingSurfaceInputBinding {
        /// Provenance ошибочного сценария.
        scenario: ScenarioIdV1,
        /// Отсутствующий входной порт.
        input: SurfaceInputPortIdV1,
    },
    /// Физический сценарий содержит неизвестный входной порт.
    UnexpectedSurfaceInputBinding {
        /// Provenance ошибочного сценария.
        scenario: ScenarioIdV1,
        /// Неизвестный входной порт.
        input: SurfaceInputPortIdV1,
    },
    /// Канонический порядок портов разошёлся со скомпилированной schema.
    SchemaMismatch {
        /// Индекс физического сценария.
        case_index: usize,
        /// Первый несовпавший индекс binding.
        binding_index: usize,
        /// Ожидаемый порт либо конец скомпилированной schema.
        expected: Option<SurfaceInputPortIdV1>,
        /// Фактический порт либо конец observation schema.
        actual: Option<SurfaceInputPortIdV1>,
    },
}

/// Зарегистрированная identity XYZ-frame в диагностике закрытого Core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorimetricFrameV1 {
    /// CIE 1931 2°, IEC 61966-2-1 D65, относительная шкала `Y=1`, XYZ v1.
    Iec61966Srgb8D65XyzRelativeY1V1,
    /// Зарезервированный frame hostile-теста, недостижимый в production.
    #[cfg(test)]
    MutationSentinelV1,
}

/// Компонент XYZ в точной диагностике.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TristimulusComponentV1 {
    /// Компонент X.
    X,
    /// Компонент Y.
    Y,
    /// Компонент Z.
    Z,
}

/// Точная конечная XYZ-точка и её зарегистрированный frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TristimulusSampleV1 {
    /// Биты IEEE-754 сохраняют точный диагностический payload и отделяют
    /// равенство записи от семантики сравнения floating-point.
    xyz_bits: [u64; 3],
    frame: ColorimetricFrameV1,
}

impl TristimulusSampleV1 {
    fn from_core(sample: TristimulusSample) -> Self {
        Self {
            xyz_bits: sample.xyz().map(f64::to_bits),
            frame: map_colorimetric_frame(sample.frame()),
        }
    }

    /// Возвращает точные конечные XYZ-компоненты.
    pub fn xyz(self) -> [f64; 3] {
        self.xyz_bits.map(f64::from_bits)
    }

    /// Возвращает зарегистрированный frame точки.
    pub const fn frame(self) -> ColorimetricFrameV1 {
        self.frame
    }
}

/// Точная причина, по которой Core не сформировал modeled LCS occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeledOccurrenceFailureV1 {
    /// Детерминированное преобразование получило недопустимую XYZ-компоненту.
    Tristimulus {
        /// Ошибочная компонента.
        component: TristimulusComponentV1,
        /// Точная числовая причина.
        reason: NumericDomainErrorV1,
    },
    /// Stimulus и appearance context принадлежат разным frame.
    FrameMismatch {
        /// Frame modeled stimulus.
        stimulus: ColorimetricFrameV1,
        /// Frame appearance context.
        context: ColorimetricFrameV1,
    },
    /// Повторное вычисление provenance получило недопустимую XYZ-компоненту.
    ProvenanceReplayFailed {
        /// Ошибочная компонента.
        component: TristimulusComponentV1,
        /// Точная числовая причина.
        reason: NumericDomainErrorV1,
    },
    /// Записанная modeled-точка не совпала с повторным вычислением provenance.
    RecordedSampleDoesNotReplay {
        /// Записанная точка.
        recorded: TristimulusSampleV1,
        /// Повторно вычисленная точка.
        replayed: TristimulusSampleV1,
    },
    /// Точка occurrence не совпала с modeled provenance.
    OccurrenceSampleMismatch {
        /// Точка occurrence.
        occurrence: TristimulusSampleV1,
        /// Точка modeled provenance.
        modeled: TristimulusSampleV1,
    },
}

/// Точное недопустимое protocol-состояние зарегистрированного evaluator-а.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvaluatorProtocolFailureV1 {
    /// WCAG evaluator вернул kernel-ошибку, недостижимую для typed Program.
    Wcag22Kernel {
        /// Версия evaluator-а и его численного доказательства.
        profile_id: Wcag22ProfileIdV1,
        /// Точная исходная kernel-ошибка.
        source: Wcag22EvaluationErrorV1,
    },
    /// Hard-вызов получил только клиентскую декларацию неприменимости.
    Wcag22ReportOnly {
        /// Версия evaluator-а и его численного доказательства.
        profile_id: Wcag22ProfileIdV1,
        /// Точная клиентская декларация.
        declaration: Wcag22ClientDeclaredNotApplicableV1,
    },
    /// Evaluator проверил другой критерий.
    Wcag22CriterionMismatch {
        /// Запрошенный критерий.
        requested: Wcag22CriterionV1,
        /// Фактически проверенный критерий.
        evaluated: Wcag22CriterionV1,
    },
}

/// Машиночитаемая identity нарушенного внутреннего контракта.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateInvariantV1 {
    /// Заимствованный matching Owner не удержал свою эпоху живой.
    OwnerAuthority,
    /// Каноническая observation schema разошлась со скомпилированным binding.
    ObservationBinding,
    /// Сохранённое evidence не принадлежит допускаемому observation.
    EvidenceBinding,
    /// Applicable evaluator вернул недопустимое protocol-состояние.
    EvaluatorProtocol,
    /// Physical и modeled точки одного evaluator-вызова разошлись.
    PhysicalModeledBinding,
    /// Зарегистрированный сигнал не сформировал свой LCS occurrence.
    ModeledOccurrenceFormation,
    /// Один выбранный state дал разные выходы в физических сценариях.
    OutputCaseInvariance,
    /// Детерминированная финальная перепроверка разошлась с поиском.
    SelectionRecheck,
    /// Закрытая программа нарушила собственную структуру исполнения.
    ProgramEvaluation,
}

/// Нарушенный внутренний контракт с точными subject и witness-фактами.
///
/// Эти варианты недостижимы через типизированный public input. Payload нужен
/// для детерминированной диагностики и не превращает breach в цветовой verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateInvariantFailureV1 {
    /// Заимствованный matching Owner не удержал свою эпоху живой.
    OwnerAuthority,
    /// Каноническая observation schema разошлась со скомпилированным binding.
    ObservationBinding {
        /// Точное расхождение schema или stream.
        source: ObservationBindingFailureV1,
    },
    /// Сохранённое evidence не принадлежит допускаемому observation.
    EvidenceBinding,
    /// Applicable evaluator вернул недопустимое protocol-состояние.
    EvaluatorProtocol {
        /// Индекс физического сценария.
        case_index: usize,
        /// Непрозрачный ID проверяемого ограничения.
        constraint: ConstraintIdV1,
        /// Непрозрачный ID физического occurrence.
        occurrence: OccurrenceIdV1,
        /// Точный appearance context evaluator-вызова.
        context: AppearanceContextV1,
        /// Недопустимое protocol-состояние.
        source: EvaluatorProtocolFailureV1,
    },
    /// Physical и modeled точки одного evaluator-вызова разошлись.
    PhysicalModeledBinding {
        /// Индекс физического сценария.
        case_index: usize,
        /// Непрозрачный ID проверяемого ограничения.
        constraint: ConstraintIdV1,
        /// Непрозрачный ID физического occurrence.
        occurrence: OccurrenceIdV1,
        /// Точный appearance context evaluator-вызова.
        context: AppearanceContextV1,
        /// Физическая encoded sRGB8-точка.
        physical: Srgb8,
        /// Modeled encoded sRGB8-точка.
        modeled: Srgb8,
    },
    /// Зарегистрированный сигнал не сформировал свой LCS occurrence.
    ModeledOccurrenceFormation {
        /// Индекс физического сценария.
        case_index: usize,
        /// Непрозрачный ID occurrence.
        occurrence: OccurrenceIdV1,
        /// Intended appearance context формирования.
        context: AppearanceContextV1,
        /// Точная причина отказа формирования.
        source: ModeledOccurrenceFailureV1,
    },
    /// Один выбранный state дал разные выходы в физических сценариях.
    OutputCaseInvariance {
        /// Непрозрачный ID выходного слота.
        output: OutputSlotIdV1,
        /// Первый сценарий, задавший ожидаемое значение.
        first_case: usize,
        /// Сценарий с отличающимся значением.
        actual_case: usize,
    },
    /// Детерминированная финальная перепроверка разошлась с поиском.
    SelectionRecheck {
        /// Индекс выбранного joint state.
        state_index: usize,
        /// Индекс физического сценария.
        case_index: usize,
        /// Непрозрачный ID ограничения.
        constraint: ConstraintIdV1,
        /// Непрозрачный ID occurrence.
        occurrence: OccurrenceIdV1,
        /// Число hard-нарушений на финальной перепроверке.
        hard_violation_count: usize,
    },
    /// Закрытая программа нарушила собственную структуру исполнения.
    ProgramEvaluation,
}

impl UpdateInvariantFailureV1 {
    /// Возвращает стабильную identity нарушенного контракта.
    pub const fn contract(&self) -> UpdateInvariantV1 {
        match self {
            Self::OwnerAuthority => UpdateInvariantV1::OwnerAuthority,
            Self::ObservationBinding { .. } => UpdateInvariantV1::ObservationBinding,
            Self::EvidenceBinding => UpdateInvariantV1::EvidenceBinding,
            Self::EvaluatorProtocol { .. } => UpdateInvariantV1::EvaluatorProtocol,
            Self::PhysicalModeledBinding { .. } => UpdateInvariantV1::PhysicalModeledBinding,
            Self::ModeledOccurrenceFormation { .. } => {
                UpdateInvariantV1::ModeledOccurrenceFormation
            }
            Self::OutputCaseInvariance { .. } => UpdateInvariantV1::OutputCaseInvariance,
            Self::SelectionRecheck { .. } => UpdateInvariantV1::SelectionRecheck,
            Self::ProgramEvaluation => UpdateInvariantV1::ProgramEvaluation,
        }
    }
}

/// Точная ошибка одного атомарного update.
///
/// Любой variant оставляет observation head и lifecycle-состояние Core
/// неизменными. [`UpdateErrorKindV1`] — только удобная производная проекция:
/// авторитетные IDs и факты отказа находятся в этом enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateErrorV1 {
    /// Session создана другой точной owner-эпохой.
    OwnerMismatch,
    /// Наблюдение не содержит ни одного физического сценария.
    EmptyScenarioSet,
    /// Один scenario ID повторён внутри наблюдения.
    DuplicateScenarioId {
        /// Повторённая непрозрачная provenance.
        scenario: ScenarioIdV1,
    },
    /// Число значений сценария не равно скомпилированной schema.
    ScenarioValueCountMismatch {
        /// Непрозрачная provenance ошибочного сценария.
        scenario: ScenarioIdV1,
        /// Число входов в скомпилированной schema.
        expected: usize,
        /// Фактическое число переданных значений.
        actual: usize,
    },
    /// Входная ревизия старше уже принятой.
    RevisionOutOfOrder {
        /// Текущая принятая ревизия.
        current: u64,
        /// Отвергнутая входная ревизия.
        incoming: u64,
    },
    /// Та же ревизия содержит другой payload.
    RevisionConflict {
        /// Ревизия с неоднозначным содержанием.
        revision: u64,
    },
    /// Ограниченный ресурс закончился до commit.
    ResourceExhausted {
        /// Фаза, которой не хватило ресурса.
        phase: UpdatePhaseV1,
    },
    /// Зарегистрированный evaluator не смог выполнить оценку.
    EvaluationFailed {
        /// Индекс физического сценария в каноническом наблюдении.
        case_index: usize,
        /// Непрозрачный ID проверяемого ограничения.
        constraint: ConstraintIdV1,
        /// Непрозрачный ID физического occurrence.
        occurrence: OccurrenceIdV1,
        /// Точный допущенный appearance context evaluator-вызова.
        context: AppearanceContextV1,
        /// Точная причина отказа и identity evaluator-а.
        source: EvaluatorFailureV1,
    },
    /// Нарушен внутренний контракт закрытого Core.
    InternalInvariant {
        /// Точный факт нарушения; identity выводится через [`UpdateInvariantFailureV1::contract`].
        source: UpdateInvariantFailureV1,
    },
}

impl UpdateErrorV1 {
    /// Возвращает стабильный класс ошибки без потери её payload.
    pub const fn kind(&self) -> UpdateErrorKindV1 {
        match self {
            Self::OwnerMismatch => UpdateErrorKindV1::OwnerMismatch,
            Self::EmptyScenarioSet
            | Self::DuplicateScenarioId { .. }
            | Self::ScenarioValueCountMismatch { .. } => UpdateErrorKindV1::InvalidObservation,
            Self::RevisionOutOfOrder { .. } => UpdateErrorKindV1::RevisionOutOfOrder,
            Self::RevisionConflict { .. } => UpdateErrorKindV1::RevisionConflict,
            Self::ResourceExhausted { .. } => UpdateErrorKindV1::ResourceExhausted,
            Self::EvaluationFailed { .. } => UpdateErrorKindV1::EvaluationFailed,
            Self::InternalInvariant { .. } => UpdateErrorKindV1::InternalInvariant,
        }
    }
}

fn map_joint_order_error(error: FiniteJointOrderErrorV1) -> JointOrderErrorV1 {
    match error {
        FiniteJointOrderErrorV1::EmptyDomain { dimension } => {
            JointOrderErrorV1::EmptyDomain { dimension }
        }
        FiniteJointOrderErrorV1::CardinalityOverflow => JointOrderErrorV1::CardinalityOverflow,
        FiniteJointOrderErrorV1::EmptyOrder => JointOrderErrorV1::EmptyOrder,
        FiniteJointOrderErrorV1::TupleArity {
            tuple,
            expected,
            actual,
        } => JointOrderErrorV1::TupleArity {
            state: tuple,
            expected,
            actual,
        },
        FiniteJointOrderErrorV1::OrdinalOutOfDomain {
            tuple,
            dimension,
            ordinal,
            domain_len,
        } => JointOrderErrorV1::OrdinalOutOfDomain {
            state: tuple,
            dimension,
            ordinal,
            domain_len,
        },
        FiniteJointOrderErrorV1::DuplicateTuple { first, duplicate } => {
            JointOrderErrorV1::DuplicateTuple {
                first_state: first,
                duplicate_state: duplicate,
            }
        }
        FiniteJointOrderErrorV1::IncompleteOrder { expected, actual } => {
            JointOrderErrorV1::IncompleteOrder { expected, actual }
        }
        FiniteJointOrderErrorV1::ResourceExhausted => JointOrderErrorV1::ResourceExhausted,
    }
}

fn map_program_compile_error(error: ProgramCompileError) -> CompileErrorV1 {
    match error {
        ProgramCompileError::DuplicateSource { source } => CompileErrorV1::DuplicateSource {
            source: SourceIdV1::from_core(source),
        },
        ProgramCompileError::DuplicateTarget { target } => CompileErrorV1::DuplicateTarget {
            target: TargetIdV1::from_core(target),
        },
        ProgramCompileError::MissingTargetSource { target, source } => {
            CompileErrorV1::MissingTargetSource {
                target: TargetIdV1::from_core(target),
                source: SourceIdV1::from_core(source),
            }
        }
        ProgramCompileError::DuplicateOpacityInput { input } => {
            CompileErrorV1::DuplicateOpacityInput {
                input: OpacityInputIdV1::from_core(input),
            }
        }
        ProgramCompileError::DuplicateSurfaceInputPort { input } => {
            CompileErrorV1::DuplicateSurfaceInputPort {
                input: SurfaceInputPortIdV1::from_core(input),
            }
        }
        ProgramCompileError::UnusedSurfaceInputPort { input } => {
            CompileErrorV1::UnusedSurfaceInputPort {
                input: SurfaceInputPortIdV1::from_core(input),
            }
        }
        ProgramCompileError::DuplicateSurfaceInputBinding {
            input,
            first,
            duplicate,
        } => CompileErrorV1::DuplicateSurfaceInputBinding {
            input: SurfaceInputPortIdV1::from_core(input),
            first: SurfaceIdV1::from_core(first),
            duplicate: SurfaceIdV1::from_core(duplicate),
        },
        ProgramCompileError::DuplicatePaint { paint } => CompileErrorV1::DuplicatePaint {
            paint: PaintIdV1::from_core(paint),
        },
        ProgramCompileError::DuplicateSurface { surface } => CompileErrorV1::DuplicateSurface {
            surface: SurfaceIdV1::from_core(surface),
        },
        ProgramCompileError::DuplicateOccurrence { occurrence } => {
            CompileErrorV1::DuplicateOccurrence {
                occurrence: OccurrenceIdV1::from_core(occurrence),
            }
        }
        ProgramCompileError::MissingPaintTarget { paint, target } => {
            CompileErrorV1::MissingPaintTarget {
                paint: PaintIdV1::from_core(paint),
                target: TargetIdV1::from_core(target),
            }
        }
        ProgramCompileError::MissingPaintSource { paint, source } => {
            CompileErrorV1::MissingPaintSource {
                paint: PaintIdV1::from_core(paint),
                source: PaintIdV1::from_core(source),
            }
        }
        ProgramCompileError::MissingPaintOpacityInput { paint, input } => {
            CompileErrorV1::MissingPaintOpacityInput {
                paint: PaintIdV1::from_core(paint),
                input: OpacityInputIdV1::from_core(input),
            }
        }
        ProgramCompileError::MissingSurfaceInputPort { surface, input } => {
            CompileErrorV1::MissingSurfaceInputPort {
                surface: SurfaceIdV1::from_core(surface),
                input: SurfaceInputPortIdV1::from_core(input),
            }
        }
        ProgramCompileError::MissingSurfaceOccurrence {
            surface,
            occurrence,
        } => CompileErrorV1::MissingSurfaceOccurrence {
            surface: SurfaceIdV1::from_core(surface),
            occurrence: OccurrenceIdV1::from_core(occurrence),
        },
        ProgramCompileError::MissingOccurrencePaint { occurrence, paint } => {
            CompileErrorV1::MissingOccurrencePaint {
                occurrence: OccurrenceIdV1::from_core(occurrence),
                paint: PaintIdV1::from_core(paint),
            }
        }
        ProgramCompileError::MissingOccurrenceBackdrop {
            occurrence,
            surface,
        } => CompileErrorV1::MissingOccurrenceBackdrop {
            occurrence: OccurrenceIdV1::from_core(occurrence),
            surface: SurfaceIdV1::from_core(surface),
        },
        ProgramCompileError::PaintCycle { paints } => {
            CompileErrorV1::PaintCycle(PaintCycleV1 { paints })
        }
        ProgramCompileError::RenderCycle {
            surfaces,
            occurrences,
        } => CompileErrorV1::RenderCycle(RenderCycleV1 {
            surfaces,
            occurrences,
        }),
        ProgramCompileError::OpacityOutOfDomain { input } => CompileErrorV1::OpacityOutOfDomain {
            input: OpacityInputIdV1::from_core(input),
        },
        ProgramCompileError::EmptyTargetDomain { target } => CompileErrorV1::EmptyTargetDomain {
            target: TargetIdV1::from_core(target),
        },
        ProgramCompileError::DuplicateTargetCandidate { target, candidate } => {
            CompileErrorV1::DuplicateTargetCandidate {
                target: TargetIdV1::from_core(target),
                candidate: TargetCandidateIdV1::from_core(candidate),
            }
        }
        ProgramCompileError::DuplicateTargetCandidateSignal {
            target,
            first,
            duplicate,
            signal,
        } => CompileErrorV1::DuplicateTargetCandidateSignal {
            target: TargetIdV1::from_core(target),
            first: TargetCandidateIdV1::from_core(first),
            duplicate: TargetCandidateIdV1::from_core(duplicate),
            encoded_srgb8: signal.srgb8(),
        },
        ProgramCompileError::UnconstrainedTarget { target } => {
            CompileErrorV1::UnconstrainedTarget {
                target: TargetIdV1::from_core(target),
            }
        }
        ProgramCompileError::DisconnectedFiniteTargets => CompileErrorV1::DisconnectedFiniteTargets,
        ProgramCompileError::UnassessedOutput { output, paint } => {
            CompileErrorV1::UnassessedOutput {
                output: OutputSlotIdV1::from_core(output),
                paint: PaintIdV1::from_core(paint),
            }
        }
        ProgramCompileError::MissingJointSelection => CompileErrorV1::MissingJointSelection,
        ProgramCompileError::JointSelectionWithoutTargets => {
            CompileErrorV1::JointSelectionWithoutTargets
        }
        ProgramCompileError::JointStateDuplicateTarget { state, target } => {
            CompileErrorV1::JointStateDuplicateTarget {
                state,
                target: TargetIdV1::from_core(target),
            }
        }
        ProgramCompileError::JointStateMissingTarget { state, target } => {
            CompileErrorV1::JointStateMissingTarget {
                state,
                target: TargetIdV1::from_core(target),
            }
        }
        ProgramCompileError::JointStateUnknownTarget { state, target } => {
            CompileErrorV1::JointStateUnknownTarget {
                state,
                target: TargetIdV1::from_core(target),
            }
        }
        ProgramCompileError::JointStateUnknownCandidate {
            state,
            target,
            candidate,
        } => CompileErrorV1::JointStateUnknownCandidate {
            state,
            target: TargetIdV1::from_core(target),
            candidate: TargetCandidateIdV1::from_core(candidate),
        },
        ProgramCompileError::InvalidJointOrder(error) => {
            CompileErrorV1::InvalidJointOrder(map_joint_order_error(error))
        }
        ProgramCompileError::EmptyObservationGroup { .. } => {
            CompileErrorV1::EmptySurfaceInputPortSet
        }
        ProgramCompileError::EmptyOccurrenceSet => CompileErrorV1::EmptyOccurrenceSet,
        ProgramCompileError::EmptyConstraintSet => CompileErrorV1::EmptyConstraintSet,
        ProgramCompileError::EmptyOutputSet => CompileErrorV1::EmptyOutputSet,
        ProgramCompileError::DuplicateConstraint { constraint } => {
            CompileErrorV1::DuplicateConstraint {
                constraint: ConstraintIdV1::from_core(constraint),
            }
        }
        ProgramCompileError::MissingConstraintOccurrence {
            constraint,
            occurrence,
        } => CompileErrorV1::MissingConstraintOccurrence {
            constraint: ConstraintIdV1::from_core(constraint),
            occurrence: OccurrenceIdV1::from_core(occurrence),
        },
        ProgramCompileError::DuplicateOutputSlot { output } => {
            CompileErrorV1::DuplicateOutputSlot {
                output: OutputSlotIdV1::from_core(output),
            }
        }
        ProgramCompileError::MissingOutputPaint { output, paint } => {
            CompileErrorV1::MissingOutputPaint {
                output: OutputSlotIdV1::from_core(output),
                paint: PaintIdV1::from_core(paint),
            }
        }
        ProgramCompileError::ResourceExhausted => CompileErrorV1::ResourceExhausted,
        ProgramCompileError::InternalInvariant => CompileErrorV1::InternalInvariant,
    }
}

const fn map_colorimetric_frame(frame: ColorimetricFrameId) -> ColorimetricFrameV1 {
    match (
        frame.observer(),
        frame.reference_white(),
        frame.scale(),
        frame.release(),
    ) {
        (
            ObserverProfileId::Cie1931TwoDegreeV1,
            ReferenceWhiteId::Iec61966D65ChromaticityV1,
            TristimulusScale::RelativeY1,
            ColorimetricFrameReleaseId::XyzV1,
        ) => ColorimetricFrameV1::Iec61966Srgb8D65XyzRelativeY1V1,
        #[cfg(test)]
        (
            ObserverProfileId::Cie1931TwoDegreeV1,
            ReferenceWhiteId::Iec61966D65ChromaticityV1,
            TristimulusScale::RelativeY1,
            ColorimetricFrameReleaseId::MutationSentinelV1,
        ) => ColorimetricFrameV1::MutationSentinelV1,
    }
}

const fn map_numeric_domain_error(error: NumericDomainError) -> NumericDomainErrorV1 {
    match error {
        NumericDomainError::NonFinite => NumericDomainErrorV1::NonFinite,
        NumericDomainError::Negative => NumericDomainErrorV1::Negative,
        NumericDomainError::NotPositive => NumericDomainErrorV1::NotPositive,
        NumericDomainError::AboveOne => NumericDomainErrorV1::AboveOne,
        NumericDomainError::HueOutOfRange => NumericDomainErrorV1::HueOutOfRange,
    }
}

const fn map_tristimulus_component(
    component: CoreTristimulusComponentV1,
) -> TristimulusComponentV1 {
    match component {
        CoreTristimulusComponentV1::X => TristimulusComponentV1::X,
        CoreTristimulusComponentV1::Y => TristimulusComponentV1::Y,
        CoreTristimulusComponentV1::Z => TristimulusComponentV1::Z,
    }
}

const fn map_tristimulus_domain_error(
    error: TristimulusDomainErrorV1,
) -> (TristimulusComponentV1, NumericDomainErrorV1) {
    (
        map_tristimulus_component(error.component()),
        map_numeric_domain_error(error.reason()),
    )
}

fn map_modeled_occurrence_error(
    error: ModeledLcsOccurrenceFormationErrorV1,
) -> ModeledOccurrenceFailureV1 {
    match error {
        ModeledLcsOccurrenceFormationErrorV1::Tristimulus(source) => {
            let (component, reason) = map_tristimulus_domain_error(source);
            ModeledOccurrenceFailureV1::Tristimulus { component, reason }
        }
        ModeledLcsOccurrenceFormationErrorV1::Formation(
            OccurrenceFormationError::FrameMismatch { stimulus, context },
        ) => ModeledOccurrenceFailureV1::FrameMismatch {
            stimulus: map_colorimetric_frame(stimulus),
            context: map_colorimetric_frame(context),
        },
        ModeledLcsOccurrenceFormationErrorV1::ProvenanceReplayFailed(source) => {
            let (component, reason) = map_tristimulus_domain_error(source);
            ModeledOccurrenceFailureV1::ProvenanceReplayFailed { component, reason }
        }
        ModeledLcsOccurrenceFormationErrorV1::RecordedSampleDoesNotReplay {
            recorded,
            replayed,
        } => ModeledOccurrenceFailureV1::RecordedSampleDoesNotReplay {
            recorded: TristimulusSampleV1::from_core(recorded),
            replayed: TristimulusSampleV1::from_core(replayed),
        },
        ModeledLcsOccurrenceFormationErrorV1::OccurrenceSampleMismatch {
            occurrence,
            modeled,
        } => ModeledOccurrenceFailureV1::OccurrenceSampleMismatch {
            occurrence: TristimulusSampleV1::from_core(occurrence),
            modeled: TristimulusSampleV1::from_core(modeled),
        },
    }
}

fn map_observation_schema_mismatch(
    error: ObservationSchemaMismatchV1,
) -> ObservationBindingFailureV1 {
    let (case_index, binding_index, expected, actual) = error.into_parts();
    ObservationBindingFailureV1::SchemaMismatch {
        case_index,
        binding_index,
        expected: expected.map(SurfaceInputPortIdV1::from_core),
        actual: actual.map(SurfaceInputPortIdV1::from_core),
    }
}

fn map_session_update_error(error: SessionUpdateError<CoreProgramPlanErrorV1>) -> UpdateErrorV1 {
    match error {
        // A borrowed matching owner keeps the exact Rc generation alive for
        // the whole transaction; expiry here is therefore an internal breach.
        SessionUpdateError::OwnerExpired => UpdateErrorV1::InternalInvariant {
            source: UpdateInvariantFailureV1::OwnerAuthority,
        },
        SessionUpdateError::Observation(error) => map_observation_error(error),
        SessionUpdateError::Plan(error) => map_plan_error(error),
        SessionUpdateError::EvidenceBindingInvariant => UpdateErrorV1::InternalInvariant {
            source: UpdateInvariantFailureV1::EvidenceBinding,
        },
    }
}

fn map_observation_error(error: ObservationError) -> UpdateErrorV1 {
    match error {
        ObservationError::EmptyCompiledSurfaceInputSchema => UpdateErrorV1::InternalInvariant {
            source: UpdateInvariantFailureV1::ObservationBinding {
                source: ObservationBindingFailureV1::EmptyCompiledSurfaceInputSchema,
            },
        },
        ObservationError::DuplicateCompiledSurfaceInputPort { input } => {
            UpdateErrorV1::InternalInvariant {
                source: UpdateInvariantFailureV1::ObservationBinding {
                    source: ObservationBindingFailureV1::DuplicateCompiledSurfaceInputPort {
                        input: SurfaceInputPortIdV1::from_core(input),
                    },
                },
            }
        }
        ObservationError::StreamMismatch { expected, actual } => UpdateErrorV1::InternalInvariant {
            source: UpdateInvariantFailureV1::ObservationBinding {
                source: ObservationBindingFailureV1::StreamMismatch {
                    expected: StreamIdV1::from_core(expected),
                    actual: StreamIdV1::from_core(actual),
                },
            },
        },
        ObservationError::EmptyScenarioSet => UpdateErrorV1::EmptyScenarioSet,
        ObservationError::DuplicateScenarioId { scenario } => UpdateErrorV1::DuplicateScenarioId {
            scenario: ScenarioIdV1::from_core(scenario),
        },
        ObservationError::SchemaOrderedValueCountMismatch {
            scenario,
            expected,
            actual,
        } => UpdateErrorV1::ScenarioValueCountMismatch {
            scenario: ScenarioIdV1::from_core(scenario),
            expected,
            actual,
        },
        ObservationError::DuplicateSurfaceInputBinding { scenario, input } => {
            UpdateErrorV1::InternalInvariant {
                source: UpdateInvariantFailureV1::ObservationBinding {
                    source: ObservationBindingFailureV1::DuplicateSurfaceInputBinding {
                        scenario: ScenarioIdV1::from_core(scenario),
                        input: SurfaceInputPortIdV1::from_core(input),
                    },
                },
            }
        }
        ObservationError::MissingSurfaceInputBinding { scenario, input } => {
            UpdateErrorV1::InternalInvariant {
                source: UpdateInvariantFailureV1::ObservationBinding {
                    source: ObservationBindingFailureV1::MissingSurfaceInputBinding {
                        scenario: ScenarioIdV1::from_core(scenario),
                        input: SurfaceInputPortIdV1::from_core(input),
                    },
                },
            }
        }
        ObservationError::UnexpectedSurfaceInputBinding { scenario, input } => {
            UpdateErrorV1::InternalInvariant {
                source: UpdateInvariantFailureV1::ObservationBinding {
                    source: ObservationBindingFailureV1::UnexpectedSurfaceInputBinding {
                        scenario: ScenarioIdV1::from_core(scenario),
                        input: SurfaceInputPortIdV1::from_core(input),
                    },
                },
            }
        }
        ObservationError::RevisionOutOfOrder { current, incoming } => {
            UpdateErrorV1::RevisionOutOfOrder {
                current: current.value(),
                incoming: incoming.value(),
            }
        }
        ObservationError::RevisionConflict { revision } => UpdateErrorV1::RevisionConflict {
            revision: revision.value(),
        },
        ObservationError::ResourceExhausted => UpdateErrorV1::ResourceExhausted {
            phase: UpdatePhaseV1::ObservationAdmission,
        },
    }
}

fn map_plan_error(error: CoreProgramPlanErrorV1) -> UpdateErrorV1 {
    match error {
        ProgramSessionEvaluationError::ObservationSchemaMismatch(source) => {
            UpdateErrorV1::InternalInvariant {
                source: UpdateInvariantFailureV1::ObservationBinding {
                    source: map_observation_schema_mismatch(source),
                },
            }
        }
        ProgramSessionEvaluationError::ResourceExhausted => UpdateErrorV1::ResourceExhausted {
            phase: UpdatePhaseV1::ProgramEvaluation,
        },
        ProgramSessionEvaluationError::Evaluator {
            case_index,
            constraint,
            occurrence,
            context,
            source,
        } => match map_evaluator_error(source) {
            Ok(source) => UpdateErrorV1::EvaluationFailed {
                case_index,
                constraint: ConstraintIdV1::from_core(constraint),
                occurrence: OccurrenceIdV1::from_core(occurrence),
                context: AppearanceContextV1::from_core(context),
                source,
            },
            Err(source) => UpdateErrorV1::InternalInvariant {
                source: UpdateInvariantFailureV1::EvaluatorProtocol {
                    case_index,
                    constraint: ConstraintIdV1::from_core(constraint),
                    occurrence: OccurrenceIdV1::from_core(occurrence),
                    context: AppearanceContextV1::from_core(context),
                    source,
                },
            },
        },
        ProgramSessionEvaluationError::ProgramTargetBinding {
            case_index,
            constraint,
            occurrence,
            context,
            physical,
            modeled,
        } => UpdateErrorV1::InternalInvariant {
            source: UpdateInvariantFailureV1::PhysicalModeledBinding {
                case_index,
                constraint: ConstraintIdV1::from_core(constraint),
                occurrence: OccurrenceIdV1::from_core(occurrence),
                context: AppearanceContextV1::from_core(context),
                physical,
                modeled,
            },
        },
        ProgramSessionEvaluationError::ModeledOccurrence {
            case_index,
            occurrence,
            context,
            source,
        } => UpdateErrorV1::InternalInvariant {
            source: UpdateInvariantFailureV1::ModeledOccurrenceFormation {
                case_index,
                occurrence: OccurrenceIdV1::from_core(occurrence),
                context: AppearanceContextV1::from_core(context),
                source: map_modeled_occurrence_error(source),
            },
        },
        ProgramSessionEvaluationError::OutputVariesAcrossCases {
            output,
            first_case,
            actual_case,
        } => UpdateErrorV1::InternalInvariant {
            source: UpdateInvariantFailureV1::OutputCaseInvariance {
                output: OutputSlotIdV1::from_core(output),
                first_case,
                actual_case,
            },
        },
        ProgramSessionEvaluationError::FinalRecheckViolation {
            state_index,
            case_index,
            constraint,
            target,
            hard_violation_count,
        } => UpdateErrorV1::InternalInvariant {
            source: UpdateInvariantFailureV1::SelectionRecheck {
                state_index,
                case_index,
                constraint: ConstraintIdV1::from_core(constraint),
                occurrence: OccurrenceIdV1::from_core(target),
                hard_violation_count,
            },
        },
        ProgramSessionEvaluationError::InternalInvariant => UpdateErrorV1::InternalInvariant {
            source: UpdateInvariantFailureV1::ProgramEvaluation,
        },
    }
}

fn map_evaluator_error(
    error: CoreProgramEvaluatorErrorV1,
) -> Result<EvaluatorFailureV1, EvaluatorProtocolFailureV1> {
    match error {
        CoreProgramEvaluatorErrorV1::ExactSrgb8(source) => match source {},
        CoreProgramEvaluatorErrorV1::Wcag22Srgb8(source) => {
            let profile_id = wcag22_profile_v1().profile_id;
            match source {
                ApplicableWcag22EvaluationErrorV1::Kernel(
                    source @ (Wcag22EvaluationErrorV1::ArtifactInvariantViolation { .. }
                    | Wcag22EvaluationErrorV1::EvidenceRegistryMismatch(_)),
                ) => Ok(EvaluatorFailureV1::Wcag22Srgb8 { profile_id, source }),
                ApplicableWcag22EvaluationErrorV1::Kernel(source) => {
                    Err(EvaluatorProtocolFailureV1::Wcag22Kernel { profile_id, source })
                }
                ApplicableWcag22EvaluationErrorV1::ReportOnly {
                    profile_id,
                    declaration,
                } => Err(EvaluatorProtocolFailureV1::Wcag22ReportOnly {
                    profile_id,
                    declaration,
                }),
                ApplicableWcag22EvaluationErrorV1::CriterionMismatch {
                    requested,
                    evaluated,
                } => Err(EvaluatorProtocolFailureV1::Wcag22CriterionMismatch {
                    requested,
                    evaluated,
                }),
            }
        }
    }
}

#[cfg(test)]
mod update_error_projection_tests {
    use super::*;
    use crate::wcag22::Wcag22ClientDeclaredNotApplicableV1;

    fn context() -> AppearanceContextV1 {
        AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Average).unwrap()
    }

    fn map_wcag_error(source: ApplicableWcag22EvaluationErrorV1) -> UpdateErrorV1 {
        let context = context();
        map_plan_error(ProgramSessionEvaluationError::Evaluator {
            case_index: 7,
            constraint: ConstraintId::new(11),
            occurrence: OccurrenceId::new(13),
            context: context.0,
            source: CoreProgramEvaluatorErrorV1::Wcag22Srgb8(source),
        })
    }

    #[test]
    fn resource_exhaustion_retains_its_exact_update_phase() {
        let observation = map_observation_error(ObservationError::ResourceExhausted);
        assert_eq!(observation.kind(), UpdateErrorKindV1::ResourceExhausted);
        assert_eq!(
            observation,
            UpdateErrorV1::ResourceExhausted {
                phase: UpdatePhaseV1::ObservationAdmission,
            }
        );
        let evaluation = map_plan_error(ProgramSessionEvaluationError::ResourceExhausted);
        assert_eq!(evaluation.kind(), UpdateErrorKindV1::ResourceExhausted);
        assert_eq!(
            evaluation,
            UpdateErrorV1::ResourceExhausted {
                phase: UpdatePhaseV1::ProgramEvaluation,
            }
        );
    }

    #[test]
    fn evaluator_artifact_failure_retains_every_actionable_fact() {
        let error = map_wcag_error(ApplicableWcag22EvaluationErrorV1::Kernel(
            Wcag22EvaluationErrorV1::ArtifactInvariantViolation {
                criterion: Wcag22CriterionV1::Sc143TextLargeScale,
                foreground: [1, 2, 3],
                background: [4, 5, 6],
            },
        ));
        assert_eq!(error.kind(), UpdateErrorKindV1::EvaluationFailed);
        assert_eq!(
            error,
            UpdateErrorV1::EvaluationFailed {
                case_index: 7,
                constraint: ConstraintIdV1::new(11),
                occurrence: OccurrenceIdV1::new(13),
                context: context(),
                source: EvaluatorFailureV1::Wcag22Srgb8 {
                    profile_id: wcag22_profile_v1().profile_id,
                    source: Wcag22EvaluationErrorV1::ArtifactInvariantViolation {
                        criterion: Wcag22CriterionV1::Sc143TextLargeScale,
                        foreground: [1, 2, 3],
                        background: [4, 5, 6],
                    },
                },
            }
        );

        let error = map_wcag_error(ApplicableWcag22EvaluationErrorV1::Kernel(
            Wcag22EvaluationErrorV1::EvidenceRegistryMismatch("registry-vs-proof".into()),
        ));
        assert_eq!(error.kind(), UpdateErrorKindV1::EvaluationFailed);
        let UpdateErrorV1::EvaluationFailed {
            source:
                EvaluatorFailureV1::Wcag22Srgb8 {
                    profile_id,
                    source: Wcag22EvaluationErrorV1::EvidenceRegistryMismatch(message),
                },
            ..
        } = error
        else {
            panic!("registry mismatch must remain an evaluator failure");
        };
        assert_eq!(profile_id, wcag22_profile_v1().profile_id);
        assert_eq!(message, "registry-vs-proof");
    }

    #[test]
    fn impossible_wcag_protocol_states_are_not_misreported_as_colour_failures() {
        let profile_id = wcag22_profile_v1().profile_id;
        let declaration = Wcag22ClientDeclaredNotApplicableV1::try_new("client-scope").unwrap();
        let cases = [
            (
                ApplicableWcag22EvaluationErrorV1::Kernel(Wcag22EvaluationErrorV1::InvalidSrgb8 {
                    field: "foreground",
                    reason: "typed Program cannot create this".into(),
                }),
                EvaluatorProtocolFailureV1::Wcag22Kernel {
                    profile_id,
                    source: Wcag22EvaluationErrorV1::InvalidSrgb8 {
                        field: "foreground",
                        reason: "typed Program cannot create this".into(),
                    },
                },
            ),
            (
                ApplicableWcag22EvaluationErrorV1::Kernel(
                    Wcag22EvaluationErrorV1::EmptyNotApplicableReason,
                ),
                EvaluatorProtocolFailureV1::Wcag22Kernel {
                    profile_id,
                    source: Wcag22EvaluationErrorV1::EmptyNotApplicableReason,
                },
            ),
            (
                ApplicableWcag22EvaluationErrorV1::ReportOnly {
                    profile_id,
                    declaration: declaration.clone(),
                },
                EvaluatorProtocolFailureV1::Wcag22ReportOnly {
                    profile_id,
                    declaration,
                },
            ),
            (
                ApplicableWcag22EvaluationErrorV1::CriterionMismatch {
                    requested: Wcag22CriterionV1::Sc143TextDefault,
                    evaluated: Wcag22CriterionV1::Sc1411UiComponentOrState,
                },
                EvaluatorProtocolFailureV1::Wcag22CriterionMismatch {
                    requested: Wcag22CriterionV1::Sc143TextDefault,
                    evaluated: Wcag22CriterionV1::Sc1411UiComponentOrState,
                },
            ),
        ];
        for (source, expected) in cases {
            let error = map_wcag_error(source);
            assert_eq!(error.kind(), UpdateErrorKindV1::InternalInvariant);
            let expected = UpdateInvariantFailureV1::EvaluatorProtocol {
                case_index: 7,
                constraint: ConstraintIdV1::new(11),
                occurrence: OccurrenceIdV1::new(13),
                context: context(),
                source: expected,
            };
            assert_eq!(expected.contract(), UpdateInvariantV1::EvaluatorProtocol);
            assert_eq!(error, UpdateErrorV1::InternalInvariant { source: expected });
        }
    }

    #[test]
    fn every_observation_binding_failure_retains_its_exact_payload() {
        let cases = [
            (
                ObservationError::EmptyCompiledSurfaceInputSchema,
                ObservationBindingFailureV1::EmptyCompiledSurfaceInputSchema,
            ),
            (
                ObservationError::DuplicateCompiledSurfaceInputPort {
                    input: SurfaceInputPortId::new(3),
                },
                ObservationBindingFailureV1::DuplicateCompiledSurfaceInputPort {
                    input: SurfaceInputPortIdV1::new(3),
                },
            ),
            (
                ObservationError::StreamMismatch {
                    expected: ObservationStreamId::new(5),
                    actual: ObservationStreamId::new(7),
                },
                ObservationBindingFailureV1::StreamMismatch {
                    expected: StreamIdV1::from_core(ObservationStreamId::new(5)),
                    actual: StreamIdV1::from_core(ObservationStreamId::new(7)),
                },
            ),
            (
                ObservationError::DuplicateSurfaceInputBinding {
                    scenario: ScenarioId::new(11),
                    input: SurfaceInputPortId::new(13),
                },
                ObservationBindingFailureV1::DuplicateSurfaceInputBinding {
                    scenario: ScenarioIdV1::from_core(ScenarioId::new(11)),
                    input: SurfaceInputPortIdV1::new(13),
                },
            ),
            (
                ObservationError::MissingSurfaceInputBinding {
                    scenario: ScenarioId::new(17),
                    input: SurfaceInputPortId::new(19),
                },
                ObservationBindingFailureV1::MissingSurfaceInputBinding {
                    scenario: ScenarioIdV1::from_core(ScenarioId::new(17)),
                    input: SurfaceInputPortIdV1::new(19),
                },
            ),
            (
                ObservationError::UnexpectedSurfaceInputBinding {
                    scenario: ScenarioId::new(23),
                    input: SurfaceInputPortId::new(29),
                },
                ObservationBindingFailureV1::UnexpectedSurfaceInputBinding {
                    scenario: ScenarioIdV1::from_core(ScenarioId::new(23)),
                    input: SurfaceInputPortIdV1::new(29),
                },
            ),
        ];

        for (source, expected) in cases {
            let error = map_observation_error(source);
            assert_eq!(error.kind(), UpdateErrorKindV1::InternalInvariant);
            assert_eq!(
                error,
                UpdateErrorV1::InternalInvariant {
                    source: UpdateInvariantFailureV1::ObservationBinding { source: expected },
                }
            );
        }
    }

    #[test]
    fn every_modeled_occurrence_failure_has_an_isomorphic_public_witness() {
        use crate::lcs_occurrence::{
            MUTATION_SENTINEL_XYZ_FRAME_V1, ModeledLcsOccurrenceFormationErrorV1,
            OccurrenceFormationError, TristimulusSample,
        };

        let domain = TristimulusSample::try_from_xyz_for_test(
            [f64::NAN, 0.2, 0.3],
            IEC_SRGB_D65_XYZ_FRAME_V1,
        )
        .unwrap_err();
        let recorded =
            TristimulusSample::try_from_xyz_for_test([0.1, 0.2, 0.3], IEC_SRGB_D65_XYZ_FRAME_V1)
                .unwrap();
        let replayed =
            TristimulusSample::try_from_xyz_for_test([0.1, 0.2, 0.4], IEC_SRGB_D65_XYZ_FRAME_V1)
                .unwrap();

        let cases = [
            (
                ModeledLcsOccurrenceFormationErrorV1::Tristimulus(domain),
                ModeledOccurrenceFailureV1::Tristimulus {
                    component: TristimulusComponentV1::X,
                    reason: NumericDomainErrorV1::NonFinite,
                },
            ),
            (
                ModeledLcsOccurrenceFormationErrorV1::Formation(
                    OccurrenceFormationError::FrameMismatch {
                        stimulus: IEC_SRGB_D65_XYZ_FRAME_V1,
                        context: MUTATION_SENTINEL_XYZ_FRAME_V1,
                    },
                ),
                ModeledOccurrenceFailureV1::FrameMismatch {
                    stimulus: ColorimetricFrameV1::Iec61966Srgb8D65XyzRelativeY1V1,
                    context: ColorimetricFrameV1::MutationSentinelV1,
                },
            ),
            (
                ModeledLcsOccurrenceFormationErrorV1::ProvenanceReplayFailed(domain),
                ModeledOccurrenceFailureV1::ProvenanceReplayFailed {
                    component: TristimulusComponentV1::X,
                    reason: NumericDomainErrorV1::NonFinite,
                },
            ),
            (
                ModeledLcsOccurrenceFormationErrorV1::RecordedSampleDoesNotReplay {
                    recorded,
                    replayed,
                },
                ModeledOccurrenceFailureV1::RecordedSampleDoesNotReplay {
                    recorded: TristimulusSampleV1::from_core(recorded),
                    replayed: TristimulusSampleV1::from_core(replayed),
                },
            ),
            (
                ModeledLcsOccurrenceFormationErrorV1::OccurrenceSampleMismatch {
                    occurrence: recorded,
                    modeled: replayed,
                },
                ModeledOccurrenceFailureV1::OccurrenceSampleMismatch {
                    occurrence: TristimulusSampleV1::from_core(recorded),
                    modeled: TristimulusSampleV1::from_core(replayed),
                },
            ),
        ];

        for (source, expected) in cases {
            assert_eq!(map_modeled_occurrence_error(source), expected);
        }
    }

    #[test]
    fn every_unreachable_core_failure_keeps_its_subject_and_witness_facts() {
        use crate::lcs_occurrence::{
            MUTATION_SENTINEL_XYZ_FRAME_V1, ModeledLcsOccurrenceFormationErrorV1,
            OccurrenceFormationError,
        };
        use crate::observation::ObservationSchemaMismatchV1;

        let assert_invariant = |error: UpdateErrorV1, source: UpdateInvariantFailureV1| {
            assert_eq!(error.kind(), UpdateErrorKindV1::InternalInvariant);
            assert_eq!(
                error,
                UpdateErrorV1::InternalInvariant {
                    source: source.clone(),
                }
            );
            assert_eq!(
                source.contract(),
                match source {
                    UpdateInvariantFailureV1::OwnerAuthority => {
                        UpdateInvariantV1::OwnerAuthority
                    }
                    UpdateInvariantFailureV1::ObservationBinding { .. } => {
                        UpdateInvariantV1::ObservationBinding
                    }
                    UpdateInvariantFailureV1::EvidenceBinding => {
                        UpdateInvariantV1::EvidenceBinding
                    }
                    UpdateInvariantFailureV1::EvaluatorProtocol { .. } => {
                        UpdateInvariantV1::EvaluatorProtocol
                    }
                    UpdateInvariantFailureV1::PhysicalModeledBinding { .. } => {
                        UpdateInvariantV1::PhysicalModeledBinding
                    }
                    UpdateInvariantFailureV1::ModeledOccurrenceFormation { .. } => {
                        UpdateInvariantV1::ModeledOccurrenceFormation
                    }
                    UpdateInvariantFailureV1::OutputCaseInvariance { .. } => {
                        UpdateInvariantV1::OutputCaseInvariance
                    }
                    UpdateInvariantFailureV1::SelectionRecheck { .. } => {
                        UpdateInvariantV1::SelectionRecheck
                    }
                    UpdateInvariantFailureV1::ProgramEvaluation => {
                        UpdateInvariantV1::ProgramEvaluation
                    }
                }
            );
        };

        assert_invariant(
            map_session_update_error(SessionUpdateError::OwnerExpired),
            UpdateInvariantFailureV1::OwnerAuthority,
        );
        assert_invariant(
            map_session_update_error(SessionUpdateError::EvidenceBindingInvariant),
            UpdateInvariantFailureV1::EvidenceBinding,
        );
        assert_invariant(
            map_observation_error(ObservationError::EmptyCompiledSurfaceInputSchema),
            UpdateInvariantFailureV1::ObservationBinding {
                source: ObservationBindingFailureV1::EmptyCompiledSurfaceInputSchema,
            },
        );

        assert_invariant(
            map_plan_error(ProgramSessionEvaluationError::ObservationSchemaMismatch(
                ObservationSchemaMismatchV1::new(2, 3, None, None),
            )),
            UpdateInvariantFailureV1::ObservationBinding {
                source: ObservationBindingFailureV1::SchemaMismatch {
                    case_index: 2,
                    binding_index: 3,
                    expected: None,
                    actual: None,
                },
            },
        );
        assert_invariant(
            map_plan_error(ProgramSessionEvaluationError::ProgramTargetBinding {
                case_index: 2,
                constraint: ConstraintId::new(3),
                occurrence: OccurrenceId::new(4),
                context: context().0,
                physical: Srgb8::new([1, 2, 3]),
                modeled: Srgb8::new([3, 2, 1]),
            }),
            UpdateInvariantFailureV1::PhysicalModeledBinding {
                case_index: 2,
                constraint: ConstraintIdV1::new(3),
                occurrence: OccurrenceIdV1::new(4),
                context: context(),
                physical: Srgb8::new([1, 2, 3]),
                modeled: Srgb8::new([3, 2, 1]),
            },
        );
        assert_invariant(
            map_plan_error(ProgramSessionEvaluationError::ModeledOccurrence {
                case_index: 2,
                occurrence: OccurrenceId::new(4),
                context: context().0,
                source: ModeledLcsOccurrenceFormationErrorV1::Formation(
                    OccurrenceFormationError::FrameMismatch {
                        stimulus: IEC_SRGB_D65_XYZ_FRAME_V1,
                        context: MUTATION_SENTINEL_XYZ_FRAME_V1,
                    },
                ),
            }),
            UpdateInvariantFailureV1::ModeledOccurrenceFormation {
                case_index: 2,
                occurrence: OccurrenceIdV1::new(4),
                context: context(),
                source: ModeledOccurrenceFailureV1::FrameMismatch {
                    stimulus: ColorimetricFrameV1::Iec61966Srgb8D65XyzRelativeY1V1,
                    context: ColorimetricFrameV1::MutationSentinelV1,
                },
            },
        );
        assert_invariant(
            map_plan_error(ProgramSessionEvaluationError::OutputVariesAcrossCases {
                output: OutputSlotId::new(5),
                first_case: 1,
                actual_case: 2,
            }),
            UpdateInvariantFailureV1::OutputCaseInvariance {
                output: OutputSlotIdV1::new(5),
                first_case: 1,
                actual_case: 2,
            },
        );
        assert_invariant(
            map_plan_error(ProgramSessionEvaluationError::FinalRecheckViolation {
                state_index: 1,
                case_index: 2,
                constraint: ConstraintId::new(3),
                target: OccurrenceId::new(4),
                hard_violation_count: 1,
            }),
            UpdateInvariantFailureV1::SelectionRecheck {
                state_index: 1,
                case_index: 2,
                constraint: ConstraintIdV1::new(3),
                occurrence: OccurrenceIdV1::new(4),
                hard_violation_count: 1,
            },
        );
        assert_invariant(
            map_plan_error(ProgramSessionEvaluationError::InternalInvariant),
            UpdateInvariantFailureV1::ProgramEvaluation,
        );
    }
}

#[cfg(test)]
mod operation_scope_tests {
    use super::*;

    #[test]
    fn operation_scope_is_a_zero_sized_borrow_marker() {
        assert_eq!(core::mem::size_of::<BorrowScopeV1<'static, 'static>>(), 0);
    }
}

#[cfg(test)]
mod compile_error_projection_tests {
    use super::*;

    #[test]
    fn nested_joint_resource_exhaustion_keeps_its_exact_reason_and_site_kind() {
        let error = map_program_compile_error(ProgramCompileError::InvalidJointOrder(
            FiniteJointOrderErrorV1::ResourceExhausted,
        ));

        assert_eq!(error.kind(), CompileErrorKindV1::InvalidJointOrder);
        assert_eq!(
            error,
            CompileErrorV1::InvalidJointOrder(JointOrderErrorV1::ResourceExhausted)
        );
    }
}
