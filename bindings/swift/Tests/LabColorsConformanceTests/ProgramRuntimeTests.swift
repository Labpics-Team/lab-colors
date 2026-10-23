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
struct RuntimeFixture: Decodable {
    let schemaVersion: Int
    let fixed: String
    let adaptive: String
    let encodingOnly: String
    let history: [RuntimeStep]
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
    private var rejected = false
    private var callbackFailure = false
    private var reentrant = false
    private var point: Data?
    private var sequence: UInt64 = 0
    private weak var owner: ProgramAttachment?
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
        if callbackFailure { throw ProgramPointSinkCallbackError.InvocationFailed }
        if rejected { return false }
        if intent.operation != .confirmExact { XCTAssertEqual(intent.desiredSequence, sequence + 1) }
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
            if let rgb = step.rgb {
                XCTAssertEqual(snapshot.outputCount(), 1)
                XCTAssertEqual(try snapshot.outputSlot(index: 0), 91)
                XCTAssertEqual(try snapshot.outputRgb(index: 0), Data(rgb))
                XCTAssertEqual(try snapshot.outputOpacity(index: 0), 1)
            }
        }
        XCTAssertEqual(try held[0].outputRgb(index: 0), Data([0, 0, 0]))
        var copied = try held[0].outputRgb(index: 0); copied[0] = 200
        XCTAssertEqual(try held[0].outputRgb(index: 0), Data([0, 0, 0]))
        XCTAssertThrowsError(try held[0].outputRgb(index: 1)) { assertCode($0, "invalid_output_index") }
        let fixed = try compileProgramWire(bytes: bytes(vectors.fixed), streamId: 2)
        XCTAssertEqual(try fixed.updateObserved(revision: 1, scenarioIds: [1], surfaces: Data([255, 255, 255]), surfaceCount: 1).outputRgb(index: 0), Data([20, 20, 20]))
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
        host.revoke(); try attachment.dispose(confirmed: true)
        XCTAssertThrowsError(try attachment.updateUnknown(revision: 3, reasonId: 7)) { assertCode($0, "program_attachment_already_disposed") }
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
