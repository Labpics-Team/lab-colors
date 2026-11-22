import Foundation
import XCTest
import LabColors

private struct AuthoringV3Fixture: Decodable {
    let schemaVersion: Int
    let computedBox: String
    let computedGray: String
    let contextual: String
    let encodingOnly: String
    let fixedOnly: String
}

final class ProgramWireBuilderV3Tests: XCTestCase {
    private func fixtureV3() throws -> AuthoringV3Fixture {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "program-authoring-v3", withExtension: "json", subdirectory: "Fixtures"))
        let result = try JSONDecoder().decode(AuthoringV3Fixture.self, from: Data(contentsOf: url))
        XCTAssertEqual(result.schemaVersion, 1)
        return result
    }

    private func graph(_ builder: ProgramWireBuilderV3, contrast: Bool = true) throws -> ProgramWireBuilderV3 {
        try builder.surfaceInputPort(31).solidPaint(41, target: 21).inputSurface(51, input: 31)
            .pointOccurrence(61, paint: 41, surface: 51)
        if contrast { try builder.wcag22VisibleUnary(true, id: 81, occurrence: 61, criterion: .sc143TextDefault) }
        return try builder.output(91, paint: 41)
    }

    private func contextual(_ domain: ProgramWireColorDomainV3 = .srgb) -> ProgramWireColorOptionsV3 {
        ProgramWireColorOptionsV3(domain: domain,
            identity: ProgramWireColorIdentityV3(maxHueDeviation: 12, chroma: [0.02, 0.35], lightness: [0.2, 0.9]),
            maxChange: 1.5, appearance: 61,
            tone: ProgramWireToneV3(from: 31, curve: [[0, 0.25], [0.5, 0.55], [1, 0.9]]))
    }

    func testIndependentV3VectorsCoverEveryFieldAndEmptyAssignment() throws {
        let expected = try fixtureV3()
        XCTAssertEqual(try graph(ProgramWireBuilderV3().computedTarget(21,
            domain: .box(lower: [0, 0, 0], upper: [15, 15, 15]), anchor: [8, 9, 10], maxDelta: 2))
            .assignment([21]).finish(), try bytes(expected.computedBox))
        XCTAssertEqual(try graph(ProgramWireBuilderV3().computedTarget(21,
            domain: .gray(lower: 0, upper: 255), anchor: [128, 128, 128], maxDelta: 2))
            .assignment([21]).finish(), try bytes(expected.computedGray))
        XCTAssertEqual(try graph(ProgramWireBuilderV3().colorTarget(21, anchor: [0, 103, 244], options: contextual()), contrast: false)
            .visibleSeparationHard(81, occurrence: 61, minimum: 0.05, maximum: 0.8)
            .assignment([21]).finish(), try bytes(expected.contextual))
        XCTAssertEqual(try graph(ProgramWireBuilderV3().source(11, rgb: [0, 0, 0]).fixedTarget(21, source: 11))
            .assignment([]).finish(), try bytes(expected.fixedOnly))
        let all = try ProgramWireBuilderV3()
            .colorTarget(21, anchor: [5, 7, 9])
            .colorTarget(22, anchor: [80, 80, 80], options: ProgramWireColorOptionsV3(domain: .neutral))
            .colorTarget(23, anchor: [90, 90, 90], options: contextual(.neutral))
            .computedTarget(24, domain: .box(lower: [1, 2, 3], upper: [4, 5, 6]), anchor: [7, 8, 9], maxDelta: 0)
            .computedTarget(25, domain: .gray(lower: 11, upper: 12), anchor: [13, 14, 15], maxDelta: 3)
            .pointOccurrence(26, paint: 27, surface: 28)
            .visibleSeparationHard(29, occurrence: 26, minimum: 0, maximum: 2).assignment([])
        XCTAssertEqual(try all.finish(), try bytes(expected.encodingOnly))
    }

    func testComputedAndContextualProgramsReachCoreAndExposeAssessments() throws {
        let expected = try fixtureV3()
        let box = try compileProgramWire(bytes: bytes(expected.computedBox), streamId: 1)
        let exact = try box.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1)
        XCTAssertEqual(exact.state(), "ready")
        XCTAssertEqual(exact.optimality(), "exact")
        XCTAssertEqual(try exact.outputRgb(index: 0), Data([8, 9, 10]))

        let gray = try compileProgramWire(bytes: bytes(expected.computedGray), streamId: 2)
        let onWhite = try gray.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1)
        let onBlack = try gray.updateObserved(revision: 2, scenarioIds: [1], surfaces: Data([0, 0, 0]), surfaceCount: 1)
        for snapshot in [onWhite, onBlack] {
            XCTAssertEqual(snapshot.state(), "ready")
            let rgb = try snapshot.outputRgb(index: 0)
            XCTAssertEqual(rgb.count, 3)
            XCTAssertEqual(Set(rgb).count, 1)
            let report = try XCTUnwrap(try snapshot.assessmentReport())
            XCTAssertEqual(report.cells.count, 1)
            let cell = try XCTUnwrap(report.cells.first)
            XCTAssertTrue(cell.hard)
            guard case let .wcag22Srgb8(verdict) = cell.assessment else { return XCTFail("expected the declared WCAG assessment") }
            XCTAssertEqual(verdict, .pass)
        }
        XCTAssertNotEqual(try onWhite.outputRgb(index: 0), try onBlack.outputRgb(index: 0))

        let contextual = try compileProgramWire(bytes: bytes(expected.contextual), streamId: 3)
        let light = try contextual.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1)
        let dark = try contextual.updateObserved(revision: 2, scenarioIds: [1], surfaces: Data([0, 0, 0]), surfaceCount: 1)
        for snapshot in [light, dark] {
            XCTAssertEqual(snapshot.state(), "ready")
            let report = try XCTUnwrap(try snapshot.assessmentReport())
            XCTAssertEqual(report.cells.count, 1)
            let cell = try XCTUnwrap(report.cells.first)
            guard case let .visibleSeparation(verdict, _, _, distance, minimum, maximum) = cell.assessment else {
                return XCTFail("expected the declared visible separation")
            }
            XCTAssertEqual(verdict, .pass)
            XCTAssertGreaterThanOrEqual(distance, minimum)
            XCTAssertLessThanOrEqual(distance, maximum)
        }
        XCTAssertNotEqual(try light.outputRgb(index: 0), try dark.outputRgb(index: 0))
    }

    func testFixedOnlyV3ChecksTheAuthoredTupleAndReportsARealConflict() throws {
        let runtime = try compileProgramWire(bytes: bytes(fixtureV3().fixedOnly), streamId: 1)
        let valid = try runtime.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1)
        XCTAssertEqual(valid.state(), "ready")
        XCTAssertEqual(try valid.outputRgb(index: 0), Data([0, 0, 0]))
        let rejected = try runtime.updateObserved(revision: 2, scenarioIds: [1], surfaces: Data([0, 0, 0]), surfaceCount: 1)
        XCTAssertEqual(rejected.state(), "unsatisfied")
        XCTAssertNotNil(try rejected.diagnostics())
        XCTAssertEqual(try valid.outputRgb(index: 0), Data([0, 0, 0]))
    }

    func testV3FailuresRollBackCompleteEntriesAndAllowContinuation() throws {
        let builder = try ProgramWireBuilderV3().source(11, rgb: [1, 2, 3])
        let before = try builder.finish()
        for options in [
            ProgramWireColorOptionsV3(identity: ProgramWireColorIdentityV3(maxHueDeviation: .nan)),
            ProgramWireColorOptionsV3(identity: ProgramWireColorIdentityV3(maxHueDeviation: 181)),
            ProgramWireColorOptionsV3(identity: ProgramWireColorIdentityV3(chroma: [0.4, 0.2])),
            ProgramWireColorOptionsV3(identity: ProgramWireColorIdentityV3(lightness: [0.4])),
            ProgramWireColorOptionsV3(maxChange: .infinity),
            ProgramWireColorOptionsV3(maxChange: 2.1),
            ProgramWireColorOptionsV3(tone: ProgramWireToneV3(from: 31, curve: [[0, 0.2], [0.5, 0.5], [0.5, 0.8], [1, 1]])),
            ProgramWireColorOptionsV3(tone: ProgramWireToneV3(from: 31, curve: [[0, 0.2], [1, .nan]])),
            ProgramWireColorOptionsV3(tone: ProgramWireToneV3(from: 31, curve: [[0, 0.2], [0.8, 1]])),
        ] {
            XCTAssertThrowsError(try builder.colorTarget(21, anchor: [1, 2, 3], options: options))
            XCTAssertEqual(try builder.finish(), before)
        }
        XCTAssertThrowsError(try builder.colorTarget(21, anchor: [1, 2]))
        XCTAssertThrowsError(try builder.computedTarget(21, domain: .box(lower: [2, 0, 0], upper: [1, 2, 3]), anchor: [1, 2, 3], maxDelta: 2))
        XCTAssertThrowsError(try builder.computedTarget(21, domain: .gray(lower: 2, upper: 1), anchor: [1, 2, 3], maxDelta: 2))
        XCTAssertThrowsError(try builder.computedTarget(21, domain: .gray(lower: 0, upper: 255), anchor: [1, 2, 3], maxDelta: Double.greatestFiniteMagnitude))
        XCTAssertThrowsError(try builder.visibleSeparationHard(81, occurrence: 61, minimum: 1, maximum: 0.5))
        XCTAssertThrowsError(try builder.visibleSeparationHard(81, occurrence: 61, minimum: 0, maximum: .nan))
        XCTAssertThrowsError(try builder.assignment(Array(repeating: 21, count: 4097)))
        XCTAssertEqual(try builder.finish(), before)
        try builder.colorTarget(21, anchor: [1, 2, 3]).assignment([21])
        let accepted = try builder.finish()
        XCTAssertThrowsError(try builder.assignment([]))
        XCTAssertEqual(try builder.finish(), accepted)
    }

    func testV3OwnsAcceptedArraysAndCanonicalizesAllNegativeZeros() throws {
        var rgb: [UInt8] = [1, 2, 3], curve = [[0.0, 0.0], [1.0, 1.0]]
        let builder = try ProgramWireBuilderV3().colorTarget(21, anchor: rgb,
            options: ProgramWireColorOptionsV3(tone: ProgramWireToneV3(from: 31, curve: curve)))
        let accepted = try builder.finish()
        rgb[0] = 99; curve[1][1] = 0.5
        XCTAssertEqual(try builder.finish(), accepted)
        func zero(_ value: Double) throws -> Data {
            try ProgramWireBuilderV3().colorTarget(21, anchor: [1, 2, 3],
                options: ProgramWireColorOptionsV3(identity: ProgramWireColorIdentityV3(maxHueDeviation: value,
                    chroma: [value, 1], lightness: [value, 1]), maxChange: value,
                    tone: ProgramWireToneV3(from: 31, curve: [[value, value], [1, 1]])))
                .computedTarget(22, domain: .gray(lower: 0, upper: 255), anchor: [1, 2, 3], maxDelta: value)
                .visibleSeparationHard(81, occurrence: 61, minimum: value, maximum: value).finish()
        }
        XCTAssertEqual(try zero(-0.0), try zero(0.0))
    }
}
