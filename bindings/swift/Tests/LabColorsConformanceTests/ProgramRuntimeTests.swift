import Foundation
import XCTest
import LabColors

struct RuntimeStep: Decodable {
    let revision: UInt64
    let backgrounds: [[UInt8]]?
    let unknownReason: UInt32?
    let state: String
    let rgb: [UInt8]?
}
struct OutputTableRow: Decodable {
    let slot: UInt32
    let rgb: [UInt8]
    let opacity: Double
    let wire: String
    var table: [Double] { [Double(slot)] + rgb.map { Double($0) } + [opacity] }
}
struct OutputTableFixture: Decodable {
    let wire: String
    let rows: [OutputTableRow]
}
struct RuntimeFixture: Decodable {
    let schemaVersion: Int
    let fixed: String
    let adaptive: String
    let encodingOnly: String
    let history: [RuntimeStep]
    let outputTable: OutputTableFixture
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
private func assertOutputTable(_ table: [Double], _ rows: [OutputTableRow], file: StaticString = #filePath, line: UInt = #line) throws {
    XCTAssertEqual(table.count, rows.count * 5, file: file, line: line)
    guard table.count == rows.count * 5 else { return }
    var remaining = Dictionary(uniqueKeysWithValues: rows.map { ($0.slot, $0) })
    for offset in stride(from: 0, to: table.count, by: 5) {
        let slot = try XCTUnwrap(UInt32(exactly: table[offset]), file: file, line: line)
        let row = try XCTUnwrap(remaining.removeValue(forKey: slot), file: file, line: line)
        XCTAssertEqual(Array(table[offset..<offset + 5]), row.table, file: file, line: line)
        XCTAssertEqual(table[offset + 4].bitPattern, row.opacity.bitPattern, file: file, line: line)
    }
    XCTAssertTrue(remaining.isEmpty, file: file, line: line)
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
    private let outputSlot: UInt32
    private let outputOpacity: Double
    private var rejected = false
    private var callbackFailure = false
    private var reentrant = false
    private var point: Data?
    private var sequence: UInt64 = 0
    private var epoch: UInt64?
    private weak var owner: ProgramAttachment?
    init(outputSlot: UInt32 = 91, outputOpacity: Double = 1) {
        self.outputSlot = outputSlot; self.outputOpacity = outputOpacity
    }
    func configure(owner: ProgramAttachment, reentrant: Bool) { lock.lock(); defer { lock.unlock() }; self.owner = owner; self.reentrant = reentrant }
    func reject(_ value: Bool) { lock.lock(); defer { lock.unlock() }; rejected = value }
    func fail(_ value: Bool) { lock.lock(); defer { lock.unlock() }; callbackFailure = value }
    func installed() -> Data? { lock.lock(); defer { lock.unlock() }; return point }
    func revoke() { lock.lock(); defer { lock.unlock() }; point = nil }
    func tryInstall(intent: ProgramPointSinkIntent) throws -> Bool {
        lock.lock(); let owner = self.owner; let reentrant = self.reentrant; lock.unlock()
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
        case .setAll, .confirmExact:
            let paint = try XCTUnwrap(intent.point)
            XCTAssertEqual(paint.slot, outputSlot)
            XCTAssertEqual(paint.source.count, 3)
            XCTAssertEqual(paint.opacity, outputOpacity)
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
        var expectedTable: [Double] = []
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
            if let rgb = step.rgb {
                XCTAssertEqual(snapshot.outputCount(), 1)
                XCTAssertEqual(try snapshot.outputSlot(index: 0), 91)
                XCTAssertEqual(try snapshot.outputRgb(index: 0), Data(rgb))
                XCTAssertEqual(try snapshot.outputOpacity(index: 0), 1)
                expectedTable = [91] + rgb.map { Double($0) } + [1]
            }
            XCTAssertEqual(try snapshot.copyOutputTable(), expectedTable)
        }
        XCTAssertEqual(try held[0].outputRgb(index: 0), Data([0, 0, 0]))
        var copied = try held[0].outputRgb(index: 0); copied[0] = 200
        XCTAssertEqual(try held[0].outputRgb(index: 0), Data([0, 0, 0]))
        var copiedTable = try held[0].copyOutputTable(); copiedTable[1] = 200
        XCTAssertNotEqual(copiedTable, [91, 0, 0, 0, 1])
        XCTAssertEqual(try held[0].copyOutputTable(), [91, 0, 0, 0, 1])
        XCTAssertThrowsError(try held[0].outputRgb(index: 1)) { assertCode($0, "invalid_output_index") }
        let fixed = try compileProgramWire(bytes: bytes(vectors.fixed), streamId: 2)
        XCTAssertEqual(try fixed.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1).outputRgb(index: 0), Data([20, 20, 20]))
    }

    func testOutputTablesPreserveExactValuesAndOutliveTheirOwners() throws {
        let declaration = try fixture().outputTable
        var retained: [Double] = []
        do {
            let runtime = try compileProgramWire(bytes: bytes(declaration.wire), streamId: 1)
            let waiting = try runtime.updateUnknown(revision: 1, reasonId: 7)
            XCTAssertEqual(waiting.state(), "waiting")
            XCTAssertEqual(waiting.outputCount(), 0)
            XCTAssertEqual(try waiting.copyOutputTable(), [])
            let first = try runtime.updateObserved(revision: 2, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1)
            let table: [Double] = try first.copyOutputTable()
            XCTAssertEqual(first.state(), "ready")
            XCTAssertEqual(first.outputCount(), UInt64(declaration.rows.count))
            try assertOutputTable(table, declaration.rows)
            for index in declaration.rows.indices {
                let offset = index * 5
                XCTAssertEqual(table[offset], Double(try first.outputSlot(index: UInt32(index))))
                XCTAssertEqual(Array(table[offset + 1..<offset + 4]), try first.outputRgb(index: UInt32(index)).map { Double($0) })
                XCTAssertEqual(table[offset + 4].bitPattern, try first.outputOpacity(index: UInt32(index)).bitPattern)
            }
            var changed = try first.copyOutputTable(); changed[0] = -1
            XCTAssertNotEqual(changed, table)
            XCTAssertEqual(try first.copyOutputTable(), table)
            let next = try runtime.updateObserved(revision: 3, scenarioIds: [1], surfaces: Data([0, 0, 0]), surfaceCount: 1)
            XCTAssertEqual(next.state(), "ready")
            XCTAssertEqual(try next.copyOutputTable(), table)
            XCTAssertEqual(try first.copyOutputTable(), table)
            retained = table
        }
        try assertOutputTable(retained, declaration.rows)

        for row in declaration.rows {
            let host = RuntimeHost(outputSlot: row.slot, outputOpacity: row.opacity)
            var attachedTable: [Double] = []
            do {
                let attachment = try attachProgramWire(bytes: bytes(row.wire), streamId: 1,
                    outputSlot: row.slot, sinkOutput: 501, presentationRoot: 71, occurrence: 61, host: host)
                defer { host.revoke(); XCTAssertNoThrow(try attachment.dispose(confirmed: true)) }
                let waiting = try attachment.updateUnknown(revision: 1, reasonId: 7)
                XCTAssertEqual(waiting.state(), "waiting")
                XCTAssertEqual(waiting.outputCount(), 0)
                XCTAssertEqual(try waiting.copyOutputTable(), [])
                let first = try attachment.updateObserved(revision: 2, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1)
                let table: [Double] = try first.copyOutputTable()
                XCTAssertEqual(first.state(), "ready")
                XCTAssertEqual(first.outputCount(), 1)
                try assertOutputTable(table, [row])
                XCTAssertEqual(table[0], Double(try first.outputSlot(index: 0)))
                XCTAssertEqual(Array(table[1..<4]), try first.outputRgb(index: 0).map { Double($0) })
                XCTAssertEqual(table[4].bitPattern, try first.outputOpacity(index: 0).bitPattern)
                var changed = try first.copyOutputTable(); changed[4] = -1
                XCTAssertNotEqual(changed, table)
                XCTAssertEqual(try first.copyOutputTable(), table)
                let next = try attachment.updateObserved(revision: 3, scenarioIds: [1], surfaces: Data([0, 0, 0]), surfaceCount: 1)
                XCTAssertEqual(try next.copyOutputTable(), table)
                attachedTable = table
            }
            try assertOutputTable(attachedTable, [row])
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

    func testAttachmentUsesOneCommittedHeadAndRequiresExplicitRevoke() throws {
        let host = RuntimeHost()
        let attachment = try attachProgramWire(bytes: bytes(fixture().adaptive), streamId: 1,
            outputSlot: 91, sinkOutput: 501, presentationRoot: 71, occurrence: 61, host: host)
        host.configure(owner: attachment, reentrant: true)
        let first = try attachment.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1)
        let firstTable: [Double] = try first.copyOutputTable()
        XCTAssertEqual(firstTable, [91, 0, 0, 0, 1])
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
        XCTAssertEqual(try unknown.copyOutputTable(), [91, 255, 255, 255, 1])
        XCTAssertFalse(unknown.hasRender())
        XCTAssertNil(host.installed())
        XCTAssertThrowsError(try attachment.materializationAuthority()) { assertCode($0, "program_materialization_not_ready") }
        XCTAssertEqual(try attachment.updateObserved(revision: 4, scenarioIds: [1],
            surfaces: Data([255, 255, 255]), surfaceCount: 1).state(), "ready")
        let conflict = try attachment.updateObserved(revision: 5, scenarioIds: [1, 2],
            surfaces: Data([255, 255, 255, 0, 0, 0]), surfaceCount: 1)
        XCTAssertEqual(conflict.state(), "failed")
        XCTAssertEqual(try conflict.copyOutputTable(), [91, 0, 0, 0, 1])
        XCTAssertFalse(conflict.hasRender())
        XCTAssertNil(host.installed())
        XCTAssertEqual(try attachment.updateObserved(revision: 6, scenarioIds: [1],
            surfaces: Data([0, 0, 0]), surfaceCount: 1).state(), "ready")
        host.revoke(); try attachment.dispose(confirmed: true)
        XCTAssertEqual(firstTable, [91, 0, 0, 0, 1])
        XCTAssertEqual(try first.copyOutputTable(), firstTable)
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
}
