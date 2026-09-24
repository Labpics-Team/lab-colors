//! РџСѓР±Р»РёС‡РЅР°СЏ РїСЂРѕРІРµСЂРєР° РєР°РЅРѕРЅРёС‡РµСЃРєРёС… wire-Р±Р°Р№С‚РѕРІ Program (v1).
//!
//! РџРµСЂРІС‹Р№ Рё РµРґРёРЅСЃС‚РІРµРЅРЅС‹Р№ РїСѓР±Р»РёС‡РЅС‹Р№ seam Program РґРѕ terminal C7c: РєР»РёРµРЅС‚ РјРѕР¶РµС‚
//! РґРѕРєР°Р·Р°С‚СЊ, С‡С‚Рѕ РµРіРѕ Р±Р°Р№С‚С‹ РєР°РЅРѕРЅРЅС‹ Рё РєРѕРјРїРёР»РёСЂСѓРµРјС‹, Рё РїРѕР»СѓС‡РёС‚СЊ content identity
//! РіСЂР°С„Р° вЂ” РЅРѕ РќР• РјРѕР¶РµС‚ РїРѕР»СѓС‡РёС‚СЊ runtime (Owner/Session/attachment РѕСЃС‚Р°СЋС‚СЃСЏ
//! РїСЂРёРІР°С‚РЅС‹РјРё). РџРѕР»РЅС‹Р№ authoring/emission РєРѕРЅС‚СЂР°РєС‚ РїСѓР±Р»РёРєСѓРµС‚ Р°С‚РѕРјР°СЂРЅС‹Р№ C7c.
//!
//! РћС‚РєР°Р·С‹ РґРІСѓС…СЃР»РѕР№РЅС‹ Рё С‚РёРїРёР·РёСЂРѕРІР°РЅС‹: [`ProgramWireCheckErrorV1::Wire`] вЂ” Р±Р°Р№С‚С‹
//! РЅР°СЂСѓС€Р°СЋС‚ РєР°РЅРѕРЅ С„РѕСЂРјР°С‚Р°; [`ProgramWireCheckErrorV1::Compile`] вЂ” Р±Р°Р№С‚С‹ РєР°РЅРѕРЅРЅС‹,
//! РЅРѕ РіСЂР°С„ СЃРµРјР°РЅС‚РёС‡РµСЃРєРё РЅРµРІР°Р»РёРґРµРЅ. РќРё РѕРґРёРЅ РёР· СЃР»РѕС‘РІ РЅРµ РІС‹СЂР°Р¶Р°РµС‚ РґСЂСѓРіРѕР№.

use crate::Srgb8;
use crate::observation::{ScenarioId, SchemaOrderedScenarioSourceV1};
use crate::program::wire::{ProgramWireErrorV1, decode_program_wire_v1};

/// РРјСЏ wire-СЃРµРєС†РёРё РІ РїСѓР±Р»РёС‡РЅРѕР№ РґРёР°РіРЅРѕСЃС‚РёРєРµ.
///
/// РЎС‚СЂРѕРєРѕРІР°СЏ РїСЂРѕРµРєС†РёСЏ РЅР°РјРµСЂРµРЅРЅРѕ: РїСѓР±Р»РёС‡РЅС‹Р№ С‚РёРї РЅРµ С‚СЏРЅРµС‚ РІРЅСѓС‚СЂРµРЅРЅРёРµ enum'С‹
/// С„РѕСЂРјР°С‚Р°, Р° Р·Р°РєСЂС‹С‚С‹Р№ СЃР»РѕРІР°СЂСЊ РёРјС‘РЅ вЂ” РєРѕРЅС‚СЂР°РєС‚ РІРµСЂСЃРёРё v1.
pub type ProgramWireSectionNameV1 = &'static str;

