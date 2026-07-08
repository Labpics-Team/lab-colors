import Foundation
import XCTest

@testable import LabColors

/// Conformance-прогон нативного (Swift/UniFFI) биндинга против закоммиченного
/// пака `conformance/vectors/*.json`. Доказывает: рантайм-ядро Rust, вызванное
/// с Swift-стороны, воспроизводит канон. Числовые поля сверяются в пределах
/// `driftTol` (= `DRIFT_TOL` ядра, 1e-6: кросс-платформенный libm-шум ~1e-13,
/// реальный дрейф — целые единицы). Композит-hex — чистая IEEE-алгебра, точен;
/// solve-hex — квантование трансцендентного резолва, допускается ±1 LSD/канал.
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

    // MARK: - Семейство: мутность

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
