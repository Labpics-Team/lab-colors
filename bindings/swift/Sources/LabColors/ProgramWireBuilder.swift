import Foundation

/// Ошибки авторинга не меняют ранее принятую декларацию. Семантику графа проверяет Core.
public enum ProgramWireError: Error, Equatable {
    case invalidDeclaration(String)
    case tooManyEntries
}

public struct ProgramCandidate {
    public let id: UInt32
    public let rgb: [UInt8]
    public let opacity: Double
    public init(id: UInt32, rgb: [UInt8], opacity: Double) {
        self.id = id; self.rgb = rgb; self.opacity = opacity
    }
}

public struct ProgramSelectionChoice {
    public let target: UInt32
    public let candidate: UInt32
    public init(target: UInt32, candidate: UInt32) {
        self.target = target; self.candidate = candidate
    }
}

public struct ProgramSelectionState {
    public let id: UInt32
    public let choices: [ProgramSelectionChoice]
    public init(id: UInt32, choices: [ProgramSelectionChoice]) {
        self.id = id; self.choices = choices
    }
}

public enum ProgramSurround: UInt8 { case average = 1, dim = 2, dark = 3 }

/// Включённые границы encoded sRGB8. Область не определяет смысл цели.
public enum ProgramWireDomainV3 {
    case box(lower: [UInt8], upper: [UInt8])
    case gray(lower: UInt8, upper: UInt8)
}

fileprivate struct WireBytes {
    var bytes: [UInt8] = []
    mutating func u8(_ value: UInt8) { bytes.append(value) }
    mutating func u16(_ value: UInt16) { for shift in stride(from: 0, to: 16, by: 8) { u8(UInt8(truncatingIfNeeded: value >> shift)) } }
    mutating func u32(_ value: UInt32) { for shift in stride(from: 0, to: 32, by: 8) { u8(UInt8(truncatingIfNeeded: value >> shift)) } }
    mutating func u64(_ value: UInt64) { for shift in stride(from: 0, to: 64, by: 8) { u8(UInt8(truncatingIfNeeded: value >> shift)) } }
    mutating func f64(_ value: Double) { u64(value.bitPattern) }
    mutating func rgb(_ value: [UInt8]) throws {
        guard value.count == 3 else { throw ProgramWireError.invalidDeclaration("RGB requires three channels") }
        bytes.append(contentsOf: value)
    }
}

private struct WireSection {
    var count: UInt32 = 0
    var payload = WireBytes()
}

/// Канонический LCPW v1. Builder только кодирует декларацию, не выбирает цвета.
/// Один изменяемый builder принадлежит одному вызывающему контексту.
public class ProgramWireBuilderV1 {
    private var sections = Array(repeating: WireSection(), count: 14)
    private var encodedLength = 10 + 4 * 14
    fileprivate var wireVersion: UInt16 { 1 }
    fileprivate let maximumEntries = 4096
    public init() {}

    // Секция владеет единственным растущим буфером. Неудачное объявление
    // откатывает только собственный хвост; прежние снимки остаются независимыми.
    fileprivate func append(_ index: Int, _ encode: (inout WireBytes) throws -> Void) throws {
        guard sections[index].count < UInt32(maximumEntries) else { throw ProgramWireError.tooManyEntries }
        let previous = sections[index].payload.bytes.count
        do {
            try encode(&sections[index].payload)
            let length = encodedLength.addingReportingOverflow(sections[index].payload.bytes.count - previous)
            guard !length.overflow, length.partialValue <= Int(UInt32.max) else {
                throw ProgramWireError.tooManyEntries
            }
            encodedLength = length.partialValue
            sections[index].count += 1
        } catch {
            sections[index].payload.bytes.removeSubrange(previous...)
            throw error
        }
    }

    fileprivate func selection(_ encode: (inout WireBytes) throws -> Void) throws {
        guard sections[3].count == 0 else { throw ProgramWireError.invalidDeclaration("selection release already exists") }
        try append(3, encode)
    }

    fileprivate func boundedCount(_ count: Int) throws -> UInt32 {
        guard count > 0 else { throw ProgramWireError.invalidDeclaration("list must not be empty") }
        guard count <= maximumEntries else { throw ProgramWireError.tooManyEntries }
        return UInt32(count)
    }