/// РџСѓР±Р»РёС‡РЅС‹Р№ typed-РѕС‚РєР°Р· РїСЂРѕРІРµСЂРєРё wire-Р±Р°Р№С‚РѕРІ.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramWireCheckErrorV1 {
    /// Р‘Р°Р№С‚С‹ РЅР°СЂСѓС€Р°СЋС‚ РєР°РЅРѕРЅ С„РѕСЂРјР°С‚Р°: РЅРµРІР°Р»РёРґРЅС‹Р№ Р·Р°РіРѕР»РѕРІРѕРє, РґР»РёРЅР°, Р·Р°РїРёСЃСЊ РёР»Рё
    /// wire-limit. `section`/`offset` СѓРєР°Р·С‹РІР°СЋС‚ РЅР° РЅР°С‡Р°Р»Рѕ РЅР°СЂСѓС€РёРІС€РµР№ Р·Р°РїРёСЃРё;
    /// РґР»СЏ Р·Р°РіРѕР»РѕРІРѕС‡РЅС‹С… РѕС‚РєР°Р·РѕРІ СЃРµРєС†РёСЏ вЂ” `"header"`.
    Wire {
        /// РЎРµРєС†РёСЏ С„РѕСЂРјР°С‚Р°, РІ РєРѕС‚РѕСЂРѕР№ Р·Р°С„РёРєСЃРёСЂРѕРІР°РЅ РѕС‚РєР°Р·.
        section: ProgramWireSectionNameV1,
        /// РЎРјРµС‰РµРЅРёРµ РЅР°С‡Р°Р»Р° Р·Р°РїРёСЃРё РІ Р±Р°Р№С‚Р°С… (0 РґР»СЏ Р·Р°РіРѕР»РѕРІРѕС‡РЅС‹С… РѕС‚РєР°Р·РѕРІ).
        offset: usize,
    },
    /// Р‘Р°Р№С‚С‹ РєР°РЅРѕРЅРЅС‹, РЅРѕ РіСЂР°С„ РѕС‚РІРµСЂРіРЅСѓС‚ СЃРµРјР°РЅС‚РёС‡РµСЃРєРѕР№ РєРѕРјРїРёР»СЏС†РёРµР№.
    ///
    /// Р”РµС‚Р°Р»РёР·Р°С†РёСЏ РєР»Р°СЃСЃР° РЅР°РјРµСЂРµРЅРЅРѕ РЅРµ РїСѓР±Р»РёРєСѓРµС‚СЃСЏ РІ v1: РїРѕР»РЅС‹Р№ typed
    /// compile-РєРѕРЅС‚СЂР°РєС‚ РїСѓР±Р»РёРєСѓРµС‚ Р°С‚РѕРјР°СЂРЅС‹Р№ C7c; РїСЂРµР¶РґРµРІСЂРµРјРµРЅРЅР°СЏ СЃС‚СЂРѕРєРѕРІР°СЏ
    /// РїСЂРѕРµРєС†РёСЏ 62 РІРЅСѓС‚СЂРµРЅРЅРёС… РєР»Р°СЃСЃРѕРІ СЃС‚Р°Р»Р° Р±С‹ Hyrum-РєРѕРЅС‚СЂР°РєС‚РѕРј РґРѕ РЅРµРіРѕ.
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

