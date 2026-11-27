package pics.lab.colors

/** One independently varying input field in an atomic multi-field update. */
data class ProgramIndependentGradient(
    val inputIndex: UInt,
    val stops: List<ProgramGradientStop>,
)

private fun List<ProgramGradientStop>.rgbBytes(): ByteArray {
    require(size <= Int.MAX_VALUE / 3) { "Gradient RGB8 buffer exceeds the JVM array size" }
    val bytes = ByteArray(size * 3)
    forEachIndexed { i, stop ->
        bytes[i * 3] = stop.color.red.toByte()
        bytes[i * 3 + 1] = stop.color.green.toByte()
        bytes[i * 3 + 2] = stop.color.blue.toByte()
    }
    return bytes
}

/** The native runtime admits and solves all authored stops together. */
fun ProgramRuntime.updateGradient(
    revision: ULong, surfaces: ByteArray, varyingInput: UInt, stops: List<ProgramGradientStop>
): ProgramSnapshot = updateGradient(revision, surfaces, varyingInput, stops.map { it.at }, stops.rgbBytes())

fun ProgramRuntime.updateGradientWith(
    revision: ULong, surfaces: ByteArray, varyingInput: UInt, stops: List<ProgramGradientStop>, accept: ProgramUpdateHost
): Boolean = updateGradientWith(revision, surfaces, varyingInput, stops.map { it.at }, stops.rgbBytes(), accept)

private data class IndependentGradientPayload(
    val inputs: List<UInt>,
    val offsets: List<UInt>,
    val positions: List<Double>,
    val colors: ByteArray,
)

private fun List<ProgramIndependentGradient>.payload(): IndependentGradientPayload {
    require(size <= UInt.MAX_VALUE.toLong()) { "Too many independent gradient fields" }
    var stopCount = 0L
    val inputs = ArrayList<UInt>(size)
    val offsets = ArrayList<UInt>(size + 1)
    val positions = ArrayList<Double>()
    var previousInput: UInt? = null
    offsets += 0u
    for (field in this) {
        require(previousInput == null || field.inputIndex > previousInput) {
            "Independent gradient input indices must be strictly increasing"
        }
        require(field.stops.isNotEmpty()) { "Independent gradient fields must contain stops" }
        stopCount += field.stops.size
        require(stopCount <= UInt.MAX_VALUE.toLong() && stopCount <= Int.MAX_VALUE / 3) {
            "Independent gradient stop data exceeds the supported size"
        }
        inputs += field.inputIndex
        positions.addAll(field.stops.map { it.at })
        offsets += stopCount.toUInt()
        previousInput = field.inputIndex
    }
    val colors = ByteArray(stopCount.toInt() * 3)
    var colorOffset = 0
    for (field in this) for (stop in field.stops) {
        colors[colorOffset++] = stop.color.red.toByte()
        colors[colorOffset++] = stop.color.green.toByte()
        colors[colorOffset++] = stop.color.blue.toByte()
    }
    return IndependentGradientPayload(inputs, offsets, positions, colors)
}

/** Updates all independent fields in one revision and one host transaction. */
fun ProgramRuntime.updateIndependentGradients(
    revision: ULong, surfaces: ByteArray, fields: List<ProgramIndependentGradient>
): ProgramSnapshot {
    val payload = fields.payload()
    return updateIndependentGradients(revision, surfaces, payload.inputs, payload.offsets, payload.positions, payload.colors)
}

/** The host accepts or rejects every independent field as one composition. */
fun ProgramRuntime.updateIndependentGradientsWith(
    revision: ULong, surfaces: ByteArray, fields: List<ProgramIndependentGradient>, accept: ProgramUpdateHost
): Boolean {
    val payload = fields.payload()
    return updateIndependentGradientsWith(revision, surfaces, payload.inputs, payload.offsets, payload.positions, payload.colors, accept)
}
