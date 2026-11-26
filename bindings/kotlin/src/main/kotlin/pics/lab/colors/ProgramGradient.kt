package pics.lab.colors

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
