//! Каноническая wire-грамматика авторского Draft-графа Program (V1–V3).
//!
//! Слой 1 двухслойного публичного контракта: байты -> декларации. Семантику
//! графа проверяет слой 2 — атомарный [`super::DraftV1::compile`]; декодер
//! сознательно не умеет выражать семантические отказы, а компилятор — байтовые
//! offset'ы. Формат — canonical binary: одна Program-декларация <=> одни байты,
//! что напрямую стыкуется с content identity и exact-bytes дисциплиной репо.
//!
//! Канон: header `LCPW` + u16 version + u32 total_len; секции строго в порядке
//! полей `CoreProgramDraftV1`; каждая секция — `u32 count` + LE-записи в
//! authored-порядке; непустой остаток после последней секции — typed отказ.
//! Дубликаты ID ловит `compile()` — декодер не дублирует его закон.

use crate::Srgb8;

use super::{
    AppearanceContextErrorV1, AppearanceContextV1, ConstraintIdV1, DirectedRelationV1, DraftV1,
    FamilyIdV1, FamilySemanticReleaseV2, FinitePaintDomainErrorV1, FinitePaintDomainV1,
    OccurrenceIdV1, OpacityInputIdV1, OutputSlotIdV1, PaintIdV1, PaintValueErrorV1, PaintValueV1,
    PresentationRootIdV1, SourceIdV1, SurfaceIdV1, SurfaceInputPortIdV1, SurroundV1,
    TargetCandidateIdV1, TargetCandidateV1, TargetIdV1,
};
use crate::relation::DirectedRelationErrorV1;
use crate::wcag22::Wcag22CriterionV1;

/// Magic-префикс формата: Lab Colors Program Wire.
pub(crate) const PROGRAM_WIRE_MAGIC_V1: [u8; 4] = *b"LCPW";
/// Исходный envelope без авторской записи joint selection.
pub(crate) const PROGRAM_WIRE_VERSION_V1: u16 = 1;
/// Explicit authored SelectionRelease envelope; V1 remains unchanged.
pub(crate) const PROGRAM_WIRE_VERSION_V2: u16 = 2;
pub(crate) const PROGRAM_WIRE_VERSION_V3: u16 = 3;
/// Верхняя граница записей одной секции: fail-closed budget pin по образцу
/// evidence-bounds. Значение намеренно щедрое для авторских графов и
/// намеренно фатальное для hostile length-полей.
pub(crate) const MAX_SECTION_ENTRIES_V1: u32 = 4096;

/// Секция, в которой декодер зафиксировал отказ. Нумерация — порядок секций
/// формата; это wire-имя, а не runtime-структура.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WireSectionV1 {
    Header,
    Sources,
    Targets,
    Families,
    JointSelection,
    SurfaceInputPorts,
    OpacityInputs,
    Paints,
    Surfaces,
    Occurrences,
    PresentationRoots,
    PresentationTargets,
    HardConstraints,
    ReportConstraints,
    Outputs,
    Trailer,
}

/// Typed-отказ слоя 1 (байты -> декларации).
///
/// Не выражает семантику графа: dangling/duplicate ссылки — территория
/// [`super::CompileErrorV1`]. Каждый вариант несёт секцию и смещение начала
/// записи, на которой канон нарушен, — этого достаточно для детерминированной
/// диагностики без раскрытия внутренних структур.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramWireErrorV1 {
    /// Первые четыре байта не `LCPW`.
    InvalidMagic,
    /// Версию формата этот декодер не понимает.
    UnsupportedVersion {
        declared: u16,
    },
    /// Буфер короче заявленной длины, длина не совпадает с фактической или
    /// после последней секции остался непустой хвост.
    InvalidLength,
    /// Запись нарушает канон представления: недопустимый discriminant,
    /// значение вне домена или неканоничная форма.
    InvalidDeclaration {
        section: WireSectionV1,
        offset: usize,
    },
    /// Длина секции превышает объявленный wire-limit.
    ResourceExhausted {
        section: WireSectionV1,
    },
    AssignmentResourceExhausted {
        section: WireSectionV1,
    },
}

/// Позиционный reader: та же fail-closed дисциплина, что у private fixture
/// (`finish()`-остаток, LE-скаляры, opacity как `f64::from_bits`), но с
/// секционным контекстом ошибок вместо плоских кодов.
struct WireReader<'a> {
    bytes: &'a [u8],
    offset: usize,
    section: WireSectionV1,
}

impl<'a> WireReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            offset: 0,
            section: WireSectionV1::Header,
        }
    }

    fn enter(&mut self, section: WireSectionV1) {
        self.section = section;
    }

    fn declaration_error(&self, offset: usize) -> ProgramWireErrorV1 {
        ProgramWireErrorV1::InvalidDeclaration {
            section: self.section,
            offset,
        }
    }

    fn read_bytes<const N: usize>(&mut self) -> Result<[u8; N], ProgramWireErrorV1> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or(ProgramWireErrorV1::InvalidLength)?;
        let source = self
            .bytes
            .get(self.offset..end)
            .ok_or(ProgramWireErrorV1::InvalidLength)?;
        let mut value = [0; N];
        value.copy_from_slice(source);
        self.offset = end;
        Ok(value)
    }

    fn read_u8(&mut self) -> Result<u8, ProgramWireErrorV1> {
        let [value] = self.read_bytes::<1>()?;
        Ok(value)
    }

    fn read_u16(&mut self) -> Result<u16, ProgramWireErrorV1> {
        Ok(u16::from_le_bytes(self.read_bytes::<2>()?))
    }

    fn read_u32(&mut self) -> Result<u32, ProgramWireErrorV1> {
        Ok(u32::from_le_bytes(self.read_bytes::<4>()?))
    }

    fn read_u64(&mut self) -> Result<u64, ProgramWireErrorV1> {
        Ok(u64::from_le_bytes(self.read_bytes::<8>()?))
    }

    fn read_f64_bits(&mut self) -> Result<f64, ProgramWireErrorV1> {
        Ok(f64::from_bits(self.read_u64()?))
    }

    fn read_rgb(&mut self) -> Result<Srgb8, ProgramWireErrorV1> {
        Ok(Srgb8::new(self.read_bytes::<3>()?))
    }

    fn read_count(&mut self) -> Result<u32, ProgramWireErrorV1> {
        let count = self.read_u32()?;
        if count > MAX_SECTION_ENTRIES_V1 {
            return Err(ProgramWireErrorV1::ResourceExhausted {
                section: self.section,
            });
        }
        Ok(count)
    }

    fn finish(&self) -> Result<(), ProgramWireErrorV1> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(ProgramWireErrorV1::InvalidLength)
        }
    }
}

fn read_selection_release(
    reader: &mut WireReader<'_>,
) -> Result<crate::selection_release::MaterialisedSelectionV1, ProgramWireErrorV1> {
    use crate::program_session::{
        JointCandidateStateV1, TargetCandidateChoiceV1, TargetCandidateId, TargetId,
    };
    use crate::selection_release::{
        SelectionCandidateKeyV1, SelectionReleaseV1, admit_selection_release_v1,
        materialise_joint_selection_v1,
    };

    let start = reader.offset;
    let revision = reader.read_u64()?;
    let group_count = reader.read_count()?;
    let mut groups = Vec::new();
    let mut bindings = Vec::new();
    let mut states_left = MAX_SECTION_ENTRIES_V1;
    let mut choices_left = MAX_SECTION_ENTRIES_V1;
    for _ in 0..group_count {
        let count = reader.read_count()?;
        states_left =
            states_left
                .checked_sub(count)
                .ok_or(ProgramWireErrorV1::ResourceExhausted {
                    section: WireSectionV1::JointSelection,
                })?;
        let mut keys = Vec::new();
        for _ in 0..count {
            let id = reader.read_u32()?;
            let key = SelectionCandidateKeyV1::new(Box::new(id.to_be_bytes()));
            let choices = reader.read_count()?;
            choices_left =
                choices_left
                    .checked_sub(choices)
                    .ok_or(ProgramWireErrorV1::ResourceExhausted {
                        section: WireSectionV1::JointSelection,
                    })?;
            let mut state = Vec::new();
            for _ in 0..choices {
                state.push(TargetCandidateChoiceV1::new(
                    TargetId::new(reader.read_u32()?),
                    TargetCandidateId::new(reader.read_u32()?),
                ));
            }
            keys.push(key.clone());
            bindings.push((JointCandidateStateV1::new(state), key));
        }
        groups.push(keys.into_boxed_slice());
    }
    let release =
        admit_selection_release_v1(SelectionReleaseV1::new(revision, groups.into_boxed_slice()))
            .map_err(|_| reader.declaration_error(start))?;
    materialise_joint_selection_v1(&release, &bindings).map_err(|_| reader.declaration_error(start))
}

/// Декодирует канонические байты в авторский Draft-граф.
///
/// Возвращаемый [`DraftV1`] дальше проходит атомарный `compile()`; сам декодер
/// гарантирует только байтовый канон и домены отдельных записей. Joint
/// selection на wire v1 непредставим (0 записей обязательно): материализация
/// порядка принадлежит `SelectionRelease`-допуску, не авторским байтам.
pub(crate) fn decode_program_wire_v1(bytes: &[u8]) -> Result<DraftV1, ProgramWireErrorV1> {
    decode_program_wire_version(bytes, Some(PROGRAM_WIRE_VERSION_V1))
}

