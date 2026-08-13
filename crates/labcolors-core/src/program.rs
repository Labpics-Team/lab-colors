//! Внутренний кандидат декларативного контракта цветовой программы.
//!
//! Модуль не публикуется до завершения emission, attachment и атомарной
//! транзакционной границы terminal C7c. Его закрытая поверхность уже служит
//! единственным concrete seam для внутренних проверок и дальнейших срезов.
//!
//! Клиент один раз описывает физический граф через [`DraftV1`]: исходные
//! сигналы, решаемые цели, Paint, Surface, их [`OccurrenceIdV1`], ограничения
//! и выходы. Идентификаторы непрозрачны: Core не выводит из них семантику.
//! [`DraftV1::compile`] проверяет граф целиком и возвращает [`OwnerV1`] —
//! единственного владельца конкретной скомпилированной эпохи.
//!
//! [`OwnerV1::instantiate`] создаёт evidence-only [`SessionV1`] для lint и
//! мониторинга. [`OwnerV1::prepare_update`] возвращает линейный
//! [`PreparedSessionTransitionV1`]: Drop ничего не публикует, а consuming commit
//! меняет только raw head и lifecycle Session. Этот путь не выдаёт sink-authority.
//!
//! Terminal runtime принадлежит [`attachment`]: один Attachment структурно
//! связывает точную compiled generation, Session, полные output→sink и
//! output→presentation bindings и линейный writer lease. Только он может
//! атомарно материализовать или отозвать весь снимок; историческое evidence
//! само по себе такого права не даёт.
//!
//! [`CertificateV1::Verified`] хранит выбранное состояние, все клетки
//! доказательства и сертифицированные Paint outputs. [`CertificateV1::Conflict`]
//! хранит исчерпывающий конфликт по всем рассмотренным состояниям.
//! [`ContentIdentityV8`] идентифицирует каноническое содержание, но не даёт
//! полномочий живого [`OwnerV1`].

#![forbid(unreachable_pub)]

/// Транзакционный point-output attachment и его линейный sink-контракт.
pub(crate) mod attachment;

use core::{fmt, iter::FusedIterator};

use crate::Srgb8;
use crate::appearance::{
    EncodedPointPaintValueV1, OccurrenceId, OpacityInputId, PaintId, SurfaceId, SurfaceInputPortId,
};
use crate::composition::{AdmittedOpacityV1, CompositionProfileV1, OpacityAdmissionErrorV1};
use crate::constraints::{
    ApplicableWcag22EvaluationErrorV1, CoreIntrinsicUnaryMeasurementV1, CoreIntrinsicUnaryPassV1,
    CoreIntrinsicUnaryViolationV1, CoreRelationMeasurementV1, CoreRelationPassV1,
    CoreRelationViolationV1, ExactSrgb8IdentityV1,
    FamilyCategoryRelationMeasurementV1 as CoreFamilyCategoryRelationMeasurementV1,
    FamilyCategoryRelationViolationV1 as CoreFamilyCategoryRelationViolationV1,
    ProgramVisiblePointBindingV1, ProgramVisiblePointPassEvidence,
    ProgramVisiblePointViolationEvidence, Wcag22Srgb8V1,
};
use crate::family::{
    FamilyDeclarationV2, FamilyId,
    FamilyMembershipMeasurementV2 as CoreFamilyMembershipMeasurementV2,
    SemanticFamilyReleaseIdV2 as CoreSemanticFamilyReleaseIdV2,
};
use crate::family_artifact::{FamilyArtifactBundleV2, FamilyArtifactContractErrorV2};
use crate::joint::FiniteJointOrderErrorV1;
use crate::lcs_occurrence::{
    AdaptingLuminanceCdM2, AppearanceContextDomainErrorV1,
    AppearanceContextFieldV1 as CoreAppearanceContextFieldV1, AppearanceContextId,
    AppearanceContextSchemaReleaseId, BackgroundLuminanceRatio, ColorSignal, ColorSignalViewV1,
    IEC_SRGB_D65_XYZ_FRAME_V1, NumericDomainError, SurroundProfileId,
};
use crate::numerics::NumericalDecisionEvidenceV1;
use crate::observation::{
    ObservationError, ObservationHeadViewV1, ObservationSchemaMismatchV1, ObservationStreamId,
    Revision, ScenarioId, SchemaOrderedScenarioSourceV1, UnknownReasonId,
};
#[cfg(test)]
use crate::program_session::DeclaredJointSelectionV1;
use crate::program_session::{
    CompiledCoreProgramV1, CompositionProfile, ConstraintId, ConstraintInvocation,
    CoreProgramConstraintInvocationV1, CoreProgramDraftErrorV1, CoreProgramDraftV1,
    CoreProgramEvaluatorErrorV1, CoreProgramEvaluatorsV1, CoreProgramPassEvidenceV1,
    CoreProgramViolationEvidenceV1, DeclaredSrgb8CleanSetPassV1 as CoreDeclaredSrgb8CleanSetPassV1,
    DeclaredSrgb8CleanSetViolationV1 as CoreDeclaredSrgb8CleanSetViolationV1,
    FinitePaintDomainAdmissionErrorV1, FinitePaintDomainV1 as CoreFinitePaintDomainV1,
    JointCandidateStateV1, Occurrence, OpacityInput, OutputBinding, OutputSlotId, Paint,
    PointPresentationRootV1, PointPresentationTargetV1, PresentationRootId, ProgramCompileError,
    ProgramConflictV1, ProgramConstraintCellV1, ProgramConstraintPassEvidenceV1,
    ProgramConstraintResultV1, ProgramConstraintSubjectV1, ProgramConstraintViolationEvidenceV1,
    ProgramContentIdentityV8, ProgramIntrinsicPaintBindingV1, ProgramIntrinsicUnaryPassEvidenceV1,
    ProgramIntrinsicUnaryViolationEvidenceV1, ProgramPaintOutputV1,
    ProgramRelationMemberDecisionV1, ProgramRelationMemberEvidenceV1, ProgramReportV1,
    ProgramSessionEvaluationError, ProgramSessionInstantiateError, ProgramSessionPlan,
    ProgramVerifiedV1, ProgramVisibleRelationBindingV1, Source, SourceId, Surface, Target,
    TargetCandidateChoiceV1, TargetCandidateId, TargetCandidateV1 as CoreTargetCandidateV1,
    TargetId,
};
#[cfg(test)]
pub(crate) use crate::relation::DirectedRelationErrorV1;
pub(crate) use crate::relation::DirectedRelationV1;
use crate::selection_release::{MaterialisedSelectionV1, SelectionReleaseIdentityV1};
use crate::session::{
    PreparedSessionTransition, Session, SessionState, SessionUpdateError, SessionView,
};
use crate::wcag22::{
    Wcag22ClientDeclaredNotApplicableV1, Wcag22CriterionV1, Wcag22EvaluationErrorV1,
    Wcag22LuminanceBoundsQ55V1, Wcag22ProfileIdV1, wcag22_profile_v1,
};

type CoreVerifiedV1 = ProgramVerifiedV1<CoreProgramEvaluatorsV1>;
type CoreConflictV1 = ProgramConflictV1<CoreProgramEvaluatorsV1>;
type CoreProgramPlanV1 = ProgramSessionPlan<CoreProgramEvaluatorsV1>;
type CoreProgramSessionV1 = Session<CoreProgramPlanV1>;
type CoreProgramStateV1 = SessionState<CoreVerifiedV1, CoreConflictV1>;
type CoreProgramSessionViewV1<'a> = SessionView<'a, CoreProgramPlanV1>;
type CorePreparedSessionTransitionV1<'a> = PreparedSessionTransition<'a, CoreProgramPlanV1>;
type CoreProgramPlanErrorV1 = ProgramSessionEvaluationError<CoreProgramEvaluatorErrorV1>;
type CoreProgramConstraintCellV1 = ProgramConstraintCellV1<CoreProgramEvaluatorsV1>;
type CoreProgramReportV1 = ProgramReportV1<CoreProgramEvaluatorsV1>;
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
        pub(crate) struct $name($core);

        impl $name {
            /// Создаёт непрозрачный идентификатор из клиентского числового ключа.
            pub(crate) const fn new(value: u32) -> Self {
                Self(<$core>::new(value))
            }

            /// Возвращает исходный клиентский числовой ключ.
            pub(crate) const fn value(self) -> u32 {
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
        pub(crate) struct $name($core);

        impl $name {
            const fn from_core(value: $core) -> Self {
                Self(value)
            }

            /// Возвращает числовой ключ сохранённой provenance.
            pub(crate) const fn value(self) -> u32 {
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
    "Идентификатор объявленного family-set.",
    FamilyIdV1,
    FamilyId
);
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
    "Идентификатор моделируемого point presentation root.",
    PresentationRootIdV1,
    PresentationRootId
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

/// Representation-independent semantic release одной family declaration.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct FamilySemanticReleaseV2(CoreSemanticFamilyReleaseIdV2);

impl FamilySemanticReleaseV2 {
    pub(crate) const fn from_core(value: CoreSemanticFamilyReleaseIdV2) -> Self {
        Self(value)
    }

    pub(crate) const fn into_core(self) -> CoreSemanticFamilyReleaseIdV2 {
        self.0
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}
projected_id!(
    "Идентификатор сценария, сохранённый как provenance.",
    ScenarioIdV1,
    ScenarioId
);

/// Один физический кандидат конечной цели.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TargetCandidateV1(CoreTargetCandidateV1);

impl TargetCandidateV1 {
    /// Связывает непрозрачный ID с одним неделимым физическим Paint value.
    pub(crate) const fn new(id: TargetCandidateIdV1, value: PaintValueV1) -> Self {
        Self(CoreTargetCandidateV1::new(id.into_core(), value.0))
    }
}

/// Admission-ошибка конечного физического домена.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FinitePaintDomainErrorV1 {
    /// Конечный домен обязан содержать хотя бы один атомарный Paint-кандидат.
    Empty,
}

/// Непустой конечный набор атомарных Paint-кандидатов одной цели.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FinitePaintDomainV1(CoreFinitePaintDomainV1);

impl FinitePaintDomainV1 {
    /// Допускает домен до помещения в Draft, поэтому пустое состояние не может
    /// дожить до компиляции графа.
    pub(crate) fn try_new(
        candidates: Vec<TargetCandidateV1>,
    ) -> Result<Self, FinitePaintDomainErrorV1> {
        CoreFinitePaintDomainV1::try_new(
            candidates
                .into_iter()
                .map(|candidate| candidate.0)
                .collect(),
        )
        .map(Self)
        .map_err(|error| match error {
            FinitePaintDomainAdmissionErrorV1::Empty => FinitePaintDomainErrorV1::Empty,
        })
    }
}

/// Admission-ошибка атомарного значения конечного Paint-кандидата.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaintValueErrorV1 {
    /// Straight alpha не является конечным binary64.
    NonFiniteOpacity,
    /// Straight alpha находится вне замкнутого `[0, 1]`.
    OpacityOutsideUnitInterval,
}

/// Одно неделимое encoded-sRGB8 source + straight-alpha значение Paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PaintValueV1(EncodedPointPaintValueV1);

impl PaintValueV1 {
    const fn from_core(value: EncodedPointPaintValueV1) -> Self {
        Self(value)
    }

    /// Принимает только физически определённый straight alpha и канонизирует
    /// оба знака нуля, чтобы одно значение имело одно identity-представление.
    pub(crate) fn try_new(source: Srgb8, opacity: f64) -> Result<Self, PaintValueErrorV1> {
        let opacity = AdmittedOpacityV1::new(opacity).map_err(|error| match error {
            OpacityAdmissionErrorV1::NonFinite => PaintValueErrorV1::NonFiniteOpacity,
            OpacityAdmissionErrorV1::OutsideUnitInterval => {
                PaintValueErrorV1::OpacityOutsideUnitInterval
            }
        })?;
        Ok(Self(EncodedPointPaintValueV1::from_admitted(
            source, opacity,
        )))
    }

    /// Явный shorthand для распространённого непрозрачного значения.
    pub(crate) const fn opaque(source: Srgb8) -> Self {
        Self(EncodedPointPaintValueV1::opaque(source))
    }

    pub(crate) const fn source(self) -> Srgb8 {
        self.0.source()
    }

    pub(crate) const fn opacity(self) -> f64 {
        self.0.opacity().value()
    }
}

/// Выбор одного кандидата для одной цели в совместном состоянии.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JointChoiceV1(TargetCandidateChoiceV1);

impl JointChoiceV1 {
    /// Создаёт типизированную пару `цель → кандидат`.
    pub(crate) const fn new(target: TargetIdV1, candidate: TargetCandidateIdV1) -> Self {
        Self(TargetCandidateChoiceV1::new(
            target.into_core(),
            candidate.into_core(),
        ))
    }
}

/// Полное явно объявленное состояние всех конечных целей.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JointStateV1(JointCandidateStateV1);

impl JointStateV1 {
    /// Создаёт состояние из одного выбора для каждой конечной цели.
    pub(crate) fn new(choices: Vec<JointChoiceV1>) -> Self {
        Self(JointCandidateStateV1::new(
            choices.into_iter().map(|choice| choice.0).collect(),
        ))
    }
}

/// Зарегистрированный режим окружения CIECAM16.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SurroundV1 {
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
pub(crate) enum AppearanceContextFieldV1 {
    /// Адаптирующая яркость в кд/м².
    AdaptingLuminanceCdM2,
    /// Безразмерное отношение фоновой яркости `Y_b/Y_w`.
    BackgroundLuminanceRatioYbYw,
}

/// Числовая причина отказа при формировании контекста восприятия.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumericDomainErrorV1 {
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
pub(crate) enum AppearanceContextErrorKindV1 {
    /// Клиентское значение находится вне объявленного домена.
    Domain,
    /// Нарушен внутренний инвариант закрытого преобразования.
    InternalInvariant,
}

/// Типизированный отказ admission контекста восприятия.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AppearanceContextErrorV1 {
    kind: AppearanceContextErrorKindV1,
    field: Option<AppearanceContextFieldV1>,
    reason: Option<NumericDomainErrorV1>,
}

impl AppearanceContextErrorV1 {
    /// Возвращает класс отказа.
    pub(crate) const fn kind(self) -> AppearanceContextErrorKindV1 {
        self.kind
    }

    /// Возвращает отвергнутое поле, когда Core смог его локализовать.
    pub(crate) const fn field(self) -> Option<AppearanceContextFieldV1> {
        self.field
    }

    /// Возвращает точную числовую причину, если отказ относится к входному домену.
    pub(crate) const fn reason(self) -> Option<NumericDomainErrorV1> {
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
pub(crate) struct AppearanceContextV1(AppearanceContextId);

impl AppearanceContextV1 {
    const fn from_core(context: AppearanceContextId) -> Self {
        Self(context)
    }

    /// Допускает явные входы CIECAM16 для encoded sRGB8/D65.
    ///
    /// `background_luminance_ratio_yb_yw` — безразмерное `Y_b/Y_w` в `(0, 1]`,
    /// а не абсолютная яркость.
    pub(crate) fn try_new(
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
    pub(crate) fn adapting_luminance_cd_m2(self) -> f64 {
        self.0.adapting_luminance_cd_m2()
    }

    /// Возвращает допущенное безразмерное отношение `Y_b/Y_w`.
    pub(crate) fn background_luminance_ratio_yb_yw(self) -> f64 {
        self.0.background_luminance_ratio()
    }

    /// Возвращает зарегистрированный режим окружения.
    pub(crate) const fn surround(self) -> SurroundV1 {
        match self.0.surround_profile() {
            SurroundProfileId::AverageV1 => SurroundV1::Average,
            SurroundProfileId::DimV1 => SurroundV1::Dim,
            SurroundProfileId::DarkV1 => SurroundV1::Dark,
        }
    }
}

/// Закрытая классификация ошибки компиляции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompileErrorKindV1 {
    /// Повторно объявлен исходный сигнал.
    DuplicateSource,
    /// Повторно объявлена цель.
    DuplicateTarget,
    /// Повторно объявлен family-set.
    DuplicateFamily,
    /// Family-set объявлен, но не связан ни с одним ограничением.
    UnusedFamily,
    /// В одной цели повторно объявлен ID кандидата.
    DuplicateTargetCandidate,
    /// Два кандидата одной цели задают одинаковое атомарное Paint value.
    DuplicateTargetCandidateValue,
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
    /// Повторно объявлен presentation root.
    DuplicatePresentationRoot,
    /// Presentation root ссылается на отсутствующий терминальный Occurrence.
    MissingPresentationRootOccurrence,
    /// Терминальный Occurrence root всё ещё потребляется downstream.
    PresentationRootConsumedDownstream,
    /// Presentation root не имеет ни одной проверяемой цели.
    UnusedPresentationRoot,
    /// Повторно объявлена одна пара presentation root/target.
    DuplicatePointPresentationTarget,
    /// Presentation target ссылается на отсутствующий root.
    MissingPointPresentationRoot,
    /// Presentation target ссылается на отсутствующий Occurrence.
    MissingPointPresentationOccurrence,
    /// Target Occurrence не принадлежит ancestry объявленного root.
    PointPresentationOccurrenceOutsideRootAncestry,
    /// Повторно объявлено ограничение.
    DuplicateConstraint,
    /// Повторно объявлен выходной слот.
    DuplicateOutputSlot,
    /// Фиксированная цель ссылается на отсутствующий исходный сигнал.
    MissingFixedSource,
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
    /// Intrinsic-unary ограничение ссылается на отсутствующую Target.
    MissingIntrinsicUnaryTarget,
    /// Family membership ссылается на отсутствующий family-set.
    MissingConstraintFamily,
    /// Reference intrinsic-отношения ссылается на отсутствующую Target.
    MissingIntrinsicRelationReference,
    /// Candidate intrinsic-отношения ссылается на отсутствующую Target.
    MissingIntrinsicRelationCandidate,
    /// Reference visible-отношения ссылается на отсутствующий Occurrence.
    MissingVisibleRelationReference,
    /// Candidate visible-отношения ссылается на отсутствующий Occurrence.
    MissingVisibleRelationCandidate,
    /// Intrinsic reference зависит от выбираемого finite Target.
    SolverDependentIntrinsicRelationReference,
    /// Visible reference зависит от выбираемого finite Target.
    SolverDependentVisibleRelationReference,
    /// Ограничение clean-set ссылается на необъявленную цель представления.
    MissingConstraintPresentationTarget,
    /// Выход ссылается на отсутствующий Paint.
    MissingOutputPaint,
    /// Граф Paint содержит цикл.
    PaintCycle,
    /// Граф рендера содержит цикл Surface/Occurrence.
    RenderCycle,
    /// Значение прозрачности находится вне `[0, 1]` или не конечно.
    OpacityOutOfDomain,
    /// Конечная цель не участвует ни в одном ограничении.
    UnconstrainedFiniteTarget,
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
pub(crate) enum CompileErrorHandleV1 {
    /// Исходный сигнал.
    Source(SourceIdV1),
    /// Цель.
    Target(TargetIdV1),
    /// Точное множество family.
    Family(FamilyIdV1),
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
    /// Моделируемый presentation root.
    PresentationRoot(PresentationRootIdV1),
    /// Ограничение.
    Constraint(ConstraintIdV1),
    /// Выходной слот.
    OutputSlot(OutputSlotIdV1),
}

impl CompileErrorHandleV1 {
    /// Возвращает клиентский числовой ключ независимо от пространства ID.
    pub(crate) const fn value(self) -> u32 {
        match self {
            Self::Source(value) => value.value(),
            Self::Target(value) => value.value(),
            Self::Family(value) => value.value(),
            Self::TargetCandidate(value) => value.value(),
            Self::OpacityInput(value) => value.value(),
            Self::Paint(value) => value.value(),
            Self::SurfaceInputPort(value) => value.value(),
            Self::Surface(value) => value.value(),
            Self::Occurrence(value) => value.value(),
            Self::PresentationRoot(value) => value.value(),
            Self::Constraint(value) => value.value(),
            Self::OutputSlot(value) => value.value(),
        }
    }
}

/// Точные участники одного цикла зависимостей Paint.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PaintCycleV1 {
    paints: Vec<PaintId>,
}

impl PaintCycleV1 {
    /// Возвращает участников цикла в каноническом порядке диагностики.
    pub(crate) fn paints(&self) -> impl ExactSizeIterator<Item = PaintIdV1> + '_ {
        self.paints.iter().copied().map(PaintIdV1::from_core)
    }
}

/// Точные участники одного цикла рендера.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RenderCycleV1 {
    surfaces: Vec<SurfaceId>,
    occurrences: Vec<OccurrenceId>,
}

impl RenderCycleV1 {
    /// Возвращает Surface-участников цикла.
    pub(crate) fn surfaces(&self) -> impl ExactSizeIterator<Item = SurfaceIdV1> + '_ {
        self.surfaces.iter().copied().map(SurfaceIdV1::from_core)
    }

    /// Возвращает Occurrence-участников цикла.
    pub(crate) fn occurrences(&self) -> impl ExactSizeIterator<Item = OccurrenceIdV1> + '_ {
        self.occurrences
            .iter()
            .copied()
            .map(OccurrenceIdV1::from_core)
    }
}

