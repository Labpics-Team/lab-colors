//! `@labpics/colors` — терминальная WASM-граница C7c.
//!
//! Один runtime-root: canonical Program wire → ProgramRuntime → ProgramSnapshot.
//! Legacy recipe engine (`LabColors.resolveTheme/loadConfig`) удалён. DOM/CSS
//! effects принадлежат npm-приложению, не WASM.

mod error;
mod program_diagnostics_projection;
mod program_report_projection;
mod raster_reference;
mod terminal_projection;

use std::cell::{Cell, RefCell};

use wasm_bindgen::prelude::*;

use crate::error::BindingError;

#[wasm_bindgen(typescript_custom_section)]
const TERMINAL_CAPABILITY_TYPES: &str = r#"
/** Proof-capable V2 site. Empty arrays explicitly mean no admitted evidence. */
export interface NumericalCapabilitySiteV2 {
  readonly siteId: string;
  readonly stableOutcomes: ReadonlyArray<string>;
  readonly compatibilityReleases: ReadonlyArray<string>;
  readonly evidenceClasses: ReadonlyArray<string>;
  readonly artifactIds: ReadonlyArray<string>;
  readonly boundIds: ReadonlyArray<string>;
  readonly proofIds: ReadonlyArray<string>;
  readonly runtimeAttestations: ReadonlyArray<string>;
}

/** Proof-capable numerical capability manifest. */
export interface NumericalCapabilityManifestV2 {
  readonly schemaVersion: 2;
  readonly coverage: string;
  readonly sites: ReadonlyArray<NumericalCapabilitySiteV2>;
  readonly checksum: string;
}

export type ProgramAssessmentVerdictV1 = "pass" | "violation";
export type ProgramDiagnosticSubjectV1 =
  | Readonly<{kind: "target"; targetId: number}>
  | Readonly<{kind: "occurrence"; occurrenceId: number}>
  | Readonly<{kind: "intrinsic-relation"; referenceTargetId: number}>
  | Readonly<{kind: "visible-relation"; referenceOccurrenceId: number}>
  | Readonly<{kind: "computed-visible-relation"}>
  | Readonly<{kind: "presentation"; rootId: number; occurrenceId: number; terminalId: number}>;
/** Local support index of the input on which the checked value depends. */
export type ProgramFieldCaseV1 = Readonly<{inputIndex: number; caseIndex: number}>;
export type ProgramDiagnosticV1 =
  | Readonly<{kind: "tonal-hierarchy"; constraintId: number; caseIndex: number;
      fieldCase: ProgramFieldCaseV1 | null;
      strongerOccurrenceId: number; weakerOccurrenceId: number; requiredRatio: number; actualRatio: number;
      requiredGap: number; actualGap: number;
      strongerPolarity: "darker" | "equal" | "lighter"; weakerPolarity: "darker" | "equal" | "lighter"}>
  | Readonly<{kind: "hard-constraint"; constraintId: number; caseIndex: number; fieldCase: ProgramFieldCaseV1 | null; subject: ProgramDiagnosticSubjectV1}>
  | Readonly<{kind: "target-domain"; targetId: number; source: readonly [number, number, number]}>
  | Readonly<{kind: "color-identity"; targetId: number; caseIndex: number; occurrenceId: number | null;
      fieldCase: ProgramFieldCaseV1 | null;
      actual: readonly [number, number, number]}>
  | Readonly<{kind: "max-change"; targetId: number; caseIndex: number; occurrenceId: number | null;
      fieldCase: ProgramFieldCaseV1 | null;
      actual: readonly [number, number, number]; distanceSquared: number; maximumDistance: number}>;
export interface ProgramAttemptWitnessV1 {
  readonly kind: "authored-tuple";
  readonly caseScenarioIds: ReadonlyArray<ReadonlyArray<number>>;
  readonly targets: ReadonlyArray<Readonly<{targetId: number; source: readonly [number, number, number]}>>;
  readonly violations: ReadonlyArray<ProgramDiagnosticV1>;
}
/** Данные текущего отказа. Witness относится к одному attempt, не к minimal unsat core. */
export interface ProgramDiagnosticsV1 {
  readonly searchVisited: bigint;
  readonly searchBudget: bigint;
  readonly witness: ProgramAttemptWitnessV1 | null;
}
export type ProgramConstraintAssessmentV1 =
  | Readonly<{
      kind: "visible-separation"; verdict: ProgramAssessmentVerdictV1;
      visible: readonly [number, number, number];
      backdrop: readonly [number, number, number];
      distance: number; distanceBounds: readonly [number, number]; minimum: number; maximum: number;
    }>
  | Readonly<{
      kind: "visible-pair-separation"; verdict: ProgramAssessmentVerdictV1;
      left: readonly [number, number, number]; right: readonly [number, number, number];
      distance: number; distanceBounds: readonly [number, number]; minimum: number; maximum: number;
    }>
  | Readonly<{
      kind: "exact-srgb8" | "wcag22-srgb8" | "intrinsic-unary" | "relation" | "tonal-hierarchy";
      verdict: ProgramAssessmentVerdictV1;
    }>
  | Readonly<{
      kind: "declared-srgb8-clean-set" | "nominal-srgb8-visible"; verdict: "pass";
      visible: readonly [number, number, number];
    }>
  | Readonly<{
      kind: "declared-srgb8-clean-set" | "nominal-srgb8-visible"; verdict: "violation"; reason: "rejected";
      visible: readonly [number, number, number];
      rejectedBlueInterval: readonly [number, number];
    }>
  | Readonly<{
      kind: "declared-srgb8-clean-set"; verdict: "violation";
      reason: "final-owned-domain-absent";
    }>;
export interface ProgramAssessmentCellV1 {
  readonly constraintId: number;
  readonly caseIndex: number;
  readonly hard: boolean;
  readonly assessment: ProgramConstraintAssessmentV1;
}
export interface ProgramScenarioProvenanceV1 {
  readonly caseIndex: number;
  readonly scenarioId: number;
}
/** Соответствие ограниченному профилю; не scalar score или человеческая оценка поля. */
export type ProgramCleanlinessAssessmentV1 =
  | Readonly<{status: "not-assessed"; reason: "no-declared-profile"}>
  | Readonly<{
      status: "conforms" | "violates" | "unavailable";
      profile: "nominal-srgb8-v2";
      basis: "declared-convention";
      scope: "declared-final-points";
      /** Количества клеток case × constraint, не пикселей или quality points. */
      accepted: number; rejected: number; unavailable: number;
    }>;
export interface ProgramTonalHierarchyPairV1 {
  readonly constraintId: number;
  readonly caseIndex: number;
  readonly strongerOccurrenceId: number;
  readonly weakerOccurrenceId: number;
  /** Отношение CRstrong / CRweak; сравниваются эти значения. */
  readonly requiredRatio: number;
  readonly actualRatio: number;
  /** Натуральный логарифм соответствующего отношения, только для диагностики. */
  readonly requiredGap: number;
  readonly actualGap: number;
  readonly strongerPolarity: "darker" | "equal" | "lighter";
  readonly weakerPolarity: "darker" | "equal" | "lighter";
  readonly satisfied: boolean;
}
export interface ProgramReferenceCapacityV1 {
  /** Совместная достижимость для всех cases при сохранённых более приоритетных paints. */
  readonly ratioLower: number;
  readonly ratioUpper: number;
  /** При ratio=1 означает достижимость исходной силы, а не максимальный CR. */
  readonly complete: boolean;
}
export interface ProgramReferenceCaseV1 {
  readonly caseIndex: number;
  readonly referenceVisible: readonly [number, number, number];
  readonly referenceBackdrop: readonly [number, number, number];
  readonly actualVisible: readonly [number, number, number];
  readonly actualBackdrop: readonly [number, number, number];
  readonly referenceContrast: number;
  readonly actualContrast: number;
  readonly polarity: "darker" | "equal" | "lighter";
  /** Положительная часть ln(referenceContrast/actualContrast), не общий quality score. */
  readonly strengthLoss: number;
  readonly virtualDistanceSquared: number;
}
export interface ProgramReferenceSelectionV1 {
  readonly targetId: number;
  readonly occurrenceId: number;
  readonly preference: "reference-strength" | "explicit-tone";
  readonly capacity: ProgramReferenceCapacityV1 | null;
  readonly cases: ReadonlyArray<ProgramReferenceCaseV1>;
}

export interface ProgramSelectionComponentV1 {
  readonly mode: "ordered-v1" | "joint-point-v1" | "ordered-reference-v1";
  readonly targets: ReadonlyArray<number>;
}

/** Scope of a declared model; unknown rendering is not a pixel guarantee. */
export interface ProgramFieldAssessmentV1 {
  readonly inputIndex: number;
  readonly from: readonly [number, number, number];
  readonly to: readonly [number, number, number];
  readonly sourceModel: "opaque-affine-encoded-srgb8-half-up-v1" | "opaque-piecewise-affine-encoded-srgb8-half-up-v1";
  readonly stops?: ReadonlyArray<{readonly at: number; readonly color: readonly [number, number, number]}>;
  readonly domain: "whole-field";
  readonly rendering: "unknown";
  readonly regionCount: number;
  readonly witnesses: ReadonlyArray<{
    readonly segmentIndex?: number;
    readonly coordinate: readonly [number, number];
    readonly nominal: readonly [number, number, number];
  }>;
}
/** Историческое свидетельство одного Ready-снимка; без права публикации. */
export interface ProgramAssessmentReportV1 {
  /** Независимая копия bytes; её изменение не меняет Core-отчёт. */
  readonly contentIdentity: Uint8Array;
  readonly streamId: number;
  readonly revision: bigint;
  readonly scenarioProvenance: ReadonlyArray<ProgramScenarioProvenanceV1>;
  readonly cells: ReadonlyArray<ProgramAssessmentCellV1>;
  readonly cleanliness: ProgramCleanlinessAssessmentV1;
  readonly tonalHierarchy: ReadonlyArray<ProgramTonalHierarchyPairV1>;
  readonly referenceSelections: ReadonlyArray<ProgramReferenceSelectionV1>;
  readonly selectionComponents: ReadonlyArray<ProgramSelectionComponentV1>;
}

export type Wcag22CriterionV1 =
  | "sc-1.4.3-text-default"
  | "sc-1.4.3-text-large-scale"
  | "sc-1.4.11-ui-component-or-state"
  | "sc-1.4.11-graphical-object";
export type Wcag22DecisionV1 = "pass" | "fail";
export interface Wcag22Q55BoundsV1 {
  /** Decimal u64 string: Q55 values exceed JavaScript's safe integer range. */
  readonly lower: string;
  readonly upper: string;
}
export interface Wcag22AssessmentV1 {
  readonly kind: "evaluated";
  readonly profileId: "wcag22-srgb8-contrast-v1";
  readonly criterion: Wcag22CriterionV1;
  readonly foreground: string;
  readonly background: string;
  readonly foregroundLuminanceQ55: Wcag22Q55BoundsV1;
  readonly backgroundLuminanceQ55: Wcag22Q55BoundsV1;
  readonly q55Scale: string;
  readonly decision: Wcag22DecisionV1;
  readonly evidence: {
    readonly kind: "canonical-finite-bounded";
    readonly artifactId: "wcag22-srgb8-luminance-q55-v1";
    readonly artifactSha256: string;
    readonly boundId: "wcag22-srgb8-outward-q55-v1";
    readonly proofId: "wcag22-srgb8-full-domain-q55-v1";
    readonly proofSha256: string;
    readonly proofPayloadSha256: string;
    readonly generatorSha256: string;
    readonly verifierSha256: string;
    readonly profileChecksum: string;
    readonly profileSha256: string;
  };
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "NumericalCapabilityManifestV2")]
    pub type JsNumericalCapabilityManifestV2;

    #[wasm_bindgen(typescript_type = "Wcag22AssessmentV1")]
    pub type JsWcag22AssessmentV1;
}
// Приватные индексы JS: to_program_js_error — 0..7, to_attachment_error — 8..14; оба сопоставления Rust, таблица/граница и program_error_projection_distinguishes_every_runtime_failure_class меняются согласованно.
#[wasm_bindgen(inline_js = r#"
export function programError(message, code, operation) {
  try {
    const error = new Error(message);
    error.code = code;
    error.operation = operation;
    return error;
  } catch {
    return new Error("Program error projection failed");
  }
}