/// РџСЂРѕРІРµСЂСЏРµС‚ РєР°РЅРѕРЅРёС‡РµСЃРєРёРµ wire-Р±Р°Р№С‚С‹ Рё РІРѕР·РІСЂР°С‰Р°РµС‚ content identity РіСЂР°С„Р°.
///
/// Identity вЂ” SHA-256 РєР°РЅРѕРЅРёС‡РµСЃРєРѕРіРѕ РїСЂРѕРѕР±СЂР°Р·Р° СЃРєРѕРјРїРёР»РёСЂРѕРІР°РЅРЅРѕРіРѕ СЃРѕРґРµСЂР¶Р°РЅРёСЏ
/// (РёРЅРІР°СЂРёР°РЅС‚РЅР° Рє РїРµСЂРµРёРјРµРЅРѕРІР°РЅРёСЋ РєР»РёРµРЅС‚СЃРєРёС… ID). РЈСЃРїРµС… РґРѕРєР°Р·С‹РІР°РµС‚: Р±Р°Р№С‚С‹
/// РєР°РЅРѕРЅРЅС‹, РіСЂР°С„ РєРѕРјРїРёР»РёСЂСѓРµРј, identity Р°РґСЂРµСЃСѓРµРјР° вЂ” РЅРёС‡РµРіРѕ Р±РѕР»СЊС€Рµ; РЅРёРєР°РєРѕР№
/// runtime-authority СЌС‚РѕС‚ РІС‹Р·РѕРІ РЅРµ РІС‹РґР°С‘С‚.
///
/// # Errors
///
/// [`ProgramWireCheckErrorV1::Wire`] вЂ” Р±Р°Р№С‚С‹ РЅР°СЂСѓС€Р°СЋС‚ РєР°РЅРѕРЅ С„РѕСЂРјР°С‚Р°;
/// [`ProgramWireCheckErrorV1::Compile`] вЂ” РіСЂР°С„ СЃРµРјР°РЅС‚РёС‡РµСЃРєРё РЅРµРІР°Р»РёРґРµРЅ.
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

    /// РЈСЃРїРµС… РІРѕР·РІСЂР°С‰Р°РµС‚ 32-Р±Р°Р№С‚РЅСѓСЋ identity, СЂР°РІРЅСѓСЋ identity РїСЂСЏРјРѕР№ РєРѕРјРїРёР»СЏС†РёРё.
    #[test]
    fn canonical_bytes_yield_the_compiled_content_identity() {
        let identity = check_program_wire_v1(&canonical_reference_bytes()).unwrap();
        assert_eq!(identity.len(), 32);
        assert_ne!(identity, [0; 32], "identity must be a real digest");
        // Determinism: РѕРґРЅРё Р±Р°Р№С‚С‹ вЂ” РѕРґРЅР° identity.
        assert_eq!(
            identity,
            check_program_wire_v1(&canonical_reference_bytes()).unwrap()
        );
    }

    /// Р‘Р°Р№С‚РѕРІС‹Р№ РґРµС„РµРєС‚ вЂ” Wire-РѕС‚РєР°Р· СЃ СЃРµРєС†РёРµР№; runtime РЅРµ РІС‹РґР°С‘С‚СЃСЏ.
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

    /// РЎРµРјР°РЅС‚РёС‡РµСЃРєРёР№ РґРµС„РµРєС‚ вЂ” Compile-РѕС‚РєР°Р· Р±РµР· Р±Р°Р№С‚РѕРІРѕР№ РґРёР°РіРЅРѕСЃС‚РёРєРё.
    #[test]
    fn semantic_defects_surface_as_compile_refusals() {
        // Paint -> dangling target: РєР°РЅРѕРЅРЅС‹Рµ Р±Р°Р№С‚С‹, РЅРµРІР°Р»РёРґРЅС‹Р№ РіСЂР°С„.
        let mut builder = ProgramWireBuilderV1::new();
        builder.solid_paint(41, 999).output(91, 41);
        let bytes = builder.finish().unwrap();
        assert!(matches!(
            check_program_wire_v1(&bytes),
            Err(ProgramWireCheckErrorV1::Compile)
        ));
    }

    /// РњР°РіРёСЏ Рё РІРµСЂСЃРёСЏ вЂ” РєРѕРЅС‚СЂР°РєС‚ С„РѕСЂРјР°С‚Р°: СЃРґРІРёРі Р»СЋР±РѕРіРѕ РёР· РЅРёС… Р»РѕРјР°РµС‚ РєР°РЅРѕРЅ.
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

    /// РњРёРіСЂР°С†РёРѕРЅРЅРѕРµ РґРѕРєР°Р·Р°С‚РµР»СЊСЃС‚РІРѕ (СЃСЂРµР· 6 wire-СѓР·Р»Р°): 11-СѓР·Р»РѕРІС‹Р№ РіСЂР°С„
    /// РїСЂРёРІР°С‚РЅРѕРіРѕ fixture ABI v2 РїРѕР»РЅРѕСЃС‚СЊСЋ РІС‹СЂР°Р¶Р°РµС‚СЃСЏ РџРЈР‘Р›РР§РќРћР™ wire-РіСЂР°РјРјР°С‚РёРєРѕР№
    /// Рё РґР°С‘С‚ Р¶РёРІСѓСЋ content identity С‡РµСЂРµР· РµРґРёРЅСЃС‚РІРµРЅРЅС‹Р№ РїСѓР±Р»РёС‡РЅС‹Р№ seam.
    ///
    /// Р­С‚Рѕ СѓСЃР»РѕРІРёРµ РІС…РѕРґР° РІ C7c: РїРѕСЃР»Рµ РїСѓР±Р»РёРєР°С†РёРё РїРѕР»РЅРѕРіРѕ РєРѕРЅС‚СЂР°РєС‚Р° fixture ABI
    /// РѕСЃС‚Р°С‘С‚СЃСЏ compat-СЃР»РѕРµРј, Р° РїСѓР±Р»РёС‡РЅР°СЏ РіСЂР°РјРјР°С‚РёРєР° СѓР¶Рµ СЃРµРіРѕРґРЅСЏ РїРѕРєСЂС‹РІР°РµС‚ РµРіРѕ
    /// С‚РѕРїРѕР»РѕРіРёСЋ (source -> fixed target -> solid paint -> opacity paint ->
    /// input surface -> source-over occurrence -> presentation root/target ->
    /// exact visible hard -> output). РРґРµРЅС‚РёС„РёРєР°С‚РѕСЂС‹ вЂ” С‚Рµ Р¶Рµ ordinals, С‡С‚Рѕ Рё РІ
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

        // РџСѓР±Р»РёС‡РЅС‹Р№ seam: Р±Р°Р№С‚С‹ РєР°РЅРѕРЅРЅС‹... РЅРѕ exact-constraint СЃ РїСЂРѕРёР·РІРѕР»СЊРЅС‹Рј
        // expected РґРѕ attachment РЅРµРІС‹РїРѕР»РЅРёРј вЂ” РєРѕРјРїРёР»СЏС†РёСЏ РіСЂР°С„Р° С‚РµРј РЅРµ РјРµРЅРµРµ
        // РѕР±СЏР·Р°РЅР° РїСЂРѕР№С‚Рё (constraint РёСЃРїРѕР»РЅСЏРµС‚СЃСЏ РІ runtime, РЅРµ РїСЂРё compile).
        let identity = check_program_wire_v1(&bytes).expect(
            "the fixture topology must be canonical and compilable through the public seam",
        );
        assert_ne!(identity, [0; 32]);

        // Determinism РїРѕРІРµСЂС… РїРѕР»РЅРѕРіРѕ fixture-РіСЂР°С„Р°.
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

