//! Публичная проверка, компиляция и runtime канонических Program wire-байтов.
//!
//! Проверка возвращает content identity без runtime authority. Компиляция создаёт
//! immutable owner для Session или terminal point attachment; физическим sink
//! владеет host. Все пути исполняют тот же закрытый Core.
//!
//! Отказы формата и семантики остаются раздельными. Возвращённая нехватка ресурсов
//! или внутренний сбой сохраняют свой класс при проверке, компиляции и update;
//! они не превращаются в отказ пользовательского графа или observation.

use crate::Srgb8;
use crate::observation::{ScenarioId, SchemaOrderedScenarioSourceV1};
use crate::program::wire::{ProgramWireErrorV1, decode_program_wire, decode_program_wire_v1};

/// Имя wire-секции в публичной диагностике.
///
/// Строковая проекция намеренно: публичный тип не тянет внутренние enum'ы
/// формата, а закрытый словарь имён — контракт версии v1.
pub type ProgramWireSectionNameV1 = &'static str;

/// Публичный typed-отказ проверки wire-байтов.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramWireCheckErrorV1 {
    /// Байты нарушают канон формата: невалидный заголовок, длина, запись или
    /// wire-limit. `section`/`offset` указывают на начало нарушившей записи;
    /// для заголовочных отказов секция — `"header"`.
    Wire {
        /// Секция формата, в которой зафиксирован отказ.
        section: ProgramWireSectionNameV1,
        /// Смещение начала записи в байтах (0 для заголовочных отказов).
        offset: usize,
    },
    /// Байты канонны, но граф отвергнут семантической компиляцией.
    ///
    /// Детализация класса намеренно не публикуется в v1: полный typed
    /// compile-контракт публикует атомарный C7c; преждевременная строковая
    /// проекция 62 внутренних классов стала бы Hyrum-контрактом до него.
    Compile,
    /// Недостаточно ресурсов для семантической компиляции.
    ResourceExhausted,
    /// Нарушен внутренний инвариант компилятора.
    InternalInvariant,
}

fn section_name(error: &ProgramWireErrorV1) -> (ProgramWireSectionNameV1, usize) {
    use crate::program::wire::WireSectionV1 as Section;
    match error {
        ProgramWireErrorV1::InvalidMagic
        | ProgramWireErrorV1::UnsupportedVersion { .. }
        | ProgramWireErrorV1::InvalidLength => ("header", 0),
        ProgramWireErrorV1::InvalidDeclaration { section, offset } => (
            match section {
                Section::Header => "header",
                Section::Sources => "sources",
                Section::Targets => "targets",
                Section::Families => "families",
                Section::JointSelection => "joint-selection",
                Section::SurfaceInputPorts => "surface-input-ports",
                Section::OpacityInputs => "opacity-inputs",
                Section::Paints => "paints",
                Section::Surfaces => "surfaces",
                Section::Occurrences => "occurrences",
                Section::PresentationRoots => "presentation-roots",
                Section::PresentationTargets => "presentation-targets",
                Section::HardConstraints => "hard-constraints",
                Section::ReportConstraints => "report-constraints",
                Section::Outputs => "outputs",
                Section::Trailer => "trailer",
            },
            *offset,
        ),
        ProgramWireErrorV1::ResourceExhausted { section }
        | ProgramWireErrorV1::AssignmentResourceExhausted { section } => (
            match section {
                Section::Sources => "sources",
                Section::Targets => "targets",
                Section::Families => "families",
                Section::JointSelection => "joint-selection",
                Section::SurfaceInputPorts => "surface-input-ports",
                Section::OpacityInputs => "opacity-inputs",
                Section::Paints => "paints",
                Section::Surfaces => "surfaces",
                Section::Occurrences => "occurrences",
                Section::PresentationRoots => "presentation-roots",
                Section::PresentationTargets => "presentation-targets",
                Section::HardConstraints => "hard-constraints",
                Section::ReportConstraints => "report-constraints",
                Section::Outputs => "outputs",
                Section::Header | Section::Trailer => "header",
            },
            0,
        ),
    }
}

/// Проверяет канонические wire-байты и возвращает content identity графа.
///
/// Identity — SHA-256 канонического прообраза скомпилированного содержания
/// (инвариантна к переименованию клиентских ID). Успех доказывает: байты
/// канонны, граф компилируем, identity адресуема — ничего больше; никакой
/// runtime-authority этот вызов не выдаёт.
///
/// # Errors
///
/// [`ProgramWireCheckErrorV1::Wire`] — байты нарушают канон формата;
/// [`ProgramWireCheckErrorV1::Compile`] — граф семантически невалиден.
pub fn check_program_wire_v1(bytes: &[u8]) -> Result<[u8; 32], ProgramWireCheckErrorV1> {
    checked_draft(decode_program_wire_v1(bytes))
}

/// Check a supported Program envelope without creating runtime or writer authority.
///
/// V1 is unchanged; V2 additionally admits an explicit authored selection release.
/// Both versions compile the same Core graph and return its content identity.
///
/// # Errors
/// Returns a format, graph, resource or internal-invariant error at its original boundary.
pub fn check_program_wire(bytes: &[u8]) -> Result<[u8; 32], ProgramWireCheckErrorV1> {
    checked_draft(decode_program_wire(bytes))
}