/// Точная причина отказа явно объявленного конечного совместного порядка.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JointOrderErrorV1 {
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
}

/// Атомарная и полная ошибка компиляции объявленной программы.
///
/// Enum авторитетен; [`Self::kind`], [`Self::primary_handle`] и
/// [`Self::related_handle`] — только удобные проекции полного payload.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CompileErrorV1 {
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
    /// Повторно объявлен один opaque family ID.
    DuplicateFamily {
        /// Повторный family ID.
        family: FamilyIdV1,
    },
    /// Объявленный family-set не используется ни одним constraint.
    UnusedFamily {
        /// Неиспользуемый family ID.
        family: FamilyIdV1,
    },
    /// Фиксированная цель ссылается на отсутствующий исходный сигнал.
    MissingFixedSource {
        /// Ошибочная фиксированная цель.
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
    /// Повторно объявлен presentation root.
    DuplicatePresentationRoot {
        /// Повторный root ID.
        root: PresentationRootIdV1,
    },
    /// Presentation root ссылается на отсутствующий терминальный Occurrence.
    MissingPresentationRootOccurrence {
        /// Ошибочный root ID.
        root: PresentationRootIdV1,
        /// Отсутствующий Occurrence, объявленный терминалом root.
        occurrence: OccurrenceIdV1,
    },
    /// Терминальный Occurrence root всё ещё потребляется downstream.
    PresentationRootConsumedDownstream {
        /// Ошибочный root ID.
        root: PresentationRootIdV1,
        /// Occurrence, объявленный терминалом root.
        occurrence: OccurrenceIdV1,
    },
    /// Root не имеет ни одной проверяемой цели.
    UnusedPresentationRoot {
        /// Неиспользуемый root ID.
        root: PresentationRootIdV1,
    },
    /// Повторно объявлена одна пара root/target.
    DuplicatePointPresentationTarget {
        /// Root повторной пары.
        root: PresentationRootIdV1,
        /// Target Occurrence повторной пары.
        occurrence: OccurrenceIdV1,
    },
    /// Target ссылается на отсутствующий root.
    MissingPointPresentationRoot {
        /// Отсутствующий root ID.
        root: PresentationRootIdV1,
    },
    /// Target ссылается на отсутствующий Occurrence.
    MissingPointPresentationOccurrence {
        /// Root ошибочного target.
        root: PresentationRootIdV1,
        /// Отсутствующий target Occurrence.
        occurrence: OccurrenceIdV1,
    },
    /// Target не принадлежит ancestry root.
    PointPresentationOccurrenceOutsideRootAncestry {
        /// Root, относительно которого проверялся target.
        root: PresentationRootIdV1,
        /// Финальный Occurrence, объявленный терминалом root.
        terminal: OccurrenceIdV1,
        /// Target Occurrence вне ancestry терминала.
        occurrence: OccurrenceIdV1,
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
    /// В одной цели повторно объявлен ID кандидата.
    DuplicateTargetCandidate {
        /// Цель кандидата.
        target: TargetIdV1,
        /// Повторный кандидат.
        candidate: TargetCandidateIdV1,
    },
    /// Два кандидата одной цели имеют одинаковое физическое Paint value.
    DuplicateTargetCandidateValue {
        /// Цель кандидатов.
        target: TargetIdV1,
        /// Первый кандидат.
        first: TargetCandidateIdV1,
        /// Повторный кандидат.
        duplicate: TargetCandidateIdV1,
        /// Совпавшая неделимая пара source + straight alpha.
        value: PaintValueV1,
    },
    /// Конечная цель не участвует ни в одном ограничении.
    UnconstrainedFiniteTarget {
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
    /// Intrinsic-unary ограничение ссылается на отсутствующую Target.
    MissingIntrinsicUnaryTarget {
        /// Ошибочное ограничение.
        constraint: ConstraintIdV1,
        /// Отсутствующая Target.
        target: TargetIdV1,
    },
    /// Ограничение принадлежности ссылается на отсутствующий family-set.
    MissingConstraintFamily {
        /// Ошибочное ограничение.
        constraint: ConstraintIdV1,
        /// Отсутствующий family ID.
        family: FamilyIdV1,
    },
    /// Reference intrinsic-отношения ссылается на отсутствующую Target.
    MissingIntrinsicRelationReference {
        /// Ошибочное ограничение.
        constraint: ConstraintIdV1,
        /// Отсутствующий reference.
        reference: TargetIdV1,
    },
    /// Candidate intrinsic-отношения ссылается на отсутствующую Target.
    MissingIntrinsicRelationCandidate {
        /// Ошибочное ограничение.
        constraint: ConstraintIdV1,
        /// Отсутствующий candidate.
        candidate: TargetIdV1,
    },
    /// Reference visible-отношения ссылается на отсутствующий Occurrence.
    MissingVisibleRelationReference {
        /// Ошибочное ограничение.
        constraint: ConstraintIdV1,
        /// Отсутствующий reference.
        reference: OccurrenceIdV1,
    },
    /// Candidate visible-отношения ссылается на отсутствующий Occurrence.
    MissingVisibleRelationCandidate {
        /// Ошибочное ограничение.
        constraint: ConstraintIdV1,
        /// Отсутствующий candidate.
        candidate: OccurrenceIdV1,
    },
    /// Intrinsic reference зависит от выбираемого finite Target.
    SolverDependentIntrinsicRelationReference {
        /// Ошибочное ограничение.
        constraint: ConstraintIdV1,
        /// Solver-зависимый reference.
        reference: TargetIdV1,
    },
    /// Visible reference зависит от выбираемого finite Target.
    SolverDependentVisibleRelationReference {
        /// Ошибочное ограничение.
        constraint: ConstraintIdV1,
        /// Solver-зависимый reference.
        reference: OccurrenceIdV1,
        /// Finite Target, из-за которого reference зависит от solver state.
        target: TargetIdV1,
    },
    /// Ограничение clean-set ссылается не на целиком объявленную цель представления.
    MissingConstraintPresentationTarget {
        /// Ошибочное ограничение.
        constraint: ConstraintIdV1,
        /// Корень отсутствующей пары.
        root: PresentationRootIdV1,
        /// Целевой `Occurrence` отсутствующей пары.
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
    pub(crate) const fn kind(&self) -> CompileErrorKindV1 {
        use CompileErrorKindV1 as Kind;

        match self {
            Self::DuplicateSource { .. } => Kind::DuplicateSource,
            Self::DuplicateTarget { .. } => Kind::DuplicateTarget,
            Self::DuplicateFamily { .. } => Kind::DuplicateFamily,
            Self::UnusedFamily { .. } => Kind::UnusedFamily,
            Self::MissingFixedSource { .. } => Kind::MissingFixedSource,
            Self::DuplicateOpacityInput { .. } => Kind::DuplicateOpacityInput,
            Self::DuplicateSurfaceInputPort { .. } => Kind::DuplicateSurfaceInputPort,
            Self::UnusedSurfaceInputPort { .. } => Kind::UnusedSurfaceInputPort,
            Self::DuplicateSurfaceInputBinding { .. } => Kind::DuplicateSurfaceInputBinding,
            Self::DuplicatePaint { .. } => Kind::DuplicatePaint,
            Self::DuplicateSurface { .. } => Kind::DuplicateSurface,
            Self::DuplicateOccurrence { .. } => Kind::DuplicateOccurrence,
            Self::DuplicatePresentationRoot { .. } => Kind::DuplicatePresentationRoot,
            Self::MissingPresentationRootOccurrence { .. } => {
                Kind::MissingPresentationRootOccurrence
            }
            Self::PresentationRootConsumedDownstream { .. } => {
                Kind::PresentationRootConsumedDownstream
            }
            Self::UnusedPresentationRoot { .. } => Kind::UnusedPresentationRoot,
            Self::DuplicatePointPresentationTarget { .. } => Kind::DuplicatePointPresentationTarget,
            Self::MissingPointPresentationRoot { .. } => Kind::MissingPointPresentationRoot,
            Self::MissingPointPresentationOccurrence { .. } => {
                Kind::MissingPointPresentationOccurrence
            }
            Self::PointPresentationOccurrenceOutsideRootAncestry { .. } => {
                Kind::PointPresentationOccurrenceOutsideRootAncestry
            }
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
            Self::DuplicateTargetCandidate { .. } => Kind::DuplicateTargetCandidate,
            Self::DuplicateTargetCandidateValue { .. } => Kind::DuplicateTargetCandidateValue,
            Self::UnconstrainedFiniteTarget { .. } => Kind::UnconstrainedFiniteTarget,
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
            Self::MissingIntrinsicUnaryTarget { .. } => Kind::MissingIntrinsicUnaryTarget,
            Self::MissingConstraintFamily { .. } => Kind::MissingConstraintFamily,
            Self::MissingIntrinsicRelationReference { .. } => {
                Kind::MissingIntrinsicRelationReference
            }
            Self::MissingIntrinsicRelationCandidate { .. } => {
                Kind::MissingIntrinsicRelationCandidate
            }
            Self::MissingVisibleRelationReference { .. } => Kind::MissingVisibleRelationReference,
            Self::MissingVisibleRelationCandidate { .. } => Kind::MissingVisibleRelationCandidate,
            Self::SolverDependentIntrinsicRelationReference { .. } => {
                Kind::SolverDependentIntrinsicRelationReference
            }
            Self::SolverDependentVisibleRelationReference { .. } => {
                Kind::SolverDependentVisibleRelationReference
            }
            Self::MissingConstraintPresentationTarget { .. } => {
                Kind::MissingConstraintPresentationTarget
            }
            Self::DuplicateOutputSlot { .. } => Kind::DuplicateOutputSlot,
            Self::MissingOutputPaint { .. } => Kind::MissingOutputPaint,
            Self::ResourceExhausted => Kind::ResourceExhausted,
            Self::InternalInvariant => Kind::InternalInvariant,
        }
    }

    /// Возвращает основной типизированный ID, если ошибка локализуема одним узлом.
    pub(crate) const fn primary_handle(&self) -> Option<CompileErrorHandleV1> {
        use CompileErrorHandleV1 as Handle;

        match self {
            Self::DuplicateSource { source } => Some(Handle::Source(*source)),
            Self::DuplicateTarget { target }
            | Self::UnconstrainedFiniteTarget { target }
            | Self::JointStateDuplicateTarget { target, .. }
            | Self::JointStateMissingTarget { target, .. }
            | Self::JointStateUnknownTarget { target, .. }
            | Self::JointStateUnknownCandidate { target, .. }
            | Self::MissingFixedSource { target, .. }
            | Self::DuplicateTargetCandidate { target, .. }
            | Self::DuplicateTargetCandidateValue { target, .. } => Some(Handle::Target(*target)),
            Self::DuplicateFamily { family } | Self::UnusedFamily { family } => {
                Some(Handle::Family(*family))
            }
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
            Self::DuplicatePresentationRoot { root }
            | Self::MissingPresentationRootOccurrence { root, .. }
            | Self::PresentationRootConsumedDownstream { root, .. }
            | Self::UnusedPresentationRoot { root }
            | Self::DuplicatePointPresentationTarget { root, .. }
            | Self::MissingPointPresentationRoot { root }
            | Self::MissingPointPresentationOccurrence { root, .. }
            | Self::PointPresentationOccurrenceOutsideRootAncestry { root, .. } => {
                Some(Handle::PresentationRoot(*root))
            }
            Self::UnassessedOutput { output, .. }
            | Self::DuplicateOutputSlot { output }
            | Self::MissingOutputPaint { output, .. } => Some(Handle::OutputSlot(*output)),
            Self::DuplicateConstraint { constraint }
            | Self::MissingConstraintOccurrence { constraint, .. }
            | Self::MissingIntrinsicUnaryTarget { constraint, .. }
            | Self::MissingConstraintFamily { constraint, .. }
            | Self::MissingIntrinsicRelationReference { constraint, .. }
            | Self::MissingIntrinsicRelationCandidate { constraint, .. }
            | Self::MissingVisibleRelationReference { constraint, .. }
            | Self::MissingVisibleRelationCandidate { constraint, .. }
            | Self::SolverDependentIntrinsicRelationReference { constraint, .. }
            | Self::SolverDependentVisibleRelationReference { constraint, .. }
            | Self::MissingConstraintPresentationTarget { constraint, .. } => {
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
    pub(crate) const fn related_handle(&self) -> Option<CompileErrorHandleV1> {
        use CompileErrorHandleV1 as Handle;

        match self {
            Self::MissingFixedSource { source, .. } => Some(Handle::Source(*source)),
            Self::DuplicateSurfaceInputBinding { duplicate, .. } => {
                Some(Handle::Surface(*duplicate))
            }
            Self::MissingPaintTarget { target, .. } => Some(Handle::Target(*target)),
            Self::MissingPaintSource { source, .. } => Some(Handle::Paint(*source)),
            Self::MissingPaintOpacityInput { input, .. } => Some(Handle::OpacityInput(*input)),
            Self::MissingSurfaceInputPort { input, .. } => Some(Handle::SurfaceInputPort(*input)),
            Self::MissingSurfaceOccurrence { occurrence, .. }
            | Self::MissingConstraintOccurrence { occurrence, .. }
            | Self::MissingVisibleRelationReference {
                reference: occurrence,
                ..
            }
            | Self::MissingVisibleRelationCandidate {
                candidate: occurrence,
                ..
            }
            | Self::MissingConstraintPresentationTarget { occurrence, .. }
            | Self::MissingPresentationRootOccurrence { occurrence, .. }
            | Self::PresentationRootConsumedDownstream { occurrence, .. }
            | Self::DuplicatePointPresentationTarget { occurrence, .. }
            | Self::MissingPointPresentationOccurrence { occurrence, .. }
            | Self::PointPresentationOccurrenceOutsideRootAncestry { occurrence, .. } => {
                Some(Handle::Occurrence(*occurrence))
            }
            Self::MissingIntrinsicRelationReference { reference, .. }
            | Self::MissingIntrinsicUnaryTarget {
                target: reference, ..
            }
            | Self::SolverDependentIntrinsicRelationReference { reference, .. } => {
                Some(Handle::Target(*reference))
            }
            Self::MissingIntrinsicRelationCandidate { candidate, .. } => {
                Some(Handle::Target(*candidate))
            }
            Self::MissingConstraintFamily { family, .. } => Some(Handle::Family(*family)),
            Self::SolverDependentVisibleRelationReference { target, .. } => {
                Some(Handle::Target(*target))
            }
            Self::MissingOccurrencePaint { paint, .. }
            | Self::UnassessedOutput { paint, .. }
            | Self::MissingOutputPaint { paint, .. } => Some(Handle::Paint(*paint)),
            Self::MissingOccurrenceBackdrop { surface, .. } => Some(Handle::Surface(*surface)),
            Self::DuplicateTargetCandidate { candidate, .. }
            | Self::DuplicateTargetCandidateValue {
                duplicate: candidate,
                ..
            }
            | Self::JointStateUnknownCandidate { candidate, .. } => {
                Some(Handle::TargetCandidate(*candidate))
            }
            Self::DuplicateSource { .. }
            | Self::DuplicateTarget { .. }
            | Self::DuplicateFamily { .. }
            | Self::UnusedFamily { .. }
            | Self::DuplicateOpacityInput { .. }
            | Self::DuplicateSurfaceInputPort { .. }
            | Self::UnusedSurfaceInputPort { .. }
            | Self::DuplicatePaint { .. }
            | Self::DuplicateSurface { .. }
            | Self::DuplicateOccurrence { .. }
            | Self::DuplicatePresentationRoot { .. }
            | Self::UnusedPresentationRoot { .. }
            | Self::MissingPointPresentationRoot { .. }
            | Self::PaintCycle(_)
            | Self::RenderCycle(_)
            | Self::OpacityOutOfDomain { .. }
            | Self::UnconstrainedFiniteTarget { .. }
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
pub(crate) struct DraftV1 {
    inner: CoreProgramDraftV1,
}

/// Ошибка изменения Draft до компиляции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DraftErrorV1 {
    /// Совместный порядок уже объявлен и не может быть молча заменён.
    JointSelectionAlreadyDeclared,
}

impl DraftV1 {
    /// Создаёт пустой Draft.
    pub(crate) fn new() -> Self {
        Self {
            inner: CoreProgramDraftV1::new(),
        }
    }

    /// Объявляет неизменяемый исходный encoded sRGB8 сигнал.
    pub(crate) fn push_source(&mut self, id: SourceIdV1, source: Srgb8) -> &mut Self {
        self.inner
            .push_source(Source::new(id.into_core(), ColorSignal::from_srgb8(source)));
        self
    }

    /// Объявляет цель, физически равную исходному сигналу.
    pub(crate) fn push_fixed_target(&mut self, id: TargetIdV1, source: SourceIdV1) -> &mut Self {
        self.inner
            .push_target(Target::fixed(id.into_core(), source.into_core()));
        self
    }

    /// Объявляет решаемую цель с конечным набором физических кандидатов.
    pub(crate) fn push_finite_target(
        &mut self,
        id: TargetIdV1,
        domain: FinitePaintDomainV1,
    ) -> &mut Self {
        self.inner
            .push_target(Target::finite(id.into_core(), domain.0));
        self
    }

    /// Объявляет одно точное допущенное множество без клиентской семантики.
    pub(crate) fn push_family(
        &mut self,
        id: FamilyIdV1,
        semantic: FamilySemanticReleaseV2,
    ) -> &mut Self {
        self.inner.push_family(FamilyDeclarationV2::new(
            id.into_core(),
            semantic.into_core(),
        ));
        self
    }

    /// Только тестовый шов совместимости для фикстур, созданных до `SelectionRelease`.
    #[cfg(test)]
    pub(crate) fn set_joint_selection(
        &mut self,
        states: Vec<JointStateV1>,
    ) -> Result<&mut Self, DraftErrorV1> {
        let order =
            DeclaredJointSelectionV1::new(states.into_iter().map(|state| state.0).collect());
        self.set_materialised_joint_selection(
            crate::selection_release::materialise_declared_joint_selection_for_test(order),
        )
    }

    /// Принимает полный порядок, материализованный допущенным `SelectionRelease`.
    pub(crate) fn set_materialised_joint_selection(
        &mut self,
        selection: MaterialisedSelectionV1,
    ) -> Result<&mut Self, DraftErrorV1> {
        self.inner
            .set_materialised_joint_selection(selection)
            .map_err(|error| match error {
                CoreProgramDraftErrorV1::JointSelectionAlreadyDeclared => {
                    DraftErrorV1::JointSelectionAlreadyDeclared
                }
            })?;
        Ok(self)
    }

    /// Объявляет один динамический вход поверхности.
    pub(crate) fn push_surface_input_port(&mut self, input: SurfaceInputPortIdV1) -> &mut Self {
        self.inner.push_surface_input_port(input.into_core());
        self
    }

    /// Объявляет числовой вход прозрачности; домен проверяется при компиляции.
    pub(crate) fn push_opacity_input(&mut self, id: OpacityInputIdV1, value: f64) -> &mut Self {
        self.inner
            .push_opacity_input(OpacityInput::new(id.into_core(), value));
        self
    }

    /// Объявляет непрозрачный Paint, связанный с целью.
    pub(crate) fn push_solid_paint(&mut self, id: PaintIdV1, target: TargetIdV1) -> &mut Self {
        self.inner.push_paint(Paint::Solid {
            id: id.into_core(),
            target: target.into_core(),
        });
        self
    }

    /// Объявляет Paint как прозрачную версию другого Paint.
    pub(crate) fn push_opacity_paint(
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
    pub(crate) fn push_input_surface(
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
    pub(crate) fn push_occurrence_surface(
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
    pub(crate) fn push_source_over_occurrence(
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

    /// Объявляет один терминальный корень моделируемого точечного графа.
    pub(crate) fn push_point_presentation_root(
        &mut self,
        id: PresentationRootIdV1,
        terminal: OccurrenceIdV1,
    ) -> &mut Self {
        self.inner
            .push_point_presentation_root(PointPresentationRootV1::new(
                id.into_core(),
                terminal.into_core(),
            ));
        self
    }

    /// Объявляет целевой `Occurrence`, для которого компилятор обязан доказать
    /// путь к указанному корню при версионированном правиле Core, моделирующем
    /// отсутствие этого `Occurrence`.
    pub(crate) fn push_point_presentation_target(
        &mut self,
        root: PresentationRootIdV1,
        occurrence: OccurrenceIdV1,
    ) -> &mut Self {
        self.inner
            .push_point_presentation_target(PointPresentationTargetV1::new(
                root.into_core(),
                occurrence.into_core(),
            ));
        self
    }

    /// Добавляет обязательное точное сравнение видимого sRGB8 результата.
    pub(crate) fn push_exact_visible_unary_hard(
        &mut self,
        id: ConstraintIdV1,
        occurrence: OccurrenceIdV1,
        expected: Srgb8,
    ) -> &mut Self {
        self.inner
            .push_hard_constraint(ConstraintInvocation::visible_unary_hard(
                id.into_core(),
                occurrence.into_core(),
                CoreProgramConstraintInvocationV1::ExactSrgb8(expected),
            ));
        self
    }

    /// Добавляет диагностическое точное сравнение, не влияющее на выбор.
    pub(crate) fn push_exact_visible_unary_report_only(
        &mut self,
        id: ConstraintIdV1,
        occurrence: OccurrenceIdV1,
        expected: Srgb8,
    ) -> &mut Self {
        self.inner
            .push_report_constraint(ConstraintInvocation::visible_unary_report_only(
                id.into_core(),
                occurrence.into_core(),
                CoreProgramConstraintInvocationV1::ExactSrgb8(expected),
            ));
        self
    }

    /// Добавляет обязательную exact-проверку intrinsic source одной Target.
    pub(crate) fn push_exact_intrinsic_unary_hard(
        &mut self,
        id: ConstraintIdV1,
        target: TargetIdV1,
        expected: Srgb8,
    ) -> &mut Self {
        self.inner
            .push_exact_intrinsic_unary_hard(id.into_core(), target.into_core(), expected);
        self
    }

    /// Требует принадлежности исходного сигнала объявленному точному множеству.
    pub(crate) fn push_intrinsic_family_membership_hard(
        &mut self,
        id: ConstraintIdV1,
        target: TargetIdV1,
        family: FamilyIdV1,
    ) -> &mut Self {
        self.inner.push_intrinsic_family_membership_hard(
            id.into_core(),
            target.into_core(),
            family.into_core(),
        );
        self
    }

    /// Диагностирует принадлежность без права исключать состояние-кандидат.
    pub(crate) fn push_intrinsic_family_membership_report_only(
        &mut self,
        id: ConstraintIdV1,
        target: TargetIdV1,
        family: FamilyIdV1,
    ) -> &mut Self {
        self.inner.push_intrinsic_family_membership_report_only(
            id.into_core(),
            target.into_core(),
            family.into_core(),
        );
        self
    }

    /// Добавляет обязательное exact-отношение между intrinsic Target signals.
    pub(crate) fn push_exact_intrinsic_relation_hard(
        &mut self,
        id: ConstraintIdV1,
        relation: DirectedRelationV1<TargetIdV1>,
    ) -> &mut Self {
        self.inner.push_exact_intrinsic_relation_hard(
            id.into_core(),
            relation
                .try_map(TargetIdV1::into_core)
                .expect("facade TargetId is a transparent bijection over Core TargetId"),
        );
        self
    }

    /// Добавляет обязательное exact-отношение между final modeled Occurrences.
    pub(crate) fn push_exact_visible_relation_hard(
        &mut self,
        id: ConstraintIdV1,
        relation: DirectedRelationV1<OccurrenceIdV1>,
    ) -> &mut Self {
        self.inner.push_exact_visible_relation_hard(
            id.into_core(),
            relation
                .try_map(OccurrenceIdV1::into_core)
                .expect("facade OccurrenceId is a transparent bijection over Core OccurrenceId"),
        );
        self
    }

    /// Требует точного байтового различия intrinsic source каждого кандидата
    /// от reference. Закон технический: он не заявляет перцептивную
    /// различимость и не читает клиентскую семантику ID.
    pub(crate) fn push_exact_intrinsic_distinction_hard(
        &mut self,
        id: ConstraintIdV1,
        relation: DirectedRelationV1<TargetIdV1>,
    ) -> &mut Self {
        self.inner.push_exact_intrinsic_distinction_hard(
            id.into_core(),
            relation
                .try_map(TargetIdV1::into_core)
                .expect("facade TargetId is a transparent bijection over Core TargetId"),
        );
        self
    }

    /// Требует точного байтового различия final modeled результата каждого
    /// кандидата от reference в объявленных контекстах.
    pub(crate) fn push_exact_visible_distinction_hard(
        &mut self,
        id: ConstraintIdV1,
        relation: DirectedRelationV1<OccurrenceIdV1>,
    ) -> &mut Self {
        self.inner.push_exact_visible_distinction_hard(
            id.into_core(),
            relation
                .try_map(OccurrenceIdV1::into_core)
                .expect("facade OccurrenceId is a transparent bijection over Core OccurrenceId"),
        );
        self
    }

    /// Требует положительной принадлежности reference и каждого кандидата
    /// одному объявленному точному family-образу. Оба endpoint обязаны нести
    /// собственный inclusion witness; отрицательное дополнение недостаточно.
    pub(crate) fn push_intrinsic_family_category_relation_hard(
        &mut self,
        id: ConstraintIdV1,
        relation: DirectedRelationV1<TargetIdV1>,
        family: FamilyIdV1,
    ) -> &mut Self {
        self.inner.push_intrinsic_family_category_relation_hard(
            id.into_core(),
            relation
                .try_map(TargetIdV1::into_core)
                .expect("facade TargetId is a transparent bijection over Core TargetId"),
            family.into_core(),
        );
        self
    }

    /// Добавляет обязательный критерий WCAG 2.2 для видимого результата.
    pub(crate) fn push_wcag22_visible_unary_hard(
        &mut self,
        id: ConstraintIdV1,
        occurrence: OccurrenceIdV1,
        criterion: Wcag22CriterionV1,
    ) -> &mut Self {
        self.inner
            .push_hard_constraint(ConstraintInvocation::visible_unary_hard(
                id.into_core(),
                occurrence.into_core(),
                CoreProgramConstraintInvocationV1::Wcag22Srgb8(criterion),
            ));
        self
    }

    /// Добавляет диагностический критерий WCAG 2.2, не влияющий на выбор.
    pub(crate) fn push_wcag22_visible_unary_report_only(
        &mut self,
        id: ConstraintIdV1,
        occurrence: OccurrenceIdV1,
        criterion: Wcag22CriterionV1,
    ) -> &mut Self {
        self.inner
            .push_report_constraint(ConstraintInvocation::visible_unary_report_only(
                id.into_core(),
                occurrence.into_core(),
                CoreProgramConstraintInvocationV1::Wcag22Srgb8(criterion),
            ));
        self
    }

    /// Требует принадлежности непустого финального вклада точной цели
    /// представления к закреплённому пакетом множеству encoded sRGB8.
    pub(crate) fn push_declared_srgb8_clean_set_hard(
        &mut self,
        id: ConstraintIdV1,
        root: PresentationRootIdV1,
        occurrence: OccurrenceIdV1,
    ) -> &mut Self {
        self.inner.push_declared_srgb8_clean_set_hard(
            id.into_core(),
            PointPresentationTargetV1::new(root.into_core(), occurrence.into_core()),
        );
        self
    }

    /// Диагностирует тот же закреплённый пакетом предикат, не влияя на выбор.
    pub(crate) fn push_declared_srgb8_clean_set_report_only(
        &mut self,
        id: ConstraintIdV1,
        root: PresentationRootIdV1,
        occurrence: OccurrenceIdV1,
    ) -> &mut Self {
        self.inner.push_declared_srgb8_clean_set_report_only(
            id.into_core(),
            PointPresentationTargetV1::new(root.into_core(), occurrence.into_core()),
        );
        self
    }

    #[cfg(test)]
    pub(crate) fn push_declared_srgb8_clean_set_final_recheck_mutant(
        &mut self,
        id: ConstraintIdV1,
        root: PresentationRootIdV1,
        occurrence: OccurrenceIdV1,
    ) -> &mut Self {
        self.inner
            .push_declared_srgb8_clean_set_final_recheck_mutant(
                id.into_core(),
                PointPresentationTargetV1::new(root.into_core(), occurrence.into_core()),
            );
        self
    }

    /// Связывает клиентский output slot с выбранным encoded Paint.
    pub(crate) fn push_output(&mut self, output: OutputSlotIdV1, paint: PaintIdV1) -> &mut Self {
        self.inner
            .push_output(OutputBinding::new(output.into_core(), paint.into_core()));
        self
    }

    /// Атомарно проверяет и компилирует весь граф.
    pub(crate) fn compile(self) -> Result<OwnerV1, CompileErrorV1> {
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
/// Созданные им standalone Session изменяются только через эту же аллокацию.
/// Attachment атомарно удерживает собственный strong pin той же эпохи, поэтому
/// уничтожение внешнего Owner не отзывает уже присоединённый runtime. Без такого
/// pin исторические evidence остаются читаемыми, но новые обновления недоступны.
pub(crate) struct OwnerV1 {
    compiled: CompiledCoreProgramV1,
}

/// Верхние границы числа клеток в новом сертификате одного Observed-update.
///
/// Границы относятся только к текущим клеткам доказательства. Они не включают
/// сохранённый прошлый сертификат, observation/provenance, выходы, операции или
/// байты конкретного транспорта.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EvidenceCellBoundsV1 {
    verified_cells: usize,
    conflict_cells: usize,
}

impl EvidenceCellBoundsV1 {
    /// Максимум клеток успешного сертификата.
    pub(crate) const fn verified_cells(self) -> usize {
        self.verified_cells
    }

    /// Максимум клеток исчерпывающего конфликтного сертификата.
    pub(crate) const fn conflict_cells(self) -> usize {
        self.conflict_cells
    }
}

/// Закрытая причина невозможности вычислить границы сертификата.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EvidenceBoundsErrorV1 {
    /// Произведение числа сценариев, ограничений и состояний не помещается в
    /// адресное пространство платформы.
    CardinalityOverflow,
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
    pub(crate) fn content_identity(&self) -> ContentIdentityV8 {
        ContentIdentityV8::from_core(self.compiled.content_identity())
    }

    /// Точный авторский `SelectionRelease` конечного Program; у fixed-only
    /// Program нет полномочий выбора.
    pub(crate) fn selection_release_identity(&self) -> Option<SelectionReleaseIdentityV1> {
        self.compiled.selection_release_identity()
    }

    /// Вычисляет верхние границы клеток для prospective Observed-update.
    ///
    /// `scenario_count` — число объявленных клиентом сценариев до admission.
    /// Core сам схлопывает физически одинаковые сценарии, поэтому фактический
    /// сертификат может быть короче. Нулевое значение разрешено только как
    /// чистый арифметический preflight; пустой Observed-update по-прежнему не
    /// допускается. Запрос не создаёт Session и не меняет состояние.
    pub(crate) fn evidence_cell_bounds(
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
    pub(crate) fn surface_input_port_count(&self) -> usize {
        self.compiled.surface_input_ports().len()
    }

    /// Число допущенных компилятором связей между целью и корнем представления точки.
    pub(crate) fn point_presentation_count(&self) -> usize {
        self.compiled.point_presentation_count()
    }

    #[cfg(test)]
    pub(crate) fn point_resolution_count_for_test(
        &self,
        session: &SessionV1,
    ) -> Option<(usize, usize)> {
        self.compiled
            .point_resolution_count_for_test(&session.session)
    }

    /// Канонический порядок входных портов для однократного binding на хосте.
    pub(crate) fn surface_input_ports(
        &self,
    ) -> impl ExactSizeIterator<Item = SurfaceInputPortIdV1> + '_ {
        self.compiled
            .surface_input_ports()
            .iter()
            .copied()
            .map(SurfaceInputPortIdV1::from_core)
    }

    /// Канонический порядок непрозрачных выходных слотов.
    pub(crate) fn output_slots(&self) -> impl ExactSizeIterator<Item = OutputSlotIdV1> + '_ {
        self.compiled
            .outputs()
            .map(|(slot, _paint)| OutputSlotIdV1::from_core(slot))
    }

    /// Unique semantic releases that the host must resolve through its trusted
    /// artifact manifest before constructing a Session or Attachment.
    pub(crate) fn required_family_releases(
        &self,
    ) -> impl ExactSizeIterator<Item = FamilySemanticReleaseV2> + '_ {
        self.compiled
            .required_family_releases()
            .iter()
            .copied()
            .map(FamilySemanticReleaseV2::from_core)
    }

    /// Допускает update без изменения зафиксированных raw head и lifecycle.
    ///
    /// Несовпадение Owner проверяется до admission, аллокаций и вычисления.
    /// Raw head, lifecycle и previous evidence меняются только в consuming
    /// commit; внутренние scratch-буферы могут быть переинициализированы здесь.
    pub(crate) fn prepare_update<'session>(
        &self,
        session: &'session mut SessionV1,
        update: UpdateV1<'_>,
    ) -> Result<PreparedSessionTransitionV1<'session>, UpdateErrorV1> {
        if !self.compiled.owns_session(&session.session) {
            return Err(UpdateErrorV1::OwnerMismatch);
        }
        let transition = session.prepare_update(update)?;
        Ok(PreparedSessionTransitionV1 { transition })
    }

    /// Создаёт Session, привязанную к одному непрозрачному stream ID.
    pub(crate) fn instantiate(&self, stream_id: u32) -> Result<SessionV1, InstantiateErrorV1> {
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

    pub(crate) fn instantiate_with_family_artifacts(
        &self,
        stream_id: u32,
        family_artifacts: FamilyArtifactBundleV2,
    ) -> Result<SessionV1, InstantiateFailureV2> {
        let stream = ObservationStreamId::new(stream_id);
        match self
            .compiled
            .instantiate_with_family_artifacts(stream, family_artifacts)
        {
            Ok(session) => Ok(SessionV1 {
                scenario_order_scratch: Vec::new(),
                session,
            }),
            Err(failure) => {
                let (cause, family_artifacts) = failure.into_parts();
                Err(InstantiateFailureV2 {
                    cause: InstantiateErrorV1::from_core(cause),
                    family_artifacts,
                })
            }
        }
    }
}

/// Cold Session failure retains the same loaded family storage for retry.
pub(crate) struct InstantiateFailureV2 {
    cause: InstantiateErrorV1,
    family_artifacts: FamilyArtifactBundleV2,
}

impl fmt::Debug for InstantiateFailureV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InstantiateFailureV2")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}

impl InstantiateFailureV2 {
    pub(crate) const fn cause(&self) -> InstantiateErrorV1 {
        self.cause
    }

    pub(crate) fn into_parts(self) -> (InstantiateErrorV1, FamilyArtifactBundleV2) {
        (self.cause, self.family_artifacts)
    }
}

/// Один заимствованный физический сценарий в скомпилированном schema order.
///
/// ID сценария — непрозрачная provenance. `values` содержит ровно один encoded
/// sRGB8 на каждый [`OwnerV1::surface_input_ports`] в том же порядке.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScenarioV1<'a> {
    scenario_id: u32,
    values: &'a [Srgb8],
}

impl<'a> ScenarioV1<'a> {
    /// Создаёт один одновременный физический кортеж.
    pub(crate) const fn new(scenario_id: u32, values: &'a [Srgb8]) -> Self {
        Self {
            scenario_id,
            values,
        }
    }
}

/// Одно revision-bound обновление; stream принадлежит Session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateV1<'a> {
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
pub(crate) struct SessionV1 {
    scenario_order_scratch: Vec<usize>,
    session: CoreProgramSessionV1,
}

impl SessionV1 {
    /// Возвращает исторические evidence без права на операции.
    pub(crate) fn evidence(&self) -> EvidenceViewV1<'_> {
        EvidenceViewV1 {
            session: self.session.view(),
        }
    }

    fn prepare_update(
        &mut self,
        update: UpdateV1<'_>,
    ) -> Result<CorePreparedSessionTransitionV1<'_>, UpdateErrorV1> {
        match update {
            UpdateV1::Observed {
                revision,
                scenarios,
            } => {
                let source = ScenarioSourceV1(scenarios);
                self.session
                    .prepare_schema_ordered(
                        Revision::new(revision),
                        &source,
                        &mut self.scenario_order_scratch,
                    )
                    .map_err(map_session_update_error)
            }
            UpdateV1::Unknown {
                revision,
                reason_id,
            } => self
                .session
                .prepare_unknown(Revision::new(revision), UnknownReasonId::new(reason_id))
                .map_err(map_session_update_error),
        }
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
pub(crate) enum StateKindV1 {
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
pub(crate) enum ObservationHeadV1 {
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
pub(crate) struct EvidenceViewV1<'a> {
    session: CoreProgramSessionViewV1<'a>,
}

impl<'a> EvidenceViewV1<'a> {
    const fn state(self) -> &'a CoreProgramStateV1 {
        self.session.state()
    }

    /// Возвращает lifecycle-класс текущего состояния.
    pub(crate) const fn kind(self) -> StateKindV1 {
        match self.state() {
            SessionState::Waiting => StateKindV1::Waiting,
            SessionState::Ready { .. } => StateKindV1::Ready,
            SessionState::Stale { .. } => StateKindV1::Stale,
            SessionState::Failed { .. } => StateKindV1::Failed,
        }
    }

    /// Возвращает сырую голову наблюдений вместе с provenance.
    pub(crate) fn observation_head(self) -> ObservationHeadV1 {
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
    pub(crate) const fn cause_certificate_index(self) -> Option<usize> {
        match self.state() {
            SessionState::Failed { .. } => Some(0),
            SessionState::Waiting | SessionState::Ready { .. } | SessionState::Stale { .. } => None,
        }
    }

    /// Сертификаты в каноническом порядке одного снимка.
    pub(crate) fn certificates(
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

/// Полностью вычисленный, но ещё не опубликованный переход одной Session.
///
/// Тип линейный: он не реализует Clone/Copy. Drop сохраняет зафиксированные raw
/// head, lifecycle и previous evidence; переинициализированный scratch не
/// откатывается и не является наблюдаемым состоянием Session. [`Self::commit`]
/// не выполняет fallible work и не утверждает запись в sink.
#[must_use = "commit the prepared transition or drop it intentionally"]
pub(crate) struct PreparedSessionTransitionV1<'session> {
    transition: CorePreparedSessionTransitionV1<'session>,
}

impl<'session> PreparedSessionTransitionV1<'session> {
    /// Публикует только уже подготовленные raw head и lifecycle Session и
    /// возвращает exact evidence-only snapshot без sink-authority.
    pub(crate) fn commit(self) -> EvidenceViewV1<'session> {
        EvidenceViewV1 {
            session: self.transition.commit(),
        }
    }
}

/// Collision-resistant адрес канонического физического содержания Program.
///
/// Identity не идентифицирует owner-эпоху и не даёт runtime-полномочий.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ContentIdentityV8([u8; 32]);

impl ContentIdentityV8 {
    const fn from_core(value: ProgramContentIdentityV8) -> Self {
        Self(*value.as_bytes())
    }

    /// Возвращает 256-битное каноническое представление identity.
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Доказательство прохождения всех hard-клеток на полном physical support.
#[derive(Clone, Copy)]
pub(crate) struct VerifiedCertificateV1<'a> {
    inner: &'a CoreVerifiedV1,
}

impl<'a> VerifiedCertificateV1<'a> {
    /// Возвращает identity скомпилированного содержания.
    pub(crate) const fn content_identity(self) -> ContentIdentityV8 {
        ContentIdentityV8::from_core(self.inner.report().content_identity())
    }

    pub(crate) const fn selection_release_identity(self) -> Option<SelectionReleaseIdentityV1> {
        self.inner.report().selection_release_identity()
    }

    /// Возвращает точное наблюдение, на котором выдан сертификат.
    pub(crate) const fn observation(self) -> ObservationV1<'a> {
        ObservationV1 {
            inner: self.inner.report().observation(),
        }
    }

    /// Возвращает индекс выбранного состояния или `None` для fixed Program.
    pub(crate) const fn selected_state_index(self) -> Option<usize> {
        self.inner.selected_state_index()
    }

    /// Возвращает все `case × constraint` клетки выбранного состояния.
    pub(crate) fn cells(
        self,
    ) -> impl ExactSizeIterator<Item = VerifiedCellV1<'a>> + FusedIterator + 'a {
        let report = self.inner.report();
        report
            .cells()
            .iter()
            .map(move |cell| VerifiedCellV1::from_core(report, cell))
    }

    /// Возвращает все сертифицированные Paint outputs в каноническом порядке.
    pub(crate) fn outputs(
        self,
    ) -> impl ExactSizeIterator<Item = CertifiedPaintOutputV1<'a>> + FusedIterator + 'a {
        self.inner
            .outputs()
            .iter()
            .map(CertifiedPaintOutputV1::from_core)
    }
}

/// Исчерпывающее доказательство, что каждое состояние нарушает hard-клетку.
#[derive(Clone, Copy)]
pub(crate) struct ConflictCertificateV1<'a> {
    inner: &'a CoreConflictV1,
}

impl<'a> ConflictCertificateV1<'a> {
    /// Возвращает identity скомпилированного содержания.
    pub(crate) const fn content_identity(self) -> ContentIdentityV8 {
        ContentIdentityV8::from_core(self.inner.report().content_identity())
    }

    pub(crate) const fn selection_release_identity(self) -> Option<SelectionReleaseIdentityV1> {
        self.inner.report().selection_release_identity()
    }

    /// Возвращает точное наблюдение, вызвавшее конфликт.
    pub(crate) const fn observation(self) -> ObservationV1<'a> {
        ObservationV1 {
            inner: self.inner.report().observation(),
        }
    }

    /// Возвращает число исчерпывающе рассмотренных состояний.
    pub(crate) const fn considered_state_count(self) -> usize {
        self.inner.considered_state_count()
    }

    /// Возвращает все `state × case × constraint` клетки конфликта.
    pub(crate) fn cells(
        self,
    ) -> impl ExactSizeIterator<Item = ConflictCellV1<'a>> + FusedIterator + 'a {
        let report = self.inner.report();
        report
            .cells()
            .iter()
            .map(move |cell| ConflictCellV1::from_core(report, cell))
    }
}

/// Закрытая заимствованная проекция одного Core-owned сертификата.
///
/// Сертификат заимствует только историю Session и может пережить Owner,
/// разрешивший исходную проекцию.
#[derive(Clone, Copy)]
pub(crate) enum CertificateV1<'a> {
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
    pub(crate) const fn content_identity(self) -> ContentIdentityV8 {
        match self {
            Self::Verified(value) => value.content_identity(),
            Self::Conflict(value) => value.content_identity(),
        }
    }

    pub(crate) const fn selection_release_identity(self) -> Option<SelectionReleaseIdentityV1> {
        match self {
            Self::Verified(value) => value.selection_release_identity(),
            Self::Conflict(value) => value.selection_release_identity(),
        }
    }

    /// Возвращает точное revision-bound наблюдение сертификата.
    pub(crate) const fn observation(self) -> ObservationV1<'a> {
        match self {
            Self::Verified(value) => value.observation(),
            Self::Conflict(value) => value.observation(),
        }
    }

    #[cfg(test)]
    pub(crate) fn observation_backing_ptr_for_test(self) -> *const () {
        self.observation().inner.backing_ptr_for_test()
    }

    #[cfg(test)]
    pub(crate) fn identity_for_test(self) -> (*const (), *const ()) {
        let certificate: *const () = match self {
            Self::Verified(value) => core::ptr::from_ref(value.inner).cast(),
            Self::Conflict(value) => core::ptr::from_ref(value.inner).cast(),
        };
        (certificate, self.observation_backing_ptr_for_test())
    }
}

/// Точное revision-bound наблюдение, сохранённое сертификатом.
#[derive(Clone, Copy)]
pub(crate) struct ObservationV1<'a> {
    inner: &'a crate::observation::RevisionBoundObservationV1,
}

impl<'a> ObservationV1<'a> {
    /// Возвращает stream provenance наблюдения.
    pub(crate) const fn stream(self) -> StreamIdV1 {
        StreamIdV1::from_core(self.inner.stream())
    }

    /// Возвращает ревизию наблюдения.
    pub(crate) const fn revision(self) -> u64 {
        self.inner.revision().value()
    }

    /// Возвращает каноническую schema, общую для всех физических cases.
    ///
    /// Позиция `i` соответствует позиции `i` в [`PhysicalCaseV1::values`].
    pub(crate) fn surface_input_ports(
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
    pub(crate) fn physical_cases(
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
pub(crate) enum SignalV1 {
    /// Encoded sRGB8 в IEC 61966-2-1 с белой точкой D65.
    Iec61966Srgb8D65(Srgb8),
}

/// Один канонический физический case и его полная provenance.
#[derive(Clone, Copy)]
pub(crate) struct PhysicalCaseV1<'a> {
    observation: &'a crate::observation::RevisionBoundObservationV1,
    index: usize,
}

impl<'a> PhysicalCaseV1<'a> {
    /// Возвращает значения case в каноническом schema order.
    pub(crate) fn values(self) -> impl ExactSizeIterator<Item = SignalV1> + FusedIterator + 'a {
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
    pub(crate) fn provenance(
        self,
    ) -> impl ExactSizeIterator<Item = ScenarioIdV1> + FusedIterator + 'a {
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
pub(crate) enum ConstraintModeV1 {
    /// Нарушение запрещает состояние.
    Hard,
    /// Результат сохраняется, но не влияет на выбор.
    ReportOnly,
}

/// Полный физический объект ограничения без подстановки одного лишь `Occurrence`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConstraintSubjectV1 {
    /// Видимый результат одного `Occurrence` в объявленном для него контексте.
    VisibleUnary {
        occurrence: OccurrenceIdV1,
        context: AppearanceContextV1,
    },
    IntrinsicUnary {
        target: TargetIdV1,
    },
    IntrinsicRelation {
        reference: TargetIdV1,
    },
    VisibleRelation {
        reference: OccurrenceIdV1,
        context: AppearanceContextV1,
    },
    /// Итоговый вклад целевого `Occurrence` в конкретный терминальный корень.
    PointPresentation {
        root: PresentationRootIdV1,
        occurrence: OccurrenceIdV1,
        terminal: OccurrenceIdV1,
    },
}

const fn project_constraint_subject(subject: ProgramConstraintSubjectV1) -> ConstraintSubjectV1 {
    match subject {
        ProgramConstraintSubjectV1::VisibleUnary {
            occurrence,
            context,
        } => ConstraintSubjectV1::VisibleUnary {
            occurrence: OccurrenceIdV1::from_core(occurrence),
            context: AppearanceContextV1::from_core(context),
        },
        ProgramConstraintSubjectV1::IntrinsicUnary { target } => {
            ConstraintSubjectV1::IntrinsicUnary {
                target: TargetIdV1::from_core(target),
            }
        }
        ProgramConstraintSubjectV1::IntrinsicRelation { reference } => {
            ConstraintSubjectV1::IntrinsicRelation {
                reference: TargetIdV1::from_core(reference),
            }
        }
        ProgramConstraintSubjectV1::VisibleRelation { reference, context } => {
            ConstraintSubjectV1::VisibleRelation {
                reference: OccurrenceIdV1::from_core(reference),
                context: AppearanceContextV1::from_core(context),
            }
        }
        ProgramConstraintSubjectV1::PointPresentation { target, terminal } => {
            ConstraintSubjectV1::PointPresentation {
                root: PresentationRootIdV1::from_core(target.root()),
                occurrence: OccurrenceIdV1::from_core(target.occurrence()),
                terminal: OccurrenceIdV1::from_core(terminal),
            }
        }
    }
}

/// Одна клетка `case × constraint` выбранного или fixed состояния.
#[derive(Clone, Copy)]
pub(crate) struct VerifiedCellV1<'a> {
    report: &'a CoreProgramReportV1,
    inner: &'a CoreProgramConstraintCellV1,
}

impl<'a> VerifiedCellV1<'a> {
    const fn from_core(
        report: &'a CoreProgramReportV1,
        inner: &'a CoreProgramConstraintCellV1,
    ) -> Self {
        Self { report, inner }
    }

    /// Возвращает индекс физического case.
    pub(crate) const fn case_index(self) -> usize {
        self.inner.case_index()
    }

    /// Возвращает ID ограничения.
    pub(crate) const fn constraint(self) -> ConstraintIdV1 {
        ConstraintIdV1::from_core(self.inner.constraint())
    }

    /// Возвращает полный физический объект ограничения.
    pub(crate) const fn subject(self) -> ConstraintSubjectV1 {
        project_constraint_subject(self.inner.subject())
    }

    /// Возвращает роль ограничения в выборе.
    pub(crate) const fn mode(self) -> ConstraintModeV1 {
        project_constraint_mode(self.inner)
    }

    /// Возвращает типизированное сохранённое evidence.
    pub(crate) fn assessment(self) -> AssessmentV1<'a> {
        project_assessment(self.report, self.inner)
    }
}

/// Одна исчерпывающая клетка `state × case × constraint` конфликта.
#[derive(Clone, Copy)]
pub(crate) struct ConflictCellV1<'a> {
    report: &'a CoreProgramReportV1,
    inner: &'a CoreProgramConstraintCellV1,
}

impl<'a> ConflictCellV1<'a> {
    const fn from_core(
        report: &'a CoreProgramReportV1,
        inner: &'a CoreProgramConstraintCellV1,
    ) -> Self {
        Self { report, inner }
    }

    /// Возвращает индекс рассмотренного состояния.
    pub(crate) const fn state_index(self) -> usize {
        self.inner.candidate_state_index()
    }

    /// Возвращает индекс физического case.
    pub(crate) const fn case_index(self) -> usize {
        self.inner.case_index()
    }

    /// Возвращает ID ограничения.
    pub(crate) const fn constraint(self) -> ConstraintIdV1 {
        ConstraintIdV1::from_core(self.inner.constraint())
    }

    /// Возвращает полный физический объект ограничения.
    pub(crate) const fn subject(self) -> ConstraintSubjectV1 {
        project_constraint_subject(self.inner.subject())
    }

    /// Возвращает роль ограничения в выборе.
    pub(crate) const fn mode(self) -> ConstraintModeV1 {
        project_constraint_mode(self.inner)
    }

    /// Возвращает типизированное сохранённое evidence.
    pub(crate) fn assessment(self) -> AssessmentV1<'a> {
        project_assessment(self.report, self.inner)
    }
}

const fn project_constraint_mode(cell: &CoreProgramConstraintCellV1) -> ConstraintModeV1 {
    if cell.is_hard() {
        ConstraintModeV1::Hard
    } else {
        ConstraintModeV1::ReportOnly
    }
}

fn project_assessment<'a>(
    report: &'a CoreProgramReportV1,
    cell: &'a CoreProgramConstraintCellV1,
) -> AssessmentV1<'a> {
    match cell.result() {
        ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::VisibleUnary(
            CoreProgramPassEvidenceV1::ExactSrgb8(evidence),
        )) => AssessmentV1::ExactSrgb8(ExactSrgb8EvidenceV1 {
            inner: ExactSrgb8EvidenceRefV1::Pass(evidence),
        }),
        ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::VisibleUnary(
                CoreProgramViolationEvidenceV1::ExactSrgb8(evidence),
            ),
        ) => AssessmentV1::ExactSrgb8(ExactSrgb8EvidenceV1 {
            inner: ExactSrgb8EvidenceRefV1::Violation(evidence),
        }),
        ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::VisibleUnary(
            CoreProgramPassEvidenceV1::Wcag22Srgb8(evidence),
        )) => AssessmentV1::Wcag22Srgb8(Wcag22Srgb8EvidenceV1 {
            inner: Wcag22Srgb8EvidenceRefV1::Pass(evidence),
        }),
        ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::VisibleUnary(
                CoreProgramViolationEvidenceV1::Wcag22Srgb8(evidence),
            ),
        ) => AssessmentV1::Wcag22Srgb8(Wcag22Srgb8EvidenceV1 {
            inner: Wcag22Srgb8EvidenceRefV1::Violation(evidence),
        }),
        ProgramConstraintResultV1::Pass(
            ProgramConstraintPassEvidenceV1::DeclaredSrgb8CleanSet(evidence),
        ) => AssessmentV1::DeclaredSrgb8CleanSet(DeclaredSrgb8CleanSetEvidenceV1 {
            inner: DeclaredSrgb8CleanSetEvidenceRefV1::Pass(evidence),
        }),
        ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::DeclaredSrgb8CleanSet(evidence),
        ) => AssessmentV1::DeclaredSrgb8CleanSet(DeclaredSrgb8CleanSetEvidenceV1 {
            inner: DeclaredSrgb8CleanSetEvidenceRefV1::Violation(evidence),
        }),
        ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::IntrinsicUnary(
            evidence,
        )) => AssessmentV1::IntrinsicUnary(IntrinsicUnaryEvidenceV1 {
            inner: IntrinsicUnaryEvidenceRefV1::Pass(evidence),
        }),
        ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::IntrinsicUnary(evidence),
        ) => AssessmentV1::IntrinsicUnary(IntrinsicUnaryEvidenceV1 {
            inner: IntrinsicUnaryEvidenceRefV1::Violation(evidence),
        }),
        ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::Relation(span)) => {
            AssessmentV1::Relation(RelationEvidenceV1 {
                verdict: VerdictV1::Pass,
                members: report
                    .relation_members_for(*span)
                    .unwrap_or_else(|| unreachable!("report owns every relation span")),
            })
        }
        ProgramConstraintResultV1::Violation(ProgramConstraintViolationEvidenceV1::Relation(
            span,
        )) => AssessmentV1::Relation(RelationEvidenceV1 {
            verdict: VerdictV1::Violation,
            members: report
                .relation_members_for(*span)
                .unwrap_or_else(|| unreachable!("report owns every relation span")),
        }),
    }
}