/// Decode a supported envelope into the same admitted Program draft.
pub(crate) fn decode_program_wire(bytes: &[u8]) -> Result<DraftV1, ProgramWireErrorV1> {
    decode_program_wire_version(bytes, None)
}

fn decode_program_wire_version(
    bytes: &[u8],
    required: Option<u16>,
) -> Result<DraftV1, ProgramWireErrorV1> {
    let mut reader = WireReader::new(bytes);

    // Header: magic + version + total_len (fail-closed до чтения секций).
    let magic = reader.read_bytes::<4>()?;
    if magic != PROGRAM_WIRE_MAGIC_V1 {
        return Err(ProgramWireErrorV1::InvalidMagic);
    }
    let version = reader.read_u16()?;
    if !matches!(
        version,
        PROGRAM_WIRE_VERSION_V1 | PROGRAM_WIRE_VERSION_V2 | PROGRAM_WIRE_VERSION_V3
    ) || required.is_some_and(|expected| version != expected)
    {
        return Err(ProgramWireErrorV1::UnsupportedVersion { declared: version });
    }
    let declared_len = reader.read_u32()?;
    if usize::try_from(declared_len).map_err(|_| ProgramWireErrorV1::InvalidLength)? != bytes.len()
    {
        return Err(ProgramWireErrorV1::InvalidLength);
    }

    let decoded = decode_body(&mut reader, version);
    decoded.map_err(|error| match error {
        ProgramWireErrorV1::ResourceExhausted { section } if version == PROGRAM_WIRE_VERSION_V3 => {
            ProgramWireErrorV1::AssignmentResourceExhausted { section }
        }
        other => other,
    })
}