fn checked_draft(
    decoded: Result<crate::program::DraftV1, ProgramWireErrorV1>,
) -> Result<[u8; 32], ProgramWireCheckErrorV1> {
    let draft = decoded.map_err(|error| {
        if matches!(
            error,
            ProgramWireErrorV1::AssignmentResourceExhausted { .. }
        ) {
            return ProgramWireCheckErrorV1::ResourceExhausted;
        }
        let (section, offset) = section_name(&error);
        ProgramWireCheckErrorV1::Wire { section, offset }
    })?;
    let owner = draft.compile().map_err(|error| match error {
        crate::program::CompileErrorV1::ResourceExhausted => {
            ProgramWireCheckErrorV1::ResourceExhausted
        }
        crate::program::CompileErrorV1::InternalInvariant => {
            ProgramWireCheckErrorV1::InternalInvariant
        }
        _ => ProgramWireCheckErrorV1::Compile,
    })?;
    Ok(*owner.content_identity().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::wire::{
        PROGRAM_WIRE_MAGIC_V1, PROGRAM_WIRE_VERSION_V1, ProgramWireBuilderV1,
    };

    fn canonical_reference_bytes() -> Vec<u8> {
        let mut builder = ProgramWireBuilderV1::new();
        builder
            .source(11, crate::Srgb8::new([0x14, 0x14, 0x14]))
            .fixed_target(21, 11)
            .surface_input_port(31)
            .solid_paint(41, 21)
            .input_surface(51, 31)
            .source_over_occurrence(61, 41, 51, 64.0, 0.2, 1)
            .presentation_root(71, 61)
            .presentation_target(71, 61)
            .wcag22_visible_unary(true, 81, 61, 3)
            .output(91, 41);
        builder.finish().unwrap()
    }

    /// Успех возвращает 32-байтную identity, равную identity прямой компиляции.
    #[test]
    fn canonical_bytes_yield_the_compiled_content_identity() {
        let identity = check_program_wire_v1(&canonical_reference_bytes()).unwrap();
        assert_eq!(identity.len(), 32);
        assert_ne!(identity, [0; 32], "identity must be a real digest");
        // Determinism: одни байты — одна identity.
        assert_eq!(
            identity,
            check_program_wire_v1(&canonical_reference_bytes()).unwrap()
        );
    }

    /// Байтовый дефект — Wire-отказ с секцией; runtime не выдаётся.
    #[test]
    fn wire_defects_surface_the_section() {
        let mut bytes = canonical_reference_bytes();
        bytes[0] = b'X';
        assert!(matches!(
            check_program_wire_v1(&bytes),
            Err(ProgramWireCheckErrorV1::Wire {
                section: "header",
                offset: 0
            })
        ));
    }

    /// Семантический дефект — Compile-отказ без байтовой диагностики.
    #[test]
    fn semantic_defects_surface_as_compile_refusals() {
        // Paint -> dangling target: канонные байты, невалидный граф.
        let mut builder = ProgramWireBuilderV1::new();
        builder.solid_paint(41, 999).output(91, 41);
        let bytes = builder.finish().unwrap();
        assert!(matches!(
            check_program_wire_v1(&bytes),
            Err(ProgramWireCheckErrorV1::Compile)
        ));
    }

    /// Магия и версия — контракт формата: сдвиг любого из них ломает канон.
    #[test]
    fn format_pins_are_part_of_the_contract() {
        assert_eq!(PROGRAM_WIRE_MAGIC_V1, *b"LCPW");
        assert_eq!(PROGRAM_WIRE_VERSION_V1, 1);
    }
}

#[cfg(test)]
mod fixture_migration_tests {
    use super::*;
    use crate::program::wire::ProgramWireBuilderV1;

    /// Миграционное доказательство (срез 6 wire-узла): 11-узловый граф
    /// приватного fixture ABI v2 полностью выражается ПУБЛИЧНОЙ wire-грамматикой
    /// и даёт живую content identity через единственный публичный seam.
    ///
    /// Это условие входа в C7c: после публикации полного контракта fixture ABI
    /// остаётся compat-слоем, а публичная грамматика уже сегодня покрывает его
    /// топологию (source -> fixed target -> solid paint -> opacity paint ->
    /// input surface -> source-over occurrence -> presentation root/target ->
    /// exact visible hard -> output). Идентификаторы — те же ordinals, что и в
    /// private_fixture.rs (AUTHORED_SOURCE=1 .. OUTPUT=17).
    #[test]
    fn the_private_fixture_graph_is_expressible_in_the_public_grammar() {
        let mut builder = ProgramWireBuilderV1::new();
        builder
            .source(1, crate::Srgb8::new([0x40, 0x40, 0x40]))
            .fixed_target(2, 1)
            .surface_input_port(6)
            .opacity_input(5, 0.5)
            .solid_paint(3, 2)
            .opacity_paint(4, 3, 5)
            .input_surface(7, 6)
            .source_over_occurrence(8, 4, 7, 64.0, 0.2, 2)
            .presentation_root(9, 8)
            .presentation_target(9, 8)
            .exact_visible_unary(true, 10, 8, crate::Srgb8::new([0x60, 0x60, 0x60]))
            .output(17, 4);
        let bytes = builder.finish().unwrap();

        // Публичный seam: байты канонны... но exact-constraint с произвольным
        // expected до attachment невыполним — компиляция графа тем не менее
        // обязана пройти (constraint исполняется в runtime, не при compile).
        let identity = check_program_wire_v1(&bytes).expect(
            "the fixture topology must be canonical and compilable through the public seam",
        );
        assert_ne!(identity, [0; 32]);

        // Determinism поверх полного fixture-графа.
        assert_eq!(identity, check_program_wire_v1(&bytes).unwrap());
    }
}

#[cfg(test)]
mod fv01_red_tests {
    use super::*;
    use core::cell::Cell;
    use std::rc::Rc;

    const OUTPUT: u32 = 17;
    const ROOT: u32 = 9;
    const OCCURRENCE: u32 = 8;
    const SINK_OUTPUT: u32 = 91;

    fn reference_wire(expected: Srgb8) -> Vec<u8> {
        let mut builder = crate::program::wire::ProgramWireBuilderV1::new();
        builder
            .source(1, Srgb8::new([0x40, 0x40, 0x40]))
            .fixed_target(2, 1)
            .surface_input_port(6)
            .opacity_input(5, 0.5)
            .solid_paint(3, 2)
            .opacity_paint(4, 3, 5)
            .input_surface(7, 6)
            .source_over_occurrence(OCCURRENCE, 4, 7, 64.0, 0.2, 2)
            .presentation_root(ROOT, OCCURRENCE)
            .presentation_target(ROOT, OCCURRENCE)
            .exact_visible_unary(true, 10, OCCURRENCE, expected)
            .output(OUTPUT, 4);
        builder.finish().unwrap()
    }

    fn non_terminal_target_wire() -> Vec<u8> {
        let mut builder = crate::program::wire::ProgramWireBuilderV1::new();
        builder
            .source(1, Srgb8::new([0x40; 3]))
            .fixed_target(2, 1)
            .surface_input_port(6)
            .opacity_input(5, 0.5)
            .solid_paint(3, 2)
            .opacity_paint(4, 3, 5)
            .input_surface(7, 6)
            .source_over_occurrence(8, 4, 7, 64.0, 0.2, 2)
            .occurrence_surface(11, 8)
            .source_over_occurrence(12, 4, 11, 64.0, 0.2, 2)
            .presentation_root(ROOT, 12)
            .presentation_target(ROOT, OCCURRENCE)
            .exact_visible_unary(true, 10, 12, Srgb8::new([0x60; 3]))
            .output(OUTPUT, 4);
        builder.finish().unwrap()
    }

    fn multi_case_wire() -> Vec<u8> {
        let mut builder = crate::program::wire::ProgramWireBuilderV1::new();
        builder
            .source(1, Srgb8::new([0; 3]))
            .fixed_target(2, 1)
            .surface_input_port(6)
            .opacity_input(5, 0.5)
            .solid_paint(3, 2)
            .opacity_paint(4, 3, 5)
            .input_surface(7, 6)
            .source_over_occurrence(8, 4, 7, 64.0, 0.2, 2)
            .presentation_root(ROOT, OCCURRENCE)
            .presentation_target(ROOT, OCCURRENCE)
            .wcag22_visible_unary(true, 10, OCCURRENCE, 3)
            .output(OUTPUT, 4);
        builder.finish().unwrap()
    }

    #[derive(Default)]
    struct Host {
        stamp: Option<ProgramPointSinkStampV1>,
        intents: Vec<ProgramPointSinkIntentV1>,
    }

    impl ProgramPointSinkHostV1 for Host {
        fn try_install(
            &mut self,
            intent: ProgramPointSinkIntentV1,
        ) -> Result<(), ProgramPointSinkHostErrorV1> {
            if self
                .stamp
                .is_some_and(|stamp| intent.expected_stamp() != stamp)
            {
                return Err(ProgramPointSinkHostErrorV1::Rejected);
            }
            self.stamp = Some(intent.desired_stamp());
            self.intents.push(intent);
            Ok(())
        }
    }

    fn ready_attachment() -> ProgramAttachmentV1<Host> {
        ready_attachment_for_expected(Srgb8::new([0x60; 3]))
    }

    fn ready_attachment_for_expected(expected: Srgb8) -> ProgramAttachmentV1<Host> {
        let compiled = compile_program_wire_v1(&reference_wire(expected)).unwrap();
        compiled
            .attach(7, OUTPUT, SINK_OUTPUT, ROOT, OCCURRENCE, Host::default())
            .unwrap()
    }

    #[test]
    fn public_attachment_rejects_an_intermediate_presentation_target() {
        let compiled = compile_program_wire_v1(&non_terminal_target_wire()).unwrap();
        let result = compiled.attach(7, OUTPUT, SINK_OUTPUT, ROOT, OCCURRENCE, Host::default());
        assert!(matches!(
            result,
            Err(ProgramAttachErrorV1::NonTerminalTarget)
        ));
    }

    #[test]
    fn public_attachment_resource_failure_keeps_head_and_allows_same_revision_retry() {
        let mut attachment = ready_attachment();
        let scenarios = [ProgramScenarioV1::new(1, vec![Srgb8::new([0x80; 3])])];
        {
            let _failure = crate::program_session::fail_program_preflight_reservation_for_test(0);
            assert!(matches!(
                attachment.update_observed(1, &scenarios),
                Err(ProgramAttachmentUpdateErrorV1::ResourceExhausted)
            ));
            assert_eq!(
                crate::program_session::program_preflight_failure_remaining_for_test(),
                None
            );
        }
        assert!(attachment.current_render().is_none());
        let retry = attachment.update_observed(1, &scenarios).unwrap();
        let render = retry.render().unwrap();
        assert_eq!(render.revision(), 1);
        assert_eq!(render.terminal_composite(), Some(Srgb8::new([0x60; 3])));
        assert_eq!(attachment.current_render(), Some(render));
        assert_eq!(render.sink_stamp().sequence(), 1);
    }

    #[test]
    fn ready_attachment_is_the_only_materialization_authority_source() {
        let mut attachment = ready_attachment();
        let snapshot = attachment
            .update_observed(1, &[ProgramScenarioV1::new(1, vec![Srgb8::new([0x80; 3])])])
            .unwrap();
        let render = snapshot.render().unwrap();
        let expectation = render.expectation();
        let authority = attachment
            .materialization_authority(
                ProgramMaterializationCandidateV1::AttachedCommit,
                expectation,
            )
            .unwrap();

        assert_eq!(authority.terminal_composite(), Srgb8::new([0x60; 3]));
        assert_eq!(
            authority.renderer_provenance(),
            ProgramRendererProvenanceV1::Unverified
        );
        assert_eq!(authority.context(), render.context());
    }

    #[test]
    fn paint_and_stale_binding_inputs_are_typed_refusals() {
        let mut attachment = ready_attachment();
        let snapshot = attachment
            .update_observed(1, &[ProgramScenarioV1::new(1, vec![Srgb8::new([0x80; 3])])])
            .unwrap();
        let render = snapshot.render().unwrap();
        let expectation = render.expectation();
        let paint = snapshot.snapshot().outputs()[0];

        assert!(matches!(
            attachment.materialization_authority(
                ProgramMaterializationCandidateV1::SourcePaint(paint),
                expectation,
            ),
            Err(ProgramMaterializationAuthorityErrorV1::SourceOrIntermediatePaint)
        ));
        assert!(matches!(
            attachment.materialization_authority(
                ProgramMaterializationCandidateV1::IntermediatePaint(paint),
                expectation,
            ),
            Err(ProgramMaterializationAuthorityErrorV1::SourceOrIntermediatePaint)
        ));
        assert!(matches!(
            attachment.materialization_authority(
                ProgramMaterializationCandidateV1::AttachedCommit,
                expectation.with_revision(0),
            ),
            Err(ProgramMaterializationAuthorityErrorV1::StaleRevision)
        ));
        assert!(matches!(
            attachment.materialization_authority(
                ProgramMaterializationCandidateV1::AttachedCommit,
                expectation.with_content_identity([0xA5; 32]),
            ),
            Err(ProgramMaterializationAuthorityErrorV1::StaleIdentity)
        ));
        assert!(matches!(
            attachment.materialization_authority(
                ProgramMaterializationCandidateV1::AttachedCommit,
                expectation.with_binding_epoch(expectation.binding_epoch().wrapping_add(1)),
            ),
            Err(ProgramMaterializationAuthorityErrorV1::ForeignBindingEpoch)
        ));
    }

    #[test]
    fn authority_uses_the_current_valid_render_not_a_constant_verdict() {
        let mut attachment = ready_attachment_for_expected(Srgb8::new([0x70; 3]));
        let snapshot = attachment
            .update_observed(1, &[ProgramScenarioV1::new(1, vec![Srgb8::new([0xA0; 3])])])
            .unwrap();
        assert_eq!(
            snapshot.render().unwrap().terminal_composite(),
            Some(Srgb8::new([0x70; 3]))
        );

        let authority = attachment.current_materialization_authority().unwrap();
        assert_eq!(authority.terminal_composite(), Srgb8::new([0x70; 3]));
    }

    #[test]
    fn authority_refuses_conflicting_terminal_composites_across_observation_cases() {
        let compiled = compile_program_wire_v1(&multi_case_wire()).unwrap();
        let mut attachment = compiled
            .attach(7, OUTPUT, SINK_OUTPUT, ROOT, OCCURRENCE, Host::default())
            .unwrap();
        let snapshot = attachment
            .update_observed(
                1,
                &[
                    ProgramScenarioV1::new(1, vec![Srgb8::new([0xFF; 3])]),
                    ProgramScenarioV1::new(2, vec![Srgb8::new([0xFE; 3])]),
                ],
            )
            .unwrap();
        assert_eq!(snapshot.render().unwrap().terminal_composite(), None);
        assert!(matches!(
            attachment.current_materialization_authority(),
            Err(ProgramMaterializationAuthorityErrorV1::AmbiguousObservationCases)
        ));
    }

    #[test]
    fn host_refusal_preserves_the_previous_materialization() {
        struct RejectingHost {
            reject: Rc<Cell<bool>>,
        }

        impl ProgramPointSinkHostV1 for RejectingHost {
            fn try_install(
                &mut self,
                _intent: ProgramPointSinkIntentV1,
            ) -> Result<(), ProgramPointSinkHostErrorV1> {
                if self.reject.get() {
                    Err(ProgramPointSinkHostErrorV1::Rejected)
                } else {
                    Ok(())
                }
            }
        }

        let control = Rc::new(Cell::new(false));
        let compiled = compile_program_wire_v1(&reference_wire(Srgb8::new([0x60; 3]))).unwrap();
        let mut attachment = compiled
            .attach(
                7,
                OUTPUT,
                SINK_OUTPUT,
                ROOT,
                OCCURRENCE,
                RejectingHost {
                    reject: control.clone(),
                },
            )
            .unwrap();
        let first = attachment
            .update_observed(1, &[ProgramScenarioV1::new(1, vec![Srgb8::new([0x80; 3])])])
            .unwrap();
        let expected = first.render().unwrap().expectation();
        control.set(true);

        assert!(matches!(
            attachment
                .update_observed(2, &[ProgramScenarioV1::new(1, vec![Srgb8::new([0x80; 3])])],),
            Err(ProgramAttachmentUpdateErrorV1::Sink(
                ProgramPointSinkErrorV1::Host(ProgramPointSinkHostErrorV1::Rejected)
            ))
        ));
        let authority = attachment
            .materialization_authority(ProgramMaterializationCandidateV1::AttachedCommit, expected)
            .unwrap();
        assert_eq!(authority.revision(), 1);
        assert_eq!(authority.terminal_composite(), Srgb8::new([0x60; 3]));
    }
}

/// Полностью скомпилированный Program без runtime-authority.
///
/// Владеет immutable графом и content identity; Session появляется только
/// через consuming [`Self::instantiate`], поэтому один runtime не может молча
/// разделить владельца с другим.
pub struct CompiledProgramV1 {
    owner: crate::program::OwnerV1,
}

/// Единственный runtime-владелец одной Session публичного Program.
pub struct ProgramSessionV1 {
    owner: crate::program::OwnerV1,
    session: crate::program::SessionV1,
    outputs_scratch: Vec<ProgramPaintOutputV1>,
}

/// Один сценарий наблюдаемой среды: непрозрачный ID и значения surface inputs
/// в каноническом порядке, объявленном Program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramScenarioV1 {
    id: u32,
    surfaces: Vec<crate::Srgb8>,
}

