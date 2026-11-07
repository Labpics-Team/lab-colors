import Foundation
import XCTest
import LabColors

struct RuntimeStep: Decodable {
    let revision: UInt64
    let backgrounds: [[UInt8]]?
    let unknownReason: UInt32?
    let state: String
    let rgb: [UInt8]?
    let reason: String?
}
struct ComputedFixture: Decodable {
    let gray: String
    let box: String
    let relation: String
    let encodingOnly: String
    let grayHistory: [RuntimeStep]
    let boxHistory: [RuntimeStep]
    let relationHistory: [RuntimeStep]
}
struct RuntimeFixture: Decodable {
    let schemaVersion: Int
    let fixed: String
    let adaptive: String
    let encodingOnly: String
    let history: [RuntimeStep]
    let computed: ComputedFixture
}
func fixture() throws -> RuntimeFixture {
    let url = try XCTUnwrap(Bundle.module.url(forResource: "program-runtime-v1", withExtension: "json", subdirectory: "Fixtures"))
    let result = try JSONDecoder().decode(RuntimeFixture.self, from: Data(contentsOf: url))
    XCTAssertEqual(result.schemaVersion, 1)
    return result
}
func bytes(_ hex: String) throws -> Data {
    let characters = Array(hex.utf8)
    guard characters.count % 2 == 0 else { throw ProgramWireError.invalidDeclaration("odd hex length") }
    return try Data(stride(from: 0, to: characters.count, by: 2).map {
        try XCTUnwrap(UInt8(String(decoding: characters[$0..<$0 + 2], as: UTF8.self), radix: 16))
    })
}
private func assertCode(_ error: Error, _ expected: String, file: StaticString = #filePath, line: UInt = #line) {
    guard case let LabColorsError.Refused(code, _) = error else { return XCTFail("unexpected error: \(error)", file: file, line: line) }
    XCTAssertEqual(code, expected, file: file, line: line)
}

// Поля сгенерированных value-records сравниваются целиком: новый metadata-field
// нельзя забыть в Swift-проекции, сохранив только правильный checksum Core.
private func reflectedJSON(_ value: Any) throws -> Any {
    let mirror = Mirror(reflecting: value)
    switch mirror.displayStyle {
    case .collection:
        return try mirror.children.map { try reflectedJSON($0.value) }
    case .struct:
        return try Dictionary(uniqueKeysWithValues: mirror.children.map {
            (try XCTUnwrap($0.label), try reflectedJSON($0.value))
        })
    default:
        return value
    }
}

// Изменяемое состояние тестового host защищено lock; callback не держит его при повторном входе.
private final class RuntimeHost: ProgramPointSinkHost, @unchecked Sendable {
    private let lock = NSLock()
    private let expectedOpacity: Double
    private var rejected = false
    private var callbackFailure = false
    private var reentrant = false
    private var point: Data?
    private var sequence: UInt64 = 0
    private var calls = 0
    private var epoch: UInt64?
    private weak var owner: ProgramAttachment?
    init(opacity: Double = 1) { expectedOpacity = opacity }
    func configure(owner: ProgramAttachment, reentrant: Bool) { lock.lock(); defer { lock.unlock() }; self.owner = owner; self.reentrant = reentrant }
    func reject(_ value: Bool) { lock.lock(); defer { lock.unlock() }; rejected = value }
    func fail(_ value: Bool) { lock.lock(); defer { lock.unlock() }; callbackFailure = value }
    func callCount() -> Int { lock.lock(); defer { lock.unlock() }; return calls }
    func installed() -> Data? { lock.lock(); defer { lock.unlock() }; return point }
    func revoke() { lock.lock(); defer { lock.unlock() }; point = nil }
    func tryInstall(intent: ProgramPointSinkIntent) throws -> Bool {
        lock.lock(); calls += 1; let owner = self.owner; let reentrant = self.reentrant; lock.unlock()
        if reentrant, let owner {
            XCTAssertThrowsError(try owner.updateUnknown(revision: 99, reasonId: 7)) { assertCode($0, "program_attachment_busy") }
            XCTAssertThrowsError(try owner.materializationAuthority()) { assertCode($0, "program_attachment_busy") }
            XCTAssertThrowsError(try owner.dispose(confirmed: false)) { assertCode($0, "program_attachment_busy") }
        }
        lock.lock(); defer { lock.unlock() }
        XCTAssertEqual(intent.expectedSequence, sequence)
        XCTAssertEqual(intent.sinkOutput, 501)
        XCTAssertNotEqual(intent.bindingEpoch, 0)
        if let epoch { XCTAssertEqual(intent.bindingEpoch, epoch) }
        switch intent.operation {
        case .setAll:
            let paint = try XCTUnwrap(intent.point)
            XCTAssertEqual(paint.slot, 91)
            XCTAssertEqual(paint.source.count, 3)
            XCTAssertEqual(paint.opacity, expectedOpacity)
        case .confirmExact:
            XCTAssertEqual(intent.desiredSequence, sequence)
            XCTAssertEqual(intent.point?.source, point)
            if let paint = intent.point {
                XCTAssertEqual(paint.slot, 91)
                XCTAssertEqual(paint.source.count, 3)
                XCTAssertEqual(paint.opacity, expectedOpacity)
            }
        case .revokeAll:
            XCTAssertNil(intent.point)
        }
        if callbackFailure { throw ProgramPointSinkCallbackError.InvocationFailed }
        if rejected { return false }
        if intent.operation != .confirmExact { XCTAssertEqual(intent.desiredSequence, sequence + 1) }
        epoch = intent.bindingEpoch
        sequence = intent.desiredSequence; point = intent.point?.source
        return true
    }
}

