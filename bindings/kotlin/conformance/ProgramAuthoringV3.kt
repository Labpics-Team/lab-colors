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

private fun referenceProgram(hierarchy: Boolean) = ProgramWireBuilderV3().apply {
    colorTarget(10u, rgb(68, 68, 68), ProgramWireColorOptionsV3(referenceAppearance = 5u))
    if (hierarchy) colorTarget(20u, rgb(102, 102, 102), ProgramWireColorOptionsV3(referenceAppearance = 15u))
    assignment(if (hierarchy) listOf(10u, 20u) else listOf(10u),
        ProgramWireAssignmentOptionsV3(listOf(ProgramWireReferenceInputV3(2u, rgb(255, 255, 255)))))
    surfaceInputPort(2u); solidPaint(3u, 10u); inputSurface(4u, 2u); pointOccurrence(5u, 3u, 4u)
    if (hierarchy) {
        solidPaint(13u, 20u); pointOccurrence(15u, 13u, 4u); preserveTonalHierarchy(30u, listOf(5u, 15u))
    }
    wcag22VisibleUnary(true, 6u, 5u, Wcag22Criterion.SC143_TEXT_DEFAULT); output(7u, 3u)
    if (hierarchy) { wcag22VisibleUnary(true, 16u, 15u, Wcag22Criterion.SC143_TEXT_DEFAULT); output(17u, 13u) }
}.finish()

private fun verifyAuthoredComposition() {
    fun wire(reverse: Boolean, tone: Boolean) = ProgramWireBuilderV3().apply {
        val ids = if (reverse) listOf(20u, 10u) else listOf(10u, 20u)
        for (id in ids) {
            val channel = if (id == 10u) 68 else 102
            colorTarget(id, rgb(channel, channel, channel), ProgramWireColorOptionsV3(
                domain = ProgramWireColorDomainV3.NEUTRAL,
                referenceAppearance = if (id == 10u) 5u else 15u,
                tone = if (tone) ProgramWireToneV3(2u, listOf(0.0 to 0.7, 1.0 to 0.7)) else null,
            ))
        }
        compose(ids, ProgramWireComposeOptionsV3(listOf(ProgramWireReferenceInputV3(2u, rgb(255, 255, 255)))))
        surfaceInputPort(2u); inputSurface(4u, 2u)
        solidPaint(3u, 10u); pointOccurrence(5u, 3u, 4u)
        solidPaint(13u, 20u); pointOccurrence(15u, 13u, 4u)
        preserveTonalHierarchy(30u, listOf(5u, 15u))
        wcag22VisibleUnary(true, 6u, 5u, Wcag22Criterion.SC143_TEXT_DEFAULT)
        wcag22VisibleUnary(true, 16u, 15u, Wcag22Criterion.SC143_TEXT_DEFAULT)
        output(7u, 3u); output(17u, 13u)
    }.finish()
    var expected: List<Double>? = null
    for (reverse in listOf(false, true)) {
        compileProgramWire(wire(reverse, true), 1u).use { runtime ->
            val first = runtime.updateObserved(1uL, listOf(1u), rgb(0, 0, 0), 1u)
            try {
                check(first.state() == "ready" && first.optimality() == "unfinished")
                val components = requireNotNull(first.selectionComponents())
                check(components == requireNotNull(first.assessmentReport()).selectionComponents)
                check(components.single().mode == ProgramSelectionMode.JOINT_POINT_V1)
                check(components.single().targets.toSet() == setOf(10u, 20u))
                val references = requireNotNull(first.referenceSelections())
                check(references.size == 2 && references.all {
                    it.preference == ProgramReferencePreference.EXPLICIT_TONE && it.capacity == null
                })
                val table = first.copyOutputTable()
                expected?.let { check(it == table) }
                expected = table
                runtime.updateUnknown(2uL, 7u).use { check(it.selectionComponents() == null) }
                check(first.selectionComponents() == components && first.copyOutputTable() == table)
            } finally { first.close() }
        }
    }
    compileProgramWire(wire(false, false), 1u).use { runtime ->
        runtime.updateObserved(1uL, listOf(1u), rgb(0, 0, 0), 1u).use { snapshot ->
            check(snapshot.state() == "ready")
            check(requireNotNull(snapshot.selectionComponents()).single().mode == ProgramSelectionMode.ORDERED_REFERENCE_V1)
        }
    }
}

