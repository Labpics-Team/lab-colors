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
    let referenceSingleton: String
    let referenceHierarchy: String
    let referenceEmpty: String
    let compose: String
    let composeReference: String
    let composeEmpty: String
    let composeReferenceEmpty: String
}

private final class RejectingFieldHost: ProgramUpdateHost, @unchecked Sendable {
    private let lock = NSLock()
    private var retained: ProgramSnapshot?

    func accept(snapshot: ProgramSnapshot) throws -> Bool {
        lock.lock()
        defer { lock.unlock() }
        retained = snapshot
        return false
    }

    func takeSnapshot() -> ProgramSnapshot? {
        lock.lock()
        defer { retained = nil; lock.unlock() }
        return retained
    }
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

    private func referenceProgram(hierarchy: Bool) throws -> Data {
        let builder = try ProgramWireBuilderV3()
            .colorTarget(10, anchor: [68, 68, 68], options: ProgramWireColorOptionsV3(referenceAppearance: 5))
        if hierarchy {
            try builder.colorTarget(20, anchor: [102, 102, 102], options: ProgramWireColorOptionsV3(referenceAppearance: 15))
        }
        try builder.assignment(hierarchy ? [10, 20] : [10],
            options: ProgramWireAssignmentOptionsV3(reference: [ProgramWireReferenceInputV3(port: 2, rgb: [255, 255, 255])]))
            .surfaceInputPort(2).solidPaint(3, target: 10).inputSurface(4, input: 2)
            .pointOccurrence(5, paint: 3, surface: 4)
        if hierarchy {
            try builder.solidPaint(13, target: 20).pointOccurrence(15, paint: 13, surface: 4)
                .preserveTonalHierarchy(30, occurrences: [5, 15])
        }
        try builder.wcag22VisibleUnary(true, id: 6, occurrence: 5, criterion: .sc143TextDefault).output(7, paint: 3)
        if hierarchy {
            try builder.wcag22VisibleUnary(true, id: 16, occurrence: 15, criterion: .sc143TextDefault).output(17, paint: 13)
        }
        return try builder.finish()
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
        XCTAssertEqual(try referenceProgram(hierarchy: false), try bytes(expected.referenceSingleton))
        XCTAssertEqual(try referenceProgram(hierarchy: true), try bytes(expected.referenceHierarchy))
        XCTAssertEqual(try ProgramWireBuilderV3().assignment([], options: ProgramWireAssignmentOptionsV3(reference: [])).finish(),
            try bytes(expected.referenceEmpty))
    }

    func testCompositionUsesIndependentVectorsAndUnchangedAssignmentFieldGrammar() throws {
        let expected = try fixtureV3()
        let targets: [UInt32] = [0x01020304, 0xffffffff, 0]
        let reference = [ProgramWireReferenceInputV3(port: 0x10203040, rgb: [1, 128, 255]),
            ProgramWireReferenceInputV3(port: 0xffffffff, rgb: [254, 129, 0])]
        let cases: [([UInt32], [ProgramWireReferenceInputV3]?, String, UInt8)] = [
            (targets, nil, expected.compose, 5), (targets, reference, expected.composeReference, 6),
            ([], nil, expected.composeEmpty, 5), ([], [], expected.composeReferenceEmpty, 6),
        ]
        for (members, inputs, fixture, tag) in cases {
            var composition = try ProgramWireBuilderV3().compose(members,
                options: ProgramWireComposeOptionsV3(reference: inputs)).finish()
            XCTAssertEqual(composition, try bytes(fixture))
            XCTAssertEqual(composition[26], tag)
            composition[26] -= 2
            XCTAssertEqual(composition, try ProgramWireBuilderV3().assignment(members,
                options: ProgramWireAssignmentOptionsV3(reference: inputs)).finish())
        }
    }