    private func opacity(_ value: Double) throws -> Double {
        guard value.isFinite, value >= 0, value <= 1 else { throw ProgramWireError.invalidDeclaration("opacity must be in [0, 1]") }
        return value == 0 ? 0 : value
    }

    @discardableResult public func source(_ id: UInt32, rgb: [UInt8]) throws -> Self {
        try append(0) { b in
            b.u32(id); try b.rgb(rgb)
        }
        return self
    }
    @discardableResult public func fixedTarget(_ id: UInt32, source: UInt32) throws -> Self {
        try append(1) { b in
            b.u32(id); b.u8(1); b.u32(source)
        }
        return self
    }
    @discardableResult public func finiteTarget(_ id: UInt32, candidates: [ProgramCandidate]) throws -> Self {
        try append(1) { b in
            b.u32(id); b.u8(2); b.u32(try boundedCount(candidates.count))
            for candidate in candidates {
                b.u32(candidate.id)
                try b.rgb(candidate.rgb)
                b.f64(try opacity(candidate.opacity))
            }
        }
        return self
    }
    @discardableResult public func family(_ id: UInt32, releaseBytes: Data) throws -> Self {
        guard releaseBytes.count == 32 else { throw ProgramWireError.invalidDeclaration("family release requires 32 bytes") }
        try append(2) { b in
            b.u32(id); b.bytes.append(contentsOf: releaseBytes)
        }
        return self
    }
    @discardableResult public func surfaceInputPort(_ id: UInt32) throws -> Self {
        try append(4) { b in
            b.u32(id)
        }
        return self
    }
    @discardableResult public func opacityInput(_ id: UInt32, value: Double) throws -> Self {
        try append(5) { b in
            b.u32(id); b.f64(try opacity(value))
        }
        return self
    }
    @discardableResult public func solidPaint(_ id: UInt32, target: UInt32) throws -> Self {
        try append(6) { b in
            b.u32(id); b.u8(1); b.u32(target)
        }
        return self
    }
    @discardableResult public func opacityPaint(_ id: UInt32, source: UInt32, opacity: UInt32) throws -> Self {
        try append(6) { b in
            b.u32(id); b.u8(2); b.u32(source); b.u32(opacity)
        }
        return self
    }
    @discardableResult public func inputSurface(_ id: UInt32, input: UInt32) throws -> Self {
        try append(7) { b in
            b.u32(id); b.u8(1); b.u32(input)
        }
        return self
    }
    @discardableResult public func occurrenceSurface(_ id: UInt32, occurrence: UInt32) throws -> Self {
        try append(7) { b in
            b.u32(id); b.u8(2); b.u32(occurrence)
        }
        return self
    }
    @discardableResult public func sourceOverOccurrence(_ id: UInt32, subject: UInt32, against: UInt32, adaptingLuminance: Double, backgroundRatio: Double, surround: ProgramSurround) throws -> Self {
        guard adaptingLuminance.isFinite, adaptingLuminance > 0, backgroundRatio.isFinite,
              backgroundRatio > 0, backgroundRatio <= 1 else {
            throw ProgramWireError.invalidDeclaration("invalid viewing conditions")
        }
        try append(8) { b in
            b.u32(id); b.u32(subject); b.u32(against)
            b.f64(adaptingLuminance); b.f64(backgroundRatio); b.u8(surround.rawValue)
        }
        return self
    }
    @discardableResult public func presentationRoot(_ id: UInt32, terminal: UInt32) throws -> Self {
        try append(9) { b in
            b.u32(id); b.u32(terminal)
        }
        return self
    }
    @discardableResult public func presentationTarget(_ root: UInt32, occurrence: UInt32) throws -> Self {
        try append(10) { b in
            b.u32(root); b.u32(occurrence)
        }
        return self
    }
    @discardableResult public func exactVisibleUnary(_ hard: Bool, id: UInt32, occurrence: UInt32, expectedRgb: [UInt8]) throws -> Self {
        try append(hard ? 11 : 12) { b in
            b.u32(id); b.u8(1); b.u32(occurrence); try b.rgb(expectedRgb)
        }
        return self
    }
    @discardableResult public func wcag22VisibleUnary(_ hard: Bool, id: UInt32, occurrence: UInt32, criterion: Wcag22Criterion) throws -> Self {
        let key: UInt8
        switch criterion {
        case .sc143TextDefault: key = 1
        case .sc143TextLargeScale: key = 2
        case .sc1411UiComponentOrState: key = 3
        case .sc1411GraphicalObject: key = 4
        }
        try append(hard ? 11 : 12) { b in
            b.u32(id); b.u8(9); b.u32(occurrence); b.u8(key)
        }
        return self
    }
    @discardableResult public func declaredSrgb8CleanSet(_ hard: Bool, id: UInt32, root: UInt32, occurrence: UInt32) throws -> Self {
        try append(hard ? 11 : 12) { b in
            b.u32(id); b.u8(10); b.u32(root); b.u32(occurrence)
        }
        return self
    }
    @discardableResult public func exactIntrinsicRelationHard(_ id: UInt32, reference: UInt32, candidates: [UInt32]) throws -> Self {
        try append(11) { b in
            b.u32(id); b.u8(4); b.u32(reference); b.u32(try boundedCount(candidates.count))
            for candidate in candidates { b.u32(candidate) }
        }
        return self
    }
    @discardableResult public func output(_ slot: UInt32, paint: UInt32) throws -> Self {
        try append(13) { b in
            b.u32(slot); b.u32(paint)
        }
        return self
    }