private fun verifyPublishedV03AuthoringAbi() {
    // Эти JVM-сигнатуры взяты из опубликованного SDK v0.3: пересборка Kotlin-клиента скрыла бы их потерю.
    val optionsType = ProgramWireColorOptionsV3::class.java
    val fields = arrayOf(ProgramWireColorDomainV3::class.java, ProgramWireColorIdentityV3::class.java,
        Double::class.javaObjectType, UInt::class.java, ProgramWireToneV3::class.java)
    val marker = Class.forName("kotlin.jvm.internal.DefaultConstructorMarker")
    val legacy = optionsType.getConstructor(*fields, marker).newInstance(
        ProgramWireColorDomainV3.NEUTRAL, null, 1.5, 0u, null, null) as ProgramWireColorOptionsV3
    check(legacy.domain == ProgramWireColorDomainV3.NEUTRAL && legacy.appearance == 0u)
    check(legacy.referenceAppearance == null)
    val defaults = optionsType.getConstructor(*fields, Int::class.javaPrimitiveType!!, marker).newInstance(
        null, null, null, null, null, 31, null) as ProgramWireColorOptionsV3
    check(defaults == ProgramWireColorOptionsV3())

    val reference = ProgramWireColorOptionsV3(referenceAppearance = 5u)
    val copied = optionsType.getMethod("copy-iC4mN9g", *fields).invoke(
        reference, reference.domain, reference.identity, reference.maxChange, reference.appearance, reference.tone)
    check(copied == reference)
    val defaultCopy = optionsType.getMethod("copy-iC4mN9g\$default", optionsType, *fields,
        Int::class.javaPrimitiveType!!, Any::class.java).invoke(null,
        reference, null, null, 1.0, null, null, 27, null) as ProgramWireColorOptionsV3
    check(defaultCopy.maxChange == 1.0 && defaultCopy.referenceAppearance == 5u)

    val builder = ProgramWireBuilderV3()
    ProgramWireBuilderV3::class.java.getMethod("assignment", List::class.java).invoke(builder, emptyList<UInt>())
    bytesEqual(builder.finish(), ProgramWireBuilderV3().assignment(emptyList(), ProgramWireAssignmentOptionsV3()).finish())
}

private fun verifyGradient(wire: ByteArray) {
    val stops = listOf(ProgramGradientStop(0.0, ProgramRgb8(0u, 0u, 0u)),
        ProgramGradientStop(0.125, ProgramRgb8(1u, 1u, 1u)), ProgramGradientStop(1.0, ProgramRgb8(3u, 3u, 3u)))
    compileProgramWire(wire, 1u).use { runtime ->
        runtime.updateGradient(1uL, rgb(0, 0, 0), 0u, stops).use { snapshot ->
            val field = requireNotNull(snapshot.gradientAssessment())
            check(field.stops == stops && field.regionCount == 11uL && field.rendering == "unknown")
            val witness = field.witnesses.last()
            check(witness.segmentIndex == 1u && witness.local == ProgramFieldWitness(3u, 4u, stops.last().color))
        }
    }
}