/// РџРѕР»РЅРѕСЃС‚СЊСЋ СЃРєРѕРјРїРёР»РёСЂРѕРІР°РЅРЅС‹Р№ Program Р±РµР· runtime-authority.
///
/// Р’Р»Р°РґРµРµС‚ immutable РіСЂР°С„РѕРј Рё content identity; Session РїРѕСЏРІР»СЏРµС‚СЃСЏ С‚РѕР»СЊРєРѕ
/// С‡РµСЂРµР· consuming [`Self::instantiate`], РїРѕСЌС‚РѕРјСѓ РѕРґРёРЅ runtime РЅРµ РјРѕР¶РµС‚ РјРѕР»С‡Р°
/// СЂР°Р·РґРµР»РёС‚СЊ РІР»Р°РґРµР»СЊС†Р° СЃ РґСЂСѓРіРёРј.
pub struct CompiledProgramV1 {
    owner: crate::program::OwnerV1,
}

/// Р•РґРёРЅСЃС‚РІРµРЅРЅС‹Р№ runtime-РІР»Р°РґРµР»РµС† РѕРґРЅРѕР№ Session РїСѓР±Р»РёС‡РЅРѕРіРѕ Program.
pub struct ProgramSessionV1 {
    owner: crate::program::OwnerV1,
    session: crate::program::SessionV1,
    outputs_scratch: Vec<ProgramPaintOutputV1>,
}

