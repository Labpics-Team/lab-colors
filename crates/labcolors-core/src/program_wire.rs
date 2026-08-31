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
        let source = ProgramScenarioSourceV1(scenarios);
        let transition = self
            .owner
            .prepare_schema_ordered_update(&mut self.session, revision, &source)
            .map_err(|_| ProgramRuntimeErrorV1::Update)?;
        let evidence = transition.commit();
        Ok(snapshot_from_evidence_into(
            evidence,
            &mut self.outputs_scratch,
        ))
    }

    /// РђС‚РѕРјР°СЂРЅРѕ РїСЂРёРјРµРЅСЏРµС‚ Unknown update СЃ РЅРµРїСЂРѕР·СЂР°С‡РЅРѕР№ РїСЂРёС‡РёРЅРѕР№.
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
        Ok(snapshot_from_evidence_into(
            transition.commit(),
            &mut self.outputs_scratch,
        ))
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
) -> ProgramSnapshotV1 {
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
        outputs_scratch.extend(verified.outputs().map(|output| ProgramPaintOutputV1 {
            slot: output.output_slot().value(),
            source: output.source(),
            opacity: output.opacity(),
        }));
    }
    ProgramSnapshotV1 {
        state,
        outputs: std::mem::take(outputs_scratch),
    }
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