const programFailureCodes = [
  "program_wire",
  "program_compile",
  "program_family_artifacts_required",
  "program_instantiate",
  "program_update",
  "program_resource_exhausted",
  "program_internal_invariant",
  "program_runtime",
  "program_attachment_binding",
  "program_attachment_instantiate",
  "program_attachment_sink_admission",
  "program_attachment_resource_exhausted",
  "program_attachment_internal_invariant",
  "program_attachment_non_terminal_target",
  "program_attachment",
];

export function programFailure(kind, operation) {
  return programError(
    kind < 8 ? "Program runtime operation failed" : "Program attachment admission failed",
    programFailureCodes[kind],
    operation,
  );
}

const bindingFailures = [
  ["invalid_color", "evaluateWcag22"],
  ["unknown_wcag22_criterion", "evaluateWcag22"],
  ["internal_error", "evaluateWcag22"],
  ["internal_error", "numericalCapabilityManifest"],
  ["invalid_output_index", "readProgramOutput"],
];

export function bindingFailure(kind, reason) {
  const [code, operation] = bindingFailures[kind];
  return programError(code + ": " + reason, code, operation);
}

const OutputRgbArray = Uint8Array;
const OutputTableArray = Float64Array;

export function copyOutputRgb(red, green, blue) {
  const bytes = new OutputRgbArray(3);
  bytes[0] = red;
  bytes[1] = green;
  bytes[2] = blue;
  return bytes;
}

export function copyOutputTable(values) {
  return new OutputTableArray(values);
}

export function certificateError(code, operation) {
  return programError(
    "Certificate envelope operation failed",
    `certificate_${code}`,
    operation,
  );
}

export function unsupportedPhysicalIdentityError() {
  return programError(
    "Unsupported program physical identity",
    "program_physical_identity",
    "physicalIdentity",
  );
}

export function certificateProjection(
  schemaVersion,
  operation,
  authorityKind,
  authorityVersion,
  runtimeArtifactId,
  producerRevision,
  producerContentIdentity,
  contextId,
  payloadType,
  payloadVersion,
  payloadLength,
  payloadSha256,
  bindingSha256,
) {
  return Object.freeze({
    schemaVersion,
    operation,
    authorityKind,
    authorityVersion,
    runtimeArtifactId,
    producerRevision,
    producerContentIdentity: Uint8Array.from(producerContentIdentity),
    contextId,
    payloadType,
    payloadVersion,
    payloadLength,
    payloadSha256: Uint8Array.from(payloadSha256),
    bindingSha256: Uint8Array.from(bindingSha256),
  });
}
const reportFreeze = Object.freeze;
const ReportBytes = Uint8Array;
const reportPush = Array.prototype.push;
export function reportArray() { return []; }
export function reportArrayFreeze(array) { return reportFreeze(array); }
export function reportArrayPush(array, value) { reportPush.call(array, value); }
export function reportScenario(caseIndex, scenarioId) {
  return reportFreeze({ caseIndex, scenarioId });
}
export function reportSummary(kind, verdict) { return reportFreeze({ kind, verdict }); }
export function diagnosticSubject(kind, first, second, third) {
  if (kind === "target") return reportFreeze({kind, targetId: first});
  if (kind === "occurrence") return reportFreeze({kind, occurrenceId: first});
  if (kind === "intrinsic-relation") return reportFreeze({kind, referenceTargetId: first});
  if (kind === "visible-relation") return reportFreeze({kind, referenceOccurrenceId: first});
  if (kind === "computed-visible-relation") return reportFreeze({kind});
  return reportFreeze({kind, rootId: first, occurrenceId: second, terminalId: third});
}
export function diagnosticFieldCase(inputIndex, caseIndex) {
  return reportFreeze({inputIndex, caseIndex});
}
export function diagnosticConstraint(constraintId, caseIndex, fieldCase, subject) {
  return reportFreeze({kind: "hard-constraint", constraintId, caseIndex, fieldCase, subject});
}
export function diagnosticTonalHierarchy(constraintId, caseIndex, fieldCase, strongerOccurrenceId, weakerOccurrenceId,
    requiredRatio, actualRatio, requiredGap, actualGap, strongerPolarity, weakerPolarity) {
  return reportFreeze({kind: "tonal-hierarchy", constraintId, caseIndex, fieldCase, strongerOccurrenceId, weakerOccurrenceId,
    requiredRatio, actualRatio, requiredGap, actualGap, strongerPolarity, weakerPolarity});
}
export function diagnosticDomain(targetId, red, green, blue) {
  return reportFreeze({kind: "target-domain", targetId, source: reportFreeze([red, green, blue])});
}
export function diagnosticColor(kind, targetId, caseIndex, fieldCase, occurrenceId, red, green, blue, distanceSquared, maximumDistance) {
  const base = {kind, targetId, caseIndex, fieldCase, occurrenceId: occurrenceId ?? null, actual: reportFreeze([red, green, blue])};
  if (kind === "max-change") { base.distanceSquared = distanceSquared; base.maximumDistance = maximumDistance; }
  return reportFreeze(base);
}
export function diagnosticTarget(targetId, red, green, blue) {
  return reportFreeze({targetId, source: reportFreeze([red, green, blue])});
}
export function diagnosticWitness(caseScenarioIds, targets, violations) {
  for (let index = 0; index < caseScenarioIds.length; index += 1) reportFreeze(caseScenarioIds[index]);
  return reportFreeze({kind: "authored-tuple", caseScenarioIds: reportFreeze(caseScenarioIds),
    targets: reportFreeze(targets), violations: reportFreeze(violations)});
}
export function diagnosticsProjection(searchVisited, searchBudget, witness) {
  return reportFreeze({searchVisited, searchBudget, witness});
}
export function reportSeparation(pair, verdict, vr, vg, vb, br, bg, bb, distance, distanceLower, distanceUpper, minimum, maximum) {
  const left = reportFreeze([vr, vg, vb]), right = reportFreeze([br, bg, bb]);
  return reportFreeze({
    ...(pair ? {kind: "visible-pair-separation", left, right} : {kind: "visible-separation", visible: left, backdrop: right}), verdict,
    distance, distanceBounds: reportFreeze([distanceLower, distanceUpper]), minimum, maximum,
  });
}
export function reportAccepted(kind, red, green, blue) {
  return reportFreeze({
    kind, verdict: "pass",
    visible: reportFreeze([red, green, blue]),
  });
}
export function reportRejected(kind, red, green, blue, lower, upper) {
  return reportFreeze({
    kind, verdict: "violation", reason: "rejected",
    visible: reportFreeze([red, green, blue]),
    rejectedBlueInterval: reportFreeze([lower, upper]),
  });
}
export function reportDomainAbsent() {
  return reportFreeze({
    kind: "declared-srgb8-clean-set", verdict: "violation",
    reason: "final-owned-domain-absent",
  });
}
export function reportCell(constraintId, caseIndex, hard, assessment) {
  return reportFreeze({ constraintId, caseIndex, hard, assessment });
}
export function reportCleanliness(status, profile, basis, scope, accepted, rejected, unavailable) {
  if (status === "not-assessed") return reportFreeze({status, reason: "no-declared-profile"});
  return reportFreeze({status, profile, basis, scope, accepted, rejected, unavailable});
}
export function reportTonalHierarchy(constraintId, caseIndex, strongerOccurrenceId, weakerOccurrenceId,
    requiredRatio, actualRatio, requiredGap, actualGap, strongerPolarity, weakerPolarity, satisfied) {
  return reportFreeze({constraintId, caseIndex, strongerOccurrenceId, weakerOccurrenceId,
    requiredRatio, actualRatio, requiredGap, actualGap, strongerPolarity, weakerPolarity, satisfied});
}
export function reportReferenceCapacity(ratioLower, ratioUpper, complete) {
  return reportFreeze({ratioLower, ratioUpper, complete});
}
export function reportGradientStop(at, red, green, blue) {
  return reportFreeze({at, color: reportFreeze([red, green, blue])});
}
export function reportFieldWitness(segmentIndex, numerator, denominator, red, green, blue) {
  return reportFreeze({...(segmentIndex === undefined ? {} : {segmentIndex}), coordinate: reportFreeze([numerator, denominator]),
    nominal: reportFreeze([red, green, blue])});
}
export function reportField(inputIndex, caseOffset, colors, sourceModel, domain, rendering, regionCount, witnesses, stops) {
  return reportFreeze({inputIndex,
    caseOffset,
    from: reportFreeze([colors[0], colors[1], colors[2]]),
    to: reportFreeze([colors[3], colors[4], colors[5]]),
    sourceModel, domain, rendering, regionCount, witnesses: reportFreeze(witnesses),
    ...(stops === undefined ? {} : {stops: reportFreeze(stops)})});
}
export function reportReferenceCase(caseIndex, colors, referenceContrast, actualContrast,
    polarity, strengthLoss, virtualDistanceSquared) {
  return reportFreeze({caseIndex,
    referenceVisible: reportFreeze([colors[0], colors[1], colors[2]]),
    referenceBackdrop: reportFreeze([colors[3], colors[4], colors[5]]),
    actualVisible: reportFreeze([colors[6], colors[7], colors[8]]),
    actualBackdrop: reportFreeze([colors[9], colors[10], colors[11]]),
    referenceContrast, actualContrast, polarity, strengthLoss, virtualDistanceSquared});
}
export function reportReferenceSelection(targetId, occurrenceId, preference, capacity, cases) {
  return reportFreeze({targetId, occurrenceId, preference, capacity, cases: reportFreeze(cases)});
}
export function reportSelectionComponent(mode, targets) {
  const copy = [];
  for (let i = 0; i < targets.length; i++) reportPush.call(copy, targets[i]);
  return reportFreeze({mode, targets: reportFreeze(copy)});
}
export function reportProjection(contentIdentity, streamId, revision, scenarioProvenance, cells, cleanliness, tonalHierarchy, referenceSelections, selectionComponents) {
  return reportFreeze({
    contentIdentity: new ReportBytes(contentIdentity),
    streamId, revision,
    scenarioProvenance: reportFreeze(scenarioProvenance),
    cells: reportFreeze(cells),
    cleanliness,
    tonalHierarchy: reportFreeze(tonalHierarchy),
    referenceSelections: reportFreeze(referenceSelections),
    selectionComponents: reportFreeze(selectionComponents),
  });
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = programError)]
    fn program_error(message: &str, code: &str, operation: &str) -> js_sys::Error;

    #[wasm_bindgen(js_name = programFailure)]
    fn program_failure_error(kind: u32, operation: &str) -> js_sys::Error;

    #[wasm_bindgen(js_name = bindingFailure)]
    fn binding_failure_error(kind: u32, reason: &str) -> js_sys::Error;

    #[wasm_bindgen(catch, js_name = copyOutputRgb)]
    fn copy_output_rgb(red: u8, green: u8, blue: u8) -> Result<js_sys::Uint8Array, JsValue>;

    #[wasm_bindgen(catch, js_name = copyOutputTable)]
    fn copy_output_table_to_js(values: &[f64]) -> Result<js_sys::Float64Array, JsValue>;

    #[wasm_bindgen(js_name = certificateError)]
    fn certificate_error(code: &str, operation: &str) -> js_sys::Error;

    #[wasm_bindgen(js_name = unsupportedPhysicalIdentityError)]
    fn unsupported_physical_identity_error() -> js_sys::Error;
    #[wasm_bindgen(js_name = certificateProjection)]
    fn certificate_projection(
        schema_version: u16,
        operation: &str,
        authority_kind: &str,
        authority_version: u16,
        runtime_artifact_id: &str,
        producer_revision: &str,
        producer_content_identity: &[u8],
        context_id: &str,
        payload_type: &str,
        payload_version: u16,
        payload_length: u32,
        payload_sha256: &[u8],
        binding_sha256: &[u8],
    ) -> JsValue;
    #[wasm_bindgen(catch, js_name = reportArray)]
    fn report_array() -> Result<js_sys::Array, JsValue>;
    #[wasm_bindgen(catch, js_name = reportArrayFreeze)]
    fn report_array_freeze(array: &js_sys::Array) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportArrayPush)]
    fn report_array_push(array: &js_sys::Array, value: &JsValue) -> Result<(), JsValue>;
    #[wasm_bindgen(catch, js_name = reportGradientStop)]
    fn report_gradient_stop(at: f64, red: u8, green: u8, blue: u8) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportFieldWitness)]
    fn report_field_witness(
        segment_index: Option<u32>,
        numerator: u16,
        denominator: u16,
        red: u8,
        green: u8,
        blue: u8,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportField)]
    fn report_field(
        input_index: usize,
        case_offset: usize,
        colors: &[u8],
        source_model: &str,
        domain: &str,
        rendering: &str,
        region_count: usize,
        witnesses: &js_sys::Array,
        stops: Option<js_sys::Array>,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportScenario)]
    fn report_scenario(case_index: usize, scenario_id: u32) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportSummary)]
    fn report_summary(kind: &str, verdict: &str) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = diagnosticSubject)]
    fn diagnostic_subject(
        kind: &str,
        first: u32,
        second: u32,
        third: u32,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = diagnosticFieldCase)]
    fn diagnostic_field_case(input: usize, case: usize) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = diagnosticConstraint)]
    fn diagnostic_constraint(
        constraint: u32,
        case: usize,
        field_case: &JsValue,
        subject: &JsValue,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = diagnosticTonalHierarchy)]
    fn diagnostic_tonal_hierarchy(
        constraint: u32,
        case: usize,
        field_case: &JsValue,
        stronger_occurrence: u32,
        weaker_occurrence: u32,
        required_ratio: f64,
        actual_ratio: f64,
        required_gap: f64,
        actual_gap: f64,
        stronger_polarity: &str,
        weaker_polarity: &str,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = diagnosticDomain)]
    fn diagnostic_domain(target: u32, red: u8, green: u8, blue: u8) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = diagnosticColor)]
    fn diagnostic_color(
        kind: &str,
        target: u32,
        case: usize,
        field_case: &JsValue,
        occurrence: Option<u32>,
        red: u8,
        green: u8,
        blue: u8,
        distance_squared: f64,
        maximum_distance: f64,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = diagnosticTarget)]
    fn diagnostic_target(target: u32, red: u8, green: u8, blue: u8) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = diagnosticWitness)]
    fn diagnostic_witness(
        cases: &js_sys::Array,
        targets: &js_sys::Array,
        violations: &js_sys::Array,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = diagnosticsProjection)]
    fn diagnostics_projection(
        visited: u64,
        budget: u64,
        witness: &JsValue,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportSeparation)]
    fn report_separation(
        pair: bool,
        verdict: &str,
        visible_red: u8,
        visible_green: u8,
        visible_blue: u8,
        backdrop_red: u8,
        backdrop_green: u8,
        backdrop_blue: u8,
        distance: f64,
        distance_lower: f64,
        distance_upper: f64,
        minimum: f64,
        maximum: f64,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportAccepted)]
    fn report_accepted(kind: &str, red: u8, green: u8, blue: u8) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportRejected)]
    fn report_rejected(
        kind: &str,
        red: u8,
        green: u8,
        blue: u8,
        lower: u8,
        upper: u8,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportDomainAbsent)]
    fn report_domain_absent() -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportCell)]
    fn report_cell(
        constraint_id: u32,
        case_index: usize,
        hard: bool,
        assessment: &JsValue,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportCleanliness)]
    fn report_cleanliness(
        status: &str,
        profile: &str,
        basis: &str,
        scope: &str,
        accepted: usize,
        rejected: usize,
        unavailable: usize,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportProjection)]
    fn report_projection(
        content_identity: &[u8],
        stream_id: u32,
        revision: u64,
        scenario_provenance: &js_sys::Array,
        cells: &js_sys::Array,
        cleanliness: &JsValue,
        tonal_hierarchy: &js_sys::Array,
        reference_selections: &js_sys::Array,
        selection_components: &js_sys::Array,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportTonalHierarchy)]
    fn report_tonal_hierarchy(
        constraint: u32,
        case: usize,
        stronger_occurrence: u32,
        weaker_occurrence: u32,
        required_ratio: f64,
        actual_ratio: f64,
        required_gap: f64,
        actual_gap: f64,
        stronger_polarity: &str,
        weaker_polarity: &str,
        satisfied: bool,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportReferenceCapacity)]
    fn report_reference_capacity(
        ratio_lower: f64,
        ratio_upper: f64,
        complete: bool,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportReferenceCase)]
    fn report_reference_case(
        case_index: usize,
        colors: &[u8],
        reference_contrast: f64,
        actual_contrast: f64,
        polarity: &str,
        strength_loss: f64,
        virtual_distance_squared: f64,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportReferenceSelection)]
    fn report_reference_selection(
        target_id: u32,
        occurrence_id: u32,
        preference: &str,
        capacity: &JsValue,
        cases: &js_sys::Array,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reportSelectionComponent)]
    fn report_selection_component(mode: &str, targets: &[u32]) -> Result<JsValue, JsValue>;
}