final class ProgramRuntimeTests: XCTestCase {
    func testSharedRuntimeHistoryAndOwnedSnapshots() throws {
        let vectors = try fixture()
        let runtime = try compileProgramWire(bytes: bytes(vectors.adaptive), streamId: 1)
        var held: [ProgramSnapshot] = []
        for step in vectors.history {
            let snapshot: ProgramSnapshot
            if let reason = step.unknownReason {
                snapshot = try runtime.updateUnknown(revision: step.revision, reasonId: reason)
            } else {
                let backgrounds = try XCTUnwrap(step.backgrounds)
                snapshot = try runtime.updateObserved(revision: step.revision,
                    scenarioIds: Array(1...UInt32(backgrounds.count)), surfaces: Data(backgrounds.flatMap { $0 }), surfaceCount: 1)
            }
            held.append(snapshot)
            XCTAssertEqual(snapshot.state(), step.state)
            XCTAssertEqual(snapshot.snapshotVersion(), 1)
            XCTAssertEqual(snapshot.revision(), step.revision)
            XCTAssertNil(snapshot.reason())
            if let rgb = step.rgb {
                XCTAssertEqual(snapshot.outputCount(), 1)
                XCTAssertEqual(try snapshot.outputSlot(index: 0), 91)
                XCTAssertEqual(try snapshot.outputRgb(index: 0), Data(rgb))
                XCTAssertEqual(try snapshot.outputOpacity(index: 0), 1)
            }
        }
        XCTAssertEqual(try held[0].outputRgb(index: 0), Data([0, 0, 0]))
        XCTAssertEqual(try held[0].presentationCompositeRgb(root: 71, scenario: 1), Data([0, 0, 0]))
        XCTAssertThrowsError(try held[0].presentationCompositeRgb(root: 91, scenario: 1)) { assertCode($0, "presentation_composite_unavailable") }
        var copied = try held[0].outputRgb(index: 0); copied[0] = 200
        XCTAssertEqual(try held[0].outputRgb(index: 0), Data([0, 0, 0]))
        XCTAssertThrowsError(try held[0].outputRgb(index: 1)) { assertCode($0, "invalid_output_index") }
        let fixed = try compileProgramWire(bytes: bytes(vectors.fixed), streamId: 2)
        XCTAssertEqual(try fixed.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1).outputRgb(index: 0), Data([20, 20, 20]))
    }

    func testComputedSnapshotsContainOnlyTheCurrentRevision() throws {
        let vectors = try fixture().computed
        let unsupported = try ProgramWireBuilderV3().computedTarget(21, domain: .gray(lower: 0, upper: 255), anchor: [0, 0, 0], maxDelta: 1)
            .finiteTarget(22, candidates: [ProgramCandidate(id: 1, rgb: [0, 0, 0], opacity: 1)]).assignment([21]).finish()
        XCTAssertThrowsError(try compileProgramWire(bytes: unsupported, streamId: 1)) { assertCode($0, "program_unsupported") }
        XCTAssertThrowsError(try attachProgramWire(bytes: unsupported, streamId: 1, outputSlot: 91,
            sinkOutput: 501, presentationRoot: 71, occurrence: 61, host: RuntimeHost())) { assertCode($0, "program_unsupported") }
        for (wire, history) in [(vectors.gray, vectors.grayHistory), (vectors.box, vectors.boxHistory), (vectors.relation, vectors.relationHistory)] {
            let runtime = try compileProgramWire(bytes: bytes(wire), streamId: .max)
            let waiting = try runtime.updateUnknown(revision: 0, reasonId: .max)
            XCTAssertEqual(waiting.snapshotVersion(), 2)
            XCTAssertEqual(waiting.state(), "waiting")
            XCTAssertEqual(waiting.revision(), 0)
            XCTAssertNil(waiting.reason())
            XCTAssertEqual(waiting.outputCount(), 0)
            var held: [ProgramSnapshot] = []
            for step in history {
                let snapshot: ProgramSnapshot
                if let reason = step.unknownReason {
                    snapshot = try runtime.updateUnknown(revision: step.revision, reasonId: reason)
                } else {
                    let backgrounds = try XCTUnwrap(step.backgrounds)
                    let surfaces = Data(backgrounds.flatMap { $0 })
                    snapshot = try runtime.updateObserved(revision: step.revision, scenarioIds: [1], surfaces: surfaces, surfaceCount: 1)
                    let replay = try runtime.updateObserved(revision: step.revision, scenarioIds: [1], surfaces: surfaces, surfaceCount: 1)
                    XCTAssertEqual(replay.state(), snapshot.state())
                    XCTAssertEqual(replay.reason(), snapshot.reason())
                    XCTAssertEqual(replay.outputCount(), snapshot.outputCount())
                }
                held.append(snapshot)
                XCTAssertEqual(snapshot.snapshotVersion(), 2)
                XCTAssertEqual(snapshot.revision(), step.revision)
                XCTAssertEqual(snapshot.state(), step.state)
                XCTAssertEqual(snapshot.reason(), step.reason)
                if let rgb = step.rgb {
                    XCTAssertEqual(snapshot.outputCount(), 1)
                    XCTAssertEqual(try snapshot.outputSlot(index: 0), 91)
                    XCTAssertEqual(try snapshot.outputRgb(index: 0), Data(rgb))
                    XCTAssertEqual(try snapshot.outputOpacity(index: 0), 0.5)
                } else {
                    XCTAssertEqual(snapshot.outputCount(), 0)
                    XCTAssertThrowsError(try snapshot.outputRgb(index: 0)) { assertCode($0, "invalid_output_index") }
                }
            }
            XCTAssertEqual(try held[0].outputRgb(index: 0), Data([64, 64, 64]))
            XCTAssertEqual(held[0].revision(), 1)
            XCTAssertThrowsError(try runtime.updateUnknown(revision: 1, reasonId: 7)) { assertCode($0, "program_update") }
            let current = try XCTUnwrap(held.last)
            let backgrounds = try XCTUnwrap(history.last?.backgrounds)
            let surfaces = Data(backgrounds.flatMap { $0 })
            var conflicting = surfaces; conflicting[0] ^= 1
            XCTAssertThrowsError(try runtime.updateObserved(revision: current.revision(), scenarioIds: [1], surfaces: conflicting, surfaceCount: 1)) { assertCode($0, "program_update") }
            let replay = try runtime.updateObserved(revision: current.revision(), scenarioIds: [1], surfaces: surfaces, surfaceCount: 1)
            XCTAssertEqual(replay.revision(), current.revision())
            XCTAssertEqual(replay.state(), current.state())
            XCTAssertEqual(try replay.outputRgb(index: 0), try current.outputRgb(index: 0))
            let high = try runtime.updateObserved(revision: .max, scenarioIds: [1], surfaces: Data([128, 128, 128]), surfaceCount: 1)
            XCTAssertEqual(high.revision(), .max)
            XCTAssertEqual(high.state(), "ready")
        }
    }

    func testComputedAttachmentRevokesBeforePublishingIncompleteOrUnsatisfied() throws {
        let vectors = try fixture().computed
        for (wire, history) in [(vectors.gray, vectors.grayHistory), (vectors.box, vectors.boxHistory), (vectors.relation, vectors.relationHistory)] {
            let host = RuntimeHost(opacity: 0.5)
            let attachment = try attachProgramWire(bytes: bytes(wire), streamId: 1, outputSlot: 91,
                sinkOutput: 501, presentationRoot: 71, occurrence: 61, host: host)
            host.configure(owner: attachment, reentrant: true)
            let first = try attachment.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([128, 128, 128]), surfaceCount: 1)
            XCTAssertEqual(first.snapshotVersion(), 2)
            XCTAssertEqual(first.revision(), 1)
            XCTAssertNil(first.reason())
            XCTAssertEqual(try attachment.materializationAuthority().terminalCompositeRgb(), Data([96, 96, 96]))
            host.reject(true)
            XCTAssertThrowsError(try attachment.updateObserved(revision: 2, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1)) { assertCode($0, "program_attachment_host_rejected") }
            XCTAssertEqual(try attachment.materializationAuthority().revision(), 1)
            XCTAssertEqual(host.installed(), Data([64, 64, 64]))
            host.reject(false)
            for step in history.dropFirst() {
                let snapshot: ProgramAttachedSnapshot
                if let reason = step.unknownReason {
                    snapshot = try attachment.updateUnknown(revision: step.revision, reasonId: reason)
                } else {
                    let surfaces = Data(try XCTUnwrap(step.backgrounds).flatMap { $0 })
                    snapshot = try attachment.updateObserved(revision: step.revision, scenarioIds: [1], surfaces: surfaces, surfaceCount: 1)
                    let replay = try attachment.updateObserved(revision: step.revision, scenarioIds: [1], surfaces: surfaces, surfaceCount: 1)
                    XCTAssertEqual(replay.state(), snapshot.state())
                }
                XCTAssertEqual(snapshot.snapshotVersion(), 2)
                XCTAssertEqual(snapshot.revision(), step.revision)
                XCTAssertEqual(snapshot.state(), step.state)
                XCTAssertEqual(snapshot.reason(), step.reason)
                XCTAssertEqual(snapshot.hasRender(), step.rgb != nil)
                if let rgb = step.rgb {
                    XCTAssertEqual(snapshot.outputCount(), 1)
                    XCTAssertEqual(try snapshot.outputRgb(index: 0), Data(rgb))
                    XCTAssertEqual(host.installed(), Data(rgb))
                } else {
                    XCTAssertEqual(snapshot.outputCount(), 0)
                    XCTAssertThrowsError(try snapshot.outputRgb(index: 0)) { assertCode($0, "invalid_output_index") }
                    XCTAssertNil(host.installed())
                    XCTAssertThrowsError(try attachment.materializationAuthority()) { assertCode($0, "program_materialization_not_ready") }
                }
            }
            XCTAssertEqual(try first.outputRgb(index: 0), Data([64, 64, 64]))
            XCTAssertThrowsError(try attachment.dispose(confirmed: false)) { assertCode($0, "program_attachment_revoke_unconfirmed") }
            host.revoke(); try attachment.dispose(confirmed: true)
            XCTAssertThrowsError(try attachment.updateUnknown(revision: 6, reasonId: 7)) { assertCode($0, "program_attachment_already_disposed") }
        }
    }

    func testRejectedInputsAndThreadMigration() throws {
        let runtime = try compileProgramWire(bytes: bytes(fixture().adaptive), streamId: 1)
        XCTAssertThrowsError(try runtime.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([255, 255]), surfaceCount: 1)) { assertCode($0, "program_update") }
        let first = try runtime.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1)
        XCTAssertEqual(try runtime.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1).state(), "ready")
        XCTAssertThrowsError(try runtime.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([0, 0, 0]), surfaceCount: 1)) { assertCode($0, "program_update") }
        let finished = expectation(description: "native runtime returns from another thread")
        Thread {
            defer { finished.fulfill() }
            do {
                let second = try runtime.updateObserved(revision: 2, scenarioIds: [1], surfaces: Data([0, 0, 0]), surfaceCount: 1)
                XCTAssertEqual(try second.outputRgb(index: 0), Data([255, 255, 255]))
            } catch { XCTFail("thread migration failed: \(error)") }
        }.start()
        wait(for: [finished], timeout: 10)
        XCTAssertEqual(try first.outputRgb(index: 0), Data([0, 0, 0]))
        XCTAssertThrowsError(try runtime.updateUnknown(revision: 1, reasonId: 7)) { assertCode($0, "program_update") }
    }

    func testFullBindingScopeKeepsAllPointsAndScenarioComposites() throws {
        final class ScopeHost: ProgramScopeSinkHost, @unchecked Sendable {
            private let lock = NSLock()
            private var commands: [ProgramScopeSinkIntent] = []
            func tryInstall(intent: ProgramScopeSinkIntent) throws -> Bool {
                lock.lock(); defer { lock.unlock() }
                commands.append(intent)
                return true
            }
            func saved() -> [ProgramScopeSinkIntent] {
                lock.lock(); defer { lock.unlock() }
                return commands
            }
        }
        let wire = try bytes(
            "4c435057010003010000020000000b0000004040400c000000a0a0a00200000015000000010b00000016000000010c000000" +
            "0000000000000000010000001f00000000000000020000002900000001150000002a00000001160000000100000033000000011f000000" +
            "020000003d000000290000003300000000000000000050409a9999999999c93f013e0000002a0000003300000000000000000050409a9999999999c93f01" +
            "02000000470000003d000000480000003e00000002000000470000003d000000480000003e0000000200000051000000013d00000040404052000000013e000000a0a0a000000000020000005b000000290000005c0000002a000000"
        )
        let host = ScopeHost()
        let attachment = try attachProgramWireWithBindings(bytes: wire, streamId: 17,
            bindings: ProgramAttachmentBindings(
                emissions: [ProgramPointEmissionBinding(slot: 91, sinkOutput: 501), ProgramPointEmissionBinding(slot: 92, sinkOutput: 502)],
                presentations: [ProgramPointPresentationBinding(slot: 91, root: 71, occurrence: 61), ProgramPointPresentationBinding(slot: 92, root: 72, occurrence: 62)]),
            host: host)
        let ready = try attachment.updateObserved(revision: 1, scenarioIds: [101, 301],
            surfaces: Data([128, 128, 128, 128, 128, 128]), surfaceCount: 1)
        XCTAssertEqual(ready.renderCount(), 2)
        XCTAssertTrue(ready.hasRender())
        XCTAssertNil(ready.render())
        XCTAssertEqual(try ready.renderAt(index: 0).outputRgb(), Data([64, 64, 64]))
        XCTAssertEqual(try ready.renderAt(index: 1).outputRgb(), Data([160, 160, 160]))
        let installed = try XCTUnwrap(host.saved().last)
        XCTAssertEqual(installed.operation, .setAll)
        XCTAssertEqual(installed.scope, [501, 502])
        XCTAssertNil(installed.sinkOutput)
        XCTAssertNil(installed.point)
        XCTAssertEqual(installed.points.map { $0.sinkOutput }, [501, 502])
        XCTAssertEqual(installed.points.map { $0.paint.slot }, [91, 92])
        XCTAssertEqual(installed.points[0].paint.source, Data([64, 64, 64]))
        XCTAssertEqual(installed.points[1].paint.source, Data([160, 160, 160]))
        XCTAssertEqual(installed.points.map { $0.paint.opacity }, [1, 1])
        XCTAssertEqual(installed.presentationComposites.count, 4)
        XCTAssertEqual(Set(installed.presentationComposites.map { "\($0.root):\($0.scenario)" }),
            Set(["71:101", "71:301", "72:101", "72:301"]))
        for composite in installed.presentationComposites {
            XCTAssertEqual(composite.source, composite.root == 71 ? Data([64, 64, 64]) : Data([160, 160, 160]))
        }
        XCTAssertThrowsError(try attachment.materializationAuthority()) { assertCode($0, "program_materialization_ambiguous_bindings") }
        let second = try ready.renderAt(index: 1)
        XCTAssertThrowsError(try attachment.materializationAuthorityFor(revision: 1,
            contentIdentity: second.contentIdentity(), bindingEpoch: second.bindingEpoch())) { assertCode($0, "program_materialization_ambiguous_bindings") }
        let authority = try attachment.materializationAuthorityForBinding(root: 72, occurrence: 62)
        XCTAssertEqual(authority.outputSlot(), 92)
        XCTAssertEqual(authority.outputRgb(), Data([160, 160, 160]))

        let stale = try attachment.updateUnknown(revision: 2, reasonId: 41)
        XCTAssertEqual(stale.renderCount(), 0)
        let revoked = try XCTUnwrap(host.saved().last)
        XCTAssertEqual(revoked.operation, .revokeAll)
        XCTAssertEqual(revoked.scope, [501, 502])
        XCTAssertNil(revoked.sinkOutput)
        XCTAssertNil(revoked.point)
        XCTAssertTrue(revoked.points.isEmpty)
        XCTAssertTrue(revoked.presentationComposites.isEmpty)
        XCTAssertEqual(ready.renderCount(), 2)
        XCTAssertEqual(installed.points[0].paint.source, Data([64, 64, 64]))
        XCTAssertEqual(installed.presentationComposites.count, 4)
        XCTAssertThrowsError(try attachment.materializationAuthorityForBinding(root: 72, occurrence: 62)) { assertCode($0, "program_materialization_not_ready") }
        try attachment.dispose(confirmed: true)
    }

    func testAttachmentUsesOneCommittedHeadAndRequiresExplicitRevoke() throws {
        let host = RuntimeHost()
        let attachment = try attachProgramWire(bytes: bytes(fixture().adaptive), streamId: 1,
            outputSlot: 91, sinkOutput: 501, presentationRoot: 71, occurrence: 61, host: host)
        host.configure(owner: attachment, reentrant: true)
        let first = try attachment.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1)
        XCTAssertTrue(first.hasRender())
        let render = try XCTUnwrap(first.render())
        let authority = try attachment.materializationAuthority()
        XCTAssertEqual(render.sinkOutput(), 501)
        XCTAssertEqual(try authority.rendererProvenance(), "unverified")
        XCTAssertEqual(try authority.physicalIdentity(), "encoded-srgb8-source-over-v1")
        XCTAssertEqual(authority.terminalCompositeRgb(), Data([0, 0, 0]))
        XCTAssertEqual(authority.contentIdentity(), render.contentIdentity())
        XCTAssertEqual(authority.contextIdentity(), render.contextIdentity())
        host.fail(true)
        XCTAssertThrowsError(try attachment.updateObserved(revision: 2, scenarioIds: [1], surfaces: Data([0, 0, 0]), surfaceCount: 1)) { assertCode($0, "program_attachment_host_protocol") }
        XCTAssertEqual(try attachment.materializationAuthority().revision(), 1)
        XCTAssertEqual(host.installed(), Data([0, 0, 0]))
        host.fail(false)
        host.reject(true)
        XCTAssertThrowsError(try attachment.updateObserved(revision: 2, scenarioIds: [1], surfaces: Data([0, 0, 0]), surfaceCount: 1)) { assertCode($0, "program_attachment_host_rejected") }
        XCTAssertEqual(try attachment.materializationAuthority().revision(), 1)
        XCTAssertEqual(host.installed(), Data([0, 0, 0]))
        host.reject(false)
        XCTAssertEqual(try attachment.updateObserved(revision: 2, scenarioIds: [1], surfaces: Data([0, 0, 0]), surfaceCount: 1).outputRgb(index: 0), Data([255, 255, 255]))
        XCTAssertThrowsError(try attachment.materializationAuthorityFor(revision: render.revision(), contentIdentity: render.contentIdentity(), bindingEpoch: render.bindingEpoch())) { assertCode($0, "program_materialization_stale_revision") }
        XCTAssertEqual(authority.terminalCompositeRgb(), Data([0, 0, 0]))
        XCTAssertThrowsError(try attachment.dispose(confirmed: false)) { assertCode($0, "program_attachment_revoke_unconfirmed") }
        XCTAssertEqual(try attachment.materializationAuthority().revision(), 2)
        let current = try attachment.materializationAuthority()
        XCTAssertEqual(try attachment.materializationAuthorityFor(revision: current.revision(),
            contentIdentity: current.contentIdentity(), bindingEpoch: current.bindingEpoch()).terminalCompositeRgb(), Data([255, 255, 255]))
        var foreign = current.contentIdentity(); foreign[0] ^= 1
        XCTAssertThrowsError(try attachment.materializationAuthorityFor(revision: 2,
            contentIdentity: foreign, bindingEpoch: current.bindingEpoch())) { assertCode($0, "program_materialization_stale_identity") }
        XCTAssertThrowsError(try attachment.materializationAuthorityFor(revision: 2,
            contentIdentity: current.contentIdentity(), bindingEpoch: current.bindingEpoch() ^ 1)) { assertCode($0, "program_materialization_foreign_binding_epoch") }
        let unknown = try attachment.updateUnknown(revision: 3, reasonId: 7)
        XCTAssertEqual(unknown.state(), "stale")
        XCTAssertFalse(unknown.hasRender())
        XCTAssertNil(host.installed())
        XCTAssertThrowsError(try attachment.materializationAuthority()) { assertCode($0, "program_materialization_not_ready") }
        XCTAssertEqual(try attachment.updateObserved(revision: 4, scenarioIds: [1],
            surfaces: Data([255, 255, 255]), surfaceCount: 1).state(), "ready")
        let conflict = try attachment.updateObserved(revision: 5, scenarioIds: [1, 2],
            surfaces: Data([255, 255, 255, 0, 0, 0]), surfaceCount: 1)
        XCTAssertEqual(conflict.state(), "failed")
        XCTAssertFalse(conflict.hasRender())
        XCTAssertNil(host.installed())
        XCTAssertEqual(try attachment.updateObserved(revision: 6, scenarioIds: [1],
            surfaces: Data([0, 0, 0]), surfaceCount: 1).state(), "ready")
        host.revoke(); try attachment.dispose(confirmed: true)
        XCTAssertThrowsError(try attachment.updateUnknown(revision: 7, reasonId: 7)) { assertCode($0, "program_attachment_already_disposed") }
        XCTAssertThrowsError(try attachment.dispose(confirmed: true)) { assertCode($0, "program_attachment_already_disposed") }
    }

    func testNativeMetadataAndSourceCertificate() throws {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "manifest", withExtension: "json", subdirectory: "Fixtures"))
        let expected = try JSONDecoder().decode(Manifest.self, from: Data(contentsOf: url)).numericalCapabilities
        let got = numericalCapabilityManifest()
        let fullPack = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
        let fullExpected = try XCTUnwrap(fullPack["numericalCapabilities"] as? NSDictionary)
        let fullActual = try XCTUnwrap(reflectedJSON(got) as? NSDictionary)
        XCTAssertEqual(fullActual, fullExpected)
        XCTAssertEqual(got.schemaVersion, expected.schemaVersion)
        XCTAssertEqual(got.checksum, expected.checksum)
        XCTAssertEqual(got.sites.map { $0.siteId }, expected.sites.map { $0.siteId })
        let envelope = try issueSourceCertificateEnvelope()
        let metadata = try decodeCertificateEnvelope(bytes: envelope)
        let identityUrl = try XCTUnwrap(Bundle.module.url(forResource: "build-identity", withExtension: "json", subdirectory: "Fixtures"))
        let identity = try JSONDecoder().decode([String: String].self, from: Data(contentsOf: identityUrl))
        XCTAssertEqual(metadata.producerRevision, try XCTUnwrap(identity["coreTree"]))
        XCTAssertEqual(metadata.producerContentIdentity.count, 32)
        XCTAssertEqual(metadata.payloadLength, 1)
        XCTAssertEqual(metadata.payloadSha256.count, 32)
        XCTAssertEqual(metadata.bindingSha256.count, 32)
        XCTAssertThrowsError(try decodeCertificateEnvelope(bytes: Data(repeating: 0, count: 20))) { assertCode($0, "certificate_invalid_magic") }
    }

    private func assessmentWire(source: [UInt8], opacity: Double, visible: [UInt8]) throws -> Data {
        try ProgramWireBuilderV1().source(11, rgb: source).fixedTarget(21, source: 11)
            .surfaceInputPort(31).opacityInput(32, value: opacity).solidPaint(42, target: 21)
            .opacityPaint(41, source: 42, opacity: 32).inputSurface(51, input: 31)
            .sourceOverOccurrence(61, subject: 41, against: 51, adaptingLuminance: 64,
                backgroundRatio: 0.2, surround: .average)
            .presentationRoot(71, terminal: 61).presentationTarget(71, occurrence: 61)
            .exactVisibleUnary(true, id: 82, occurrence: 61, expectedRgb: visible)
            .exactVisibleUnary(false, id: 83, occurrence: 61, expectedRgb: [255, 255, 255])
            .declaredSrgb8CleanSet(false, id: 81, root: 71, occurrence: 61).output(91, paint: 41).finish()
    }

    func testNativeAssessmentReportPreservesCellsAndDuplicateProvenance() throws {
        let wire = try assessmentWire(source: [0, 199, 64], opacity: 1, visible: [0, 199, 64])
        var held: ProgramAssessmentReport?
        let orders: [[UInt32]] = [[9, 7], [7, 9]]
        for ids in orders {
            let runtime = try compileProgramWire(bytes: wire, streamId: UInt32.max)
            let snapshot = try runtime.updateObserved(revision: UInt64.max, scenarioIds: ids,
                surfaces: Data(repeating: 0, count: 6), surfaceCount: 1)
            XCTAssertEqual(snapshot.state(), "ready")
            XCTAssertEqual(try snapshot.outputRgb(index: 0), Data([0, 199, 64]))
            let report = try XCTUnwrap(snapshot.assessmentReport())
            XCTAssertEqual(report.contentIdentity.count, 32)
            XCTAssertEqual(report.streamId, UInt32.max)
            XCTAssertEqual(report.revision, UInt64.max)
            XCTAssertEqual(report.scenarioProvenance.map { $0.caseIndex }, [0, 0])
            XCTAssertEqual(report.scenarioProvenance.map { $0.scenarioId }, [7, 9])
            XCTAssertEqual(report.cells.map { $0.constraintId }, [81, 82, 83])
            XCTAssertEqual(report.cells.map { $0.caseIndex }, [0, 0, 0])
            XCTAssertEqual(report.cells.map { $0.hard }, [false, true, false])
            guard case let .declaredSrgb8CleanSetRejected(visible, interval) = report.cells[0].assessment else {
                return XCTFail("report must retain nominal clean-set rejection")
            }
            XCTAssertEqual([visible.red, visible.green, visible.blue], [0, 199, 64])
            XCTAssertEqual([interval.lower, interval.upper], [64, 108])
            XCTAssertEqual(report.cells[1].assessment, .exactSrgb8(verdict: .pass))
            // Итоговый [0,199,64] нарушает report-only равенство белому.
            XCTAssertEqual(report.cells[2].assessment, .exactSrgb8(verdict: .violation))
            XCTAssertEqual(try snapshot.outputSlot(index: 0), 91)
            if let held { XCTAssertEqual(report, held) }
            held = report
        }
    }

    func testNativeFinalAlphaReportIsOwnedAndNonReadyNeverHasReport() throws {
        let wire = try assessmentWire(source: [128, 128, 129], opacity: 0.5, visible: [128, 128, 128])
        let runtime = try compileProgramWire(bytes: wire, streamId: 100)
        let waiting = try runtime.updateUnknown(revision: 1, reasonId: 99)
        XCTAssertEqual(waiting.state(), "waiting")
        XCTAssertNil(try waiting.assessmentReport())
        let first = try runtime.updateObserved(revision: 2, scenarioIds: [7], surfaces: Data([128, 128, 127]), surfaceCount: 1)
        XCTAssertEqual(first.state(), "ready")
        let report = try XCTUnwrap(first.assessmentReport())
        // Encoded-sRGB midpoint: (129 + 127)/2 = 128; исходный blue 129 сохраняется отдельно.
        guard case let .declaredSrgb8CleanSetPass(visible) = report.cells[0].assessment else {
            return XCTFail("final neutral gray must pass")
        }
        XCTAssertEqual([visible.red, visible.green, visible.blue], [128, 128, 128])
        XCTAssertEqual(try first.outputRgb(index: 0), Data([128, 128, 129]))
        XCTAssertEqual(try first.outputOpacity(index: 0), 0.5)
        let stale = try runtime.updateUnknown(revision: 3, reasonId: 99)
        XCTAssertEqual(stale.state(), "stale")
        XCTAssertNil(try stale.assessmentReport())
        XCTAssertEqual(try stale.outputRgb(index: 0), try first.outputRgb(index: 0))
        let fresh = try runtime.updateObserved(revision: 4, scenarioIds: [7], surfaces: Data([128, 128, 127]), surfaceCount: 1)
        XCTAssertEqual(try XCTUnwrap(fresh.assessmentReport()).revision, 4)
        let failed = try runtime.updateObserved(revision: 5, scenarioIds: [7], surfaces: Data([0, 0, 0]), surfaceCount: 1)
        XCTAssertEqual(failed.state(), "failed")
        XCTAssertNil(try failed.assessmentReport())
        XCTAssertEqual(try failed.outputRgb(index: 0), try first.outputRgb(index: 0))
        var copied = try XCTUnwrap(first.assessmentReport())
        copied.contentIdentity[0] ^= 1
        copied.scenarioProvenance.removeAll()
        copied.cells.removeAll()
        XCTAssertNotEqual(copied, report)
        XCTAssertEqual(try XCTUnwrap(first.assessmentReport()), report)
        let absent = try compileProgramWire(bytes: assessmentWire(source: [0, 0, 0], opacity: 1, visible: [0, 0, 0]), streamId: 101)
            .updateObserved(revision: 1, scenarioIds: [7], surfaces: Data([0, 0, 0]), surfaceCount: 1)
        XCTAssertEqual(absent.state(), "ready")
        let absentReport = try XCTUnwrap(absent.assessmentReport())
        XCTAssertEqual(absentReport.cells.count, 3)
        XCTAssertEqual(absentReport.cells[0].assessment, .declaredSrgb8CleanSetFinalOwnedDomainAbsent)
        XCTAssertNotEqual(absentReport.contentIdentity, report.contentIdentity)
    }

    func testNativeAttachedReportReadDoesNotInstallOrReissueAuthority() throws {
        let host = RuntimeHost()
        let attachment = try attachProgramWire(bytes: bytes(fixture().adaptive), streamId: 103,
            outputSlot: 91, sinkOutput: 501, presentationRoot: 71, occurrence: 61, host: host)
        let first = try attachment.updateObserved(revision: 1, scenarioIds: [7], surfaces: Data([255, 255, 255]), surfaceCount: 1)
        XCTAssertEqual(first.state(), "ready")
        let render = try XCTUnwrap(first.render())
        let before = host.callCount()
        let report = try XCTUnwrap(first.assessmentReport())
        XCTAssertEqual(report.contentIdentity, render.contentIdentity())
        XCTAssertEqual(report.streamId, 103)
        XCTAssertEqual(report.revision, render.revision())
        XCTAssertEqual(report.cells.count, 1)
        XCTAssertEqual(report.cells[0].constraintId, 81)
        XCTAssertTrue(report.cells[0].hard)
        XCTAssertEqual(report.cells[0].assessment, .wcag22Srgb8(verdict: .pass))
        XCTAssertEqual(host.callCount(), before)
        host.reject(true)
        XCTAssertThrowsError(try attachment.updateObserved(revision: 2, scenarioIds: [7], surfaces: Data([0, 0, 0]), surfaceCount: 1)) {
            assertCode($0, "program_attachment_host_rejected")
        }
        XCTAssertEqual(try XCTUnwrap(first.assessmentReport()), report)
        XCTAssertEqual(try attachment.materializationAuthority().revision(), 1)
        host.reject(false)
        let revoked = try attachment.updateUnknown(revision: 2, reasonId: 99)
        XCTAssertEqual(revoked.state(), "stale")
        XCTAssertNil(try revoked.assessmentReport())
        XCTAssertFalse(revoked.hasRender())
        XCTAssertNil(host.installed())
        XCTAssertThrowsError(try attachment.materializationAuthorityFor(revision: report.revision,
            contentIdentity: report.contentIdentity, bindingEpoch: render.bindingEpoch())) {
            assertCode($0, "program_materialization_not_ready")
        }
        let afterRevoke = host.callCount()
        var copied = try XCTUnwrap(first.assessmentReport())
        copied.contentIdentity.removeAll(); copied.cells.removeAll(); copied.scenarioProvenance.removeAll()
        XCTAssertNotEqual(copied, report)
        XCTAssertEqual(try XCTUnwrap(first.assessmentReport()), report)
        XCTAssertEqual(host.callCount(), afterRevoke)
        let failed = try attachment.updateObserved(revision: 3, scenarioIds: [7, 9], surfaces: Data([0, 0, 0, 255, 255, 255]), surfaceCount: 1)
        XCTAssertEqual(failed.state(), "failed")
        XCTAssertNil(try failed.assessmentReport())
        XCTAssertFalse(failed.hasRender())
        XCTAssertEqual(try XCTUnwrap(first.assessmentReport()), report)
        try attachment.dispose(confirmed: true)
    }
}


