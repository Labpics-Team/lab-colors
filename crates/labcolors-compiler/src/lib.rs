//! Offline compiler boundary for `@labpics/colors/compiler`.
//!
//! This crate is intentionally a mechanical WASM shell over the versioned
//! protocol. It owns no theme vocabulary, runtime state, colour solver or wire
//! projection; those remain in `labcolors-core` and `labcolors-protocol`.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_COMPILER_TYPES: &'static str = r##"
import type { Wcag22CriterionV1 } from "../wcag22.js";

/**
 * Exact non-negative `u64` emitted as canonical decimal JSON text.
 *
 * Output-only branding prevents TypeScript from pretending that an arbitrary
 * integer-looking string has passed the Rust range/canonicality check.
 */
declare const decimalU64V1Brand: unique symbol;
export type DecimalU64V1 = string & {
  readonly [decimalU64V1Brand]: "DecimalU64V1";
};

/** One exact final encoded-sRGB8 colour. */
export type Srgb8BytesV1 = readonly [number, number, number];

/** One exact SHA-256 digest or 256-bit LSB0 partition. */
export type Bytes32V1 = readonly [
  number, number, number, number, number, number, number, number,
  number, number, number, number, number, number, number, number,
  number, number, number, number, number, number, number, number,
  number, number, number, number, number, number, number, number,
];

export interface Wcag22FeasibilityApplicableRelationV1 {
  readonly relationId: string;
  readonly occurrenceId: string;
  readonly kind: "applicable";
  readonly criterion: Wcag22CriterionV1;
  readonly adjacent: ReadonlyArray<Srgb8BytesV1>;
}

export interface Wcag22FeasibilityNotApplicableRelationV1 {
  readonly relationId: string;
  readonly occurrenceId: string;
  readonly kind: "notApplicable";
  readonly reasonId: string;
}

export type Wcag22FeasibilityRelationV1 =
  | Wcag22FeasibilityApplicableRelationV1
  | Wcag22FeasibilityNotApplicableRelationV1;

/** Strict decoded form of the UTF-8 JSON accepted by the byte API. */
export interface Wcag22FeasibilityRequestV1 {
  readonly schemaVersion: 1;
  readonly domainId: "srgb8-neutral-axis-v1";
  readonly resourceProfileId: "compile-v1";
  readonly relations: ReadonlyArray<Wcag22FeasibilityRelationV1>;
}

export interface Wcag22FeasibilityProofV1 {
  readonly evaluationId: Bytes32V1;
  readonly resourceProfileId: "compile-v1";
  readonly domainId: "srgb8-neutral-axis-v1";
  readonly domainDigest: Bytes32V1;
  readonly domainCount: DecimalU64V1;
  readonly domainFirst: Srgb8BytesV1;
  readonly domainLast: Srgb8BytesV1;
  readonly relationSetDigest: Bytes32V1;
  readonly canonicalRelations: DecimalU64V1;
  readonly applicableRelations: DecimalU64V1;
  readonly notApplicableRelations: DecimalU64V1;
  readonly applicableEdges: DecimalU64V1;
  readonly logicalAssessments: DecimalU64V1;
  readonly matrixDigest: Bytes32V1;
  /** Exact 256-bit candidate partition, candidate-index LSB0. */
  readonly partition: Bytes32V1;
  readonly wcag22ProfileId: "wcag22-srgb8-contrast-v1";
  readonly artifactId: "wcag22-srgb8-luminance-q55-v1";
  readonly boundId: "wcag22-srgb8-outward-q55-v1";
  readonly proofId: "wcag22-srgb8-full-domain-q55-v1";
  readonly proofSha256: Bytes32V1;
}

export interface Wcag22FeasibilityEvaluatedV1 {
  /** The complete registered domain in Core-owned candidate order, once. */
  readonly domain: ReadonlyArray<Srgb8BytesV1>;
  /** Canonical declarations, once; no per-cell relation duplication. */
  readonly relations: ReadonlyArray<Wcag22FeasibilityRelationV1>;
  /** Candidate-major failure bits at `candidate * E + edge`, packed LSB0. */
  readonly failureMatrix: ReadonlyArray<number>;
  readonly proof: Wcag22FeasibilityProofV1;
}