fn to_certificate_js_error(error: labcolors_core::certificate::CertificateErrorV1) -> JsValue {
    // Форматирование принадлежит существующей JS-проекции: не создаём временную
    // Rust String для уже закрытого статического Core-кода.
    certificate_error(error.code(), "decodeCertificateEnvelope").into()
}

fn to_certificate_producer_js_error(
    error: labcolors_core::certificate::CertificateProducerErrorV1,
) -> JsValue {
    certificate_error(error.code(), "issueSourceCertificateEnvelope").into()
}

// Локальные теги единственной JS-проекции отказов оценки и чтения.
#[repr(u32)]
enum BindingFailure {
    InvalidColor,
    UnknownWcag22Criterion,
    Wcag22Internal,
    ManifestInternal,
    OutputIndex,
}

fn to_js_error(error: BindingError) -> JsValue {
    let (kind, reason) = match &error {
        BindingError::InvalidColor { reason } => (BindingFailure::InvalidColor, reason.as_str()),
        BindingError::UnknownWcag22Criterion { requested } => {
            (BindingFailure::UnknownWcag22Criterion, requested.as_str())
        }
        BindingError::Internal { reason } => (BindingFailure::Wcag22Internal, reason.as_str()),
    };
    binding_failure_error(kind as u32, reason).into()
}

// Одна ошибка чтения сохраняет оба snapshot-владельца и прежний результат.
#[inline(never)]
fn output_index_error() -> JsValue {
    binding_failure_error(
        BindingFailure::OutputIndex as u32,
        "program output index must be a u32 within snapshot bounds",
    )
    .into()
}

fn copy_snapshot_outputs(
    outputs: &[labcolors_core::program_wire::ProgramPaintOutputV1],
) -> Result<js_sys::Float64Array, JsValue> {
    let exhausted = || {
        to_program_js_error(
            labcolors_core::program_wire::ProgramRuntimeErrorV1::ResourceExhausted,
            ProgramOperation::CopyOutputTable,
        )
    };
    let length = outputs.len().checked_mul(5).ok_or_else(exhausted)?;
    let mut table = Vec::new();
    table.try_reserve_exact(length).map_err(|_| exhausted())?;
    for output in outputs {
        let [red, green, blue] = output.source().bytes();
        table.extend_from_slice(&[
            f64::from(output.slot()),
            f64::from(red),
            f64::from(green),
            f64::from(blue),
            output.opacity(),
        ]);
    }
    // JS заимствует таблицу только на время синхронной копии. Rust сохраняет
    // владение и освобождает её даже при отказе JS, до проекции ошибки.
    // u32/u8 точно представимы в f64; opacity переносится без округления.
    let copied = copy_output_table_to_js(&table);
    drop(table);
    copied.map_err(|_| exhausted())
}

#[derive(Clone, Copy)]
enum ProgramOperation {
    Diagnostics,
    CompileProgramWire,
    UpdateObserved,
    UpdateDeclaredField,
    UpdateDeclaredFieldWith,
    UpdateGradient,
    UpdateGradientWith,
    UpdateIndependentGradients,
    UpdateIndependentGradientsWith,
    UpdateObservedWith,
    UpdateUnknown,
    CopyOutputTable,
    AttachProgramWire,
    AttachmentUpdateObserved,
    AttachmentUpdateUnknown,
    AttachmentDispose,
    MaterializationAuthority,
    AssessmentReport,
    CleanlinessAssessment,
    ReferenceSelections,
    SelectionComponents,
    FieldAssessment,
}

impl ProgramOperation {
    const fn key(self) -> &'static str {
        match self {
            Self::Diagnostics => "diagnostics",
            Self::CompileProgramWire => "compileProgramWire",
            Self::UpdateObserved => "updateObserved",
            Self::UpdateDeclaredField => "updateDeclaredField",
            Self::UpdateDeclaredFieldWith => "updateDeclaredFieldWith",
            Self::UpdateGradient => "updateGradient",
            Self::UpdateGradientWith => "updateGradientWith",
            Self::UpdateIndependentGradients => "updateIndependentGradients",
            Self::UpdateIndependentGradientsWith => "updateIndependentGradientsWith",
            Self::UpdateObservedWith => "updateObservedWith",
            Self::UpdateUnknown => "updateUnknown",
            Self::CopyOutputTable => "copyOutputTable",
            Self::AttachProgramWire => "attachProgramWire",
            Self::AttachmentUpdateObserved => "attachmentUpdateObserved",
            Self::AttachmentUpdateUnknown => "attachmentUpdateUnknown",
            Self::AttachmentDispose => "attachmentDispose",
            Self::MaterializationAuthority => "materializationAuthority",
            Self::AssessmentReport => "assessmentReport",
            Self::CleanlinessAssessment => "cleanlinessAssessment",
            Self::ReferenceSelections => "referenceSelections",
            Self::SelectionComponents => "selectionComponents",
            Self::FieldAssessment => "fieldAssessment",
        }
    }
}

