import Foundation

/// Exact non-negative `u64` carried by the V1 JSON protocol as canonical
/// decimal text, so JavaScript and native consumers share one integer law.
public struct DecimalU64V1: Codable, Equatable, Hashable, Sendable {
    public let value: UInt64

    public init(_ value: UInt64) {
        self.value = value
    }

    public init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        guard let value = UInt64(raw), String(value) == raw else {
            throw DecodingError.dataCorrupted(
                .init(
                    codingPath: decoder.codingPath,
                    debugDescription: "expected canonical decimal UInt64 text"))
        }
        self.value = value
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(String(value))
    }
}

/// One exact final encoded-sRGB8 colour.
public struct Srgb8BytesV1: Codable, Equatable, Hashable, Sendable {
    public let red: UInt8
    public let green: UInt8
    public let blue: UInt8

    public init(red: UInt8, green: UInt8, blue: UInt8) {
        self.red = red
        self.green = green
        self.blue = blue
    }

    public var bytes: [UInt8] { [red, green, blue] }

    public init(from decoder: Decoder) throws {
        var container = try decoder.unkeyedContainer()
        red = try container.decode(UInt8.self)
        green = try container.decode(UInt8.self)
        blue = try container.decode(UInt8.self)
        guard container.isAtEnd else {
            throw DecodingError.dataCorruptedError(
                in: container, debugDescription: "sRGB8 value must contain exactly three bytes")
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.unkeyedContainer()
        try container.encode(red)
        try container.encode(green)
        try container.encode(blue)
    }
}

/// One exact SHA-256 digest or 256-bit LSB0 partition.
public struct Bytes32V1: Codable, Equatable, Hashable, Sendable {
    public let bytes: [UInt8]

    public init?(exactBytes: [UInt8]) {
        guard exactBytes.count == 32 else { return nil }
        bytes = exactBytes
    }

    public init(from decoder: Decoder) throws {
        let bytes = try decoder.singleValueContainer().decode([UInt8].self)
        guard bytes.count == 32 else {
            throw DecodingError.dataCorrupted(
                .init(
                    codingPath: decoder.codingPath,
                    debugDescription: "expected exactly 32 bytes"))
        }
        self.bytes = bytes
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(bytes)
    }
}

public enum Wcag22FeasibilityDomainIdV1: String, Codable, Sendable {
    case srgb8NeutralAxisV1 = "srgb8-neutral-axis-v1"
}

public enum Wcag22FeasibilityResourceProfileIdV1: String, Codable, Sendable {
    case compileV1 = "compile-v1"
}

public enum Wcag22FeasibilityCriterionV1: String, Codable, Sendable {
    case sc143TextDefault = "sc-1.4.3-text-default"
    case sc143TextLargeScale = "sc-1.4.3-text-large-scale"
    case sc1411UiComponentOrState = "sc-1.4.11-ui-component-or-state"
    case sc1411GraphicalObject = "sc-1.4.11-graphical-object"
}

public enum Wcag22FeasibilityRelationV1: Codable, Equatable, Sendable {
    case applicable(
        relationId: String,
        occurrenceId: String,
        criterion: Wcag22FeasibilityCriterionV1,
        adjacent: [Srgb8BytesV1]
    )
    case notApplicable(relationId: String, occurrenceId: String, reasonId: String)

    private enum CodingKeys: String, CodingKey {
        case relationId, occurrenceId, kind, criterion, adjacent, reasonId
    }

    private enum Kind: String, Codable {
        case applicable
        case notApplicable
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Kind.self, forKey: .kind) {
        case .applicable:
            self = try .applicable(
                relationId: container.decode(String.self, forKey: .relationId),
                occurrenceId: container.decode(String.self, forKey: .occurrenceId),
                criterion: container.decode(
                    Wcag22FeasibilityCriterionV1.self, forKey: .criterion),
                adjacent: container.decode([Srgb8BytesV1].self, forKey: .adjacent))
        case .notApplicable:
            self = try .notApplicable(
                relationId: container.decode(String.self, forKey: .relationId),
                occurrenceId: container.decode(String.self, forKey: .occurrenceId),
                reasonId: container.decode(String.self, forKey: .reasonId))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .applicable(relationId, occurrenceId, criterion, adjacent):
            try container.encode(Kind.applicable, forKey: .kind)
            try container.encode(relationId, forKey: .relationId)
            try container.encode(occurrenceId, forKey: .occurrenceId)
            try container.encode(criterion, forKey: .criterion)
            try container.encode(adjacent, forKey: .adjacent)
        case let .notApplicable(relationId, occurrenceId, reasonId):
            try container.encode(Kind.notApplicable, forKey: .kind)
            try container.encode(relationId, forKey: .relationId)
            try container.encode(occurrenceId, forKey: .occurrenceId)
            try container.encode(reasonId, forKey: .reasonId)
        }
    }
}