export interface Wcag22FeasibilityNotEvaluatedResultV1 {
  readonly domainId: "srgb8-neutral-axis-v1";
  readonly domainDigest: Bytes32V1;
  readonly relationSetDigest: Bytes32V1;
  readonly resourceProfileId: "compile-v1";
  readonly relations: ReadonlyArray<Wcag22FeasibilityNotApplicableRelationV1>;
}

export type Wcag22FeasibilityV1 =
  | { readonly status: "feasible"; readonly result: Wcag22FeasibilityEvaluatedV1 }
  | { readonly status: "infeasible"; readonly result: Wcag22FeasibilityEvaluatedV1 }
  | { readonly status: "notEvaluated"; readonly result: Wcag22FeasibilityNotEvaluatedResultV1 };

export type Wcag22FeasibilityTransportErrorV1 =
  | {
      readonly code: "envelopeTooLarge";
      readonly requestedBytes: DecimalU64V1;
      readonly limitBytes: DecimalU64V1;
    }
  | { readonly code: "invalidUtf8" }
  | {
      readonly code: "malformedEnvelope";
      readonly class: "syntax" | "shape" | "endOfInput" | "io";
    }
  | { readonly code: "unsupportedSchemaVersion"; readonly received: number }
  | { readonly code: "unsupportedDomainId"; readonly received: string }
  | { readonly code: "unsupportedResourceProfileId"; readonly received: string }
  | { readonly code: "unsupportedCriterion"; readonly received: string }
  | { readonly code: "emptyNotApplicableReason" };

export type Wcag22FeasibilityInvalidRequestV1 =
  | { readonly code: "emptyRelationId" }
  | { readonly code: "emptyOccurrenceId" }
  | { readonly code: "emptyRelations" }
  | { readonly code: "emptyAdjacentSet"; readonly relationId: string }
  | { readonly code: "conflictingRelationId"; readonly relationId: string }
  | { readonly code: "arithmeticOverflow" };

export type Wcag22FeasibilityAtomicErrorV1 =
  | { readonly code: "invalidSrgb8"; readonly field: string; readonly reason: string }
  | { readonly code: "emptyNotApplicableReason" }
  | {
      readonly code: "artifactInvariantViolation";
      readonly criterion: Wcag22CriterionV1;
      readonly foreground: Srgb8BytesV1;
      readonly background: Srgb8BytesV1;
    }
  | { readonly code: "evidenceRegistryMismatch"; readonly message: string };

export type Wcag22FeasibilityEvaluatorInvariantV1 =
  | { readonly code: "source"; readonly details: Wcag22FeasibilityAtomicErrorV1 }
  | { readonly code: "unexpectedNotEvaluated" }
  | { readonly code: "inputMismatch" }
  | { readonly code: "criterionMismatch" }
  | { readonly code: "evidenceMismatch" };

export type Wcag22FeasibilityCompilerInvariantV1 =
  | { readonly code: "layoutMismatch" }
  | {
      readonly code: "assessmentCardinalityMismatch";
      readonly expected: DecimalU64V1;
      readonly observed: DecimalU64V1;
    }
  | {
      readonly code: "candidateCardinalityMismatch";
      readonly expected: DecimalU64V1;
      readonly observed: DecimalU64V1;
    }
  | { readonly code: "decisionStorageRejectedCell" }
  | { readonly code: "decisionStorageRejectedPartition" }
  | { readonly code: "completeResultMismatch" };

export type Wcag22FeasibilityResourceDimensionV1 =
  | "rawRelations"
  | "rawAdjacentEntries"
  | "opaqueUtf8Bytes"
  | "canonicalRelations"
  | "applicableEdges"
  | "logicalAssessments"
  | "packedResultBytes";