private fun verifyDeclaredField(wire: ByteArray) {
    val start = ProgramRgb8(0u, 0u, 0u)
    val end = ProgramRgb8(2u, 2u, 2u)
    val expected = ProgramFieldAssessment(0u, start, end,
        "opaque-affine-encoded-srgb8-half-up-v1", "whole-field", "unknown", 7u, listOf(
            ProgramFieldWitness(0u, 1u, start),
            ProgramFieldWitness(1u, 4u, ProgramRgb8(1u, 1u, 1u)),
            ProgramFieldWitness(3u, 4u, end),
        ))
    var rejected: ProgramSnapshot? = null
    var accepted: ProgramSnapshot? = null
    val ownedReport: ProgramFieldAssessment
    try {
        ownedReport = compileProgramWire(wire, 1u).use { runtime ->
            val table = runtime.updateObserved(1uL, listOf(9u), rgb(255, 255, 255), 1u).use { previous ->
                check(previous.state() == "ready" && previous.fieldAssessment() == null)
                previous.copyOutputTable()
            }
            val host = object : ProgramUpdateHost {
                override fun accept(snapshot: ProgramSnapshot): Boolean {
                    rejected = snapshot
                    return false
                }
            }
            check(!runtime.updateDeclaredFieldWith(2uL, rgb(17, 17, 17), 0u, start, end, host))
            requireNotNull(rejected).let {
                check(it.state() == "ready" && it.revision() == 2uL)
                check(it.fieldAssessment() == expected)
            }
            runtime.updateObserved(1uL, listOf(9u), rgb(255, 255, 255), 1u).use { replay ->
                check(replay.revision() == 1uL && replay.fieldAssessment() == null)
                check(replay.copyOutputTable() == table)
            }
            val field = runtime.updateDeclaredField(2uL, rgb(199, 199, 199), 0u, start, end)
            accepted = field
            check(field.state() == "ready")
            val report = requireNotNull(field.fieldAssessment())
            check(report == expected)
            try {
                runtime.updateDeclaredField(2uL, rgb(0, 0, 0), 0u, end, start).close()
                error("reversed field replaced an accepted revision")
            } catch (failure: LabColorsException.Refused) {
                check(failure.code == "program_update" && failure.operation == "updateDeclaredField")
            }
            runtime.updateDeclaredField(2uL, rgb(199, 199, 199), 0u, start, end).use { fieldReplay ->
                check(fieldReplay.state() == "ready" && fieldReplay.revision() == 2uL)
                check(fieldReplay.fieldAssessment() == report)
            }
            runtime.updateDeclaredField(3uL, rgb(0, 0, 0), 0u, end, start).use { reversed ->
                val reverseReport = requireNotNull(reversed.fieldAssessment())
                check(reverseReport.from == end && reverseReport.to == start)
                check(reverseReport.regionCount == expected.regionCount)
            }
            runtime.updateObserved(4uL, listOf(9u), rgb(255, 255, 255), 1u).use { restored ->
                check(restored.fieldAssessment() == null)
                check(restored.copyOutputTable() == table)
            }
            report
        }
        check(requireNotNull(rejected).fieldAssessment() == expected)
        check(requireNotNull(accepted).fieldAssessment() == ownedReport)
    } finally {
        rejected?.close()
        accepted?.close()
    }
    check(ownedReport == expected)
}