/// РћРґРёРЅ СЃС†РµРЅР°СЂРёР№ РЅР°Р±Р»СЋРґР°РµРјРѕР№ СЃСЂРµРґС‹: РЅРµРїСЂРѕР·СЂР°С‡РЅС‹Р№ ID Рё Р·РЅР°С‡РµРЅРёСЏ surface inputs
/// РІ РєР°РЅРѕРЅРёС‡РµСЃРєРѕРј РїРѕСЂСЏРґРєРµ, РѕР±СЉСЏРІР»РµРЅРЅРѕРј Program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramScenarioV1 {
    id: u32,
    surfaces: Vec<crate::Srgb8>,
}

impl ProgramScenarioV1 {
    /// РЎРѕР·РґР°С‘С‚ owned-СЃС†РµРЅР°СЂРёР№; РєР°СЂРґРёРЅР°Р»СЊРЅРѕСЃС‚СЊ СЃРІРµСЂСЏРµС‚СЃСЏ СЃ Program РїСЂРё update.
    #[must_use]
    pub fn new(id: u32, surfaces: Vec<crate::Srgb8>) -> Self {
        Self { id, surfaces }
    }
}

/// Lifecycle-РєР»Р°СЃСЃ РѕРґРЅРѕРіРѕ РѕРїСѓР±Р»РёРєРѕРІР°РЅРЅРѕРіРѕ snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramSnapshotStateV1 {
    Waiting,
    Ready,
    Stale,
    Failed,
}

/// РћРґРёРЅ СЃРµСЂС‚РёС„РёС†РёСЂРѕРІР°РЅРЅС‹Р№ Paint output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgramPaintOutputV1 {
    slot: u32,
    source: crate::Srgb8,
    opacity: f64,
}

impl ProgramPaintOutputV1 {
    /// РќРµРїСЂРѕР·СЂР°С‡РЅС‹Р№ РєР»РёРµРЅС‚СЃРєРёР№ output slot.
    #[must_use]
    pub const fn slot(self) -> u32 {
        self.slot
    }

    /// РЎРµСЂС‚РёС„РёС†РёСЂРѕРІР°РЅРЅС‹Р№ encoded sRGB8 source.
    #[must_use]
    pub const fn source(self) -> crate::Srgb8 {
        self.source
    }

    /// РЎРµСЂС‚РёС„РёС†РёСЂРѕРІР°РЅРЅР°СЏ straight opacity РІ `0..=1`.
    #[must_use]
    pub const fn opacity(self) -> f64 {
        self.opacity
    }
}