fn decode_body(reader: &mut WireReader<'_>, version: u16) -> Result<DraftV1, ProgramWireErrorV1> {
    let mut draft = DraftV1::new();

    // Sources: id u32 + rgb.
    reader.enter(WireSectionV1::Sources);
    for _ in 0..reader.read_count()? {
        let id = reader.read_u32()?;
        let rgb = reader.read_rgb()?;
        draft.push_source(SourceIdV1::new(id), rgb);
    }

    // Targets: tag u8 (1=fixed{source u32} | 2=finite{candidates}).
    reader.enter(WireSectionV1::Targets);
    for _ in 0..reader.read_count()? {
        let entry_offset = reader.offset;
        let id = reader.read_u32()?;
        match reader.read_u8()? {
            1 => {
                let source = reader.read_u32()?;
                draft.push_fixed_target(TargetIdV1::new(id), SourceIdV1::new(source));
            }
            2 => {
                let candidate_count = reader.read_count()?;
                let mut candidates = Vec::new();
                for _ in 0..candidate_count {
                    let candidate_offset = reader.offset;
                    let candidate_id = reader.read_u32()?;
                    let rgb = reader.read_rgb()?;
                    let opacity = reader.read_f64_bits()?;
                    let value = PaintValueV1::try_new(rgb, opacity).map_err(|error| {
                        let _: PaintValueErrorV1 = error;
                        reader.declaration_error(candidate_offset)
                    })?;
                    candidates.push(TargetCandidateV1::new(
                        TargetCandidateIdV1::new(candidate_id),
                        value,
                    ));
                }
                let domain = FinitePaintDomainV1::try_new(candidates).map_err(|error| {
                    let _: FinitePaintDomainErrorV1 = error;
                    reader.declaration_error(entry_offset)
                })?;
                draft.push_finite_target(TargetIdV1::new(id), domain);
            }
            3 if version == PROGRAM_WIRE_VERSION_V3 => {
                use crate::program_session::computed_assignment::AssignmentDomainV1;
                let domain = match reader.read_u8()? {
                    1 => AssignmentDomainV1::Cube {
                        lower: reader.read_bytes::<3>()?,
                        upper: reader.read_bytes::<3>()?,
                    },
                    2 => AssignmentDomainV1::Gray {
                        lower: reader.read_u8()?,
                        upper: reader.read_u8()?,
                    },
                    _ => return Err(reader.declaration_error(entry_offset)),
                };
                let anchor = reader.read_rgb()?;
                let tolerance = reader.read_f64_bits()?;
                draft.push_computed_target(TargetIdV1::new(id), domain, anchor, tolerance);
            }
            _ => return Err(reader.declaration_error(entry_offset)),
        }
    }

    // Families: id u32 + 32-байтный semantic release.
    reader.enter(WireSectionV1::Families);
    for _ in 0..reader.read_count()? {
        let id = reader.read_u32()?;
        let release = reader.read_bytes::<32>()?;
        draft.push_family(
            FamilyIdV1::new(id),
            FamilySemanticReleaseV2::from_wire_bytes(release),
        );
    }

    // Joint selection: wire v1 требует ровно 0 записей — порядок выборки
    // материализуется только допуском SelectionRelease, не авторскими байтами.
    reader.enter(WireSectionV1::JointSelection);
    let joint_offset = reader.offset;
    let selection_count = reader.read_count()?;
    match (version, selection_count) {
        (PROGRAM_WIRE_VERSION_V1 | PROGRAM_WIRE_VERSION_V2, 0) => {}
        (PROGRAM_WIRE_VERSION_V3, 1) => {
            if reader.read_u16()? != 2 {
                return Err(reader.declaration_error(joint_offset));
            }
            let count = reader.read_count()?;
            let mut order = Vec::new();
            order.try_reserve_exact(count as usize).map_err(|_| {
                ProgramWireErrorV1::ResourceExhausted {
                    section: WireSectionV1::JointSelection,
                }
            })?;
            for _ in 0..count {
                order.push(crate::program_session::TargetId::new(reader.read_u32()?));
            }
            draft.set_assignment(order);
        }
        (PROGRAM_WIRE_VERSION_V2, 1) => {
            let selection = read_selection_release(reader)?;
            draft
                .set_materialised_joint_selection(selection)
                .map_err(|_| reader.declaration_error(joint_offset))?;
        }
        _ => return Err(reader.declaration_error(joint_offset)),
    }

    // Surface input ports: id u32.
    reader.enter(WireSectionV1::SurfaceInputPorts);
    for _ in 0..reader.read_count()? {
        let id = reader.read_u32()?;
        draft.push_surface_input_port(SurfaceInputPortIdV1::new(id));
    }

    // Opacity inputs: id u32 + f64-bits (домен проверяет компиляция).
    reader.enter(WireSectionV1::OpacityInputs);
    for _ in 0..reader.read_count()? {
        let id = reader.read_u32()?;
        let value = reader.read_f64_bits()?;
        draft.push_opacity_input(OpacityInputIdV1::new(id), value);
    }

    // Paints: tag u8 (1=solid{target} | 2=opacity{source paint, opacity input}).
    reader.enter(WireSectionV1::Paints);
    for _ in 0..reader.read_count()? {
        let entry_offset = reader.offset;
        let id = reader.read_u32()?;
        match reader.read_u8()? {
            1 => {
                let target = reader.read_u32()?;
                draft.push_solid_paint(PaintIdV1::new(id), TargetIdV1::new(target));
            }
            2 => {
                let source = reader.read_u32()?;
                let opacity = reader.read_u32()?;
                draft.push_opacity_paint(
                    PaintIdV1::new(id),
                    PaintIdV1::new(source),
                    OpacityInputIdV1::new(opacity),
                );
            }
            _ => return Err(reader.declaration_error(entry_offset)),
        }
    }

    // Surfaces: tag u8 (1=input{port} | 2=from-occurrence{occurrence}).
    reader.enter(WireSectionV1::Surfaces);
    for _ in 0..reader.read_count()? {
        let entry_offset = reader.offset;
        let id = reader.read_u32()?;
        match reader.read_u8()? {
            1 => {
                let input = reader.read_u32()?;
                draft.push_input_surface(SurfaceIdV1::new(id), SurfaceInputPortIdV1::new(input));
            }
            2 => {
                let occurrence = reader.read_u32()?;
                draft
                    .push_occurrence_surface(SurfaceIdV1::new(id), OccurrenceIdV1::new(occurrence));
            }
            _ => return Err(reader.declaration_error(entry_offset)),
        }
    }

    // Occurrences: id + paint + surface + appearance (La f64, Yb/Yw f64,
    // surround u8: 1=Average 2=Dim 3=Dark). Единственный composition profile
    // v1 — encoded sRGB8 source-over, тег не нужен.
    reader.enter(WireSectionV1::Occurrences);
    for _ in 0..reader.read_count()? {
        let entry_offset = reader.offset;
        let id = reader.read_u32()?;
        let subject = reader.read_u32()?;
        let against = reader.read_u32()?;
        let adapting = reader.read_f64_bits()?;
        let background_ratio = reader.read_f64_bits()?;
        let surround = match reader.read_u8()? {
            1 => SurroundV1::Average,
            2 => SurroundV1::Dim,
            3 => SurroundV1::Dark,
            _ => return Err(reader.declaration_error(entry_offset)),
        };
        let context = AppearanceContextV1::try_new(adapting, background_ratio, surround).map_err(
            |error| {
                let _: AppearanceContextErrorV1 = error;
                reader.declaration_error(entry_offset)
            },
        )?;
        draft.push_source_over_occurrence(
            OccurrenceIdV1::new(id),
            PaintIdV1::new(subject),
            SurfaceIdV1::new(against),
            context,
        );
    }

    // Presentation roots: id + terminal occurrence.
    reader.enter(WireSectionV1::PresentationRoots);
    for _ in 0..reader.read_count()? {
        let id = reader.read_u32()?;
        let terminal = reader.read_u32()?;
        draft.push_point_presentation_root(
            PresentationRootIdV1::new(id),
            OccurrenceIdV1::new(terminal),
        );
    }

    // Presentation targets: root + occurrence.
    reader.enter(WireSectionV1::PresentationTargets);
    for _ in 0..reader.read_count()? {
        let root = reader.read_u32()?;
        let occurrence = reader.read_u32()?;
        draft.push_point_presentation_target(
            PresentationRootIdV1::new(root),
            OccurrenceIdV1::new(occurrence),
        );
    }

    // Hard constraints, затем report constraints — одна грамматика записей,
    // разный режим. Report-грамматика допускает только report-able виды.
    reader.enter(WireSectionV1::HardConstraints);
    for _ in 0..reader.read_count()? {
        decode_constraint(reader, &mut draft, ConstraintModeV1::Hard)?;
    }
    reader.enter(WireSectionV1::ReportConstraints);
    for _ in 0..reader.read_count()? {
        decode_constraint(reader, &mut draft, ConstraintModeV1::ReportOnly)?;
    }

    // Outputs: slot + paint.
    reader.enter(WireSectionV1::Outputs);
    for _ in 0..reader.read_count()? {
        let slot = reader.read_u32()?;
        let paint = reader.read_u32()?;
        draft.push_output(OutputSlotIdV1::new(slot), PaintIdV1::new(paint));
    }

    reader.enter(WireSectionV1::Trailer);
    reader.finish()?;
    Ok(draft)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConstraintModeV1 {
    Hard,
    ReportOnly,
}

/// Виды констрейнтов wire v1. Дискриминанты — контракт формата: их сдвиг
/// без новой версии молча переклассифицировал бы записи.
const KIND_EXACT_VISIBLE_UNARY: u8 = 1;
const KIND_EXACT_INTRINSIC_UNARY: u8 = 2;
const KIND_FAMILY_MEMBERSHIP: u8 = 3;
const KIND_EXACT_INTRINSIC_RELATION: u8 = 4;
const KIND_EXACT_VISIBLE_RELATION: u8 = 5;
const KIND_INTRINSIC_DISTINCTION: u8 = 6;
const KIND_VISIBLE_DISTINCTION: u8 = 7;
const KIND_FAMILY_CATEGORY_RELATION: u8 = 8;
const KIND_WCAG22_VISIBLE_UNARY: u8 = 9;
const KIND_CLEAN_SET: u8 = 10;

fn decode_constraint(
    reader: &mut WireReader<'_>,
    draft: &mut DraftV1,
    mode: ConstraintModeV1,
) -> Result<(), ProgramWireErrorV1> {
    let entry_offset = reader.offset;
    let id = ConstraintIdV1::new(reader.read_u32()?);
    let kind = reader.read_u8()?;
    let hard = matches!(mode, ConstraintModeV1::Hard);
    match kind {
        KIND_EXACT_VISIBLE_UNARY => {
            let occurrence = OccurrenceIdV1::new(reader.read_u32()?);
            let expected = reader.read_rgb()?;
            if hard {
                draft.push_exact_visible_unary_hard(id, occurrence, expected);
            } else {
                draft.push_exact_visible_unary_report_only(id, occurrence, expected);
            }
        }
        KIND_EXACT_INTRINSIC_UNARY if hard => {
            let target = TargetIdV1::new(reader.read_u32()?);
            let expected = reader.read_rgb()?;
            draft.push_exact_intrinsic_unary_hard(id, target, expected);
        }
        KIND_FAMILY_MEMBERSHIP => {
            let target = TargetIdV1::new(reader.read_u32()?);
            let family = FamilyIdV1::new(reader.read_u32()?);
            if hard {
                draft.push_intrinsic_family_membership_hard(id, target, family);
            } else {
                draft.push_intrinsic_family_membership_report_only(id, target, family);
            }
        }
        KIND_EXACT_INTRINSIC_RELATION if hard => {
            let relation = decode_relation(reader, entry_offset, TargetIdV1::new)?;
            draft.push_exact_intrinsic_relation_hard(id, relation);
        }
        KIND_EXACT_VISIBLE_RELATION if hard => {
            let relation = decode_relation(reader, entry_offset, OccurrenceIdV1::new)?;
            draft.push_exact_visible_relation_hard(id, relation);
        }
        KIND_INTRINSIC_DISTINCTION if hard => {
            let relation = decode_relation(reader, entry_offset, TargetIdV1::new)?;
            draft.push_exact_intrinsic_distinction_hard(id, relation);
        }
        KIND_VISIBLE_DISTINCTION if hard => {
            let relation = decode_relation(reader, entry_offset, OccurrenceIdV1::new)?;
            draft.push_exact_visible_distinction_hard(id, relation);
        }
        KIND_FAMILY_CATEGORY_RELATION if hard => {
            let family = FamilyIdV1::new(reader.read_u32()?);
            let relation = decode_relation(reader, entry_offset, TargetIdV1::new)?;
            draft.push_intrinsic_family_category_relation_hard(id, relation, family);
        }
        KIND_WCAG22_VISIBLE_UNARY => {
            let occurrence = OccurrenceIdV1::new(reader.read_u32()?);
            let criterion = match reader.read_u8()? {
                1 => Wcag22CriterionV1::Sc143TextDefault,
                2 => Wcag22CriterionV1::Sc143TextLargeScale,
                3 => Wcag22CriterionV1::Sc1411UiComponentOrState,
                4 => Wcag22CriterionV1::Sc1411GraphicalObject,
                _ => return Err(reader.declaration_error(entry_offset)),
            };
            if hard {
                draft.push_wcag22_visible_unary_hard(id, occurrence, criterion);
            } else {
                draft.push_wcag22_visible_unary_report_only(id, occurrence, criterion);
            }
        }
        KIND_CLEAN_SET => {
            let root = PresentationRootIdV1::new(reader.read_u32()?);
            let occurrence = OccurrenceIdV1::new(reader.read_u32()?);
            if hard {
                draft.push_declared_srgb8_clean_set_hard(id, root, occurrence);
            } else {
                draft.push_declared_srgb8_clean_set_report_only(id, root, occurrence);
            }
        }
        _ => return Err(reader.declaration_error(entry_offset)),
    }
    Ok(())
}

/// Читает directed relation: reference u32 + count + candidates u32.
/// Канон topology (непустота, сортировка без повторов, reference вне
/// candidates) доказывает `DirectedRelationV1::try_new` — decoder лишь
/// проецирует его отказ в байтовый.
fn decode_relation<T, F>(
    reader: &mut WireReader<'_>,
    entry_offset: usize,
    make: F,
) -> Result<DirectedRelationV1<T>, ProgramWireErrorV1>
where
    T: Copy + Ord + core::fmt::Debug,
    F: Fn(u32) -> T,
{
    let reference = make(reader.read_u32()?);
    let count = reader.read_count()?;
    let mut candidates = Vec::new();
    for _ in 0..count {
        candidates.push(make(reader.read_u32()?));
    }
    DirectedRelationV1::try_new(reference, candidates).map_err(|error| {
        let _: DirectedRelationErrorV1<T> = error;
        reader.declaration_error(entry_offset)
    })
}

#[cfg(test)]
mod tests {
    use super::super::{
        AppearanceContextV1, ConstraintIdV1, DraftV1, OccurrenceIdV1, OutputSlotIdV1, PaintIdV1,
        PresentationRootIdV1, SourceIdV1, SurfaceIdV1, SurfaceInputPortIdV1, SurroundV1,
        TargetIdV1,
    };
    use super::*;

    struct WireBuilder {
        bytes: Vec<u8>,
    }

    impl WireBuilder {
        fn new() -> Self {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&PROGRAM_WIRE_MAGIC_V1);
            bytes.extend_from_slice(&PROGRAM_WIRE_VERSION_V1.to_le_bytes());
            bytes.extend_from_slice(&0_u32.to_le_bytes()); // patched by seal()
            Self { bytes }
        }

        fn u8(&mut self, value: u8) -> &mut Self {
            self.bytes.push(value);
            self
        }

        fn u32(&mut self, value: u32) -> &mut Self {
            self.bytes.extend_from_slice(&value.to_le_bytes());
            self
        }

        fn f64_bits(&mut self, value: f64) -> &mut Self {
            self.bytes.extend_from_slice(&value.to_bits().to_le_bytes());
            self
        }

        fn rgb(&mut self, rgb: [u8; 3]) -> &mut Self {
            self.bytes.extend_from_slice(&rgb);
            self
        }

        fn seal(mut self) -> Vec<u8> {
            let len = u32::try_from(self.bytes.len()).unwrap();
            self.bytes[6..10].copy_from_slice(&len.to_le_bytes());
            self.bytes
        }
    }

    /// Канонические байты нетривиального графа: source -> fixed target ->
    /// solid paint -> input surface -> occurrence -> presentation root +
    /// WCAG22 hard + exact visible report + output binding.
    fn reference_wire() -> Vec<u8> {
        let mut wire = WireBuilder::new();
        wire.u32(1).u32(11).rgb([0x14, 0x14, 0x14]); // sources: 1
        wire.u32(1).u32(21).u8(1).u32(11); // targets: 1 fixed
        wire.u32(0); // families
        wire.u32(0); // joint selection (v1: always 0)
        wire.u32(1).u32(31); // surface input ports
        wire.u32(0); // opacity inputs
        wire.u32(1).u32(41).u8(1).u32(21); // paints: solid
        wire.u32(1).u32(51).u8(1).u32(31); // surfaces: input
        wire.u32(1) // occurrences
            .u32(61)
            .u32(41)
            .u32(51)
            .f64_bits(64.0)
            .f64_bits(0.2)
            .u8(1);
        wire.u32(1).u32(71).u32(61); // presentation roots
        wire.u32(1).u32(71).u32(61); // presentation targets
        wire.u32(1) // hard constraints: wcag22
            .u32(81)
            .u8(KIND_WCAG22_VISIBLE_UNARY)
            .u32(61)
            .u8(3);
        wire.u32(1) // report constraints: exact visible
            .u32(82)
            .u8(KIND_EXACT_VISIBLE_UNARY)
            .u32(61)
            .rgb([0x14, 0x14, 0x14]);
        wire.u32(1).u32(91).u32(41); // outputs
        wire.seal()
    }

    /// Тот же граф, объявленный напрямую строителем DraftV1.
    fn reference_draft() -> DraftV1 {
        let mut draft = DraftV1::new();
        draft.push_source(SourceIdV1::new(11), crate::Srgb8::new([0x14, 0x14, 0x14]));
        draft.push_fixed_target(TargetIdV1::new(21), SourceIdV1::new(11));
        draft.push_surface_input_port(SurfaceInputPortIdV1::new(31));
        draft.push_solid_paint(PaintIdV1::new(41), TargetIdV1::new(21));
        draft.push_input_surface(SurfaceIdV1::new(51), SurfaceInputPortIdV1::new(31));
        draft.push_source_over_occurrence(
            OccurrenceIdV1::new(61),
            PaintIdV1::new(41),
            SurfaceIdV1::new(51),
            AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Average).unwrap(),
        );
        draft.push_point_presentation_root(PresentationRootIdV1::new(71), OccurrenceIdV1::new(61));
        draft
            .push_point_presentation_target(PresentationRootIdV1::new(71), OccurrenceIdV1::new(61));
        draft.push_wcag22_visible_unary_hard(
            ConstraintIdV1::new(81),
            OccurrenceIdV1::new(61),
            Wcag22CriterionV1::Sc1411UiComponentOrState,
        );
        draft.push_exact_visible_unary_report_only(
            ConstraintIdV1::new(82),
            OccurrenceIdV1::new(61),
            crate::Srgb8::new([0x14, 0x14, 0x14]),
        );
        draft.push_output(OutputSlotIdV1::new(91), PaintIdV1::new(41));
        draft
    }

    /// Печатает канонический reference hex для кросс-язычного JS-parity
    /// теста (packages/colors/test/program-wire.test.mjs). ignored по
    /// умолчанию: это генератор фикстуры, не проверка.
    #[test]
    #[ignore = "fixture emitter: cargo test -p labcolors-core --lib wire::tests::_emit_reference_wire_hex -- --ignored --nocapture"]
    fn _emit_reference_wire_hex() {
        let hex: String = reference_wire().iter().fold(
            String::with_capacity(reference_wire().len() * 2),
            |mut s, b| {
                use std::fmt::Write;
                let _ = write!(s, "{b:02x}");
                s
            },
        );
        println!("REFERENCE_WIRE_HEX={hex}");
    }

    /// Builder эмитирует байт-в-байт те же канонические байты, что и ручная
    /// сборка reference wire, — канон существует в одном экземпляре.
    #[test]
    fn builder_bytes_are_byte_identical_to_the_reference_wire() {
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
            .exact_visible_unary(false, 82, 61, crate::Srgb8::new([0x14, 0x14, 0x14]))
            .output(91, 41);
        let bytes = builder.finish().unwrap();
        assert_eq!(bytes, reference_wire());
    }

    /// Roundtrip: builder -> decode -> compile даёт ту же identity, что и
    /// прямой draft; повторный encode тех же деклараций байт-идентичен.
    #[test]
    fn builder_roundtrip_preserves_identity_and_bytes() {
        let build = || {
            let mut builder = ProgramWireBuilderV1::new();
            builder
                .source(11, crate::Srgb8::new([0x14, 0x14, 0x14]))
                .source(12, crate::Srgb8::new([0x20, 0x20, 0x20]))
                // finite target + joint selection непредставимы на wire v1:
                // порядок выборки материализует SelectionRelease-допуск, а
                // graph без selection не компилируется — поэтому roundtrip
                // покрывает полный fixed-граф (см. joint_selection_on_wire_is_refused).
                .fixed_target(22, 12)
                .fixed_target(21, 11)
                .surface_input_port(31)
                .opacity_input(35, 0.75)
                .solid_paint(41, 21)
                .solid_paint(42, 22)
                .opacity_paint(43, 41, 35)
                .input_surface(51, 31)
                .source_over_occurrence(61, 43, 51, 64.0, 0.2, 2)
                .occurrence_surface(52, 61)
                .source_over_occurrence(62, 42, 52, 64.0, 0.2, 2)
                .presentation_root(71, 62)
                .presentation_target(71, 62)
                .exact_intrinsic_relation_hard(83, 21, &[22])
                .wcag22_visible_unary(true, 81, 62, 1)
                .output(91, 42);
            builder.finish().unwrap()
        };
        let first = build();
        let second = build();
        assert_eq!(first, second, "canonical bytes must be deterministic");

        let decoded = decode_program_wire_v1(&first).unwrap();
        let identity = decoded.compile().unwrap().content_identity();
        let decoded_again = decode_program_wire_v1(&second).unwrap();
        assert_eq!(
            identity,
            decoded_again.compile().unwrap().content_identity()
        );
    }

    /// Wire-limit энкодера симметричен декодеру: слишком большая секция —
    /// typed отказ, не молчаливое эмитирование неканоничного счётчика.
    #[test]
    fn builder_refuses_oversized_sections() {
        let mut builder = ProgramWireBuilderV1::new();
        for id in 0..=MAX_SECTION_ENTRIES_V1 {
            builder.surface_input_port(id);
        }
        assert!(matches!(
            builder.finish(),
            Err(ProgramWireEncodeErrorV1::TooManyEntries)
        ));
    }

    /// Happy path: байты компилируются в ту же content identity, что и граф,
    /// построенный напрямую, — wire не вносит и не теряет ни одного смысла.
    #[test]
    fn canonical_bytes_compile_to_the_directly_authored_identity() {
        let decoded = decode_program_wire_v1(&reference_wire()).unwrap();
        let from_wire = decoded.compile().unwrap().content_identity();
        let direct = reference_draft().compile().unwrap().content_identity();
        assert_eq!(from_wire, direct);
    }

    /// Каждый header-дефект — свой typed отказ, не паника и не смещение чтения.
    #[test]
    fn header_defects_are_typed_refusals() {
        let reference = reference_wire();

        let mut wrong_magic = reference.clone();
        wrong_magic[0] = b'X';
        assert!(matches!(
            decode_program_wire_v1(&wrong_magic),
            Err(ProgramWireErrorV1::InvalidMagic)
        ));

        let mut wrong_version = reference.clone();
        wrong_version[4] = 9;
        assert!(matches!(
            decode_program_wire_v1(&wrong_version),
            Err(ProgramWireErrorV1::UnsupportedVersion { declared: 9 })
        ));

        let mut wrong_len = reference.clone();
        wrong_len[6] ^= 0xFF;
        assert!(matches!(
            decode_program_wire_v1(&wrong_len),
            Err(ProgramWireErrorV1::InvalidLength)
        ));

        let mut trailing = reference.clone();
        trailing.push(0);
        assert!(matches!(
            decode_program_wire_v1(&trailing),
            Err(ProgramWireErrorV1::InvalidLength)
        ));

        let truncated = &reference[..reference.len() - 1];
        assert!(matches!(
            decode_program_wire_v1(truncated),
            Err(ProgramWireErrorV1::InvalidLength)
        ));
    }

    /// Недопустимый discriminant записи — typed InvalidDeclaration с секцией.
    #[test]
    fn invalid_discriminants_name_their_section() {
        // target tag 3 не существует.
        let mut wire = WireBuilder::new();
        wire.u32(0); // sources
        wire.u32(1).u32(21).u8(3); // targets: invalid tag
        let mut bytes = wire.seal();
        let len = u32::try_from(bytes.len()).unwrap();
        bytes[6..10].copy_from_slice(&len.to_le_bytes());
        match decode_program_wire_v1(&bytes) {
            Err(ProgramWireErrorV1::InvalidDeclaration { section, .. }) => {
                assert_eq!(section, WireSectionV1::Targets);
            }
            Err(other) => panic!("expected targets declaration refusal, got {other:?}"),
            Ok(_) => panic!("expected targets declaration refusal, got Ok"),
        }
    }

    /// Wire v1 не умеет объявлять joint selection: ненулевой счётчик — отказ.
    #[test]
    fn joint_selection_on_wire_is_refused() {
        let mut wire = WireBuilder::new();
        wire.u32(0); // sources
        wire.u32(0); // targets
        wire.u32(0); // families
        wire.u32(1); // joint selection: forbidden non-zero
        let bytes = wire.seal();
        match decode_program_wire_v1(&bytes) {
            Err(ProgramWireErrorV1::InvalidDeclaration { section, .. }) => {
                assert_eq!(section, WireSectionV1::JointSelection);
            }
            Err(other) => panic!("expected joint-selection refusal, got {other:?}"),
            Ok(_) => panic!("expected joint-selection refusal, got Ok"),
        }
    }

    /// Length-поле выше wire-limit — typed ResourceExhausted до аллокации.
    #[test]
    fn hostile_section_count_is_resource_exhausted() {
        let mut wire = WireBuilder::new();
        wire.u32(u32::MAX); // sources: hostile count
        let bytes = wire.seal();
        assert!(matches!(
            decode_program_wire_v1(&bytes),
            Err(ProgramWireErrorV1::ResourceExhausted {
                section: WireSectionV1::Sources
            })
        ));
    }

    /// Неканоничная opacity (NaN) в кандидате — байтовый отказ, не паника.
    #[test]
    fn non_finite_candidate_opacity_is_a_declaration_refusal() {
        let mut wire = WireBuilder::new();
        wire.u32(0); // sources
        wire.u32(1) // targets: finite with NaN opacity candidate
            .u32(21)
            .u8(2)
            .u32(1)
            .u32(1)
            .rgb([1, 2, 3])
            .f64_bits(f64::NAN);
        let bytes = wire.seal();
        match decode_program_wire_v1(&bytes) {
            Err(ProgramWireErrorV1::InvalidDeclaration { section, .. }) => {
                assert_eq!(section, WireSectionV1::Targets);
            }
            Err(other) => panic!("expected candidate refusal, got {other:?}"),
            Ok(_) => panic!("expected candidate refusal, got Ok"),
        }
    }

    /// Report-грамматика не принимает hard-only виды: relation в report — отказ.
    #[test]
    fn hard_only_kinds_are_refused_in_the_report_section() {
        let mut wire = WireBuilder::new();
        wire.u32(0); // sources
        wire.u32(0); // targets
        wire.u32(0); // families
        wire.u32(0); // joint
        wire.u32(0); // ports
        wire.u32(0); // opacities
        wire.u32(0); // paints
        wire.u32(0); // surfaces
        wire.u32(0); // occurrences
        wire.u32(0); // roots
        wire.u32(0); // presentation targets
        wire.u32(0); // hard constraints
        wire.u32(1) // report constraints: intrinsic relation (hard-only)
            .u32(81)
            .u8(KIND_EXACT_INTRINSIC_RELATION)
            .u32(1)
            .u32(1)
            .u32(2);
        wire.u32(0); // outputs
        let bytes = wire.seal();
        match decode_program_wire_v1(&bytes) {
            Err(ProgramWireErrorV1::InvalidDeclaration { section, .. }) => {
                assert_eq!(section, WireSectionV1::ReportConstraints);
            }
            Err(other) => panic!("expected report-section refusal, got {other:?}"),
            Ok(_) => panic!("expected report-section refusal, got Ok"),
        }
    }

    /// Семантический дефект (dangling ссылка) проходит декодер и отклоняется
    /// компилятором: слои отказов не смешаны.
    #[test]
    fn semantic_defects_belong_to_the_compiler_layer() {
        let mut wire = WireBuilder::new();
        wire.u32(0); // sources
        wire.u32(0); // targets
        wire.u32(0); // families
        wire.u32(0); // joint
        wire.u32(0); // ports
        wire.u32(0); // opacities
        wire.u32(1).u32(41).u8(1).u32(999); // paint -> dangling target
        wire.u32(0); // surfaces
        wire.u32(0); // occurrences
        wire.u32(0); // roots
        wire.u32(0); // presentation targets
        wire.u32(0); // hard
        wire.u32(0); // report
        wire.u32(0); // outputs
        let bytes = wire.seal();
        let draft = decode_program_wire_v1(&bytes).expect("bytes are canonical");
        assert!(
            draft.compile().is_err(),
            "dangling target must fail compile"
        );
    }
}

