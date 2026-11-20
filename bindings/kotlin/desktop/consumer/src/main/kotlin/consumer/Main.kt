package consumer

import pics.lab.colors.*

private fun String.hex() = chunked(2).map { it.toInt(16).toByte() }.toByteArray()

fun main() {
    // Only the installed Maven package is on this application's classpath.
    // No jna.library.path, Rust checkout, generated source or local native library.
    val held = mutableListOf<ProgramSnapshot>()
    val tables = mutableListOf<List<Double>>()
    compileProgramWire(adaptiveWire.hex(), UInt.MAX_VALUE).use { runtime ->
        var expectedTable: List<Double> = emptyList()
        try {
            for (row in history) {
                val snapshot = if (row.reason != null) runtime.updateUnknown(row.revision, row.reason)
                else runtime.updateObserved(row.revision, row.ids, row.background, 1u)
                held.add(snapshot)
                check(snapshot.state() == row.state)
                row.rgb?.let {
                    check(snapshot.outputCount() == 1uL)
                    check(snapshot.outputSlot(0u) == 91u)
                    check(snapshot.outputRgb(0u).contentEquals(it))
                    check(snapshot.outputOpacity(0u) == 1.0)
                    expectedTable = listOf(91.0) + it.map { channel -> (channel.toInt() and 255).toDouble() } + 1.0
                }
                val table: List<Double> = snapshot.copyOutputTable()
                check(table == expectedTable)
                tables.add(table)
            }
            val first = held.first()
            first.outputRgb(0u)[0] = 123
            check(first.outputRgb(0u).contentEquals(byteArrayOf(0, 0, 0)))
            try {
                first.outputRgb(1u)
                error("out-of-range read accepted")
            } catch (failure: LabColorsException.Refused) {
                check(failure.code == "invalid_output_index")
            }
        } finally {
            held.forEach { it.close() }
        }
    }
    check(tables.first() == listOf(91.0, 0.0, 0.0, 0.0, 1.0))
    val declared = ProgramWireBuilderV1().apply {
        source(11u, byteArrayOf(20, 20, 20)); fixedTarget(21u, 11u)
        surfaceInputPort(31u); solidPaint(41u, 21u); inputSurface(51u, 31u)
        sourceOverOccurrence(61u, 41u, 51u, 64.0, 0.2, ProgramSurround.AVERAGE)
        presentationRoot(71u, 61u); presentationTarget(71u, 61u)
        wcag22VisibleUnary(true, 81u, 61u, Wcag22Criterion.SC143_TEXT_DEFAULT)
        output(91u, 41u)
    }.finish()
    check(declared.contentEquals(fixedWire.hex()))
    val closed = compileProgramWire(declared, 1u)
    closed.close(); closed.close()
    val afterClose = runCatching { closed.updateUnknown(2uL, 7u) }
    afterClose.getOrNull()?.close()
    check(afterClose.exceptionOrNull() is IllegalStateException) { "disposed Program was usable" }
    println("Installed desktop Program: history, RGB, opacity, owned output tables, retained snapshot, builder and disposal passed")
}