impl ProgramScenarioV1 {
    /// Создаёт owned-сценарий; кардинальность сверяется с Program при update.
    #[must_use]
    pub fn new(id: u32, surfaces: Vec<crate::Srgb8>) -> Self {
        Self { id, surfaces }
    }
}

/// Lifecycle-класс одного опубликованного snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramSnapshotStateV1 {
    Waiting,
    Ready,
    Stale,
    Failed,
    Unsatisfied,
    Incomplete,
}

/// Один сертифицированный Paint output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgramPaintOutputV1 {
    slot: u32,
    source: crate::Srgb8,
    opacity: f64,
}

impl ProgramPaintOutputV1 {
    /// Непрозрачный клиентский output slot.
    #[must_use]
    pub const fn slot(self) -> u32 {
        self.slot
    }

    /// Сертифицированный encoded sRGB8 source.
    #[must_use]
    pub const fn source(self) -> crate::Srgb8 {
        self.source
    }

    /// Сертифицированная straight opacity в `0..=1`.
    #[must_use]
    pub const fn opacity(self) -> f64 {
        self.opacity
    }
}

/// Owned snapshot Session после атомарного update.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgramSnapshotV1 {
    snapshot_version: u32,
    revision: u64,
    state: ProgramSnapshotStateV1,
    outputs: Vec<ProgramPaintOutputV1>,
}

impl ProgramSnapshotV1 {
    /// Версия проекции: 1 сохраняет historical outputs V1/V2, 2 показывает current V3.
    #[must_use]
    pub const fn snapshot_version(&self) -> u32 {
        self.snapshot_version
    }

    /// Ревизия принятого наблюдения, независимо от наличия current outputs.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Причина остановки вычисления; Unknown reason ID имеет отдельный смысл.
    #[must_use]
    pub const fn reason(&self) -> Option<&'static str> {
        if self.snapshot_version == 2 && matches!(self.state, ProgramSnapshotStateV1::Incomplete) {
            Some("limit")
        } else {
            None
        }
    }

    #[must_use]
    pub const fn state(&self) -> ProgramSnapshotStateV1 {
        self.state
    }

    #[must_use]
    pub fn outputs(&self) -> &[ProgramPaintOutputV1] {
        &self.outputs
    }
}

/// Непрозрачная identity допущенного контекста представления Occurrence.
///
/// Контекст не является renderer-наблюдением. Его bytes — отдельный
/// стабильный digest входов контекста и служат только для проверки binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProgramAppearanceContextIdV1(crate::lcs_occurrence::AppearanceContextId);

impl ProgramAppearanceContextIdV1 {
    fn from_core(value: crate::lcs_occurrence::AppearanceContextId) -> Self {
        Self(value)
    }

    /// Возвращает identity именно этого допущенного контекста представления.
    #[must_use]
    pub fn identity_bytes(self) -> [u8; 32] {
        use crate::lcs_occurrence::{
            AppearanceContextSchemaReleaseId, ColorimetricFrameReleaseId, ObserverProfileId,
            ReferenceWhiteId, SurroundProfileId, TristimulusScale,
        };

        let context = self.0;
        let mut hasher = crate::sha256::Hasher::new();
        hasher.update(b"labcolors.program-appearance-context.v1\0");
        hasher.update(&[match context.schema_release() {
            AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1 => 1,
        }]);
        let frame = context.frame();
        hasher.update(&[match frame.observer() {
            ObserverProfileId::Cie1931TwoDegreeV1 => 1,
        }]);
        hasher.update(&[match frame.reference_white() {
            ReferenceWhiteId::Iec61966D65ChromaticityV1 => 1,
        }]);
        hasher.update(&[match frame.scale() {
            TristimulusScale::RelativeY1 => 1,
        }]);
        hasher.update(&[match frame.release() {
            ColorimetricFrameReleaseId::XyzV1 => 1,
            #[cfg(test)]
            ColorimetricFrameReleaseId::MutationSentinelV1 => 2,
        }]);
        hasher.update(&context.adapting_luminance_cd_m2().to_bits().to_be_bytes());
        hasher.update(&context.background_luminance_ratio().to_bits().to_be_bytes());
        hasher.update(&[match context.surround_profile() {
            SurroundProfileId::AverageV1 => 1,
            SurroundProfileId::DimV1 => 2,
            SurroundProfileId::DarkV1 => 3,
        }]);
        *hasher.finalize().as_bytes()
    }
}

/// CAS identity одной допущенной привязки к host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProgramPointSinkStampV1 {
    sequence: u64,
    binding_epoch: u64,
}

impl ProgramPointSinkStampV1 {
    /// Создаёт значение для host-протокола. Реальная attachment-эпоха всегда
    /// не нулевая и минтится Core admission, а не вызывающим кодом.
    #[must_use]
    pub const fn new(sequence: u64, binding_epoch: u64) -> Self {
        Self {
            sequence,
            binding_epoch,
        }
    }

    /// Ожидаемая либо опубликованная последовательность.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    /// Эпоха привязки; смена realm или scope host должна её менять.
    #[must_use]
    pub const fn binding_epoch(self) -> u64 {
        self.binding_epoch
    }
}

/// Команда, которую attachment передаёт единственной внешней границе эффекта.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramPointSinkOperationV1 {
    SetAll,
    RevokeAll,
    ConfirmExact,
}

/// Полный атомарный intent терминального point sink.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgramPointSinkIntentV1 {
    operation: ProgramPointSinkOperationV1,
    revision: u64,
    expected: ProgramPointSinkStampV1,
    desired: ProgramPointSinkStampV1,
    point: Option<ProgramPaintOutputV1>,
    sink_output: u32,
}

impl ProgramPointSinkIntentV1 {
    /// Вид команды.
    #[must_use]
    pub const fn operation(self) -> ProgramPointSinkOperationV1 {
        self.operation
    }

    /// Revision, публикуемая вместе с командой.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }

    /// CAS stamp до атомарной установки.
    #[must_use]
    pub const fn expected_stamp(self) -> ProgramPointSinkStampV1 {
        self.expected
    }

    /// CAS stamp после атомарной установки.
    #[must_use]
    pub const fn desired_stamp(self) -> ProgramPointSinkStampV1 {
        self.desired
    }

    /// Paint patch для SetAll/ConfirmExact; для RevokeAll отсутствует.
    #[must_use]
    pub const fn point(self) -> Option<ProgramPaintOutputV1> {
        self.point
    }

    /// Непрозрачный host-side ID принадлежащего sink output.
    #[must_use]
    pub const fn sink_output(self) -> u32 {
        self.sink_output
    }
}

/// Host отказал без изменения своего snapshot либо нарушил синхронный ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramPointSinkHostErrorV1 {
    Rejected,
    Protocol,
}

/// Единственная публичная граница эффекта attachment.
///
/// Реализация обязана применить intent атомарно целиком либо вернуть ошибку
/// без изменения своего owned snapshot. Она живёт у consumer/harness, а не в
/// sink пакета Lab-Colors.
pub trait ProgramPointSinkHostV1 {
    fn try_install(
        &mut self,
        intent: ProgramPointSinkIntentV1,
    ) -> Result<(), ProgramPointSinkHostErrorV1>;
}

/// Типизированный отказ холодного attachment до публикации в host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramAttachErrorV1 {
    Binding,
    Instantiate,
    SinkAdmission,
    ResourceExhausted,
    InternalInvariant,
    NonTerminalTarget,
}

/// Типизированный отказ update. Любая ошибка сохраняет предыдущие Session и состояние host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramPointSinkErrorV1 {
    PatchScopeMismatch,
    StampMismatch,
    RevisionMismatch,
    AlreadyInstalled,
    Host(ProgramPointSinkHostErrorV1),
}

/// Типизированный отказ update через attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramAttachmentUpdateErrorV1 {
    Update,
    ResourceExhausted,
    Sink(ProgramPointSinkErrorV1),
    InternalInvariant,
    AlreadyDisposed,
}

/// Внутренний физический профиль point-программы, если он привязан.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ProgramPhysicalIdentityV1 {
    EncodedSrgb8SourceOverV1,
}

/// Provenance renderer-а намеренно не усиливается первым срезом FV-01.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ProgramRendererProvenanceV1 {
    Unverified,
}

/// Компактное ожидание точной привязки attachment для допуска authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramMaterializationExpectationV1 {
    content_identity: [u8; 32],
    revision: u64,
    sink_stamp: ProgramPointSinkStampV1,
    presentation_root: u32,
    occurrence: u32,
    context: ProgramAppearanceContextIdV1,
}

impl ProgramMaterializationExpectationV1 {
    /// Меняет ожидаемую content identity для негативной проверки.
    #[must_use]
    pub const fn with_content_identity(mut self, content_identity: [u8; 32]) -> Self {
        self.content_identity = content_identity;
        self
    }

    /// Меняет ожидаемую revision для негативной проверки.
    #[must_use]
    pub const fn with_revision(mut self, revision: u64) -> Self {
        self.revision = revision;
        self
    }

    /// Меняет ожидаемую эпоху привязки host для негативной проверки.
    #[must_use]
    pub const fn with_binding_epoch(mut self, binding_epoch: u64) -> Self {
        self.sink_stamp = ProgramPointSinkStampV1::new(self.sink_stamp.sequence(), binding_epoch);
        self
    }

    /// Ожидаемая content identity.
    #[must_use]
    pub const fn content_identity(self) -> [u8; 32] {
        self.content_identity
    }

    /// Ожидаемая revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }

    /// Ожидаемый sink stamp.
    #[must_use]
    pub const fn sink_stamp(self) -> ProgramPointSinkStampV1 {
        self.sink_stamp
    }

    /// Ожидаемый root.
    #[must_use]
    pub const fn presentation_root(self) -> u32 {
        self.presentation_root
    }

    /// Ожидаемый target occurrence.
    #[must_use]
    pub const fn occurrence(self) -> u32 {
        self.occurrence
    }

    /// Ожидаемый appearance context.
    #[must_use]
    pub const fn context(self) -> ProgramAppearanceContextIdV1 {
        self.context
    }

    /// Ожидаемая binding epoch.
    #[must_use]
    pub const fn binding_epoch(self) -> u64 {
        self.sink_stamp.binding_epoch()
    }
}

/// Единственный допустимый источник допуска authority.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ProgramMaterializationCandidateV1 {
    /// Использовать точный `Ready` commit текущего attached snapshot.
    AttachedCommit,
    /// Обычный сертифицированный Paint: он не является attachment.
    SourcePaint(ProgramPaintOutputV1),
    /// Paint промежуточного слоя: он не является terminal root.
    IntermediatePaint(ProgramPaintOutputV1),
}

/// Типизированный отказ выдачи authority для attached materialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramMaterializationAuthorityErrorV1 {
    NotReady,
    SourceOrIntermediatePaint,
    StaleRevision,
    StaleIdentity,
    StaleSinkStamp,
    ForeignBindingEpoch,
    TerminalBindingMismatch,
    MissingPointAbsenceProof,
    AmbiguousObservationCases,
}

/// Материализация, полученная только из `Ready` commit attachment.
///
/// Это не portable certificate, не renderer observation и не утверждение о
/// восприятии. `renderer_provenance=Unverified` оставляет эту границу явной.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttachedMaterializationAuthorityV1 {
    output: ProgramPaintOutputV1,
    content_identity: [u8; 32],
    physical_identity: Option<ProgramPhysicalIdentityV1>,
    revision: u64,
    sink_stamp: ProgramPointSinkStampV1,
    presentation_root: u32,
    occurrence: u32,
    context: ProgramAppearanceContextIdV1,
    renderer_provenance: ProgramRendererProvenanceV1,
    terminal_composite: Srgb8,
}

impl AttachedMaterializationAuthorityV1 {
    /// Сертифицированный source-plus-opacity output до внешнего sink.
    #[must_use]
    pub const fn output(self) -> ProgramPaintOutputV1 {
        self.output
    }

    /// Content identity Program.
    #[must_use]
    pub const fn content_identity(self) -> [u8; 32] {
        self.content_identity
    }

    /// Привязанный физический профиль, если он поддержан текущей схемой.
    #[must_use]
    pub const fn physical_identity(self) -> Option<ProgramPhysicalIdentityV1> {
        self.physical_identity
    }

    /// Revision, опубликованная вместе с materialization.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }

    /// Exact sink stamp.
    #[must_use]
    pub const fn sink_stamp(self) -> ProgramPointSinkStampV1 {
        self.sink_stamp
    }

    /// Terminal presentation root.
    #[must_use]
    pub const fn presentation_root(self) -> u32 {
        self.presentation_root
    }

    /// Terminal presentation occurrence.
    #[must_use]
    pub const fn occurrence(self) -> u32 {
        self.occurrence
    }

    /// Appearance context, вошедший в Program identity и binding.
    #[must_use]
    pub const fn context(self) -> ProgramAppearanceContextIdV1 {
        self.context
    }

    /// Provenance renderer boundary.
    #[must_use]
    pub const fn renderer_provenance(self) -> ProgramRendererProvenanceV1 {
        self.renderer_provenance
    }

    /// Exact modeled terminal composite, доказанный point absence replay.
    #[must_use]
    pub const fn terminal_composite(self) -> Srgb8 {
        self.terminal_composite
    }
}

/// Один materialized render после успешной установки в host; сам по себе он ещё
/// не выдаёт authority без проверки expectation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgramAttachedRenderV1 {
    output: ProgramPaintOutputV1,
    sink_output: u32,
    revision: u64,
    sink_stamp: ProgramPointSinkStampV1,
    content_identity: [u8; 32],
    presentation_root: u32,
    occurrence: u32,
    context: ProgramAppearanceContextIdV1,
    physical_identity: Option<ProgramPhysicalIdentityV1>,
    terminal_composite: Option<Srgb8>,
    terminal_composite_ambiguous: bool,
}

impl ProgramAttachedRenderV1 {
    /// Output, source и straight opacity.
    #[must_use]
    pub const fn output(self) -> ProgramPaintOutputV1 {
        self.output
    }

    /// Host-side sink output ID.
    #[must_use]
    pub const fn sink_output(self) -> u32 {
        self.sink_output
    }

    /// Опубликованная revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }

    /// Опубликованный sink stamp.
    #[must_use]
    pub const fn sink_stamp(self) -> ProgramPointSinkStampV1 {
        self.sink_stamp
    }

    /// Content identity.
    #[must_use]
    pub const fn content_identity(self) -> [u8; 32] {
        self.content_identity
    }

    /// Terminal root.
    #[must_use]
    pub const fn presentation_root(self) -> u32 {
        self.presentation_root
    }

    /// Terminal occurrence.
    #[must_use]
    pub const fn occurrence(self) -> u32 {
        self.occurrence
    }

    /// Bound appearance context.
    #[must_use]
    pub const fn context(self) -> ProgramAppearanceContextIdV1 {
        self.context
    }

    /// Bound physical profile.
    #[must_use]
    pub const fn physical_identity(self) -> Option<ProgramPhysicalIdentityV1> {
        self.physical_identity
    }

    /// Modeled terminal composite, отсутствующий при пустом owned domain.
    #[must_use]
    pub const fn terminal_composite(self) -> Option<Srgb8> {
        self.terminal_composite
    }

    /// Делает expectation для собственного authority admission.
    #[must_use]
    pub const fn expectation(self) -> ProgramMaterializationExpectationV1 {
        ProgramMaterializationExpectationV1 {
            content_identity: self.content_identity,
            revision: self.revision,
            sink_stamp: self.sink_stamp,
            presentation_root: self.presentation_root,
            occurrence: self.occurrence,
            context: self.context,
        }
    }
}

/// Owned snapshot attachment. Render-проекция присутствует только для Ready.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgramAttachedSnapshotV1 {
    snapshot: ProgramSnapshotV1,
    render: Option<ProgramAttachedRenderV1>,
}

impl ProgramAttachedSnapshotV1 {
    /// Evidence-only snapshot с обычным lifecycle и Paint outputs.
    #[must_use]
    pub const fn snapshot(&self) -> &ProgramSnapshotV1 {
        &self.snapshot
    }

    /// Exact render commit; `None` для Waiting/Stale/Failed.
    #[must_use]
    pub const fn render(&self) -> Option<ProgramAttachedRenderV1> {
        self.render
    }
}

fn materialization_authority_from_render(
    render: ProgramAttachedRenderV1,
    candidate: ProgramMaterializationCandidateV1,
    expected: ProgramMaterializationExpectationV1,
) -> Result<AttachedMaterializationAuthorityV1, ProgramMaterializationAuthorityErrorV1> {
    match candidate {
        ProgramMaterializationCandidateV1::AttachedCommit => {}
        ProgramMaterializationCandidateV1::SourcePaint(_)
        | ProgramMaterializationCandidateV1::IntermediatePaint(_) => {
            return Err(ProgramMaterializationAuthorityErrorV1::SourceOrIntermediatePaint);
        }
    }
    if render.revision != expected.revision {
        return Err(ProgramMaterializationAuthorityErrorV1::StaleRevision);
    }
    if render.content_identity != expected.content_identity {
        return Err(ProgramMaterializationAuthorityErrorV1::StaleIdentity);
    }
    if render.sink_stamp.binding_epoch() != expected.sink_stamp.binding_epoch() {
        return Err(ProgramMaterializationAuthorityErrorV1::ForeignBindingEpoch);
    }
    if render.sink_stamp.sequence() != expected.sink_stamp.sequence() {
        return Err(ProgramMaterializationAuthorityErrorV1::StaleSinkStamp);
    }
    if render.presentation_root != expected.presentation_root
        || render.occurrence != expected.occurrence
        || render.context != expected.context
    {
        return Err(ProgramMaterializationAuthorityErrorV1::TerminalBindingMismatch);
    }
    if render.terminal_composite_ambiguous {
        return Err(ProgramMaterializationAuthorityErrorV1::AmbiguousObservationCases);
    }
    let Some(terminal_composite) = render.terminal_composite else {
        return Err(ProgramMaterializationAuthorityErrorV1::MissingPointAbsenceProof);
    };
    Ok(AttachedMaterializationAuthorityV1 {
        output: render.output,
        content_identity: render.content_identity,
        physical_identity: render.physical_identity,
        revision: render.revision,
        sink_stamp: render.sink_stamp,
        presentation_root: render.presentation_root,
        occurrence: render.occurrence,
        context: render.context,
        renderer_provenance: ProgramRendererProvenanceV1::Unverified,
        terminal_composite,
    })
}

/// Ошибка подтверждения освобождения attachment, принадлежащего внешнему host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramDisposeErrorV1<E> {
    AlreadyDisposed,
    Confirmation(E),
}

struct ProgramPointSinkHostAdapterV1<H> {
    host: H,
    sink_output: crate::program::attachment::handoff::HandoffPointSinkOutputIdV1,
}

impl<H> crate::program::attachment::handoff::HandoffPointSinkHostV1
    for ProgramPointSinkHostAdapterV1<H>
where
    H: ProgramPointSinkHostV1,
{
    fn try_install(
        &mut self,
        intent: crate::program::attachment::handoff::HandoffPointSinkHostIntentV1,
    ) -> Result<(), crate::program::attachment::handoff::HandoffPointSinkHostErrorV1> {
        let operation = match intent.operation() {
            1 => ProgramPointSinkOperationV1::SetAll,
            2 => ProgramPointSinkOperationV1::RevokeAll,
            3 => ProgramPointSinkOperationV1::ConfirmExact,
            _ => {
                return Err(
                    crate::program::attachment::handoff::HandoffPointSinkHostErrorV1::Protocol,
                );
            }
        };
        let binding_epoch = intent.binding_epoch().value().get();
        let point = intent
            .point()
            .map(|(output, _sink_output, paint)| ProgramPaintOutputV1 {
                slot: output.value(),
                source: paint.source(),
                opacity: f64::from_bits(paint.opacity_bits()),
            });
        let public_intent = ProgramPointSinkIntentV1 {
            operation,
            revision: intent.revision(),
            expected: ProgramPointSinkStampV1::new(intent.expected_sequence(), binding_epoch),
            desired: ProgramPointSinkStampV1::new(intent.desired_sequence(), binding_epoch),
            point,
            sink_output: self.sink_output.value(),
        };
        self.host
            .try_install(public_intent)
            .map_err(|error| match error {
                ProgramPointSinkHostErrorV1::Rejected => {
                    crate::program::attachment::handoff::HandoffPointSinkHostErrorV1::Rejected
                }
                ProgramPointSinkHostErrorV1::Protocol => {
                    crate::program::attachment::handoff::HandoffPointSinkHostErrorV1::Protocol
                }
            })
    }
}