/** The same external consumer checks run against installed Android and JVM packages. */
fun verifyProgramAuthoringV3(fixtures: Map<String, String>) {
    verifyPublishedV03AuthoringAbi()
    verifyAuthoredComposition()
    val referenceOptions = ProgramWireColorOptionsV3(referenceAppearance = 5u)
    check(referenceOptions.copy() == referenceOptions)
    check(referenceOptions.copy(maxChange = 1.0).referenceAppearance == 5u)
    check(referenceOptions.copy(referenceAppearance = 0u).referenceAppearance == 0u)
    check(referenceOptions.copy(referenceAppearance = null) != referenceOptions)
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
    val composeTargets = listOf(0x01020304u, 0xffffffffu, 0u)
    val composeReference = listOf(ProgramWireReferenceInputV3(0x10203040u, rgb(1, 128, 255)),
        ProgramWireReferenceInputV3(0xffffffffu, rgb(254, 129, 0)))
    for ((name, members, inputs) in listOf(
        Triple("compose", composeTargets, null), Triple("composeReference", composeTargets, composeReference),
        Triple("composeEmpty", emptyList(), null), Triple("composeReferenceEmpty", emptyList(), emptyList()),
    )) {
        val composition = ProgramWireBuilderV3().compose(members, ProgramWireComposeOptionsV3(inputs)).finish()
        bytesEqual(requireNotNull(fixtures[name]).wireBytes(), composition)
        check(composition[26].toInt() == if (inputs == null) 5 else 6)
        composition[26] = (composition[26] - 2).toByte()
        bytesEqual(ProgramWireBuilderV3().assignment(members, ProgramWireAssignmentOptionsV3(inputs)).finish(), composition)
    }
    for ((name, actual) in mapOf("computedBox" to box, "computedGray" to gray,
        "contextual" to contextual, "fixedOnly" to fixed, "encodingOnly" to encodingOnly,
        "referenceSingleton" to referenceProgram(false), "referenceHierarchy" to referenceProgram(true),
        "referenceEmpty" to ProgramWireBuilderV3().assignment(emptyList(), ProgramWireAssignmentOptionsV3(emptyList())).finish())) {
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
    rejects { builder.colorTarget(2u, rgb(1, 2, 3), ProgramWireColorOptionsV3(appearance = 0u, referenceAppearance = 0u)) }
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
    rejects { builder.compose(List(4097) { 1u }) }
    rejects { builder.preserveTonalHierarchy(7u, emptyList()) }
    rejects { builder.preserveTonalHierarchy(7u, listOf(3u)) }
    rejects { builder.preserveTonalHierarchy(7u, listOf(3u, 3u)) }
    rejects { builder.preserveTonalHierarchy(7u, List(4097) { it.toUInt() }) }
    rejects { builder.assignment(emptyList(), ProgramWireAssignmentOptionsV3(listOf(ProgramWireReferenceInputV3(0u, rgb(1, 2))))) }
    rejects { builder.assignment(emptyList(), ProgramWireAssignmentOptionsV3(List(2) { ProgramWireReferenceInputV3(0u, rgb(1, 2, 3)) })) }
    rejects { builder.assignment(emptyList(), ProgramWireAssignmentOptionsV3(List(4097) { ProgramWireReferenceInputV3(it.toUInt(), rgb(1, 2, 3)) })) }
    rejects { builder.compose(emptyList(), ProgramWireComposeOptionsV3(listOf(ProgramWireReferenceInputV3(0u, rgb(1, 2))))) }
    rejects { builder.compose(emptyList(), ProgramWireComposeOptionsV3(List(2) { ProgramWireReferenceInputV3(0u, rgb(1, 2, 3)) })) }
    rejects { builder.compose(emptyList(), ProgramWireComposeOptionsV3(List(4097) { ProgramWireReferenceInputV3(it.toUInt(), rgb(1, 2, 3)) })) }
    val reentrant = object : AbstractList<UInt>() {
        override val size = 1
        override fun get(index: Int): UInt {
            builder.source(10u, rgb(1, 2, 3))
            return 1u
        }
    }
    rejects { builder.assignment(reentrant) }
    rejects { builder.compose(reentrant) }
    val reentrantHierarchy = object : AbstractList<UInt>() {
        override val size: Int get() { builder.source(10u, rgb(1, 2, 3)); return 2 }
        override fun get(index: Int): UInt = index.toUInt()
    }
    rejects { builder.preserveTonalHierarchy(7u, reentrantHierarchy) }
    val reentrantReference = object : AbstractList<ProgramWireReferenceInputV3>() {
        override val size = 1
        override fun get(index: Int): ProgramWireReferenceInputV3 {
            builder.source(10u, rgb(1, 2, 3))
            return ProgramWireReferenceInputV3(0u, rgb(1, 2, 3))
        }
    }
    rejects { builder.assignment(emptyList(), ProgramWireAssignmentOptionsV3(reentrantReference)) }
    rejects { builder.compose(emptyList(), ProgramWireComposeOptionsV3(reentrantReference)) }
    val reentrantReferenceSize = object : AbstractList<ProgramWireReferenceInputV3>() {
        override val size: Int get() { builder.finish(); return 0 }
        override fun get(index: Int) = error("unreachable")
    }
    rejects { builder.assignment(emptyList(), ProgramWireAssignmentOptionsV3(reentrantReferenceSize)) }
    rejects { builder.compose(emptyList(), ProgramWireComposeOptionsV3(reentrantReferenceSize)) }
    builder.assignment(listOf(1u))
    val assigned = builder.finish()
    try { builder.assignment(emptyList()); error("duplicate assignment was accepted") }
    catch (_: ProgramWireException.InvalidDeclaration) { }
    try { builder.compose(emptyList()); error("composition replaced an admitted assignment") }
    catch (_: ProgramWireException.InvalidDeclaration) { }
    bytesEqual(assigned, builder.finish())

    val composedRgb = rgb(255, 128, 0)
    val composedTargets = mutableListOf(1u, 0u)
    val composedInputs = mutableListOf(ProgramWireReferenceInputV3(0u, composedRgb))
    val composed = ProgramWireBuilderV3().compose(composedTargets, ProgramWireComposeOptionsV3(composedInputs))
    val composedOwned = composed.finish()
    composedRgb.fill(0); composedTargets.clear(); composedInputs.clear()
    try { composed.assignment(emptyList()); error("assignment replaced an admitted composition") }
    catch (_: ProgramWireException.InvalidDeclaration) { }
    try { composed.compose(emptyList()); error("duplicate composition was accepted") }
    catch (_: ProgramWireException.InvalidDeclaration) { }
    bytesEqual(composedOwned, composed.finish())

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

    val referenceRgb = rgb(255, 255, 255)
    val referenceInputs = mutableListOf(ProgramWireReferenceInputV3(0u, referenceRgb))
    val hierarchyMembers = mutableListOf(0u, 1u)
    val referenceCapture = ProgramWireBuilderV3()
        .preserveTonalHierarchy(7u, hierarchyMembers)
        .assignment(emptyList(), ProgramWireAssignmentOptionsV3(referenceInputs))
    val referenceOwned = referenceCapture.finish()
    referenceRgb.fill(0); referenceInputs.clear(); hierarchyMembers[1] = 0u
    bytesEqual(referenceOwned, referenceCapture.finish())
    check(!ProgramWireBuilderV3().assignment(emptyList()).finish().contentEquals(
        ProgramWireBuilderV3().assignment(emptyList(), ProgramWireAssignmentOptionsV3(emptyList())).finish()))
    check(!ProgramWireBuilderV3().colorTarget(2u, rgb(1, 2, 3), ProgramWireColorOptionsV3(referenceAppearance = 0u)).finish()
        .contentEquals(ProgramWireBuilderV3().colorTarget(2u, rgb(1, 2, 3)).finish()))

    for (hierarchy in listOf(false, true)) {
        val runtime = compileProgramWire(referenceProgram(hierarchy), 1u)
        val original = runtime.updateObserved(1uL, listOf(1u), rgb(255, 255, 255), 1u)
        val selections = requireNotNull(original.referenceSelections())
        try {
            check(original.state() == "ready")
            bytesEqual(rgb(68, 68, 68), original.outputRgb(0u))
            check(selections == requireNotNull(original.assessmentReport()).referenceSelections)
            check(selections.size == if (hierarchy) 2 else 1)
            val strong = selections.first()
            check(strong.targetId == 10u && strong.occurrenceId == 5u)
            check(strong.preference == ProgramReferencePreference.REFERENCE_STRENGTH)
            val referenceCase = strong.cases.single()
            check(referenceCase.referenceVisible == ProgramRgb8(68u, 68u, 68u))
            check(referenceCase.referenceBackdrop == ProgramRgb8(255u, 255u, 255u))
            check(referenceCase.actualContrast == referenceCase.referenceContrast && referenceCase.referenceContrast > 9.0)
            check(referenceCase.polarity == ProgramTonalPolarity.DARKER && referenceCase.strengthLoss == 0.0)
            check(referenceCase.virtualDistanceSquared == 0.0)
            val capacity = requireNotNull(strong.capacity)
            check(capacity.ratioLower <= capacity.ratioUpper && capacity.ratioLower >= 1.0)
            runtime.updateObserved(2uL, listOf(1u), rgb(0, 0, 0), 1u).use { dark ->
                check(dark.state() == "ready")
                check(!dark.outputRgb(0u).contentEquals(original.outputRgb(0u)))
                val darkSelections = requireNotNull(dark.referenceSelections())
                check(darkSelections == requireNotNull(dark.assessmentReport()).referenceSelections)
                check(darkSelections.first().cases.single().polarity == ProgramTonalPolarity.LIGHTER)
                if (hierarchy) {
                    bytesEqual(rgb(102, 102, 102), original.outputRgb(1u))
                    val authoredPair = requireNotNull(original.assessmentReport()).tonalHierarchy.single()
                    val adaptedPair = requireNotNull(dark.assessmentReport()).tonalHierarchy.single()
                    check(authoredPair.constraintId == 30u && authoredPair.strongerOccurrenceId == 5u && authoredPair.weakerOccurrenceId == 15u)
                    check(authoredPair.requiredRatio == authoredPair.actualRatio && authoredPair.requiredRatio > 1.0)
                    check(kotlin.math.abs(authoredPair.requiredGap - kotlin.math.ln(authoredPair.requiredRatio)) < 1e-12)
                    check(adaptedPair.requiredRatio == authoredPair.requiredRatio && adaptedPair.satisfied)
                    check(adaptedPair.actualRatio >= adaptedPair.requiredRatio)
                }
            }
            runtime.updateObserved(3uL, listOf(1u), rgb(255, 255, 255), 1u).use { restored ->
                bytesEqual(original.outputRgb(0u), restored.outputRgb(0u))
                if (hierarchy) bytesEqual(original.outputRgb(1u), restored.outputRgb(1u))
            }
            runtime.updateUnknown(4uL, 7u).use { check(it.referenceSelections() == null) }
        } finally { runtime.close() }
        try { check(original.referenceSelections() == selections) } finally { original.close() }
        check(selections.first().cases.single().referenceVisible == ProgramRgb8(68u, 68u, 68u))
    }

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
                val report = requireNotNull(snapshot.assessmentReport())
                check(report.cleanliness.status == ProgramCleanlinessStatus.NOT_ASSESSED)
                check(report.cleanliness.profile == null)
                check(snapshot.cleanlinessAssessment() == report.cleanliness)
                val cell = report.cells.single()
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
            check(it.cleanlinessAssessment() == null)
        }
    }
    val nominal = ProgramWireBuilderV3().apply {
        source(1u, rgb(255, 255, 255)); fixedTarget(2u, 1u)
        surfaceInputPort(6u); opacityInput(5u, 0.5)
        solidPaint(3u, 2u); opacityPaint(4u, 3u, 5u); inputSurface(7u, 6u)
        pointOccurrence(8u, 4u, 7u); nominalSrgb8Visible(false, 10u, 8u)
        output(17u, 4u); assignment(emptyList())
    }.finish()
    verifyDeclaredField(nominal)
    verifyGradient(nominal)
    compileProgramWire(nominal, 1u).use { runtime ->
        runtime.updateObserved(1uL, listOf(9u), rgb(255, 255, 255), 1u).use { first ->
            val summary = requireNotNull(first.cleanlinessAssessment())
            check(summary.status == ProgramCleanlinessStatus.CONFORMS)
            check(summary.profile == "nominal-srgb8-v2" && summary.basis == "declared-convention")
            check(summary.scope == "declared-final-points")
            check(summary.accepted == 1uL && summary.rejected == 0uL && summary.unavailable == 0uL)
            check(requireNotNull(first.assessmentReport()).cleanliness == summary)
            runtime.updateObserved(2uL, listOf(9u), rgb(1, 1, 3), 1u).use { changed ->
                check(changed.state() == "ready")
                val report = requireNotNull(changed.assessmentReport())
                check(changed.cleanlinessAssessment() == report.cleanliness)
                check(report.cleanliness.status == ProgramCleanlinessStatus.VIOLATES && report.cleanliness.rejected == 1uL)
                val rejection = report.cells.single().assessment as ProgramConstraintAssessment.NominalSrgb8VisibleRejected
                check(listOf(rejection.visible.red, rejection.visible.green, rejection.visible.blue) == listOf<UByte>(128u, 128u, 129u))
                check(129 in rejection.rejectedBlueLower.toInt()..rejection.rejectedBlueUpper.toInt())
            }
            check(first.cleanlinessAssessment() == summary)
        }
    }
}
