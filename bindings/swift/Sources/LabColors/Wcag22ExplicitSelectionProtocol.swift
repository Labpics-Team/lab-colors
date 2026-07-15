import Foundation

/// Typed Swift projection of the atomic `wcag22-explicit-selection-v1`
/// operation (#296-C3). The wire truth is
/// `crates/labcolors-protocol/src/explicit_selection.rs`: one request carries
/// only the client-declared finite domain, relations, resource profile and
/// policy; every count, digest, matrix, evaluation ID, proof and receipt is
/// derived and sealed by Core in a single call. These types therefore decode
/// outcomes only — proof-bearing structs expose `let` fields without public
/// initializers, and `Wcag22ExplicitSelectionOutcomeV1.init(from:)` re-checks
/// the structural terminal laws before a value escapes the decoder.

/// Stable Core key of the single V1 explicit domain kind.
public enum Wcag22ExplicitDomainKindV1: String, Codable, Sendable {
    case explicitSrgb8SetV1 = "explicit-srgb8-set-v1"
}

/// One canonical explicit candidate: opaque client identity plus exact final
/// encoded-sRGB8 bytes.
public struct Wcag22ExplicitCandidateV1: Codable, Equatable, Sendable {
    public let candidateId: String
    public let emitted: Srgb8BytesV1
}

/// Sealed fully projected proof of one complete explicit enumeration.
/// Domain-neutral descriptor: kind, digest and exact finite cardinality; the
/// neutral-only `domainFirst`/`domainLast` fields do not exist here, and the
/// feasible partition is variable-width (`ceil(candidates/8)` bytes, LSB0 by
/// canonical candidate index).
public struct Wcag22ExplicitSelectionEvaluationProofV1: Codable, Equatable, Sendable {
    public let evaluationId: Bytes32V1
    public let resourceProfileId: Wcag22FeasibilityResourceProfileIdV1
    public let domainKind: Wcag22ExplicitDomainKindV1
    public let domainDigest: Bytes32V1
    public let candidateCount: DecimalU64V1
    public let relationSetDigest: Bytes32V1
    public let canonicalRelations: DecimalU64V1
    public let applicableRelations: DecimalU64V1
    public let notApplicableRelations: DecimalU64V1
    public let applicableEdges: DecimalU64V1
    public let logicalAssessments: DecimalU64V1
    public let matrixDigest: Bytes32V1
    public let partition: [UInt8]
    public let wcag22ProfileId: Wcag22FeasibilityProfileIdV1
    public let artifactId: Wcag22FeasibilityArtifactIdV1
    public let boundId: Wcag22FeasibilityBoundIdV1
    public let proofId: Wcag22FeasibilityProofIdV1
    public let proofSha256: Bytes32V1
}

/// Complete evaluated payload shared by the `selected`, `noSelection` and
/// `infeasible` terminals. Candidates are transported once, in canonical
/// (byte-sorted by opaque ID) order.
public struct Wcag22ExplicitSelectionEvaluatedV1: Codable, Equatable, Sendable {
    public let candidates: [Wcag22ExplicitCandidateV1]
    public let relations: [Wcag22FeasibilityRelationV1]
    public let failureMatrix: [UInt8]
    public let proof: Wcag22ExplicitSelectionEvaluationProofV1
}

/// Declaration-only terminal without fabricated numerical evidence.
public struct Wcag22ExplicitSelectionNotEvaluatedV1: Codable, Equatable, Sendable {
    public let domainKind: Wcag22ExplicitDomainKindV1
    public let domainDigest: Bytes32V1
    public let candidateCount: DecimalU64V1
    public let relationSetDigest: Bytes32V1
    public let resourceProfileId: Wcag22FeasibilityResourceProfileIdV1
    public let candidates: [Wcag22ExplicitCandidateV1]
    public let relations: [Wcag22FeasibilityRelationV1]
}

/// Sealed final recheck of the selected row against the same numerical proof.
public struct Wcag22ExplicitSelectionFinalVerificationV1: Codable, Equatable, Sendable {
    public let relationSetDigest: Bytes32V1
    public let verifiedApplicableEdges: DecimalU64V1
    public let wcag22ProfileId: Wcag22FeasibilityProfileIdV1
    public let artifactId: Wcag22FeasibilityArtifactIdV1
    public let boundId: Wcag22FeasibilityBoundIdV1
    public let proofId: Wcag22FeasibilityProofIdV1
    public let proofSha256: Bytes32V1
    public let receiptDigest: Bytes32V1
}

