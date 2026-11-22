package pics.lab.colors.conformance

import pics.lab.colors.*

private fun rgb(vararg channels: Int) = channels.map { it.toByte() }.toByteArray()
private fun String.wireBytes() = chunked(2).map { it.toInt(16).toByte() }.toByteArray()
private fun bytesEqual(expected: ByteArray, actual: ByteArray) = check(expected.contentEquals(actual))

private fun contextualOptions() = ProgramWireColorOptionsV3(
    identity = ProgramWireColorIdentityV3(12.0, 0.02 to 0.35, 0.2 to 0.9),
    maxChange = 1.5, appearance = 61u,
    tone = ProgramWireToneV3(31u, listOf(0.0 to 0.25, 0.5 to 0.55, 1.0 to 0.9)),
)

private fun graph(builder: ProgramWireBuilderV3, contrast: Boolean = true) = builder.apply {
    surfaceInputPort(31u); solidPaint(41u, 21u); inputSurface(51u, 31u)
    pointOccurrence(61u, 41u, 51u)
    if (contrast) wcag22VisibleUnary(true, 81u, 61u, Wcag22Criterion.SC143_TEXT_DEFAULT)
    output(91u, 41u)
}

/** The same external consumer checks run against installed Android and JVM packages. */
fun verifyProgramAuthoringV3(fixtures: Map<String, String>) {
    val box = graph(ProgramWireBuilderV3().apply {
        computedTarget(21u, ProgramWireDomainV3.Box(rgb(0, 0, 0), rgb(15, 15, 15)), rgb(8, 9, 10), 2.0)
    }).assignment(listOf(21u)).finish()
    val gray = graph(ProgramWireBuilderV3().apply {
        computedTarget(21u, ProgramWireDomainV3.Gray(0u, 255u), rgb(128, 128, 128), 2.0)
    }).assignment(listOf(21u)).finish()
    val contextual = graph(ProgramWireBuilderV3().apply {
        colorTarget(21u, rgb(0, 103, 244), contextualOptions())
    }, contrast = false).apply {
        visibleSeparationHard(81u, 61u, 0.05, 0.8)
    }.assignment(listOf(21u)).finish()
    val fixed = graph(ProgramWireBuilderV3().apply {
        source(11u, rgb(0, 0, 0)); fixedTarget(21u, 11u)
    }).assignment(emptyList()).finish()
    val encodingOnly = ProgramWireBuilderV3().apply {
        colorTarget(21u, rgb(5, 7, 9))
        colorTarget(22u, rgb(80, 80, 80), ProgramWireColorOptionsV3(domain = ProgramWireColorDomainV3.NEUTRAL))
        colorTarget(23u, rgb(90, 90, 90), contextualOptions().copy(domain = ProgramWireColorDomainV3.NEUTRAL))
        computedTarget(24u, ProgramWireDomainV3.Box(rgb(1, 2, 3), rgb(4, 5, 6)), rgb(7, 8, 9), -0.0)
        computedTarget(25u, ProgramWireDomainV3.Gray(11u, 12u), rgb(13, 14, 15), 3.0)
        pointOccurrence(26u, 27u, 28u); visibleSeparationHard(29u, 26u, -0.0, 2.0)
        assignment(emptyList())
    }.finish()
    for ((name, actual) in mapOf("computedBox" to box, "computedGray" to gray,
        "contextual" to contextual, "fixedOnly" to fixed, "encodingOnly" to encodingOnly)) {
        bytesEqual(requireNotNull(fixtures[name]) { "missing independent wire fixture $name" }.wireBytes(), actual)
    }

    val builder = ProgramWireBuilderV3().apply { colorTarget(1u, rgb(5, 7, 9)) }
    val before = builder.finish()
    fun rejects(action: () -> Unit) {
        try { action(); error("invalid declaration was accepted") }
        catch (_: ProgramWireException) { }
        bytesEqual(before, builder.finish())
    }
    rejects { builder.colorTarget(2u, rgb(1, 2)) }
    rejects { builder.colorTarget(2u, rgb(1, 2, 3), ProgramWireColorOptionsV3(maxChange = Double.NaN)) }
    rejects { builder.colorTarget(2u, rgb(1, 2, 3), ProgramWireColorOptionsV3(maxChange = 2.01)) }
    rejects { builder.colorTarget(2u, rgb(1, 2, 3), ProgramWireColorOptionsV3(identity = ProgramWireColorIdentityV3(maxHueDeviation = 181.0))) }
    rejects { builder.colorTarget(2u, rgb(1, 2, 3), ProgramWireColorOptionsV3(identity = ProgramWireColorIdentityV3(chroma = 0.6 to 0.2))) }
    rejects { builder.colorTarget(2u, rgb(1, 2, 3), ProgramWireColorOptionsV3(identity = ProgramWireColorIdentityV3(lightness = 0.0 to Double.POSITIVE_INFINITY))) }
    for (curve in listOf(emptyList(), listOf(0.0 to 0.5), listOf(0.1 to 0.5, 1.0 to 0.9),
        listOf(0.0 to 0.5, 0.0 to 0.7, 1.0 to 0.9), listOf(0.0 to 0.5, 1.0 to 1.1))) {
        rejects { builder.colorTarget(2u, rgb(1, 2, 3), ProgramWireColorOptionsV3(tone = ProgramWireToneV3(31u, curve))) }
    }
    rejects { builder.computedTarget(2u, ProgramWireDomainV3.Box(rgb(1, 2, 8), rgb(3, 4, 7)), rgb(1, 2, 3), 2.0) }
    rejects { builder.computedTarget(2u, ProgramWireDomainV3.Gray(30u, 20u), rgb(1, 2, 3), 2.0) }
    rejects { builder.computedTarget(2u, ProgramWireDomainV3.Gray(0u, 255u), rgb(1, 2, 3), Double.MAX_VALUE) }
    rejects { builder.visibleSeparationHard(3u, 61u, 0.5, 0.4) }
    rejects { builder.assignment(List(4097) { 1u }) }
    val reentrant = object : AbstractList<UInt>() {
        override val size = 1
        override fun get(index: Int): UInt {
            builder.source(10u, rgb(1, 2, 3))
            return 1u
        }
    }
    rejects { builder.assignment(reentrant) }
    builder.assignment(listOf(1u))
    val assigned = builder.finish()
    try { builder.assignment(emptyList()); error("duplicate assignment was accepted") }
    catch (_: ProgramWireException.InvalidDeclaration) { }
    bytesEqual(assigned, builder.finish())

    val anchor = rgb(1, 2, 3)
    val curve = mutableListOf(0.0 to 0.2, 1.0 to 0.9)
    val captured = ProgramWireBuilderV3().apply {
        colorTarget(1u, anchor, ProgramWireColorOptionsV3(tone = ProgramWireToneV3(31u, curve)))
    }
    val owned = captured.finish()
    anchor.fill(99); curve[0] = 0.0 to 0.8
    bytesEqual(owned, captured.finish())
    captured.finish().fill(0)
    bytesEqual(owned, captured.finish())

    compileProgramWire(box, 1u).use { runtime ->
        runtime.updateObserved(1uL, listOf(1u), rgb(255, 255, 255), 1u).use { snapshot ->
            check(snapshot.state() == "ready")
            bytesEqual(rgb(8, 9, 10), snapshot.outputRgb(0u))
        }
    }
    compileProgramWire(gray, 1u).use { runtime ->
        val white = runtime.updateObserved(1uL, listOf(1u), rgb(255, 255, 255), 1u)
        val whiteRgb = white.outputRgb(0u)
        val dark = runtime.updateObserved(2uL, listOf(1u), rgb(0, 0, 0), 1u)
        try {
            check(white.snapshotVersion() == 2u && dark.snapshotVersion() == 2u)
            check(white.state() == "ready" && dark.state() == "ready")
            check(!white.outputRgb(0u).contentEquals(dark.outputRgb(0u)))
            for ((snapshot, background) in listOf(white to "#FFFFFF", dark to "#000000")) {
                val channels = snapshot.outputRgb(0u).map { it.toInt() and 255 }
                check(channels.distinct().size == 1)
                val foreground = "#" + channels.joinToString("") { it.toString(16).padStart(2, '0') }
                check(evaluateWcag22(foreground, background, Wcag22Criterion.SC143_TEXT_DEFAULT).decision == Wcag22Decision.PASS)
                val cell = requireNotNull(snapshot.assessmentReport()).cells.single()
                check(cell.hard && cell.constraintId == 81u)
                check((cell.assessment as ProgramConstraintAssessment.Wcag22Srgb8).verdict == ProgramAssessmentVerdict.PASS)
            }
            var proposed: ProgramSnapshot? = null
            val refuse = object : ProgramUpdateHost {
                override fun accept(snapshot: ProgramSnapshot): Boolean {
                    proposed = snapshot
                    check(snapshot.revision() == 3uL)
                    return false
                }
            }
            check(!runtime.updateObservedWith(3uL, listOf(1u), rgb(255, 255, 255), 1u, refuse))
            requireNotNull(proposed).use { candidate -> bytesEqual(white.outputRgb(0u), candidate.outputRgb(0u)) }
            // Same proposed revision with different facts is still a fresh update.
            runtime.updateObserved(3uL, listOf(1u), rgb(0, 0, 0), 1u).use { bytesEqual(dark.outputRgb(0u), it.outputRgb(0u)) }
            val brokenHost = object : ProgramUpdateHost {
                override fun accept(snapshot: ProgramSnapshot): Boolean {
                    snapshot.close()
                    error("foreign host failure before install")
                }
            }
            try {
                runtime.updateObservedWith(4uL, listOf(1u), rgb(255, 255, 255), 1u, brokenHost)
                error("foreign callback exception was ignored")
            } catch (failure: LabColorsException.Refused) {
                check(failure.code == "program_host_protocol")
            }
            runtime.updateObserved(4uL, listOf(1u), rgb(0, 0, 0), 1u).close()
            val accept = object : ProgramUpdateHost {
                override fun accept(snapshot: ProgramSnapshot): Boolean = snapshot.use {
                    bytesEqual(whiteRgb, it.outputRgb(0u))
                    try {
                        runtime.updateUnknown(9uL, 7u).close()
                        error("callback reentered the runtime")
                    } catch (failure: LabColorsException.Refused) { check(failure.code == "program_busy") }
                    true
                }
            }
            check(runtime.updateObservedWith(5uL, listOf(1u), rgb(255, 255, 255), 1u, accept))
            runtime.updateObserved(5uL, listOf(1u), rgb(255, 255, 255), 1u).use {
                check(it.state() == "ready" && it.revision() == 5uL)
                bytesEqual(whiteRgb, it.outputRgb(0u))
            }
            bytesEqual(whiteRgb, white.outputRgb(0u))
        } finally { white.close(); dark.close() }
    }
    compileProgramWire(contextual, 1u).use { runtime ->
        val white = runtime.updateObserved(1uL, listOf(1u), rgb(255, 255, 255), 1u)
        try {
            runtime.updateObserved(2uL, listOf(1u), rgb(0, 0, 0), 1u).use { dark ->
                check(white.state() == "ready" && dark.state() == "ready")
                check(!white.outputRgb(0u).contentEquals(dark.outputRgb(0u)))
                for (snapshot in listOf(white, dark)) {
                    val separation = requireNotNull(snapshot.assessmentReport()).cells.single().assessment as ProgramConstraintAssessment.VisibleSeparation
                    check(separation.verdict == ProgramAssessmentVerdict.PASS)
                    check(separation.distance in separation.minimum..separation.maximum)
                    check(separation.distanceLower in separation.minimum..separation.distance)
                    check(separation.distanceUpper in separation.distance..separation.maximum)
                }
            }
        } finally { white.close() }
    }
    compileProgramWire(fixed, 1u).use { runtime ->
        runtime.updateObserved(1uL, listOf(1u), rgb(255, 255, 255), 1u).use { check(it.state() == "ready") }
        runtime.updateObserved(2uL, listOf(1u), rgb(0, 0, 0), 1u).use {
            check(it.state() == "unsatisfied")
            check(requireNotNull(it.diagnostics()).witness?.violations?.isNotEmpty() == true)
        }
    }
}
