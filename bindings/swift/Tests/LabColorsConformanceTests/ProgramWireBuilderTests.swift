import Foundation
import XCTest
import LabColors

final class ProgramWireBuilderTests: XCTestCase {
    private func graph<T: ProgramWireBuilderV1>(_ builder: T) throws -> T {
        try builder.surfaceInputPort(31).solidPaint(41, target: 21).inputSurface(51, input: 31)
            .sourceOverOccurrence(61, subject: 41, against: 51, adaptingLuminance: 64, backgroundRatio: 0.2, surround: .average)
            .presentationRoot(71, terminal: 61).presentationTarget(71, occurrence: 61)
            .wcag22VisibleUnary(true, id: 81, occurrence: 61, criterion: .sc143TextDefault).output(91, paint: 41)
        return builder
    }

    func testIndependentWireVectorsCoverEveryAuthoringOperation() throws {
        let expected = try fixture()
        let fixed = try ProgramWireBuilderV1().source(11, rgb: [20, 20, 20]).fixedTarget(21, source: 11)
        XCTAssertEqual(try graph(fixed).finish(), try bytes(expected.fixed))
        let adaptive = try ProgramWireBuilderV2().finiteTarget(21, candidates: [
            ProgramCandidate(id: 201, rgb: [0, 0, 0], opacity: 1),
            ProgramCandidate(id: 202, rgb: [255, 255, 255], opacity: 1),
        ]).selectionRelease(1, rankGroups: [
            [ProgramSelectionState(id: 1, choices: [ProgramSelectionChoice(target: 21, candidate: 201)])],
            [ProgramSelectionState(id: 2, choices: [ProgramSelectionChoice(target: 21, candidate: 202)])],
        ])
        XCTAssertEqual(try graph(adaptive).finish(), try bytes(expected.adaptive))
        let runtime = try compileProgramWire(bytes: adaptive.finish(), streamId: 1)
        XCTAssertEqual(try runtime.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1).outputRgb(index: 0), Data([0, 0, 0]))
        let all = try ProgramWireBuilderV1().source(11, rgb: [1, 2, 3]).fixedTarget(21, source: 11)
            .finiteTarget(22, candidates: [
                ProgramCandidate(id: 201, rgb: [0, 0, 0], opacity: 0.5),
                ProgramCandidate(id: 202, rgb: [255, 255, 255], opacity: 1),
            ]).family(90, releaseBytes: Data(repeating: 0, count: 32))
            .surfaceInputPort(31).opacityInput(32, value: 0.5).solidPaint(41, target: 21)
            .opacityPaint(42, source: 41, opacity: 32).inputSurface(51, input: 31).occurrenceSurface(52, occurrence: 61)
            .sourceOverOccurrence(61, subject: 42, against: 51, adaptingLuminance: 64, backgroundRatio: 0.2, surround: .dim)
            .presentationRoot(71, terminal: 61).presentationTarget(71, occurrence: 61)
            .exactVisibleUnary(true, id: 81, occurrence: 61, expectedRgb: [1, 2, 3])
            .wcag22VisibleUnary(false, id: 82, occurrence: 61, criterion: .sc1411UiComponentOrState)
            .declaredSrgb8CleanSet(true, id: 83, root: 71, occurrence: 61)
            .exactIntrinsicRelationHard(84, reference: 11, candidates: [11]).output(91, paint: 42)
        XCTAssertEqual(try all.finish(), try bytes(expected.encodingOnly))
    }

    func testRejectedDeclarationLeavesPreviousBytesAndAllowsLegalContinuation() throws {
        let builder = try ProgramWireBuilderV2().source(11, rgb: [20, 20, 20])
        let before = try builder.finish()
        XCTAssertThrowsError(try builder.finiteTarget(21, candidates: [ProgramCandidate(id: 1, rgb: [1, 2], opacity: 1)]))
        XCTAssertThrowsError(try builder.opacityInput(32, value: .nan))
        XCTAssertThrowsError(try builder.opacityInput(32, value: .infinity))
        XCTAssertThrowsError(try builder.opacityInput(32, value: -0.1))
        XCTAssertThrowsError(try builder.family(90, releaseBytes: Data(repeating: 0, count: 31)))
        XCTAssertThrowsError(try builder.selectionRelease(1, rankGroups: [[]]))
        XCTAssertEqual(try builder.finish(), before)
        try builder.fixedTarget(21, source: 11)
        let result = try compileProgramWire(bytes: graph(builder).finish(), streamId: 1)
        XCTAssertEqual(try result.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1).state(), "ready")
    }

    func testEncodingOwnsAcceptedInputsAndCanonicalizesNegativeZero() throws {
        var rgb: [UInt8] = [1, 2, 3]
        let builder = try ProgramWireBuilderV1().source(1, rgb: rgb)
        let accepted = try builder.finish()
        rgb[0] = 99
        XCTAssertEqual(try builder.finish(), accepted)
        XCTAssertEqual(try ProgramWireBuilderV1().opacityInput(1, value: -0.0).finish(),
                       try ProgramWireBuilderV1().opacityInput(1, value: 0.0).finish())
        let bounded = ProgramWireBuilderV1()
        for id in 0..<4096 { try bounded.surfaceInputPort(UInt32(id)) }
        let full = try bounded.finish()
        XCTAssertThrowsError(try bounded.surfaceInputPort(4096))
        XCTAssertEqual(try bounded.finish(), full)
    }

    func testLateFailureRollsBackTheWholeEntryWithoutLosingEarlierEntries() throws {
        let builder = try ProgramWireBuilderV2().source(11, rgb: [20, 20, 20])
        let before = try builder.finish()
        XCTAssertThrowsError(try builder.finiteTarget(21, candidates: [
            ProgramCandidate(id: 1, rgb: [1, 2, 3], opacity: 1),
            ProgramCandidate(id: 2, rgb: [4, 5, 6], opacity: .nan),
        ]))
        let good = ProgramSelectionState(id: 1, choices: [ProgramSelectionChoice(target: 21, candidate: 1)])
        XCTAssertThrowsError(try builder.selectionRelease(1, rankGroups: [[good], []]))
        XCTAssertEqual(try builder.finish(), before)
        try builder.selectionRelease(2, rankGroups: [[good]])
        let accepted = try builder.finish()
        XCTAssertThrowsError(try builder.selectionRelease(3, rankGroups: [[good]]))
        XCTAssertEqual(try builder.finish(), accepted)
        XCTAssertEqual(before, try ProgramWireBuilderV2().source(11, rgb: [20, 20, 20]).finish())
    }

    func testGrowthPreservesHeldSnapshotsAndIndependentSectionCounts() throws {
        let builder = ProgramWireBuilderV1()
        var held: [(Int, Data)] = []
        for index in 0..<4096 {
            if index.isMultiple(of: 257) { held.append((index, try builder.finish())) }
            try builder.source(UInt32(index), rgb: [1, 2, 3]).output(UInt32(index), paint: UInt32(index))
        }
        for (count, bytes) in held {
            let reference = ProgramWireBuilderV1()
            for index in 0..<count {
                try reference.source(UInt32(index), rgb: [1, 2, 3]).output(UInt32(index), paint: UInt32(index))
            }
            XCTAssertEqual(bytes, try reference.finish())
            let encodedLength = bytes.dropFirst(6).prefix(4).enumerated().reduce(UInt32(0)) {
                $0 | (UInt32($1.element) << (8 * $1.offset))
            }
            XCTAssertEqual(Int(encodedLength), bytes.count)
            XCTAssertEqual(bytes.count, 66 + count * (7 + 8))
        }
        let full = try builder.finish()
        XCTAssertThrowsError(try builder.source(4096, rgb: [1, 2, 3]))
        XCTAssertEqual(try builder.finish(), full)
    }

    func testComputedDomainsAndPreferenceOrderUseIndependentWireBytes() throws {
        let expected = try fixture().computed
        let all = try ProgramWireBuilderV3()
            .computedTarget(0x10203040, domain: .box(lower: [1, 2, 3], upper: [4, 5, 6]), anchor: [250, 0, 7], maxDelta: 0.25)
            .computedTarget(19, domain: .gray(lower: 10, upper: 12), anchor: [9, 8, 7], maxDelta: 8)
            .assignment([0x10203040, 19])
        XCTAssertEqual(try all.finish(), try bytes(expected.encodingOnly))
        for (domain, vector) in [(ProgramWireDomainV3.gray(lower: 0, upper: 255), expected.gray),
                                 (.box(lower: [0, 0, 0], upper: [255, 255, 255]), expected.box)] {
            let builder = try ProgramWireBuilderV3().computedTarget(21, domain: domain, anchor: [64, 64, 64], maxDelta: 1).assignment([21])
            try builder.surfaceInputPort(31).opacityInput(32, value: 0.5)
                .solidPaint(41, target: 21).opacityPaint(42, source: 41, opacity: 32).inputSurface(51, input: 31)
                .sourceOverOccurrence(61, subject: 42, against: 51, adaptingLuminance: 64, backgroundRatio: 0.2, surround: .dim)
                .presentationRoot(71, terminal: 61).presentationTarget(71, occurrence: 61)
                .exactVisibleUnary(true, id: 81, occurrence: 61, expectedRgb: [96, 96, 96]).output(91, paint: 42)
            XCTAssertEqual(try builder.finish(), try bytes(vector))
        }
    }

    func testComputedRejectedInputsPreserveBytesAndLegalContinuation() throws {
        let builder = try ProgramWireBuilderV3().source(11, rgb: [20, 20, 20])
        let before = try builder.finish()
        let invalid: [ProgramWireDomainV3] = [
            .gray(lower: 2, upper: 1), .box(lower: [1, 2], upper: [3, 4, 5]),
            .box(lower: [1, 2, 3], upper: [3, 4]),
            .box(lower: [2, 0, 0], upper: [1, 1, 1]),
            .box(lower: [0, 2, 0], upper: [1, 1, 1]),
            .box(lower: [0, 0, 2], upper: [1, 1, 1]),
        ]
        for domain in invalid { XCTAssertThrowsError(try builder.computedTarget(21, domain: domain, anchor: [0, 0, 0], maxDelta: 1)) }
        for delta in [-1, -Double.leastNonzeroMagnitude, .nan, .infinity, -.infinity, .greatestFiniteMagnitude, 1e155] {
            XCTAssertThrowsError(try builder.computedTarget(21, domain: .gray(lower: 0, upper: 255), anchor: [0, 0, 0], maxDelta: delta))
        }
        XCTAssertThrowsError(try builder.computedTarget(21, domain: .gray(lower: 0, upper: 255), anchor: [0, 0], maxDelta: 1))
        XCTAssertThrowsError(try builder.assignment([]))
        XCTAssertThrowsError(try builder.assignment(Array(repeating: 21, count: 4097)))
        XCTAssertEqual(try builder.finish(), before)
        try builder.computedTarget(21, domain: .gray(lower: 255, upper: 255), anchor: [0, 0, 0], maxDelta: 1).assignment([21])
        let accepted = try builder.finish()
        XCTAssertThrowsError(try builder.assignment([21]))
        XCTAssertEqual(try builder.finish(), accepted)
    }

    func testComputedBoundsCanonicalZeroAndOwnedSnapshots() throws {
        for domain in [ProgramWireDomainV3.gray(lower: 0, upper: 255), .gray(lower: 19, upper: 19),
                       .box(lower: [0, 0, 0], upper: [255, 255, 255]), .box(lower: [1, 2, 3], upper: [1, 2, 3])] {
            XCTAssertEqual(try ProgramWireBuilderV3().computedTarget(.max, domain: domain, anchor: [0, 0, 0], maxDelta: -0.0).finish(),
                           try ProgramWireBuilderV3().computedTarget(.max, domain: domain, anchor: [0, 0, 0], maxDelta: 0.0).finish())
        }
        var lower: [UInt8] = [1, 2, 3], anchor: [UInt8] = [4, 5, 6]
        var order: [UInt32] = [29, 21]
        let builder = try ProgramWireBuilderV3().computedTarget(21, domain: .box(lower: lower, upper: [7, 8, 9]), anchor: anchor, maxDelta: 1).assignment(order)
        let accepted = try builder.finish()
        lower[0] = 255; anchor[0] = 255; order.reverse()
        XCTAssertEqual(try builder.finish(), accepted)
        let full = ProgramWireBuilderV3()
        for id in 0..<4096 { try full.computedTarget(UInt32(id), domain: .gray(lower: 0, upper: 0), anchor: [0, 0, 0], maxDelta: 0) }
        try full.assignment((0..<4096).reversed().map(UInt32.init))
        let snapshot = try full.finish()
        XCTAssertThrowsError(try full.computedTarget(4096, domain: .gray(lower: 0, upper: 0), anchor: [0, 0, 0], maxDelta: 0))
        XCTAssertEqual(try full.finish(), snapshot)
    }

}