    func testCompositionFailureRollsBackAndSelectionsCannotReplaceEachOther() throws {
        let builder = try ProgramWireBuilderV3().source(1, rgb: [1, 2, 3])
        let before = try builder.finish()
        XCTAssertThrowsError(try builder.compose(Array(repeating: 1, count: 4097)))
        for inputs in [
            [ProgramWireReferenceInputV3(port: 0, rgb: [1, 2])],
            Array(repeating: ProgramWireReferenceInputV3(port: 0, rgb: [1, 2, 3]), count: 2),
            (0...4096).map { ProgramWireReferenceInputV3(port: UInt32($0), rgb: [1, 2, 3]) },
        ] {
            XCTAssertThrowsError(try builder.compose([], options: ProgramWireComposeOptionsV3(reference: inputs)))
            XCTAssertEqual(try builder.finish(), before)
        }
        var targets: [UInt32] = [1], rgb: [UInt8] = [255, 255, 255]
        var inputs = [ProgramWireReferenceInputV3(port: 0, rgb: rgb)]
        try builder.compose(targets, options: ProgramWireComposeOptionsV3(reference: inputs))
        let composed = try builder.finish()
        targets[0] = 2; rgb[0] = 0; inputs.removeAll()
        XCTAssertThrowsError(try builder.compose([]))
        XCTAssertThrowsError(try builder.assignment([]))
        XCTAssertEqual(try builder.finish(), composed)
        let assigned = try ProgramWireBuilderV3().assignment([1])
        let accepted = try assigned.finish()
        XCTAssertThrowsError(try assigned.compose([]))
        XCTAssertEqual(try assigned.finish(), accepted)
    }