public enum Wcag22FeasibilityProfileIdV1: String, Codable, Sendable {
    case wcag22Srgb8ContrastV1 = "wcag22-srgb8-contrast-v1"
}

public enum Wcag22FeasibilityArtifactIdV1: String, Codable, Sendable {
    case wcag22Srgb8LuminanceQ55V1 = "wcag22-srgb8-luminance-q55-v1"
}

public enum Wcag22FeasibilityBoundIdV1: String, Codable, Sendable {
    case wcag22Srgb8OutwardQ55V1 = "wcag22-srgb8-outward-q55-v1"
}

public enum Wcag22FeasibilityProofIdV1: String, Codable, Sendable {
    case wcag22Srgb8FullDomainQ55V1 = "wcag22-srgb8-full-domain-q55-v1"
}

public struct Wcag22FeasibilityProofV1: Codable, Equatable, Sendable {
    public let evaluationId: Bytes32V1
    public let resourceProfileId: Wcag22FeasibilityResourceProfileIdV1
    public let domainId: Wcag22FeasibilityDomainIdV1
    public let domainDigest: Bytes32V1
    public let domainCount: DecimalU64V1
    public let domainFirst: Srgb8BytesV1
    public let domainLast: Srgb8BytesV1
    public let relationSetDigest: Bytes32V1
    public let canonicalRelations: DecimalU64V1
    public let applicableRelations: DecimalU64V1
    public let notApplicableRelations: DecimalU64V1
    public let applicableEdges: DecimalU64V1
    public let logicalAssessments: DecimalU64V1
    public let matrixDigest: Bytes32V1
    public let partition: Bytes32V1
    public let wcag22ProfileId: Wcag22FeasibilityProfileIdV1
    public let artifactId: Wcag22FeasibilityArtifactIdV1
    public let boundId: Wcag22FeasibilityBoundIdV1
    public let proofId: Wcag22FeasibilityProofIdV1
    public let proofSha256: Bytes32V1
}

public struct Wcag22FeasibilityEvaluatedV1: Codable, Equatable, Sendable {
    public let domain: [Srgb8BytesV1]
    public let relations: [Wcag22FeasibilityRelationV1]
    public let failureMatrix: [UInt8]
    public let proof: Wcag22FeasibilityProofV1
}

public struct Wcag22FeasibilityNotEvaluatedV1: Codable, Equatable, Sendable {
    public let domainId: Wcag22FeasibilityDomainIdV1
    public let domainDigest: Bytes32V1
    public let relationSetDigest: Bytes32V1
    public let resourceProfileId: Wcag22FeasibilityResourceProfileIdV1
    public let relations: [Wcag22FeasibilityRelationV1]
}

public enum Wcag22FeasibilityV1: Codable, Equatable, Sendable {
    case feasible(Wcag22FeasibilityEvaluatedV1)
    case infeasible(Wcag22FeasibilityEvaluatedV1)
    case notEvaluated(Wcag22FeasibilityNotEvaluatedV1)