// Callback хранит owned value; его копия не становится текущим допуском Core.
private final class RasterConsumer: ProgramFinalRasterConsumer, @unchecked Sendable {
    private let lock = NSLock()
    private var received: [ProgramFinalRasterBytes] = []
    private var rejected = false
    private var failure = false
    private weak var owner: ProgramRuntime?
    func configure(owner: ProgramRuntime?, rejected: Bool = false, failure: Bool = false) {
        lock.lock(); defer { lock.unlock() }
        self.owner = owner; self.rejected = rejected; self.failure = failure
    }
    func frames() -> [ProgramFinalRasterBytes] { lock.lock(); defer { lock.unlock() }; return received }
    func consume(raster: ProgramFinalRasterBytes) throws -> Bool {
        lock.lock(); let owner = self.owner; let rejected = self.rejected; let failure = self.failure; lock.unlock()
        if let owner {
            XCTAssertThrowsError(try owner.updateUnknown(revision: 99, reasonId: 7)) { assertCode($0, "program_attachment_busy") }
            XCTAssertThrowsError(try owner.finalRasterContentIdentity()) { assertCode($0, "program_attachment_busy") }
            XCTAssertThrowsError(try owner.finalRasterRefusal()) { assertCode($0, "program_attachment_busy") }
            XCTAssertThrowsError(try owner.consumeFinalRaster(revision: raster.revision, content: Data(repeating: 0, count: 32),
                output: raster.outputSlot, context: Data(repeating: 7, count: 32), consumer: self)) { assertCode($0, "program_attachment_busy") }
        }
        if failure { throw ProgramFinalRasterCallbackError.InvocationFailed }
        if rejected { return false }
        lock.lock(); received.append(raster); lock.unlock()
        return true
    }
}

