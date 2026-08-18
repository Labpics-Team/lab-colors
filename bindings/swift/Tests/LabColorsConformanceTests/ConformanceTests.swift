import Foundation
import XCTest

@testable import LabColors

/// Conformance-прогон нативного (Swift/UniFFI) биндинга против закоммиченного
/// пака `conformance/vectors/*.json`. Доказывает: рантайм-ядро Rust, вызванное
/// с Swift-стороны, воспроизводит зафиксированный conformance-пак. Числовые
/// поля сверяются в пределах `driftTol` (= `DRIFT_TOL` conformance-пака, 1e-6:
/// кросс-платформенный libm-шум ~1e-13, реальный дрейф — целые единицы).
/// Композит-hex — чистая IEEE-алгебра, точен; solve-hex — квантование
/// трансцендентного резолва, допускается ±1 LSD/канал.
/// Glow-проверка ниже намеренно НЕ утверждает bit-parity CAM16: она проверяет
/// типизированный класс решения и точный certificate композитинга отдельно.
final class ConformanceTests: XCTestCase {

    static let driftTol = 1e-6

    // Compile V1 permits one 64-KiB packed result. With a 32-byte partition,
    // the remaining 65,504 bytes hold 2,047 32-byte edge columns; 256 × 2,047
    // gives the exact work count. The strict protocol grammar independently
    // reaches its 657,380-byte ceiling when opaque IDs consume their 64-KiB cap.
    // MARK: - Локация закоммиченных векторов (относительно этого файла)

