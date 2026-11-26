import Foundation

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
}