/// Typed-отказ канонического builder-а: декларация непредставима на wire v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramWireEncodeErrorV1 {
    /// Секция перерастает wire-limit: граф может быть легален для компиляции,
    /// но непредставим в этой версии формата.
    TooManyEntries,
}

/// Канонический builder байтов wire v1.
///
/// Двойник декодера с той же секционной грамматикой: клиент объявляет граф в
/// порядке секций, builder эмитирует единственное каноническое представление.
/// Никакого доступа к внутренностям Program: builder — генератор байтов, их
/// смысл доказывают `decode_program_wire_v1` + `compile()`.
///
/// Канон гарантируется конструкцией: секции пишутся в фиксированном порядке
/// (нарушение порядка — панике здесь неоткуда взяться, порядок навязан
/// поэтапными типами ниже), записи — в порядке вызовов, представление LE.
pub(crate) struct ProgramWireBuilderV1 {
    sources: Vec<u8>,
    sources_count: usize,
    targets: Vec<u8>,
    targets_count: usize,
    families: Vec<u8>,
    families_count: usize,
    surface_input_ports: Vec<u8>,
    surface_input_ports_count: usize,
    opacity_inputs: Vec<u8>,
    opacity_inputs_count: usize,
    paints: Vec<u8>,
    paints_count: usize,
    surfaces: Vec<u8>,
    surfaces_count: usize,
    occurrences: Vec<u8>,
    occurrences_count: usize,
    presentation_roots: Vec<u8>,
    presentation_roots_count: usize,
    presentation_targets: Vec<u8>,
    presentation_targets_count: usize,
    hard_constraints: Vec<u8>,
    hard_constraints_count: usize,
    report_constraints: Vec<u8>,
    report_constraints_count: usize,
    outputs: Vec<u8>,
    outputs_count: usize,
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_f64_bits(bytes: &mut Vec<u8>, value: f64) {
    bytes.extend_from_slice(&value.to_bits().to_le_bytes());
}

impl ProgramWireBuilderV1 {
    pub(crate) fn new() -> Self {
        Self {
            sources: Vec::new(),
            sources_count: 0,
            targets: Vec::new(),
            targets_count: 0,
            families: Vec::new(),
            families_count: 0,
            surface_input_ports: Vec::new(),
            surface_input_ports_count: 0,
            opacity_inputs: Vec::new(),
            opacity_inputs_count: 0,
            paints: Vec::new(),
            paints_count: 0,
            surfaces: Vec::new(),
            surfaces_count: 0,
            occurrences: Vec::new(),
            occurrences_count: 0,
            presentation_roots: Vec::new(),
            presentation_roots_count: 0,
            presentation_targets: Vec::new(),
            presentation_targets_count: 0,
            hard_constraints: Vec::new(),
            hard_constraints_count: 0,
            report_constraints: Vec::new(),
            report_constraints_count: 0,
            outputs: Vec::new(),
            outputs_count: 0,
        }
    }