    private enum CodingKeys: String, CodingKey { case status, result }
    private enum Status: String, Codable { case feasible, infeasible, notEvaluated }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Status.self, forKey: .status) {
        case .feasible:
            self = try .feasible(
                container.decode(Wcag22FeasibilityEvaluatedV1.self, forKey: .result))
        case .infeasible:
            self = try .infeasible(
                container.decode(Wcag22FeasibilityEvaluatedV1.self, forKey: .result))
        case .notEvaluated:
            self = try .notEvaluated(
                container.decode(Wcag22FeasibilityNotEvaluatedV1.self, forKey: .result))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .feasible(value):
            try container.encode(Status.feasible, forKey: .status)
            try container.encode(value, forKey: .result)
        case let .infeasible(value):
            try container.encode(Status.infeasible, forKey: .status)
            try container.encode(value, forKey: .result)
        case let .notEvaluated(value):
            try container.encode(Status.notEvaluated, forKey: .status)
            try container.encode(value, forKey: .result)
        }
    }

    fileprivate func validate(codingPath: [CodingKey]) throws {
        func corrupted(_ message: String) -> DecodingError {
            .dataCorrupted(.init(codingPath: codingPath, debugDescription: message))
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
        func validateEvaluated(
            _ value: Wcag22FeasibilityEvaluatedV1,
            expectsFeasible: Bool
        ) throws {
            guard value.domain.count == 256 else {
                throw corrupted("evaluated domain must contain exactly 256 candidates")
            }
            for (index, candidate) in value.domain.enumerated() {
                guard let channel = UInt8(exactly: index),
                      candidate == Srgb8BytesV1(red: channel, green: channel, blue: channel)
                else {
                    throw corrupted("evaluated domain differs from registered candidate order")
                }
            }
            let counts = try relationCounts(value.relations)
            guard counts.applicable > 0, counts.edges > 0 else {
                throw corrupted("evaluated terminal must contain an applicable edge")
            }
            let (matrixBytes, matrixOverflow) = counts.edges.multipliedReportingOverflow(by: 32)
            let (logicalAssessments, logicalOverflow) =
                counts.edges.multipliedReportingOverflow(by: 256)
            guard !matrixOverflow, value.failureMatrix.count == matrixBytes else {
                throw corrupted("failure matrix length does not equal 32E")
            }
            guard !logicalOverflow else {
                throw corrupted("logical assessment count overflow")
            }
            guard value.proof.domainCount.value == UInt64(value.domain.count),
                  value.proof.domainFirst == value.domain.first,
                  value.proof.domainLast == value.domain.last,
                  value.proof.canonicalRelations.value == UInt64(value.relations.count),
                  value.proof.applicableRelations.value == UInt64(counts.applicable),
                  value.proof.notApplicableRelations.value == UInt64(counts.notApplicable),
                  value.proof.applicableEdges.value == UInt64(counts.edges),
                  value.proof.logicalAssessments.value == UInt64(logicalAssessments)
            else {
                throw corrupted("proof counts do not match transported domain/relations")
            }

            func lsb0(_ bytes: [UInt8], _ index: Int) -> Bool {
                (bytes[index / 8] & (UInt8(1) << UInt8(index % 8))) != 0
            }
            var hasFeasibleCandidate = false
            for candidate in 0..<256 {
                var rowFailed = false
                for edge in 0..<counts.edges where
                    lsb0(value.failureMatrix, candidate * counts.edges + edge)
                {
                    rowFailed = true
                }
                let partitionSaysFeasible = lsb0(value.proof.partition.bytes, candidate)
                guard partitionSaysFeasible == !rowFailed else {
                    throw corrupted("partition disagrees with candidate-major LSB0 matrix")
                }
                hasFeasibleCandidate = hasFeasibleCandidate || partitionSaysFeasible
            }
            guard hasFeasibleCandidate == expectsFeasible else {
                throw corrupted("feasibility terminal disagrees with its complete partition")
            }
        }

        switch self {
        case let .feasible(value):
            try validateEvaluated(value, expectsFeasible: true)
        case let .infeasible(value):
            try validateEvaluated(value, expectsFeasible: false)
        case let .notEvaluated(value):
            guard !value.relations.isEmpty else {
                throw corrupted("NotEvaluated must retain at least one declaration")
            }
            let counts = try relationCounts(value.relations)
            guard counts.applicable == 0, counts.notApplicable == value.relations.count else {
                throw corrupted("NotEvaluated cannot carry an applicable relation")
            }
        }
    }
}

public enum Wcag22MalformedEnvelopeClassV1: String, Codable, Sendable {
    case syntax
    case shape
    case endOfInput
    case io
}