/// Sealed selected candidate with its final receipt.
public struct Wcag22ExplicitSelectionSelectedV1: Codable, Equatable, Sendable {
    public let candidateId: String
    public let emitted: Srgb8BytesV1
    public let evaluationId: Bytes32V1
    public let policyId: String
    public let policyDigest: Bytes32V1
    public let selectedPolicyOrdinal: DecimalU64V1
    public let receiptDigest: Bytes32V1
    public let finalVerification: Wcag22ExplicitSelectionFinalVerificationV1
}

/// Exhaustive reason for the absence of a selection.
public enum Wcag22ExplicitSelectionNoSelectionReasonV1: String, Codable, Sendable {
    case noDeclaredCandidateFeasible
}

/// Sealed refusal of a valid policy without a hidden fallback.
public struct Wcag22ExplicitSelectionNoSelectionV1: Codable, Equatable, Sendable {
    public let reason: Wcag22ExplicitSelectionNoSelectionReasonV1
    public let policyId: String
    public let policyDigest: Bytes32V1
    public let evaluationId: Bytes32V1
}

/// Sealed binding of a fully validated policy without a selection receipt.
public struct Wcag22ExplicitSelectionPolicyBindingV1: Codable, Equatable, Sendable {
    public let policyId: String
    public let policyDigest: Bytes32V1
    public let declaredEntries: DecimalU64V1
}

/// Successful result: exactly one of the four lawful terminals. The enum shape
/// itself guarantees that `infeasible`/`notEvaluated` cannot carry a selection
/// and that `selected`/`noSelection` cannot drop their evaluated evidence.
public enum Wcag22ExplicitSelectionResultV1: Codable, Equatable, Sendable {
    case selected(
        feasibility: Wcag22ExplicitSelectionEvaluatedV1,
        selection: Wcag22ExplicitSelectionSelectedV1
    )
    case noSelection(
        feasibility: Wcag22ExplicitSelectionEvaluatedV1,
        selection: Wcag22ExplicitSelectionNoSelectionV1
    )
    case infeasible(
        feasibility: Wcag22ExplicitSelectionEvaluatedV1,
        policy: Wcag22ExplicitSelectionPolicyBindingV1
    )
    case notEvaluated(
        feasibility: Wcag22ExplicitSelectionNotEvaluatedV1,
        policy: Wcag22ExplicitSelectionPolicyBindingV1
    )