    static var vectorsDir: URL = {
        // <repo>/bindings/swift/Tests/LabColorsConformanceTests/ConformanceTests.swift
        // → снять имя файла и 4 каталога до корня репозитория.
        var dir = URL(fileURLWithPath: #filePath)
        for _ in 0..<5 { dir = dir.deletingLastPathComponent() }
        return dir.appendingPathComponent("conformance").appendingPathComponent("vectors")
    }()

    func load<T: Decodable>(_ file: String, as type: T.Type) throws -> T {
        let url = Self.vectorsDir.appendingPathComponent(file)
        let data = try Data(contentsOf: url)
        return try JSONDecoder().decode(T.self, from: data)
    }

    // MARK: - Вспомогательные мапперы словаря

    func theme(_ s: String) -> Theme {
        switch s {
        case "light": return .light
        case "dark": return .dark
        case "light-ic": return .lightIc
        case "dark-ic": return .darkIc
        default: fatalError("неизвестная тема в паке: \(s)")
        }
    }

    func failureCategory(_ key: String) -> FailureCategory {
        switch key {
        case "unreachable": return .unreachable
        case "unresolved": return .unresolved
        case "rejected": return .rejected
        default: fatalError("неизвестная failure category в pack: \(key)")
        }
    }

    func contractSpec(_ c: ContractJSON) -> ContractSpec {
        switch c.kind {
        case "text": return .text(lc: c.lc!)
        case "ui": return .ui(lc: c.lc!)
        case "range": return .range(floor: c.floor!, ceiling: c.ceiling!)
        default: fatalError("неизвестный контракт в паке: \(c.kind)")
        }
    }

    func wcag22Criterion(_ key: String) -> Wcag22Criterion {
        switch key {
        case "sc-1.4.3-text-default": return .sc143TextDefault
        case "sc-1.4.3-text-large-scale": return .sc143TextLargeScale
        case "sc-1.4.11-ui-component-or-state": return .sc1411UiComponentOrState
        case "sc-1.4.11-graphical-object": return .sc1411GraphicalObject
        default: fatalError("unknown WCAG22 criterion in pack: \(key)")
        }
    }

    func channels(_ hex: String) -> [Int] {
        let s = hex.hasPrefix("#") ? String(hex.dropFirst()) : hex
        precondition(s.count == 6, "ожидался #RRGGBB, получено \(hex)")
        return stride(from: 0, to: 6, by: 2).map { off -> Int in
            let start = s.index(s.startIndex, offsetBy: off)
            let end = s.index(start, offsetBy: 2)
            return Int(s[start..<end], radix: 16)!
        }
    }

    /// Independent encoded-sRGB8 screen oracle. It intentionally does not call
    /// the source-over FFI primitive: on black those operators coincide and
    /// would make the Glow certificate assertion vacuous.
    func screenComposite(tint: String, alpha: Double, background: String) -> String {
        let glow = channels(tint)
        let bg = channels(background)
        var result: [Int] = []
        result.reserveCapacity(3)
        for channel in 0..<3 {
            let backgroundChannel = Double(bg[channel])
            let glowChannel = Double(glow[channel])
            let backgroundHeadroom = Double(255 - bg[channel])
            let contribution = alpha * glowChannel * backgroundHeadroom / 255.0
            let rounded = Int(floor(backgroundChannel + contribution + 0.5))
            result.append(rounded)
        }
        return String(format: "#%02X%02X%02X", result[0], result[1], result[2])
    }

    /// Квантованный цвет conformant в пределах ±1 LSB на канал (кросс-платформенно).
    func assertHexWithinOne(_ a: String, _ b: String, _ ctx: String) {
        let ca = channels(a), cb = channels(b)
        for i in 0..<3 {
            XCTAssertLessThanOrEqual(
                abs(ca[i] - cb[i]), 1, "\(ctx): канал \(i) \(a) vs \(b) расходится > 1 LSB")
        }
    }

    // MARK: - Метаданные / версия

    func testCoreVersionMatchesManifest() throws {
        let manifest = try load("manifest.json", as: Manifest.self)
        XCTAssertEqual(manifest.packVersion, "10.0.0", "Swift fixture обязан исполнять pack v10")
        XCTAssertFalse(coreVersion().isEmpty)
        XCTAssertEqual(
            coreVersion(), manifest.coreVersion,
            "версия ядра биндинга разошлась с манифестом пака")
    }

    // MARK: - Capability manifest (численные решения)

    /// FNV-1a-32 (как `packDigest`) — независимая Swift-копия примитива ядра,
    /// чтобы пересчёт checksum не опирался на проверяемый Rust-код.
    func fnv1a32(_ bytes: [UInt8]) -> UInt32 {
        var hash: UInt32 = 0x811c_9dc5
        for byte in bytes {
            hash ^= UInt32(byte)
            hash = hash &* 0x0100_0193
        }
        return hash
    }

    /// Независимый пересчёт drift-checksum capability manifest по canonical
    /// preimage ядра (labcolors-core/src/numerics.rs): length-prefixed (u32 LE
    /// длина + байты) домен-сепаратор, u32 LE schema version, coverage key,
    /// u32 LE счётчик sites (сортировка по сырым UTF-8 байтам siteId), на site —
    /// siteId и семь списков ключей; каждый список: u32 LE count (явный и для
    /// пустого) + отсортированные length-prefixed ключи. Кодирование повторено
    /// здесь НАМЕРЕННО: тест — оракул, он не должен переиспользовать encoder,
    /// который проверяет.
    func testCapabilityManifestChecksumRecomputes() throws {
        let manifest = try load("manifest.json", as: Manifest.self)
        let caps = manifest.numericalCapabilities
        // Оракул реализует canonical preimage V2: другая версия схемы обязана
        // падать здесь, а не молча проходить с пересчитанным checksum.
        XCTAssertEqual(caps.schemaVersion, 2, "неподдерживаемая версия capability-схемы")
        XCTAssertEqual(caps.coverage, "migrated-sites-only-v1", "coverage capability manifest")
        XCTAssertFalse(caps.sites.isEmpty, "capability manifest без единого migrated site пуст")
        for site in caps.sites {
            XCTAssertFalse(site.siteId.isEmpty, "siteId обязан быть непустым")
            XCTAssertFalse(
                site.stableOutcomes.isEmpty,
                "site \(site.siteId) обязан объявлять lawful stable outcome")
        }

        var preimage: [UInt8] = []
        func pushU32LE(_ value: UInt32) {
            preimage.append(contentsOf: [
                UInt8(truncatingIfNeeded: value),
                UInt8(truncatingIfNeeded: value >> 8),
                UInt8(truncatingIfNeeded: value >> 16),
                UInt8(truncatingIfNeeded: value >> 24),
            ])
        }
        func pushLenPrefixed(_ key: String) {
            let bytes = Array(key.utf8)
            pushU32LE(UInt32(bytes.count))
            preimage.append(contentsOf: bytes)
        }
        // Сортировка по сырым UTF-8 байтам (эквивалент sort_unstable по &[u8]
        // в ядре), а не по Unicode-коллации String.
        func pushSortedKeyList(_ keys: [String]) {
            let sorted = keys.sorted {
                Array($0.utf8).lexicographicallyPrecedes(Array($1.utf8))
            }
            pushU32LE(UInt32(sorted.count))
            for key in sorted { pushLenPrefixed(key) }
        }

        pushLenPrefixed("labcolors.numerical-capability.v2")
        pushU32LE(caps.schemaVersion)
        pushLenPrefixed(caps.coverage)
        let sites = caps.sites.sorted {
            Array($0.siteId.utf8).lexicographicallyPrecedes(Array($1.siteId.utf8))
        }
        pushU32LE(UInt32(sites.count))
        for site in sites {
            pushLenPrefixed(site.siteId)
            pushSortedKeyList(site.stableOutcomes)
            pushSortedKeyList(site.compatibilityReleases)
            pushSortedKeyList(site.evidenceClasses)
            pushSortedKeyList(site.artifactIds)
            pushSortedKeyList(site.boundIds)
            pushSortedKeyList(site.proofIds)
            pushSortedKeyList(site.runtimeAttestations)
        }

        let recomputed = String(format: "%08x", fnv1a32(preimage))
        XCTAssertEqual(
            recomputed, caps.checksum,
            "checksum capability manifest не сходится с независимым Swift-пересчётом")
    }

    // MARK: - Семейство: контрасты

    func testContrasts() throws {
        let vectors = try load("contrasts.json", as: [ContrastVec].self)
        XCTAssertFalse(vectors.isEmpty)
        for v in vectors {
            let got = try contrast(fg: v.fg, bg: v.bg, theme: theme(v.theme))
            XCTAssertEqual(got.lc, v.lc, accuracy: Self.driftTol, "lc \(v.fg)/\(v.bg) \(v.theme)")
            XCTAssertEqual(
                got.wcagRatio, v.wcagRatio, accuracy: Self.driftTol,
                "wcag \(v.fg)/\(v.bg) \(v.theme)")
        }
    }

    // MARK: - Семейство: подложка → α

    func testAlpha() throws {
        let vectors = try load("alpha.json", as: [AlphaVec].self)
        XCTAssertFalse(vectors.isEmpty)
        for v in vectors {
            let comp = try composite(tint: v.tint, alpha: v.alpha, bg: v.bg)
            // Композит — чистая IEEE-алгебра: обязан совпасть точно.
            XCTAssertEqual(comp, v.composite, "композит \(v.tint)@\(v.alpha) на \(v.bg)")
            let m = try minAlpha(tint: v.tint, bg: v.bg)
            XCTAssertEqual(m, v.minAlpha, accuracy: Self.driftTol, "min_alpha \(v.tint)/\(v.bg)")
        }
    }

    // MARK: - Low-level Glow decision contract

    // MARK: - Семейство: резолв (снапшоты токенов)

    func testSolve() throws {
        let vectors = try load("solve.json", as: [SolveVec].self)
        XCTAssertFalse(vectors.isEmpty)
        for v in vectors {
            let spec = contractSpec(v.contract)
            let th = theme(v.theme)
            switch v.outcome.kind {
            case "solved":
                let got = try solveContrast(bg: v.bg, contract: spec, theme: th)
                assertHexWithinOne(got.hex, v.outcome.hex!, "solve hex на \(v.bg)")
                XCTAssertEqual(got.lc, v.outcome.lc!, accuracy: Self.driftTol, "solve lc \(v.bg)")
                XCTAssertEqual(
                    got.wcagRatio, v.outcome.wcagRatio!, accuracy: Self.driftTol,
                    "solve wcag \(v.bg)")
                XCTAssertEqual(got.floorOverride, v.outcome.floorOverride!, "floor_override \(v.bg)")
            case "failure":
                XCTAssertThrowsError(try solveContrast(bg: v.bg, contract: spec, theme: th)) { err in
                    guard case let ColorError.Failure(category, code) = err else {
                        return XCTFail("ожидался typed failure, получено \(err) на \(v.bg)")
                    }
                    XCTAssertEqual(
                        category, failureCategory(v.outcome.category!),
                        "категория failure на \(v.bg)")
                    XCTAssertEqual(code, v.outcome.code!, "код failure на \(v.bg)")
                }
            default:
                XCTFail("неизвестный исход в паке: \(v.outcome.kind)")
            }
        }
    }

    // MARK: - Exact WCAG 2.2 final-sRGB8 assessment

    func testWcag22() throws {
        let vectors = try load("wcag22.json", as: [Wcag22Vec].self)
        XCTAssertFalse(vectors.isEmpty)
        for vector in vectors {
            let got = try evaluateWcag22(
                foreground: vector.foreground,
                background: vector.background,
                criterion: wcag22Criterion(vector.criterion))
            XCTAssertEqual(got.profileId, vector.profileId)
            XCTAssertEqual(got.criterion, wcag22Criterion(vector.criterion))
            XCTAssertEqual(got.foreground, vector.foreground)
            XCTAssertEqual(got.background, vector.background)
            XCTAssertEqual(got.decision == .pass ? "pass" : "fail", vector.decision)
            XCTAssertEqual(got.foregroundLuminance.lower, UInt64(vector.foregroundLowerQ55))
            XCTAssertEqual(got.foregroundLuminance.upper, UInt64(vector.foregroundUpperQ55))
            XCTAssertEqual(got.backgroundLuminance.lower, UInt64(vector.backgroundLowerQ55))
            XCTAssertEqual(got.backgroundLuminance.upper, UInt64(vector.backgroundUpperQ55))
            XCTAssertEqual(got.q55Scale, UInt64(vector.q55Scale))
            XCTAssertEqual(got.evidence.kind, vector.evidenceKind)
            XCTAssertEqual(got.evidence.artifactId, vector.artifactId)
            XCTAssertEqual(got.evidence.artifactSha256, vector.artifactSha256)
            XCTAssertEqual(got.evidence.boundId, vector.boundId)
            XCTAssertEqual(got.evidence.proofId, vector.proofId)
            XCTAssertEqual(got.evidence.proofSha256, vector.proofSha256)
            XCTAssertEqual(got.evidence.proofPayloadSha256, vector.proofPayloadSha256)
            XCTAssertEqual(got.evidence.generatorSha256, vector.generatorSha256)
            XCTAssertEqual(got.evidence.verifierSha256, vector.verifierSha256)
            XCTAssertEqual(got.evidence.profileChecksum, vector.profileChecksum)
            XCTAssertEqual(got.evidence.profileSha256, vector.profileSha256)
        }
    }
}

// MARK: - Codable-зеркала схемы векторов

struct Manifest: Codable {
    let packVersion: String
    let coreVersion: String
    let packDigest: String
    let numericalCapabilities: CapabilityManifest
}

/// Зеркало proof-capable capability manifest (pack 4.0.0): typed-проекция core registry
/// численных решений. Заменяет прозаический `numericalSites` из pack 2.x —
/// биндинг сверяет typed rows и drift-checksum, а не research-тексты.
struct CapabilityManifest: Codable {
    let schemaVersion: UInt32
    let coverage: String
    let sites: [CapabilitySite]
    let checksum: String
}

/// Одна capability-строка site. Пустой список — явная часть контракта
/// («evidence отсутствует»), а не пропуск поля.
struct CapabilitySite: Codable {
    let siteId: String
    let stableOutcomes: [String]
    let compatibilityReleases: [String]
    let evidenceClasses: [String]
    let artifactIds: [String]
    let boundIds: [String]
    let proofIds: [String]
    let runtimeAttestations: [String]
}

struct Wcag22Vec: Codable {
    let foreground: String
    let background: String
    let criterion: String
    let profileId: String
    let decision: String
    let foregroundLowerQ55: String
    let foregroundUpperQ55: String
    let backgroundLowerQ55: String
    let backgroundUpperQ55: String
    let q55Scale: String
    let evidenceKind: String
    let artifactId: String
    let artifactSha256: String
    let boundId: String
    let proofId: String
    let proofSha256: String
    let proofPayloadSha256: String
    let generatorSha256: String
    let verifierSha256: String
    let profileChecksum: String
    let profileSha256: String
}

struct ContrastVec: Codable {
    let fg: String
    let bg: String
    let theme: String
    let lc: Double
    let wcagRatio: Double
}

struct AlphaVec: Codable {
    let tint: String
    let alpha: Double
    let bg: String
    let composite: String
    let minAlpha: Double
}

struct ContractJSON: Codable {
    let kind: String
    let lc: Double?
    let floor: Double?
    let ceiling: Double?
}

struct OutcomeJSON: Codable {
    let kind: String
    let hex: String?
    let lc: Double?
    let wcagRatio: Double?
    let floorOverride: Bool?
    let category: String?
    let code: String?
}

struct SolveVec: Codable {
    let bg: String
    let contract: ContractJSON
    let theme: String
    let outcome: OutcomeJSON
}