public enum Wcag22FeasibilityTransportErrorV1: Codable, Equatable, Sendable {
    case envelopeTooLarge(requestedBytes: DecimalU64V1, limitBytes: DecimalU64V1)
    case invalidUtf8
    case malformedEnvelope(Wcag22MalformedEnvelopeClassV1)
    case unsupportedSchemaVersion(UInt32)
    case unsupportedDomainId(String)
    case unsupportedResourceProfileId(String)
    case unsupportedCriterion(String)
    case emptyNotApplicableReason

    private enum CodingKeys: String, CodingKey {
        case code, requestedBytes, limitBytes, `class`, received
    }

    private enum Code: String, Codable {
        case envelopeTooLarge, invalidUtf8, malformedEnvelope, unsupportedSchemaVersion
        case unsupportedDomainId, unsupportedResourceProfileId, unsupportedCriterion
        case emptyNotApplicableReason
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Code.self, forKey: .code) {
        case .envelopeTooLarge:
            self = try .envelopeTooLarge(
                requestedBytes: container.decode(DecimalU64V1.self, forKey: .requestedBytes),
                limitBytes: container.decode(DecimalU64V1.self, forKey: .limitBytes))
        case .invalidUtf8:
            self = .invalidUtf8
        case .malformedEnvelope:
            self = try .malformedEnvelope(
                container.decode(Wcag22MalformedEnvelopeClassV1.self, forKey: .class))
        case .unsupportedSchemaVersion:
            self = try .unsupportedSchemaVersion(container.decode(UInt32.self, forKey: .received))
        case .unsupportedDomainId:
            self = try .unsupportedDomainId(container.decode(String.self, forKey: .received))
        case .unsupportedResourceProfileId:
            self = try .unsupportedResourceProfileId(
                container.decode(String.self, forKey: .received))
        case .unsupportedCriterion:
            self = try .unsupportedCriterion(container.decode(String.self, forKey: .received))
        case .emptyNotApplicableReason:
            self = .emptyNotApplicableReason
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .envelopeTooLarge(requestedBytes, limitBytes):
            try container.encode(Code.envelopeTooLarge, forKey: .code)
            try container.encode(requestedBytes, forKey: .requestedBytes)
            try container.encode(limitBytes, forKey: .limitBytes)
        case .invalidUtf8:
            try container.encode(Code.invalidUtf8, forKey: .code)
        case let .malformedEnvelope(value):
            try container.encode(Code.malformedEnvelope, forKey: .code)
            try container.encode(value, forKey: .class)
        case let .unsupportedSchemaVersion(value):
            try container.encode(Code.unsupportedSchemaVersion, forKey: .code)
            try container.encode(value, forKey: .received)
        case let .unsupportedDomainId(value):
            try container.encode(Code.unsupportedDomainId, forKey: .code)
            try container.encode(value, forKey: .received)
        case let .unsupportedResourceProfileId(value):
            try container.encode(Code.unsupportedResourceProfileId, forKey: .code)
            try container.encode(value, forKey: .received)
        case let .unsupportedCriterion(value):
            try container.encode(Code.unsupportedCriterion, forKey: .code)
            try container.encode(value, forKey: .received)
        case .emptyNotApplicableReason:
            try container.encode(Code.emptyNotApplicableReason, forKey: .code)
        }
    }
}

public enum Wcag22FeasibilityInvalidRequestV1: Codable, Equatable, Sendable {
    case emptyRelationId
    case emptyOccurrenceId
    case emptyRelations
    case emptyAdjacentSet(relationId: String)
    case conflictingRelationId(relationId: String)
    case arithmeticOverflow