/// Owned snapshot Session РїРѕСЃР»Рµ Р°С‚РѕРјР°СЂРЅРѕРіРѕ update.
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
            .map_err(|error| match error {
                crate::program_session::PointOutputPresentationBindErrorV1::NonTerminalTarget {
                    ..
                } => ProgramAttachErrorV1::NonTerminalTarget,
                _ => ProgramAttachErrorV1::Binding,
            })?;
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
            })?;
        Ok(ProgramAttachmentV1 {
            inner: Some(inner),
            output_count,
            outputs_scratch,
            current_render: None,
        })
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
        crate::program::attachment::AttachmentUpdateErrorV1::Update(_) => {
            ProgramAttachmentUpdateErrorV1::Update
        }
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
    let snapshot = snapshot_from_evidence_into(commit.evidence(), outputs_scratch)
        .map_err(|_| ProgramAttachmentUpdateErrorV1::ResourceExhausted)?;
    let render = commit.render_outputs().next().map(|render| {
        let mut terminal_composite = None;
        let mut saw_matching_certificate = false;
        let mut saw_empty_domain = false;
        let mut saw_conflicting_composite = false;
        for certificate in render
            .certificate()
            .point_causal_certificates()
            .filter(|certificate| {
                certificate.presentation_root().value() == render.root().value()
                    && certificate.target().value() == render.occurrence().value()
            })
        {
            saw_matching_certificate = true;
            match certificate.domain() {
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
            content_identity: *render.certificate().content_identity().as_bytes(),
            presentation_root: render.root().value(),
            occurrence: render.occurrence().value(),
            context: ProgramAppearanceContextIdV1::from_core(render.context()),
            // Closed Program V1 имеет единственный composition profile:
            // EncodedSrgb8SourceOverV1. Это профиль математики, не утверждение
            // о topology, числе слоёв или наблюдении реального renderer-а.
            physical_identity: Some(ProgramPhysicalIdentityV1::EncodedSrgb8SourceOverV1),
            terminal_composite,
            terminal_composite_ambiguous,
        }
    });
    Ok(ProgramAttachedSnapshotV1 { snapshot, render })
}

/// Typed-РѕС‚РєР°Р· РїСѓР±Р»РёС‡РЅРѕРіРѕ runtime-СЃРµР°РјР°. Payload РІРЅСѓС‚СЂРµРЅРЅРёС… РІР°СЂРёР°РЅС‚РѕРІ РЅРµ
/// СЂР°СЃРєСЂС‹РІР°РµС‚СЃСЏ РїСЂРµР¶РґРµРІСЂРµРјРµРЅРЅРѕ; enum non_exhaustive РґР»СЏ СЌРІРѕР»СЋС†РёРё.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramRuntimeErrorV1 {
    Wire,
    Compile,
    FamilyArtifactsRequired,
    Instantiate,
    Update,
}

/// РљРѕРјРїРёР»РёСЂСѓРµС‚ РєР°РЅРѕРЅРёС‡РµСЃРєРёРµ Program wire bytes РІ immutable owner.
///
/// Family-РіСЂР°С„С‹ РІ РїРµСЂРІРѕРј РїСѓР±Р»РёС‡РЅРѕРј runtime-СЃСЂРµР·Рµ РѕС‚РєР»РѕРЅСЏСЋС‚СЃСЏ: РґРѕРІРµСЂРёРµ Рє family
/// artifact РѕР±РµСЃРїРµС‡РёРІР°РµС‚ РІС‹Р·С‹РІР°СЋС‰РёР№, Р° public trust-РїР°СЂР°РјРµС‚СЂ Р±СѓРґРµС‚ РѕС‚РґРµР»СЊРЅРѕР№
/// РІРµСЂСЃРёРµР№ seam, РЅРµ silent assumption.
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
    /// Content identity immutable РіСЂР°С„Р°.
    #[must_use]
    pub fn content_identity(&self) -> [u8; 32] {
        *self.owner.content_identity().as_bytes()
    }

    /// Consuming instantiate: owner Рё session РїРµСЂРµС…РѕРґСЏС‚ РѕРґРЅРѕРјСѓ runtime.
    pub fn instantiate(self, stream_id: u32) -> Result<ProgramSessionV1, ProgramRuntimeErrorV1> {
        let session = self
            .owner
            .instantiate(stream_id)
            .map_err(|_| ProgramRuntimeErrorV1::Instantiate)?;
        Ok(ProgramSessionV1 {
            owner: self.owner,
            session,
            outputs_scratch: Vec::new(),
        })
    }
}