fn to_program_js_error(
    error: labcolors_core::program_wire::ProgramRuntimeErrorV1,
    operation: ProgramOperation,
) -> JsValue {
    use labcolors_core::program_wire::ProgramRuntimeErrorV1 as E;
    // Закрытые private-теги этой JS-проекции, не discriminants Core enum и не
    // новый public ABI. Browser-тест сверяет каждый tag с прежним строковым кодом.
    let kind = match error {
        E::Wire => 0,
        E::Compile => 1,
        E::FamilyArtifactsRequired => 2,
        E::Instantiate => 3,
        E::FieldCorrelationRequired => {
            return program_error(
                "The declared gradients require a joint spatial observation",
                "program_field_correlation_required",
                operation.key(),
            )
            .into();
        }
        E::Update => 4,
        E::ResourceExhausted => 5,
        E::InternalInvariant => 6,
        _ => 7,
    };
    program_failure_error(kind, operation.key()).into()
}

fn attachment_js_error(message: &str, code: &str, operation: ProgramOperation) -> JsValue {
    program_error(message, code, operation.key()).into()
}

fn physical_identity_string(
    identity: Option<labcolors_core::program_wire::ProgramPhysicalIdentityV1>,
) -> String {
    match identity {
        Some(labcolors_core::program_wire::ProgramPhysicalIdentityV1::EncodedSrgb8SourceOverV1) => {
            "encoded-srgb8-source-over-v1".to_string()
        }
        None => "unknown".to_string(),
        // Это чистый getter: при неизвестном варианте состояние attachment не
        // меняется, поэтому typed throw сохраняет string ABI без Result glue.
        Some(_) => wasm_bindgen::throw_val(unsupported_physical_identity_error().into()),
    }
}

fn to_attachment_error(
    error: labcolors_core::program_wire::ProgramAttachErrorV1,
    operation: ProgramOperation,
) -> JsValue {
    use labcolors_core::program_wire::ProgramAttachErrorV1 as E;
    // Продолжение закрытой private-таблицы той же JS-проекции: отдельный диапазон
    // сохраняет admission message и прежний unknown-класс attachment.
    let kind = match error {
        E::Binding => 8,
        E::Instantiate => 9,
        E::SinkAdmission => 10,
        E::ResourceExhausted => 11,
        E::InternalInvariant => 12,
        E::NonTerminalTarget => 13,
        _ => 14,
    };
    program_failure_error(kind, operation.key()).into()
}

fn to_attachment_update_error(
    error: labcolors_core::program_wire::ProgramAttachmentUpdateErrorV1,
    operation: ProgramOperation,
) -> JsValue {
    use labcolors_core::program_wire::{
        ProgramAttachmentUpdateErrorV1 as E, ProgramPointSinkErrorV1 as Sink,
        ProgramPointSinkHostErrorV1 as Host,
    };
    let code = match error {
        E::Update => "program_attachment_update",
        E::ResourceExhausted => "program_attachment_resource_exhausted",
        E::InternalInvariant => "program_attachment_internal_invariant",
        E::AlreadyDisposed => "program_attachment_already_disposed",
        E::Sink(Sink::PatchScopeMismatch) => "program_attachment_patch_scope_mismatch",
        E::Sink(Sink::StampMismatch) => "program_attachment_stamp_mismatch",
        E::Sink(Sink::RevisionMismatch) => "program_attachment_revision_mismatch",
        E::Sink(Sink::AlreadyInstalled) => "program_attachment_already_installed",
        E::Sink(Sink::Host(Host::Rejected)) => "program_attachment_host_rejected",
        E::Sink(Sink::Host(Host::Protocol)) => "program_attachment_host_protocol",
        // Будущий non_exhaustive-вариант не должен маскироваться под
        // известный `Update`: package classifier обязан отказать ему.
        _ => "program_attachment_update_unknown",
    };
    attachment_js_error("Program attachment update failed", code, operation)
}

fn to_authority_error(
    error: labcolors_core::program_wire::ProgramMaterializationAuthorityErrorV1,
) -> JsValue {
    use labcolors_core::program_wire::ProgramMaterializationAuthorityErrorV1 as E;
    let code = match error {
        E::NotReady => "program_materialization_not_ready",
        E::SourceOrIntermediatePaint => "program_materialization_paint_not_authority",
        E::StaleRevision => "program_materialization_stale_revision",
        E::StaleIdentity => "program_materialization_stale_identity",
        E::StaleSinkStamp => "program_materialization_stale_sink_stamp",
        E::ForeignBindingEpoch => "program_materialization_foreign_binding_epoch",
        E::TerminalBindingMismatch => "program_materialization_terminal_binding_mismatch",
        E::MissingPointAbsenceProof => "program_materialization_missing_point_absence_proof",
        E::AmbiguousObservationCases => "program_materialization_ambiguous_observation_cases",
        _ => "program_materialization_authority",
    };
    attachment_js_error(
        "Attached materialization authority was refused",
        code,
        ProgramOperation::MaterializationAuthority,
    )
}

// На JS-границе integer ABI усекал/оборачивал вход ДО проверки Core.
// Проверяем исходный JsValue: as_f64 не вызывает пользовательское coercion,
// а u64::try_from дополнительно проверяет, что bigint не потерял старшие биты.
fn checked_u32(value: JsValue) -> Option<u32> {
    let number = value.as_f64()?;
    if number.is_finite() && number.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&number)
    {
        Some(number as u32)
    } else {
        None
    }
}

fn checked_output_index(value: JsValue) -> Result<usize, JsValue> {
    checked_u32(value)
        .map(|index| index as usize)
        .ok_or_else(output_index_error)
}

/// Единственный публичный манифест численных возможностей.
#[wasm_bindgen(js_name = numericalCapabilityManifest)]
pub fn numerical_capability_manifest() -> Result<JsNumericalCapabilityManifestV2, JsValue> {
    let json = terminal_projection::capability_manifest_json();
    js_sys::JSON::parse(&json)
        .map(JsValue::unchecked_into)
        .map_err(|_| {
            binding_failure_error(
                BindingFailure::ManifestInternal as u32,
                "capability manifest не распарсился как JSON",
            )
            .into()
        })
}

/// Точная оценка WCAG 2.2 одной финальной пары sRGB8.
#[wasm_bindgen(js_name = evaluateWcag22)]
pub fn evaluate_wcag22(
    foreground_hex: &str,
    background_hex: &str,
    criterion: &str,
) -> Result<JsWcag22AssessmentV1, JsValue> {
    use labcolors_core::wcag22::Wcag22CriterionV1 as C;
    let project = to_js_error;
    let criterion = C::parse(criterion).ok_or_else(|| {
        project(BindingError::UnknownWcag22Criterion {
            requested: criterion.to_string(),
        })
    })?;
    let assessment =
        labcolors_core::wcag22::evaluate_wcag22_hex(foreground_hex, background_hex, criterion)
            .map_err(|error| {
                use labcolors_core::wcag22::Wcag22EvaluationErrorV1 as E;
                project(match error {
                    E::InvalidSrgb8 { field, reason } => BindingError::InvalidColor {
                        reason: format!("{field}: {reason}"),
                    },
                    other => BindingError::Internal {
                        reason: other.to_string(),
                    },
                })
            })?;
    let json = terminal_projection::wcag22_json(&assessment).map_err(project)?;
    js_sys::JSON::parse(&json)
        .map(JsValue::unchecked_into)
        .map_err(|_| {
            project(BindingError::Internal {
                reason: "WCAG22 projection не распарсился как JSON".to_string(),
            })
        })
}

/// Выдаёт собственную копию байтов сертификата встроенного дескриптора исходников Core.
/// Идентичность и маркер создаёт Core; capability и состояние приёма остаются внутри.
#[wasm_bindgen(js_name = issueSourceCertificateEnvelope)]
pub fn issue_source_certificate_envelope() -> Result<Vec<u8>, JsValue> {
    let certificate = labcolors_core::certificate::issue_source_certificate_v1()
        .map_err(to_certificate_producer_js_error)?;
    certificate
        .try_to_bytes()
        .map_err(to_certificate_producer_js_error)
}

/// Decodes untrusted certificate bytes into immutable metadata for inspection
/// only. The opaque body, producer capability, and admission state stay in
/// Core and are not projected to JavaScript.
#[wasm_bindgen(js_name = decodeCertificateEnvelope)]
pub fn decode_certificate_envelope(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let inner = labcolors_core::certificate::UntrustedEnvelopeV1::decode(bytes)
        .map_err(to_certificate_js_error)?;
    let key = inner.admission_key();
    Ok(certificate_projection(
        labcolors_core::certificate::CERTIFICATE_ENVELOPE_SCHEMA_VERSION_V1,
        key.operation().key(),
        key.authority_kind().key(),
        key.authority_version(),
        key.runtime_artifact_id(),
        key.producer_revision(),
        key.producer_content_identity(),
        key.context_id(),
        key.payload_type().key(),
        key.payload_version(),
        inner.payload_len(),
        inner.payload_sha256(),
        inner.binding_sha256(),
    ))
}

/// Публичный runtime одного скомпилированного Program.
#[wasm_bindgen]
pub struct ProgramRuntime {
    inner: labcolors_core::program_wire::ProgramSessionV1,
}

/// Owned snapshot одного Program update.
#[wasm_bindgen]
pub struct ProgramSnapshot {
    inner: labcolors_core::program_wire::ProgramSnapshotV1,
}

#[wasm_bindgen]
impl ProgramSnapshot {
    /// Individual nominal domains. No shared spatial coordinate is implied.
    #[wasm_bindgen(js_name = fieldAssessments, unchecked_return_type = "ReadonlyArray<ProgramFieldAssessmentV1>")]
    pub fn field_assessments(&self) -> Result<JsValue, JsValue> {
        let project = || -> Result<JsValue, JsValue> {
            let values = report_array()?;
            for field in self.inner.field_assessments() {
                report_array_push(
                    &values,
                    &program_report_projection::project_field(Some(field))?,
                )?;
            }
            report_array_freeze(&values)
        };
        project().map_err(|_| {
            to_program_js_error(
                labcolors_core::program_wire::ProgramRuntimeErrorV1::ResourceExhausted,
                ProgramOperation::FieldAssessment,
            )
        })
    }

    /// Scope is preserved on positive and negative declared-field results.
    #[wasm_bindgen(js_name = fieldAssessment, unchecked_return_type = "ProgramFieldAssessmentV1 | null")]
    pub fn field_assessment(&self) -> Result<JsValue, JsValue> {
        program_report_projection::project_field(self.inner.field_assessment()).map_err(|_| {
            to_program_js_error(
                labcolors_core::program_wire::ProgramRuntimeErrorV1::ResourceExhausted,
                ProgramOperation::FieldAssessment,
            )
        })
    }
    /// Копирует typed attempt witness текущего Unsatisfied/Incomplete; иначе null.
    #[wasm_bindgen(unchecked_return_type = "ProgramDiagnosticsV1 | null")]
    pub fn diagnostics(&self) -> Result<JsValue, JsValue> {
        program_diagnostics_projection::project(self.inner.diagnostics()).map_err(|_| {
            to_program_js_error(
                labcolors_core::program_wire::ProgramRuntimeErrorV1::ResourceExhausted,
                ProgramOperation::Diagnostics,
            )
        })
    }

    /// Копирует финальный отчёт Ready-снимка; вне Ready возвращает null.
    #[wasm_bindgen(js_name = assessmentReport, unchecked_return_type = "ProgramAssessmentReportV1 | null")]
    pub fn assessment_report(&self) -> Result<JsValue, JsValue> {
        program_report_projection::project(self.inner.assessment_report()).map_err(|_| {
            to_program_js_error(
                labcolors_core::program_wire::ProgramRuntimeErrorV1::ResourceExhausted,
                ProgramOperation::AssessmentReport,
            )
        })
    }