    private enum CodingKeys: String, CodingKey { case code, relationId }
    private enum Code: String, Codable {
        case emptyRelationId, emptyOccurrenceId, emptyRelations, emptyAdjacentSet
        case conflictingRelationId, arithmeticOverflow
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Code.self, forKey: .code) {
        case .emptyRelationId: self = .emptyRelationId
        case .emptyOccurrenceId: self = .emptyOccurrenceId
        case .emptyRelations: self = .emptyRelations
        case .emptyAdjacentSet:
            self = try .emptyAdjacentSet(
                relationId: container.decode(String.self, forKey: .relationId))
        case .conflictingRelationId:
            self = try .conflictingRelationId(
                relationId: container.decode(String.self, forKey: .relationId))
        case .arithmeticOverflow: self = .arithmeticOverflow
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .emptyRelationId: try container.encode(Code.emptyRelationId, forKey: .code)
        case .emptyOccurrenceId: try container.encode(Code.emptyOccurrenceId, forKey: .code)
        case .emptyRelations: try container.encode(Code.emptyRelations, forKey: .code)
        case let .emptyAdjacentSet(relationId):
            try container.encode(Code.emptyAdjacentSet, forKey: .code)
            try container.encode(relationId, forKey: .relationId)
        case let .conflictingRelationId(relationId):
            try container.encode(Code.conflictingRelationId, forKey: .code)
            try container.encode(relationId, forKey: .relationId)
        case .arithmeticOverflow: try container.encode(Code.arithmeticOverflow, forKey: .code)
        }
    }
}

public enum Wcag22FeasibilityAtomicErrorV1: Codable, Equatable, Sendable {
    case invalidSrgb8(field: String, reason: String)
    case emptyNotApplicableReason
    case artifactInvariantViolation(
        criterion: Wcag22FeasibilityCriterionV1,
        foreground: Srgb8BytesV1,
        background: Srgb8BytesV1
    )
    case evidenceRegistryMismatch(message: String)

    private enum CodingKeys: String, CodingKey {
        case code, field, reason, criterion, foreground, background, message
    }
    private enum Code: String, Codable {
        case invalidSrgb8, emptyNotApplicableReason, artifactInvariantViolation
        case evidenceRegistryMismatch
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Code.self, forKey: .code) {
        case .invalidSrgb8:
            self = try .invalidSrgb8(
                field: container.decode(String.self, forKey: .field),
                reason: container.decode(String.self, forKey: .reason))
        case .emptyNotApplicableReason:
            self = .emptyNotApplicableReason
        case .artifactInvariantViolation:
            self = try .artifactInvariantViolation(
                criterion: container.decode(
                    Wcag22FeasibilityCriterionV1.self, forKey: .criterion),
                foreground: container.decode(Srgb8BytesV1.self, forKey: .foreground),
                background: container.decode(Srgb8BytesV1.self, forKey: .background))
        case .evidenceRegistryMismatch:
            self = try .evidenceRegistryMismatch(
                message: container.decode(String.self, forKey: .message))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .invalidSrgb8(field, reason):
            try container.encode(Code.invalidSrgb8, forKey: .code)
            try container.encode(field, forKey: .field)
            try container.encode(reason, forKey: .reason)
        case .emptyNotApplicableReason:
            try container.encode(Code.emptyNotApplicableReason, forKey: .code)
        case let .artifactInvariantViolation(criterion, foreground, background):
            try container.encode(Code.artifactInvariantViolation, forKey: .code)
            try container.encode(criterion, forKey: .criterion)
            try container.encode(foreground, forKey: .foreground)
            try container.encode(background, forKey: .background)
        case let .evidenceRegistryMismatch(message):
            try container.encode(Code.evidenceRegistryMismatch, forKey: .code)
            try container.encode(message, forKey: .message)
        }
    }
}

public enum Wcag22FeasibilityEvaluatorInvariantV1: Codable, Equatable, Sendable {
    case source(Wcag22FeasibilityAtomicErrorV1)
    case unexpectedNotEvaluated
    case inputMismatch
    case criterionMismatch
    case evidenceMismatch

    private enum CodingKeys: String, CodingKey { case code, details }
    private enum Code: String, Codable {
        case source, unexpectedNotEvaluated, inputMismatch, criterionMismatch, evidenceMismatch
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Code.self, forKey: .code) {
        case .source:
            self = try .source(
                container.decode(Wcag22FeasibilityAtomicErrorV1.self, forKey: .details))
        case .unexpectedNotEvaluated: self = .unexpectedNotEvaluated
        case .inputMismatch: self = .inputMismatch
        case .criterionMismatch: self = .criterionMismatch
        case .evidenceMismatch: self = .evidenceMismatch
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .source(value):
            try container.encode(Code.source, forKey: .code)
            try container.encode(value, forKey: .details)
        case .unexpectedNotEvaluated:
            try container.encode(Code.unexpectedNotEvaluated, forKey: .code)
        case .inputMismatch: try container.encode(Code.inputMismatch, forKey: .code)
        case .criterionMismatch: try container.encode(Code.criterionMismatch, forKey: .code)
        case .evidenceMismatch: try container.encode(Code.evidenceMismatch, forKey: .code)
        }
    }
}