final class FinalRasterRuntimeTests: XCTestCase {
    private let context = Data(repeating: 7, count: 32)
    private func wire(_ requirement: ProgramFinalRasterCleanlinessRequirement) -> Data {
        ProgramWireBuilderV4(sourcePort: 1, destinationPort: 2, operatorId: 3,
            outputSlot: 17, cleanliness: requirement).finish()
    }
    private func update(_ runtime: ProgramRuntime, _ revision: UInt64, _ source: Data, _ destination: Data,
                        width: UInt32 = 2, height: UInt32 = 2, dpr: UInt8 = 2, context: Data? = nil) throws -> ProgramSnapshot {
        try runtime.updateFinalRaster(revision: revision, width: width, height: height, dpr: dpr,
            context: context ?? self.context, sourcePort: 1, source: source, destinationPort: 2, destination: destination)
    }
    private func consume(_ runtime: ProgramRuntime, _ revision: UInt64, _ consumer: RasterConsumer) throws {
        try runtime.consumeFinalRaster(revision: revision, content: runtime.finalRasterContentIdentity(),
            output: 17, context: context, consumer: consumer)
    }

    func testEveryPolicyUsesCompleteFinalPixelsAndIndependentWire() throws {
        let clean = Data(Array(repeating: [UInt8(0), 199, 63, 255], count: 4).flatMap { $0 })
        var dirty = clean; dirty[14] = 64
        let background = Data(Array(repeating: [UInt8(0), 0, 0, 255], count: 4).flatMap { $0 })
        let policies: [(ProgramFinalRasterCleanlinessRequirement, UInt8)] = [
            (.off, 0), (.lintExactNominalSrgb8V2, 2), (.mandatoryExactNominalSrgb8V2, 1)]
        var identities: [Data] = []
        for (policy, tag) in policies {
            var expected = try bytes("4c435057040028000000010001000000000000000200000000000000030000000000000011000000")
            expected[11] = tag
            XCTAssertEqual(wire(policy), expected)
            let runtime = try compileProgramWire(bytes: wire(policy), streamId: .max)
            let identity = try runtime.finalRasterContentIdentity()
            XCTAssertEqual(identity.count, 32); XCTAssertFalse(identities.contains(identity)); identities.append(identity)
            let consumer = RasterConsumer()
            let first = try update(runtime, 1, clean, background)
            XCTAssertEqual(first.state(), "ready"); XCTAssertNil(try first.assessmentReport())
            try consume(runtime, 1, consumer)
            let saved = try XCTUnwrap(consumer.frames().last)
            XCTAssertEqual(saved.scope, "ModeledFinalRasterV1")
            XCTAssertEqual(saved.width, 2); XCTAssertEqual(saved.height, 2); XCTAssertEqual(saved.devicePixelRatio, 2)
            XCTAssertEqual(saved.revision, 1); XCTAssertEqual(saved.outputSlot, 17)
            XCTAssertEqual(saved.pixels, Data(Array(repeating: [UInt8(0), 199, 63], count: 4).flatMap { $0 }))
            XCTAssertEqual(saved.cleanliness.requirement, policy)
            XCTAssertEqual(saved.cleanliness.verdict, policy == .off ? .notApplied : .pass)
            XCTAssertEqual(saved.cleanliness.checkedPixelCount, policy == .off ? 0 : 4)
            XCTAssertEqual(saved.cleanliness.violationCount, 0); XCTAssertNil(saved.cleanliness.firstViolation)
            XCTAssertEqual(saved.cleanliness.profile, policy == .off ? nil : "ExactNominalSrgb8CleanSetV2")
            XCTAssertEqual(saved.cleanliness.profileRelease?.count, policy == .off ? nil : 32)

            let second = try update(runtime, 2, dirty, background)
            XCTAssertNil(try second.assessmentReport())
            if policy == .mandatoryExactNominalSrgb8V2 {
                XCTAssertEqual(second.state(), "failed")
                let refusal = try XCTUnwrap(runtime.finalRasterRefusal())
                XCTAssertEqual(refusal.code, "program_field_convention"); XCTAssertEqual(refusal.pixelIndex, 3)
                let pixel = try XCTUnwrap(refusal.composite); let interval = try XCTUnwrap(refusal.rejectedBlueInterval)
                XCTAssertEqual([pixel.red, pixel.green, pixel.blue], [0, 199, 64])
                XCTAssertEqual([interval.lower, interval.upper], [64, 108])
                XCTAssertThrowsError(try consume(runtime, 2, consumer)) { assertCode($0, "program_field_stale") }
                XCTAssertEqual(consumer.frames().count, 1)
            } else {
                XCTAssertEqual(second.state(), "ready"); XCTAssertNil(try runtime.finalRasterRefusal())
                try consume(runtime, 2, consumer)
                let frame = try XCTUnwrap(consumer.frames().last)
                XCTAssertEqual(frame.pixels, Data([0, 199, 63, 0, 199, 63, 0, 199, 63, 0, 199, 64]))
                XCTAssertEqual(frame.cleanliness.verdict, policy == .off ? .notApplied : .violation)
                XCTAssertEqual(frame.cleanliness.checkedPixelCount, policy == .off ? 0 : 4)
                XCTAssertEqual(frame.cleanliness.violationCount, policy == .off ? 0 : 1)
                if policy == .off {
                    XCTAssertNil(frame.cleanliness.firstViolation); XCTAssertNil(frame.cleanliness.profileRelease)
                } else {
                    let witness = try XCTUnwrap(frame.cleanliness.firstViolation)
                    XCTAssertEqual(witness.pixelIndex, 3)
                    XCTAssertEqual([witness.composite.red, witness.composite.green, witness.composite.blue], [0, 199, 64])
                    XCTAssertEqual([witness.rejectedBlueInterval.lower, witness.rejectedBlueInterval.upper], [64, 108])
                }
            }
            XCTAssertEqual(try update(runtime, 3, clean, background).state(), "ready")
            try consume(runtime, 3, consumer)
            var copied = saved; copied.pixels[0] = 255; copied.cleanliness.profileRelease?.removeAll()
            XCTAssertEqual(consumer.frames().first?.pixels, saved.pixels)
            XCTAssertEqual(consumer.frames().last?.pixels, saved.pixels)
            XCTAssertEqual(consumer.frames().last?.cleanliness.profileRelease, saved.cleanliness.profileRelease)
            if policy == .lintExactNominalSrgb8V2 {
                var multiple = dirty; multiple[6] = 64
                XCTAssertEqual(try update(runtime, 4, multiple, background).state(), "ready")
                try consume(runtime, 4, consumer)
                let report = try XCTUnwrap(consumer.frames().last).cleanliness
                XCTAssertEqual(report.checkedPixelCount, 4); XCTAssertEqual(report.violationCount, 2)
                XCTAssertEqual(report.firstViolation?.pixelIndex, 1)
            }
        }
    }