    /// Копирует только сводку профиля; отсутствие текущего Ready даёт null.
    #[wasm_bindgen(js_name = cleanlinessAssessment, unchecked_return_type = "ProgramCleanlinessAssessmentV1 | null")]
    pub fn cleanliness_assessment(&self) -> Result<JsValue, JsValue> {
        program_report_projection::project_cleanliness(self.inner.cleanliness_assessment()).map_err(
            |_| {
                to_program_js_error(
                    labcolors_core::program_wire::ProgramRuntimeErrorV1::ResourceExhausted,
                    ProgramOperation::CleanlinessAssessment,
                )
            },
        )
    }

    /// Копирует объяснение адаптации без остальных assessment cells; вне Ready — null.
    #[wasm_bindgen(js_name = referenceSelections, unchecked_return_type = "ReadonlyArray<ProgramReferenceSelectionV1> | null")]
    pub fn reference_selections(&self) -> Result<JsValue, JsValue> {
        program_report_projection::project_reference_selections(self.inner.assessment_report())
            .map_err(|_| {
                to_program_js_error(
                    labcolors_core::program_wire::ProgramRuntimeErrorV1::ResourceExhausted,
                    ProgramOperation::ReferenceSelections,
                )
            })
    }

    /// Compiled component policies from Core; outside Ready the report is absent.
    #[wasm_bindgen(js_name = selectionComponents, unchecked_return_type = "ReadonlyArray<ProgramSelectionComponentV1> | null")]
    pub fn selection_components(&self) -> Result<JsValue, JsValue> {
        program_report_projection::project_selection_components(self.inner.assessment_report())
            .map_err(|_| {
                to_program_js_error(
                    labcolors_core::program_wire::ProgramRuntimeErrorV1::ResourceExhausted,
                    ProgramOperation::SelectionComponents,
                )
            })
    }

    #[wasm_bindgen(getter, js_name = snapshotVersion)]
    pub fn snapshot_version(&self) -> u32 {
        self.inner.snapshot_version()
    }

    #[wasm_bindgen(getter)]
    pub fn revision(&self) -> u64 {
        self.inner.revision()
    }

    #[wasm_bindgen(getter)]
    pub fn reason(&self) -> Option<String> {
        self.inner.reason().map(str::to_owned)
    }

    /// A hard-valid assignment can be ready while optimization remains unfinished.
    #[wasm_bindgen(
        getter,
        unchecked_return_type = "\"exact\" | \"unfinished\" | undefined"
    )]
    pub fn optimality(&self) -> Option<String> {
        self.inner.optimality().map(|value| value.key().to_owned())
    }

    /// Lifecycle key; computed programs add unsatisfied and incomplete.
    #[wasm_bindgen(getter)]
    pub fn state(&self) -> String {
        self.inner.state().key().to_string()
    }

    #[wasm_bindgen(js_name = outputCount)]
    pub fn output_count(&self) -> usize {
        self.inner.outputs().len()
    }

    /// Независимая JS-копия outputs: строки `[slot, red, green, blue, opacity]`
    /// в порядке поштучных геттеров. Состояние snapshot не меняется.
    #[wasm_bindgen(js_name = copyOutputTable)]
    pub fn copy_output_table(&self) -> Result<js_sys::Float64Array, JsValue> {
        copy_snapshot_outputs(self.inner.outputs())
    }

    #[wasm_bindgen(js_name = outputSlot)]
    pub fn output_slot(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
    ) -> Result<u32, JsValue> {
        let index = checked_output_index(index)?;
        self.inner
            .outputs()
            .get(index)
            .map(|output| output.slot())
            .ok_or_else(output_index_error)
    }

    #[wasm_bindgen(js_name = outputRgb)]
    pub fn output_rgb(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
    ) -> Result<js_sys::Uint8Array, JsValue> {
        let index = checked_output_index(index)?;
        let output = self
            .inner
            .outputs()
            .get(index)
            .ok_or_else(output_index_error)?;
        let [red, green, blue] = output.source().bytes();
        copy_output_rgb(red, green, blue)
    }

    #[wasm_bindgen(js_name = outputOpacity)]
    pub fn output_opacity(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
    ) -> Result<f64, JsValue> {
        let index = checked_output_index(index)?;
        self.inner
            .outputs()
            .get(index)
            .map(|output| output.opacity())
            .ok_or_else(output_index_error)
    }
}

/// Компилирует canonical Program wire bytes и создаёт одну runtime Session.
#[wasm_bindgen(js_name = compileProgramWire)]
pub fn compile_program_wire(
    bytes: &[u8],
    #[wasm_bindgen(unchecked_param_type = "number")] stream_id: JsValue,
) -> Result<ProgramRuntime, JsValue> {
    use labcolors_core::program_wire::ProgramRuntimeErrorV1 as E;
    let stream_id = checked_u32(stream_id)
        .ok_or_else(|| to_program_js_error(E::Instantiate, ProgramOperation::CompileProgramWire))?;
    let compiled = labcolors_core::program_wire::compile_program_wire(bytes)
        .map_err(|error| to_program_js_error(error, ProgramOperation::CompileProgramWire))?;
    let session = compiled
        .instantiate(stream_id)
        .map_err(|error| to_program_js_error(error, ProgramOperation::CompileProgramWire))?;
    Ok(ProgramRuntime { inner: session })
}

#[wasm_bindgen]
impl ProgramRuntime {
    /// Prepares the model without publishing it until the host explicitly accepts.
    #[wasm_bindgen(js_name = updateDeclaredFieldWith, unchecked_return_type = "ProgramSnapshot | undefined")]
    pub fn update_declared_field_with(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        surfaces: &[u8],
        #[wasm_bindgen(unchecked_param_type = "number")] varying_input: JsValue,
        from: &[u8],
        to: &[u8],
        #[wasm_bindgen(unchecked_param_type = "(snapshot: ProgramSnapshot) => boolean")]
        accept: &js_sys::Function,
    ) -> Result<JsValue, JsValue> {
        use labcolors_core::program_wire::ProgramRuntimeErrorV1 as E;
        let operation = ProgramOperation::UpdateDeclaredFieldWith;
        let revision =
            u64::try_from(revision).map_err(|_| to_program_js_error(E::Update, operation))?;
        let field = checked_declared_field(surfaces, varying_input, from, to, operation)?;
        let prepared = self
            .inner
            .prepare_declared_field(revision, field)
            .map_err(|error| to_program_js_error(error, operation))?;
        accept_program_update(prepared, accept, operation)
    }
    /// One opaque affine source over the complete normalized interval.
    /// Core derives coverage; no support samples are supplied by JavaScript.
    #[wasm_bindgen(js_name = updateDeclaredField)]
    pub fn update_declared_field(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        surfaces: &[u8],
        #[wasm_bindgen(unchecked_param_type = "number")] varying_input: JsValue,
        from: &[u8],
        to: &[u8],
    ) -> Result<ProgramSnapshot, JsValue> {
        use labcolors_core::program_wire::ProgramRuntimeErrorV1 as E;
        let operation = ProgramOperation::UpdateDeclaredField;
        let revision =
            u64::try_from(revision).map_err(|_| to_program_js_error(E::Update, operation))?;
        let field = checked_declared_field(surfaces, varying_input, from, to, operation)?;
        self.inner
            .update_declared_field(revision, field)
            .map(|inner| ProgramSnapshot { inner })
            .map_err(|error| to_program_js_error(error, operation))
    }
    /// One continuous opaque gradient with strictly increasing authored positions.
    #[wasm_bindgen(js_name = updateGradient)]
    pub fn update_gradient(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        surfaces: &[u8],
        #[wasm_bindgen(unchecked_param_type = "number")] varying_input: JsValue,
        positions: &[f64],
        colors: &[u8],
    ) -> Result<ProgramSnapshot, JsValue> {
        let operation = ProgramOperation::UpdateGradient;
        let (revision, field) = checked_gradient(
            revision,
            surfaces,
            varying_input,
            positions,
            colors,
            operation,
        )?;
        self.inner
            .update_declared_field(revision, field)
            .map(|inner| ProgramSnapshot { inner })
            .map_err(|e| to_program_js_error(e, operation))
    }

    /// Prepare one composition for the entire gradient, then ask the host to accept it.
    #[wasm_bindgen(js_name = updateGradientWith, unchecked_return_type = "ProgramSnapshot | undefined")]
    pub fn update_gradient_with(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        surfaces: &[u8],
        #[wasm_bindgen(unchecked_param_type = "number")] varying_input: JsValue,
        positions: &[f64],
        colors: &[u8],
        #[wasm_bindgen(unchecked_param_type = "(snapshot: ProgramSnapshot) => boolean")]
        accept: &js_sys::Function,
    ) -> Result<JsValue, JsValue> {
        let operation = ProgramOperation::UpdateGradientWith;
        let (revision, field) = checked_gradient(
            revision,
            surfaces,
            varying_input,
            positions,
            colors,
            operation,
        )?;
        let prepared = self
            .inner
            .prepare_declared_field(revision, field)
            .map_err(|e| to_program_js_error(e, operation))?;
        accept_program_update(prepared, accept, operation)
    }

    /// Multiple independent gradients, each with its own normalized coordinate.
    #[wasm_bindgen(js_name = updateIndependentGradients)]
    pub fn update_independent_gradients(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        surfaces: &[u8],
        inputs: &[u32],
        offsets: &[u32],
        positions: &[f64],
        colors: &[u8],
    ) -> Result<ProgramSnapshot, JsValue> {
        let op = ProgramOperation::UpdateIndependentGradients;
        let (revision, field) = checked_independent_gradients(
            revision, surfaces, inputs, offsets, positions, colors, op,
        )?;
        self.inner
            .update_declared_field(revision, field)
            .map(|inner| ProgramSnapshot { inner })
            .map_err(|err| to_program_js_error(err, op))
    }

    /// All modeled surfaces and paints are offered in one prepare/commit.
    #[allow(clippy::too_many_arguments)]
    #[wasm_bindgen(js_name = updateIndependentGradientsWith, unchecked_return_type = "ProgramSnapshot | undefined")]
    pub fn update_independent_gradients_with(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        surfaces: &[u8],
        inputs: &[u32],
        offsets: &[u32],
        positions: &[f64],
        colors: &[u8],
        #[wasm_bindgen(unchecked_param_type = "(snapshot: ProgramSnapshot) => boolean")]
        accept: &js_sys::Function,
    ) -> Result<JsValue, JsValue> {
        let op = ProgramOperation::UpdateIndependentGradientsWith;
        let (revision, field) = checked_independent_gradients(
            revision, surfaces, inputs, offsets, positions, colors, op,
        )?;
        let prepared = self
            .inner
            .prepare_declared_field(revision, field)
            .map_err(|err| to_program_js_error(err, op))?;
        accept_program_update(prepared, accept, op)
    }

    /// Атомарный observed update. `surfaces` — row-major
    /// `scenario_ids.len() × surface_count × 3` bytes.
    #[wasm_bindgen(js_name = updateObserved)]
    pub fn update_observed(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        scenario_ids: &[u32],
        surfaces: &[u8],
        #[wasm_bindgen(unchecked_param_type = "number")] surface_count: JsValue,
    ) -> Result<ProgramSnapshot, JsValue> {
        use labcolors_core::program_wire::ProgramRuntimeErrorV1 as E;
        let revision = u64::try_from(revision)
            .map_err(|_| to_program_js_error(E::Update, ProgramOperation::UpdateObserved))?;
        let surface_count = checked_u32(surface_count)
            .ok_or_else(|| to_program_js_error(E::Update, ProgramOperation::UpdateObserved))?
            as usize;
        let observation = labcolors_core::program_wire::ProgramObservationV1::from_rgb8(
            scenario_ids,
            surfaces,
            surface_count,
        )
        .map_err(|_| to_program_js_error(E::Update, ProgramOperation::UpdateObserved))?;
        self.inner
            .update_observed_view(revision, observation)
            .map(|inner| ProgramSnapshot { inner })
            .map_err(|error| to_program_js_error(error, ProgramOperation::UpdateObserved))
    }