public enum Wcag22FeasibilityCompilerInvariantV1: Codable, Equatable, Sendable {
    case layoutMismatch
    case assessmentCardinalityMismatch(expected: DecimalU64V1, observed: DecimalU64V1)
    case candidateCardinalityMismatch(expected: DecimalU64V1, observed: DecimalU64V1)
    case decisionStorageRejectedCell
    case decisionStorageRejectedPartition
    case completeResultMismatch

    private enum CodingKeys: String, CodingKey { case code, expected, observed }
    private enum Code: String, Codable {
        case layoutMismatch, assessmentCardinalityMismatch, candidateCardinalityMismatch
        case decisionStorageRejectedCell, decisionStorageRejectedPartition, completeResultMismatch
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Code.self, forKey: .code) {
        case .layoutMismatch: self = .layoutMismatch
        case .assessmentCardinalityMismatch:
            self = try .assessmentCardinalityMismatch(
                expected: container.decode(DecimalU64V1.self, forKey: .expected),
                observed: container.decode(DecimalU64V1.self, forKey: .observed))
        case .candidateCardinalityMismatch:
            self = try .candidateCardinalityMismatch(
                expected: container.decode(DecimalU64V1.self, forKey: .expected),
                observed: container.decode(DecimalU64V1.self, forKey: .observed))
        case .decisionStorageRejectedCell: self = .decisionStorageRejectedCell
        case .decisionStorageRejectedPartition: self = .decisionStorageRejectedPartition
        case .completeResultMismatch: self = .completeResultMismatch
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .layoutMismatch:
            try container.encode(Code.layoutMismatch, forKey: .code)
        case let .assessmentCardinalityMismatch(expected, observed):
            try container.encode(Code.assessmentCardinalityMismatch, forKey: .code)
            try container.encode(expected, forKey: .expected)
            try container.encode(observed, forKey: .observed)
        case let .candidateCardinalityMismatch(expected, observed):
            try container.encode(Code.candidateCardinalityMismatch, forKey: .code)
            try container.encode(expected, forKey: .expected)
            try container.encode(observed, forKey: .observed)
        case .decisionStorageRejectedCell:
            try container.encode(Code.decisionStorageRejectedCell, forKey: .code)
        case .decisionStorageRejectedPartition:
            try container.encode(Code.decisionStorageRejectedPartition, forKey: .code)
        case .completeResultMismatch:
            try container.encode(Code.completeResultMismatch, forKey: .code)
        }
    }
}

public enum Wcag22FeasibilityResourceDimensionV1: String, Codable, Sendable {
    case rawRelations
    case rawAdjacentEntries
    case opaqueUtf8Bytes
    case canonicalRelations
    case applicableEdges
    case logicalAssessments
    case packedResultBytes
}

public enum Wcag22FeasibilityCoreErrorV1: Codable, Equatable, Sendable {
    case invalidRequest(Wcag22FeasibilityInvalidRequestV1)
    case resourceLimitExceeded(
        profileId: Wcag22FeasibilityResourceProfileIdV1,
        dimension: Wcag22FeasibilityResourceDimensionV1,
        requested: DecimalU64V1,
        limit: DecimalU64V1
    )
    case allocationFailed(
        profileId: Wcag22FeasibilityResourceProfileIdV1,
        requestedBytes: DecimalU64V1
    )
    case evaluatorInvariantViolation(
        candidate: Srgb8BytesV1,
        relationId: String,
        adjacent: Srgb8BytesV1,
        violation: Wcag22FeasibilityEvaluatorInvariantV1
    )
    case compilerInvariantViolation(Wcag22FeasibilityCompilerInvariantV1)