    public func finish() throws -> Data {
        var out = WireBytes()
        out.bytes.reserveCapacity(encodedLength)
        out.bytes.append(contentsOf: [0x4c, 0x43, 0x50, 0x57])
        out.u16(wireVersion); out.u32(UInt32(encodedLength))
        for section in sections {
            out.u32(section.count)
            out.bytes.append(contentsOf: section.payload.bytes)
        }
        return Data(out.bytes)
    }

}

/// LCPW v2 добавляет явное предпочтение полных состояний без второго решателя.
public final class ProgramWireBuilderV2: ProgramWireBuilderV1 {
    fileprivate override var wireVersion: UInt16 { 2 }
    @discardableResult public func selectionRelease(_ revision: UInt64, rankGroups: [[ProgramSelectionState]]) throws -> Self {
        try selection { b in
            b.u64(revision); b.u32(try boundedCount(rankGroups.count))
            var states = 0, choices = 0
            for group in rankGroups {
                b.u32(try boundedCount(group.count)); states += group.count
                guard states <= maximumEntries else { throw ProgramWireError.tooManyEntries }
                for state in group {
                    b.u32(state.id); b.u32(try boundedCount(state.choices.count)); choices += state.choices.count
                    guard choices <= maximumEntries else { throw ProgramWireError.tooManyEntries }
                    for choice in state.choices { b.u32(choice.target); b.u32(choice.candidate) }
                }
            }
        }
        return self
    }
}

/// LCPW v3 объявляет области и предпочтения; конкретные цвета назначает Core.
public final class ProgramWireBuilderV3: ProgramWireBuilderV1 {
    fileprivate override var wireVersion: UInt16 { 3 }

    @discardableResult public func computedTarget(_ id: UInt32, domain: ProgramWireDomainV3, anchor: [UInt8], maxDelta: Double) throws -> Self {
        guard maxDelta.isFinite, maxDelta >= 0, (maxDelta * maxDelta).isFinite else {
            throw ProgramWireError.invalidDeclaration("maxDelta and its square must be finite and nonnegative")
        }
        try append(1) { b in
            b.u32(id); b.u8(3)
            switch domain {
            case let .box(lower, upper):
                guard lower.count == 3, upper.count == 3,
                      zip(lower, upper).allSatisfy({ $0 <= $1 }) else {
                    throw ProgramWireError.invalidDeclaration("invalid box bounds")
                }
                b.u8(1); try b.rgb(lower); try b.rgb(upper)
            case let .gray(lower, upper):
                guard lower <= upper else { throw ProgramWireError.invalidDeclaration("invalid gray bounds") }
                b.u8(2); b.u8(lower); b.u8(upper)
            }
            try b.rgb(anchor); b.f64(maxDelta == 0 ? 0 : maxDelta)
        }
        return self
    }

    @discardableResult public func assignment(_ targetsInPreferenceOrder: [UInt32]) throws -> Self {
        try selection { b in
            b.u16(1); b.u32(try boundedCount(targetsInPreferenceOrder.count))
            for target in targetsInPreferenceOrder { b.u32(target) }
        }
        return self
    }
}
