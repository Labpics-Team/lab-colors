/// <reference lib="esnext.disposable" />

// Terminal C7c public surface: canonical Program wire is the sole authoring
// and runtime root. Recipe DTOs and legacy theme helpers are not re-exported.

import type { Wcag22AssessmentV1 } from "./pkg/labcolors.js";
import type { ProgramAttachment } from "./pkg/labcolors.js";
import type { Wcag22CriterionV1 } from "./wcag22.js";

export {
  compileProgramWire,
  numericalCapabilityManifest,
  ProgramRuntime,
  ProgramSnapshot,
  ProgramAttachment,
  ProgramAttachedSnapshot,
  ProgramAttachedRender,
  AttachedMaterializationAuthority,
} from "./pkg/labcolors.js";

export type {
  NumericalCapabilitySiteV2,
  NumericalCapabilityManifestV2,
  Wcag22DecisionV1,
  Wcag22Q55BoundsV1,
  Wcag22AssessmentV1,
} from "./pkg/labcolors.js";
export type { Wcag22CriterionV1 } from "./wcag22.js";

export type ProgramCompileErrorCode =
  | "program_wire"
  | "program_compile"
  | "program_family_artifacts_required"
  | "program_instantiate";
export type ProgramAttachmentErrorCode =
  | ProgramCompileErrorCode
  | "program_attachment_binding"
  | "program_attachment_instantiate"
  | "program_attachment_sink_admission"
  | "program_attachment_resource_exhausted"
  | "program_attachment_non_terminal_target";
export type ProgramAttachmentUpdateErrorCode =
  | "program_attachment_update"
  | "program_attachment_resource_exhausted"
  | "program_attachment_internal_invariant"
  | "program_attachment_already_disposed"
  | "program_attachment_patch_scope_mismatch"
  | "program_attachment_stamp_mismatch"
  | "program_attachment_revision_mismatch"
  | "program_attachment_already_installed"
  | "program_attachment_host_rejected"
  | "program_attachment_host_protocol"
  | "program_attachment_busy";
export type ProgramAttachmentDisposeErrorCode =
  | "program_attachment_already_disposed"
  | "program_attachment_revoke_unconfirmed"
  | "program_attachment_dispose"
  | "program_attachment_busy";
export type ProgramAttachmentFreeErrorCode =
  | "program_attachment_revoke_unconfirmed"
  | "program_attachment_busy";
export type ProgramPhysicalIdentityErrorCode = "program_physical_identity";
export type ProgramMaterializationErrorCode =
  | "program_materialization_not_ready"
  | "program_materialization_paint_not_authority"
  | "program_materialization_stale_revision"
  | "program_materialization_stale_identity"
  | "program_materialization_stale_sink_stamp"
  | "program_materialization_foreign_binding_epoch"
  | "program_materialization_terminal_binding_mismatch"
  | "program_materialization_missing_point_absence_proof"
  | "program_materialization_ambiguous_observation_cases"
  | "program_attachment_busy";
export type ProgramUpdateOperation = "updateObserved" | "updateUnknown";
export type ProgramError = Error & (
  | Readonly<{ code: ProgramCompileErrorCode; operation: "compileProgramWire" }>
  | Readonly<{ code: "program_update"; operation: ProgramUpdateOperation }>
  | Readonly<{ code: ProgramAttachmentErrorCode; operation: "attachProgramWire" }>
  | Readonly<{
      code: ProgramAttachmentUpdateErrorCode;
      operation: "attachmentUpdateObserved" | "attachmentUpdateUnknown";
    }>
  | Readonly<{ code: ProgramAttachmentDisposeErrorCode; operation: "attachmentDispose" }>
  | Readonly<{ code: ProgramAttachmentFreeErrorCode; operation: "attachmentFree" }>
  | Readonly<{ code: ProgramPhysicalIdentityErrorCode; operation: "physicalIdentity" }>
  | Readonly<{ code: ProgramMaterializationErrorCode; operation: "materializationAuthority" }>
);
export type ProgramErrorCode = ProgramError["code"];
export type ProgramOperation = ProgramError["operation"];
export declare function isProgramError(error: unknown): error is ProgramError;

export type CertificateErrorCode =
  | "certificate_invalid_magic"
  | "certificate_unsupported_schema"
  | "certificate_unknown_operation"
  | "certificate_unknown_authority_kind"
  | "certificate_unsupported_authority_version"
  | "certificate_invalid_utf8"
  | "certificate_invalid_length"
  | "certificate_truncated_input"
  | "certificate_trailing_bytes"
  | "certificate_non_canonical_revision"
  | "certificate_invalid_payload_type"
  | "certificate_unsupported_payload_version"
  | "certificate_payload_digest_mismatch"
  | "certificate_binding_digest_mismatch"
  | "certificate_missing_producer_attestation"
  | "certificate_producer_binding_mismatch"
  | "certificate_runtime_artifact_mismatch"
  | "certificate_producer_revision_mismatch"
  | "certificate_content_identity_mismatch"
  | "certificate_context_mismatch"
  | "certificate_resource_limit_exceeded"
  | "certificate_admission_capacity_exceeded"
  | "certificate_unsupported_opaque"
  | "certificate_binding_conflict"
  | "certificate_invalid_input";
export type CertificateError = Error & Readonly<{
  code: CertificateErrorCode;
  operation: "decodeCertificateEnvelope";
}>;
export interface UntrustedCertificateEnvelopeV1 {
  readonly schemaVersion: 1;
  readonly operation: "issue-certificate";
  readonly authorityKind: "generic-typed-certificate";
  readonly authorityVersion: 1;
  readonly runtimeArtifactId: string;
  readonly producerRevision: string;
  readonly producerContentIdentity: Uint8Array;
  readonly contextId: string;
  readonly payloadType: "non-semantic-transport-v1";
  readonly payloadVersion: 1;
  readonly payloadLength: number;
  readonly payloadSha256: Uint8Array;
  readonly bindingSha256: Uint8Array;
}
export declare const MAX_CERTIFICATE_ENVELOPE_BYTES: 2097152;
export declare function decodeCertificateEnvelope(
  bytes: Uint8Array,
): UntrustedCertificateEnvelopeV1;
export declare function isCertificateError(error: unknown): error is CertificateError;

export type ProgramPointSinkOperation = "setAll" | "revokeAll" | "confirmExact";
export interface ProgramPointSinkPoint {
  readonly slot: number;
  readonly source: Uint8Array;
  readonly opacity: number;
}
export interface ProgramPointSinkIntent {
  readonly operation: ProgramPointSinkOperation;
  readonly revision: bigint;
  readonly expectedSequence: bigint;
  readonly desiredSequence: bigint;
  readonly bindingEpoch: bigint;
  readonly sinkOutput: number;
  readonly point: ProgramPointSinkPoint | null;
}
export type ProgramPointSinkHost = (intent: ProgramPointSinkIntent) => boolean;

type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;
type SyncInitInput = BufferSource | WebAssembly.Module;

export declare function init(
  input?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>,
): Promise<void>;
export declare function initSync(input: { module: SyncInitInput } | SyncInitInput): void;
export default init;

export declare function evaluateWcag22(
  foreground: string,
  background: string,
  criterion: Wcag22CriterionV1,
): Wcag22AssessmentV1;

export declare function attachProgramWire(
  bytes: Uint8Array,
  streamId: number,
  outputSlot: number,
  sinkOutput: number,
  presentationRoot: number,
  occurrence: number,
  host: ProgramPointSinkHost,
): ProgramAttachment;