    pub(crate) fn source(&mut self, id: u32, rgb: Srgb8) -> &mut Self {
        push_u32(&mut self.sources, id);
        self.sources.extend_from_slice(&rgb.bytes());
        self.sources_count += 1;
        self
    }

    pub(crate) fn fixed_target(&mut self, id: u32, source: u32) -> &mut Self {
        push_u32(&mut self.targets, id);
        self.targets.push(1);
        push_u32(&mut self.targets, source);
        self.targets_count += 1;
        self
    }

    #[cfg(test)]
    pub(crate) fn computed_box_target(
        &mut self,
        id: u32,
        lower: [u8; 3],
        upper: [u8; 3],
        anchor: Srgb8,
        max_delta: f64,
    ) -> &mut Self {
        push_u32(&mut self.targets, id);
        self.targets.extend_from_slice(&[3, 1]);
        self.targets.extend_from_slice(&lower);
        self.targets.extend_from_slice(&upper);
        self.targets.extend_from_slice(&anchor.bytes());
        push_f64_bits(&mut self.targets, max_delta);
        self.targets_count += 1;
        self
    }

    #[cfg(test)]
    pub(crate) fn computed_gray_target(
        &mut self,
        id: u32,
        lower: u8,
        upper: u8,
        anchor: Srgb8,
        max_delta: f64,
    ) -> &mut Self {
        push_u32(&mut self.targets, id);
        self.targets.extend_from_slice(&[3, 2, lower, upper]);
        self.targets.extend_from_slice(&anchor.bytes());
        push_f64_bits(&mut self.targets, max_delta);
        self.targets_count += 1;
        self
    }

    #[cfg(test)]
    pub(crate) fn finish_assignment(
        self,
        targets: &[u32],
    ) -> Result<Vec<u8>, ProgramWireEncodeErrorV1> {
        let offset = 10 + 12 + self.sources.len() + self.targets.len() + self.families.len();
        let mut bytes = self.finish()?;
        let mut selection = Vec::new();
        selection.extend_from_slice(&2_u16.to_le_bytes());
        push_u32(
            &mut selection,
            u32::try_from(targets.len()).map_err(|_| ProgramWireEncodeErrorV1::TooManyEntries)?,
        );
        for &target in targets {
            push_u32(&mut selection, target);
        }
        bytes[4..6].copy_from_slice(&3_u16.to_le_bytes());
        bytes[offset..offset + 4].copy_from_slice(&1_u32.to_le_bytes());
        bytes.splice(offset + 4..offset + 4, selection);
        let len =
            u32::try_from(bytes.len()).map_err(|_| ProgramWireEncodeErrorV1::TooManyEntries)?;
        bytes[6..10].copy_from_slice(&len.to_le_bytes());
        Ok(bytes)
    }

