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
    @discardableResult public func nominalSrgb8Visible(_ hard: Bool, id: UInt32, occurrence: UInt32) throws -> Self {
        try append(hard ? 11 : 12) { b in
            b.u32(id); b.u8(12); b.u32(occurrence)
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

/// Включённые границы конечного encoded-sRGB8 domain. Серая ось содержит [g, g, g].
public enum ProgramWireDomainV3 {
    case box(lower: [UInt8], upper: [UInt8])
    case gray(lower: UInt8, upper: UInt8)
}

public enum ProgramWireColorDomainV3: Equatable { case srgb, neutral }

/// Отсутствующая граница сохраняет пакетную политику Core; adapter её не вычисляет.
public struct ProgramWireColorIdentityV3 {
    public let maxHueDeviation: Double?
    public let chroma: [Double]?
    public let lightness: [Double]?
    public init(maxHueDeviation: Double? = nil, chroma: [Double]? = nil, lightness: [Double]? = nil) {
        self.maxHueDeviation = maxHueDeviation; self.chroma = chroma; self.lightness = lightness
    }
}

public struct ProgramWireToneV3 {
    public let from: UInt32
    public let curve: [[Double]]
    public init(from: UInt32, curve: [[Double]]) { self.from = from; self.curve = curve }
}
public enum ProgramWireToneCoordinateV3 {
    case lightness(Double)
    case swatch([UInt8])

    fileprivate var isSwatch: Bool {
        if case .swatch = self { return true }
        return false
    }
}

public struct ProgramWireColorOptionsV3 {
    public let domain: ProgramWireColorDomainV3
    public let identity: ProgramWireColorIdentityV3?
    public let maxChange: Double
    public let appearance: UInt32?
    public let tone: ProgramWireToneV3?
    public let referenceAppearance: UInt32?
    public init(domain: ProgramWireColorDomainV3 = .srgb, identity: ProgramWireColorIdentityV3? = nil,
                maxChange: Double = 2, appearance: UInt32? = nil, tone: ProgramWireToneV3? = nil,
                referenceAppearance: UInt32? = nil) {
        self.domain = domain; self.identity = identity; self.maxChange = maxChange
        self.appearance = appearance; self.tone = tone; self.referenceAppearance = referenceAppearance
    }
}

public struct ProgramWireReferenceInputV3 {
    public let port: UInt32
    public let rgb: [UInt8]
    public init(port: UInt32, rgb: [UInt8]) { self.port = port; self.rgb = rgb }
}

public struct ProgramWireAssignmentOptionsV3 {
    /// nil omits the reference; an empty array explicitly declares a reference without input ports.
    public let reference: [ProgramWireReferenceInputV3]?
    public init(reference: [ProgramWireReferenceInputV3]? = nil) { self.reference = reference }
}

public struct ProgramWireComposeOptionsV3 {
    /// nil omits the reference; an empty array explicitly declares a reference without input ports.
    public let reference: [ProgramWireReferenceInputV3]?
    public init(reference: [ProgramWireReferenceInputV3]? = nil) { self.reference = reference }
}

/// LCPW v3 передаёт domain, опоры и предпочтения. Выбор и композицию выполняет Core.
public final class ProgramWireBuilderV3: ProgramWireBuilderV1 {
    fileprivate override var wireVersion: UInt16 { 3 }

    private func bounded(_ value: Double, maximum: Double) throws -> Double {
        guard value.isFinite, value >= 0, value <= maximum else {
            throw ProgramWireError.invalidDeclaration("number is outside the declared domain")
        }
        return value == 0 ? 0 : value
    }

    private func pair(_ values: [Double]) throws -> [Double] {
        guard values.count == 2 else { throw ProgramWireError.invalidDeclaration("range requires two bounds") }
        let lower = try bounded(values[0], maximum: 1), upper = try bounded(values[1], maximum: 1)
        guard lower <= upper else { throw ProgramWireError.invalidDeclaration("range bounds must be increasing") }
        return [lower, upper]
    }

    @discardableResult public func colorTarget(_ id: UInt32, anchor: [UInt8], options: ProgramWireColorOptionsV3 = ProgramWireColorOptionsV3()) throws -> Self {
        try appendColorTarget(id, anchor: anchor, options: options, backdropTone: nil)
    }

    /// Кривая от видимой подложки occurrence в options.appearance или options.referenceAppearance.
    @discardableResult public func colorTarget(_ id: UInt32, anchor: [UInt8], options: ProgramWireColorOptionsV3,
                                               backdropTone: [[Double]]) throws -> Self {
        try appendColorTarget(id, anchor: anchor, options: options, backdropTone: backdropTone)
    }

    private func appendColorTarget(_ id: UInt32, anchor: [UInt8], options: ProgramWireColorOptionsV3,
                                   backdropTone: [[Double]]?,
                                   coordinates: [(ProgramWireToneCoordinateV3, ProgramWireToneCoordinateV3)]? = nil,
                                   from: UInt32? = nil) throws -> Self {
        try append(1) { b in
            b.u32(id); b.u8(4); try b.rgb(anchor)
            b.u8(options.domain == .neutral ? 1 : 0)
            let identity = options.identity
            let flags: UInt8 = (identity?.maxHueDeviation == nil ? 0 : 1)
                | (identity?.chroma == nil ? 0 : 2) | (identity?.lightness == nil ? 0 : 4)
            b.u8(flags)
            if let hue = identity?.maxHueDeviation { b.f64(try bounded(hue, maximum: 180)) }
            if let chroma = identity?.chroma { for value in try pair(chroma) { b.f64(value) } }
            if let lightness = identity?.lightness { for value in try pair(lightness) { b.f64(value) } }
            b.f64(try bounded(options.maxChange, maximum: 2))
            guard options.appearance == nil || options.referenceAppearance == nil else {
                throw ProgramWireError.invalidDeclaration("appearance and referenceAppearance are mutually exclusive")
            }
            b.u8(options.referenceAppearance != nil ? 2 : options.appearance != nil ? 1 : 0)
            let appearance = options.referenceAppearance ?? options.appearance
            if let appearance { b.u32(appearance) }
            if let coordinates {
                guard options.tone == nil, from != nil || appearance != nil else {
                    throw ProgramWireError.invalidDeclaration("declare one tone source and an appearance for backdrop tone")
                }
                guard coordinates.count >= 2 else { throw ProgramWireError.invalidDeclaration("tone requires two or more stops") }
                let count = try boundedCount(coordinates.count)
                let sampled = coordinates.contains { $0.0.isSwatch || $0.1.isSwatch }
                let numericInputs = coordinates.allSatisfy { !$0.0.isSwatch }
                b.u8((from == nil ? 2 : 1) + (sampled ? 2 : 0))
                if let from { b.u32(from) }
                b.u32(count)
                var previous: Double = -1
                for (i, point) in coordinates.enumerated() {
                    if numericInputs, case let .lightness(x) = point.0 {
                        guard x > previous, i != 0 || x == 0, i != coordinates.count - 1 || x == 1 else {
                            throw ProgramWireError.invalidDeclaration("tone inputs must increase from 0 to 1")
                        }
                        previous = x
                    }
                    if sampled { b.u8((point.0.isSwatch ? 1 : 0) | (point.1.isSwatch ? 2 : 0)) }
                    func write(_ coordinate: ProgramWireToneCoordinateV3) throws {
                        switch coordinate {
                        case .lightness(let value): b.f64(try bounded(value, maximum: 1))
                        case .swatch(let rgb): try b.rgb(rgb)
                        }
                    }
                    try write(point.0); try write(point.1)
                }
                return
            }
            if backdropTone != nil && (appearance == nil || options.tone != nil) {
                throw ProgramWireError.invalidDeclaration("backdrop tone requires an appearance and only one tone source")
            }
            b.u8(backdropTone != nil ? 2 : options.tone == nil ? 0 : 1)
            if let tone = options.tone { b.u32(tone.from) }
            if let curve = backdropTone ?? options.tone?.curve {
                guard curve.count >= 2 else { throw ProgramWireError.invalidDeclaration("tone requires two or more stops") }
                b.u32(try boundedCount(curve.count))
                var previous: Double?
                for (index, point) in curve.enumerated() {
                    guard point.count == 2 else { throw ProgramWireError.invalidDeclaration("tone stop requires two coordinates") }
                    let x = try bounded(point[0], maximum: 1), y = try bounded(point[1], maximum: 1)
                    guard (index != 0 || x == 0), (index != curve.count - 1 || x == 1),
                          previous.map({ x > $0 }) ?? true else {
                        throw ProgramWireError.invalidDeclaration("tone inputs must increase from 0 to 1")
                    }
                    b.f64(x); b.f64(y); previous = x
                }
            }
        }
        return self
    }

    /// Координата кривой задаётся Oklab L или образцом sRGB8, который проектирует Core.
    @discardableResult public func colorTarget(_ id: UInt32, anchor: [UInt8], options: ProgramWireColorOptionsV3,
                                               tone: [(ProgramWireToneCoordinateV3, ProgramWireToneCoordinateV3)],
                                               from: UInt32? = nil) throws -> Self {
        try appendColorTarget(id, anchor: anchor, options: options, backdropTone: nil, coordinates: tone, from: from)
    }

    @discardableResult public func pointOccurrence(_ id: UInt32, paint: UInt32, surface: UInt32) throws -> Self {
        try append(8) { b in
            b.u32(id); b.u32(paint); b.u32(surface); b.f64(0); b.f64(0); b.u8(0)
        }
        return self
    }

    private func separationBounds(_ minimum: Double, _ maximum: Double) throws -> (Double, Double) {
        let lower = try bounded(minimum, maximum: 2), upper = try bounded(maximum, maximum: 2)
        guard lower <= upper else { throw ProgramWireError.invalidDeclaration("separation bounds must be increasing") }
        return (lower, upper)
    }

    @discardableResult public func visibleSeparationHard(_ id: UInt32, occurrence: UInt32, minimum: Double, maximum: Double) throws -> Self {
        let (lower, upper) = try separationBounds(minimum, maximum)
        try append(11) { b in
            b.u32(id); b.u8(11); b.u32(occurrence); b.f64(lower); b.f64(upper)
        }
        return self
    }

    @discardableResult public func visiblePairSeparationHard(_ id: UInt32, left: UInt32, right: UInt32, minimum: Double, maximum: Double) throws -> Self {
        guard left != right else { throw ProgramWireError.invalidDeclaration("pair occurrences must be distinct") }
        let (lower, upper) = try separationBounds(minimum, maximum)
        try append(11) { b in
            b.u32(id); b.u8(14); b.u32(left); b.u32(right); b.f64(lower); b.f64(upper)
        }
        return self
    }

    @discardableResult public func computedTarget(_ id: UInt32, domain: ProgramWireDomainV3, anchor: [UInt8], maxDelta: Double) throws -> Self {
        guard maxDelta.isFinite, maxDelta >= 0, (maxDelta * maxDelta).isFinite else {
            throw ProgramWireError.invalidDeclaration("maxDelta must be nonnegative with a finite square")
        }
        try append(1) { b in
            b.u32(id); b.u8(3)
            switch domain {
            case let .box(lower, upper):
                guard lower.count == 3, upper.count == 3, zip(lower, upper).allSatisfy({ $0.0 <= $0.1 }) else {
                    throw ProgramWireError.invalidDeclaration("box requires ordered RGB bounds")
                }
                b.u8(1); try b.rgb(lower); try b.rgb(upper)
            case let .gray(lower, upper):
                guard lower <= upper else { throw ProgramWireError.invalidDeclaration("gray bounds must be increasing") }
                b.u8(2); b.u8(lower); b.u8(upper)
            }
            try b.rgb(anchor); b.f64(maxDelta == 0 ? 0 : maxDelta)
        }
        return self
    }

    @discardableResult public func preserveTonalHierarchy(_ id: UInt32, occurrences: [UInt32]) throws -> Self {
        guard occurrences.count >= 2 else { throw ProgramWireError.invalidDeclaration("hierarchy requires at least two occurrences") }
        guard occurrences.count <= maximumEntries else { throw ProgramWireError.tooManyEntries }
        guard Set(occurrences).count == occurrences.count else {
            throw ProgramWireError.invalidDeclaration("hierarchy occurrences must be distinct")
        }
        try append(11) { b in
            b.u32(id); b.u8(13); b.u32(UInt32(occurrences.count))
            for occurrence in occurrences { b.u32(occurrence) }
        }
        return self
    }

    @discardableResult public func assignment(_ targetsInPreferenceOrder: [UInt32],
        options: ProgramWireAssignmentOptionsV3 = ProgramWireAssignmentOptionsV3()) throws -> Self {
        try selectionV3(targetsInPreferenceOrder, reference: options.reference, tag: 3)
    }

    /// Every computed target appears once. Core composes whole physical components;
    /// declaration order only preserves compatibility for implicit-reference components.
    @discardableResult public func compose(_ targets: [UInt32],
        options: ProgramWireComposeOptionsV3 = ProgramWireComposeOptionsV3()) throws -> Self {
        try selectionV3(targets, reference: options.reference, tag: 5)
    }

    /// Balances reference uses on physically independent backdrops.
    @discardableResult public func composeReference(_ targets: [UInt32], reference: [ProgramWireReferenceInputV3]) throws -> Self {
        try selectionV3(targets, reference: reference, tag: 6)
    }

    private func selectionV3(_ targets: [UInt32], reference: [ProgramWireReferenceInputV3]?, tag: UInt8) throws -> Self {
        guard targets.count <= maximumEntries else { throw ProgramWireError.tooManyEntries }
        try selection { b in
            b.u8(reference == nil ? tag : tag + 1); b.u8(0); b.u32(UInt32(targets.count))
            for target in targets { b.u32(target) }
            if let reference = reference {
                guard reference.count <= maximumEntries else { throw ProgramWireError.tooManyEntries }
                var ports = Set<UInt32>()
                b.u32(UInt32(reference.count))
                for input in reference {
                    guard ports.insert(input.port).inserted else {
                        throw ProgramWireError.invalidDeclaration("reference input ports must be distinct")
                    }
                    b.u32(input.port); try b.rgb(input.rgb)
                }
            }
        }
        return self
    }
}