    private enum CodingKeys: String, CodingKey { case status, feasibility, selection, policy }
    private enum Status: String, Codable { case selected, noSelection, infeasible, notEvaluated }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Status.self, forKey: .status) {
        case .selected:
            self = try .selected(
                feasibility: container.decode(
                    Wcag22ExplicitSelectionEvaluatedV1.self, forKey: .feasibility),
                selection: container.decode(
                    Wcag22ExplicitSelectionSelectedV1.self, forKey: .selection))
        case .noSelection:
            self = try .noSelection(
                feasibility: container.decode(
                    Wcag22ExplicitSelectionEvaluatedV1.self, forKey: .feasibility),
                selection: container.decode(
                    Wcag22ExplicitSelectionNoSelectionV1.self, forKey: .selection))
        case .infeasible:
            self = try .infeasible(
                feasibility: container.decode(
                    Wcag22ExplicitSelectionEvaluatedV1.self, forKey: .feasibility),
                policy: container.decode(
                    Wcag22ExplicitSelectionPolicyBindingV1.self, forKey: .policy))
        case .notEvaluated:
            self = try .notEvaluated(
                feasibility: container.decode(
                    Wcag22ExplicitSelectionNotEvaluatedV1.self, forKey: .feasibility),
                policy: container.decode(
                    Wcag22ExplicitSelectionPolicyBindingV1.self, forKey: .policy))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .selected(feasibility, selection):
            try container.encode(Status.selected, forKey: .status)
            try container.encode(feasibility, forKey: .feasibility)
            try container.encode(selection, forKey: .selection)
        case let .noSelection(feasibility, selection):
            try container.encode(Status.noSelection, forKey: .status)
            try container.encode(feasibility, forKey: .feasibility)
            try container.encode(selection, forKey: .selection)
        case let .infeasible(feasibility, policy):
            try container.encode(Status.infeasible, forKey: .status)
            try container.encode(feasibility, forKey: .feasibility)
            try container.encode(policy, forKey: .policy)
        case let .notEvaluated(feasibility, policy):
            try container.encode(Status.notEvaluated, forKey: .status)
            try container.encode(feasibility, forKey: .feasibility)
            try container.encode(policy, forKey: .policy)
        }
    }

    fileprivate func validate(codingPath: [CodingKey]) throws {
        func corrupted(_ message: String) -> DecodingError {
            .dataCorrupted(.init(codingPath: codingPath, debugDescription: message))
        }
        func utf8(_ id: String) -> [UInt8] { Array(id.utf8) }
        func bitBytes(_ bits: Int) -> Int { bits / 8 + (bits % 8 == 0 ? 0 : 1) }
        func lsb0(_ bytes: [UInt8], _ index: Int) -> Bool {
            (bytes[index / 8] & (UInt8(1) << UInt8(index % 8))) != 0
        }
        func relationCounts(
            _ relations: [Wcag22FeasibilityRelationV1]
        ) throws -> (applicable: Int, notApplicable: Int, edges: Int) {
            var applicable = 0
            var notApplicable = 0
            var edges = 0
            for relation in relations {
                switch relation {
                case let .applicable(_, _, _, adjacent):
                    applicable += 1
                    let (next, overflow) = edges.addingReportingOverflow(adjacent.count)
                    guard !overflow else { throw corrupted("applicable edge count overflow") }
                    edges = next
                case .notApplicable:
                    notApplicable += 1
                }
            }
            return (applicable, notApplicable, edges)
        }
        /// Canonical relation order mirrors Core: relations strictly
        /// increasing by raw relation-ID bytes, each applicable adjacency
        /// strictly increasing and unique. The transported relationSetDigest
        /// was computed by Core over exactly this order, so a reordered wire
        /// would silently disagree with its own sealed digest.
        func validateCanonicalRelations(_ relations: [Wcag22FeasibilityRelationV1]) throws {
            func relationId(_ relation: Wcag22FeasibilityRelationV1) -> String {
                switch relation {
                case let .applicable(id, _, _, _): return id
                case let .notApplicable(id, _, _): return id
                }
            }
            for pair in zip(relations, relations.dropFirst()) {
                guard utf8(relationId(pair.0))
                    .lexicographicallyPrecedes(utf8(relationId(pair.1)))
                else {
                    throw corrupted("relations differ from canonical byte-sorted ID order")
                }
            }
            for relation in relations {
                guard case let .applicable(_, _, _, adjacent) = relation else { continue }
                for pair in zip(adjacent, adjacent.dropFirst()) {
                    let left = [pair.0.red, pair.0.green, pair.0.blue]
                    let right = [pair.1.red, pair.1.green, pair.1.blue]
                    guard left.lexicographicallyPrecedes(right) else {
                        throw corrupted(
                            "applicable adjacency differs from canonical sorted unique order")
                    }
                }
            }
        }
        /// Canonical explicit order: opaque IDs strictly increasing by raw
        /// UTF-8 bytes (which also proves uniqueness). Swift `String` equality
        /// is Unicode-canonical, so the opaque byte law is checked on bytes.
        func validateCanonicalCandidates(_ candidates: [Wcag22ExplicitCandidateV1]) throws {
            guard !candidates.isEmpty else {
                throw corrupted("explicit domain must transport at least one candidate")
            }
            for pair in zip(candidates, candidates.dropFirst()) {
                guard utf8(pair.0.candidateId)
                    .lexicographicallyPrecedes(utf8(pair.1.candidateId))
                else {
                    throw corrupted("candidates differ from canonical byte-sorted ID order")
                }
            }
        }
        func validateEvaluated(
            _ value: Wcag22ExplicitSelectionEvaluatedV1,
            expectsFeasible: Bool
        ) throws {
            try validateCanonicalCandidates(value.candidates)
            try validateCanonicalRelations(value.relations)
            let candidateCount = value.candidates.count
            let counts = try relationCounts(value.relations)
            guard counts.applicable > 0, counts.edges > 0 else {
                throw corrupted("evaluated terminal must contain an applicable edge")
            }
            let (assessments, assessmentsOverflow) =
                candidateCount.multipliedReportingOverflow(by: counts.edges)
            guard !assessmentsOverflow else {
                throw corrupted("logical assessment count overflow")
            }
            guard value.failureMatrix.count == bitBytes(assessments) else {
                throw corrupted("failure matrix length does not equal ceil(C*E/8)")
            }
            guard value.proof.partition.count == bitBytes(candidateCount) else {
                throw corrupted("partition length does not equal ceil(C/8)")
            }
            guard value.proof.candidateCount.value == UInt64(candidateCount),
                  value.proof.canonicalRelations.value == UInt64(value.relations.count),
                  value.proof.applicableRelations.value == UInt64(counts.applicable),
                  value.proof.notApplicableRelations.value == UInt64(counts.notApplicable),
                  value.proof.applicableEdges.value == UInt64(counts.edges),
                  value.proof.logicalAssessments.value == UInt64(assessments)
            else {
                throw corrupted("proof counts do not match transported candidates/relations")
            }

            var hasFeasibleCandidate = false
            for candidate in 0..<candidateCount {
                var rowFailed = false
                for edge in 0..<counts.edges where
                    lsb0(value.failureMatrix, candidate * counts.edges + edge)
                {
                    rowFailed = true
                }
                let partitionSaysFeasible = lsb0(value.proof.partition, candidate)
                guard partitionSaysFeasible == !rowFailed else {
                    throw corrupted("partition disagrees with candidate-major LSB0 matrix")
                }
                hasFeasibleCandidate = hasFeasibleCandidate || partitionSaysFeasible
            }
            for bit in assessments..<(value.failureMatrix.count * 8)
            where lsb0(value.failureMatrix, bit) {
                throw corrupted("failure matrix padding bits must stay zero")
            }
            for bit in candidateCount..<(value.proof.partition.count * 8)
            where lsb0(value.proof.partition, bit) {
                throw corrupted("partition padding bits must stay zero")
            }
            guard hasFeasibleCandidate == expectsFeasible else {
                throw corrupted("terminal disagrees with its complete partition")
            }
        }

        switch self {
        case let .selected(feasibility, selection):
            try validateEvaluated(feasibility, expectsFeasible: true)
            guard selection.evaluationId == feasibility.proof.evaluationId else {
                throw corrupted("selection is not bound to the sealed evaluation")
            }
            guard let index = feasibility.candidates.firstIndex(where: {
                utf8($0.candidateId) == utf8(selection.candidateId)
            }) else {
                throw corrupted("selected candidate is not a member of the sealed domain")
            }
            guard feasibility.candidates[index].emitted == selection.emitted else {
                throw corrupted("selected bytes differ from the sealed domain member")
            }
            guard lsb0(feasibility.proof.partition, index) else {
                throw corrupted("selected candidate row is not feasible in the partition")
            }
            let verification = selection.finalVerification
            guard verification.verifiedApplicableEdges == feasibility.proof.applicableEdges
            else {
                throw corrupted("final verification did not recheck every applicable edge")
            }
            guard verification.relationSetDigest == feasibility.proof.relationSetDigest,
                  verification.proofSha256 == feasibility.proof.proofSha256
            else {
                throw corrupted("final verification is not bound to the sealed proof")
            }
            guard selection.receiptDigest == verification.receiptDigest else {
                throw corrupted("selection and final verification receipts diverge")
            }
        case let .noSelection(feasibility, selection):
            try validateEvaluated(feasibility, expectsFeasible: true)
            guard selection.evaluationId == feasibility.proof.evaluationId else {
                throw corrupted("no-selection refusal is not bound to the sealed evaluation")
            }
        case let .infeasible(feasibility, _):
            try validateEvaluated(feasibility, expectsFeasible: false)
        case let .notEvaluated(feasibility, _):
            try validateCanonicalCandidates(feasibility.candidates)
            try validateCanonicalRelations(feasibility.relations)
            guard feasibility.candidateCount.value == UInt64(feasibility.candidates.count)
            else {
                throw corrupted("candidate count does not match transported candidates")
            }
            guard !feasibility.relations.isEmpty else {
                throw corrupted("NotEvaluated must retain at least one declaration")
            }
            let counts = try relationCounts(feasibility.relations)
            guard counts.applicable == 0,
                  counts.notApplicable == feasibility.relations.count
            else {
                throw corrupted("NotEvaluated cannot carry an applicable relation")
            }
        }
    }
}