    private enum CodingKeys: String, CodingKey { case code, details }
    private enum DetailKeys: String, CodingKey {
        case profileId, dimension, requested, limit, requestedBytes
        case candidate, relationId, adjacent, violation
    }
    private enum Code: String, Codable {
        case invalidRequest, resourceLimitExceeded, allocationFailed
        case evaluatorInvariantViolation, compilerInvariantViolation
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Code.self, forKey: .code) {
        case .invalidRequest:
            self = try .invalidRequest(
                container.decode(Wcag22FeasibilityInvalidRequestV1.self, forKey: .details))
        case .resourceLimitExceeded:
            let details = try container.nestedContainer(keyedBy: DetailKeys.self, forKey: .details)
            self = try .resourceLimitExceeded(
                profileId: details.decode(
                    Wcag22FeasibilityResourceProfileIdV1.self, forKey: .profileId),
                dimension: details.decode(
                    Wcag22FeasibilityResourceDimensionV1.self, forKey: .dimension),
                requested: details.decode(DecimalU64V1.self, forKey: .requested),
                limit: details.decode(DecimalU64V1.self, forKey: .limit))
        case .allocationFailed:
            let details = try container.nestedContainer(keyedBy: DetailKeys.self, forKey: .details)
            self = try .allocationFailed(
                profileId: details.decode(
                    Wcag22FeasibilityResourceProfileIdV1.self, forKey: .profileId),
                requestedBytes: details.decode(DecimalU64V1.self, forKey: .requestedBytes))
        case .evaluatorInvariantViolation:
            let details = try container.nestedContainer(keyedBy: DetailKeys.self, forKey: .details)
            self = try .evaluatorInvariantViolation(
                candidate: details.decode(Srgb8BytesV1.self, forKey: .candidate),
                relationId: details.decode(String.self, forKey: .relationId),
                adjacent: details.decode(Srgb8BytesV1.self, forKey: .adjacent),
                violation: details.decode(
                    Wcag22FeasibilityEvaluatorInvariantV1.self, forKey: .violation))
        case .compilerInvariantViolation:
            self = try .compilerInvariantViolation(
                container.decode(Wcag22FeasibilityCompilerInvariantV1.self, forKey: .details))
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
        case let .allocationFailed(profileId, requestedBytes):
            try container.encode(Code.allocationFailed, forKey: .code)
            var details = container.nestedContainer(keyedBy: DetailKeys.self, forKey: .details)
            try details.encode(profileId, forKey: .profileId)
            try details.encode(requestedBytes, forKey: .requestedBytes)
        case let .evaluatorInvariantViolation(candidate, relationId, adjacent, violation):
            try container.encode(Code.evaluatorInvariantViolation, forKey: .code)
            var details = container.nestedContainer(keyedBy: DetailKeys.self, forKey: .details)
            try details.encode(candidate, forKey: .candidate)
            try details.encode(relationId, forKey: .relationId)
            try details.encode(adjacent, forKey: .adjacent)
            try details.encode(violation, forKey: .violation)
        case let .compilerInvariantViolation(value):
            try container.encode(Code.compilerInvariantViolation, forKey: .code)
            try container.encode(value, forKey: .details)
        }
    }
}

public enum Wcag22FeasibilityProtocolErrorV1: Codable, Equatable, Sendable {
    case transport(Wcag22FeasibilityTransportErrorV1)
    case core(Wcag22FeasibilityCoreErrorV1)
    case incompatibleCoreContract

    private enum CodingKeys: String, CodingKey { case source, error }
    private enum Source: String, Codable { case transport, core, incompatibleCoreContract }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Source.self, forKey: .source) {
        case .transport:
            self = try .transport(
                container.decode(Wcag22FeasibilityTransportErrorV1.self, forKey: .error))
        case .core:
            self = try .core(
                container.decode(Wcag22FeasibilityCoreErrorV1.self, forKey: .error))
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
        case let .core(value):
            try container.encode(Source.core, forKey: .source)
            try container.encode(value, forKey: .error)
        case .incompatibleCoreContract:
            try container.encode(Source.incompatibleCoreContract, forKey: .source)
        }
    }
}

