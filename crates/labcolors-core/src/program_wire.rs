//! Публичная проверка канонических wire-байтов Program (v1).
//!
//! Первый и единственный публичный seam Program до terminal C7c: клиент может
//! доказать, что его байты канонны и компилируемы, и получить content identity
//! графа — но НЕ может получить runtime (Owner/Session/attachment остаются
//! приватными). Полный authoring/emission контракт публикует атомарный C7c.
//!
//! Отказы двухслойны и типизированы: [`ProgramWireCheckErrorV1::Wire`] — байты
//! нарушают канон формата; [`ProgramWireCheckErrorV1::Compile`] — байты канонны,
//! но граф семантически невалиден. Ни один из слоёв не выражает другой.

use crate::program::wire::{ProgramWireErrorV1, decode_program_wire_v1};

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
        ProgramWireErrorV1::ResourceExhausted { section } => (
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
    let draft = decode_program_wire_v1(bytes).map_err(|error| {
        let (section, offset) = section_name(&error);
        ProgramWireCheckErrorV1::Wire { section, offset }
    })?;
    let owner = draft
        .compile()
        .map_err(|_| ProgramWireCheckErrorV1::Compile)?;
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

    /// Сертифицированная straight opacity в [0,1].
    #[must_use]
    pub const fn opacity(self) -> f64 {
        self.opacity
    }
}

/// Owned snapshot Session после атомарного update.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgramSnapshotV1 {
    state: ProgramSnapshotStateV1,
    outputs: Vec<ProgramPaintOutputV1>,
}

impl ProgramSnapshotV1 {
    #[must_use]
    pub const fn state(&self) -> ProgramSnapshotStateV1 {
        self.state
    }

    #[must_use]
    pub fn outputs(&self) -> &[ProgramPaintOutputV1] {
        &self.outputs
    }
}

/// Typed-отказ публичного runtime-сеама. Payload внутренних вариантов не
/// раскрывается преждевременно; enum non_exhaustive для эволюции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramRuntimeErrorV1 {
    Wire,
    Compile,
    FamilyArtifactsRequired,
    Instantiate,
    Update,
}

/// Компилирует канонические Program wire bytes в immutable owner.
///
/// Family-графы в первом публичном runtime-срезе отклоняются: доверие к family
/// artifact обеспечивает вызывающий, а public trust-параметр будет отдельной
/// версией seam, не silent assumption.
pub fn compile_program_wire_v1(bytes: &[u8]) -> Result<CompiledProgramV1, ProgramRuntimeErrorV1> {
    let draft = decode_program_wire_v1(bytes).map_err(|_| ProgramRuntimeErrorV1::Wire)?;
    let owner = draft
        .compile()
        .map_err(|_| ProgramRuntimeErrorV1::Compile)?;
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
            .map_err(|_| ProgramRuntimeErrorV1::Instantiate)?;
        Ok(ProgramSessionV1 {
            owner: self.owner,
            session,
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
        let admitted: Vec<crate::program::ScenarioV1<'_>> = scenarios
            .iter()
            .map(|scenario| crate::program::ScenarioV1::new(scenario.id, &scenario.surfaces))
            .collect();
        let transition = self
            .owner
            .prepare_update(
                &mut self.session,
                crate::program::UpdateV1::Observed {
                    revision,
                    scenarios: &admitted,
                },
            )
            .map_err(|_| ProgramRuntimeErrorV1::Update)?;
        let evidence = transition.commit();
        Ok(snapshot_from_evidence(evidence))
    }

    /// Атомарно применяет Unknown update с непрозрачной причиной.
    pub fn update_unknown(
        &mut self,
        revision: u64,
        reason_id: u32,
    ) -> Result<ProgramSnapshotV1, ProgramRuntimeErrorV1> {
        let transition = self
            .owner
            .prepare_update(
                &mut self.session,
                crate::program::UpdateV1::Unknown {
                    revision,
                    reason_id,
                },
            )
            .map_err(|_| ProgramRuntimeErrorV1::Update)?;
        Ok(snapshot_from_evidence(transition.commit()))
    }
}

fn snapshot_from_evidence(evidence: crate::program::EvidenceViewV1<'_>) -> ProgramSnapshotV1 {
    use crate::program::{CertificateV1, StateKindV1};
    let state = match evidence.kind() {
        StateKindV1::Waiting => ProgramSnapshotStateV1::Waiting,
        StateKindV1::Ready => ProgramSnapshotStateV1::Ready,
        StateKindV1::Stale => ProgramSnapshotStateV1::Stale,
        StateKindV1::Failed => ProgramSnapshotStateV1::Failed,
    };
    let outputs = evidence
        .certificates()
        .find_map(|certificate| match certificate {
            CertificateV1::Verified(verified) => Some(
                verified
                    .outputs()
                    .map(|output| ProgramPaintOutputV1 {
                        slot: output.output_slot().value(),
                        source: output.source(),
                        opacity: output.opacity(),
                    })
                    .collect(),
            ),
            CertificateV1::Conflict(_) => None,
        })
        .unwrap_or_default();
    ProgramSnapshotV1 { state, outputs }
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