/// Transport failure of the raw envelope or the strict schema: the shared
/// feasibility algebra plus the policy-kind gate owned by this operation.
public enum Wcag22ExplicitSelectionTransportErrorV1: Codable, Equatable, Sendable {
    case common(Wcag22FeasibilityTransportErrorV1)
    case unsupportedPolicyKind(String)

    private enum CodingKeys: String, CodingKey { case code, received }
    private enum Code: String, Codable { case unsupportedPolicyKind }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        if try container.decode(String.self, forKey: .code)
            == Code.unsupportedPolicyKind.rawValue
        {
            self = try .unsupportedPolicyKind(container.decode(String.self, forKey: .received))
        } else {
            self = try .common(Wcag22FeasibilityTransportErrorV1(from: decoder))
        }
    }

    public func encode(to encoder: Encoder) throws {
        switch self {
        case let .common(value):
            try value.encode(to: encoder)
        case let .unsupportedPolicyKind(received):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(Code.unsupportedPolicyKind, forKey: .code)
            try container.encode(received, forKey: .received)
        }
    }
}

/// Exact A-phase failure. The shared feasibility Core algebra is reused as
/// data; only the explicit-domain `invalidRequest` codes are new, and they
/// are representable exactly once (the shared invalid-request enum cannot
/// carry them).
public enum Wcag22ExplicitSelectionFeasibilityErrorV1: Codable, Equatable, Sendable {
    case emptyCandidateId
    case emptyCandidates
    case duplicateCandidateId(candidateId: String)
    case common(Wcag22FeasibilityCoreErrorV1)