    /// Готовит один owned-снимок, затем синхронно вызывает `accept(snapshot)`.
    /// `true` принимает ревизию и возвращает тот же снимок; `false` возвращает
    /// undefined. Ошибка callback оставляет прежнюю ревизию и пробрасывается.
    /// Callback владеет переданным снимком и атомарностью своих внешних записей.
    #[wasm_bindgen(js_name = updateObservedWith, unchecked_return_type = "ProgramSnapshot | undefined")]
    pub fn update_observed_with(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        scenario_ids: &[u32],
        surfaces: &[u8],
        #[wasm_bindgen(unchecked_param_type = "number")] surface_count: JsValue,
        #[wasm_bindgen(unchecked_param_type = "(snapshot: ProgramSnapshot) => boolean")]
        accept: &js_sys::Function,
    ) -> Result<JsValue, JsValue> {
        use labcolors_core::program_wire::ProgramRuntimeErrorV1 as E;
        let operation = ProgramOperation::UpdateObservedWith;
        let revision =
            u64::try_from(revision).map_err(|_| to_program_js_error(E::Update, operation))?;
        let surface_count = checked_u32(surface_count)
            .ok_or_else(|| to_program_js_error(E::Update, operation))?
            as usize;
        let observation = labcolors_core::program_wire::ProgramObservationV1::from_rgb8(
            scenario_ids,
            surfaces,
            surface_count,
        )
        .map_err(|_| to_program_js_error(E::Update, operation))?;
        let prepared = self
            .inner
            .prepare_observed_view(revision, observation)
            .map_err(|error| to_program_js_error(error, operation))?;
        accept_program_update(prepared, accept, operation)
    }

    #[wasm_bindgen(js_name = updateUnknown)]
    pub fn update_unknown(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        #[wasm_bindgen(unchecked_param_type = "number")] reason_id: JsValue,
    ) -> Result<ProgramSnapshot, JsValue> {
        use labcolors_core::program_wire::ProgramRuntimeErrorV1 as E;
        let revision = u64::try_from(revision)
            .map_err(|_| to_program_js_error(E::Update, ProgramOperation::UpdateUnknown))?;
        let reason_id = checked_u32(reason_id)
            .ok_or_else(|| to_program_js_error(E::Update, ProgramOperation::UpdateUnknown))?;
        self.inner
            .update_unknown(revision, reason_id)
            .map(|inner| ProgramSnapshot { inner })
            .map_err(|error| to_program_js_error(error, ProgramOperation::UpdateUnknown))
    }
}

fn checked_independent_gradients<'a>(
    revision: JsValue,
    surfaces: &'a [u8],
    inputs: &'a [u32],
    offsets: &'a [u32],
    positions: &'a [f64],
    colors: &'a [u8],
    op: ProgramOperation,
) -> Result<
    (
        u64,
        labcolors_core::program_wire::ProgramDeclaredFieldV1<'a>,
    ),
    JsValue,
> {
    use labcolors_core::program_wire::{ProgramDeclaredFieldV1, ProgramRuntimeErrorV1};
    let invalid = || to_program_js_error(ProgramRuntimeErrorV1::Update, op);
    let revision = u64::try_from(revision).map_err(|_| invalid())?;
    let field = ProgramDeclaredFieldV1::from_independent_gradients_rgb8(
        surfaces, inputs, offsets, positions, colors,
    )
    .map_err(|_| invalid())?;
    Ok((revision, field))
}

fn checked_gradient<'a>(
    revision: JsValue,
    surfaces: &'a [u8],
    varying_input: JsValue,
    positions: &'a [f64],
    colors: &'a [u8],
    operation: ProgramOperation,
) -> Result<
    (
        u64,
        labcolors_core::program_wire::ProgramDeclaredFieldV1<'a>,
    ),
    JsValue,
> {
    use labcolors_core::program_wire::{ProgramDeclaredFieldV1, ProgramRuntimeErrorV1};
    let invalid = || to_program_js_error(ProgramRuntimeErrorV1::Update, operation);
    let revision = u64::try_from(revision).map_err(|_| invalid())?;
    let input = checked_u32(varying_input).ok_or_else(invalid)?;
    let field =
        ProgramDeclaredFieldV1::from_gradient_rgb8(surfaces, input as usize, positions, colors)
            .map_err(|_| invalid())?;
    Ok((revision, field))
}

fn checked_declared_field<'a>(
    surfaces: &'a [u8],
    varying_input: JsValue,
    from: &[u8],
    to: &[u8],
    operation: ProgramOperation,
) -> Result<labcolors_core::program_wire::ProgramDeclaredFieldV1<'a>, JsValue> {
    use labcolors_core::program_wire::{ProgramDeclaredFieldV1, ProgramRuntimeErrorV1 as E};
    let invalid = || to_program_js_error(E::Update, operation);
    let input = checked_u32(varying_input).ok_or_else(invalid)? as usize;
    let from: [u8; 3] = from.try_into().map_err(|_| invalid())?;
    let to: [u8; 3] = to.try_into().map_err(|_| invalid())?;
    ProgramDeclaredFieldV1::from_rgb8(
        surfaces,
        input,
        labcolors_core::Srgb8::new(from),
        labcolors_core::Srgb8::new(to),
    )
    .map_err(|_| invalid())
}

fn accept_program_update(
    prepared: labcolors_core::program_wire::PreparedProgramUpdateV1<'_>,
    accept: &js_sys::Function,
    operation: ProgramOperation,
) -> Result<JsValue, JsValue> {
    let mut snapshot = JsValue::UNDEFINED;
    let accepted = prepared.commit_with(|inner| -> Result<bool, JsValue> {
        // Allocate the JS wrapper before host acceptance. After true, only the
        // already prepared Core transition is published, without recomputation.
        snapshot = ProgramSnapshot { inner }.into();
        accept
            .call1(&JsValue::UNDEFINED, &snapshot)?
            .as_bool()
            .ok_or_else(|| {
                program_error(
                    "Program acceptance callback must return a boolean",
                    "program_host_protocol",
                    operation.key(),
                )
                .into()
            })
    })?;
    Ok(if accepted {
        snapshot
    } else {
        JsValue::UNDEFINED
    })
}

/// Синхронный WASM-адаптер point sink, принадлежащего consumer.
#[derive(Clone)]
struct JsPointSinkHostV1 {
    callback: js_sys::Function,
}

impl labcolors_core::program_wire::ProgramPointSinkHostV1 for JsPointSinkHostV1 {
    fn try_install(
        &mut self,
        intent: labcolors_core::program_wire::ProgramPointSinkIntentV1,
    ) -> Result<(), labcolors_core::program_wire::ProgramPointSinkHostErrorV1> {
        let object = js_sys::Object::new();
        let object_value: JsValue = object.clone().into();
        let operation = match intent.operation() {
            labcolors_core::program_wire::ProgramPointSinkOperationV1::SetAll => "setAll",
            labcolors_core::program_wire::ProgramPointSinkOperationV1::RevokeAll => "revokeAll",
            labcolors_core::program_wire::ProgramPointSinkOperationV1::ConfirmExact => {
                "confirmExact"
            }
            _ => return Err(labcolors_core::program_wire::ProgramPointSinkHostErrorV1::Protocol),
        };
        let set = |key: &str, value: &JsValue| {
            js_sys::Reflect::set(&object_value, &JsValue::from_str(key), value).is_ok()
        };
        let bigint = |value: u64| -> JsValue { js_sys::BigInt::from(value).into() };
        if !set("operation", &JsValue::from_str(operation))
            || !set("revision", &bigint(intent.revision()))
            || !set(
                "expectedSequence",
                &bigint(intent.expected_stamp().sequence()),
            )
            || !set(
                "desiredSequence",
                &bigint(intent.desired_stamp().sequence()),
            )
            || !set(
                "bindingEpoch",
                &bigint(intent.expected_stamp().binding_epoch()),
            )
            || !set(
                "sinkOutput",
                &JsValue::from_f64(f64::from(intent.sink_output())),
            )
        {
            return Err(labcolors_core::program_wire::ProgramPointSinkHostErrorV1::Protocol);
        }
        let point = match intent.point() {
            Some(point) => {
                let point_object = js_sys::Object::new();
                let point_value: JsValue = point_object.clone().into();
                let rgb = point.source().bytes();
                let rgb = js_sys::Uint8Array::from(&rgb[..]);
                if js_sys::Reflect::set(
                    &point_value,
                    &JsValue::from_str("slot"),
                    &JsValue::from_f64(f64::from(point.slot())),
                )
                .is_err()
                    || js_sys::Reflect::set(&point_value, &JsValue::from_str("source"), &rgb.into())
                        .is_err()
                    || js_sys::Reflect::set(
                        &point_value,
                        &JsValue::from_str("opacity"),
                        &JsValue::from_f64(point.opacity()),
                    )
                    .is_err()
                {
                    return Err(
                        labcolors_core::program_wire::ProgramPointSinkHostErrorV1::Protocol,
                    );
                }
                point_value
            }
            None => JsValue::NULL,
        };
        if !set("point", &point) {
            return Err(labcolors_core::program_wire::ProgramPointSinkHostErrorV1::Protocol);
        }
        let status = self
            .callback
            .call1(&JsValue::UNDEFINED, &object_value)
            .map_err(|_| labcolors_core::program_wire::ProgramPointSinkHostErrorV1::Protocol)?;
        let accepted = status
            .as_bool()
            .ok_or(labcolors_core::program_wire::ProgramPointSinkHostErrorV1::Protocol)?;
        if !accepted {
            Err(labcolors_core::program_wire::ProgramPointSinkHostErrorV1::Rejected)
        } else {
            Ok(())
        }
    }
}

fn attachment_snapshot_error(operation: ProgramOperation) -> JsValue {
    attachment_js_error(
        "Program attachment input shape is invalid",
        "program_attachment_update",
        operation,
    )
}

fn attachment_observation<'a>(
    scenario_ids: &'a [u32],
    surfaces: &'a [u8],
    surface_count: JsValue,
    operation: ProgramOperation,
) -> Result<labcolors_core::program_wire::ProgramObservationV1<'a>, JsValue> {
    let surface_count =
        checked_u32(surface_count).ok_or_else(|| attachment_snapshot_error(operation))? as usize;
    labcolors_core::program_wire::ProgramObservationV1::from_rgb8(
        scenario_ids,
        surfaces,
        surface_count,
    )
    .map_err(|_| attachment_snapshot_error(operation))
}

