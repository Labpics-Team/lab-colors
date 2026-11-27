import Foundation

/// One independently varying input field in an atomic multi-field update.
public struct ProgramIndependentGradient {
    public let inputIndex: UInt32
    public let stops: [ProgramGradientStop]

    public init(inputIndex: UInt32, stops: [ProgramGradientStop]) {
        self.inputIndex = inputIndex
        self.stops = stops
    }
}

private extension Array where Element == ProgramGradientStop {
    var rgbBytes: Data {
        var bytes = Data(capacity: count * 3)
        for stop in self {
            bytes.append(stop.color.red)
            bytes.append(stop.color.green)
            bytes.append(stop.color.blue)
        }
        return bytes
    }
}

public extension ProgramRuntime {
    /// The native runtime admits and solves all authored stops together.
    func updateGradient(revision: UInt64, surfaces: Data, varyingInput: UInt32, stops: [ProgramGradientStop]) throws -> ProgramSnapshot {
        try updateGradient(revision: revision, surfaces: surfaces, varyingInput: varyingInput, positions: stops.map(\.at), colors: stops.rgbBytes)
    }

    func updateGradientWith(revision: UInt64, surfaces: Data, varyingInput: UInt32, stops: [ProgramGradientStop], accept: ProgramUpdateHost) throws -> Bool {
        try updateGradientWith(revision: revision, surfaces: surfaces, varyingInput: varyingInput, positions: stops.map(\.at), colors: stops.rgbBytes, accept: accept)
    }

    /// Updates all independent fields in one revision and one host transaction.
    func updateIndependentGradients(revision: UInt64, surfaces: Data, fields: [ProgramIndependentGradient]) throws -> ProgramSnapshot {
        let payload = try fields.independentPayload
        return try updateIndependentGradients(revision: revision, surfaces: surfaces, inputs: payload.inputs,
            offsets: payload.offsets, positions: payload.positions, colors: payload.colors)
    }

    /// The host accepts or rejects every independent field as one composition.
    func updateIndependentGradientsWith(revision: UInt64, surfaces: Data, fields: [ProgramIndependentGradient], accept: ProgramUpdateHost) throws -> Bool {
        let payload = try fields.independentPayload
        return try updateIndependentGradientsWith(revision: revision, surfaces: surfaces, inputs: payload.inputs,
            offsets: payload.offsets, positions: payload.positions, colors: payload.colors, accept: accept)
    }
}

private struct IndependentGradientPayload {
    let inputs: [UInt32]
    let offsets: [UInt32]
    let positions: [Double]
    let colors: Data
}

private extension Array where Element == ProgramIndependentGradient {
    var independentPayload: IndependentGradientPayload {
        get throws {
            var inputs: [UInt32] = []
            var offsets: [UInt32] = [0]
            var positions: [Double] = []
            var colors = Data()
            inputs.reserveCapacity(count)
            offsets.reserveCapacity(count + 1)
            var previousInput: UInt32?
            var stopCount: UInt32 = 0
            for field in self {
                guard previousInput.map({ field.inputIndex > $0 }) ?? true,
                      !field.stops.isEmpty,
                      field.stops.count <= Int(UInt32.max - stopCount),
                      field.stops.count <= (Int.max - colors.count) / 3 else {
                    throw ProgramWireError.invalidDeclaration("Independent gradient fields must be ordered, non-empty, and fit the native input limits")
                }
                inputs.append(field.inputIndex)
                positions.append(contentsOf: field.stops.map(\.at))
                colors.append(contentsOf: field.stops.rgbBytes)
                stopCount += UInt32(field.stops.count)
                offsets.append(stopCount)
                previousInput = field.inputIndex
            }
            return IndependentGradientPayload(inputs: inputs, offsets: offsets, positions: positions, colors: colors)
        }
    }
}