export type Wcag22FeasibilityCoreErrorV1 =
  | {
      readonly code: "invalidRequest";
      readonly details: Wcag22FeasibilityInvalidRequestV1;
    }
  | {
      readonly code: "resourceLimitExceeded";
      readonly details: {
        readonly profileId: "compile-v1";
        readonly dimension: Wcag22FeasibilityResourceDimensionV1;
        readonly requested: DecimalU64V1;
        readonly limit: DecimalU64V1;
      };
    }
  | {
      readonly code: "allocationFailed";
      readonly details: {
        readonly profileId: "compile-v1";
        readonly requestedBytes: DecimalU64V1;
      };
    }
  | {
      readonly code: "evaluatorInvariantViolation";
      readonly details: {
        readonly candidate: Srgb8BytesV1;
        readonly relationId: string;
        readonly adjacent: Srgb8BytesV1;
        readonly violation: Wcag22FeasibilityEvaluatorInvariantV1;
      };
    }
  | {
      readonly code: "compilerInvariantViolation";
      readonly details: Wcag22FeasibilityCompilerInvariantV1;
    };

export type Wcag22FeasibilityProtocolErrorV1 =
  | { readonly source: "transport"; readonly error: Wcag22FeasibilityTransportErrorV1 }
  | { readonly source: "core"; readonly error: Wcag22FeasibilityCoreErrorV1 }
  | { readonly source: "incompatibleCoreContract" };

export type Wcag22FeasibilityOutcomeV1 =
  | {
      readonly schemaVersion: 1;
      readonly outcome: "success";
      readonly feasibility: Wcag22FeasibilityV1;
    }
  | {
      readonly schemaVersion: 1;
      readonly outcome: "failure";
      readonly error: Wcag22FeasibilityProtocolErrorV1;
    };
"##;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Wcag22FeasibilityOutcomeV1")]
    pub type JsWcag22FeasibilityOutcomeV1;
}

const WASM_MAX_ENVELOPE_BYTES_V1: u32 = {
    assert!(labcolors_protocol::MAX_ENVELOPE_BYTES_V1 <= u32::MAX as u64);
    labcolors_protocol::MAX_ENVELOPE_BYTES_V1 as u32
};

/// Exact protocol-owned ceiling exposed as a JavaScript `number`.
#[wasm_bindgen(js_name = wcag22FeasibilityMaxRequestBytesV1)]
pub fn wcag22_feasibility_max_request_bytes_v1() -> u32 {
    WASM_MAX_ENVELOPE_BYTES_V1
}

/// Evaluate one strict V1 UTF-8 JSON byte envelope.
#[wasm_bindgen(js_name = evaluateWcag22FeasibilityV1)]
pub fn evaluate_wcag22_feasibility_v1(
    request: &[u8],
) -> Result<JsWcag22FeasibilityOutcomeV1, JsError> {
    protocol_outcome_to_js(labcolors_protocol::evaluate_wcag22_feasibility_v1(request))
}

/// Construct the canonical oversize failure without copying a rejected input.
#[wasm_bindgen(js_name = wcag22FeasibilityEnvelopeTooLargeV1)]
pub fn wcag22_feasibility_envelope_too_large_v1(
    requested_bytes: u64,
) -> Result<JsWcag22FeasibilityOutcomeV1, JsError> {
    protocol_outcome_to_js(labcolors_protocol::envelope_too_large_outcome_v1(
        requested_bytes,
    ))
}

fn protocol_outcome_to_js(
    outcome: labcolors_protocol::ProtocolOutcomeV1,
) -> Result<JsWcag22FeasibilityOutcomeV1, JsError> {
    let encoded = labcolors_protocol::encode_outcome_v1(&outcome)
        .map_err(|error| JsError::new(&format!("protocol encoding failed: {error}")))?;
    let json = std::str::from_utf8(&encoded)
        .map_err(|_| JsError::new("protocol emitted non-UTF-8 JSON"))?;
    let parsed =
        js_sys::JSON::parse(json).map_err(|_| JsError::new("protocol JSON did not parse"))?;
    Ok(parsed.unchecked_into())
}

// ─────────────────────────────────────────────────────────────────────────────
// Атомарная операция `wcag22-explicit-selection-v1` (#296-C3)
// ─────────────────────────────────────────────────────────────────────────────

#[wasm_bindgen(typescript_custom_section)]
const TS_EXPLICIT_SELECTION_TYPES: &'static str = r##"
/** One explicit client-declared candidate. */
export interface Wcag22ExplicitCandidateV1 {
  readonly candidateId: string;
  readonly emitted: Srgb8BytesV1;
}