    private enum CodingKeys: String, CodingKey { case code, details }
    private enum DetailKeys: String, CodingKey { case code, candidateId }
    private enum Code: String, Codable { case invalidRequest }
    private enum DetailCode: String, Codable {
        case emptyCandidateId, emptyCandidates, duplicateCandidateId
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        if try container.decode(String.self, forKey: .code) == Code.invalidRequest.rawValue {
            let details = try container.nestedContainer(keyedBy: DetailKeys.self, forKey: .details)
            switch DetailCode(rawValue: try details.decode(String.self, forKey: .code)) {
            case .emptyCandidateId:
                self = .emptyCandidateId
                return
            case .emptyCandidates:
                self = .emptyCandidates
                return
            case .duplicateCandidateId:
                self = try .duplicateCandidateId(
                    candidateId: details.decode(String.self, forKey: .candidateId))
                return
            case nil:
                break
            }
        }
        self = try .common(Wcag22FeasibilityCoreErrorV1(from: decoder))
    }

    public func encode(to encoder: Encoder) throws {
        switch self {
        case let .common(value):
            try value.encode(to: encoder)
        case .emptyCandidateId:
            try encodeInvalid(to: encoder, code: .emptyCandidateId, candidateId: nil)
        case .emptyCandidates:
            try encodeInvalid(to: encoder, code: .emptyCandidates, candidateId: nil)
        case let .duplicateCandidateId(candidateId):
            try encodeInvalid(to: encoder, code: .duplicateCandidateId, candidateId: candidateId)
        }
    }

    private func encodeInvalid(
        to encoder: Encoder,
        code detailCode: DetailCode,
        candidateId: String?
    ) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(Code.invalidRequest, forKey: .code)
        var details = container.nestedContainer(keyedBy: DetailKeys.self, forKey: .details)
        try details.encode(detailCode, forKey: .code)
        if let candidateId {
            try details.encode(candidateId, forKey: .candidateId)
        }
    }
}

/// Invalid or contradictory selection input projected exactly from Core.
public enum Wcag22ExplicitSelectionInvalidRequestV1: Codable, Equatable, Sendable {
    case emptyPolicyId
    case emptyCandidateOrder
    case emptyCandidateId
    case arithmeticOverflow
    case policyCardinalityExceedsDomain(requested: DecimalU64V1, domain: DecimalU64V1)
    case foreignCandidateId(candidateId: String)
    case duplicateCandidateId(candidateId: String)

    private enum CodingKeys: String, CodingKey { case code, requested, domain, candidateId }
    private enum Code: String, Codable {
        case emptyPolicyId, emptyCandidateOrder, emptyCandidateId, arithmeticOverflow
        case policyCardinalityExceedsDomain, foreignCandidateId, duplicateCandidateId
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Code.self, forKey: .code) {
        case .emptyPolicyId: self = .emptyPolicyId
        case .emptyCandidateOrder: self = .emptyCandidateOrder
        case .emptyCandidateId: self = .emptyCandidateId
        case .arithmeticOverflow: self = .arithmeticOverflow
        case .policyCardinalityExceedsDomain:
            self = try .policyCardinalityExceedsDomain(
                requested: container.decode(DecimalU64V1.self, forKey: .requested),
                domain: container.decode(DecimalU64V1.self, forKey: .domain))
        case .foreignCandidateId:
            self = try .foreignCandidateId(
                candidateId: container.decode(String.self, forKey: .candidateId))
        case .duplicateCandidateId:
            self = try .duplicateCandidateId(
                candidateId: container.decode(String.self, forKey: .candidateId))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .emptyPolicyId: try container.encode(Code.emptyPolicyId, forKey: .code)
        case .emptyCandidateOrder: try container.encode(Code.emptyCandidateOrder, forKey: .code)
        case .emptyCandidateId: try container.encode(Code.emptyCandidateId, forKey: .code)
        case .arithmeticOverflow: try container.encode(Code.arithmeticOverflow, forKey: .code)
        case let .policyCardinalityExceedsDomain(requested, domain):
            try container.encode(Code.policyCardinalityExceedsDomain, forKey: .code)
            try container.encode(requested, forKey: .requested)
            try container.encode(domain, forKey: .domain)
        case let .foreignCandidateId(candidateId):
            try container.encode(Code.foreignCandidateId, forKey: .code)
            try container.encode(candidateId, forKey: .candidateId)
        case let .duplicateCandidateId(candidateId):
            try container.encode(Code.duplicateCandidateId, forKey: .code)
            try container.encode(candidateId, forKey: .candidateId)
        }
    }
}