type ProgramInnerAttachmentV1<H> =
    crate::program::attachment::handoff::HandoffAttachmentV1<ProgramPointSinkHostAdapterV1<H>>;

/// Публичный типизированный attachment для одного неизменяемого Program и sink,
/// принадлежащего host.
///
/// Host получает полный point intent и остаётся владельцем DOM или другого
/// внешнего состояния. Lab-Colors сам не записывает renderer surface.
pub struct ProgramAttachmentV1<H>
where
    H: ProgramPointSinkHostV1,
{
    inner: Option<ProgramInnerAttachmentV1<H>>,
    output_count: usize,
    outputs_scratch: Vec<ProgramPaintOutputV1>,
    current_render: Option<ProgramAttachedRenderV1>,
}

impl CompiledProgramV1 {
    /// Создаёт один точный terminal point attachment из скомпилированного Program.
    ///
    /// Первый публичный seam поддерживает один output и одну presentation target;
    /// вся остальная топология output/presentation отклоняется допуском Core.
    pub fn attach<H>(
        self,
        stream_id: u32,
        output_slot: u32,
        sink_output: u32,
        presentation_root: u32,
        occurrence: u32,
        host: H,
    ) -> Result<ProgramAttachmentV1<H>, ProgramAttachErrorV1>
    where
        H: ProgramPointSinkHostV1,
    {
        let output_count = self.owner.output_slots().count();
        if output_count != 1 {
            return Err(ProgramAttachErrorV1::Binding);
        }
        self.owner
            .bind_terminal_point_output_presentation(
                crate::program::OutputSlotIdV1::new(output_slot),
                crate::program::PresentationRootIdV1::new(presentation_root),
                crate::program::OccurrenceIdV1::new(occurrence),
            )
            .map_err(map_attachment_binding_error)?;
        let mut outputs_scratch = Vec::new();
        outputs_scratch
            .try_reserve_exact(output_count)
            .map_err(|_| ProgramAttachErrorV1::ResourceExhausted)?;

        let sink_output_id =
            crate::program::attachment::handoff::HandoffPointSinkOutputIdV1::new(sink_output);
        let sink = crate::program::attachment::handoff::handoff_point_sink(
            sink_output_id,
            ProgramPointSinkHostAdapterV1 {
                host,
                sink_output: sink_output_id,
            },
        );
        let emissions = [
            crate::program::attachment::AuthoredPointEmissionBindingV1::new(
                crate::program::OutputSlotIdV1::new(output_slot),
                sink_output_id,
            ),
        ];
        let presentations = [
            crate::program::attachment::AuthoredPointPresentationBindingV1::new(
                crate::program::OutputSlotIdV1::new(output_slot),
                crate::program::PresentationRootIdV1::new(presentation_root),
                crate::program::OccurrenceIdV1::new(occurrence),
            ),
        ];
        let inner = self
            .owner
            .attach_external(
                stream_id,
                &emissions,
                &presentations,
                crate::family_artifact::FamilyArtifactBundleV2::empty(),
                sink,
            )
            .map_err(|failure| match failure.kind() {
                crate::program::attachment::AttachmentCreateFailureKindV1::Binding => {
                    ProgramAttachErrorV1::Binding
                }
                crate::program::attachment::AttachmentCreateFailureKindV1::Instantiate => {
                    ProgramAttachErrorV1::Instantiate
                }
                crate::program::attachment::AttachmentCreateFailureKindV1::SinkAdmission => {
                    ProgramAttachErrorV1::SinkAdmission
                }
                crate::program::attachment::AttachmentCreateFailureKindV1::ResourceExhausted => {
                    ProgramAttachErrorV1::ResourceExhausted
                }
                crate::program::attachment::AttachmentCreateFailureKindV1::InternalInvariant => {
                    ProgramAttachErrorV1::InternalInvariant
                }
            })?;
        Ok(ProgramAttachmentV1 {
            inner: Some(inner),
            output_count,
            outputs_scratch,
            current_render: None,
        })
    }
}

fn map_attachment_binding_error(
    error: crate::program_session::PointOutputPresentationBindErrorV1,
) -> ProgramAttachErrorV1 {
    match error {
        crate::program_session::PointOutputPresentationBindErrorV1::InternalInvariant => {
            ProgramAttachErrorV1::InternalInvariant
        }
        crate::program_session::PointOutputPresentationBindErrorV1::NonTerminalTarget {
            ..
        } => ProgramAttachErrorV1::NonTerminalTarget,
        _ => ProgramAttachErrorV1::Binding,
    }
}

impl<H> ProgramAttachmentV1<H>
where
    H: ProgramPointSinkHostV1,
{
    /// Атомарно применяет observed revision и возвращает owned snapshot.
    pub fn update_observed(
        &mut self,
        revision: u64,
        scenarios: &[ProgramScenarioV1],
    ) -> Result<ProgramAttachedSnapshotV1, ProgramAttachmentUpdateErrorV1> {
        let mut core_scenarios = Vec::new();
        core_scenarios
            .try_reserve_exact(scenarios.len())
            .map_err(|_| ProgramAttachmentUpdateErrorV1::ResourceExhausted)?;
        for scenario in scenarios {
            core_scenarios.push(crate::program::ScenarioV1::new(
                scenario.id,
                &scenario.surfaces,
            ));
        }
        self.apply(crate::program::UpdateV1::Observed {
            revision,
            scenarios: &core_scenarios,
        })
    }

    /// Атомарно отзывает sink для недоступного observation.
    pub fn update_unknown(
        &mut self,
        revision: u64,
        reason_id: u32,
    ) -> Result<ProgramAttachedSnapshotV1, ProgramAttachmentUpdateErrorV1> {
        self.apply(crate::program::UpdateV1::Unknown {
            revision,
            reason_id,
        })
    }

    /// Потребляет attachment после точного revoke, подтверждённого внешним владельцем.
    pub fn dispose<E>(
        &mut self,
        confirm: impl FnOnce() -> Result<(), E>,
    ) -> Result<(), ProgramDisposeErrorV1<E>> {
        let result = crate::program::attachment::ExternallyManagedAttachmentV1::confirm_and_consume_external_dispose(
            &mut self.inner,
            confirm,
        );
        let disposed = result.is_ok();
        let result = result.map_err(|error| match error {
            crate::program::attachment::ExternalDisposeErrorV1::MissingAttachment => {
                ProgramDisposeErrorV1::AlreadyDisposed
            }
            crate::program::attachment::ExternalDisposeErrorV1::Confirmation(error) => {
                ProgramDisposeErrorV1::Confirmation(error)
            }
        });
        if disposed {
            self.current_render = None;
        }
        result
    }

    /// Текущий committed render; исторический snapshot его не заменяет.
    pub fn current_render(&self) -> Option<ProgramAttachedRenderV1> {
        self.current_render
    }

    /// Допускает authority только из текущего committed head attachment.
    /// Исторические snapshot не переносят эту capability.
    pub fn materialization_authority(
        &self,
        candidate: ProgramMaterializationCandidateV1,
        expected: ProgramMaterializationExpectationV1,
    ) -> Result<AttachedMaterializationAuthorityV1, ProgramMaterializationAuthorityErrorV1> {
        let Some(render) = self.current_render else {
            return Err(ProgramMaterializationAuthorityErrorV1::NotReady);
        };
        materialization_authority_from_render(render, candidate, expected)
    }

    /// Создаёт authority из одного атомарно прочитанного current head.
    pub fn current_materialization_authority(
        &self,
    ) -> Result<AttachedMaterializationAuthorityV1, ProgramMaterializationAuthorityErrorV1> {
        let Some(render) = self.current_render else {
            return Err(ProgramMaterializationAuthorityErrorV1::NotReady);
        };
        materialization_authority_from_render(
            render,
            ProgramMaterializationCandidateV1::AttachedCommit,
            render.expectation(),
        )
    }

    fn apply(
        &mut self,
        update: crate::program::UpdateV1<'_>,
    ) -> Result<ProgramAttachedSnapshotV1, ProgramAttachmentUpdateErrorV1> {
        let Self {
            inner,
            output_count,
            outputs_scratch,
            current_render,
        } = self;
        outputs_scratch
            .try_reserve_exact(*output_count)
            .map_err(|_| ProgramAttachmentUpdateErrorV1::ResourceExhausted)?;
        let attachment = inner
            .as_mut()
            .ok_or(ProgramAttachmentUpdateErrorV1::AlreadyDisposed)?;
        let commit = attachment
            .update(update)
            .map_err(map_attachment_update_error)?;
        let snapshot = attachment_snapshot_from_commit(commit, outputs_scratch)?;
        *current_render = snapshot.render();
        Ok(snapshot)
    }
}

fn map_attachment_update_error(
    error: crate::program::attachment::AttachmentUpdateErrorV1<
        crate::program::attachment::handoff::HandoffPointSinkErrorV1,
    >,
) -> ProgramAttachmentUpdateErrorV1 {
    match error {
        crate::program::attachment::AttachmentUpdateErrorV1::Update(error) => match error {
            crate::program::UpdateErrorV1::ResourceExhausted { .. } => {
                ProgramAttachmentUpdateErrorV1::ResourceExhausted
            }
            crate::program::UpdateErrorV1::InternalInvariant { .. } => {
                ProgramAttachmentUpdateErrorV1::InternalInvariant
            }
            _ => ProgramAttachmentUpdateErrorV1::Update,
        },
        crate::program::attachment::AttachmentUpdateErrorV1::SinkPrepare(error)
        | crate::program::attachment::AttachmentUpdateErrorV1::SinkInstall(error) => {
            ProgramAttachmentUpdateErrorV1::Sink(map_sink_error(error))
        }
        crate::program::attachment::AttachmentUpdateErrorV1::InternalInvariant(_) => {
            ProgramAttachmentUpdateErrorV1::InternalInvariant
        }
    }
}