/// Создаёт один публичный attachment и передаёт все внешние эффекты в `host`.
#[wasm_bindgen(js_name = attachProgramWire)]
pub fn attach_program_wire(
    bytes: &[u8],
    #[wasm_bindgen(unchecked_param_type = "number")] stream_id: JsValue,
    #[wasm_bindgen(unchecked_param_type = "number")] output_slot: JsValue,
    #[wasm_bindgen(unchecked_param_type = "number")] sink_output: JsValue,
    #[wasm_bindgen(unchecked_param_type = "number")] presentation_root: JsValue,
    #[wasm_bindgen(unchecked_param_type = "number")] occurrence: JsValue,
    host: &js_sys::Function,
) -> Result<ProgramAttachment, JsValue> {
    let parse = |value: JsValue| {
        checked_u32(value).ok_or_else(|| {
            attachment_js_error(
                "Program attachment identifiers must be u32 numbers",
                "program_attachment_binding",
                ProgramOperation::AttachProgramWire,
            )
        })
    };
    let stream_id = parse(stream_id)?;
    let output_slot = parse(output_slot)?;
    let sink_output = parse(sink_output)?;
    let presentation_root = parse(presentation_root)?;
    let occurrence = parse(occurrence)?;
    let compiled = labcolors_core::program_wire::compile_program_wire(bytes)
        .map_err(|error| to_program_js_error(error, ProgramOperation::AttachProgramWire))?;
    let inner = compiled
        .attach(
            stream_id,
            output_slot,
            sink_output,
            presentation_root,
            occurrence,
            JsPointSinkHostV1 {
                callback: host.clone(),
            },
        )
        .map_err(|error| to_attachment_error(error, ProgramOperation::AttachProgramWire))?;
    Ok(ProgramAttachment {
        inner: RefCell::new(inner),
        busy: Cell::new(false),
    })
}

#[wasm_bindgen(js_name = ProgramAttachment)]
pub struct ProgramAttachment {
    // Общий JS receiver позволяет допустить busy на уровне Rust внутри host
    // callback; изменяемое Core attachment остаётся за этой WASM-only ячейкой.
    inner: RefCell<labcolors_core::program_wire::ProgramAttachmentV1<JsPointSinkHostV1>>,
    busy: Cell<bool>,
}

struct AttachmentBusyGuard<'a> {
    busy: &'a Cell<bool>,
}

impl Drop for AttachmentBusyGuard<'_> {
    fn drop(&mut self) {
        self.busy.set(false);
    }
}

impl ProgramAttachment {
    fn enter(
        busy: &Cell<bool>,
        operation: ProgramOperation,
    ) -> Result<AttachmentBusyGuard<'_>, JsValue> {
        if busy.get() {
            return Err(attachment_js_error(
                "Program attachment operation is already in progress",
                "program_attachment_busy",
                operation,
            ));
        }
        busy.set(true);
        Ok(AttachmentBusyGuard { busy })
    }
}

#[wasm_bindgen]
impl ProgramAttachment {
    /// Атомарно применяет observed revision через host sink.
    #[wasm_bindgen(js_name = updateObserved)]
    pub fn update_observed(
        &self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        scenario_ids: &[u32],
        surfaces: &[u8],
        #[wasm_bindgen(unchecked_param_type = "number")] surface_count: JsValue,
    ) -> Result<ProgramAttachedSnapshot, JsValue> {
        let _busy = Self::enter(&self.busy, ProgramOperation::AttachmentUpdateObserved)?;
        let revision = u64::try_from(revision)
            .map_err(|_| attachment_snapshot_error(ProgramOperation::AttachmentUpdateObserved))?;
        let observation = attachment_observation(
            scenario_ids,
            surfaces,
            surface_count,
            ProgramOperation::AttachmentUpdateObserved,
        )?;
        let result = self
            .inner
            .borrow_mut()
            .update_observed_view(revision, observation);
        result
            .map(|inner| ProgramAttachedSnapshot { inner })
            .map_err(|error| {
                to_attachment_update_error(error, ProgramOperation::AttachmentUpdateObserved)
            })
    }

    /// Атомарно отзывает owned sink для недоступной revision.
    #[wasm_bindgen(js_name = updateUnknown)]
    pub fn update_unknown(
        &self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        #[wasm_bindgen(unchecked_param_type = "number")] reason_id: JsValue,
    ) -> Result<ProgramAttachedSnapshot, JsValue> {
        let _busy = Self::enter(&self.busy, ProgramOperation::AttachmentUpdateUnknown)?;
        let revision = u64::try_from(revision)
            .map_err(|_| attachment_snapshot_error(ProgramOperation::AttachmentUpdateUnknown))?;
        let reason_id = checked_u32(reason_id)
            .ok_or_else(|| attachment_snapshot_error(ProgramOperation::AttachmentUpdateUnknown))?;
        let result = self.inner.borrow_mut().update_unknown(revision, reason_id);
        result
            .map(|inner| ProgramAttachedSnapshot { inner })
            .map_err(|error| {
                to_attachment_update_error(error, ProgramOperation::AttachmentUpdateUnknown)
            })
    }

    /// Допускает authority только из текущего committed head attachment.
    #[wasm_bindgen(js_name = materializationAuthority)]
    pub fn materialization_authority(&self) -> Result<AttachedMaterializationAuthority, JsValue> {
        let _busy = Self::enter(&self.busy, ProgramOperation::MaterializationAuthority)?;
        let result = self.inner.borrow().current_materialization_authority();
        result
            .map(|inner| AttachedMaterializationAuthority { inner })
            .map_err(to_authority_error)
    }

    /// Проверяет переданные caller-ом revision, identity и epoch против current head.
    #[wasm_bindgen(js_name = materializationAuthorityFor)]
    pub fn materialization_authority_for(
        &self,
        #[wasm_bindgen(unchecked_param_type = "bigint")] revision: JsValue,
        content_identity: &[u8],
        #[wasm_bindgen(unchecked_param_type = "bigint")] binding_epoch: JsValue,
    ) -> Result<AttachedMaterializationAuthority, JsValue> {
        let _busy = Self::enter(&self.busy, ProgramOperation::MaterializationAuthority)?;
        let render = self.inner.borrow().current_render().ok_or_else(|| {
            to_authority_error(
                labcolors_core::program_wire::ProgramMaterializationAuthorityErrorV1::NotReady,
            )
        })?;
        let content_identity: [u8; 32] = content_identity.try_into().map_err(|_| {
            attachment_js_error(
                "Materialization content identity must contain 32 bytes",
                "program_materialization_stale_identity",
                ProgramOperation::MaterializationAuthority,
            )
        })?;
        let revision = u64::try_from(revision).map_err(|_| {
            to_authority_error(
                labcolors_core::program_wire::ProgramMaterializationAuthorityErrorV1::StaleRevision,
            )
        })?;
        let binding_epoch = u64::try_from(binding_epoch).map_err(|_| {
            to_authority_error(
                labcolors_core::program_wire::ProgramMaterializationAuthorityErrorV1::ForeignBindingEpoch,
            )
        })?;
        let expected = render
            .expectation()
            .with_content_identity(content_identity)
            .with_revision(revision)
            .with_binding_epoch(binding_epoch);
        let result = self.inner.borrow().materialization_authority(
            labcolors_core::program_wire::ProgramMaterializationCandidateV1::AttachedCommit,
            expected,
        );
        result
            .map(|inner| AttachedMaterializationAuthority { inner })
            .map_err(to_authority_error)
    }

    /// Потребляет attachment только после подтверждения внешнего revoke caller-ом.
    #[wasm_bindgen]
    pub fn dispose(
        &self,
        #[wasm_bindgen(unchecked_param_type = "boolean")] confirmed: JsValue,
    ) -> Result<(), JsValue> {
        let _busy = Self::enter(&self.busy, ProgramOperation::AttachmentDispose)?;
        let confirmed = confirmed.as_bool() == Some(true);
        let result = self
            .inner
            .borrow_mut()
            .dispose(|| if confirmed { Ok(()) } else { Err(()) });
        result.map_err(|error| match error {
            labcolors_core::program_wire::ProgramDisposeErrorV1::AlreadyDisposed => {
                attachment_js_error(
                    "Program attachment is already disposed",
                    "program_attachment_already_disposed",
                    ProgramOperation::AttachmentDispose,
                )
            }
            labcolors_core::program_wire::ProgramDisposeErrorV1::Confirmation(()) => {
                attachment_js_error(
                    "External sink revoke was not confirmed",
                    "program_attachment_revoke_unconfirmed",
                    ProgramOperation::AttachmentDispose,
                )
            }
            _ => attachment_js_error(
                "Program attachment disposal failed",
                "program_attachment_dispose",
                ProgramOperation::AttachmentDispose,
            ),
        })
    }
}

#[wasm_bindgen(js_name = ProgramAttachedSnapshot)]
pub struct ProgramAttachedSnapshot {
    inner: labcolors_core::program_wire::ProgramAttachedSnapshotV1,
}

#[wasm_bindgen]
impl ProgramAttachedSnapshot {
    #[wasm_bindgen(getter)]
    pub fn state(&self) -> String {
        self.inner.snapshot().state().key().to_string()
    }

    #[wasm_bindgen(js_name = outputCount)]
    pub fn output_count(&self) -> usize {
        self.inner.snapshot().outputs().len()
    }

    /// Независимая JS-копия outputs: строки `[slot, red, green, blue, opacity]`
    /// в порядке поштучных геттеров. Состояние snapshot не меняется.
    #[wasm_bindgen(js_name = copyOutputTable)]
    pub fn copy_output_table(&self) -> Result<js_sys::Float64Array, JsValue> {
        copy_snapshot_outputs(self.inner.snapshot().outputs())
    }

    #[wasm_bindgen(js_name = outputSlot)]
    pub fn output_slot(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
    ) -> Result<u32, JsValue> {
        let index = checked_output_index(index)?;
        self.inner
            .snapshot()
            .outputs()
            .get(index)
            .map(|output| output.slot())
            .ok_or_else(output_index_error)
    }

    #[wasm_bindgen(js_name = outputRgb)]
    pub fn output_rgb(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
    ) -> Result<Box<[u8]>, JsValue> {
        let index = checked_output_index(index)?;
        self.inner
            .snapshot()
            .outputs()
            .get(index)
            .map(|output| output.source().bytes().to_vec().into_boxed_slice())
            .ok_or_else(output_index_error)
    }

    #[wasm_bindgen(js_name = outputOpacity)]
    pub fn output_opacity(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
    ) -> Result<f64, JsValue> {
        let index = checked_output_index(index)?;
        self.inner
            .snapshot()
            .outputs()
            .get(index)
            .map(|output| output.opacity())
            .ok_or_else(output_index_error)
    }

    #[wasm_bindgen(js_name = hasRender)]
    pub fn has_render(&self) -> bool {
        self.inner.render().is_some()
    }

    #[wasm_bindgen]
    pub fn render(&self) -> Option<ProgramAttachedRender> {
        self.inner
            .render()
            .map(|inner| ProgramAttachedRender { inner })
    }
}

#[wasm_bindgen(js_name = ProgramAttachedRender)]
pub struct ProgramAttachedRender {
    inner: labcolors_core::program_wire::ProgramAttachedRenderV1,
}

#[wasm_bindgen]
impl ProgramAttachedRender {
    #[wasm_bindgen(js_name = outputSlot)]
    pub fn output_slot(&self) -> u32 {
        self.inner.output().slot()
    }

    #[wasm_bindgen(js_name = sinkOutput)]
    pub fn sink_output(&self) -> u32 {
        self.inner.sink_output()
    }

    #[wasm_bindgen(js_name = outputRgb)]
    pub fn output_rgb(&self) -> Box<[u8]> {
        self.inner
            .output()
            .source()
            .bytes()
            .to_vec()
            .into_boxed_slice()
    }

    #[wasm_bindgen(js_name = outputOpacity)]
    pub fn output_opacity(&self) -> f64 {
        self.inner.output().opacity()
    }

    #[wasm_bindgen]
    pub fn revision(&self) -> u64 {
        self.inner.revision()
    }