/// Wire projection of one atomic WCAG verdict.
public enum Wcag22ExplicitSelectionDecisionV1: String, Codable, Sendable {
    case pass
    case fail
}

/// The final recheck of the selected row diverged from the sealed proof.
public enum Wcag22ExplicitSelectionIntegrityViolationV1: Codable, Equatable, Sendable {
    case evaluatorContract(
        candidateId: String,
        relationId: String,
        adjacent: Srgb8BytesV1,
        violation: Wcag22FeasibilityEvaluatorInvariantV1
    )
    case sealedDecisionMismatch(
        candidateId: String,
        relationId: String,
        adjacent: Srgb8BytesV1,
        sealed: Wcag22ExplicitSelectionDecisionV1,
        rechecked: Wcag22ExplicitSelectionDecisionV1
    )
    case selectedRowNotPassing(candidateId: String, relationId: String, adjacent: Srgb8BytesV1)
    case applicableEdgeCountMismatch(expected: DecimalU64V1, observed: DecimalU64V1)
    case sealedTraversalArithmeticOverflow

    private enum CodingKeys: String, CodingKey {
        case code, candidateId, relationId, adjacent, violation, sealed, rechecked
        case expected, observed
    }
    private enum Code: String, Codable {
        case evaluatorContract, sealedDecisionMismatch, selectedRowNotPassing
        case applicableEdgeCountMismatch, sealedTraversalArithmeticOverflow
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Code.self, forKey: .code) {
        case .evaluatorContract:
            self = try .evaluatorContract(
                candidateId: container.decode(String.self, forKey: .candidateId),
                relationId: container.decode(String.self, forKey: .relationId),
                adjacent: container.decode(Srgb8BytesV1.self, forKey: .adjacent),
                violation: container.decode(
                    Wcag22FeasibilityEvaluatorInvariantV1.self, forKey: .violation))
        case .sealedDecisionMismatch:
            self = try .sealedDecisionMismatch(
                candidateId: container.decode(String.self, forKey: .candidateId),
                relationId: container.decode(String.self, forKey: .relationId),
                adjacent: container.decode(Srgb8BytesV1.self, forKey: .adjacent),
                sealed: container.decode(
                    Wcag22ExplicitSelectionDecisionV1.self, forKey: .sealed),
                rechecked: container.decode(
                    Wcag22ExplicitSelectionDecisionV1.self, forKey: .rechecked))
        case .selectedRowNotPassing:
            self = try .selectedRowNotPassing(
                candidateId: container.decode(String.self, forKey: .candidateId),
                relationId: container.decode(String.self, forKey: .relationId),
                adjacent: container.decode(Srgb8BytesV1.self, forKey: .adjacent))
        case .applicableEdgeCountMismatch:
            self = try .applicableEdgeCountMismatch(
                expected: container.decode(DecimalU64V1.self, forKey: .expected),
                observed: container.decode(DecimalU64V1.self, forKey: .observed))
        case .sealedTraversalArithmeticOverflow:
            self = .sealedTraversalArithmeticOverflow
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .evaluatorContract(candidateId, relationId, adjacent, violation):
            try container.encode(Code.evaluatorContract, forKey: .code)
            try container.encode(candidateId, forKey: .candidateId)
            try container.encode(relationId, forKey: .relationId)
            try container.encode(adjacent, forKey: .adjacent)
            try container.encode(violation, forKey: .violation)
        case let .sealedDecisionMismatch(candidateId, relationId, adjacent, sealed, rechecked):
            try container.encode(Code.sealedDecisionMismatch, forKey: .code)
            try container.encode(candidateId, forKey: .candidateId)
            try container.encode(relationId, forKey: .relationId)
            try container.encode(adjacent, forKey: .adjacent)
            try container.encode(sealed, forKey: .sealed)
            try container.encode(rechecked, forKey: .rechecked)
        case let .selectedRowNotPassing(candidateId, relationId, adjacent):
            try container.encode(Code.selectedRowNotPassing, forKey: .code)
            try container.encode(candidateId, forKey: .candidateId)
            try container.encode(relationId, forKey: .relationId)
            try container.encode(adjacent, forKey: .adjacent)
        case let .applicableEdgeCountMismatch(expected, observed):
            try container.encode(Code.applicableEdgeCountMismatch, forKey: .code)
            try container.encode(expected, forKey: .expected)
            try container.encode(observed, forKey: .observed)
        case .sealedTraversalArithmeticOverflow:
            try container.encode(Code.sealedTraversalArithmeticOverflow, forKey: .code)
        }
    }
}

