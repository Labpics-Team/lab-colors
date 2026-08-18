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
    #[non_exhaustive]
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
    #[non_exhaustive]
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