    /// Кандидаты — пары (id, rgb, opacity-bits) в authored-порядке.
    pub(crate) fn finite_target(&mut self, id: u32, candidates: &[(u32, Srgb8, f64)]) -> &mut Self {
        push_u32(&mut self.targets, id);
        self.targets.push(2);
        push_u32(
            &mut self.targets,
            u32::try_from(candidates.len()).unwrap_or(u32::MAX),
        );
        for (candidate_id, rgb, opacity) in candidates {
            push_u32(&mut self.targets, *candidate_id);
            self.targets.extend_from_slice(&rgb.bytes());
            push_f64_bits(&mut self.targets, *opacity);
        }
        self.targets_count += 1;
        self
    }

    pub(crate) fn family(&mut self, id: u32, release: [u8; 32]) -> &mut Self {
        push_u32(&mut self.families, id);
        self.families.extend_from_slice(&release);
        self.families_count += 1;
        self
    }

    pub(crate) fn surface_input_port(&mut self, id: u32) -> &mut Self {
        push_u32(&mut self.surface_input_ports, id);
        self.surface_input_ports_count += 1;
        self
    }

    pub(crate) fn opacity_input(&mut self, id: u32, value: f64) -> &mut Self {
        push_u32(&mut self.opacity_inputs, id);
        push_f64_bits(&mut self.opacity_inputs, value);
        self.opacity_inputs_count += 1;
        self
    }

    pub(crate) fn solid_paint(&mut self, id: u32, target: u32) -> &mut Self {
        push_u32(&mut self.paints, id);
        self.paints.push(1);
        push_u32(&mut self.paints, target);
        self.paints_count += 1;
        self
    }

    pub(crate) fn opacity_paint(&mut self, id: u32, source: u32, opacity: u32) -> &mut Self {
        push_u32(&mut self.paints, id);
        self.paints.push(2);
        push_u32(&mut self.paints, source);
        push_u32(&mut self.paints, opacity);
        self.paints_count += 1;
        self
    }

    pub(crate) fn input_surface(&mut self, id: u32, input: u32) -> &mut Self {
        push_u32(&mut self.surfaces, id);
        self.surfaces.push(1);
        push_u32(&mut self.surfaces, input);
        self.surfaces_count += 1;
        self
    }

    pub(crate) fn occurrence_surface(&mut self, id: u32, occurrence: u32) -> &mut Self {
        push_u32(&mut self.surfaces, id);
        self.surfaces.push(2);
        push_u32(&mut self.surfaces, occurrence);
        self.surfaces_count += 1;
        self
    }

    pub(crate) fn source_over_occurrence(
        &mut self,
        id: u32,
        subject: u32,
        against: u32,
        adapting_luminance: f64,
        background_ratio: f64,
        surround: u8,
    ) -> &mut Self {
        push_u32(&mut self.occurrences, id);
        push_u32(&mut self.occurrences, subject);
        push_u32(&mut self.occurrences, against);
        push_f64_bits(&mut self.occurrences, adapting_luminance);
        push_f64_bits(&mut self.occurrences, background_ratio);
        self.occurrences.push(surround);
        self.occurrences_count += 1;
        self
    }

    pub(crate) fn presentation_root(&mut self, id: u32, terminal: u32) -> &mut Self {
        push_u32(&mut self.presentation_roots, id);
        push_u32(&mut self.presentation_roots, terminal);
        self.presentation_roots_count += 1;
        self
    }

    pub(crate) fn presentation_target(&mut self, root: u32, occurrence: u32) -> &mut Self {
        push_u32(&mut self.presentation_targets, root);
        push_u32(&mut self.presentation_targets, occurrence);
        self.presentation_targets_count += 1;
        self
    }

    fn constraint_section(&mut self, hard: bool) -> &mut Vec<u8> {
        if hard {
            self.hard_constraints_count += 1;
            &mut self.hard_constraints
        } else {
            self.report_constraints_count += 1;
            &mut self.report_constraints
        }
    }

    pub(crate) fn exact_visible_unary(
        &mut self,
        hard: bool,
        id: u32,
        occurrence: u32,
        expected: Srgb8,
    ) -> &mut Self {
        let section = self.constraint_section(hard);
        push_u32(section, id);
        section.push(KIND_EXACT_VISIBLE_UNARY);
        push_u32(section, occurrence);
        section.extend_from_slice(&expected.bytes());
        self
    }

    pub(crate) fn wcag22_visible_unary(
        &mut self,
        hard: bool,
        id: u32,
        occurrence: u32,
        criterion: u8,
    ) -> &mut Self {
        let section = self.constraint_section(hard);
        push_u32(section, id);
        section.push(KIND_WCAG22_VISIBLE_UNARY);
        push_u32(section, occurrence);
        section.push(criterion);
        self
    }

    pub(crate) fn exact_intrinsic_relation_hard(
        &mut self,
        id: u32,
        reference: u32,
        candidates: &[u32],
    ) -> &mut Self {
        let section = self.constraint_section(true);
        push_u32(section, id);
        section.push(KIND_EXACT_INTRINSIC_RELATION);
        push_u32(section, reference);
        push_u32(section, u32::try_from(candidates.len()).unwrap_or(u32::MAX));
        for candidate in candidates {
            push_u32(section, *candidate);
        }
        self
    }

    #[cfg(test)]
    pub(crate) fn exact_visible_relation_hard(
        &mut self,
        id: u32,
        reference: u32,
        candidates: &[u32],
    ) -> &mut Self {
        let section = self.constraint_section(true);
        push_u32(section, id);
        section.push(KIND_EXACT_VISIBLE_RELATION);
        push_u32(section, reference);
        push_u32(section, candidates.len() as u32);
        for candidate in candidates {
            push_u32(section, *candidate);
        }
        self
    }

    pub(crate) fn output(&mut self, slot: u32, paint: u32) -> &mut Self {
        push_u32(&mut self.outputs, slot);
        push_u32(&mut self.outputs, paint);
        self.outputs_count += 1;
        self
    }

    /// Эмитирует единственные канонические байты объявленного графа.
    pub(crate) fn finish(self) -> Result<Vec<u8>, ProgramWireEncodeErrorV1> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&PROGRAM_WIRE_MAGIC_V1);
        bytes.extend_from_slice(&PROGRAM_WIRE_VERSION_V1.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());

        let sections: [(usize, &[u8]); 14] = [
            (self.sources_count, &self.sources),
            (self.targets_count, &self.targets),
            (self.families_count, &self.families),
            (0, &[]), // joint selection: РЅРµРїСЂРµРґСЃС‚Р°РІРёРј РЅР° wire v1 by design
            (self.surface_input_ports_count, &self.surface_input_ports),
            (self.opacity_inputs_count, &self.opacity_inputs),
            (self.paints_count, &self.paints),
            (self.surfaces_count, &self.surfaces),
            (self.occurrences_count, &self.occurrences),
            (self.presentation_roots_count, &self.presentation_roots),
            (self.presentation_targets_count, &self.presentation_targets),
            (self.hard_constraints_count, &self.hard_constraints),
            (self.report_constraints_count, &self.report_constraints),
            (self.outputs_count, &self.outputs),
        ];
        for (count, payload) in sections {
            let count = u32::try_from(count)
                .ok()
                .filter(|value| *value <= MAX_SECTION_ENTRIES_V1)
                .ok_or(ProgramWireEncodeErrorV1::TooManyEntries)?;
            bytes.extend_from_slice(&count.to_le_bytes());
            bytes.extend_from_slice(payload);
        }

        let len =
            u32::try_from(bytes.len()).map_err(|_| ProgramWireEncodeErrorV1::TooManyEntries)?;
        bytes[6..10].copy_from_slice(&len.to_le_bytes());
        Ok(bytes)
    }
}

#[cfg(kani)]
mod proofs;

#[cfg(test)]
mod assignment_v3_tests {
    use super::*;
    use crate::program_wire::{ProgramRuntimeErrorV1, ProgramScenarioV1, compile_program_wire};

    fn declaration(lower: u8, upper: u8, tolerance: f64) -> ProgramWireBuilderV1 {
        let mut builder = ProgramWireBuilderV1::new();
        builder
            .computed_gray_target(2, lower, upper, Srgb8::new([150; 3]), tolerance)
            .surface_input_port(6)
            .solid_paint(3, 2)
            .input_surface(7, 6)
            .source_over_occurrence(8, 3, 7, 64.0, 0.2, 1)
            .wcag22_visible_unary(true, 10, 8, 3)
            .output(17, 3);
        builder
    }