/** The sole V1 selection policy: first feasible ID in declared order. */
export interface Wcag22ExplicitSelectionPolicyV1 {
  readonly policyKind: "first-feasible-in-declared-order-v1";
  readonly policyId: string;
  readonly orderedCandidateIds: ReadonlyArray<string>;
}

/** Strict decoded form of the UTF-8 JSON accepted by the byte API. */
export interface Wcag22ExplicitSelectionRequestV1 {
  readonly schemaVersion: 1;
  readonly domainId: "explicit-srgb8-set-v1";
  readonly resourceProfileId: "compile-v1";
  readonly candidates: ReadonlyArray<Wcag22ExplicitCandidateV1>;
  readonly relations: ReadonlyArray<Wcag22FeasibilityRelationV1>;
  readonly policy: Wcag22ExplicitSelectionPolicyV1;
}

/** Domain-neutral explicit-set proof: no neutral-only first/last fields. */
export interface Wcag22ExplicitEvaluationProofV1 {
  readonly evaluationId: Bytes32V1;
  readonly resourceProfileId: "compile-v1";
  readonly domainKind: "explicit-srgb8-set-v1";
  readonly domainDigest: Bytes32V1;
  readonly candidateCount: DecimalU64V1;
  readonly relationSetDigest: Bytes32V1;
  readonly canonicalRelations: DecimalU64V1;
  readonly applicableRelations: DecimalU64V1;
  readonly notApplicableRelations: DecimalU64V1;
  readonly applicableEdges: DecimalU64V1;
  readonly logicalAssessments: DecimalU64V1;
  readonly matrixDigest: Bytes32V1;
  /** Variable-width feasible partition, candidate-index LSB0. */
  readonly partition: ReadonlyArray<number>;
  readonly wcag22ProfileId: "wcag22-srgb8-contrast-v1";
  readonly artifactId: "wcag22-srgb8-luminance-q55-v1";
  readonly boundId: "wcag22-srgb8-outward-q55-v1";
  readonly proofId: "wcag22-srgb8-full-domain-q55-v1";
  readonly proofSha256: Bytes32V1;
}

export interface Wcag22ExplicitEvaluatedV1 {
  /** Canonical candidates in exact ID-byte order, once. */
  readonly candidates: ReadonlyArray<Wcag22ExplicitCandidateV1>;
  readonly relations: ReadonlyArray<Wcag22FeasibilityRelationV1>;
  /** Candidate-major failure bits at `candidate * E + edge`, packed LSB0. */
  readonly failureMatrix: ReadonlyArray<number>;
  readonly proof: Wcag22ExplicitEvaluationProofV1;
}

export interface Wcag22ExplicitNotEvaluatedV1 {
  readonly domainKind: "explicit-srgb8-set-v1";
  readonly domainDigest: Bytes32V1;
  readonly candidateCount: DecimalU64V1;
  readonly relationSetDigest: Bytes32V1;
  readonly resourceProfileId: "compile-v1";
  readonly candidates: ReadonlyArray<Wcag22ExplicitCandidateV1>;
  readonly relations: ReadonlyArray<Wcag22FeasibilityNotApplicableRelationV1>;
}

export interface Wcag22ExplicitFinalVerificationV1 {
  readonly relationSetDigest: Bytes32V1;
  readonly verifiedApplicableEdges: DecimalU64V1;
  readonly wcag22ProfileId: "wcag22-srgb8-contrast-v1";
  readonly artifactId: "wcag22-srgb8-luminance-q55-v1";
  readonly boundId: "wcag22-srgb8-outward-q55-v1";
  readonly proofId: "wcag22-srgb8-full-domain-q55-v1";
  readonly proofSha256: Bytes32V1;
  readonly receiptDigest: Bytes32V1;
}

export interface Wcag22ExplicitSelectedV1 {
  readonly candidateId: string;
  readonly emitted: Srgb8BytesV1;
  readonly evaluationId: Bytes32V1;
  readonly policyId: string;
  readonly policyDigest: Bytes32V1;
  readonly selectedPolicyOrdinal: DecimalU64V1;
  readonly receiptDigest: Bytes32V1;
  readonly finalVerification: Wcag22ExplicitFinalVerificationV1;
}

