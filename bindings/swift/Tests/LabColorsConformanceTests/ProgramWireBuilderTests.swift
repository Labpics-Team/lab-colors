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
}