impl ProgramSessionV1 {
    /// РђС‚РѕРјР°СЂРЅРѕ РїСЂРёРјРµРЅСЏРµС‚ observed update Рё РІРѕР·РІСЂР°С‰Р°РµС‚ owned snapshot.
    ///
    /// РџРѕРґРіРѕС‚РѕРІР»РµРЅРЅС‹Р№ РїРµСЂРµС…РѕРґ Р»РёР±Рѕ commit'РёС‚СЃСЏ С†РµР»РёРєРѕРј, Р»РёР±Рѕ РїСЂРё Р»СЋР±РѕРј РѕС‚РєР°Р·Рµ
    /// Session СЃРѕС…СЂР°РЅСЏРµС‚ РїСЂРµРґС‹РґСѓС‰РёРµ head/lifecycle/evidence вЂ” Р·Р°РєРѕРЅ РІРЅСѓС‚СЂРµРЅРЅРµР№
    /// `PreparedSessionTransitionV1` РЅРµ РѕСЃР»Р°Р±Р»СЏРµС‚СЃСЏ РїСѓР±Р»РёС‡РЅРѕР№ РѕР±С‘СЂС‚РєРѕР№.
    pub fn update_observed(
        &mut self,
        revision: u64,
        scenarios: &[ProgramScenarioV1],
    ) -> Result<ProgramSnapshotV1, ProgramRuntimeErrorV1> {
        self.outputs_scratch
            .try_reserve_exact(self.owner.output_slots().count())
            .map_err(|_| ProgramRuntimeErrorV1::Update)?;
        let source = ProgramScenarioSourceV1(scenarios);
        let transition = self
            .owner
            .prepare_schema_ordered_update(&mut self.session, revision, &source)
            .map_err(|_| ProgramRuntimeErrorV1::Update)?;
        let evidence = transition.commit();
        snapshot_from_evidence_into(evidence, &mut self.outputs_scratch)
            .map_err(|_| ProgramRuntimeErrorV1::Update)
    }

    /// РђС‚РѕРјР°СЂРЅРѕ РїСЂРёРјРµРЅСЏРµС‚ Unknown update СЃ РЅРµРїСЂРѕР·СЂР°С‡РЅРѕР№ РїСЂРёС‡РёРЅРѕР№.
    pub fn update_unknown(
        &mut self,
        revision: u64,
        reason_id: u32,
    ) -> Result<ProgramSnapshotV1, ProgramRuntimeErrorV1> {
        self.outputs_scratch
            .try_reserve_exact(self.owner.output_slots().count())
            .map_err(|_| ProgramRuntimeErrorV1::Update)?;
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
        snapshot_from_evidence_into(transition.commit(), &mut self.outputs_scratch)
            .map_err(|_| ProgramRuntimeErrorV1::Update)
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
) -> Result<ProgramSnapshotV1, ()> {
    use crate::program::{CertificateV1, StateKindV1};
    let state = match evidence.kind() {
        StateKindV1::Waiting => ProgramSnapshotStateV1::Waiting,
        StateKindV1::Ready => ProgramSnapshotStateV1::Ready,
        StateKindV1::Stale => ProgramSnapshotStateV1::Stale,
        StateKindV1::Failed => ProgramSnapshotStateV1::Failed,
    };
    outputs_scratch.clear();
    if let Some(verified) = evidence
        .certificates()
        .find_map(|certificate| match certificate {
            CertificateV1::Verified(verified) => Some(verified),
            CertificateV1::Conflict(_) => None,
        })
    {
        outputs_scratch
            .try_reserve_exact(verified.outputs().len())
            .map_err(|_| ())?;
        outputs_scratch.extend(verified.outputs().map(|output| ProgramPaintOutputV1 {
            slot: output.output_slot().value(),
            source: output.source(),
            opacity: output.opacity(),
        }));
    }
    Ok(ProgramSnapshotV1 {
        state,
        outputs: std::mem::take(outputs_scratch),
    })
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
        // РЎР»РµРґСѓСЋС‰РёР№ РІР°Р»РёРґРЅС‹Р№ update РґРѕР»Р¶РµРЅ РїСЂРѕРґРѕР»Р¶РёС‚СЊ С‚Сѓ Р¶Рµ Session Рё СЃРЅРѕРІР° Ready.
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
