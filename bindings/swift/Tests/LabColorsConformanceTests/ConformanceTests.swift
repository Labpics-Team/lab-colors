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

    func contractSpec(_ c: ContractJSON) -> ContractSpec {
        switch c.kind {
        case "text": return .text(lc: c.lc!)
        case "ui": return .ui(lc: c.lc!)
        case "range": return .range(floor: c.floor!, ceiling: c.ceiling!)
        default: fatalError("неизвестный контракт в паке: \(c.kind)")
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
    /// siteId и шесть списков ключей; каждый список: u32 LE count (явный и для
    /// пустого) + отсортированные length-prefixed ключи. Кодирование повторено
    /// здесь НАМЕРЕННО: тест — оракул, он не должен переиспользовать encoder,
    /// который проверяет.
    func testCapabilityManifestChecksumRecomputes() throws {
        let manifest = try load("manifest.json", as: Manifest.self)
        let caps = manifest.numericalCapabilities
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

        pushLenPrefixed("labcolors.numerical-capability.v1")
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

    // MARK: - Семейство: лестницы

    func testLadders() throws {
        let vectors = try load("ladders.json", as: [LadderVec].self)
        XCTAssertFalse(vectors.isEmpty)
        for v in vectors {
            let light = try ladderAlpha(position: v.position, theme: .light)
            let dark = try ladderAlpha(position: v.position, theme: .dark)
            XCTAssertEqual(light, v.alphaLight, accuracy: Self.driftTol, "α_light \(v.position)")
            XCTAssertEqual(dark, v.alphaDark, accuracy: Self.driftTol, "α_dark \(v.position)")
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

    func testGeneratedGlowSurfaceContainsOnlyAtomicProvenanceVariants() throws {
        var packageRoot = URL(fileURLWithPath: #filePath)
        for _ in 0..<3 { packageRoot = packageRoot.deletingLastPathComponent() }
        let generatedSource = packageRoot
            .appendingPathComponent("Sources")
            .appendingPathComponent("LabColors")
            .appendingPathComponent("labcolors.swift")
        let source = try String(contentsOf: generatedSource, encoding: .utf8)

        // Positive cases делают negative API-проверки невакуумными: читается
        // именно сгенерированная algebraic Glow surface, а не пустой/чужой файл.
        XCTAssertTrue(source.contains("public enum GlowPointDecision"))
        XCTAssertTrue(source.contains("case stableExactNoop("))
        XCTAssertTrue(source.contains("case legacyReached("))
        XCTAssertTrue(source.contains("case legacyUnreachable("))
        XCTAssertTrue(source.contains("case indeterminate("))
        XCTAssertTrue(source.contains("public enum GlowDecisionProfile"))
        XCTAssertTrue(source.contains("case IncompatibleCoreContract("))
        XCTAssertFalse(source.contains("case determinate("))
        XCTAssertFalse(source.contains("public enum GlowDecisionGuarantee"))
        XCTAssertFalse(source.contains("public enum GlowTargetStatus"))
        XCTAssertFalse(source.contains("public enum GlowDiagnosticProfile"))
    }

    func testInvalidPublicGlowInputsKeepPublicInputErrorTaxonomy() {
        let invalidInputs: [(String, String, Double)] = [
            ("not-a-color", "#000000", 2.3006),
            ("#C0B2FA", "not-a-color", 2.3006),
            ("#C0B2FA", "#000000", 0.0),
            ("#C0B2FA", "#000000", -1.0),
            ("#C0B2FA", "#000000", .nan),
            ("#C0B2FA", "#000000", .infinity),
        ]
        for (tint, background, targetDj) in invalidInputs {
            XCTAssertThrowsError(try solveGlowPoint(
                tint: tint,
                background: background,
                targetDj: targetDj,
                theme: .light,
                profile: .stableV1
            )) { error in
                guard case let ColorError.InvalidGlowRequest(reason) = error else {
                    return XCTFail(
                        "public input error не должен становиться adapter incompatibility: \(error)")
                }
                XCTAssertFalse(reason.isEmpty)
            }
        }
    }

    func testGlowInputProfilesRemainExplicitAndOutputProvenanceIsAtomic() throws {
        let tint = "#C0B2FA"
        let background = "#101012"
        let targetDj = 2.3006

        // Independent exact-rational anti-vacuum probe: at alpha 1/2 the two
        // encoded operators differ by two bytes in R/G. Keeping this fixture
        // independent from the solver-selected alpha prevents an accidental
        // same-hex quantisation from weakening the oracle.
        let screenProbe = screenComposite(tint: tint, alpha: 0.5, background: background)
        let sourceOverProbe = try composite(tint: tint, alpha: 0.5, bg: background)
        XCTAssertEqual(screenProbe, "#6A6386")
        XCTAssertEqual(sourceOverProbe, "#686186")
        XCTAssertNotEqual(screenProbe, sourceOverProbe)

        let stable = try solveGlowPoint(
            tint: tint,
            background: background,
            targetDj: targetDj,
            theme: .light,
            profile: .stableV1)
        switch stable {
        case let .indeterminate(siteId, evidence):
            XCTAssertEqual(siteId, .glowTargetOrMaximumV1)
            guard case .soundBoundUnavailable = evidence else {
                return XCTFail("stable-v1 обязан вернуть typed unavailable-bound evidence")
            }
        case .stableExactNoop, .legacyReached, .legacyUnreachable:
            XCTFail("stable-v1 не должен выбирать состояние без sound bound")
        }

        let legacy = try solveGlowPoint(
            tint: tint,
            background: background,
            targetDj: targetDj,
            theme: .light,
            profile: .legacyPlatformDependentV1)
        switch legacy {
        case let .legacyReached(value):
            XCTAssertEqual(value.compositeProfile, .encodedSrgb8ScreenV1)
            XCTAssertEqual(value.compositeGuarantee, .bitExact)
            XCTAssertEqual(value.targetDj, targetDj)
            let recomposite = screenComposite(
                tint: tint, alpha: value.alpha, background: background)
            XCTAssertEqual(
                recomposite,
                value.compositeHex,
                "bit-exact относится к композитору, не к CAM16 decision")
        case .stableExactNoop, .legacyUnreachable, .indeterminate:
            XCTFail("explicit legacy reached fixture обязан вернуть atomic legacyReached")
        }

        func assertStableNoop(tint: String, background: String, composite expected: String) throws {
            let decision = try solveGlowPoint(
                tint: tint,
                background: background,
                targetDj: targetDj,
                theme: .light,
                profile: .stableV1)
            switch decision {
            case let .stableExactNoop(value):
                XCTAssertEqual(value.compositeProfile, .encodedSrgb8ScreenV1)
                XCTAssertEqual(value.compositeGuarantee, .bitExact)
                XCTAssertEqual(value.achievedDj, 0.0)
                XCTAssertEqual(value.compositeHex, expected)
            case .legacyReached, .legacyUnreachable, .indeterminate:
                XCTFail("exact screen no-op обязан вернуть atomic stableExactNoop")
            }
        }

        try assertStableNoop(tint: tint, background: "#FFFFFF", composite: "#FFFFFF")
        try assertStableNoop(tint: "#010000", background: "#FE0000", composite: "#FE0000")

        let legacyUnreachable = try solveGlowPoint(
            tint: tint,
            background: "#FFFFFF",
            targetDj: targetDj,
            theme: .light,
            profile: .legacyPlatformDependentV1)
        switch legacyUnreachable {
        case let .legacyUnreachable(value):
            XCTAssertEqual(value.compositeProfile, .encodedSrgb8ScreenV1)
            XCTAssertEqual(value.compositeGuarantee, .bitExact)
            XCTAssertEqual(value.compositeHex, "#FFFFFF")
        case .stableExactNoop, .legacyReached, .indeterminate:
            XCTFail("explicit legacy no-op обязан вернуть atomic legacyUnreachable")
        }

        let crossing = try solveGlowPoint(
            tint: "#800000",
            background: "#FE0000",
            targetDj: targetDj,
            theme: .light,
            profile: .stableV1)
        switch crossing {
        case let .indeterminate(siteId, evidence):
            XCTAssertEqual(siteId, .glowTargetOrMaximumV1)
            guard case .soundBoundUnavailable = evidence else {
                return XCTFail("first crossing обязан сохранить typed unavailable-bound evidence")
            }
        case .stableExactNoop, .legacyReached, .legacyUnreachable:
            XCTFail("#800000 над #FE0000 пересекает первый half-LSB wall")
        }
    }

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
            case "unreachable":
                XCTAssertThrowsError(try solveContrast(bg: v.bg, contract: spec, theme: th)) { err in
                    guard case let ColorError.Unreachable(code) = err else {
                        return XCTFail("ожидался Unreachable, получено \(err) на \(v.bg)")
                    }
                    XCTAssertEqual(code, v.outcome.code!, "код недостижимости на \(v.bg)")
                }
            default:
                XCTFail("неизвестный исход в паке: \(v.outcome.kind)")
            }
        }
    }

    // MARK: - Семейство: legacy proxy coordinate

    func testMuddiness() throws {
        let vectors = try load("muddiness.json", as: [MuddinessVec].self)
        XCTAssertFalse(vectors.isEmpty)
        for v in vectors {
            let got = try muddiness(hex: v.hex)
            XCTAssertEqual(got, v.score, accuracy: Self.driftTol, "muddiness \(v.hex)")
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

/// Зеркало capability manifest (pack 3.0.0): typed-проекция core registry
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
    let runtimeAttestations: [String]
}

struct ContrastVec: Codable {
    let fg: String
    let bg: String
    let theme: String
    let lc: Double
    let wcagRatio: Double
}

struct LadderVec: Codable {
    let position: String
    let alphaLight: Double
    let alphaDark: Double
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
    let code: String?
}

struct SolveVec: Codable {
    let bg: String
    let contract: ContractJSON
    let theme: String
    let outcome: OutcomeJSON
}

struct MuddinessVec: Codable {
    let hex: String
    let score: Double
}