    func testAuthoredReferenceAndHierarchySurviveContextChangesAndRuntimeRelease() throws {
        for hierarchy in [false, true] {
            var runtime: ProgramRuntime? = try compileProgramWire(bytes: referenceProgram(hierarchy: hierarchy), streamId: 1)
            let original = try XCTUnwrap(runtime).updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1)
            XCTAssertEqual(original.state(), "ready")
            XCTAssertEqual(try original.outputRgb(index: 0), Data([68, 68, 68]))
            let selections = try XCTUnwrap(try original.referenceSelections())
            XCTAssertEqual(selections, try original.assessmentReport()?.referenceSelections)
            XCTAssertEqual(selections.count, hierarchy ? 2 : 1)
            let strong = try XCTUnwrap(selections.first)
            XCTAssertEqual(strong.targetId, 10)
            XCTAssertEqual(strong.occurrenceId, 5)
            XCTAssertEqual(strong.preference, .referenceStrength)
            let referenceCase = try XCTUnwrap(strong.cases.first)
            XCTAssertEqual(referenceCase.referenceVisible, ProgramRgb8(red: 68, green: 68, blue: 68))
            XCTAssertEqual(referenceCase.referenceBackdrop, ProgramRgb8(red: 255, green: 255, blue: 255))
            XCTAssertEqual(referenceCase.actualContrast, referenceCase.referenceContrast)
            XCTAssertGreaterThan(referenceCase.referenceContrast, 9)
            XCTAssertEqual(referenceCase.polarity, .darker)
            XCTAssertEqual(referenceCase.strengthLoss, 0)
            XCTAssertEqual(referenceCase.virtualDistanceSquared, 0)
            let capacity = try XCTUnwrap(strong.capacity)
            XCTAssertLessThanOrEqual(capacity.ratioLower, capacity.ratioUpper)
            XCTAssertGreaterThanOrEqual(capacity.ratioLower, 1)

            let dark = try XCTUnwrap(runtime).updateObserved(revision: 2, scenarioIds: [1], surfaces: Data([0, 0, 0]), surfaceCount: 1)
            XCTAssertEqual(dark.state(), "ready")
            XCTAssertNotEqual(try dark.outputRgb(index: 0), try original.outputRgb(index: 0))
            let darkSelections = try XCTUnwrap(try dark.referenceSelections())
            XCTAssertEqual(darkSelections, try dark.assessmentReport()?.referenceSelections)
            XCTAssertEqual(darkSelections.first?.cases.first?.polarity, .lighter)
            if hierarchy {
                XCTAssertEqual(try original.outputRgb(index: 1), Data([102, 102, 102]))
                let authoredPair = try XCTUnwrap(try original.assessmentReport()?.tonalHierarchy.first)
                let adaptedPair = try XCTUnwrap(try dark.assessmentReport()?.tonalHierarchy.first)
                XCTAssertEqual(authoredPair.constraintId, 30)
                XCTAssertEqual(authoredPair.strongerOccurrenceId, 5)
                XCTAssertEqual(authoredPair.weakerOccurrenceId, 15)
                XCTAssertEqual(authoredPair.requiredRatio, authoredPair.actualRatio)
                XCTAssertGreaterThan(authoredPair.requiredRatio, 1)
                XCTAssertEqual(authoredPair.requiredGap, log(authoredPair.requiredRatio), accuracy: 1e-12)
                XCTAssertEqual(adaptedPair.requiredRatio, authoredPair.requiredRatio)
                XCTAssertTrue(adaptedPair.satisfied)
                XCTAssertGreaterThanOrEqual(adaptedPair.actualRatio, adaptedPair.requiredRatio)
            }
            let restored = try XCTUnwrap(runtime).updateObserved(revision: 3, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1)
            XCTAssertEqual(try restored.outputRgb(index: 0), try original.outputRgb(index: 0))
            if hierarchy { XCTAssertEqual(try restored.outputRgb(index: 1), try original.outputRgb(index: 1)) }
            let unknown = try XCTUnwrap(runtime).updateUnknown(revision: 4, reasonId: 7)
            XCTAssertNil(try unknown.referenceSelections())
            runtime = nil
            XCTAssertEqual(try original.referenceSelections(), selections)
            XCTAssertEqual(selections.first?.cases.first?.referenceVisible, ProgramRgb8(red: 68, green: 68, blue: 68))
        }
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
            XCTAssertEqual(report.cleanliness.status, .notAssessed)
            XCTAssertNil(report.cleanliness.profile)
            XCTAssertEqual(try snapshot.cleanlinessAssessment(), report.cleanliness)
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
            guard case let .visibleSeparation(verdict, _, _, distance, lower, upper, minimum, maximum) = cell.assessment else {
                return XCTFail("expected the declared visible separation")
            }
            XCTAssertEqual(verdict, .pass)
            XCTAssertGreaterThanOrEqual(lower, minimum)
            XCTAssertLessThanOrEqual(upper, maximum)
            XCTAssertGreaterThanOrEqual(distance, lower)
            XCTAssertLessThanOrEqual(distance, upper)
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
        XCTAssertNil(try rejected.cleanlinessAssessment())
        XCTAssertEqual(try valid.outputRgb(index: 0), Data([0, 0, 0]))
    }

    private func nominalProgram() throws -> Data {
        try ProgramWireBuilderV3().source(1, rgb: [255, 255, 255]).fixedTarget(2, source: 1)
            .surfaceInputPort(6).opacityInput(5, value: 0.5)
            .solidPaint(3, target: 2).opacityPaint(4, source: 3, opacity: 5).inputSurface(7, input: 6)
            .pointOccurrence(8, paint: 4, surface: 7).nominalSrgb8Visible(false, id: 10, occurrence: 8)
            .output(17, paint: 4).assignment([]).finish()
    }

    func testDeclaredFieldReportsSurviveRejectionDirectionChangesAndRuntimeRelease() throws {
        let start = ProgramRgb8(red: 0, green: 0, blue: 0)
        let end = ProgramRgb8(red: 2, green: 2, blue: 2)
        let expected = ProgramFieldAssessment(inputIndex: 0, from: start, to: end,
            sourceModel: "opaque-affine-encoded-srgb8-half-up-v1", domain: "whole-field", rendering: "unknown",
            regionCount: 7, witnesses: [
                ProgramFieldWitness(numerator: 0, denominator: 1, nominal: start),
                ProgramFieldWitness(numerator: 1, denominator: 4, nominal: ProgramRgb8(red: 1, green: 1, blue: 1)),
                ProgramFieldWitness(numerator: 3, denominator: 4, nominal: end),
            ])
        var runtime: ProgramRuntime? = try compileProgramWire(bytes: nominalProgram(), streamId: 1)
        let previous = try XCTUnwrap(runtime).updateObserved(revision: 1, scenarioIds: [9],
            surfaces: Data([255, 255, 255]), surfaceCount: 1)
        let table = try previous.copyOutputTable()
        XCTAssertEqual(previous.state(), "ready")
        XCTAssertNil(try previous.fieldAssessment())

        let host = RejectingFieldHost()
        XCTAssertFalse(try XCTUnwrap(runtime).updateDeclaredFieldWith(revision: 2, surfaces: Data([17, 17, 17]),
            varyingInput: 0, from: start, to: end, accept: host))
        let rejected = try XCTUnwrap(host.takeSnapshot())
        XCTAssertEqual(rejected.state(), "ready")
        XCTAssertEqual(rejected.revision(), 2)
        XCTAssertEqual(try rejected.fieldAssessment(), expected)
        let replay = try XCTUnwrap(runtime).updateObserved(revision: 1, scenarioIds: [9],
            surfaces: Data([255, 255, 255]), surfaceCount: 1)
        XCTAssertEqual(replay.revision(), 1)
        XCTAssertEqual(try replay.copyOutputTable(), table)
        XCTAssertNil(try replay.fieldAssessment())

        let accepted = try XCTUnwrap(runtime).updateDeclaredField(revision: 2, surfaces: Data([199, 199, 199]),
            varyingInput: 0, from: start, to: end)
        XCTAssertEqual(accepted.state(), "ready")
        let ownedReport = try XCTUnwrap(try accepted.fieldAssessment())
        XCTAssertEqual(ownedReport, expected)
        XCTAssertThrowsError(try XCTUnwrap(runtime).updateDeclaredField(revision: 2, surfaces: Data([0, 0, 0]),
            varyingInput: 0, from: end, to: start)) { error in
            guard case let LabColorsError.Refused(code, operation) = error else {
                return XCTFail("unexpected field update error: \(error)")
            }
            XCTAssertEqual(code, "program_update")
            XCTAssertEqual(operation, "updateDeclaredField")
        }
        let fieldReplay = try XCTUnwrap(runtime).updateDeclaredField(revision: 2, surfaces: Data([199, 199, 199]),
            varyingInput: 0, from: start, to: end)
        XCTAssertEqual(fieldReplay.state(), "ready")
        XCTAssertEqual(fieldReplay.revision(), 2)
        XCTAssertEqual(try fieldReplay.fieldAssessment(), ownedReport)
        let reversed = try XCTUnwrap(runtime).updateDeclaredField(revision: 3, surfaces: Data([0, 0, 0]),
            varyingInput: 0, from: end, to: start)
        let reverseReport = try XCTUnwrap(try reversed.fieldAssessment())
        XCTAssertEqual(reverseReport.from, end)
        XCTAssertEqual(reverseReport.to, start)
        XCTAssertEqual(reverseReport.regionCount, expected.regionCount)
        let restored = try XCTUnwrap(runtime).updateObserved(revision: 4, scenarioIds: [9],
            surfaces: Data([255, 255, 255]), surfaceCount: 1)
        XCTAssertNil(try restored.fieldAssessment())
        XCTAssertEqual(try restored.copyOutputTable(), table)

        runtime = nil
        XCTAssertEqual(try rejected.fieldAssessment(), expected)
        XCTAssertEqual(try accepted.fieldAssessment(), ownedReport)
        XCTAssertEqual(try previous.copyOutputTable(), table)
    }

    func testNominalVisibleReportsAndSummaryCrossTheNativeBoundary() throws {
        let runtime = try compileProgramWire(bytes: nominalProgram(), streamId: 1)
        let first = try runtime.updateObserved(revision: 1, scenarioIds: [9], surfaces: Data([255, 255, 255]), surfaceCount: 1)
        let summary = try XCTUnwrap(try first.cleanlinessAssessment())
        XCTAssertEqual(summary.status, .conforms)
        XCTAssertEqual(summary.profile, "nominal-srgb8-v2")
        XCTAssertEqual(summary.basis, "declared-convention")
        XCTAssertEqual(summary.scope, "declared-final-points")
        XCTAssertEqual(summary.accepted, 1)
        XCTAssertEqual(summary.rejected, 0)
        XCTAssertEqual(summary.unavailable, 0)
        XCTAssertEqual(try first.assessmentReport()?.cleanliness, summary)
        let changed = try runtime.updateObserved(revision: 2, scenarioIds: [9], surfaces: Data([1, 1, 3]), surfaceCount: 1)
        XCTAssertEqual(changed.state(), "ready")
        let report = try XCTUnwrap(try changed.assessmentReport())
        XCTAssertEqual(try changed.cleanlinessAssessment(), report.cleanliness)
        XCTAssertEqual(report.cleanliness.status, .violates)
        XCTAssertEqual(report.cleanliness.rejected, 1)
        guard case let .nominalSrgb8VisibleRejected(visible, lower, upper) = try XCTUnwrap(report.cells.first).assessment else {
            return XCTFail("expected the final visible rejection")
        }
        XCTAssertEqual([visible.red, visible.green, visible.blue], [128, 128, 129])
        XCTAssertTrue((lower...upper).contains(129))
        XCTAssertEqual(try first.cleanlinessAssessment(), summary)
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
            ProgramWireColorOptionsV3(appearance: 0, referenceAppearance: 0),
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
        let invalidHierarchies: [[UInt32]] = [[], [3], [3, 3], Array(0...4096)]
        for occurrences in invalidHierarchies {
            XCTAssertThrowsError(try builder.preserveTonalHierarchy(7, occurrences: occurrences))
            XCTAssertEqual(try builder.finish(), before)
        }
        for inputs in [
            [ProgramWireReferenceInputV3(port: 0, rgb: [1, 2])],
            Array(repeating: ProgramWireReferenceInputV3(port: 0, rgb: [1, 2, 3]), count: 2),
            (0...4096).map { ProgramWireReferenceInputV3(port: UInt32($0), rgb: [1, 2, 3]) },
        ] {
            XCTAssertThrowsError(try builder.assignment([], options: ProgramWireAssignmentOptionsV3(reference: inputs)))
            XCTAssertEqual(try builder.finish(), before)
        }
        XCTAssertEqual(try builder.finish(), before)
        try builder.colorTarget(21, anchor: [1, 2, 3]).assignment([21])
        let accepted = try builder.finish()
        XCTAssertThrowsError(try builder.assignment([]))
        XCTAssertEqual(try builder.finish(), accepted)
    }

    func testReferenceAuthoringOwnsValuesAndDistinguishesMissingReference() throws {
        var referenceRgb: [UInt8] = [255, 255, 255]
        var referenceInputs = [ProgramWireReferenceInputV3(port: 0, rgb: referenceRgb)]
        var hierarchyMembers: [UInt32] = [0, 1]
        let referenceBuilder = try ProgramWireBuilderV3().preserveTonalHierarchy(7, occurrences: hierarchyMembers)
            .assignment([], options: ProgramWireAssignmentOptionsV3(reference: referenceInputs))
        let referenceAccepted = try referenceBuilder.finish()
        referenceRgb[0] = 0; referenceInputs.removeAll(); hierarchyMembers[1] = 0
        XCTAssertEqual(try referenceBuilder.finish(), referenceAccepted)
        XCTAssertNotEqual(try ProgramWireBuilderV3().assignment([]).finish(),
            try ProgramWireBuilderV3().assignment([], options: ProgramWireAssignmentOptionsV3(reference: [])).finish())
        XCTAssertNotEqual(try ProgramWireBuilderV3().colorTarget(2, anchor: [1, 2, 3], options: ProgramWireColorOptionsV3(referenceAppearance: 0)).finish(),
            try ProgramWireBuilderV3().colorTarget(2, anchor: [1, 2, 3]).finish())
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
