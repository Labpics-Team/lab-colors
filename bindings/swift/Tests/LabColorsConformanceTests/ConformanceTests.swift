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
    static let exactMaxFeasibilityRequestBytes = 657_380
    static let exactMaxFeasibilityOpaqueBytes = 65_536
    static let exactMaxFeasibilityPackedResultBytes = 65_536
    static let exactMaxFeasibilityDomainCandidates = 256
    static let exactMaxFeasibilityPartitionBytes = exactMaxFeasibilityDomainCandidates / 8
    static let exactMaxFeasibilityRelations =
        (exactMaxFeasibilityPackedResultBytes - exactMaxFeasibilityPartitionBytes)
        / exactMaxFeasibilityPartitionBytes
    static let exactMaxFeasibilityMatrixBytes =
        exactMaxFeasibilityPartitionBytes * exactMaxFeasibilityRelations
    static let exactMaxFeasibilityLogicalAssessments =
        exactMaxFeasibilityDomainCandidates * exactMaxFeasibilityRelations

    struct ExactMaxFeasibilityWitness {
        let request: Data
        let rawRelations: Int
        let rawApplicableRelations: Int
        let rawAdjacentEntries: Int
        let opaqueUtf8Bytes: Int
    }

    enum ExactMaxFeasibilityWitnessError: Error {
        case mismatch(String)
    }

    func makeExactMaxFeasibilityWitness(
        relationCount: Int = exactMaxFeasibilityRelations
    ) throws -> ExactMaxFeasibilityWitness {
        let alphabet: [UInt8] = [
            0, 1, 2, 3, 4, 5, 6, 7, 11, 14, 15, 16, 17, 18, 19, 20, 21, 22,
            23, 24, 25, 26, 27, 28, 29, 30, 31,
        ]
        let alphabetCount = alphabet.count
        let relationIdBytes = relationCount * 3
        let firstOccurrenceBytes =
            Self.exactMaxFeasibilityOpaqueBytes - relationIdBytes - (relationCount - 1)
        guard firstOccurrenceBytes > 0 else {
            throw ExactMaxFeasibilityWitnessError.mismatch("first occurrence ID is empty")
        }

        var opaqueUtf8Bytes = 0
        let relations: [[String: Any]] = (0..<relationCount).map { index in
            let relationId =
                String(UnicodeScalar(Int(alphabet[(index / (alphabetCount * alphabetCount))
                    % alphabetCount]))!)
                + String(UnicodeScalar(Int(alphabet[(index / alphabetCount) % alphabetCount]))!)
                + String(UnicodeScalar(Int(alphabet[index % alphabetCount]))!)
            let occurrenceId = index == 0
                ? String(repeating: "\0", count: firstOccurrenceBytes)
                : "\0"
            opaqueUtf8Bytes += relationId.utf8.count + occurrenceId.utf8.count
            return [
                "relationId": relationId,
                "occurrenceId": occurrenceId,
                "kind": "applicable",
                "criterion": "sc-1.4.11-ui-component-or-state",
                "adjacent": [[255, 255, 255]],
            ]
        }
        let request = try JSONSerialization.data(
            withJSONObject: [
                "schemaVersion": 1,
                "domainId": "srgb8-neutral-axis-v1",
                "resourceProfileId": "compile-v1",
                "relations": relations,
            ],
            options: [.sortedKeys])
        return ExactMaxFeasibilityWitness(
            request: request,
            rawRelations: relations.count,
            rawApplicableRelations: relations.count,
            rawAdjacentEntries: relations.count,
            opaqueUtf8Bytes: opaqueUtf8Bytes)
    }

    func validateExactMaxFeasibilityWitness(
        _ witness: ExactMaxFeasibilityWitness
    ) throws {
        let expected = [
            ("request bytes", witness.request.count, Self.exactMaxFeasibilityRequestBytes),
            ("raw relations", witness.rawRelations, Self.exactMaxFeasibilityRelations),
            (
                "raw applicable relations", witness.rawApplicableRelations,
                Self.exactMaxFeasibilityRelations
            ),
            (
                "raw adjacent entries", witness.rawAdjacentEntries,
                Self.exactMaxFeasibilityRelations
            ),
            (
                "opaque UTF-8 bytes", witness.opaqueUtf8Bytes,
                Self.exactMaxFeasibilityOpaqueBytes
            ),
        ]
        for (field, actual, limit) in expected where actual != limit {
            throw ExactMaxFeasibilityWitnessError.mismatch(
                "\(field): expected \(limit), got \(actual)")
        }
    }

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

    // MARK: - WCAG 2.2 finite-domain feasibility protocol

    func testWcag22FeasibilityPackReplaysThroughRawAndTypedSwiftSurfaces() throws {
        let vectors = try load(
            "wcag22-feasibility.json", as: [Wcag22FeasibilityVector].self)
        XCTAssertEqual(vectors.count, 13, "pack 5 family cardinality")

        for vector in vectors {
            let request = Array(vector.requestJson.utf8)
            let raw = try evaluateWcag22FeasibilityRawV1(request: Data(request))
            XCTAssertEqual(
                raw, Data(vector.outcomeJson.utf8),
                "canonical FFI bytes \(vector.caseId)")

            let typed = try evaluateWcag22Feasibility(Data(request))
            let expected = try JSONDecoder().decode(
                Wcag22FeasibilityOutcomeV1.self, from: Data(vector.outcomeJson.utf8))
            XCTAssertEqual(typed, expected, "typed outcome \(vector.caseId)")
        }
    }

    func testWcag22FeasibilityDomainPackingAndOpaqueIdentityLaws() throws {
        let vectors = try load(
            "wcag22-feasibility.json", as: [Wcag22FeasibilityVector].self)
        var evaluatedById: [String: Wcag22FeasibilityEvaluatedV1] = [:]
        var feasibleCounts: [String: Int] = [:]
        var sawNotEvaluated = false
        var sawConflict = false
        var sawRawResourceRejection = false

        func lsb0(_ bytes: [UInt8], _ index: Int) -> Bool {
            (bytes[index / 8] & (UInt8(1) << UInt8(index % 8))) != 0
        }

        for vector in vectors {
            let outcome = try evaluateWcag22Feasibility(Array(vector.requestJson.utf8))
            switch outcome {
            case let .success(feasibility):
                switch feasibility {
                case let .feasible(value), let .infeasible(value):
                    XCTAssertEqual(value.domain.count, 256, vector.caseId)
                    for (index, candidate) in value.domain.enumerated() {
                        XCTAssertEqual(
                            candidate.bytes,
                            [UInt8(index), UInt8(index), UInt8(index)],
                            "Core-owned domain order \(vector.caseId) @ \(index)")
                    }

                    let edgeCount = value.relations.reduce(into: 0) { count, relation in
                        if case let .applicable(_, _, _, adjacent) = relation {
                            count += adjacent.count
                        }
                    }
                    XCTAssertEqual(value.proof.applicableEdges.value, UInt64(edgeCount))
                    XCTAssertEqual(value.failureMatrix.count, 32 * edgeCount)
                    XCTAssertEqual(value.proof.partition.bytes.count, 32)
                    XCTAssertEqual(
                        value.proof.logicalAssessments.value, UInt64(256 * edgeCount))

                    var feasibleCount = 0
                    for candidate in 0..<256 {
                        var rowFailed = false
                        for edge in 0..<edgeCount {
                            if lsb0(value.failureMatrix, candidate * edgeCount + edge) {
                                rowFailed = true
                            }
                        }
                        let partitionSaysFeasible = lsb0(value.proof.partition.bytes, candidate)
                        XCTAssertEqual(
                            partitionSaysFeasible, !rowFailed,
                            "candidate-major LSB0 law \(vector.caseId) @ \(candidate)")
                        if partitionSaysFeasible { feasibleCount += 1 }
                    }
                    feasibleCounts[vector.caseId] = feasibleCount
                    evaluatedById[vector.caseId] = value
                case let .notEvaluated(value):
                    sawNotEvaluated = true
                    XCTAssertEqual(vector.caseId, "all-not-applicable")
                    XCTAssertFalse(value.relations.isEmpty)
                    for relation in value.relations {
                        guard case .notApplicable = relation else {
                            return XCTFail("NotEvaluated carried an applicable relation")
                        }
                    }
                }
            case let .failure(error):
                switch error {
                case let .core(coreError):
                    switch coreError {
                    case let .invalidRequest(invalid):
                        if case let .conflictingRelationId(relationId) = invalid {
                            sawConflict = true
                            XCTAssertEqual(vector.caseId, "conflicting-relation-id")
                            XCTAssertEqual(relationId, "same-id")
                        }
                    case let .resourceLimitExceeded(_, dimension, requested, limit):
                        sawRawResourceRejection = true
                        XCTAssertEqual(vector.caseId, "raw-adjacent-resource-rejection")
                        XCTAssertEqual(dimension, .rawAdjacentEntries)
                        XCTAssertGreaterThan(requested.value, limit.value)
                    case .allocationFailed, .evaluatorInvariantViolation,
                         .compilerInvariantViolation:
                        XCTFail("unexpected pack error branch: \(coreError)")
                    }
                case .transport, .incompatibleCoreContract:
                    XCTFail("unexpected pack failure source: \(error)")
                }
            }
        }

        XCTAssertEqual(feasibleCounts["text-default-seven"], 7)
        XCTAssertEqual(feasibleCounts["text-default-two"], 2)
        XCTAssertEqual(feasibleCounts["text-default-zero"], 0)
        XCTAssertEqual(feasibleCounts["text-large-scale-ninety-two"], 92)
        XCTAssertEqual(feasibleCounts["ui-component-ninety-two"], 92)
        XCTAssertEqual(feasibleCounts["graphical-object-ninety-two"], 92)
        XCTAssertEqual(feasibleCounts["ui-component-fifty-nine"], 59)
        XCTAssertTrue(sawNotEvaluated)
        XCTAssertTrue(sawConflict)
        XCTAssertTrue(sawRawResourceRejection)

        let opaqueA = try XCTUnwrap(evaluatedById["opaque-identity-a"])
        let opaqueB = try XCTUnwrap(evaluatedById["opaque-identity-b"])
        XCTAssertEqual(opaqueA.failureMatrix, opaqueB.failureMatrix)
        XCTAssertEqual(opaqueA.proof.partition, opaqueB.proof.partition)
        XCTAssertEqual(opaqueA.proof.matrixDigest, opaqueB.proof.matrixDigest)
        XCTAssertNotEqual(opaqueA.proof.relationSetDigest, opaqueB.proof.relationSetDigest)
        XCTAssertNotEqual(opaqueA.proof.evaluationId, opaqueB.proof.evaluationId)
    }

    func testWcag22FeasibilityPreflightAvoidsOversizeRawCopy() throws {
        var rawCalls = 0
        var oversizeCalls: [UInt64] = []
        let invalidUtf8Failure = Data(
            #"{"schemaVersion":1,"outcome":"failure","error":{"source":"transport","error":{"code":"invalidUtf8"}}}"#.utf8)
        let oversizeFailure = Data(
            #"{"schemaVersion":1,"outcome":"failure","error":{"source":"transport","error":{"code":"envelopeTooLarge","requestedBytes":"4","limitBytes":"3"}}}"#.utf8)
        let bridge = Wcag22FeasibilityBridge(
            maxRequestBytes: { 3 },
            evaluateRaw: { bytes in
                rawCalls += 1
                XCTAssertEqual(bytes.count, 3)
                return invalidUtf8Failure
            },
            envelopeTooLarge: { requested in
                oversizeCalls.append(requested)
                return oversizeFailure
            })

        _ = try evaluateWcag22Feasibility(Data([0, 1, 2]), using: bridge)
        XCTAssertEqual(rawCalls, 1, "limit bytes must reach authoritative Rust path")
        XCTAssertTrue(oversizeCalls.isEmpty)

        let outcome = try evaluateWcag22Feasibility(Data([0, 1, 2, 3]), using: bridge)
        XCTAssertEqual(rawCalls, 1, "limit+1 must not materialize/pass the raw FFI vector")
        XCTAssertEqual(oversizeCalls, [4])
        guard case let .failure(.transport(.envelopeTooLarge(requested, limit))) = outcome else {
            return XCTFail("oversize preflight did not preserve typed protocol failure")
        }
        XCTAssertEqual(requested.value, 4)
        XCTAssertEqual(limit.value, 3)
    }

    func testWcag22FeasibilityOversizePreflightMatchesScalarHelper() throws {
        let maxBytes = wcag22FeasibilityMaxBytes()
        XCTAssertEqual(maxBytes, wcag22FeasibilityMaxRequestBytesV1())

        let requestedBytes = maxBytes + 1
        var rawCalls = 0
        var scalarCalls = 0
        var ffiSubmittedBytes = 0
        let live = Wcag22FeasibilityBridge.live
        let observing = Wcag22FeasibilityBridge(
            maxRequestBytes: live.maxRequestBytes,
            evaluateRaw: { request in
                rawCalls += 1
                ffiSubmittedBytes += request.count
                return try live.evaluateRaw(request)
            },
            envelopeTooLarge: { requested in
                scalarCalls += 1
                return try live.envelopeTooLarge(requested)
            })

        let oversized = Data(repeating: 0x20, count: Int(requestedBytes))
        let outcome = try evaluateWcag22Feasibility(oversized, using: observing)
        XCTAssertEqual(rawCalls, 0, "limit+1 must not materialize/pass the raw FFI vector")
        XCTAssertEqual(scalarCalls, 1)
        XCTAssertEqual(ffiSubmittedBytes, 0)
        guard case let .failure(.transport(.envelopeTooLarge(requested, limit))) = outcome else {
            return XCTFail("oversize preflight did not preserve typed protocol failure")
        }
        XCTAssertEqual(requested.value, requestedBytes)
        XCTAssertEqual(limit.value, maxBytes)

        let scalarBytes = try wcag22FeasibilityEnvelopeTooLargeV1(
            requestedBytes: requestedBytes)
        let scalarOutcome = try JSONDecoder().decode(
            Wcag22FeasibilityOutcomeV1.self, from: scalarBytes)
        XCTAssertEqual(
            scalarOutcome, outcome,
            "scalar helper and typed preflight must agree on the canonical failure")
    }

    func testWcag22FeasibilityExactMaximumCrossesTypedUniFFIBoundaryOnce() throws {
        let witness = try makeExactMaxFeasibilityWitness()
        try validateExactMaxFeasibilityWitness(witness)
        let offByOne = try makeExactMaxFeasibilityWitness(
            relationCount: Self.exactMaxFeasibilityRelations - 1)
        XCTAssertThrowsError(try validateExactMaxFeasibilityWitness(offByOne))
        XCTAssertEqual(
            wcag22FeasibilityMaxBytes(), UInt64(Self.exactMaxFeasibilityRequestBytes))

        var rawCalls = 0
        var scalarCalls = 0
        var ffiSubmittedBytes = 0
        var rawOutput: Data?
        let live = Wcag22FeasibilityBridge.live
        let observing = Wcag22FeasibilityBridge(
            maxRequestBytes: live.maxRequestBytes,
            evaluateRaw: { request in
                rawCalls += 1
                ffiSubmittedBytes += request.count
                let output = try live.evaluateRaw(request)
                rawOutput = output
                return output
            },
            envelopeTooLarge: { requested in
                scalarCalls += 1
                return try live.envelopeTooLarge(requested)
            })

        let outcome = try evaluateWcag22Feasibility(witness.request, using: observing)
        XCTAssertEqual(rawCalls, 1)
        XCTAssertEqual(scalarCalls, 0)
        XCTAssertEqual(ffiSubmittedBytes, Self.exactMaxFeasibilityRequestBytes)

        guard case let .success(feasibility) = outcome else {
            return XCTFail("exact maximum request did not preserve Success")
        }
        guard case let .feasible(result) = feasibility else {
            return XCTFail("exact maximum request did not produce a feasible terminal")
        }
        let edgeCount = result.relations.reduce(into: 0) { count, relation in
            if case let .applicable(_, _, _, adjacent) = relation {
                count += adjacent.count
            }
        }
        XCTAssertEqual(result.domain.count, Self.exactMaxFeasibilityDomainCandidates)
        XCTAssertEqual(result.relations.count, Self.exactMaxFeasibilityRelations)
        XCTAssertEqual(edgeCount, Self.exactMaxFeasibilityRelations)
        XCTAssertEqual(result.failureMatrix.count, Self.exactMaxFeasibilityMatrixBytes)
        XCTAssertEqual(
            result.proof.partition.bytes.count, Self.exactMaxFeasibilityPartitionBytes)
        XCTAssertEqual(
            result.proof.canonicalRelations.value,
            UInt64(Self.exactMaxFeasibilityRelations))
        XCTAssertEqual(
            result.proof.applicableRelations.value,
            UInt64(Self.exactMaxFeasibilityRelations))
        XCTAssertEqual(result.proof.notApplicableRelations.value, 0)
        XCTAssertEqual(
            result.proof.applicableEdges.value,
            UInt64(Self.exactMaxFeasibilityRelations))
        XCTAssertEqual(
            result.proof.logicalAssessments.value,
            UInt64(Self.exactMaxFeasibilityLogicalAssessments))

        let outputData = try XCTUnwrap(rawOutput)
        var root = try XCTUnwrap(
            JSONSerialization.jsonObject(with: outputData) as? [String: Any])
        var rawFeasibility = try XCTUnwrap(root["feasibility"] as? [String: Any])
        var rawResult = try XCTUnwrap(rawFeasibility["result"] as? [String: Any])
        XCTAssertEqual(
            Set(rawResult.keys), Set(["domain", "relations", "failureMatrix", "proof"]))

        func assertNoCellOrProportionalDTO(_ value: Any) {
            if let object = value as? [String: Any] {
                for (key, child) in object {
                    XCTAssertFalse(key.localizedCaseInsensitiveContains("cell"))
                    XCTAssertFalse(key.localizedCaseInsensitiveContains("proportional"))
                    assertNoCellOrProportionalDTO(child)
                }
            } else if let array = value as? [Any] {
                for child in array { assertNoCellOrProportionalDTO(child) }
            }
        }
        assertNoCellOrProportionalDTO(root)

        var rawRelations = try XCTUnwrap(rawResult["relations"] as? [[String: Any]])
        rawRelations.removeLast()
        rawResult["relations"] = rawRelations
        rawFeasibility["result"] = rawResult
        root["feasibility"] = rawFeasibility
        let missingEdge = try JSONSerialization.data(
            withJSONObject: root, options: [.sortedKeys])
        XCTAssertThrowsError(
            try JSONDecoder().decode(Wcag22FeasibilityOutcomeV1.self, from: missingEdge),
            "one missing canonical edge must invalidate the typed terminal")
    }

    func testWcag22FeasibilityPackedConsumerRejectsStructuralMutations() throws {
        let vectors = try load(
            "wcag22-feasibility.json", as: [Wcag22FeasibilityVector].self)
        let fixture = try XCTUnwrap(vectors.first { $0.caseId == "text-default-seven" })
        let canonicalData = Data(fixture.outcomeJson.utf8)
        let canonical = try JSONDecoder().decode(
            Wcag22FeasibilityOutcomeV1.self, from: canonicalData)

        func mutated(
            _ change: (inout [[Int]], inout [Int], inout [String: Any]) throws -> Void
        ) throws -> Data {
            var root = try XCTUnwrap(
                JSONSerialization.jsonObject(with: canonicalData) as? [String: Any])
            var feasibility = try XCTUnwrap(root["feasibility"] as? [String: Any])
            var result = try XCTUnwrap(feasibility["result"] as? [String: Any])
            var domain = try XCTUnwrap(result["domain"] as? [[Int]])
            var matrix = try XCTUnwrap(result["failureMatrix"] as? [Int])
            var proof = try XCTUnwrap(result["proof"] as? [String: Any])
            try change(&domain, &matrix, &proof)
            result["domain"] = domain
            result["failureMatrix"] = matrix
            result["proof"] = proof
            feasibility["result"] = result
            root["feasibility"] = feasibility
            return try JSONSerialization.data(withJSONObject: root, options: [.sortedKeys])
        }

        let wrongFirstBit = try mutated { _, matrix, _ in matrix[0] ^= 1 }
        XCTAssertThrowsError(
            try JSONDecoder().decode(Wcag22FeasibilityOutcomeV1.self, from: wrongFirstBit))

        let truncated = try mutated { _, matrix, _ in matrix.removeLast() }
        XCTAssertThrowsError(
            try JSONDecoder().decode(Wcag22FeasibilityOutcomeV1.self, from: truncated))

        let transposedDomain = try mutated { domain, _, _ in domain.swapAt(0, 1) }
        XCTAssertThrowsError(
            try JSONDecoder().decode(Wcag22FeasibilityOutcomeV1.self, from: transposedDomain))

        let interiorTransposedDomain = try mutated { domain, _, _ in domain.swapAt(1, 2) }
        XCTAssertThrowsError(
            try JSONDecoder().decode(
                Wcag22FeasibilityOutcomeV1.self, from: interiorTransposedDomain))

        func reversedBits(_ byte: Int) -> Int {
            var source = byte
            var result = 0
            for _ in 0..<8 {
                result = (result << 1) | (source & 1)
                source >>= 1
            }
            return result
        }
        let msb0 = try mutated { _, matrix, _ in matrix = matrix.map(reversedBits) }
        XCTAssertThrowsError(
            try JSONDecoder().decode(Wcag22FeasibilityOutcomeV1.self, from: msb0))

        var forgedRoot = try XCTUnwrap(
            JSONSerialization.jsonObject(with: canonicalData) as? [String: Any])
        var forgedFeasibility = try XCTUnwrap(forgedRoot["feasibility"] as? [String: Any])
        forgedFeasibility["status"] = "infeasible"
        forgedRoot["feasibility"] = forgedFeasibility
        let forgedTerminal = try JSONSerialization.data(
            withJSONObject: forgedRoot, options: [.sortedKeys])
        XCTAssertThrowsError(
            try JSONDecoder().decode(Wcag22FeasibilityOutcomeV1.self, from: forgedTerminal))

        let changedIdentity = try mutated { _, _, proof in
            var digest = try XCTUnwrap(proof["relationSetDigest"] as? [Int])
            let first = try XCTUnwrap(digest.indices.first)
            digest[first] ^= 1
            proof["relationSetDigest"] = digest
        }
        let changedIdentityOutcome = try JSONDecoder().decode(
            Wcag22FeasibilityOutcomeV1.self, from: changedIdentity)
        XCTAssertNotEqual(
            changedIdentityOutcome, canonical,
            "pack equality must reject an identity mutation even when shape remains lawful")
    }

    func testWcag22FeasibilitySwiftFailureAlgebraRoundTripsExhaustively() throws {
        let one = DecimalU64V1(1)
        let two = DecimalU64V1(2)
        let rgb = Srgb8BytesV1(red: 1, green: 2, blue: 3)
        let transport: [Wcag22FeasibilityTransportErrorV1] = [
            .envelopeTooLarge(requestedBytes: two, limitBytes: one),
            .invalidUtf8,
            .malformedEnvelope(.syntax),
            .malformedEnvelope(.shape),
            .malformedEnvelope(.endOfInput),
            .malformedEnvelope(.io),
            .unsupportedSchemaVersion(2),
            .unsupportedDomainId("future"),
            .unsupportedResourceProfileId("future"),
            .unsupportedCriterion("future"),
            .emptyNotApplicableReason,
        ]
        let invalid: [Wcag22FeasibilityInvalidRequestV1] = [
            .emptyRelationId,
            .emptyOccurrenceId,
            .emptyRelations,
            .emptyAdjacentSet(relationId: "r"),
            .conflictingRelationId(relationId: "r"),
            .arithmeticOverflow,
        ]
        let atomic: [Wcag22FeasibilityAtomicErrorV1] = [
            .invalidSrgb8(field: "foreground", reason: "fixture"),
            .emptyNotApplicableReason,
            .artifactInvariantViolation(
                criterion: .sc143TextDefault, foreground: rgb, background: rgb),
            .evidenceRegistryMismatch(message: "fixture"),
        ]
        var evaluator: [Wcag22FeasibilityEvaluatorInvariantV1] = atomic.map { .source($0) }
        evaluator += [
            .unexpectedNotEvaluated, .inputMismatch, .criterionMismatch, .evidenceMismatch,
        ]
        let compiler: [Wcag22FeasibilityCompilerInvariantV1] = [
            .layoutMismatch,
            .assessmentCardinalityMismatch(expected: one, observed: two),
            .candidateCardinalityMismatch(expected: one, observed: two),
            .decisionStorageRejectedCell,
            .decisionStorageRejectedPartition,
            .completeResultMismatch,
        ]
        var core: [Wcag22FeasibilityCoreErrorV1] = invalid.map { .invalidRequest($0) }
        core += [
            .resourceLimitExceeded(
                profileId: .compileV1,
                dimension: .rawRelations,
                requested: two,
                limit: one),
            .allocationFailed(profileId: .compileV1, requestedBytes: two),
        ]
        core += evaluator.map {
            .evaluatorInvariantViolation(
                candidate: rgb, relationId: "r", adjacent: rgb, violation: $0)
        }
        core += compiler.map { .compilerInvariantViolation($0) }

        let protocolErrors = transport.map { Wcag22FeasibilityProtocolErrorV1.transport($0) }
            + core.map { Wcag22FeasibilityProtocolErrorV1.core($0) }
            + [.incompatibleCoreContract]
        XCTAssertEqual(protocolErrors.count, 34, "new branch requires an explicit test fixture")
        for error in protocolErrors {
            let outcome = Wcag22FeasibilityOutcomeV1.failure(error)
            let encoded = try JSONEncoder().encode(outcome)
            let decoded = try JSONDecoder().decode(
                Wcag22FeasibilityOutcomeV1.self, from: encoded)
            XCTAssertEqual(decoded, outcome)
        }
    }

    // MARK: - Atomic WCAG 2.2 explicit-selection protocol (#296-C3)

    func loadExplicitSelectionVectors() throws -> [Wcag22ExplicitSelectionVector] {
        let vectors = try load(
            "wcag22-explicit-selection.json", as: [Wcag22ExplicitSelectionVector].self)
        XCTAssertEqual(vectors.count, 15, "atomic explicit-selection family cardinality")
        return vectors
    }

    /// (a) Реплей всех 15 векторов: байт-точная сверка сырого FFI-выхода с
    /// закоммиченным исходом плюс равенство типизированной проекции.
    func testWcag22ExplicitSelectionPackReplaysThroughRawAndTypedSwiftSurfaces() throws {
        for vector in try loadExplicitSelectionVectors() {
            let request = Data(vector.requestJson.utf8)
            let raw = try evaluateWcag22ExplicitSelectionRawV1(request: request)
            XCTAssertEqual(
                raw, Data(vector.outcomeJson.utf8),
                "canonical FFI bytes \(vector.caseId)")

            let typed = try evaluateWcag22ExplicitSelection(request)
            let expected = try JSONDecoder().decode(
                Wcag22ExplicitSelectionOutcomeV1.self, from: Data(vector.outcomeJson.utf8))
            XCTAssertEqual(typed, expected, "typed outcome \(vector.caseId)")
        }
    }

    /// (b) decode → re-encode через Codable-типы воспроизводит канонический
    /// JSON-терм: ни одно поле не теряется, не переименовывается и не меняет
    /// представления. Порядок ключей serde пинуется байт-точно сырым
    /// FFI-реплеем выше; Swift-овский JSONEncoder порядок вставки не
    /// сохраняет, поэтому байтовая сверка идёт после нормализации ОБЕИХ
    /// сторон одним и тем же sorted-keys сериализатором.
    func testWcag22ExplicitSelectionTypedDecodeReencodesCanonicalTree() throws {
        func normalized(_ data: Data) throws -> Data {
            try JSONSerialization.data(
                withJSONObject: JSONSerialization.jsonObject(with: data),
                options: [.sortedKeys])
        }
        for vector in try loadExplicitSelectionVectors() {
            let canonical = Data(vector.outcomeJson.utf8)
            let decoded = try JSONDecoder().decode(
                Wcag22ExplicitSelectionOutcomeV1.self, from: canonical)
            let reencoded = try JSONEncoder().encode(decoded)
            let reencodedTerm = try normalized(reencoded)
            let canonicalTerm = try normalized(canonical)
            XCTAssertEqual(
                reencodedTerm, canonicalTerm,
                "decode→re-encode drifted from the canonical term \(vector.caseId)")
            let redecoded = try JSONDecoder().decode(
                Wcag22ExplicitSelectionOutcomeV1.self, from: reencoded)
            XCTAssertEqual(
                redecoded, decoded,
                "re-encoded bytes must decode back to the same value \(vector.caseId)")
        }
    }

    /// (c) limit+1 отклоняется ДО сырой FFI-копии; typed-путь и скалярный
    /// helper обязаны выдавать один и тот же канонический отказ.
    func testWcag22ExplicitSelectionOversizePreflightMatchesScalarHelper() throws {
        let maxBytes = wcag22ExplicitSelectionMaxBytes()
        XCTAssertEqual(maxBytes, wcag22ExplicitSelectionMaxRequestBytesV1())
        // Кросс-платформенный пин конверта, разделяемый с packages/colors.
        XCTAssertEqual(maxBytes, 3_889_322)

        let requestedBytes = maxBytes + 1
        var rawCalls = 0
        var scalarCalls = 0
        var ffiSubmittedBytes = 0
        let live = Wcag22ExplicitSelectionBridge.live
        let observing = Wcag22ExplicitSelectionBridge(
            maxRequestBytes: live.maxRequestBytes,
            evaluateRaw: { request in
                rawCalls += 1
                ffiSubmittedBytes += request.count
                return try live.evaluateRaw(request)
            },
            envelopeTooLarge: { requested in
                scalarCalls += 1
                return try live.envelopeTooLarge(requested)
            })
        let oversized = Data(repeating: 0x20, count: Int(requestedBytes))
        let outcome = try evaluateWcag22ExplicitSelection(oversized, using: observing)
        XCTAssertEqual(rawCalls, 0, "limit+1 must not materialize/pass the raw FFI vector")
        XCTAssertEqual(scalarCalls, 1)
        XCTAssertEqual(ffiSubmittedBytes, 0)
        guard case let .failure(.transport(.common(.envelopeTooLarge(requested, limit)))) =
            outcome
        else {
            return XCTFail("oversize preflight did not preserve typed protocol failure")
        }
        XCTAssertEqual(requested.value, requestedBytes)
        XCTAssertEqual(limit.value, maxBytes)

        let scalarBytes = try wcag22ExplicitSelectionEnvelopeTooLargeV1(
            requestedBytes: requestedBytes)
        let scalarOutcome = try JSONDecoder().decode(
            Wcag22ExplicitSelectionOutcomeV1.self, from: scalarBytes)
        XCTAssertEqual(
            scalarOutcome, outcome,
            "scalar helper and typed preflight must agree on the canonical failure")
    }

    /// (d) Структурные мутации законного `selected`-терминала обязаны падать
    /// на decode: испорченная ширина партиции, чужой status, потерянный
    /// selection, перевёрнутый бит матрицы, недосчитанная финальная
    /// перепроверка и нарушенный канонический порядок кандидатов.
    func testWcag22ExplicitSelectionTypedConsumerRejectsStructuralMutations() throws {
        let vectors = try loadExplicitSelectionVectors()
        let fixture = try XCTUnwrap(
            vectors.first { $0.caseId == "selected-declared-order-overrides-canonical" })
        let canonicalData = Data(fixture.outcomeJson.utf8)
        XCTAssertNoThrow(
            try JSONDecoder().decode(Wcag22ExplicitSelectionOutcomeV1.self, from: canonicalData),
            "mutation oracle requires a lawful canonical fixture")

        func mutated(
            _ change: (inout [String: Any]) throws -> Void
        ) throws -> Data {
            var root = try XCTUnwrap(
                JSONSerialization.jsonObject(with: canonicalData) as? [String: Any])
            var result = try XCTUnwrap(root["result"] as? [String: Any])
            try change(&result)
            root["result"] = result
            return try JSONSerialization.data(withJSONObject: root, options: [.sortedKeys])
        }
        func mutatedFeasibility(
            _ change: (inout [String: Any]) throws -> Void
        ) throws -> Data {
            try mutated { result in
                var feasibility = try XCTUnwrap(result["feasibility"] as? [String: Any])
                try change(&feasibility)
                result["feasibility"] = feasibility
            }
        }
        func assertDecodeFails(_ data: Data, _ context: String) {
            XCTAssertThrowsError(
                try JSONDecoder().decode(Wcag22ExplicitSelectionOutcomeV1.self, from: data),
                context)
        }

        // Лишний нулевой байт держит padding законным, но ломает ceil(C/8).
        let widePartition = try mutatedFeasibility { feasibility in
            var proof = try XCTUnwrap(feasibility["proof"] as? [String: Any])
            var partition = try XCTUnwrap(proof["partition"] as? [Int])
            partition.append(0)
            proof["partition"] = partition
            feasibility["proof"] = proof
        }
        assertDecodeFails(widePartition, "damaged partition width must fail decode")

        // Чужой терминальный ключ нейтральной feasibility-операции.
        let foreignStatus = try mutated { $0["status"] = "feasible" }
        assertDecodeFails(foreignStatus, "foreign status key must fail decode")

        // Selected без своего sealed-выбора непредставим.
        let lostSelection = try mutated { $0.removeValue(forKey: "selection") }
        assertDecodeFails(lostSelection, "lost selection on selected must fail decode")

        // Первый бит матрицы рассогласовывает партицию с candidate-major LSB0.
        let flippedMatrixBit = try mutatedFeasibility { feasibility in
            var matrix = try XCTUnwrap(feasibility["failureMatrix"] as? [Int])
            matrix[0] ^= 1
            feasibility["failureMatrix"] = matrix
        }
        assertDecodeFails(flippedMatrixBit, "flipped matrix bit must fail decode")

        // Финальная перепроверка обязана покрыть каждое applicable-ребро.
        let partialVerification = try mutated { result in
            var selection = try XCTUnwrap(result["selection"] as? [String: Any])
            var verification = try XCTUnwrap(
                selection["finalVerification"] as? [String: Any])
            verification["verifiedApplicableEdges"] = "0"
            selection["finalVerification"] = verification
            result["selection"] = selection
        }
        assertDecodeFails(
            partialVerification, "under-counted final verification must fail decode")

        // Канонический byte-sorted порядок кандидатов Core-owned.
        let transposedCandidates = try mutatedFeasibility { feasibility in
            var candidates = try XCTUnwrap(feasibility["candidates"] as? [[String: Any]])
            candidates.swapAt(0, 1)
            feasibility["candidates"] = candidates
        }
        assertDecodeFails(transposedCandidates, "transposed candidates must fail decode")
    }

    /// Полная алгебра отказов атомарной операции обязана пережить
    /// encode→decode без потери ветки или payload.
    func testWcag22ExplicitSelectionFailureAlgebraRoundTripsExhaustively() throws {
        let one = DecimalU64V1(1)
        let two = DecimalU64V1(2)
        let rgb = Srgb8BytesV1(red: 1, green: 2, blue: 3)
        let transport: [Wcag22ExplicitSelectionTransportErrorV1] = [
            .common(.envelopeTooLarge(requestedBytes: two, limitBytes: one)),
            .common(.malformedEnvelope(.shape)),
            .common(.unsupportedDomainId("future")),
            .unsupportedPolicyKind("best-feasible-v1"),
        ]
        let feasibility: [Wcag22ExplicitSelectionFeasibilityErrorV1] = [
            .emptyCandidateId,
            .emptyCandidates,
            .duplicateCandidateId(candidateId: "twin"),
            .common(.invalidRequest(.emptyRelations)),
            .common(.resourceLimitExceeded(
                profileId: .compileV1,
                dimension: .rawAdjacentEntries,
                requested: two,
                limit: one)),
        ]
        let invalid: [Wcag22ExplicitSelectionInvalidRequestV1] = [
            .emptyPolicyId,
            .emptyCandidateOrder,
            .emptyCandidateId,
            .arithmeticOverflow,
            .policyCardinalityExceedsDomain(requested: two, domain: one),
            .foreignCandidateId(candidateId: "foreign"),
            .duplicateCandidateId(candidateId: "twin"),
        ]
        let integrity: [Wcag22ExplicitSelectionIntegrityViolationV1] = [
            .evaluatorContract(
                candidateId: "c", relationId: "r", adjacent: rgb, violation: .inputMismatch),
            .sealedDecisionMismatch(
                candidateId: "c", relationId: "r", adjacent: rgb,
                sealed: .pass, rechecked: .fail),
            .selectedRowNotPassing(candidateId: "c", relationId: "r", adjacent: rgb),
            .applicableEdgeCountMismatch(expected: one, observed: two),
            .sealedTraversalArithmeticOverflow,
        ]
        var selection: [Wcag22ExplicitSelectionErrorV1] = invalid.map { .invalidRequest($0) }
        selection.append(.resourceLimitExceeded(
            profileId: .compileV1,
            dimension: .logicalAssessments,
            requested: two,
            limit: one))
        selection += integrity.map { .integrityViolation($0) }

        let errors: [Wcag22ExplicitSelectionOperationErrorV1] =
            transport.map { .transport($0) }
            + feasibility.map { .feasibility($0) }
            + selection.map { .selection($0) }
            + [.incompatibleCoreContract]
        XCTAssertEqual(errors.count, 23, "new branch requires an explicit test fixture")
        for error in errors {
            let outcome = Wcag22ExplicitSelectionOutcomeV1.failure(error)
            let encoded = try JSONEncoder().encode(outcome)
            let decoded = try JSONDecoder().decode(
                Wcag22ExplicitSelectionOutcomeV1.self, from: encoded)
            XCTAssertEqual(decoded, outcome)
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

struct Wcag22FeasibilityVector: Codable {
    let caseId: String
    let requestJson: String
    let outcomeJson: String
}

struct Wcag22ExplicitSelectionVector: Codable {
    let caseId: String
    let requestJson: String
    let outcomeJson: String
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