    #[test]
    fn v3_admission_owns_domains_tolerance_and_complete_preference_order() {
        for tolerance in [-1.0, f64::NAN, f64::INFINITY, f64::MAX] {
            let bytes = declaration(0, 255, tolerance)
                .finish_assignment(&[2])
                .expect("wire декларации кодируется");
            assert!(matches!(
                compile_program_wire(&bytes),
                Err(ProgramRuntimeErrorV1::Compile)
            ));
        }
        for order in [vec![], vec![77], vec![2, 2]] {
            let bytes = declaration(0, 255, 1.0)
                .finish_assignment(&order)
                .expect("wire декларации кодируется");
            assert!(matches!(
                compile_program_wire(&bytes),
                Err(ProgramRuntimeErrorV1::Compile)
            ));
        }
        let invalid_domain = declaration(200, 100, 1.0)
            .finish_assignment(&[2])
            .expect("wire декларации кодируется");
        assert!(matches!(
            compile_program_wire(&invalid_domain),
            Err(ProgramRuntimeErrorV1::Compile)
        ));
        let positive = declaration(0, 255, 0.0)
            .finish_assignment(&[2])
            .expect("wire декларации кодируется");
        let negative = declaration(0, 255, -0.0)
            .finish_assignment(&[2])
            .expect("wire декларации кодируется");
        assert_eq!(
            compile_program_wire(&positive)
                .expect("допустимый Program компилируется")
                .content_identity(),
            compile_program_wire(&negative)
                .expect("допустимый Program компилируется")
                .content_identity()
        );

        let mut mixed = declaration(0, 255, 1.0);
        mixed.finite_target(20, &[(21, Srgb8::new([50; 3]), 1.0)]);
        let bytes = mixed
            .finish_assignment(&[2])
            .expect("wire декларации кодируется");
        assert!(matches!(
            compile_program_wire(&bytes),
            Err(ProgramRuntimeErrorV1::Unsupported)
        ));
        assert!(
            compile_program_wire(
                &declaration(0, 255, 1.0)
                    .finish_assignment(&[2])
                    .expect("wire декларации кодируется")
            )
            .is_ok()
        );
    }

    #[test]
    fn v3_joint_assignment_uses_the_authored_preference_list() {
        let declaration = |order: &[u32]| {
            let mut builder = ProgramWireBuilderV1::new();
            builder
                .computed_gray_target(21, 0, 4, Srgb8::new([0; 3]), 1.0)
                .computed_gray_target(22, 0, 4, Srgb8::new([0; 3]), 1.0)
                .surface_input_port(31)
                .opacity_input(32, 0.5)
                .solid_paint(41, 21)
                .solid_paint(42, 22)
                .opacity_paint(43, 42, 32)
                .input_surface(51, 31)
                .source_over_occurrence(61, 41, 51, 64.0, 0.2, 1)
                .occurrence_surface(52, 61)
                .source_over_occurrence(62, 43, 52, 64.0, 0.2, 1)
                .exact_visible_unary(true, 81, 62, Srgb8::new([2; 3]))
                .output(91, 41)
                .output(92, 43);
            builder
                .finish_assignment(order)
                .expect("wire декларации кодируется")
        };
        let mut identities = Vec::new();
        for (order, expected) in [([21, 22], [0, 3]), ([22, 21], [3, 0])] {
            let compiled = compile_program_wire(&declaration(&order))
                .expect("допустимый Program компилируется");
            identities.push(compiled.content_identity());
            let mut session = compiled
                .instantiate(1)
                .expect("Session создаётся с допустимым stream");
            let snapshot = session
                .update_observed(1, &[ProgramScenarioV1::new(1, vec![Srgb8::new([0; 3])])])
                .expect("наблюдение принимается");
            for (slot, gray) in [(91, expected[0]), (92, expected[1])] {
                let output = snapshot
                    .outputs()
                    .iter()
                    .find(|output| output.slot() == slot)
                    .expect("объявленный output присутствует");
                assert_eq!(output.source(), Srgb8::new([gray; 3]));
            }
        }
        assert_ne!(identities[0], identities[1]);
    }

    #[test]
    fn v3_computed_declaration_reaches_the_existing_program_owner() {
        let mut builder = ProgramWireBuilderV1::new();
        builder
            .computed_gray_target(2, 0, 255, Srgb8::new([150; 3]), 1.0)
            .surface_input_port(6)
            .solid_paint(3, 2)
            .input_surface(7, 6)
            .source_over_occurrence(8, 3, 7, 64.0, 0.2, 1)
            .wcag22_visible_unary(true, 10, 8, 3)
            .output(17, 3);
        let bytes = builder
            .finish_assignment(&[2])
            .expect("wire декларации кодируется");
        assert!(decode_program_wire_v1(&bytes).is_err());
        assert!(crate::program_wire::compile_program_wire(&bytes).is_ok());
    }
}

#[cfg(test)]
mod selection_v2_tests {
    use super::*;
    use crate::program_wire::{
        ProgramRuntimeErrorV1, ProgramScenarioV1, ProgramSnapshotStateV1, check_program_wire,
        check_program_wire_v1, compile_program_wire, compile_program_wire_v1,
    };

    type State = (u32, Vec<(u32, u32)>);

    fn declaration(reverse: bool) -> ProgramWireBuilderV1 {
        let mut builder = ProgramWireBuilderV1::new();
        let mut candidates = vec![
            (201, Srgb8::new([180; 3]), 1.0),
            (202, Srgb8::new([80; 3]), 1.0),
            (203, Srgb8::new([0; 3]), 1.0),
        ];
        if reverse {
            candidates.reverse();
        }
        builder
            .finite_target(21, &candidates)
            .surface_input_port(31)
            .solid_paint(41, 21)
            .input_surface(51, 31)
            .source_over_occurrence(61, 41, 51, 64.0, 0.2, 1)
            .presentation_root(71, 61)
            .presentation_target(71, 61)
            .wcag22_visible_unary(true, 81, 61, 3)
            .output(91, 41);
        builder
    }

    fn finish(
        builder: ProgramWireBuilderV1,
        revision: u64,
        groups: &[Vec<State>],
    ) -> Result<Vec<u8>, String> {
        let start =
            10 + 12 + builder.sources.len() + builder.targets.len() + builder.families.len();
        let mut bytes = builder.finish().map_err(|error| format!("{error:?}"))?;
        let mut selection = Vec::new();
        push_u32(&mut selection, 1);
        selection.extend_from_slice(&revision.to_le_bytes());
        push_u32(&mut selection, groups.len() as u32);
        for group in groups {
            push_u32(&mut selection, group.len() as u32);
            for (id, choices) in group {
                push_u32(&mut selection, *id);
                push_u32(&mut selection, choices.len() as u32);
                for (target, candidate) in choices {
                    push_u32(&mut selection, *target);
                    push_u32(&mut selection, *candidate);
                }
            }
        }
        bytes.splice(start..start + 4, selection);
        bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
        let len = bytes.len() as u32;
        bytes[6..10].copy_from_slice(&len.to_le_bytes());
        Ok(bytes)
    }

    fn groups() -> Vec<Vec<State>> {
        vec![
            vec![(1, vec![(21, 201)])],
            vec![(2, vec![(21, 202)])],
            vec![(3, vec![(21, 203)])],
        ]
    }

    fn scenario(id: u32, level: u8) -> ProgramScenarioV1 {
        ProgramScenarioV1::new(id, vec![Srgb8::new([level; 3])])
    }

    #[test]
    fn finite_selection_is_reachable_through_public_program_compilation() -> Result<(), String> {
        let bytes = finish(declaration(false), 1, &groups())?;
        assert!(check_program_wire(&bytes).is_ok());
        assert!(
            check_program_wire_v1(&bytes).is_err(),
            "strict legacy entry must not reinterpret V2"
        );
        assert!(compile_program_wire_v1(&bytes).is_err());
        let mut runtime = compile_program_wire(&bytes)
            .map_err(|error| format!("{error:?}"))?
            .instantiate(1)
            .map_err(|error| format!("{error:?}"))?;
        let white = runtime
            .update_observed(1, &[scenario(1, 255)])
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(white.state(), ProgramSnapshotStateV1::Ready);
        assert_eq!(white.outputs()[0].source(), Srgb8::new([80; 3]));
        let paired = runtime
            .update_observed(2, &[scenario(1, 255), scenario(2, 100)])
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(paired.state(), ProgramSnapshotStateV1::Ready);
        assert_eq!(paired.outputs()[0].source(), Srgb8::new([0; 3]));
        let conflict = runtime
            .update_observed(3, &[scenario(1, 255), scenario(2, 0)])
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(conflict.state(), ProgramSnapshotStateV1::Failed);
        let restored = runtime
            .update_observed(4, &[scenario(1, 255)])
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(restored.state(), ProgramSnapshotStateV1::Ready);
        assert_eq!(restored.outputs()[0].source(), Srgb8::new([80; 3]));
        assert_eq!(
            white.outputs()[0].source(),
            Srgb8::new([80; 3]),
            "old snapshots are unchanged"
        );
        Ok(())
    }

    #[test]
    fn v1_finite_graph_stays_refused_without_an_admitted_selection() -> Result<(), String> {
        let bytes = declaration(false)
            .finish()
            .map_err(|error| format!("{error:?}"))?;
        assert!(matches!(
            compile_program_wire(&bytes),
            Err(ProgramRuntimeErrorV1::Compile)
        ));
        assert!(matches!(
            compile_program_wire_v1(&bytes),
            Err(ProgramRuntimeErrorV1::Compile)
        ));
        let mut selected = finish(declaration(false), 1, &groups())?;
        selected[4..6].copy_from_slice(&1u16.to_le_bytes());
        assert!(matches!(
            compile_program_wire(&selected),
            Err(ProgramRuntimeErrorV1::Wire)
        ));
        Ok(())
    }