fn map_sink_error(
    error: crate::program::attachment::handoff::HandoffPointSinkErrorV1,
) -> ProgramPointSinkErrorV1 {
    use crate::program::attachment::handoff::HandoffPointSinkErrorV1 as Error;
    match error {
        Error::PatchScopeMismatch => ProgramPointSinkErrorV1::PatchScopeMismatch,
        Error::StampMismatch => ProgramPointSinkErrorV1::StampMismatch,
        Error::RevisionMismatch => ProgramPointSinkErrorV1::RevisionMismatch,
        Error::AlreadyInstalled => ProgramPointSinkErrorV1::AlreadyInstalled,
        Error::Host(error) => ProgramPointSinkErrorV1::Host(match error {
            crate::program::attachment::handoff::HandoffPointSinkHostErrorV1::Rejected => {
                ProgramPointSinkHostErrorV1::Rejected
            }
            crate::program::attachment::handoff::HandoffPointSinkHostErrorV1::Protocol => {
                ProgramPointSinkHostErrorV1::Protocol
            }
        }),
    }
}

fn attachment_snapshot_from_commit(
    commit: crate::program::attachment::AttachmentCommitV1<
        '_,
        crate::program::attachment::handoff::HandoffPointSinkOutputIdV1,
    >,
    outputs_scratch: &mut Vec<ProgramPaintOutputV1>,
) -> Result<ProgramAttachedSnapshotV1, ProgramAttachmentUpdateErrorV1> {
    let snapshot = snapshot_from_evidence_into(
        commit.evidence(),
        outputs_scratch,
        commit.snapshot_version(),
    )
    .map_err(|error| match error {
        ProgramRuntimeErrorV1::ResourceExhausted => {
            ProgramAttachmentUpdateErrorV1::ResourceExhausted
        }
        _ => ProgramAttachmentUpdateErrorV1::InternalInvariant,
    })?;
    let render = commit.render_outputs().next().map(|render| {
        let mut terminal_composite = None;
        let mut saw_matching_certificate = false;
        let mut saw_empty_domain = false;
        let mut saw_conflicting_composite = false;
        for domain in render.evidence().point_domains() {
            if domain.presentation_root().value() != render.root().value()
                || domain.target().value() != render.occurrence().value()
            {
                continue;
            }
            saw_matching_certificate = true;
            match domain.domain() {
                crate::appearance::ExactFinalOwnedPointDomainV1::Empty => {
                    saw_empty_domain = true;
                }
                crate::appearance::ExactFinalOwnedPointDomainV1::Singleton { visible } => {
                    let observed = Srgb8::new(visible);
                    if terminal_composite.is_some_and(|current| current != observed) {
                        saw_conflicting_composite = true;
                    } else if terminal_composite.is_none() {
                        terminal_composite = Some(observed);
                    }
                }
            }
        }
        let terminal_composite_ambiguous =
            saw_matching_certificate && !saw_empty_domain && saw_conflicting_composite;
        if !saw_matching_certificate || saw_empty_domain || terminal_composite_ambiguous {
            terminal_composite = None;
        }
        let paint = render.paint();
        let published = render.published_stamp();
        ProgramAttachedRenderV1 {
            output: ProgramPaintOutputV1 {
                slot: render.output().value(),
                source: paint.source(),
                opacity: f64::from_bits(paint.opacity_bits()),
            },
            sink_output: render.sink_output().value(),
            revision: published.revision(),
            sink_stamp: ProgramPointSinkStampV1::new(
                published.sink_stamp().sequence(),
                published.sink_stamp().binding_epoch().value().get(),
            ),
            content_identity: *render.evidence().content_identity().as_bytes(),
            presentation_root: render.root().value(),
            occurrence: render.occurrence().value(),
            context: ProgramAppearanceContextIdV1::from_core(render.context()),
            // Общий Program сохраняет единственный composition profile:
            // EncodedSrgb8SourceOverV1. Это профиль математики, не утверждение
            // о topology, числе слоёв или наблюдении реального renderer-а.
            physical_identity: Some(ProgramPhysicalIdentityV1::EncodedSrgb8SourceOverV1),
            terminal_composite,
            terminal_composite_ambiguous,
        }
    });
    Ok(ProgramAttachedSnapshotV1 { snapshot, render })
}

/// Typed-отказ публичного runtime-сеама. Payload внутренних вариантов не
/// раскрывается преждевременно; enum non_exhaustive для эволюции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramRuntimeErrorV1 {
    Wire,
    Compile,
    /// Распознанное сочетание возможностей пока не поддержано владельцем Program.
    Unsupported,
    FamilyArtifactsRequired,
    Instantiate,
    Update,
    /// Недостаточно ресурсов для компиляции, создания Session или update.
    ResourceExhausted,
    /// Нарушен внутренний инвариант, а не контракт пользовательского ввода.
    InternalInvariant,
}

/// Компилирует канонические Program wire bytes в immutable owner.
///
/// Family-графы в первом публичном runtime-срезе отклоняются: доверие к family
/// artifact обеспечивает вызывающий, а public trust-параметр будет отдельной
/// версией seam, не silent assumption.
pub fn compile_program_wire_v1(bytes: &[u8]) -> Result<CompiledProgramV1, ProgramRuntimeErrorV1> {
    compile_draft(decode_program_wire_v1(bytes))
}

/// Компилирует поддерживаемую wire-версию в существующий immutable owner.
///
/// V2 задаёт полный порядок конечных кандидатов, V3 — области и предпочтение
/// вычисляемого назначения. Общая Session проверяет ограничения и фактический
/// результат; остановленный поиск V3 не публикует промежуточный кандидат.
///
/// # Errors
/// Возвращает типизированные отказы формата, графа, ресурсов, family admission
/// либо распознанного неподдержанного сочетания возможностей.
pub fn compile_program_wire(bytes: &[u8]) -> Result<CompiledProgramV1, ProgramRuntimeErrorV1> {
    compile_draft(decode_program_wire(bytes))
}

fn compile_draft(
    decoded: Result<crate::program::DraftV1, ProgramWireErrorV1>,
) -> Result<CompiledProgramV1, ProgramRuntimeErrorV1> {
    let draft = decoded.map_err(|error| match error {
        ProgramWireErrorV1::AssignmentResourceExhausted { .. } => {
            ProgramRuntimeErrorV1::ResourceExhausted
        }
        _ => ProgramRuntimeErrorV1::Wire,
    })?;
    let owner = draft.compile().map_err(|error| match error {
        crate::program::CompileErrorV1::Unsupported => ProgramRuntimeErrorV1::Unsupported,
        crate::program::CompileErrorV1::ResourceExhausted => {
            ProgramRuntimeErrorV1::ResourceExhausted
        }
        crate::program::CompileErrorV1::InternalInvariant => {
            ProgramRuntimeErrorV1::InternalInvariant
        }
        _ => ProgramRuntimeErrorV1::Compile,
    })?;
    if owner.required_family_releases().next().is_some() {
        return Err(ProgramRuntimeErrorV1::FamilyArtifactsRequired);
    }
    Ok(CompiledProgramV1 { owner })
}

impl CompiledProgramV1 {
    /// Content identity immutable графа.
    #[must_use]
    pub fn content_identity(&self) -> [u8; 32] {
        *self.owner.content_identity().as_bytes()
    }

    /// Consuming instantiate: owner и session переходят одному runtime.
    pub fn instantiate(self, stream_id: u32) -> Result<ProgramSessionV1, ProgramRuntimeErrorV1> {
        let session = self
            .owner
            .instantiate(stream_id)
            .map_err(|error| match error {
                crate::program::InstantiateErrorV1::ResourceExhausted => {
                    ProgramRuntimeErrorV1::ResourceExhausted
                }
                crate::program::InstantiateErrorV1::InternalInvariant => {
                    ProgramRuntimeErrorV1::InternalInvariant
                }
                _ => ProgramRuntimeErrorV1::Instantiate,
            })?;
        Ok(ProgramSessionV1 {
            owner: self.owner,
            session,
            outputs_scratch: Vec::new(),
        })
    }
}

impl ProgramSessionV1 {
    /// Атомарно применяет observed update и возвращает owned snapshot.
    ///
    /// Подготовленный переход либо commit'ится целиком, либо при любом отказе
    /// Session сохраняет предыдущие head/lifecycle/evidence — закон внутренней
    /// `PreparedSessionTransitionV1` не ослабляется публичной обёрткой.
    pub fn update_observed(
        &mut self,
        revision: u64,
        scenarios: &[ProgramScenarioV1],
    ) -> Result<ProgramSnapshotV1, ProgramRuntimeErrorV1> {
        self.outputs_scratch
            .try_reserve_exact(self.owner.output_slots().count())
            .map_err(|_| ProgramRuntimeErrorV1::ResourceExhausted)?;
        let source = ProgramScenarioSourceV1(scenarios);
        let transition = self
            .owner
            .prepare_schema_ordered_update(&mut self.session, revision, &source)
            .map_err(map_runtime_update_error)?;
        let evidence = transition.commit();
        snapshot_from_evidence_into(
            evidence,
            &mut self.outputs_scratch,
            self.owner.snapshot_version(),
        )
    }

    /// Атомарно применяет Unknown update с непрозрачной причиной.
    pub fn update_unknown(
        &mut self,
        revision: u64,
        reason_id: u32,
    ) -> Result<ProgramSnapshotV1, ProgramRuntimeErrorV1> {
        self.outputs_scratch
            .try_reserve_exact(self.owner.output_slots().count())
            .map_err(|_| ProgramRuntimeErrorV1::ResourceExhausted)?;
        let transition = self
            .owner
            .prepare_update(
                &mut self.session,
                crate::program::UpdateV1::Unknown {
                    revision,
                    reason_id,
                },
            )
            .map_err(map_runtime_update_error)?;
        snapshot_from_evidence_into(
            transition.commit(),
            &mut self.outputs_scratch,
            self.owner.snapshot_version(),
        )
    }
}

fn map_runtime_update_error(error: crate::program::UpdateErrorV1) -> ProgramRuntimeErrorV1 {
    match error {
        crate::program::UpdateErrorV1::ResourceExhausted { .. } => {
            ProgramRuntimeErrorV1::ResourceExhausted
        }
        crate::program::UpdateErrorV1::InternalInvariant { .. } => {
            ProgramRuntimeErrorV1::InternalInvariant
        }
        _ => ProgramRuntimeErrorV1::Update,
    }
}

struct ProgramScenarioSourceV1<'a>(&'a [ProgramScenarioV1]);

impl SchemaOrderedScenarioSourceV1 for ProgramScenarioSourceV1<'_> {
    fn scenario_count(&self) -> usize {
        self.0.len()
    }

    fn scenario_id(&self, scenario_index: usize) -> ScenarioId {
        ScenarioId::new(self.0[scenario_index].id)
    }

    fn value_count(&self, scenario_index: usize) -> usize {
        self.0[scenario_index].surfaces.len()
    }

    fn value(&self, scenario_index: usize, binding_index: usize) -> Srgb8 {
        self.0[scenario_index].surfaces[binding_index]
    }
}