    func testLockedAlphaChangedBackgroundStaleContextAndRecovery() throws {
        // Половинный foreground: 100 + round(199*127/255)=199; 32 + round(62*127/255)=63.
        let source = Data([0, 100, 32, 128])
        let clean = Data([0, 199, 62, 255]); let dirty = Data([0, 199, 64, 255])
        let policies: [ProgramFinalRasterCleanlinessRequirement] = [.lintExactNominalSrgb8V2, .mandatoryExactNominalSrgb8V2]
        for policy in policies {
            let runtime = try compileProgramWire(bytes: wire(policy), streamId: 1)
            let consumer = RasterConsumer()
            XCTAssertEqual(try update(runtime, 1, source, clean, width: 1, height: 1).state(), "ready")
            try consume(runtime, 1, consumer)
            let saved = try XCTUnwrap(consumer.frames().last)
            XCTAssertEqual(saved.pixels, Data([0, 199, 63])); XCTAssertEqual(saved.cleanliness.verdict, .pass)
            let changed = try update(runtime, 2, source, dirty, width: 1, height: 1)
            XCTAssertEqual(changed.state(), policy == .mandatoryExactNominalSrgb8V2 ? "failed" : "ready")
            XCTAssertThrowsError(try consume(runtime, 1, consumer)) { assertCode($0, "program_field_stale") }
            if policy == .lintExactNominalSrgb8V2 {
                try consume(runtime, 2, consumer)
                XCTAssertEqual(consumer.frames().last?.pixels, Data([0, 199, 64]))
                XCTAssertEqual(consumer.frames().last?.cleanliness.verdict, .violation)
            }
            XCTAssertEqual(try runtime.updateUnknown(revision: 3, reasonId: 55).state(), "stale")
            XCTAssertThrowsError(try consume(runtime, 2, consumer)) { assertCode($0, "program_field_stale") }
            let newContext = Data(repeating: 8, count: 32)
            XCTAssertEqual(try update(runtime, 4, source, clean, width: 1, height: 1, context: newContext).state(), "ready")
            let count = consumer.frames().count
            XCTAssertThrowsError(try consume(runtime, 4, consumer)) { assertCode($0, "program_field_context") }
            XCTAssertEqual(consumer.frames().count, count)
            try runtime.consumeFinalRaster(revision: 4, content: runtime.finalRasterContentIdentity(),
                output: 17, context: newContext, consumer: consumer)
            XCTAssertEqual(consumer.frames().last?.pixels, saved.pixels)
            XCTAssertEqual(saved.revision, 1); XCTAssertEqual(saved.cleanliness.verdict, .pass)
            XCTAssertEqual(source, Data([0, 100, 32, 128])); XCTAssertEqual(dirty, Data([0, 199, 64, 255]))
        }
    }