/// Закрытое семейство сохранённого evaluator evidence.
#[derive(Clone, Copy)]
pub(crate) enum AssessmentV1<'a> {
    /// Evidence точного сравнения encoded sRGB8.
    ExactSrgb8(ExactSrgb8EvidenceV1<'a>),
    /// Evidence применимого критерия WCAG 2.2.
    Wcag22Srgb8(Wcag22Srgb8EvidenceV1<'a>),
    IntrinsicUnary(IntrinsicUnaryEvidenceV1<'a>),
    Relation(RelationEvidenceV1<'a>),
    /// Свидетельство закреплённого пакетом clean-set над финальным результатом
    /// представления.
    DeclaredSrgb8CleanSet(DeclaredSrgb8CleanSetEvidenceV1<'a>),
}

impl AssessmentV1<'_> {
    /// Возвращает несовместимый с противоположным исход классификатора.
    pub(crate) const fn verdict(self) -> VerdictV1 {
        match self {
            Self::ExactSrgb8(value) => value.verdict(),
            Self::Wcag22Srgb8(value) => value.verdict(),
            Self::IntrinsicUnary(value) => value.verdict(),
            Self::Relation(value) => value.verdict(),
            Self::DeclaredSrgb8CleanSet(value) => value.verdict(),
        }
    }
}

/// Полное заимствованное evidence одного intrinsic-unary ограничения.
#[derive(Clone, Copy)]
enum IntrinsicUnaryEvidenceRefV1<'a> {
    Pass(&'a ProgramIntrinsicUnaryPassEvidenceV1),
    Violation(&'a ProgramIntrinsicUnaryViolationEvidenceV1),
}

#[derive(Clone, Copy)]
pub(crate) struct IntrinsicUnaryEvidenceV1<'a> {
    inner: IntrinsicUnaryEvidenceRefV1<'a>,
}

impl IntrinsicUnaryEvidenceV1<'_> {
    pub(crate) const fn verdict(self) -> VerdictV1 {
        match self.inner {
            IntrinsicUnaryEvidenceRefV1::Pass(_) => VerdictV1::Pass,
            IntrinsicUnaryEvidenceRefV1::Violation(_) => VerdictV1::Violation,
        }
    }

    pub(crate) const fn binding(self) -> IntrinsicPaintBindingV1 {
        let binding = match self.inner {
            IntrinsicUnaryEvidenceRefV1::Pass(value) => value.binding(),
            IntrinsicUnaryEvidenceRefV1::Violation(value) => value.binding(),
        };
        IntrinsicPaintBindingV1::from_core(binding)
    }

    pub(crate) const fn measurement(self) -> IntrinsicUnaryMeasurementV1 {
        let measurement = match self.inner {
            IntrinsicUnaryEvidenceRefV1::Pass(value) => value.measurement(),
            IntrinsicUnaryEvidenceRefV1::Violation(value) => value.measurement(),
        };
        match measurement {
            CoreIntrinsicUnaryMeasurementV1::ExactSrgb8(value) => {
                IntrinsicUnaryMeasurementV1::ExactSrgb8(ExactSrgb8UnaryMeasurementV1 {
                    expected: value.expected(),
                    actual: value.actual(),
                })
            }
            CoreIntrinsicUnaryMeasurementV1::FamilyMembership {
                family,
                measurement,
            } => IntrinsicUnaryMeasurementV1::FamilyMembership(
                FamilyMembershipMeasurementV2::from_core(family, measurement),
            ),
        }
    }

    pub(crate) const fn proof(self) -> IntrinsicUnaryProofV1 {
        match self.inner {
            IntrinsicUnaryEvidenceRefV1::Pass(value) => match value.proof() {
                CoreIntrinsicUnaryPassV1::ExactSrgb8(_) => IntrinsicUnaryProofV1::ExactSrgb8Pass,
                CoreIntrinsicUnaryPassV1::FamilyMembership(_) => {
                    IntrinsicUnaryProofV1::FamilyMembershipPass
                }
            },
            IntrinsicUnaryEvidenceRefV1::Violation(value) => match value.proof() {
                CoreIntrinsicUnaryViolationV1::ExactSrgb8(_) => {
                    IntrinsicUnaryProofV1::ExactSrgb8Violation
                }
                CoreIntrinsicUnaryViolationV1::FamilyMembership(_) => {
                    IntrinsicUnaryProofV1::FamilyMembershipViolation
                }
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IntrinsicPaintBindingV1 {
    target: TargetIdV1,
    value: PaintValueV1,
}

impl IntrinsicPaintBindingV1 {
    const fn from_core(value: ProgramIntrinsicPaintBindingV1) -> Self {
        Self {
            target: TargetIdV1::from_core(value.target()),
            value: PaintValueV1::from_core(value.value()),
        }
    }

    pub(crate) const fn target(self) -> TargetIdV1 {
        self.target
    }

    pub(crate) const fn value(self) -> PaintValueV1 {
        self.value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactSrgb8UnaryMeasurementV1 {
    expected: Srgb8,
    actual: Srgb8,
}

impl ExactSrgb8UnaryMeasurementV1 {
    pub(crate) const fn expected(self) -> Srgb8 {
        self.expected
    }

    pub(crate) const fn actual(self) -> Srgb8 {
        self.actual
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntrinsicUnaryMeasurementV1 {
    /// Измерение точного равенства.
    ExactSrgb8(ExactSrgb8UnaryMeasurementV1),
    /// Измерение принадлежности точному образу family.
    FamilyMembership(FamilyMembershipMeasurementV2),
}

/// Точный сигнал и semantic family release, проверенные одним вызовом.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyMembershipMeasurementV2 {
    family: FamilyIdV1,
    semantic: FamilySemanticReleaseV2,
    signal: Srgb8,
}

impl FamilyMembershipMeasurementV2 {
    const fn from_core(family: FamilyId, value: CoreFamilyMembershipMeasurementV2) -> Self {
        Self {
            family: FamilyIdV1::from_core(family),
            semantic: FamilySemanticReleaseV2::from_core(value.semantic()),
            signal: value.signal().srgb8(),
        }
    }

    /// Возвращает opaque-объявление family, связанное ограничением.
    pub(crate) const fn family(self) -> FamilyIdV1 {
        self.family
    }

    /// Возвращает representation-independent semantic release family.
    pub(crate) const fn semantic(self) -> FamilySemanticReleaseV2 {
        self.semantic
    }

    /// Возвращает классифицированный точный исходный сигнал.
    pub(crate) const fn signal(self) -> Srgb8 {
        self.signal
    }
}

/// Взаимоисключающие точные доказательства одного intrinsic-unary вызова.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntrinsicUnaryProofV1 {
    /// Точное равенство выполнено.
    ExactSrgb8Pass,
    /// Точное равенство нарушено.
    ExactSrgb8Violation,
    /// Принадлежность точному образу family подтверждена.
    FamilyMembershipPass,
    /// Принадлежность точному образу family нарушена.
    FamilyMembershipViolation,
}

/// Полное заимствованное member-evidence одного directional-ограничения.
#[derive(Clone, Copy)]
pub(crate) struct RelationEvidenceV1<'a> {
    verdict: VerdictV1,
    members: &'a [ProgramRelationMemberEvidenceV1],
}

impl<'a> RelationEvidenceV1<'a> {
    pub(crate) const fn verdict(self) -> VerdictV1 {
        self.verdict
    }

    pub(crate) fn member_count(self) -> usize {
        self.members.len()
    }

    pub(crate) fn members(
        self,
    ) -> impl ExactSizeIterator<Item = RelationMemberV1<'a>> + FusedIterator + 'a {
        self.members.iter().map(RelationMemberV1::from_core)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum RelationMemberV1<'a> {
    Intrinsic(IntrinsicRelationMemberV1<'a>),
    Visible(VisibleRelationMemberV1<'a>),
}

impl<'a> RelationMemberV1<'a> {
    fn from_core(inner: &'a ProgramRelationMemberEvidenceV1) -> Self {
        match inner {
            ProgramRelationMemberEvidenceV1::Intrinsic { .. } => {
                Self::Intrinsic(IntrinsicRelationMemberV1 { inner })
            }
            ProgramRelationMemberEvidenceV1::Visible { .. } => {
                Self::Visible(VisibleRelationMemberV1 { inner })
            }
        }
    }

    pub(crate) const fn measurement(self) -> RelationMeasurementV1 {
        let measurement = match self {
            Self::Intrinsic(value) => value.inner.measurement(),
            Self::Visible(value) => value.inner.measurement(),
        };
        project_relation_measurement(measurement)
    }

    pub(crate) const fn verdict(self) -> VerdictV1 {
        match self {
            Self::Intrinsic(value) => value.verdict(),
            Self::Visible(value) => value.verdict(),
        }
    }

    pub(crate) const fn proof(self) -> RelationMemberProofV1 {
        let decision = match self {
            Self::Intrinsic(value) => value.inner.decision(),
            Self::Visible(value) => value.inner.decision(),
        };
        project_relation_proof(decision)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct IntrinsicRelationMemberV1<'a> {
    inner: &'a ProgramRelationMemberEvidenceV1,
}

impl IntrinsicRelationMemberV1<'_> {
    pub(crate) const fn reference(self) -> IntrinsicPaintBindingV1 {
        let Some((reference, _)) = self.inner.intrinsic_bindings() else {
            unreachable!()
        };
        IntrinsicPaintBindingV1::from_core(*reference)
    }

    pub(crate) const fn candidate(self) -> IntrinsicPaintBindingV1 {
        let Some((_, candidate)) = self.inner.intrinsic_bindings() else {
            unreachable!()
        };
        IntrinsicPaintBindingV1::from_core(*candidate)
    }

    pub(crate) const fn verdict(self) -> VerdictV1 {
        project_relation_verdict(self.inner.decision())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct VisibleRelationMemberV1<'a> {
    inner: &'a ProgramRelationMemberEvidenceV1,
}

impl<'a> VisibleRelationMemberV1<'a> {
    pub(crate) const fn reference(self) -> VisibleRelationEndpointV1<'a> {
        let Some((reference, _)) = self.inner.visible_bindings() else {
            unreachable!()
        };
        VisibleRelationEndpointV1 { inner: reference }
    }

    pub(crate) const fn candidate(self) -> VisibleRelationEndpointV1<'a> {
        let Some((_, candidate)) = self.inner.visible_bindings() else {
            unreachable!()
        };
        VisibleRelationEndpointV1 { inner: candidate }
    }

    pub(crate) const fn verdict(self) -> VerdictV1 {
        project_relation_verdict(self.inner.decision())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct VisibleRelationEndpointV1<'a> {
    inner: &'a ProgramVisibleRelationBindingV1,
}

impl<'a> VisibleRelationEndpointV1<'a> {
    pub(crate) const fn occurrence(self) -> OccurrenceIdV1 {
        OccurrenceIdV1::from_core(self.inner.occurrence())
    }

    pub(crate) const fn binding(self) -> PointBindingV1<'a> {
        PointBindingV1 {
            inner: self.inner.physical_ref(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactSrgb8RelationMeasurementV1 {
    reference: Srgb8,
    candidate: Srgb8,
}

impl ExactSrgb8RelationMeasurementV1 {
    pub(crate) const fn reference(self) -> Srgb8 {
        self.reference
    }

    pub(crate) const fn candidate(self) -> Srgb8 {
        self.candidate
    }
}

/// Положительные категориальные измерения обоих endpoints одного member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyCategoryRelationMeasurementV1 {
    reference: FamilyMembershipMeasurementV2,
    candidate: FamilyMembershipMeasurementV2,
}

impl FamilyCategoryRelationMeasurementV1 {
    const fn from_core(family: FamilyId, value: CoreFamilyCategoryRelationMeasurementV1) -> Self {
        Self {
            reference: FamilyMembershipMeasurementV2::from_core(family, value.reference()),
            candidate: FamilyMembershipMeasurementV2::from_core(family, value.candidate()),
        }
    }

    pub(crate) const fn reference(self) -> FamilyMembershipMeasurementV2 {
        self.reference
    }

    pub(crate) const fn candidate(self) -> FamilyMembershipMeasurementV2 {
        self.candidate
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelationMeasurementV1 {
    ExactSrgb8(ExactSrgb8RelationMeasurementV1),
    ExactSrgb8Distinction(ExactSrgb8RelationMeasurementV1),
    FamilyCategory(FamilyCategoryRelationMeasurementV1),
}

/// Endpoint, оставшийся без положительного категориального свидетельства.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyCategoryViolationKindV1 {
    ReferenceEndpoint,
    CandidateEndpoint,
    BothEndpoints,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelationMemberProofV1 {
    ExactSrgb8Pass,
    ExactSrgb8Violation,
    ExactSrgb8DistinctionPass,
    ExactSrgb8DistinctionViolation,
    FamilyCategoryPass,
    FamilyCategoryViolation(FamilyCategoryViolationKindV1),
}

const fn project_relation_measurement(value: CoreRelationMeasurementV1) -> RelationMeasurementV1 {
    match value {
        CoreRelationMeasurementV1::ExactSrgb8(value) => {
            RelationMeasurementV1::ExactSrgb8(ExactSrgb8RelationMeasurementV1 {
                reference: value.reference(),
                candidate: value.candidate(),
            })
        }
        CoreRelationMeasurementV1::ExactSrgb8Distinction(value) => {
            RelationMeasurementV1::ExactSrgb8Distinction(ExactSrgb8RelationMeasurementV1 {
                reference: value.reference(),
                candidate: value.candidate(),
            })
        }
        CoreRelationMeasurementV1::FamilyCategory {
            family,
            measurement,
        } => RelationMeasurementV1::FamilyCategory(FamilyCategoryRelationMeasurementV1::from_core(
            family,
            measurement,
        )),
    }
}

const fn project_relation_verdict(value: ProgramRelationMemberDecisionV1) -> VerdictV1 {
    match value {
        ProgramRelationMemberDecisionV1::Pass(_) => VerdictV1::Pass,
        ProgramRelationMemberDecisionV1::Violation(_) => VerdictV1::Violation,
    }
}

const fn project_relation_proof(value: ProgramRelationMemberDecisionV1) -> RelationMemberProofV1 {
    match value {
        ProgramRelationMemberDecisionV1::Pass(CoreRelationPassV1::ExactSrgb8(_)) => {
            RelationMemberProofV1::ExactSrgb8Pass
        }
        ProgramRelationMemberDecisionV1::Violation(CoreRelationViolationV1::ExactSrgb8(_)) => {
            RelationMemberProofV1::ExactSrgb8Violation
        }
        ProgramRelationMemberDecisionV1::Pass(CoreRelationPassV1::ExactSrgb8Distinction(_)) => {
            RelationMemberProofV1::ExactSrgb8DistinctionPass
        }
        ProgramRelationMemberDecisionV1::Violation(
            CoreRelationViolationV1::ExactSrgb8Distinction(_),
        ) => RelationMemberProofV1::ExactSrgb8DistinctionViolation,
        ProgramRelationMemberDecisionV1::Pass(CoreRelationPassV1::FamilyCategory(_)) => {
            RelationMemberProofV1::FamilyCategoryPass
        }
        ProgramRelationMemberDecisionV1::Violation(CoreRelationViolationV1::FamilyCategory(
            violation,
        )) => RelationMemberProofV1::FamilyCategoryViolation(match violation {
            CoreFamilyCategoryRelationViolationV1::ReferenceEndpoint => {
                FamilyCategoryViolationKindV1::ReferenceEndpoint
            }
            CoreFamilyCategoryRelationViolationV1::CandidateEndpoint => {
                FamilyCategoryViolationKindV1::CandidateEndpoint
            }
            CoreFamilyCategoryRelationViolationV1::BothEndpoints => {
                FamilyCategoryViolationKindV1::BothEndpoints
            }
        }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeclaredSrgb8CleanSetViolationKindV1 {
    FinalOwnedDomainAbsent,
    Rejected,
}

#[derive(Clone, Copy)]
enum DeclaredSrgb8CleanSetEvidenceRefV1<'a> {
    Pass(&'a CoreDeclaredSrgb8CleanSetPassV1),
    Violation(&'a CoreDeclaredSrgb8CleanSetViolationV1),
}

/// Заимствованное свидетельство одной абсолютной проверки финального домена sRGB8.
#[derive(Clone, Copy)]
pub(crate) struct DeclaredSrgb8CleanSetEvidenceV1<'a> {
    inner: DeclaredSrgb8CleanSetEvidenceRefV1<'a>,
}

impl DeclaredSrgb8CleanSetEvidenceV1<'_> {
    pub(crate) const fn verdict(self) -> VerdictV1 {
        match self.inner {
            DeclaredSrgb8CleanSetEvidenceRefV1::Pass(_) => VerdictV1::Pass,
            DeclaredSrgb8CleanSetEvidenceRefV1::Violation(_) => VerdictV1::Violation,
        }
    }

    pub(crate) const fn violation(self) -> Option<DeclaredSrgb8CleanSetViolationKindV1> {
        match self.inner {
            DeclaredSrgb8CleanSetEvidenceRefV1::Pass(_) => None,
            DeclaredSrgb8CleanSetEvidenceRefV1::Violation(
                CoreDeclaredSrgb8CleanSetViolationV1::FinalOwnedDomainAbsent,
            ) => Some(DeclaredSrgb8CleanSetViolationKindV1::FinalOwnedDomainAbsent),
            DeclaredSrgb8CleanSetEvidenceRefV1::Violation(
                CoreDeclaredSrgb8CleanSetViolationV1::Rejected { .. },
            ) => Some(DeclaredSrgb8CleanSetViolationKindV1::Rejected),
        }
    }

    pub(crate) const fn visible(self) -> Option<Srgb8> {
        match self.inner {
            DeclaredSrgb8CleanSetEvidenceRefV1::Pass(evidence) => Some(evidence.visible()),
            DeclaredSrgb8CleanSetEvidenceRefV1::Violation(evidence) => evidence.visible(),
        }
    }

    pub(crate) const fn rejected_blue_interval(self) -> Option<[u8; 2]> {
        match self.inner {
            DeclaredSrgb8CleanSetEvidenceRefV1::Pass(_) => None,
            DeclaredSrgb8CleanSetEvidenceRefV1::Violation(evidence) => {
                match evidence.rejected_blue_interval() {
                    Some(interval) => Some(interval.endpoints()),
                    None => None,
                }
            }
        }
    }
}

/// Несовместимые сохранённые исходы классификатора.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerdictV1 {
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

/// Evidence точного sRGB8 сравнения с физической occurrence и объявленным context.
#[derive(Clone, Copy)]
pub(crate) struct ExactSrgb8EvidenceV1<'a> {
    inner: ExactSrgb8EvidenceRefV1<'a>,
}

impl<'a> ExactSrgb8EvidenceV1<'a> {
    /// Возвращает сохранённый исход классификатора.
    pub(crate) const fn verdict(self) -> VerdictV1 {
        match self.inner {
            ExactSrgb8EvidenceRefV1::Pass(_) => VerdictV1::Pass,
            ExactSrgb8EvidenceRefV1::Violation(_) => VerdictV1::Violation,
        }
    }

    /// Возвращает ожидаемый encoded sRGB8 результат.
    pub(crate) fn expected(self) -> Srgb8 {
        match self.inner {
            ExactSrgb8EvidenceRefV1::Pass(value) => value.target(),
            ExactSrgb8EvidenceRefV1::Violation(value) => value.target(),
        }
    }

    /// Возвращает физическую occurrence-привязку и объявленный appearance context.
    pub(crate) fn binding(self) -> PointBindingV1<'a> {
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

/// WCAG 2.2 evidence с физической occurrence и объявленным appearance context.
#[derive(Clone, Copy)]
pub(crate) struct Wcag22Srgb8EvidenceV1<'a> {
    inner: Wcag22Srgb8EvidenceRefV1<'a>,
}

impl<'a> Wcag22Srgb8EvidenceV1<'a> {
    /// Возвращает сохранённый исход классификатора.
    pub(crate) const fn verdict(self) -> VerdictV1 {
        match self.inner {
            Wcag22Srgb8EvidenceRefV1::Pass(_) => VerdictV1::Pass,
            Wcag22Srgb8EvidenceRefV1::Violation(_) => VerdictV1::Violation,
        }
    }

    /// Возвращает версию применённого WCAG 2.2 профиля.
    pub(crate) fn profile_id(self) -> Wcag22ProfileIdV1 {
        match self.inner {
            Wcag22Srgb8EvidenceRefV1::Pass(value) => value.measurement().value().profile_id(),
            Wcag22Srgb8EvidenceRefV1::Violation(value) => value.measurement().value().profile_id(),
        }
    }

    /// Возвращает применённый критерий.
    pub(crate) fn criterion(self) -> Wcag22CriterionV1 {
        match self.inner {
            Wcag22Srgb8EvidenceRefV1::Pass(value) => value.measurement().value().criterion(),
            Wcag22Srgb8EvidenceRefV1::Violation(value) => value.measurement().value().criterion(),
        }
    }

    /// Возвращает сертифицированные границы яркости foreground.
    pub(crate) fn foreground_luminance(self) -> Wcag22LuminanceBoundsQ55V1 {
        let measurement = match self.inner {
            Wcag22Srgb8EvidenceRefV1::Pass(value) => value.measurement().value().measurement(),
            Wcag22Srgb8EvidenceRefV1::Violation(value) => value.measurement().value().measurement(),
        };
        measurement.foreground_luminance
    }

    /// Возвращает сертифицированные границы яркости background.
    pub(crate) fn background_luminance(self) -> Wcag22LuminanceBoundsQ55V1 {
        let measurement = match self.inner {
            Wcag22Srgb8EvidenceRefV1::Pass(value) => value.measurement().value().measurement(),
            Wcag22Srgb8EvidenceRefV1::Violation(value) => value.measurement().value().measurement(),
        };
        measurement.background_luminance
    }

    /// Возвращает числовое доказательство устойчивости решения.
    pub(crate) fn numerical_evidence(self) -> &'a NumericalDecisionEvidenceV1 {
        match self.inner {
            Wcag22Srgb8EvidenceRefV1::Pass(value) => value.measurement().value().evidence(),
            Wcag22Srgb8EvidenceRefV1::Violation(value) => value.measurement().value().evidence(),
        }
    }

    /// Возвращает физическую occurrence-привязку и объявленный appearance context.
    pub(crate) fn binding(self) -> PointBindingV1<'a> {
        let value = match self.inner {
            Wcag22Srgb8EvidenceRefV1::Pass(value) => value.binding(),
            Wcag22Srgb8EvidenceRefV1::Violation(value) => value.binding(),
        };
        PointBindingV1 { inner: value }
    }
}

/// Общая привязка физической композиции и объявленного appearance context.
#[derive(Clone, Copy)]
pub(crate) struct PointBindingV1<'a> {
    inner: &'a ProgramVisiblePointBindingV1,
}

impl<'a> PointBindingV1<'a> {
    /// Возвращает закрытый тип точной физической композиции.
    pub(crate) const fn physical(self) -> PhysicalPointV1<'a> {
        match self.inner.physical().occurrence().profile() {
            CompositionProfileV1::EncodedSrgb8SourceOverV1 => {
                PhysicalPointV1::EncodedSrgb8SourceOver(EncodedSrgb8SourceOverV1 {
                    inner: self.inner,
                })
            }
        }
    }

    /// Возвращает точный объявленный контекст без неявного построения LCS-view.
    pub(crate) const fn appearance_context(self) -> AppearanceContextV1 {
        AppearanceContextV1(self.inner.context())
    }
}

/// Закрытое семейство точной физической композиции.
#[derive(Clone, Copy)]
pub(crate) enum PhysicalPointV1<'a> {
    /// Encoded-sRGB8 source-over композиция.
    EncodedSrgb8SourceOver(EncodedSrgb8SourceOverV1<'a>),
}

/// Точная привязка одного encoded-sRGB8 source-over Occurrence.
#[derive(Clone, Copy)]
pub(crate) struct EncodedSrgb8SourceOverV1<'a> {
    inner: &'a ProgramVisiblePointBindingV1,
}

impl EncodedSrgb8SourceOverV1<'_> {
    /// Возвращает ID накладываемого Paint.
    pub(crate) const fn subject_paint(self) -> PaintIdV1 {
        PaintIdV1::from_core(self.inner.physical().program_occurrence().subject())
    }

    /// Возвращает ID backdrop Surface.
    pub(crate) const fn backdrop_surface(self) -> SurfaceIdV1 {
        SurfaceIdV1::from_core(
            self.inner
                .physical()
                .program_occurrence()
                .backdrop_surface(),
        )
    }

    /// Возвращает исходный encoded sRGB8 subject до композиции.
    pub(crate) const fn subject(self) -> Srgb8 {
        Srgb8::new(self.inner.physical().occurrence().subject_rgb())
    }

    /// Возвращает точную прозрачность subject в `[0, 1]`.
    pub(crate) const fn opacity(self) -> f64 {
        f64::from_bits(self.inner.physical().occurrence().subject_opacity_bits())
    }

    /// Возвращает observed encoded sRGB8 backdrop.
    pub(crate) const fn backdrop(self) -> Srgb8 {
        Srgb8::new(self.inner.physical().occurrence().backdrop_rgb())
    }

    /// Возвращает видимый encoded sRGB8 результат композиции.
    pub(crate) const fn visible(self) -> Srgb8 {
        Srgb8::new(self.inner.physical().occurrence().output_rgb())
    }
}

/// Один Core-сертифицированный encoded Paint, направленный в клиентский slot.
///
/// Это ещё не результат sink, attachment, renderer или final-visible
/// композиции.
#[derive(Clone, Copy)]
pub(crate) struct CertifiedPaintOutputV1<'a> {
    inner: &'a ProgramPaintOutputV1,
}

impl<'a> CertifiedPaintOutputV1<'a> {
    const fn from_core(inner: &'a ProgramPaintOutputV1) -> Self {
        Self { inner }
    }

    /// Возвращает клиентский выходной слот.
    pub(crate) const fn output_slot(self) -> OutputSlotIdV1 {
        OutputSlotIdV1::from_core((*self.inner).output())
    }

    /// Возвращает ID сертифицированного Paint.
    pub(crate) const fn paint(self) -> PaintIdV1 {
        PaintIdV1::from_core((*self.inner).paint().id())
    }

    /// Возвращает исходный encoded sRGB8 сигнал Paint.
    pub(crate) const fn source(self) -> Srgb8 {
        (*self.inner).paint().source()
    }

    /// Возвращает сертифицированную прозрачность Paint.
    pub(crate) const fn opacity(self) -> f64 {
        (*self.inner).paint().opacity().value()
    }
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

/// Точное несовпадение semantic artifact pool с требованиями Program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyArtifactErrorV2 {
    Missing { semantic: FamilySemanticReleaseV2 },
    Extra { semantic: FamilySemanticReleaseV2 },
    Duplicate { semantic: FamilySemanticReleaseV2 },
}

impl FamilyArtifactErrorV2 {
    const fn from_core(error: FamilyArtifactContractErrorV2) -> Self {
        match error {
            FamilyArtifactContractErrorV2::Missing { semantic } => Self::Missing {
                semantic: FamilySemanticReleaseV2::from_core(semantic),
            },
            FamilyArtifactContractErrorV2::Extra { semantic } => Self::Extra {
                semantic: FamilySemanticReleaseV2::from_core(semantic),
            },
            FamilyArtifactContractErrorV2::Duplicate { semantic } => Self::Duplicate {
                semantic: FamilySemanticReleaseV2::from_core(semantic),
            },
        }
    }
}

/// Ошибочные состояния создания Session представлены закрытой суммой.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstantiateErrorV1 {
    ResourceExhausted,
    InternalInvariant,
    FamilyArtifacts(FamilyArtifactErrorV2),
}

impl InstantiateErrorV1 {
    fn from_core(error: ProgramSessionInstantiateError) -> Self {
        match error {
            ProgramSessionInstantiateError::ResourceExhausted => Self::ResourceExhausted,
            ProgramSessionInstantiateError::InternalInvariant => Self::InternalInvariant,
            ProgramSessionInstantiateError::FamilyArtifacts(cause) => {
                Self::FamilyArtifacts(FamilyArtifactErrorV2::from_core(cause))
            }
        }
    }
}

/// Закрытая классификация ошибки одного атомарного update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateErrorKindV1 {
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
pub(crate) enum UpdatePhaseV1 {
    /// Admission и канонизация физического наблюдения.
    ObservationAdmission,
    /// Вычисление, поиск и финальная перепроверка Program.
    ProgramEvaluation,
}

/// Точный отказ зарегистрированного evaluator-а.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EvaluatorFailureV1 {
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
pub(crate) enum ObservationBindingFailureV1 {
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

/// Точное недопустимое protocol-состояние зарегистрированного evaluator-а.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::enum_variant_names,
    reason = "the variant name preserves evaluator provenance as this closed family grows"
)]
pub(crate) enum EvaluatorProtocolFailureV1 {
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
pub(crate) enum UpdateInvariantV1 {
    /// Заимствованный matching Owner не удержал свою эпоху живой.
    OwnerAuthority,
    /// Закрытое хранилище наблюдений нарушило собственный arena-контракт.
    ObservationStorage,
    /// Каноническая observation schema разошлась со скомпилированным binding.
    ObservationBinding,
    /// Сохранённое evidence не принадлежит допускаемому observation.
    EvidenceBinding,
    /// Applicable evaluator вернул недопустимое protocol-состояние.
    EvaluatorProtocol,
    /// Один выбранный state дал разные выходы в физических сценариях.
    OutputCaseInvariance,
    /// Детерминированная финальная перепроверка разошлась с поиском.
    SelectionRecheck,
    /// Закрытая программа нарушила собственную структуру исполнения.
    ProgramEvaluation,
}

/// Нарушенный внутренний контракт с точными subject и witness-фактами.
///
/// Эти варианты недостижимы через типизированный boundary input. Payload нужен
/// для детерминированной диагностики и не превращает breach в цветовой verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UpdateInvariantFailureV1 {
    /// Заимствованный matching Owner не удержал свою эпоху живой.
    OwnerAuthority,
    /// Закрытое хранилище наблюдений нарушило собственный arena-контракт.
    ObservationStorage,
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
        /// Полный физический объект финальной перепроверки.
        subject: ConstraintSubjectV1,
        /// Число hard-нарушений на финальной перепроверке.
        hard_violation_count: usize,
    },
    /// Закрытая программа нарушила собственную структуру исполнения.
    ProgramEvaluation,
}

impl UpdateInvariantFailureV1 {
    /// Возвращает стабильную identity нарушенного контракта.
    pub(crate) const fn contract(&self) -> UpdateInvariantV1 {
        match self {
            Self::OwnerAuthority => UpdateInvariantV1::OwnerAuthority,
            Self::ObservationStorage => UpdateInvariantV1::ObservationStorage,
            Self::ObservationBinding { .. } => UpdateInvariantV1::ObservationBinding,
            Self::EvidenceBinding => UpdateInvariantV1::EvidenceBinding,
            Self::EvaluatorProtocol { .. } => UpdateInvariantV1::EvaluatorProtocol,
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
pub(crate) enum UpdateErrorV1 {
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
    pub(crate) const fn kind(&self) -> UpdateErrorKindV1 {
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
        ProgramCompileError::DuplicateFamily { family } => CompileErrorV1::DuplicateFamily {
            family: FamilyIdV1::from_core(family),
        },
        ProgramCompileError::UnusedFamily { family } => CompileErrorV1::UnusedFamily {
            family: FamilyIdV1::from_core(family),
        },
        ProgramCompileError::MissingFixedSource { target, source } => {
            CompileErrorV1::MissingFixedSource {
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
        ProgramCompileError::DuplicatePresentationRoot { root } => {
            CompileErrorV1::DuplicatePresentationRoot {
                root: PresentationRootIdV1::from_core(root),
            }
        }
        ProgramCompileError::MissingPresentationRootOccurrence { root, occurrence } => {
            CompileErrorV1::MissingPresentationRootOccurrence {
                root: PresentationRootIdV1::from_core(root),
                occurrence: OccurrenceIdV1::from_core(occurrence),
            }
        }
        ProgramCompileError::PresentationRootConsumedDownstream { root, occurrence } => {
            CompileErrorV1::PresentationRootConsumedDownstream {
                root: PresentationRootIdV1::from_core(root),
                occurrence: OccurrenceIdV1::from_core(occurrence),
            }
        }
        ProgramCompileError::UnusedPresentationRoot { root } => {
            CompileErrorV1::UnusedPresentationRoot {
                root: PresentationRootIdV1::from_core(root),
            }
        }
        ProgramCompileError::DuplicatePointPresentationTarget { root, occurrence } => {
            CompileErrorV1::DuplicatePointPresentationTarget {
                root: PresentationRootIdV1::from_core(root),
                occurrence: OccurrenceIdV1::from_core(occurrence),
            }
        }
        ProgramCompileError::MissingPointPresentationRoot { root } => {
            CompileErrorV1::MissingPointPresentationRoot {
                root: PresentationRootIdV1::from_core(root),
            }
        }
        ProgramCompileError::MissingPointPresentationOccurrence { root, occurrence } => {
            CompileErrorV1::MissingPointPresentationOccurrence {
                root: PresentationRootIdV1::from_core(root),
                occurrence: OccurrenceIdV1::from_core(occurrence),
            }
        }
        ProgramCompileError::PointPresentationOccurrenceOutsideRootAncestry {
            root,
            terminal,
            occurrence,
        } => CompileErrorV1::PointPresentationOccurrenceOutsideRootAncestry {
            root: PresentationRootIdV1::from_core(root),
            terminal: OccurrenceIdV1::from_core(terminal),
            occurrence: OccurrenceIdV1::from_core(occurrence),
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
        ProgramCompileError::DuplicateTargetCandidate { target, candidate } => {
            CompileErrorV1::DuplicateTargetCandidate {
                target: TargetIdV1::from_core(target),
                candidate: TargetCandidateIdV1::from_core(candidate),
            }
        }
        ProgramCompileError::DuplicateTargetCandidateValue {
            target,
            first,
            duplicate,
            value,
        } => CompileErrorV1::DuplicateTargetCandidateValue {
            target: TargetIdV1::from_core(target),
            first: TargetCandidateIdV1::from_core(first),
            duplicate: TargetCandidateIdV1::from_core(duplicate),
            value: PaintValueV1(value),
        },
        ProgramCompileError::UnconstrainedFiniteTarget { target } => {
            CompileErrorV1::UnconstrainedFiniteTarget {
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
        ProgramCompileError::MissingIntrinsicUnaryTarget { constraint, target } => {
            CompileErrorV1::MissingIntrinsicUnaryTarget {
                constraint: ConstraintIdV1::from_core(constraint),
                target: TargetIdV1::from_core(target),
            }
        }
        ProgramCompileError::MissingConstraintFamily { constraint, family } => {
            CompileErrorV1::MissingConstraintFamily {
                constraint: ConstraintIdV1::from_core(constraint),
                family: FamilyIdV1::from_core(family),
            }
        }
        ProgramCompileError::MissingIntrinsicRelationReference {
            constraint,
            reference,
        } => CompileErrorV1::MissingIntrinsicRelationReference {
            constraint: ConstraintIdV1::from_core(constraint),
            reference: TargetIdV1::from_core(reference),
        },
        ProgramCompileError::MissingIntrinsicRelationCandidate {
            constraint,
            candidate,
        } => CompileErrorV1::MissingIntrinsicRelationCandidate {
            constraint: ConstraintIdV1::from_core(constraint),
            candidate: TargetIdV1::from_core(candidate),
        },
        ProgramCompileError::MissingVisibleRelationReference {
            constraint,
            reference,
        } => CompileErrorV1::MissingVisibleRelationReference {
            constraint: ConstraintIdV1::from_core(constraint),
            reference: OccurrenceIdV1::from_core(reference),
        },
        ProgramCompileError::MissingVisibleRelationCandidate {
            constraint,
            candidate,
        } => CompileErrorV1::MissingVisibleRelationCandidate {
            constraint: ConstraintIdV1::from_core(constraint),
            candidate: OccurrenceIdV1::from_core(candidate),
        },
        ProgramCompileError::SolverDependentIntrinsicRelationReference {
            constraint,
            reference,
        } => CompileErrorV1::SolverDependentIntrinsicRelationReference {
            constraint: ConstraintIdV1::from_core(constraint),
            reference: TargetIdV1::from_core(reference),
        },
        ProgramCompileError::SolverDependentVisibleRelationReference {
            constraint,
            reference,
            target,
        } => CompileErrorV1::SolverDependentVisibleRelationReference {
            constraint: ConstraintIdV1::from_core(constraint),
            reference: OccurrenceIdV1::from_core(reference),
            target: TargetIdV1::from_core(target),
        },
        ProgramCompileError::MissingConstraintPresentationTarget {
            constraint,
            root,
            occurrence,
        } => CompileErrorV1::MissingConstraintPresentationTarget {
            constraint: ConstraintIdV1::from_core(constraint),
            root: PresentationRootIdV1::from_core(root),
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
        ObservationError::InternalInvariant => UpdateErrorV1::InternalInvariant {
            source: UpdateInvariantFailureV1::ObservationStorage,
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
            subject,
            hard_violation_count,
        } => UpdateErrorV1::InternalInvariant {
            source: UpdateInvariantFailureV1::SelectionRecheck {
                state_index,
                case_index,
                constraint: ConstraintIdV1::from_core(constraint),
                subject: project_constraint_subject(subject),
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
    fn every_unreachable_core_failure_keeps_its_subject_and_witness_facts() {
        use crate::observation::ObservationSchemaMismatchV1;
        let subject_context = context().0;

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
                    UpdateInvariantFailureV1::ObservationStorage => {
                        UpdateInvariantV1::ObservationStorage
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
            map_observation_error(ObservationError::InternalInvariant),
            UpdateInvariantFailureV1::ObservationStorage,
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
                subject: ProgramConstraintSubjectV1::VisibleUnary {
                    occurrence: OccurrenceId::new(4),
                    context: subject_context,
                },
                hard_violation_count: 1,
            }),
            UpdateInvariantFailureV1::SelectionRecheck {
                state_index: 1,
                case_index: 2,
                constraint: ConstraintIdV1::new(3),
                subject: ConstraintSubjectV1::VisibleUnary {
                    occurrence: OccurrenceIdV1::new(4),
                    context: AppearanceContextV1::from_core(subject_context),
                },
                hard_violation_count: 1,
            },
        );
        assert_invariant(
            map_plan_error(ProgramSessionEvaluationError::InternalInvariant),
            UpdateInvariantFailureV1::ProgramEvaluation,
        );
    }
}