fn snapshot_from_evidence_into(
    evidence: crate::program::EvidenceViewV1<'_>,
    outputs_scratch: &mut Vec<ProgramPaintOutputV1>,
    snapshot_version: u32,
) -> Result<ProgramSnapshotV1, ProgramRuntimeErrorV1> {
    use crate::program::{CertificateV1, ObservationHeadV1, StateKindV1};
    let revision = match evidence.observation_head() {
        ObservationHeadV1::Observed { revision, .. }
        | ObservationHeadV1::Unknown { revision, .. } => revision,
        ObservationHeadV1::Empty => return Err(ProgramRuntimeErrorV1::InternalInvariant),
    };
    let state = match evidence.kind() {
        StateKindV1::Waiting => ProgramSnapshotStateV1::Waiting,
        StateKindV1::Ready => ProgramSnapshotStateV1::Ready,
        StateKindV1::Stale => ProgramSnapshotStateV1::Stale,
        StateKindV1::Failed => ProgramSnapshotStateV1::Failed,
        StateKindV1::Unsatisfied => ProgramSnapshotStateV1::Unsatisfied,
        StateKindV1::Incomplete => ProgramSnapshotStateV1::Incomplete,
    };
    outputs_scratch.clear();
    match snapshot_version {
        1 => {
            if let Some(verified) =
                evidence
                    .certificates()
                    .find_map(|certificate| match certificate {
                        CertificateV1::Verified(verified) => Some(verified),
                        CertificateV1::Conflict(_) => None,
                    })
            {
                copy_snapshot_outputs(verified.outputs(), outputs_scratch)?;
            }
        }
        2 => {
            if let Some(verified) = evidence.current_verified() {
                copy_snapshot_outputs(verified.outputs(), outputs_scratch)?;
            }
        }
        _ => return Err(ProgramRuntimeErrorV1::InternalInvariant),
    }
    Ok(ProgramSnapshotV1 {
        snapshot_version,
        revision,
        state,
        outputs: std::mem::take(outputs_scratch),
    })
}