/// Complete failure algebra of the selection phase projected as data.
public enum Wcag22ExplicitSelectionErrorV1: Codable, Equatable, Sendable {
    case invalidRequest(Wcag22ExplicitSelectionInvalidRequestV1)
    case resourceLimitExceeded(
        profileId: Wcag22FeasibilityResourceProfileIdV1,
        dimension: Wcag22FeasibilityResourceDimensionV1,
        requested: DecimalU64V1,
        limit: DecimalU64V1
    )
    case integrityViolation(Wcag22ExplicitSelectionIntegrityViolationV1)

    private enum CodingKeys: String, CodingKey { case code, details }
    private enum DetailKeys: String, CodingKey { case profileId, dimension, requested, limit }
    private enum Code: String, Codable {
        case invalidRequest, resourceLimitExceeded, integrityViolation
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Code.self, forKey: .code) {
        case .invalidRequest:
            self = try .invalidRequest(
                container.decode(Wcag22ExplicitSelectionInvalidRequestV1.self, forKey: .details))
        case .resourceLimitExceeded:
            let details = try container.nestedContainer(keyedBy: DetailKeys.self, forKey: .details)
            self = try .resourceLimitExceeded(
                profileId: details.decode(
                    Wcag22FeasibilityResourceProfileIdV1.self, forKey: .profileId),
                dimension: details.decode(
                    Wcag22FeasibilityResourceDimensionV1.self, forKey: .dimension),
                requested: details.decode(DecimalU64V1.self, forKey: .requested),
                limit: details.decode(DecimalU64V1.self, forKey: .limit))
        case .integrityViolation:
            self = try .integrityViolation(
                container.decode(
                    Wcag22ExplicitSelectionIntegrityViolationV1.self, forKey: .details))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .invalidRequest(value):
            try container.encode(Code.invalidRequest, forKey: .code)
            try container.encode(value, forKey: .details)
        case let .resourceLimitExceeded(profileId, dimension, requested, limit):
            try container.encode(Code.resourceLimitExceeded, forKey: .code)
            var details = container.nestedContainer(keyedBy: DetailKeys.self, forKey: .details)
            try details.encode(profileId, forKey: .profileId)
            try details.encode(dimension, forKey: .dimension)
            try details.encode(requested, forKey: .requested)
            try details.encode(limit, forKey: .limit)
        case let .integrityViolation(value):
            try container.encode(Code.integrityViolation, forKey: .code)
            try container.encode(value, forKey: .details)
        }
    }
}

/// Failure source at the public boundary of the atomic operation.
public enum Wcag22ExplicitSelectionOperationErrorV1: Codable, Equatable, Sendable {
    case transport(Wcag22ExplicitSelectionTransportErrorV1)
    case feasibility(Wcag22ExplicitSelectionFeasibilityErrorV1)
    case selection(Wcag22ExplicitSelectionErrorV1)
    case incompatibleCoreContract

    private enum CodingKeys: String, CodingKey { case source, error }
    private enum Source: String, Codable {
        case transport, feasibility, selection, incompatibleCoreContract
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Source.self, forKey: .source) {
        case .transport:
            self = try .transport(
                container.decode(Wcag22ExplicitSelectionTransportErrorV1.self, forKey: .error))
        case .feasibility:
            self = try .feasibility(
                container.decode(Wcag22ExplicitSelectionFeasibilityErrorV1.self, forKey: .error))
        case .selection:
            self = try .selection(
                container.decode(Wcag22ExplicitSelectionErrorV1.self, forKey: .error))
        case .incompatibleCoreContract:
            self = .incompatibleCoreContract
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .transport(value):
            try container.encode(Source.transport, forKey: .source)
            try container.encode(value, forKey: .error)
        case let .feasibility(value):
            try container.encode(Source.feasibility, forKey: .source)
            try container.encode(value, forKey: .error)
        case let .selection(value):
            try container.encode(Source.selection, forKey: .source)
            try container.encode(value, forKey: .error)
        case .incompatibleCoreContract:
            try container.encode(Source.incompatibleCoreContract, forKey: .source)
        }
    }
}