/// Total Swift result. Semantic input and Core failures remain typed data;
/// throwing is reserved for UniFFI/internal protocol decoding failure.
public enum Wcag22FeasibilityOutcomeV1: Codable, Equatable, Sendable {
    case success(Wcag22FeasibilityV1)
    case failure(Wcag22FeasibilityProtocolErrorV1)

    private enum CodingKeys: String, CodingKey {
        case schemaVersion, outcome, feasibility, error
    }
    private enum Outcome: String, Codable { case success, failure }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let schemaVersion = try container.decode(UInt32.self, forKey: .schemaVersion)
        guard schemaVersion == 1 else {
            throw DecodingError.dataCorruptedError(
                forKey: .schemaVersion,
                in: container,
                debugDescription: "unsupported feasibility outcome schema version")
        }
        switch try container.decode(Outcome.self, forKey: .outcome) {
        case .success:
            let feasibility = try container.decode(
                Wcag22FeasibilityV1.self, forKey: .feasibility)
            try feasibility.validate(codingPath: decoder.codingPath + [CodingKeys.feasibility])
            self = .success(feasibility)
        case .failure:
            self = try .failure(
                container.decode(Wcag22FeasibilityProtocolErrorV1.self, forKey: .error))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(UInt32(1), forKey: .schemaVersion)
        switch self {
        case let .success(value):
            try container.encode(Outcome.success, forKey: .outcome)
            try container.encode(value, forKey: .feasibility)
        case let .failure(value):
            try container.encode(Outcome.failure, forKey: .outcome)
            try container.encode(value, forKey: .error)
        }
    }
}

/// Injectable only under `@testable import`; production uses the generated
/// UniFFI byte functions. It makes the pre-copy admission law executable.
struct Wcag22FeasibilityBridge {
    let maxRequestBytes: () -> UInt64
    let evaluateRaw: (Data) throws -> Data
    let envelopeTooLarge: (UInt64) throws -> Data

    static let live = Self(
        maxRequestBytes: wcag22FeasibilityMaxRequestBytesV1,
        evaluateRaw: { try evaluateWcag22FeasibilityRawV1(request: $0) },
        envelopeTooLarge: {
            try wcag22FeasibilityEnvelopeTooLargeV1(requestedBytes: $0)
        })
}

/// Exact protocol-owned request byte ceiling.
public func wcag22FeasibilityMaxBytes() -> UInt64 {
    Wcag22FeasibilityBridge.live.maxRequestBytes()
}

/// Evaluate one exact V1 request carried as `Data`.
public func evaluateWcag22Feasibility(
    _ request: Data
) throws -> Wcag22FeasibilityOutcomeV1 {
    try evaluateWcag22Feasibility(request, using: .live)
}

/// Evaluate one exact V1 request carried as bytes. The size comparison occurs
/// before `Data` materialization and therefore before the raw UniFFI copy.
public func evaluateWcag22Feasibility(
    _ request: [UInt8]
) throws -> Wcag22FeasibilityOutcomeV1 {
    let bridge = Wcag22FeasibilityBridge.live
    return try evaluateWcag22Feasibility(
        requestedBytes: UInt64(request.count),
        using: bridge,
        evaluateAdmitted: { try bridge.evaluateRaw(Data(request)) })
}

func evaluateWcag22Feasibility(
    _ request: Data,
    using bridge: Wcag22FeasibilityBridge
) throws -> Wcag22FeasibilityOutcomeV1 {
    try evaluateWcag22Feasibility(
        requestedBytes: UInt64(request.count),
        using: bridge,
        evaluateAdmitted: { try bridge.evaluateRaw(request) })
}

private func evaluateWcag22Feasibility(
    requestedBytes: UInt64,
    using bridge: Wcag22FeasibilityBridge,
    evaluateAdmitted: () throws -> Data
) throws -> Wcag22FeasibilityOutcomeV1 {
    let encoded: Data
    if requestedBytes > bridge.maxRequestBytes() {
        encoded = try bridge.envelopeTooLarge(requestedBytes)
    } else {
        encoded = try evaluateAdmitted()
    }
    return try JSONDecoder().decode(Wcag22FeasibilityOutcomeV1.self, from: encoded)
}