fn copy_snapshot_outputs<'a>(
    outputs: impl ExactSizeIterator<Item = crate::program::CertifiedPaintOutputV1<'a>>,
    destination: &mut Vec<ProgramPaintOutputV1>,
) -> Result<(), ProgramRuntimeErrorV1> {
    destination
        .try_reserve_exact(outputs.len())
        .map_err(|_| ProgramRuntimeErrorV1::ResourceExhausted)?;
    destination.extend(outputs.map(|output| ProgramPaintOutputV1 {
        slot: output.output_slot().value(),
        source: output.source(),
        opacity: output.opacity(),
    }));
    Ok(())
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    use crate::program::wire::ProgramWireBuilderV1;

    fn runtime_wire(expected: crate::Srgb8) -> Vec<u8> {
        let mut builder = ProgramWireBuilderV1::new();
        builder
            .source(1, crate::Srgb8::new([0x40, 0x40, 0x40]))
            .fixed_target(2, 1)
            .surface_input_port(6)
            .opacity_input(5, 0.5)
            .solid_paint(3, 2)
            .opacity_paint(4, 3, 5)
            .input_surface(7, 6)
            .source_over_occurrence(8, 4, 7, 64.0, 0.2, 2)
            .presentation_root(9, 8)
            .presentation_target(9, 8)
            .exact_visible_unary(true, 10, 8, expected)
            .output(17, 4);
        builder.finish().unwrap()
    }

    fn computed_runtime_wire() -> Vec<u8> {
        computed_runtime_wire_for_domain(false)
    }

    fn computed_runtime_wire_for_domain(full_cube: bool) -> Vec<u8> {
        let mut builder = ProgramWireBuilderV1::new();
        if full_cube {
            builder.computed_box_target(2, [0; 3], [255; 3], Srgb8::new([64; 3]), 1.0);
        } else {
            builder.computed_gray_target(2, 0, 255, Srgb8::new([64; 3]), 1.0);
        }
        builder
            .surface_input_port(6)
            .opacity_input(5, 0.5)
            .solid_paint(3, 2)
            .opacity_paint(4, 3, 5)
            .input_surface(7, 6)
            .source_over_occurrence(8, 4, 7, 64.0, 0.2, 2)
            .presentation_root(9, 8)
            .presentation_target(9, 8)
            .exact_visible_unary(true, 10, 8, Srgb8::new([96; 3]))
            .output(17, 4);
        builder
            .finish_assignment(&[2])
            .expect("wire декларации кодируется")
    }

    #[test]
    fn computed_snapshot_keeps_current_output_separate_from_legacy_history() {
        let mut session = compile_program_wire(&computed_runtime_wire())
            .expect("V3 проходит обычный публичный вход")
            .instantiate(100)
            .expect("Session создаётся с допустимым stream");
        let scenarios = |gray| [ProgramScenarioV1::new(7, vec![Srgb8::new([gray; 3])])];
        let ready = session
            .update_observed(1, &scenarios(128))
            .expect("наблюдение принимается");
        assert_eq!(ready.snapshot_version(), 2);
        assert_eq!(ready.revision(), 1);
        assert_eq!(ready.reason(), None);
        assert_eq!(ready.state().key(), "ready");
        assert_eq!(ready.outputs()[0].source(), Srgb8::new([64; 3]));

        let stale = session
            .update_unknown(2, 41)
            .expect("неизвестное наблюдение принимается");
        assert_eq!(stale.state().key(), "stale");
        assert_eq!(stale.snapshot_version(), 2);
        assert_eq!(stale.revision(), 2);
        assert_eq!(stale.reason(), None);
        assert!(stale.outputs().is_empty());
        let unsatisfied = session
            .update_observed(3, &scenarios(255))
            .expect("наблюдение принимается");
        assert_eq!(unsatisfied.state().key(), "unsatisfied");
        assert_eq!(unsatisfied.revision(), 3);
        assert_eq!(unsatisfied.reason(), None);
        assert!(unsatisfied.outputs().is_empty());
        let recovered = session
            .update_observed(4, &scenarios(0))
            .expect("наблюдение принимается");
        assert_eq!(recovered.state().key(), "ready");
        assert_eq!(recovered.outputs()[0].source(), Srgb8::new([191; 3]));
        assert_eq!(ready.outputs()[0].source(), Srgb8::new([64; 3]));

        let mut legacy = compile_program_wire_v1(&runtime_wire(Srgb8::new([96; 3])))
            .expect("V1 Program компилируется")
            .instantiate(101)
            .expect("Session создаётся с допустимым stream");
        let _ = legacy
            .update_observed(1, &scenarios(128))
            .expect("наблюдение принимается");
        let historical = legacy
            .update_unknown(2, 41)
            .expect("неизвестное наблюдение принимается");
        assert_eq!(historical.state().key(), "stale");
        assert_eq!(historical.snapshot_version(), 1);
        assert_eq!(historical.revision(), 2);
        assert_eq!(historical.reason(), None);
        assert_eq!(historical.outputs()[0].source(), Srgb8::new([64; 3]));
    }

    #[test]
    fn computed_snapshot_unknown_reason_is_not_search_limit() {
        let mut session = compile_program_wire(&computed_runtime_wire())
            .expect("допустимый Program компилируется")
            .instantiate(100)
            .expect("Session создаётся с допустимым stream");
        let waiting = session
            .update_unknown(1, u32::MAX)
            .expect("неизвестное наблюдение принимается");
        assert_eq!(waiting.snapshot_version(), 2);
        assert_eq!(waiting.revision(), 1);
        assert_eq!(waiting.state(), ProgramSnapshotStateV1::Waiting);
        assert_eq!(waiting.reason(), None);
        assert!(waiting.outputs().is_empty());
    }

    #[test]
    fn v3_admission_exhaustion_preserves_old_wire_error_classification() {
        for version in [1_u16, 2, 3] {
            let mut bytes = b"LCPW".to_vec();
            bytes.extend_from_slice(&version.to_le_bytes());
            bytes.extend_from_slice(&14_u32.to_le_bytes());
            bytes.extend_from_slice(&4097_u32.to_le_bytes());
            if version == 3 {
                assert_eq!(
                    compile_program_wire(&bytes).err(),
                    Some(ProgramRuntimeErrorV1::ResourceExhausted)
                );
                assert_eq!(
                    check_program_wire(&bytes),
                    Err(ProgramWireCheckErrorV1::ResourceExhausted)
                );
            } else {
                assert_eq!(
                    compile_program_wire(&bytes).err(),
                    Some(ProgramRuntimeErrorV1::Wire)
                );
                assert_eq!(
                    check_program_wire(&bytes),
                    Err(ProgramWireCheckErrorV1::Wire {
                        section: "sources",
                        offset: 0
                    })
                );
            }
        }
    }

    #[test]
    fn computed_attachment_revokes_stopped_outputs_and_preserves_rejected_head() {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;

        struct Host {
            reject: Rc<Cell<bool>>,
            installed: Rc<RefCell<Vec<ProgramPointSinkIntentV1>>>,
        }

        impl ProgramPointSinkHostV1 for Host {
            fn try_install(
                &mut self,
                intent: ProgramPointSinkIntentV1,
            ) -> Result<(), ProgramPointSinkHostErrorV1> {
                if self.reject.get()
                    || self
                        .installed
                        .borrow()
                        .last()
                        .is_some_and(|previous| previous.desired_stamp() != intent.expected_stamp())
                {
                    return Err(ProgramPointSinkHostErrorV1::Rejected);
                }
                self.installed.borrow_mut().push(intent);
                Ok(())
            }
        }

        for (full_cube, stopped_state, reason) in [
            (false, ProgramSnapshotStateV1::Unsatisfied, None),
            (true, ProgramSnapshotStateV1::Incomplete, Some("limit")),
        ] {
            let installed = Rc::new(RefCell::new(Vec::new()));
            let reject = Rc::new(Cell::new(false));
            let compiled = compile_program_wire(&computed_runtime_wire_for_domain(full_cube))
                .expect("допустимый Program компилируется");
            let identity = compiled.content_identity();
            let mut attachment = compiled
                .attach(
                    100,
                    17,
                    91,
                    9,
                    8,
                    Host {
                        reject: Rc::clone(&reject),
                        installed: Rc::clone(&installed),
                    },
                )
                .expect("подготовка тестового сценария успешна");
            let scenarios = |gray| [ProgramScenarioV1::new(7, vec![Srgb8::new([gray; 3])])];
            let first = attachment
                .update_observed(1, &scenarios(128))
                .expect("наблюдение принимается");
            assert_eq!(first.snapshot().snapshot_version(), 2);
            assert_eq!(first.snapshot().revision(), 1);
            assert_eq!(first.snapshot().reason(), None);
            assert_eq!(
                first
                    .render()
                    .expect("готовый результат содержит render")
                    .content_identity(),
                identity
            );
            assert_eq!(
                attachment
                    .current_materialization_authority()
                    .expect("готовый результат имеет authority")
                    .revision(),
                1
            );

            reject.set(true);
            assert!(matches!(
                attachment.update_observed(2, &scenarios(255)),
                Err(ProgramAttachmentUpdateErrorV1::Sink(
                    ProgramPointSinkErrorV1::Host(ProgramPointSinkHostErrorV1::Rejected)
                ))
            ));
            assert_eq!(installed.borrow().len(), 1);
            assert_eq!(
                attachment
                    .current_materialization_authority()
                    .expect("готовый результат имеет authority")
                    .revision(),
                1
            );
            reject.set(false);

            let stopped = attachment
                .update_observed(2, &scenarios(255))
                .expect("наблюдение принимается");
            assert_eq!(stopped.snapshot().snapshot_version(), 2);
            assert_eq!(stopped.snapshot().revision(), 2);
            assert_eq!(stopped.snapshot().state(), stopped_state);
            assert_eq!(stopped.snapshot().reason(), reason);
            assert!(stopped.snapshot().outputs().is_empty());
            assert!(stopped.render().is_none());
            assert!(attachment.current_render().is_none());
            assert_eq!(
                attachment.current_materialization_authority(),
                Err(ProgramMaterializationAuthorityErrorV1::NotReady)
            );
            assert_eq!(
                installed.borrow()[1].operation(),
                ProgramPointSinkOperationV1::RevokeAll
            );
            assert!(installed.borrow()[1].point().is_none());

            let replay = attachment
                .update_observed(2, &scenarios(255))
                .expect("наблюдение принимается");
            assert_eq!(replay, stopped);
            assert_eq!(
                installed.borrow()[2].operation(),
                ProgramPointSinkOperationV1::ConfirmExact
            );
            assert!(installed.borrow()[2].point().is_none());
            assert!(attachment.update_observed(1, &scenarios(128)).is_err());
            assert!(attachment.update_observed(2, &scenarios(0)).is_err());
            assert_eq!(installed.borrow().len(), 3);

            let recovered = attachment
                .update_observed(3, &scenarios(128))
                .expect("наблюдение принимается");
            assert_eq!(recovered.snapshot().state(), ProgramSnapshotStateV1::Ready);
            assert_eq!(recovered.snapshot().revision(), 3);
            assert_eq!(recovered.snapshot().reason(), None);
            assert_eq!(
                recovered.snapshot().outputs()[0].source(),
                Srgb8::new([64; 3])
            );
            assert_eq!(
                installed.borrow()[3].operation(),
                ProgramPointSinkOperationV1::SetAll
            );
            assert_eq!(first.snapshot().revision(), 1);
            assert_eq!(first.snapshot().outputs()[0].source(), Srgb8::new([64; 3]));
        }
    }

    #[test]
    fn wire_runtime_compiles_instantiates_updates_and_returns_certified_outputs() {
        // source #404040 at 0.5 over backdrop #808080 -> encoded source-over #606060.
        let compiled =
            compile_program_wire_v1(&runtime_wire(crate::Srgb8::new([0x60, 0x60, 0x60]))).unwrap();
        let identity = compiled.content_identity();
        assert_ne!(identity, [0; 32]);
        let mut session = compiled.instantiate(100).unwrap();
        let snapshot = session
            .update_observed(
                1,
                &[ProgramScenarioV1::new(
                    7,
                    vec![crate::Srgb8::new([0x80, 0x80, 0x80])],
                )],
            )
            .unwrap();
        assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
        assert_eq!(snapshot.outputs().len(), 1);
        assert_eq!(snapshot.outputs()[0].slot(), 17);
        assert_eq!(
            snapshot.outputs()[0].source(),
            crate::Srgb8::new([0x40, 0x40, 0x40])
        );
        assert_eq!(snapshot.outputs()[0].opacity().to_bits(), 0.5_f64.to_bits());
    }

    #[test]
    fn public_session_preserves_resource_failure_and_retries_same_revision() {
        let compiled =
            compile_program_wire_v1(&runtime_wire(crate::Srgb8::new([0x60; 3]))).unwrap();
        let mut session = compiled.instantiate(100).unwrap();
        let scenarios = [ProgramScenarioV1::new(
            7,
            vec![crate::Srgb8::new([0x80; 3])],
        )];
        {
            let _failure = crate::program_session::fail_program_preflight_reservation_for_test(0);
            assert!(matches!(
                session.update_observed(1, &scenarios),
                Err(ProgramRuntimeErrorV1::ResourceExhausted)
            ));
            assert_eq!(
                crate::program_session::program_preflight_failure_remaining_for_test(),
                None
            );
        }
        let retry = session.update_observed(1, &scenarios).unwrap();
        assert_eq!(retry.state(), ProgramSnapshotStateV1::Ready);
        assert_eq!(retry.outputs()[0].source(), crate::Srgb8::new([0x40; 3]));
        assert_eq!(retry.outputs()[0].opacity(), 0.5);
    }

    #[test]
    fn update_projection_retains_infrastructure_class_and_semantic_refusals() {
        use crate::program::{UpdateErrorV1, UpdateInvariantFailureV1, UpdatePhaseV1};
        use crate::program_session::PointOutputPresentationBindErrorV1 as BindError;
        assert_eq!(
            map_attachment_binding_error(BindError::InternalInvariant),
            ProgramAttachErrorV1::InternalInvariant
        );
        assert_eq!(
            map_attachment_binding_error(BindError::MissingOutput {
                output: crate::program_session::OutputSlotId::new(1),
            }),
            ProgramAttachErrorV1::Binding
        );
        assert_eq!(
            map_attachment_binding_error(BindError::NonTerminalTarget {
                root: crate::program_session::PresentationRootId::new(1),
                terminal: crate::appearance::OccurrenceId::new(2),
                occurrence: crate::appearance::OccurrenceId::new(3),
            }),
            ProgramAttachErrorV1::NonTerminalTarget
        );
        for phase in [
            UpdatePhaseV1::ObservationAdmission,
            UpdatePhaseV1::ProgramEvaluation,
        ] {
            assert_eq!(
                map_runtime_update_error(UpdateErrorV1::ResourceExhausted { phase }),
                ProgramRuntimeErrorV1::ResourceExhausted
            );
            assert_eq!(
                map_attachment_update_error(
                    crate::program::attachment::AttachmentUpdateErrorV1::Update(
                        UpdateErrorV1::ResourceExhausted { phase }
                    )
                ),
                ProgramAttachmentUpdateErrorV1::ResourceExhausted
            );
        }
        let invariant = || UpdateErrorV1::InternalInvariant {
            source: UpdateInvariantFailureV1::OwnerAuthority,
        };
        assert_eq!(
            map_runtime_update_error(invariant()),
            ProgramRuntimeErrorV1::InternalInvariant
        );
        assert_eq!(
            map_attachment_update_error(
                crate::program::attachment::AttachmentUpdateErrorV1::Update(invariant())
            ),
            ProgramAttachmentUpdateErrorV1::InternalInvariant
        );
        assert_eq!(
            map_runtime_update_error(UpdateErrorV1::EmptyScenarioSet),
            ProgramRuntimeErrorV1::Update
        );
        assert_eq!(
            map_attachment_update_error(
                crate::program::attachment::AttachmentUpdateErrorV1::Update(
                    UpdateErrorV1::EmptyScenarioSet
                )
            ),
            ProgramAttachmentUpdateErrorV1::Update
        );
    }

    #[test]
    fn update_failure_preserves_previous_ready_snapshot() {
        let compiled =
            compile_program_wire_v1(&runtime_wire(crate::Srgb8::new([0x60, 0x60, 0x60]))).unwrap();
        let mut session = compiled.instantiate(100).unwrap();
        let first = session
            .update_observed(
                1,
                &[ProgramScenarioV1::new(
                    7,
                    vec![crate::Srgb8::new([0x80, 0x80, 0x80])],
                )],
            )
            .unwrap();
        assert_eq!(first.state(), ProgramSnapshotStateV1::Ready);
        let refused = session.update_observed(2, &[]);
        assert!(matches!(refused, Err(ProgramRuntimeErrorV1::Update)));
        // Следующий валидный update должен продолжить ту же Session и снова Ready.
        let second = session
            .update_observed(
                2,
                &[ProgramScenarioV1::new(
                    7,
                    vec![crate::Srgb8::new([0x80, 0x80, 0x80])],
                )],
            )
            .unwrap();
        assert_eq!(second.state(), ProgramSnapshotStateV1::Ready);
    }

    #[test]
    fn detached_snapshots_remain_readable_after_later_reuse_and_session_drop() {
        let compiled =
            compile_program_wire_v1(&runtime_wire(crate::Srgb8::new([0x60, 0x60, 0x60]))).unwrap();
        let mut session = compiled.instantiate(100).unwrap();
        let backdrop =
            |id, code| ProgramScenarioV1::new(id, vec![crate::Srgb8::new([code, code, code])]);

        let first = session.update_observed(1, &[backdrop(1, 0x80)]).unwrap();
        let second = session.update_observed(2, &[backdrop(2, 0x80)]).unwrap();
        for revision in 3..=7 {
            session
                .update_observed(revision, &[backdrop(revision as u32, 0x80)])
                .expect("later updates must reuse internal arena slots");
        }
        drop(session);

        for snapshot in [&first, &second] {
            assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
            assert_eq!(snapshot.outputs().len(), 1);
            assert_eq!(snapshot.outputs()[0].slot(), 17);
            assert_eq!(snapshot.outputs()[0].source(), crate::Srgb8::new([0x40; 3]));
            assert_eq!(snapshot.outputs()[0].opacity().to_bits(), 0.5_f64.to_bits());
        }
    }

    #[test]
    fn family_graphs_fail_closed_until_a_public_trust_parameter_exists() {
        let mut builder = ProgramWireBuilderV1::new();
        builder.family(1, [7; 32]);
        let bytes = builder.finish().unwrap();
        assert!(matches!(
            compile_program_wire_v1(&bytes),
            Err(ProgramRuntimeErrorV1::Compile | ProgramRuntimeErrorV1::FamilyArtifactsRequired)
        ));
    }
}

impl ProgramSnapshotStateV1 {
    /// Стабильное wire-имя состояния. Новая ветвь требует явной проекции у владельца.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Ready => "ready",
            Self::Stale => "stale",
            Self::Failed => "failed",
            Self::Unsatisfied => "unsatisfied",
            Self::Incomplete => "incomplete",
        }
    }
}

#[cfg(test)]
mod snapshot_state_contract {
    use super::ProgramSnapshotStateV1;

    #[test]
    fn wire_state_projection_preserves_every_lifecycle_state() {
        use ProgramSnapshotStateV1::*;
        assert_eq!(
            [Waiting, Ready, Stale, Failed, Unsatisfied, Incomplete]
                .map(ProgramSnapshotStateV1::key),
            [
                "waiting",
                "ready",
                "stale",
                "failed",
                "unsatisfied",
                "incomplete"
            ]
        );
    }
}