/// Total Swift result of the atomic operation. Semantic input, feasibility and
/// selection failures remain typed data; throwing is reserved for
/// UniFFI/internal protocol decoding failure. Decoding a success terminal
/// re-checks the structural laws above before the value escapes.
public enum Wcag22ExplicitSelectionOutcomeV1: Codable, Equatable, Sendable {
    case success(Wcag22ExplicitSelectionResultV1)
    case failure(Wcag22ExplicitSelectionOperationErrorV1)

    private enum CodingKeys: String, CodingKey { case schemaVersion, outcome, result, error }
    private enum Outcome: String, Codable { case success, failure }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let schemaVersion = try container.decode(UInt32.self, forKey: .schemaVersion)
        guard schemaVersion == 1 else {
            throw DecodingError.dataCorruptedError(
                forKey: .schemaVersion,
                in: container,
                debugDescription: "unsupported explicit-selection outcome schema version")
        }
        switch try container.decode(Outcome.self, forKey: .outcome) {
        case .success:
            let result = try container.decode(
                Wcag22ExplicitSelectionResultV1.self, forKey: .result)
            try result.validate(codingPath: decoder.codingPath + [CodingKeys.result])
            self = .success(result)
        case .failure:
            self = try .failure(
                container.decode(
                    Wcag22ExplicitSelectionOperationErrorV1.self, forKey: .error))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(UInt32(1), forKey: .schemaVersion)
        switch self {
        case let .success(value):
            try container.encode(Outcome.success, forKey: .outcome)
            try container.encode(value, forKey: .result)
        case let .failure(value):
            try container.encode(Outcome.failure, forKey: .outcome)
            try container.encode(value, forKey: .error)
        }
    }
}

/// Injectable only under `@testable import`; production uses the generated
/// UniFFI byte functions. It makes the pre-copy admission law executable.
struct Wcag22ExplicitSelectionBridge {
    let maxRequestBytes: () -> UInt64
    let evaluateRaw: (Data) throws -> Data
    let envelopeTooLarge: (UInt64) throws -> Data

    static let live = Self(
        maxRequestBytes: wcag22ExplicitSelectionMaxRequestBytesV1,
        evaluateRaw: { try evaluateWcag22ExplicitSelectionRawV1(request: $0) },
        envelopeTooLarge: {
            try wcag22ExplicitSelectionEnvelopeTooLargeV1(requestedBytes: $0)
        })
}

/// Exact protocol-owned request byte ceiling.
public func wcag22ExplicitSelectionMaxBytes() -> UInt64 {
    Wcag22ExplicitSelectionBridge.live.maxRequestBytes()
}

/// Evaluate one exact atomic V1 request carried as `Data`.
public func evaluateWcag22ExplicitSelection(
    _ request: Data
) throws -> Wcag22ExplicitSelectionOutcomeV1 {
    try evaluateWcag22ExplicitSelection(request, using: .live)
}

/// Evaluate one exact atomic V1 request carried as bytes. The size comparison
/// occurs before `Data` materialization and therefore before the raw UniFFI
/// copy.
public func evaluateWcag22ExplicitSelection(
    _ request: [UInt8]
) throws -> Wcag22ExplicitSelectionOutcomeV1 {
    let bridge = Wcag22ExplicitSelectionBridge.live
    return try evaluateWcag22ExplicitSelection(
        requestedBytes: UInt64(request.count),
        using: bridge,
        evaluateAdmitted: { try bridge.evaluateRaw(Data(request)) })
}

func evaluateWcag22ExplicitSelection(
    _ request: Data,
    using bridge: Wcag22ExplicitSelectionBridge
) throws -> Wcag22ExplicitSelectionOutcomeV1 {
    try evaluateWcag22ExplicitSelection(
        requestedBytes: UInt64(request.count),
        using: bridge,
        evaluateAdmitted: { try bridge.evaluateRaw(request) })
}

private func evaluateWcag22ExplicitSelection(
    requestedBytes: UInt64,
    using bridge: Wcag22ExplicitSelectionBridge,
    evaluateAdmitted: () throws -> Data
) throws -> Wcag22ExplicitSelectionOutcomeV1 {
    let encoded: Data
    if requestedBytes > bridge.maxRequestBytes() {
        encoded = try bridge.envelopeTooLarge(requestedBytes)
    } else {
        encoded = try evaluateAdmitted()
    }
    return try JSONDecoder().decode(Wcag22ExplicitSelectionOutcomeV1.self, from: encoded)
}