export interface Wcag22ExplicitNoSelectionV1 {
  readonly reason: "noDeclaredCandidateFeasible";
  readonly policyId: string;
  readonly policyDigest: Bytes32V1;
  readonly evaluationId: Bytes32V1;
}

export interface Wcag22ExplicitPolicyBindingV1 {
  readonly policyId: string;
  readonly policyDigest: Bytes32V1;
  readonly declaredEntries: DecimalU64V1;
}

export type Wcag22ExplicitSelectionResultV1 =
  | {
      readonly status: "selected";
      readonly feasibility: Wcag22ExplicitEvaluatedV1;
      readonly selection: Wcag22ExplicitSelectedV1;
    }
  | {
      readonly status: "noSelection";
      readonly feasibility: Wcag22ExplicitEvaluatedV1;
      readonly selection: Wcag22ExplicitNoSelectionV1;
    }
  | {
      readonly status: "infeasible";
      readonly feasibility: Wcag22ExplicitEvaluatedV1;
      readonly policy: Wcag22ExplicitPolicyBindingV1;
    }
  | {
      readonly status: "notEvaluated";
      readonly feasibility: Wcag22ExplicitNotEvaluatedV1;
      readonly policy: Wcag22ExplicitPolicyBindingV1;
    };

/** Explicit-domain invalid-request codes extend the shared feasibility set. */
export type Wcag22ExplicitFeasibilityInvalidRequestV1 =
  | Wcag22FeasibilityInvalidRequestV1
  | { readonly code: "emptyCandidateId" }
  | { readonly code: "emptyCandidates" }
  | { readonly code: "duplicateCandidateId"; readonly candidateId: string };

/** Feasibility-phase failures of the atomic operation. */
export type Wcag22ExplicitFeasibilityCoreErrorV1 =
  | Exclude<Wcag22FeasibilityCoreErrorV1, { readonly code: "invalidRequest" }>
  | {
      readonly code: "invalidRequest";
      readonly details: Wcag22ExplicitFeasibilityInvalidRequestV1;
    };

export type Wcag22ExplicitSelectionTransportErrorV1 =
  | Wcag22FeasibilityTransportErrorV1
  | { readonly code: "unsupportedPolicyKind"; readonly received: string };

export type Wcag22ExplicitInvalidSelectionRequestV1 =
  | { readonly code: "emptyPolicyId" }
  | { readonly code: "emptyCandidateOrder" }
  | { readonly code: "emptyCandidateId" }
  | { readonly code: "arithmeticOverflow" }
  | {
      readonly code: "policyCardinalityExceedsDomain";
      readonly requested: DecimalU64V1;
      readonly domain: DecimalU64V1;
    }
  | { readonly code: "foreignCandidateId"; readonly candidateId: string }
  | { readonly code: "duplicateCandidateId"; readonly candidateId: string };

export type Wcag22ExplicitDecisionV1 = "pass" | "fail";

export type Wcag22ExplicitSelectionIntegrityViolationV1 =
  | {
      readonly code: "evaluatorContract";
      readonly candidateId: string;
      readonly relationId: string;
      readonly adjacent: Srgb8BytesV1;
      readonly violation: Wcag22FeasibilityEvaluatorInvariantV1;
    }
  | {
      readonly code: "sealedDecisionMismatch";
      readonly candidateId: string;
      readonly relationId: string;
      readonly adjacent: Srgb8BytesV1;
      readonly sealed: Wcag22ExplicitDecisionV1;
      readonly rechecked: Wcag22ExplicitDecisionV1;
    }
  | {
      readonly code: "selectedRowNotPassing";
      readonly candidateId: string;
      readonly relationId: string;
      readonly adjacent: Srgb8BytesV1;
    }
  | {
      readonly code: "applicableEdgeCountMismatch";
      readonly expected: DecimalU64V1;
      readonly observed: DecimalU64V1;
    }
  | { readonly code: "sealedTraversalArithmeticOverflow" };