    func testAdmissionBindingsCallbackFailuresAndReentryPreserveCurrentFrame() throws {
        let runtime = try compileProgramWire(bytes: wire(.lintExactNominalSrgb8V2), streamId: 1)
        let source = Data(repeating: 0, count: 16)
        let background = Data(Array(repeating: [UInt8(64), 64, 64, 255], count: 4).flatMap { $0 })
        let consumer = RasterConsumer()
        XCTAssertEqual(try update(runtime, 1, source, background).state(), "ready")
        XCTAssertEqual(try update(runtime, 1, source, background).state(), "ready")
        XCTAssertThrowsError(try update(runtime, 2, Data([0]), background)) { assertCode($0, "program_field_length") }
        XCTAssertThrowsError(try update(runtime, 2, source, background, dpr: 5)) { assertCode($0, "program_field_dpr") }
        XCTAssertThrowsError(try update(runtime, 2, source, background, width: 65537, height: 1)) { assertCode($0, "program_resource_exhausted") }
        var altered = background; altered[12] = 65
        XCTAssertThrowsError(try update(runtime, 1, source, altered)) { assertCode($0, "program_field_update") }
        let identity = try runtime.finalRasterContentIdentity()
        for (content, output, context, code) in [
            (Data(repeating: 0, count: 32), UInt32(17), self.context, "program_field_program"),
            (identity, UInt32(18), self.context, "program_field_output"),
            (identity, UInt32(17), Data(repeating: 8, count: 32), "program_field_context")] {
            XCTAssertThrowsError(try runtime.consumeFinalRaster(revision: 1, content: content,
                output: output, context: context, consumer: consumer)) { assertCode($0, code) }
        }
        XCTAssertTrue(consumer.frames().isEmpty)
        consumer.configure(owner: runtime)
        try consume(runtime, 1, consumer)
        consumer.configure(owner: nil, rejected: true)
        XCTAssertThrowsError(try consume(runtime, 1, consumer)) { assertCode($0, "program_field_consumer_rejected") }
        consumer.configure(owner: nil, failure: true)
        XCTAssertThrowsError(try consume(runtime, 1, consumer)) { assertCode($0, "program_field_consumer_failed") }
        consumer.configure(owner: nil)
        try consume(runtime, 1, consumer)
        XCTAssertEqual(consumer.frames().count, 2)
        XCTAssertEqual(consumer.frames()[0].pixels, consumer.frames()[1].pixels)
        let transparent = Data(Array(repeating: [UInt8(64), 64, 64, 128], count: 4).flatMap { $0 })
        XCTAssertEqual(try update(runtime, 2, source, transparent).state(), "failed")
        let refusal = try XCTUnwrap(runtime.finalRasterRefusal())
        XCTAssertEqual(refusal.code, "program_field_transparent"); XCTAssertEqual(refusal.pixelIndex, 0)
        XCTAssertNil(refusal.composite); XCTAssertNil(refusal.rejectedBlueInterval)
        XCTAssertThrowsError(try consume(runtime, 2, consumer)) { assertCode($0, "program_field_stale") }
        XCTAssertEqual(consumer.frames().count, 2)
        XCTAssertEqual(try update(runtime, 3, source, background).state(), "ready")
        try consume(runtime, 3, consumer)
        XCTAssertEqual(consumer.frames().last?.pixels, consumer.frames().first?.pixels)
    }
}