    #[wasm_bindgen(js_name = sinkSequence)]
    pub fn sink_sequence(&self) -> u64 {
        self.inner.sink_stamp().sequence()
    }

    #[wasm_bindgen(js_name = bindingEpoch)]
    pub fn binding_epoch(&self) -> u64 {
        self.inner.sink_stamp().binding_epoch()
    }

    #[wasm_bindgen(js_name = contentIdentity)]
    pub fn content_identity(&self) -> Box<[u8]> {
        self.inner.content_identity().to_vec().into_boxed_slice()
    }

    #[wasm_bindgen(js_name = contextIdentity)]
    pub fn context_identity(&self) -> Box<[u8]> {
        self.inner
            .context()
            .identity_bytes()
            .to_vec()
            .into_boxed_slice()
    }

    #[wasm_bindgen(js_name = presentationRoot)]
    pub fn presentation_root(&self) -> u32 {
        self.inner.presentation_root()
    }

    #[wasm_bindgen]
    pub fn occurrence(&self) -> u32 {
        self.inner.occurrence()
    }

    #[wasm_bindgen(js_name = physicalIdentity)]
    pub fn physical_identity(&self) -> String {
        physical_identity_string(self.inner.physical_identity())
    }

    #[wasm_bindgen(js_name = terminalCompositeRgb)]
    pub fn terminal_composite_rgb(&self) -> Option<Box<[u8]>> {
        self.inner
            .terminal_composite()
            .map(|value| value.bytes().to_vec().into_boxed_slice())
    }
}

#[wasm_bindgen]
pub struct AttachedMaterializationAuthority {
    inner: labcolors_core::program_wire::AttachedMaterializationAuthorityV1,
}

#[wasm_bindgen]
impl AttachedMaterializationAuthority {
    #[wasm_bindgen(js_name = outputSlot)]
    pub fn output_slot(&self) -> u32 {
        self.inner.output().slot()
    }

    #[wasm_bindgen(js_name = outputRgb)]
    pub fn output_rgb(&self) -> Box<[u8]> {
        self.inner
            .output()
            .source()
            .bytes()
            .to_vec()
            .into_boxed_slice()
    }

    #[wasm_bindgen(js_name = outputOpacity)]
    pub fn output_opacity(&self) -> f64 {
        self.inner.output().opacity()
    }

    #[wasm_bindgen]
    pub fn revision(&self) -> u64 {
        self.inner.revision()
    }

    #[wasm_bindgen(js_name = sinkSequence)]
    pub fn sink_sequence(&self) -> u64 {
        self.inner.sink_stamp().sequence()
    }

    #[wasm_bindgen(js_name = bindingEpoch)]
    pub fn binding_epoch(&self) -> u64 {
        self.inner.sink_stamp().binding_epoch()
    }

    #[wasm_bindgen(js_name = contentIdentity)]
    pub fn content_identity(&self) -> Box<[u8]> {
        self.inner.content_identity().to_vec().into_boxed_slice()
    }

    #[wasm_bindgen(js_name = contextIdentity)]
    pub fn context_identity(&self) -> Box<[u8]> {
        self.inner
            .context()
            .identity_bytes()
            .to_vec()
            .into_boxed_slice()
    }

    #[wasm_bindgen(js_name = presentationRoot)]
    pub fn presentation_root(&self) -> u32 {
        self.inner.presentation_root()
    }

    #[wasm_bindgen]
    pub fn occurrence(&self) -> u32 {
        self.inner.occurrence()
    }

    #[wasm_bindgen(js_name = physicalIdentity)]
    pub fn physical_identity(&self) -> String {
        physical_identity_string(self.inner.physical_identity())
    }

    #[wasm_bindgen(js_name = rendererProvenance)]
    pub fn renderer_provenance(&self) -> String {
        match self.inner.renderer_provenance() {
            labcolors_core::program_wire::ProgramRendererProvenanceV1::Unverified => "unverified",
            _ => "unknown",
        }
        .to_string()
    }

    #[wasm_bindgen(js_name = terminalCompositeRgb)]
    pub fn terminal_composite_rgb(&self) -> Box<[u8]> {
        self.inner
            .terminal_composite()
            .bytes()
            .to_vec()
            .into_boxed_slice()
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod browser_tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    const REFERENCE_WIRE_HEX: &str = concat!(
        "4c4350570100b3000000010000000b0000001414140100000015000000010b0000000000",
        "000000000000010000001f00000000000000010000002900000001150000000100000033",
        "000000011f000000010000003d000000290000003300000000000000000050409a999999",
        "9999c93f0101000000470000003d00000001000000470000003d00000001000000510000",
        "00093d000000030100000052000000013d000000141414010000005b00000029000000",
    );

    fn reference_wire() -> Vec<u8> {
        REFERENCE_WIRE_HEX
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let text = core::str::from_utf8(pair).expect("ASCII hex");
                u8::from_str_radix(text, 16).expect("canonical hex")
            })
            .collect()
    }

    fn assert_program_error(error: JsValue, code: &str, operation: &str) {
        assert!(error.is_instance_of::<js_sys::Error>());
        assert_eq!(
            js_sys::Reflect::get(&error, &JsValue::from_str("code"))
                .expect("code property")
                .as_string()
                .as_deref(),
            Some(code)
        );
        assert_eq!(
            js_sys::Reflect::get(&error, &JsValue::from_str("operation"))
                .expect("operation property")
                .as_string()
                .as_deref(),
            Some(operation)
        );
    }

    #[wasm_bindgen_test]
    fn source_producer_errors_keep_static_codes_and_operation() {
        use labcolors_core::certificate::{CertificateErrorV1, CertificateProducerErrorV1 as E};

        for (error, code) in [
            (
                E::ProducerIdentityUnavailable,
                "certificate_producer_identity_unavailable",
            ),
            (
                E::Envelope(CertificateErrorV1::ResourceLimitExceeded),
                "certificate_resource_limit_exceeded",
            ),
        ] {
            let projected = to_certificate_producer_js_error(error);
            let message = js_sys::Reflect::get(&projected, &JsValue::from_str("message"))
                .expect("message property")
                .as_string()
                .expect("static message");
            assert_eq!(message, "Certificate envelope operation failed");
            assert!(message.len() <= 256);
            assert_program_error(projected, code, "issueSourceCertificateEnvelope");
        }
    }

    #[wasm_bindgen_test]
    fn program_error_projection_distinguishes_every_runtime_failure_class() {
        use labcolors_core::program_wire::ProgramRuntimeErrorV1 as E;

        for (error, operation, code) in [
            (
                E::Wire,
                ProgramOperation::CompileProgramWire,
                "program_wire",
            ),
            (
                E::Compile,
                ProgramOperation::CompileProgramWire,
                "program_compile",
            ),
            (
                E::FamilyArtifactsRequired,
                ProgramOperation::CompileProgramWire,
                "program_family_artifacts_required",
            ),
            (
                E::Instantiate,
                ProgramOperation::CompileProgramWire,
                "program_instantiate",
            ),
            (
                E::Update,
                ProgramOperation::UpdateObserved,
                "program_update",
            ),
            (
                E::ResourceExhausted,
                ProgramOperation::CompileProgramWire,
                "program_resource_exhausted",
            ),
            (
                E::InternalInvariant,
                ProgramOperation::CompileProgramWire,
                "program_internal_invariant",
            ),
            (
                E::ResourceExhausted,
                ProgramOperation::UpdateObserved,
                "program_resource_exhausted",
            ),
            (
                E::ResourceExhausted,
                ProgramOperation::CopyOutputTable,
                "program_resource_exhausted",
            ),
            (
                E::InternalInvariant,
                ProgramOperation::UpdateUnknown,
                "program_internal_invariant",
            ),
        ] {
            let projected = to_program_js_error(error, operation);
            assert_eq!(
                js_sys::Reflect::get(&projected, &JsValue::from_str("message"))
                    .unwrap()
                    .as_string()
                    .as_deref(),
                Some("Program runtime operation failed")
            );
            assert_program_error(projected, code, operation.key());
        }
        for (error, code) in [
            (
                labcolors_core::program_wire::ProgramAttachErrorV1::Binding,
                "program_attachment_binding",
            ),
            (
                labcolors_core::program_wire::ProgramAttachErrorV1::Instantiate,
                "program_attachment_instantiate",
            ),
            (
                labcolors_core::program_wire::ProgramAttachErrorV1::SinkAdmission,
                "program_attachment_sink_admission",
            ),
            (
                labcolors_core::program_wire::ProgramAttachErrorV1::ResourceExhausted,
                "program_attachment_resource_exhausted",
            ),
            (
                labcolors_core::program_wire::ProgramAttachErrorV1::InternalInvariant,
                "program_attachment_internal_invariant",
            ),
            (
                labcolors_core::program_wire::ProgramAttachErrorV1::NonTerminalTarget,
                "program_attachment_non_terminal_target",
            ),
        ] {
            let projected = to_attachment_error(error, ProgramOperation::AttachProgramWire);
            assert_eq!(
                js_sys::Reflect::get(&projected, &JsValue::from_str("message"))
                    .unwrap()
                    .as_string()
                    .as_deref(),
                Some("Program attachment admission failed")
            );
            assert_program_error(projected, code, "attachProgramWire");
        }
    }

    #[wasm_bindgen_test]
    fn terminal_program_wire_and_update_failures_keep_operation_context() {
        let wire_error = match compile_program_wire(&[], 1.into()) {
            Ok(_) => panic!("empty wire must fail"),
            Err(error) => error,
        };
        assert_program_error(wire_error, "program_wire", "compileProgramWire");

        let mut runtime =
            compile_program_wire(&reference_wire(), 7.into()).expect("canonical wire");
        let update_error = match runtime.update_observed(1_u64.into(), &[], &[], 1.into()) {
            Ok(_) => panic!("empty scenario set must fail"),
            Err(error) => error,
        };
        assert_program_error(update_error, "program_update", "updateObserved");

        let shape_error = match runtime.update_observed(1_u64.into(), &[1], &[255, 255], 1.into()) {
            Ok(_) => panic!("incomplete surface matrix must fail"),
            Err(error) => error,
        };
        assert_program_error(shape_error, "program_update", "updateObserved");
    }

    #[wasm_bindgen_test]
    fn terminal_program_compiles_updates_and_projects_one_snapshot() {
        let mut runtime =
            compile_program_wire(&reference_wire(), 1.into()).expect("canonical wire");
        let snapshot = runtime
            .update_observed(1_u64.into(), &[1], &[255, 255, 255], 1.into())
            .expect("observed update");

        assert_eq!(snapshot.state(), "ready");
        assert_eq!(snapshot.output_count(), 1);
        assert_eq!(snapshot.output_slot(0.into()).unwrap(), 91);
        assert_eq!(
            snapshot.output_rgb(0.into()).unwrap().to_vec().as_slice(),
            &[20, 20, 20]
        );
        assert_eq!(snapshot.output_opacity(0.into()).unwrap(), 1.0);
    }

    #[wasm_bindgen_test]
    fn rejected_surface_matrix_does_not_poison_the_next_atomic_update() {
        let mut runtime =
            compile_program_wire(&reference_wire(), 7.into()).expect("canonical wire");
        assert!(
            runtime
                .update_observed(1_u64.into(), &[1], &[255, 255], 1.into())
                .is_err()
        );

        let snapshot = runtime
            .update_observed(1_u64.into(), &[1], &[255, 255, 255], 1.into())
            .expect("valid update after refusal");
        assert_eq!(snapshot.state(), "ready");
        assert_eq!(snapshot.output_count(), 1);
    }
}