    #[test]
    fn authored_group_order_is_policy_while_tie_and_declaration_permutations_are_not()
    -> Result<(), String> {
        let tied = vec![vec![
            (9, vec![(21, 201)]),
            (2, vec![(21, 202)]),
            (1, vec![(21, 203)]),
        ]];
        let reversed = vec![tied[0].iter().cloned().rev().collect::<Vec<_>>()];
        let a = finish(declaration(false), 7, &tied)?;
        let b = finish(declaration(true), 7, &reversed)?;
        assert_eq!(
            check_program_wire(&a).map_err(|error| format!("{error:?}"))?,
            check_program_wire(&b).map_err(|error| format!("{error:?}"))?
        );
        let mut runtime = compile_program_wire(&a)
            .map_err(|error| format!("{error:?}"))?
            .instantiate(1)
            .map_err(|error| format!("{error:?}"))?;
        let snapshot = runtime
            .update_observed(1, &[scenario(1, 255)])
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(snapshot.outputs()[0].source(), Srgb8::new([0; 3]));
        assert_ne!(
            check_program_wire(&a).map_err(|error| format!("{error:?}"))?,
            check_program_wire(&finish(declaration(false), 8, &tied)?)
                .map_err(|error| format!("{error:?}"))?
        );
        assert_ne!(
            check_program_wire(&a).map_err(|error| format!("{error:?}"))?,
            check_program_wire(&finish(declaration(false), 7, &groups())?)
                .map_err(|error| format!("{error:?}"))?
        );
        Ok(())
    }

    #[test]
    fn complete_cartesian_domain_and_declared_keys_are_admitted_by_the_existing_owners()
    -> Result<(), String> {
        let original = groups();
        let invalid = vec![
            vec![],
            vec![vec![]],
            vec![original[0].clone()],
            vec![
                original[0].clone(),
                original[1].clone(),
                original[0].clone(),
            ],
            vec![
                vec![(1, vec![(21, 201)])],
                vec![(1, vec![(21, 202)])],
                original[2].clone(),
            ],
            vec![vec![(1, vec![])], original[1].clone(), original[2].clone()],
            vec![
                vec![(1, vec![(99, 201)])],
                original[1].clone(),
                original[2].clone(),
            ],
            vec![
                vec![(1, vec![(21, 999)])],
                original[1].clone(),
                original[2].clone(),
            ],
            vec![
                vec![(1, vec![(21, 201), (21, 201)])],
                original[1].clone(),
                original[2].clone(),
            ],
        ];
        for invalid_groups in invalid {
            assert!(
                compile_program_wire(&finish(declaration(false), 1, &invalid_groups)?).is_err()
            );
        }
        assert!(compile_program_wire(&finish(declaration(false), 1, &original)?).is_ok());
        Ok(())
    }

    #[test]
    fn nested_release_counts_cannot_exceed_the_single_section_budget() -> Result<(), String> {
        let too_many = vec![vec![(
            1,
            vec![(21, 201); MAX_SECTION_ENTRIES_V1 as usize + 1],
        )]];
        assert!(matches!(
            decode_program_wire(&finish(declaration(false), 1, &too_many)?),
            Err(ProgramWireErrorV1::ResourceExhausted {
                section: WireSectionV1::JointSelection
            })
        ));
        let across_groups = vec![
            vec![(1, vec![(21, 201); 2048])],
            vec![(2, vec![(21, 202); 2049])],
        ];
        assert!(matches!(
            decode_program_wire(&finish(declaration(false), 1, &across_groups)?),
            Err(ProgramWireErrorV1::ResourceExhausted {
                section: WireSectionV1::JointSelection
            })
        ));
        Ok(())
    }

    #[test]
    fn truncated_selection_never_falls_back_to_unordered_candidates() -> Result<(), String> {
        let bytes = finish(declaration(false), u64::MAX, &groups())?;
        for end in 0..bytes.len() {
            let mut partial = bytes[..end].to_vec();
            if end >= 10 {
                partial[6..10].copy_from_slice(&(end as u32).to_le_bytes());
            }
            assert!(
                compile_program_wire(&partial).is_err(),
                "accepted truncated prefix {end}"
            );
        }
        assert!(compile_program_wire(&bytes).is_ok());
        Ok(())
    }
    #[test]
    fn finite_clean_set_keeps_the_same_final_composite_owner() -> Result<(), String> {
        let mut builder = ProgramWireBuilderV1::new();
        builder
            .finite_target(
                21,
                &[
                    (201, Srgb8::new([255; 3]), 0.5),
                    (202, Srgb8::new([128; 3]), 1.0),
                ],
            )
            .surface_input_port(31)
            .solid_paint(41, 21)
            .input_surface(51, 31)
            .source_over_occurrence(61, 41, 51, 64.0, 0.2, 1)
            .presentation_root(71, 61)
            .presentation_target(71, 61)
            .output(91, 41);
        builder.hard_constraints_count += 1;
        push_u32(&mut builder.hard_constraints, 81);
        builder.hard_constraints.push(KIND_CLEAN_SET);
        push_u32(&mut builder.hard_constraints, 71);
        push_u32(&mut builder.hard_constraints, 61);
        let ranks = vec![vec![(1, vec![(21, 201)])], vec![(2, vec![(21, 202)])]];
        let bytes = finish(builder, 1, &ranks)?;
        let draft = decode_program_wire(&bytes).map_err(|error| format!("{error:?}"))?;
        let _compiled = draft
            .compile()
            .map_err(|error| format!("finite clean-set compile failed: {error:?}"))?;
        let mut runtime = compile_program_wire(&bytes)
            .map_err(|error| format!("{error:?}"))?
            .instantiate(1)
            .map_err(|error| format!("{error:?}"))?;
        let first = runtime
            .update_observed(1, &[scenario(1, 1)])
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(first.state(), ProgramSnapshotStateV1::Ready);
        assert_eq!(first.outputs()[0].source(), Srgb8::new([255; 3]));
        let both = runtime
            .update_observed(
                2,
                &[
                    scenario(1, 1),
                    ProgramScenarioV1::new(2, vec![Srgb8::new([1, 1, 3])]),
                ],
            )
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(both.state(), ProgramSnapshotStateV1::Ready);
        assert_eq!(both.outputs()[0].source(), Srgb8::new([128; 3]));
        Ok(())
    }

    #[test]
    fn transport_matches_direct_release_materialisation_without_a_parallel_order()
    -> Result<(), String> {
        use crate::program_session::{
            JointCandidateStateV1, TargetCandidateChoiceV1, TargetCandidateId, TargetId,
        };
        use crate::selection_release::{
            SelectionCandidateKeyV1, SelectionReleaseV1, admit_selection_release_v1,
            materialise_joint_selection_v1,
        };
        let revision = 0x1020_3040_5060_7080;
        let key = |id: u32| SelectionCandidateKeyV1::new(Box::new(id.to_be_bytes()));
        let release = SelectionReleaseV1::new(
            revision,
            vec![
                vec![key(1)].into_boxed_slice(),
                vec![key(2)].into_boxed_slice(),
                vec![key(3)].into_boxed_slice(),
            ]
            .into_boxed_slice(),
        );
        let admitted = admit_selection_release_v1(release).map_err(|error| format!("{error:?}"))?;
        let states: Vec<_> = [(1, 201), (2, 202), (3, 203)]
            .into_iter()
            .map(|(id, candidate)| {
                (
                    JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(
                        TargetId::new(21),
                        TargetCandidateId::new(candidate),
                    )]),
                    key(id),
                )
            })
            .collect();
        let materialised = materialise_joint_selection_v1(&admitted, &states)
            .map_err(|error| format!("{error:?}"))?;
        let mut direct = decode_program_wire_v1(
            &declaration(false)
                .finish()
                .map_err(|error| format!("{error:?}"))?,
        )
        .map_err(|error| format!("{error:?}"))?;
        direct
            .set_materialised_joint_selection(materialised)
            .map_err(|error| format!("{error:?}"))?;
        let compiled = direct.compile().map_err(|error| format!("{error:?}"))?;
        let encoded = finish(declaration(false), revision, &groups())?;
        assert_eq!(
            check_program_wire(&encoded).map_err(|error| format!("{error:?}"))?,
            *compiled.content_identity().as_bytes()
        );
        Ok(())
    }

    #[test]
    fn every_single_byte_mutation_preserves_typed_totality_and_checker_agreement()
    -> Result<(), String> {
        let bytes = finish(declaration(false), 9, &groups())?;
        for offset in 0..bytes.len() {
            for mask in [1, 0x80, 0xff] {
                let mut mutated = bytes.clone();
                mutated[offset] ^= mask;
                let outcome = std::panic::catch_unwind(|| {
                    let checked = check_program_wire(&mutated);
                    let compiled = compile_program_wire(&mutated);
                    assert_eq!(checked.is_ok(), compiled.is_ok());
                    if let (Ok(identity), Ok(program)) = (checked, compiled) {
                        assert_eq!(identity, program.content_identity());
                    }
                });
                assert!(outcome.is_ok(), "byte {offset}, xor {mask}");
            }
        }
        Ok(())
    }
}