export type Wcag22ExplicitSelectionErrorV1 =
  | {
      readonly code: "invalidRequest";
      readonly details: Wcag22ExplicitInvalidSelectionRequestV1;
    }
  | {
      readonly code: "resourceLimitExceeded";
      readonly details: {
        readonly profileId: "compile-v1";
        readonly dimension: Wcag22FeasibilityResourceDimensionV1;
        readonly requested: DecimalU64V1;
        readonly limit: DecimalU64V1;
      };
    }
  | {
      readonly code: "integrityViolation";
      readonly details: Wcag22ExplicitSelectionIntegrityViolationV1;
    };

export type Wcag22ExplicitSelectionOperationErrorV1 =
  | {
      readonly source: "transport";
      readonly error: Wcag22ExplicitSelectionTransportErrorV1;
    }
  | {
      readonly source: "feasibility";
      readonly error: Wcag22ExplicitFeasibilityCoreErrorV1;
    }
  | { readonly source: "selection"; readonly error: Wcag22ExplicitSelectionErrorV1 }
  | { readonly source: "incompatibleCoreContract" };

export type Wcag22ExplicitSelectionOutcomeV1 =
  | {
      readonly schemaVersion: 1;
      readonly outcome: "success";
      readonly result: Wcag22ExplicitSelectionResultV1;
    }
  | {
      readonly schemaVersion: 1;
      readonly outcome: "failure";
      readonly error: Wcag22ExplicitSelectionOperationErrorV1;
    };
"##;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Wcag22ExplicitSelectionOutcomeV1")]
    pub type JsWcag22ExplicitSelectionOutcomeV1;
}

const WASM_MAX_EXPLICIT_SELECTION_ENVELOPE_BYTES_V1: u32 = {
    assert!(
        labcolors_protocol::explicit_selection::MAX_EXPLICIT_SELECTION_ENVELOPE_BYTES_V1
            <= u32::MAX as u64
    );
    labcolors_protocol::explicit_selection::MAX_EXPLICIT_SELECTION_ENVELOPE_BYTES_V1 as u32
};

/// Exact protocol-owned ceiling of the atomic operation as a JS `number`.
#[wasm_bindgen(js_name = wcag22ExplicitSelectionMaxRequestBytesV1)]
pub fn wcag22_explicit_selection_max_request_bytes_v1() -> u32 {
    WASM_MAX_EXPLICIT_SELECTION_ENVELOPE_BYTES_V1
}

/// Evaluate one strict atomic `wcag22-explicit-selection-v1` byte envelope.
#[wasm_bindgen(js_name = evaluateWcag22ExplicitSelectionV1)]
pub fn evaluate_wcag22_explicit_selection_v1(
    request: &[u8],
) -> Result<JsWcag22ExplicitSelectionOutcomeV1, JsError> {
    explicit_selection_outcome_to_js(
        labcolors_protocol::explicit_selection::evaluate_wcag22_explicit_selection_v1(request),
    )
}

/// Construct the canonical oversize failure without copying a rejected input.
#[wasm_bindgen(js_name = wcag22ExplicitSelectionEnvelopeTooLargeV1)]
pub fn wcag22_explicit_selection_envelope_too_large_v1(
    requested_bytes: u64,
) -> Result<JsWcag22ExplicitSelectionOutcomeV1, JsError> {
    explicit_selection_outcome_to_js(
        labcolors_protocol::explicit_selection::explicit_selection_envelope_too_large_outcome_v1(
            requested_bytes,
        ),
    )
}

fn explicit_selection_outcome_to_js(
    outcome: labcolors_protocol::explicit_selection::OutcomeV1,
) -> Result<JsWcag22ExplicitSelectionOutcomeV1, JsError> {
    let encoded =
        labcolors_protocol::explicit_selection::encode_explicit_selection_outcome_v1(&outcome)
            .map_err(|error| JsError::new(&format!("protocol encoding failed: {error}")))?;
    let json = std::str::from_utf8(&encoded)
        .map_err(|_| JsError::new("protocol emitted non-UTF-8 JSON"))?;
    let parsed =
        js_sys::JSON::parse(json).map_err(|_| JsError::new("protocol JSON did not parse"))?;
    Ok(parsed.unchecked_into())
}
