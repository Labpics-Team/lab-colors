package pics.lab.colors.consumer

import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import pics.lab.colors.*
import java.lang.ref.WeakReference
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

internal fun asset(name: String): String = InstrumentationRegistry.getInstrumentation().context.assets
    .open(name).bufferedReader().use { it.readText() }
internal fun hex(value: String): ByteArray = value.removePrefix("#").chunked(2).map { it.toInt(16).toByte() }.toByteArray()
internal fun rgb(vararg channels: Int) = channels.map { it.toByte() }.toByteArray()
private fun JSONArray.rgb(): ByteArray = ByteArray(length()) { getInt(it).also { value -> require(value in 0..255) }.toByte() }
private fun expectCode(code: String, block: () -> Unit) {
    try { block(); fail("expected $code") }
    catch (error: LabColorsException.Refused) { assertEquals(code, error.code) }
}

private class Host(private val expectedOpacity: Double = 1.0) : ProgramPointSinkHost {
    var owner = WeakReference<ProgramAttachment>(null)
    var reject = false
    var fail = false
    var reenter = false
    var point: ByteArray? = null
    var sequence = 0uL
    private var calls = 0
    @Synchronized fun callCount(): Int = calls
    var epoch: ULong? = null
    @Synchronized override fun tryInstall(intent: ProgramPointSinkIntent): Boolean {
        calls++
        if (reenter) {
            val attachment = requireNotNull(owner.get())
            expectCode("program_attachment_busy") { attachment.updateUnknown(99uL, 7u).close() }
            expectCode("program_attachment_busy") { attachment.materializationAuthority().close() }
            expectCode("program_attachment_busy") { attachment.dispose(false) }
        }
        assertEquals(sequence, intent.expectedSequence)
        assertEquals(501u, intent.sinkOutput)
        assertNotEquals(0uL, intent.bindingEpoch)
        epoch?.let { assertEquals(it, intent.bindingEpoch) }
        when (intent.operation) {
            ProgramPointSinkOperation.SET_ALL -> {
                val paint = requireNotNull(intent.point)
                assertEquals(91u, paint.slot); assertEquals(3, paint.source.size); assertEquals(expectedOpacity, paint.opacity, 0.0)
            }
            ProgramPointSinkOperation.CONFIRM_EXACT -> {
                assertEquals(sequence, intent.desiredSequence)
                assertArrayEquals(point, intent.point?.source)
                intent.point?.let {
                    assertEquals(91u, it.slot); assertEquals(3, it.source.size); assertEquals(expectedOpacity, it.opacity, 0.0)
                }
            }
            ProgramPointSinkOperation.REVOKE_ALL -> assertNull(intent.point)
        }
        if (fail) throw ProgramPointSinkCallbackException.InvocationFailed()
        if (reject) return false
        if (intent.operation != ProgramPointSinkOperation.CONFIRM_EXACT) assertEquals(sequence + 1uL, intent.desiredSequence)
        point = intent.point?.source?.copyOf()
        sequence = intent.desiredSequence
        epoch = intent.bindingEpoch
        return true
    }
}

class ContractTest {
    private fun fixture() = JSONObject(asset("program-runtime-v1.json"))
    private fun graph(builder: ProgramWireBuilderV1): ProgramWireBuilderV1 = builder.apply {
        surfaceInputPort(31u); solidPaint(41u, 21u); inputSurface(51u, 31u)
        sourceOverOccurrence(61u, 41u, 51u, 64.0, 0.2, ProgramSurround.AVERAGE)
        presentationRoot(71u, 61u); presentationTarget(71u, 61u)
        wcag22VisibleUnary(true, 81u, 61u, Wcag22Criterion.SC143_TEXT_DEFAULT); output(91u, 41u)
    }
    @Test fun runtimeHistoryAndThreadMigration() {
        val vectors = fixture()
        compileProgramWire(hex(vectors.getString("adaptive")), UInt.MAX_VALUE).use { runtime ->
            val held = mutableListOf<ProgramSnapshot>()
            val worker = Executors.newSingleThreadExecutor()
            try {
                val history = vectors.getJSONArray("history")
                for (index in 0 until history.length()) {
                    val row = history.getJSONObject(index)
                    val revision = row.getLong("revision").toULong()
                    val snapshot = worker.submit<ProgramSnapshot> {
                        if (row.has("unknownReason")) runtime.updateUnknown(revision, row.getInt("unknownReason").toUInt())
                        else {
                            val backgrounds = row.getJSONArray("backgrounds")
                            val ids = (1..backgrounds.length()).map { it.toUInt() }
                            val surfaces = (0 until backgrounds.length()).flatMap { backgrounds.getJSONArray(it).rgb().toList() }.toByteArray()
                            runtime.updateObserved(revision, ids, surfaces, 1u)
                        }
                    }.get(10, TimeUnit.SECONDS)
                    held.add(snapshot)
                    assertEquals(row.getString("state"), snapshot.state())
                    assertEquals(1u, snapshot.snapshotVersion()); assertEquals(revision, snapshot.revision()); assertNull(snapshot.reason())
                    if (row.has("rgb")) {
                        assertEquals(1uL, snapshot.outputCount())
                        assertEquals(91u, snapshot.outputSlot(0u))
                        assertArrayEquals(row.getJSONArray("rgb").rgb(), snapshot.outputRgb(0u))
                    }
                }
                val first = held.first()
                assertArrayEquals(rgb(0, 0, 0), first.presentationCompositeRgb(71u, 1u))
                expectCode("presentation_composite_unavailable") { first.presentationCompositeRgb(91u, 1u) }
                first.outputRgb(0u)[0] = 123
                assertArrayEquals(rgb(0, 0, 0), first.outputRgb(0u))
                expectCode("invalid_output_index") { first.outputRgb(1u) }
                runtime.updateObserved(ULong.MAX_VALUE, listOf(1u), rgb(255, 255, 255), 1u).use {
                    assertEquals("ready", it.state())
                    assertArrayEquals(rgb(0, 0, 0), it.outputRgb(0u))
                }
                runtime.updateObserved(ULong.MAX_VALUE, listOf(1u), rgb(255, 255, 255), 1u).close()
                expectCode("program_update") { runtime.updateUnknown(1uL, 7u).close() }
            } finally { held.forEach { it.close() }; worker.shutdownNow() }
        }
        val runtime = compileProgramWire(hex(vectors.getString("fixed")), 1u)
        expectCode("program_update") { runtime.updateObserved(1uL, listOf(1u), rgb(255, 255), 1u).close() }
        runtime.updateObserved(1uL, listOf(1u), rgb(255, 255, 255), 1u).use { assertEquals("ready", it.state()) }
        runtime.updateObserved(1uL, listOf(1u), rgb(255, 255, 255), 1u).close()
        expectCode("program_update") { runtime.updateObserved(1uL, listOf(1u), rgb(0, 0, 0), 1u).close() }
        runtime.close(); runtime.close()
        try { runtime.updateUnknown(2uL, 7u); fail("closed native handle was usable") }
        catch (_: IllegalStateException) { }
        computedRuntimeHistory()
    }

    private fun computedRuntimeHistory() {
        val vectors = fixture().getJSONObject("computed")
        val unsupported = ProgramWireBuilderV3().computedTarget(21u, ProgramWireDomainV3.Gray(0u, 255u), rgb(0, 0, 0), 1.0)
            .assignment(listOf(21u)).apply { finiteTarget(22u, listOf(ProgramCandidate(1u, rgb(0, 0, 0), 1.0))) }.finish()
        expectCode("program_unsupported") { compileProgramWire(unsupported, 1u).close() }
        expectCode("program_unsupported") { attachProgramWire(unsupported, 1u, 91u, 501u, 71u, 61u, Host()).close() }
        for (domain in listOf("gray", "box", "relation")) {
            compileProgramWire(hex(vectors.getString(domain)), UInt.MAX_VALUE).use { runtime ->
                runtime.updateUnknown(0uL, UInt.MAX_VALUE).use {
                    assertEquals(2u, it.snapshotVersion()); assertEquals(0uL, it.revision())
                    assertEquals("waiting", it.state()); assertNull(it.reason()); assertEquals(0uL, it.outputCount())
                }
                val held = mutableListOf<ProgramSnapshot>()
                try {
                    val history = vectors.getJSONArray(domain + "History")
                    for (index in 0 until history.length()) {
                        val row = history.getJSONObject(index)
                        val revision = row.getLong("revision").toULong()
                        val snapshot = if (row.has("unknownReason")) runtime.updateUnknown(revision, row.getInt("unknownReason").toUInt())
                        else {
                            val surfaces = row.getJSONArray("backgrounds").getJSONArray(0).rgb()
                            runtime.updateObserved(revision, listOf(1u), surfaces, 1u).also { current ->
                                runtime.updateObserved(revision, listOf(1u), surfaces, 1u).use {
                                    assertEquals(current.state(), it.state()); assertEquals(current.reason(), it.reason())
                                    assertEquals(current.outputCount(), it.outputCount())
                                }
                            }
                        }
                        held.add(snapshot)
                        assertEquals(2u, snapshot.snapshotVersion()); assertEquals(revision, snapshot.revision())
                        assertEquals(row.getString("state"), snapshot.state())
                        assertEquals(if (row.has("reason")) row.getString("reason") else null, snapshot.reason())
                        if (row.has("rgb")) {
                            assertEquals(1uL, snapshot.outputCount()); assertEquals(91u, snapshot.outputSlot(0u))
                            assertArrayEquals(row.getJSONArray("rgb").rgb(), snapshot.outputRgb(0u))
                            assertEquals(0.5, snapshot.outputOpacity(0u), 0.0)
                        } else {
                            assertEquals(0uL, snapshot.outputCount())
                            expectCode("invalid_output_index") { snapshot.outputRgb(0u) }
                        }
                    }
                    assertArrayEquals(rgb(64, 64, 64), held.first().outputRgb(0u)); assertEquals(1uL, held.first().revision())
                    expectCode("program_update") { runtime.updateUnknown(1uL, 7u).close() }
                    val current = held.last()
                    val surfaces = history.getJSONObject(history.length() - 1).getJSONArray("backgrounds").getJSONArray(0).rgb()
                    val conflicting = surfaces.copyOf().apply { this[0] = (this[0].toInt() xor 1).toByte() }
                    expectCode("program_update") { runtime.updateObserved(current.revision(), listOf(1u), conflicting, 1u).close() }
                    runtime.updateObserved(current.revision(), listOf(1u), surfaces, 1u).use {
                        assertEquals(current.revision(), it.revision()); assertEquals(current.state(), it.state())
                        assertArrayEquals(current.outputRgb(0u), it.outputRgb(0u))
                    }
                    runtime.updateObserved(ULong.MAX_VALUE, listOf(1u), rgb(128, 128, 128), 1u).use {
                        assertEquals(ULong.MAX_VALUE, it.revision()); assertEquals("ready", it.state())
                    }
                } finally { held.forEach { it.close() } }
            }
        }
    }

    @Test fun authoringIsCanonicalAndTransactional() {
        val vectors = fixture()
        assertArrayEquals(hex(vectors.getString("fixed")), graph(ProgramWireBuilderV1().apply {
            source(11u, rgb(20, 20, 20)); fixedTarget(21u, 11u)
        }).finish())
        val adaptive = ProgramWireBuilderV2().apply {
            finiteTarget(21u, listOf(ProgramCandidate(201u, rgb(0, 0, 0), 1.0), ProgramCandidate(202u, rgb(255, 255, 255), 1.0)))
            selectionRelease(1uL, listOf(
                listOf(ProgramSelectionState(1u, listOf(ProgramSelectionChoice(21u, 201u)))),
                listOf(ProgramSelectionState(2u, listOf(ProgramSelectionChoice(21u, 202u))))))
        }
        assertArrayEquals(hex(vectors.getString("adaptive")), graph(adaptive).finish())
        val all = ProgramWireBuilderV1().apply {
            source(11u, rgb(1, 2, 3)); fixedTarget(21u, 11u)
            finiteTarget(22u, listOf(ProgramCandidate(201u, rgb(0, 0, 0), 0.5), ProgramCandidate(202u, rgb(255, 255, 255), 1.0)))
            family(90u, ByteArray(32)); surfaceInputPort(31u); opacityInput(32u, 0.5)
            solidPaint(41u, 21u); opacityPaint(42u, 41u, 32u); inputSurface(51u, 31u); occurrenceSurface(52u, 61u)
            sourceOverOccurrence(61u, 42u, 51u, 64.0, 0.2, ProgramSurround.DIM)
            presentationRoot(71u, 61u); presentationTarget(71u, 61u)
            exactVisibleUnary(true, 81u, 61u, rgb(1, 2, 3))
            wcag22VisibleUnary(false, 82u, 61u, Wcag22Criterion.SC1411_UI_COMPONENT_OR_STATE)
            declaredSrgb8CleanSet(true, 83u, 71u, 61u); exactIntrinsicRelationHard(84u, 11u, listOf(11u)); output(91u, 42u)
        }
        assertArrayEquals(hex(vectors.getString("encodingOnly")), all.finish())
        val before = all.finish()
        try { all.finiteTarget(99u, listOf(ProgramCandidate(1u, rgb(1, 2, 3), 1.0), ProgramCandidate(2u, rgb(1, 2, 3), Double.NaN))); fail("invalid entry accepted") }
        catch (_: ProgramWireException.InvalidDeclaration) { }
        assertArrayEquals(before, all.finish())
        val v2 = ProgramWireBuilderV2()
        val state = ProgramSelectionState(1u, listOf(ProgramSelectionChoice(21u, 201u)))
        val empty = v2.finish()
        try { v2.selectionRelease(1uL, listOf(listOf(state), emptyList())); fail("empty late group accepted") }
        catch (_: ProgramWireException.InvalidDeclaration) { }
        assertArrayEquals(empty, v2.finish())
        v2.selectionRelease(ULong.MAX_VALUE, listOf(listOf(state)))
        val accepted = v2.finish()
        try { v2.selectionRelease(2uL, listOf(listOf(state))); fail("duplicate release accepted") }
        catch (_: ProgramWireException.InvalidDeclaration) { }
        assertArrayEquals(accepted, v2.finish())
        // Даже обход Kotlin internal через JVM-reflection не может испортить v1.
        val v1 = ProgramWireBuilderV1()
        val v1Before = v1.finish()
        val selection = ProgramWireBuilderV1::class.java.declaredMethods.single {
            it.name.substringBefore('-').substringBefore('$') == "selection"
        }
        selection.isAccessible = true
        try { selection.invoke(v1, 1L, listOf(listOf(state))); fail("v1 selection was encoded") }
        catch (failure: java.lang.reflect.InvocationTargetException) {
            assertTrue(failure.cause is ProgramWireException.InvalidDeclaration)
        }
        assertArrayEquals(v1Before, v1.finish())
        val reentrant = object : AbstractList<ProgramCandidate>() {
            override val size: Int get() = 1
            override fun get(index: Int): ProgramCandidate {
                all.source(999u, rgb(1, 2, 3))
                return ProgramCandidate(1u, rgb(1, 2, 3), 1.0)
            }
        }
        try { all.finiteTarget(99u, reentrant); fail("reentrant write accepted") }
        catch (_: ProgramWireException.InvalidDeclaration) { }
        assertArrayEquals(before, all.finish())
        val lawful = object : AbstractList<UInt>() {
            override val size: Int get() = 1
            override fun get(index: Int) = 11u
            override fun iterator(): Iterator<UInt> = error("caller iterator must not control bounded encoding")
        }
        assertArrayEquals(ProgramWireBuilderV1().exactIntrinsicRelationHard(1u, 11u, listOf(11u)).finish(),
            ProgramWireBuilderV1().exactIntrinsicRelationHard(1u, 11u, lawful).finish())
        val bounded = ProgramWireBuilderV1()
        repeat(4096) { bounded.surfaceInputPort(it.toUInt()) }
        val full = bounded.finish()
        try { bounded.surfaceInputPort(4096u); fail("over-limit entry accepted") }
        catch (_: ProgramWireException.TooManyEntries) { }
        assertArrayEquals(full, bounded.finish())
        assertArrayEquals(ProgramWireBuilderV1().opacityInput(1u, 0.0).finish(), ProgramWireBuilderV1().opacityInput(1u, -0.0).finish())
        val mutable = rgb(1, 2, 3)
        val owner = ProgramWireBuilderV1().source(UInt.MAX_VALUE, mutable)
        val saved = owner.finish(); mutable[0] = 99; owner.source(1u, rgb(0, 0, 0))
        assertArrayEquals(ProgramWireBuilderV1().source(UInt.MAX_VALUE, rgb(1, 2, 3)).finish(), saved)
        computedAuthoring(vectors.getJSONObject("computed"))
    }

    private fun computedAuthoring(vectors: JSONObject) {
        val encoded = ProgramWireBuilderV3()
            .computedTarget(0x10203040u, ProgramWireDomainV3.Box(rgb(1, 2, 3), rgb(4, 5, 6)), rgb(250, 0, 7), 0.25)
            .computedTarget(19u, ProgramWireDomainV3.Gray(10u, 12u), rgb(9, 8, 7), 8.0)
            .assignment(listOf(0x10203040u, 19u)).finish()
        assertArrayEquals(hex(vectors.getString("encodingOnly")), encoded)
        for ((domain, name) in listOf(ProgramWireDomainV3.Gray(0u, 255u) to "gray",
                ProgramWireDomainV3.Box(rgb(0, 0, 0), rgb(255, 255, 255)) to "box")) {
            val wire = ProgramWireBuilderV3().computedTarget(21u, domain, rgb(64, 64, 64), 1.0).assignment(listOf(21u)).apply {
                surfaceInputPort(31u); opacityInput(32u, 0.5); solidPaint(41u, 21u); opacityPaint(42u, 41u, 32u)
                inputSurface(51u, 31u); sourceOverOccurrence(61u, 42u, 51u, 64.0, 0.2, ProgramSurround.DIM)
                presentationRoot(71u, 61u); presentationTarget(71u, 61u)
                exactVisibleUnary(true, 81u, 61u, rgb(96, 96, 96)); output(91u, 42u)
            }.finish()
            assertArrayEquals(hex(vectors.getString(name)), wire)
        }
        val builder = ProgramWireBuilderV3().apply { source(11u, rgb(20, 20, 20)) }
        val before = builder.finish()
        val invalid = listOf(ProgramWireDomainV3.Gray(2u, 1u),
            ProgramWireDomainV3.Box(rgb(1, 2), rgb(3, 4, 5)), ProgramWireDomainV3.Box(rgb(1, 2, 3), rgb(3, 4)),
            ProgramWireDomainV3.Box(rgb(2, 0, 0), rgb(1, 1, 1)), ProgramWireDomainV3.Box(rgb(0, 2, 0), rgb(1, 1, 1)),
            ProgramWireDomainV3.Box(rgb(0, 0, 2), rgb(1, 1, 1)))
        for (domain in invalid) {
            try { builder.computedTarget(21u, domain, rgb(0, 0, 0), 1.0); fail("invalid domain accepted") }
            catch (_: ProgramWireException.InvalidDeclaration) { }
            assertArrayEquals(before, builder.finish())
        }
        for (delta in listOf(-1.0, -Double.MIN_VALUE, Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY, Double.MAX_VALUE, 1e155)) {
            try { builder.computedTarget(21u, ProgramWireDomainV3.Gray(0u, 255u), rgb(0, 0, 0), delta); fail("invalid delta accepted") }
            catch (_: ProgramWireException.InvalidDeclaration) { }
            assertArrayEquals(before, builder.finish())
        }
        try { builder.computedTarget(21u, ProgramWireDomainV3.Gray(0u, 255u), rgb(0, 0), 1.0); fail("invalid anchor accepted") }
        catch (_: ProgramWireException.InvalidDeclaration) { }
        try { builder.assignment(emptyList()); fail("empty order accepted") }
        catch (_: ProgramWireException.InvalidDeclaration) { }
        val oversized = object : AbstractList<UInt>() { override val size = 4097; override fun get(index: Int): UInt = error("limit before reads") }
        try { builder.assignment(oversized); fail("oversized order accepted") }
        catch (_: ProgramWireException.TooManyEntries) { }
        val reentrant = object : AbstractList<UInt>() {
            override val size = 1
            override fun get(index: Int): UInt { builder.assignment(listOf(29u)); return 21u }
        }
        try { builder.assignment(reentrant); fail("reentrant order accepted") }
        catch (_: ProgramWireException.InvalidDeclaration) { }
        assertArrayEquals(before, builder.finish())
        val lawful = object : AbstractList<UInt>() {
            override val size = 1; override fun get(index: Int) = 21u
            override fun iterator(): Iterator<UInt> = error("iterator must not control encoding")
        }
        builder.assignment(lawful)
        val accepted = builder.finish()
        try { builder.assignment(listOf(21u)); fail("duplicate assignment accepted") }
        catch (_: ProgramWireException.InvalidDeclaration) { }
        assertArrayEquals(accepted, builder.finish())
        for (domain in listOf(ProgramWireDomainV3.Gray(0u, 255u), ProgramWireDomainV3.Gray(19u, 19u),
                ProgramWireDomainV3.Box(rgb(0, 0, 0), rgb(255, 255, 255)), ProgramWireDomainV3.Box(rgb(1, 2, 3), rgb(1, 2, 3)))) {
            assertArrayEquals(ProgramWireBuilderV3().computedTarget(UInt.MAX_VALUE, domain, rgb(0, 0, 0), 0.0).finish(),
                ProgramWireBuilderV3().computedTarget(UInt.MAX_VALUE, domain, rgb(0, 0, 0), -0.0).finish())
        }
        val lower = rgb(1, 2, 3); val anchor = rgb(4, 5, 6); val order = mutableListOf(29u, 21u)
        val owner = ProgramWireBuilderV3().computedTarget(21u, ProgramWireDomainV3.Box(lower, rgb(7, 8, 9)), anchor, 1.0).assignment(order)
        val saved = owner.finish(); lower[0] = -1; anchor[0] = -1; order.reverse()
        assertArrayEquals(saved, owner.finish())
        val bounded = ProgramWireBuilderV3()
        repeat(4096) { bounded.computedTarget(it.toUInt(), ProgramWireDomainV3.Gray(0u, 0u), rgb(0, 0, 0), 0.0) }
        bounded.assignment((4095 downTo 0).map { it.toUInt() })
        val full = bounded.finish()
        try { bounded.computedTarget(4096u, ProgramWireDomainV3.Gray(0u, 0u), rgb(0, 0, 0), 0.0); fail("over-limit target accepted") }
        catch (_: ProgramWireException.TooManyEntries) { }
        assertArrayEquals(full, bounded.finish())
    }

    @Test fun fullBindingScopeKeepsAllPointsAndScenarioComposites() {
        val host = object : ProgramScopeSinkHost {
            private val commands = mutableListOf<ProgramScopeSinkIntent>()
            @Synchronized override fun tryInstall(intent: ProgramScopeSinkIntent): Boolean {
                commands.add(intent)
                return true
            }
            @Synchronized fun saved(): List<ProgramScopeSinkIntent> = commands.toList()
        }
        val wire = hex(
            "4c435057010003010000020000000b0000004040400c000000a0a0a00200000015000000010b00000016000000010c000000" +
            "0000000000000000010000001f00000000000000020000002900000001150000002a00000001160000000100000033000000011f000000" +
            "020000003d000000290000003300000000000000000050409a9999999999c93f013e0000002a0000003300000000000000000050409a9999999999c93f01" +
            "02000000470000003d000000480000003e00000002000000470000003d000000480000003e0000000200000051000000013d00000040404052000000013e000000a0a0a000000000020000005b000000290000005c0000002a000000"
        )
        val bindings = ProgramAttachmentBindings(
            listOf(ProgramPointEmissionBinding(91u, 501u), ProgramPointEmissionBinding(92u, 502u)),
            listOf(ProgramPointPresentationBinding(91u, 71u, 61u), ProgramPointPresentationBinding(92u, 72u, 62u)),
        )
        attachProgramWireWithBindings(wire, 17u, bindings, host).use { attachment ->
            attachment.updateObserved(1uL, listOf(101u, 301u), rgb(128, 128, 128, 128, 128, 128), 1u).use { ready ->
                assertEquals(2uL, ready.renderCount())
                assertTrue(ready.hasRender()); assertNull(ready.render())
                ready.renderAt(0u).use { assertArrayEquals(rgb(64, 64, 64), it.outputRgb()) }
                ready.renderAt(1u).use { assertArrayEquals(rgb(160, 160, 160), it.outputRgb()) }
                val installed = host.saved().last()
                assertEquals(ProgramPointSinkOperation.SET_ALL, installed.operation)
                assertEquals(listOf(501u, 502u), installed.scope)
                assertNull(installed.sinkOutput); assertNull(installed.point)
                assertEquals(listOf(501u, 502u), installed.points.map { it.sinkOutput })
                assertEquals(listOf(91u, 92u), installed.points.map { it.paint.slot })
                assertArrayEquals(rgb(64, 64, 64), installed.points[0].paint.source)
                assertArrayEquals(rgb(160, 160, 160), installed.points[1].paint.source)
                assertEquals(listOf(1.0, 1.0), installed.points.map { it.paint.opacity })
                assertEquals(4, installed.presentationComposites.size)
                assertEquals(setOf(71u to 101u, 71u to 301u, 72u to 101u, 72u to 301u),
                    installed.presentationComposites.map { it.root to it.scenario }.toSet())
                for (composite in installed.presentationComposites) {
                    assertArrayEquals(if (composite.root == 71u) rgb(64, 64, 64) else rgb(160, 160, 160), composite.source)
                }
                expectCode("program_materialization_ambiguous_bindings") { attachment.materializationAuthority().close() }
                ready.renderAt(1u).use { second ->
                    expectCode("program_materialization_ambiguous_bindings") {
                        attachment.materializationAuthorityFor(1uL, second.contentIdentity(), second.bindingEpoch()).close()
                    }
                }
                attachment.materializationAuthorityForBinding(72u, 62u).use {
                    assertEquals(92u, it.outputSlot()); assertArrayEquals(rgb(160, 160, 160), it.outputRgb())
                }

                attachment.updateUnknown(2uL, 41u).use { assertEquals(0uL, it.renderCount()) }
                val revoked = host.saved().last()
                assertEquals(ProgramPointSinkOperation.REVOKE_ALL, revoked.operation)
                assertEquals(listOf(501u, 502u), revoked.scope)
                assertNull(revoked.sinkOutput); assertNull(revoked.point)
                assertTrue(revoked.points.isEmpty()); assertTrue(revoked.presentationComposites.isEmpty())
                assertEquals(2uL, ready.renderCount())
                assertArrayEquals(rgb(64, 64, 64), installed.points[0].paint.source)
                assertEquals(4, installed.presentationComposites.size)
                expectCode("program_materialization_not_ready") { attachment.materializationAuthorityForBinding(72u, 62u).close() }
                attachment.dispose(true)
            }
        }
    }

    @Test fun hostFailureReentryRevocationAndRecovery() {
        val host = Host()
        attachProgramWire(hex(fixture().getString("adaptive")), 1u, 91u, 501u, 71u, 61u, host).use { attachment ->
            host.owner = WeakReference(attachment); host.reenter = true
            attachment.updateObserved(1uL, listOf(1u), rgb(255, 255, 255), 1u).close()
            attachment.materializationAuthority().use { old ->
                host.fail = true
                expectCode("program_attachment_host_protocol") { attachment.updateObserved(2uL, listOf(1u), rgb(0, 0, 0), 1u).close() }
                host.fail = false; host.reject = true
                expectCode("program_attachment_host_rejected") { attachment.updateObserved(2uL, listOf(1u), rgb(0, 0, 0), 1u).close() }
                attachment.materializationAuthority().use { assertEquals(1uL, it.revision()) }
                assertArrayEquals(rgb(0, 0, 0), host.point)
                host.reject = false
                attachment.updateObserved(2uL, listOf(1u), rgb(0, 0, 0), 1u).close()
                expectCode("program_materialization_stale_revision") { attachment.materializationAuthorityFor(old.revision(), old.contentIdentity(), old.bindingEpoch()).close() }
                assertArrayEquals(rgb(0, 0, 0), old.terminalCompositeRgb())
            }
            attachment.materializationAuthority().use { current ->
                val foreign = current.contentIdentity().apply { this[0] = (this[0].toInt() xor 1).toByte() }
                expectCode("program_materialization_stale_identity") { attachment.materializationAuthorityFor(2uL, foreign, current.bindingEpoch()).close() }
                expectCode("program_materialization_foreign_binding_epoch") { attachment.materializationAuthorityFor(2uL, current.contentIdentity(), current.bindingEpoch() xor 1uL).close() }
                assertEquals("unverified", current.rendererProvenance())
            }
            attachment.updateUnknown(3uL, 7u).use { assertFalse(it.hasRender()); assertEquals("stale", it.state()) }
            assertNull(host.point)
            attachment.updateObserved(4uL, listOf(1u, 2u), rgb(0, 0, 0, 255, 255, 255), 1u).use { assertEquals("failed", it.state()) }
            assertNull(host.point)
            attachment.updateObserved(5uL, listOf(1u), rgb(255, 255, 255), 1u).use { assertEquals("ready", it.state()) }
            expectCode("program_attachment_revoke_unconfirmed") { attachment.dispose(false) }
            host.point = null; attachment.dispose(true)
            expectCode("program_attachment_already_disposed") { attachment.updateUnknown(6uL, 7u).close() }
        }
        computedAttachmentHistory()
    }

    private fun computedAttachmentHistory() {
        val vectors = fixture().getJSONObject("computed")
        for (domain in listOf("gray", "box", "relation")) {
            val host = Host(0.5)
            attachProgramWire(hex(vectors.getString(domain)), 1u, 91u, 501u, 71u, 61u, host).use { attachment ->
                host.owner = WeakReference(attachment); host.reenter = true
                attachment.updateObserved(1uL, listOf(1u), rgb(128, 128, 128), 1u).use { first ->
                    assertEquals(2u, first.snapshotVersion()); assertEquals(1uL, first.revision()); assertNull(first.reason())
                    attachment.materializationAuthority().use { assertArrayEquals(rgb(96, 96, 96), it.terminalCompositeRgb()) }
                    host.reject = true
                    expectCode("program_attachment_host_rejected") { attachment.updateObserved(2uL, listOf(1u), rgb(255, 255, 255), 1u).close() }
                    attachment.materializationAuthority().use { assertEquals(1uL, it.revision()) }
                    assertArrayEquals(rgb(64, 64, 64), host.point); host.reject = false
                    val history = vectors.getJSONArray(domain + "History")
                    for (index in 1 until history.length()) {
                        val row = history.getJSONObject(index); val revision = row.getLong("revision").toULong()
                        val snapshot = if (row.has("unknownReason")) attachment.updateUnknown(revision, row.getInt("unknownReason").toUInt())
                        else {
                            val surfaces = row.getJSONArray("backgrounds").getJSONArray(0).rgb()
                            attachment.updateObserved(revision, listOf(1u), surfaces, 1u).also { current ->
                                attachment.updateObserved(revision, listOf(1u), surfaces, 1u).use { assertEquals(current.state(), it.state()) }
                            }
                        }
                        snapshot.use {
                            assertEquals(2u, it.snapshotVersion()); assertEquals(revision, it.revision())
                            assertEquals(row.getString("state"), it.state())
                            assertEquals(if (row.has("reason")) row.getString("reason") else null, it.reason())
                            assertEquals(row.has("rgb"), it.hasRender())
                            if (row.has("rgb")) {
                                assertEquals(1uL, it.outputCount()); assertArrayEquals(row.getJSONArray("rgb").rgb(), it.outputRgb(0u))
                                assertArrayEquals(row.getJSONArray("rgb").rgb(), host.point)
                            } else {
                                assertEquals(0uL, it.outputCount()); assertNull(host.point)
                                expectCode("invalid_output_index") { it.outputRgb(0u) }
                                expectCode("program_materialization_not_ready") { attachment.materializationAuthority().close() }
                            }
                        }
                    }
                    assertArrayEquals(rgb(64, 64, 64), first.outputRgb(0u))
                }
                expectCode("program_attachment_revoke_unconfirmed") { attachment.dispose(false) }
                host.point = null; attachment.dispose(true)
                expectCode("program_attachment_already_disposed") { attachment.updateUnknown(6uL, 7u).close() }
            }
        }
    }

    @Test fun numericalVectors() {
        val manifest = JSONObject(asset("manifest.json"))
        assertEquals(manifest.getString("coreVersion"), coreVersion())
        val counts = manifest.getJSONObject("counts")
        val families = setOf("contrasts", "alpha", "solve", "wcag22")
        assertEquals(families + "total", counts.keys().asSequence().toSet())
        var total = 0
        fun cases(name: String, test: (JSONObject) -> Unit) {
            val rows = JSONArray(asset("$name.json"))
            assertEquals(counts.getInt(name), rows.length()); assertTrue(rows.length() > 0)
            repeat(rows.length()) { test(rows.getJSONObject(it)); total++ }
        }
        fun theme(key: String) = Theme.valueOf(key.replace('-', '_').uppercase())
        cases("contrasts") { row ->
            val got = contrast(row.getString("fg"), row.getString("bg"), theme(row.getString("theme")))
            assertEquals(row.getDouble("lc"), got.lc, 1e-6); assertEquals(row.getDouble("wcagRatio"), got.wcagRatio, 1e-6)
        }
        cases("alpha") { row ->
            assertEquals(row.getString("composite"), composite(row.getString("tint"), row.getDouble("alpha"), row.getString("bg")))
            assertEquals(row.getDouble("minAlpha"), minAlpha(row.getString("tint"), row.getString("bg")), 1e-6)
        }
        cases("solve") { row ->
            val spec = row.getJSONObject("contract")
            val contract = when (spec.getString("kind")) {
                "text" -> ContractSpec.Text(spec.getDouble("lc"))
                "ui" -> ContractSpec.Ui(spec.getDouble("lc"))
                "range" -> ContractSpec.Range(spec.getDouble("floor"), spec.getDouble("ceiling"))
                else -> error("unknown contract fixture")
            }
            val expected = row.getJSONObject("outcome")
            try {
                val got = solveContrast(row.getString("bg"), contract, theme(row.getString("theme")))
                assertEquals("solved", expected.getString("kind"))
                hex(got.hex).zip(hex(expected.getString("hex"))).forEach { (a, b) -> assertTrue(kotlin.math.abs((a.toInt() and 255) - (b.toInt() and 255)) <= 1) }
                assertEquals(expected.getDouble("lc"), got.lc, 1e-6); assertEquals(expected.getDouble("wcagRatio"), got.wcagRatio, 1e-6)
            } catch (failure: ColorException.Failure) {
                assertEquals("failure", expected.getString("kind")); assertEquals(expected.getString("code"), failure.code)
                assertEquals(expected.getString("category").uppercase(), failure.category.name)
            }
        }
        cases("wcag22") { row ->
            val criterion = when (row.getString("criterion")) {
                "sc-1.4.3-text-default" -> Wcag22Criterion.SC143_TEXT_DEFAULT
                "sc-1.4.3-text-large-scale" -> Wcag22Criterion.SC143_TEXT_LARGE_SCALE
                "sc-1.4.11-ui-component-or-state" -> Wcag22Criterion.SC1411_UI_COMPONENT_OR_STATE
                "sc-1.4.11-graphical-object" -> Wcag22Criterion.SC1411_GRAPHICAL_OBJECT
                else -> error("unknown WCAG fixture")
            }
            val got = evaluateWcag22(row.getString("foreground"), row.getString("background"), criterion)
            assertEquals(row.getString("profileId"), got.profileId); assertEquals(criterion, got.criterion)
            assertEquals(row.getString("decision").uppercase(), got.decision.name)
            assertEquals(row.getString("foregroundLowerQ55"), got.foregroundLuminance.lower.toString())
            assertEquals(row.getString("foregroundUpperQ55"), got.foregroundLuminance.upper.toString())
            assertEquals(row.getString("backgroundLowerQ55"), got.backgroundLuminance.lower.toString())
            assertEquals(row.getString("backgroundUpperQ55"), got.backgroundLuminance.upper.toString())
            assertEquals(row.getString("q55Scale"), got.q55Scale.toString())
            assertEquals(row.getString("evidenceKind"), got.evidence.kind)
            assertEquals(row.getString("artifactSha256"), got.evidence.artifactSha256)
            assertEquals(row.getString("proofSha256"), got.evidence.proofSha256)
        }
        assertEquals(counts.getInt("total"), total)
    }

    @Test fun sourceIdentityAndUntrustedEnvelope() {
        val expected = JSONObject(asset("build-metadata.json"))
        val envelope = issueSourceCertificateEnvelope()
        val metadata = decodeCertificateEnvelope(envelope)
        assertEquals(expected.getString("coreTree"), metadata.producerRevision)
        assertEquals(32, metadata.producerContentIdentity.size)
        assertEquals(32, metadata.payloadSha256.size)
        expectCode("certificate_invalid_magic") { decodeCertificateEnvelope(ByteArray(20)) }
        val corrupted = envelope.copyOf().apply { this[lastIndex] = (this[lastIndex].toInt() xor 1).toByte() }
        try { decodeCertificateEnvelope(corrupted); fail("changed envelope accepted") }
        catch (_: LabColorsException) { }
    }

    private fun assessmentWire(sourceRgb: ByteArray, opacity: Double, visible: ByteArray): ByteArray =
        ProgramWireBuilderV1().apply {
            source(11u, sourceRgb); fixedTarget(21u, 11u); surfaceInputPort(31u)
            opacityInput(32u, opacity); solidPaint(42u, 21u); opacityPaint(41u, 42u, 32u)
            inputSurface(51u, 31u); sourceOverOccurrence(61u, 41u, 51u, 64.0, 0.2, ProgramSurround.AVERAGE)
            presentationRoot(71u, 61u); presentationTarget(71u, 61u)
            exactVisibleUnary(true, 82u, 61u, visible)
            exactVisibleUnary(false, 83u, 61u, rgb(255, 255, 255))
            declaredSrgb8CleanSet(false, 81u, 71u, 61u); output(91u, 41u)
        }.finish()

    @Test fun nativeAssessmentReportPreservesCellsAndDuplicateProvenance() {
        val wire = assessmentWire(rgb(0, 199, 64), 1.0, rgb(0, 199, 64))
        var held: ProgramAssessmentReport? = null
        for (ids in listOf(listOf(9u, 7u), listOf(7u, 9u))) {
            compileProgramWire(wire, UInt.MAX_VALUE).use { runtime ->
                runtime.updateObserved(ULong.MAX_VALUE, ids, ByteArray(6), 1u).use { snapshot ->
                    assertEquals("ready", snapshot.state())
                    assertArrayEquals(rgb(0, 199, 64), snapshot.outputRgb(0u))
                    val report = requireNotNull(snapshot.assessmentReport())
                    assertEquals(32, report.contentIdentity.size)
                    assertEquals(UInt.MAX_VALUE, report.streamId)
                    assertEquals(ULong.MAX_VALUE, report.revision)
                    assertEquals(listOf(0uL, 0uL), report.scenarioProvenance.map { it.caseIndex })
                    assertEquals(listOf(7u, 9u), report.scenarioProvenance.map { it.scenarioId })
                    assertEquals(listOf(81u, 82u, 83u), report.cells.map { it.constraintId })
                    assertEquals(listOf(0uL, 0uL, 0uL), report.cells.map { it.caseIndex })
                    assertEquals(listOf(false, true, false), report.cells.map { it.hard })
                    val rejected = report.cells[0].assessment as ProgramConstraintAssessment.DeclaredSrgb8CleanSetRejected
                    assertEquals(listOf(0, 199, 64), listOf(rejected.visible.red, rejected.visible.green, rejected.visible.blue).map { it.toInt() })
                    assertEquals(listOf(64, 108), listOf(rejected.rejectedBlueInterval.lower, rejected.rejectedBlueInterval.upper).map { it.toInt() })
                    val exact = report.cells[1].assessment as ProgramConstraintAssessment.ExactSrgb8
                    assertEquals(ProgramAssessmentVerdict.PASS, exact.verdict)
                    // Итоговый [0,199,64] нарушает report-only равенство белому.
                    val reportOnly = report.cells[2].assessment as ProgramConstraintAssessment.ExactSrgb8
                    assertEquals(ProgramAssessmentVerdict.VIOLATION, reportOnly.verdict)
                    assertEquals(91u, snapshot.outputSlot(0u))
                    held?.let { previous ->
                        assertArrayEquals(previous.contentIdentity, report.contentIdentity)
                        assertEquals(previous.scenarioProvenance, report.scenarioProvenance)
                        assertEquals(previous.cells, report.cells)
                    }
                    held = report
                }
            }
        }
    }

    @Test fun nativeFinalAlphaReportIsOwnedAndNonReadyNeverHasReport() {
        val wire = assessmentWire(rgb(128, 128, 129), 0.5, rgb(128, 128, 128))
        compileProgramWire(wire, 100u).use { runtime ->
            runtime.updateUnknown(1uL, 99u).use {
                assertEquals("waiting", it.state()); assertNull(it.assessmentReport())
            }
            runtime.updateObserved(2uL, listOf(7u), rgb(128, 128, 127), 1u).use { first ->
                assertEquals("ready", first.state())
                val report = requireNotNull(first.assessmentReport())
                // Encoded-sRGB midpoint: (129 + 127)/2 = 128; source blue 129 сохраняется отдельно.
                val pass = report.cells[0].assessment as ProgramConstraintAssessment.DeclaredSrgb8CleanSetPass
                assertEquals(listOf(128, 128, 128), listOf(pass.visible.red, pass.visible.green, pass.visible.blue).map { it.toInt() })
                assertArrayEquals(rgb(128, 128, 129), first.outputRgb(0u))
                assertEquals(0.5, first.outputOpacity(0u), 0.0)
                runtime.updateUnknown(3uL, 99u).use {
                    assertEquals("stale", it.state()); assertNull(it.assessmentReport())
                    assertArrayEquals(first.outputRgb(0u), it.outputRgb(0u))
                }
                runtime.updateObserved(4uL, listOf(7u), rgb(128, 128, 127), 1u).use {
                    assertEquals(4uL, requireNotNull(it.assessmentReport()).revision)
                }
                runtime.updateObserved(5uL, listOf(7u), rgb(0, 0, 0), 1u).use {
                    assertEquals("failed", it.state()); assertNull(it.assessmentReport())
                    assertArrayEquals(first.outputRgb(0u), it.outputRgb(0u))
                }
                val copied = requireNotNull(first.assessmentReport())
                copied.contentIdentity[0] = (copied.contentIdentity[0].toInt() xor 1).toByte()
                assertFalse(copied.contentIdentity.contentEquals(report.contentIdentity))
                assertArrayEquals(report.contentIdentity, requireNotNull(first.assessmentReport()).contentIdentity)
                assertEquals(report.cells, requireNotNull(first.assessmentReport()).cells)
                runtime.close()
                assertEquals(2uL, requireNotNull(first.assessmentReport()).revision)
                assertArrayEquals(report.contentIdentity, requireNotNull(first.assessmentReport()).contentIdentity)
            }
        }
        compileProgramWire(assessmentWire(rgb(0, 0, 0), 1.0, rgb(0, 0, 0)), 101u).use { owner ->
            owner.updateObserved(1uL, listOf(7u), rgb(0, 0, 0), 1u).use { absent ->
                assertEquals("ready", absent.state())
                val report = requireNotNull(absent.assessmentReport())
                assertEquals(3, report.cells.size)
                assertEquals(81u, report.cells[0].constraintId)
                assertFalse(report.cells[0].hard)
                assertTrue(report.cells[0].assessment is ProgramConstraintAssessment.DeclaredSrgb8CleanSetFinalOwnedDomainAbsent)
            }
        }
    }

    @Test fun nativeAttachedReportReadDoesNotInstallOrReissueAuthority() {
        val host = Host()
        attachProgramWire(hex(fixture().getString("adaptive")), 103u, 91u, 501u, 71u, 61u, host).use { attachment ->
            attachment.updateObserved(1uL, listOf(7u), rgb(255, 255, 255), 1u).use { first ->
                assertEquals("ready", first.state())
                first.render()!!.use { render ->
                    val before = host.callCount()
                    val report = requireNotNull(first.assessmentReport())
                    assertArrayEquals(render.contentIdentity(), report.contentIdentity)
                    assertEquals(103u, report.streamId); assertEquals(render.revision(), report.revision)
                    assertEquals(1, report.cells.size); assertEquals(81u, report.cells[0].constraintId)
                    assertTrue(report.cells[0].hard)
                    val wcag = report.cells[0].assessment as ProgramConstraintAssessment.Wcag22Srgb8
                    assertEquals(ProgramAssessmentVerdict.PASS, wcag.verdict)
                    assertEquals(before, host.callCount())
                    host.reject = true
                    expectCode("program_attachment_host_rejected") {
                        attachment.updateObserved(2uL, listOf(7u), rgb(0, 0, 0), 1u).close()
                    }
                    assertEquals(1uL, requireNotNull(first.assessmentReport()).revision)
                    attachment.materializationAuthority().use { assertEquals(1uL, it.revision()) }
                    host.reject = false
                    attachment.updateUnknown(2uL, 99u).use {
                        assertEquals("stale", it.state()); assertNull(it.assessmentReport()); assertFalse(it.hasRender())
                    }
                    assertNull(host.point)
                    expectCode("program_materialization_not_ready") {
                        attachment.materializationAuthorityFor(report.revision, report.contentIdentity, render.bindingEpoch()).close()
                    }
                    val afterRevoke = host.callCount()
                    val copied = requireNotNull(first.assessmentReport())
                    copied.contentIdentity[0] = (copied.contentIdentity[0].toInt() xor 1).toByte()
                    assertFalse(copied.contentIdentity.contentEquals(report.contentIdentity))
                    assertArrayEquals(report.contentIdentity, requireNotNull(first.assessmentReport()).contentIdentity)
                    assertEquals(report.cells, requireNotNull(first.assessmentReport()).cells)
                    assertEquals(afterRevoke, host.callCount())
                    attachment.updateObserved(3uL, listOf(7u, 9u), rgb(0, 0, 0, 255, 255, 255), 1u).use {
                        assertEquals("failed", it.state()); assertNull(it.assessmentReport()); assertFalse(it.hasRender())
                    }
                    assertEquals(1uL, requireNotNull(first.assessmentReport()).revision)
                }
            }
            attachment.dispose(true)
        }
    }
}


// ByteArray и records переживают native handle; текущий допуск проверяется новым вызовом Core.
private class RasterConsumer : ProgramFinalRasterConsumer {
    private val received = mutableListOf<ProgramFinalRasterBytes>()
    private var owner: ProgramRuntime? = null
    private var rejected = false
    private var failure = false
    @Synchronized fun configure(owner: ProgramRuntime? = null, rejected: Boolean = false, failure: Boolean = false) {
        this.owner = owner; this.rejected = rejected; this.failure = failure
    }
    @Synchronized fun frames(): List<ProgramFinalRasterBytes> = received.toList()
    override fun consume(raster: ProgramFinalRasterBytes): Boolean {
        val (owner, rejected, failure) = synchronized(this) { Triple(owner, rejected, failure) }
        owner?.let {
            expectCode("program_attachment_busy") { it.updateUnknown(99uL, 7u).close() }
            expectCode("program_attachment_busy") { it.finalRasterContentIdentity() }
            expectCode("program_attachment_busy") { it.finalRasterRefusal() }
            expectCode("program_attachment_busy") {
                it.consumeFinalRaster(raster.revision, ByteArray(32), raster.outputSlot, ByteArray(32) { 7 }, this)
            }
        }
        if (failure) throw ProgramFinalRasterCallbackException.InvocationFailed()
        if (rejected) return false
        synchronized(this) { received.add(raster) }
        return true
    }
}

class FinalRasterContractTest {
    private val context = ByteArray(32) { 7 }
    private fun wire(requirement: ProgramFinalRasterCleanlinessRequirement) =
        ProgramWireBuilderV4(1uL, 2uL, 3uL, 17u, requirement).finish()
    private fun update(runtime: ProgramRuntime, revision: ULong, source: ByteArray, destination: ByteArray,
                       width: UInt = 2u, height: UInt = 2u, dpr: UByte = 2u, context: ByteArray = this.context) =
        runtime.updateFinalRaster(revision, width, height, dpr, context, 1uL, source, 2uL, destination)
    private fun consume(runtime: ProgramRuntime, revision: ULong, consumer: RasterConsumer) =
        runtime.consumeFinalRaster(revision, runtime.finalRasterContentIdentity(), 17u, context, consumer)

    @Test fun everyPolicyUsesCompleteFinalPixelsAndIndependentWire() {
        val clean = ByteArray(16) { rgb(0, 199, 63, 255)[it % 4] }
        val dirty = clean.copyOf().apply { this[14] = 64 }
        val background = ByteArray(16) { rgb(0, 0, 0, 255)[it % 4] }
        val policies = listOf(ProgramFinalRasterCleanlinessRequirement.OFF to 0,
            ProgramFinalRasterCleanlinessRequirement.LINT_EXACT_NOMINAL_SRGB8_V2 to 2,
            ProgramFinalRasterCleanlinessRequirement.MANDATORY_EXACT_NOMINAL_SRGB8_V2 to 1)
        val identities = mutableListOf<ByteArray>()
        var held: ProgramFinalRasterBytes? = null
        for ((policy, tag) in policies) {
            val expected = hex("4c435057040028000000010001000000000000000200000000000000030000000000000011000000")
            expected[11] = tag.toByte()
            assertArrayEquals(expected, wire(policy))
            compileProgramWire(wire(policy), UInt.MAX_VALUE).use { runtime ->
                val identity = runtime.finalRasterContentIdentity()
                assertEquals(32, identity.size); assertFalse(identities.any { it.contentEquals(identity) }); identities.add(identity)
                val consumer = RasterConsumer()
                update(runtime, 1uL, clean, background).use {
                    assertEquals("ready", it.state()); assertNull(it.assessmentReport())
                }
                consume(runtime, 1uL, consumer)
                val saved = consumer.frames().last()
                assertEquals("ModeledFinalRasterV1", saved.scope)
                assertEquals(2u, saved.width); assertEquals(2u, saved.height); assertEquals(2u.toUByte(), saved.devicePixelRatio)
                assertEquals(1uL, saved.revision); assertEquals(17u, saved.outputSlot)
                assertArrayEquals(ByteArray(12) { rgb(0, 199, 63)[it % 3] }, saved.pixels)
                assertEquals(policy, saved.cleanliness.requirement)
                val off = policy == ProgramFinalRasterCleanlinessRequirement.OFF
                assertEquals(if (off) ProgramFinalRasterAssessmentVerdict.NOT_APPLIED else ProgramFinalRasterAssessmentVerdict.PASS,
                    saved.cleanliness.verdict)
                assertEquals(if (off) 0uL else 4uL, saved.cleanliness.checkedPixelCount)
                assertEquals(0uL, saved.cleanliness.violationCount); assertNull(saved.cleanliness.firstViolation)
                assertEquals(if (off) null else "ExactNominalSrgb8CleanSetV2", saved.cleanliness.profile)
                assertEquals(if (off) null else 32, saved.cleanliness.profileRelease?.size)
                update(runtime, 2uL, dirty, background).use { second ->
                    assertNull(second.assessmentReport())
                    if (policy == ProgramFinalRasterCleanlinessRequirement.MANDATORY_EXACT_NOMINAL_SRGB8_V2) {
                        assertEquals("failed", second.state())
                        val refusal = requireNotNull(runtime.finalRasterRefusal())
                        assertEquals("program_field_convention", refusal.code); assertEquals(3uL, refusal.pixelIndex)
                        val pixel = requireNotNull(refusal.composite); val interval = requireNotNull(refusal.rejectedBlueInterval)
                        assertEquals(listOf(0, 199, 64), listOf(pixel.red, pixel.green, pixel.blue).map { it.toInt() })
                        assertEquals(listOf(64, 108), listOf(interval.lower, interval.upper).map { it.toInt() })
                        expectCode("program_field_stale") { consume(runtime, 2uL, consumer) }
                        assertEquals(1, consumer.frames().size)
                    } else {
                        assertEquals("ready", second.state()); assertNull(runtime.finalRasterRefusal())
                        consume(runtime, 2uL, consumer)
                        val frame = consumer.frames().last()
                        assertArrayEquals(rgb(0, 199, 63, 0, 199, 63, 0, 199, 63, 0, 199, 64), frame.pixels)
                        assertEquals(if (off) ProgramFinalRasterAssessmentVerdict.NOT_APPLIED else ProgramFinalRasterAssessmentVerdict.VIOLATION,
                            frame.cleanliness.verdict)
                        assertEquals(if (off) 0uL else 4uL, frame.cleanliness.checkedPixelCount)
                        assertEquals(if (off) 0uL else 1uL, frame.cleanliness.violationCount)
                        if (off) {
                            assertNull(frame.cleanliness.firstViolation); assertNull(frame.cleanliness.profileRelease)
                        } else {
                            val witness = requireNotNull(frame.cleanliness.firstViolation)
                            assertEquals(3uL, witness.pixelIndex)
                            assertEquals(listOf(0, 199, 64), listOf(witness.composite.red, witness.composite.green, witness.composite.blue).map { it.toInt() })
                            assertEquals(listOf(64, 108), listOf(witness.rejectedBlueInterval.lower, witness.rejectedBlueInterval.upper).map { it.toInt() })
                        }
                    }
                }
                update(runtime, 3uL, clean, background).use { assertEquals("ready", it.state()) }
                consume(runtime, 3uL, consumer)
                val copied = saved.pixels.copyOf().apply { this[0] = -1 }
                assertFalse(copied.contentEquals(saved.pixels))
                assertArrayEquals(saved.pixels, consumer.frames().first().pixels)
                assertArrayEquals(saved.pixels, consumer.frames().last().pixels)
                saved.cleanliness.profileRelease?.let { release ->
                    val changed = release.copyOf().apply { fill(0) }
                    assertFalse(changed.contentEquals(release))
                    assertArrayEquals(release, consumer.frames().last().cleanliness.profileRelease)
                }
                if (policy == ProgramFinalRasterCleanlinessRequirement.LINT_EXACT_NOMINAL_SRGB8_V2) {
                    val multiple = dirty.copyOf().apply { this[6] = 64 }
                    update(runtime, 4uL, multiple, background).use { assertEquals("ready", it.state()) }
                    consume(runtime, 4uL, consumer)
                    val report = consumer.frames().last().cleanliness
                    assertEquals(4uL, report.checkedPixelCount); assertEquals(2uL, report.violationCount)
                    assertEquals(1uL, report.firstViolation?.pixelIndex)
                }
                held = saved
            }
            assertEquals(1uL, requireNotNull(held).revision)
            assertArrayEquals(ByteArray(12) { rgb(0, 199, 63)[it % 3] }, requireNotNull(held).pixels)
        }
    }

    @Test fun lockedAlphaChangedBackgroundStaleContextAndRecovery() {
        // Половинный foreground: 100 + round(199*127/255)=199; 32 + round(62*127/255)=63.
        val source = rgb(0, 100, 32, 128); val clean = rgb(0, 199, 62, 255); val dirty = rgb(0, 199, 64, 255)
        val policies = listOf(ProgramFinalRasterCleanlinessRequirement.LINT_EXACT_NOMINAL_SRGB8_V2,
            ProgramFinalRasterCleanlinessRequirement.MANDATORY_EXACT_NOMINAL_SRGB8_V2)
        for (policy in policies) compileProgramWire(wire(policy), 1u).use { runtime ->
            val consumer = RasterConsumer()
            update(runtime, 1uL, source, clean, 1u, 1u).use { assertEquals("ready", it.state()) }
            consume(runtime, 1uL, consumer)
            val saved = consumer.frames().last()
            assertArrayEquals(rgb(0, 199, 63), saved.pixels); assertEquals(ProgramFinalRasterAssessmentVerdict.PASS, saved.cleanliness.verdict)
            update(runtime, 2uL, source, dirty, 1u, 1u).use {
                assertEquals(if (policy == ProgramFinalRasterCleanlinessRequirement.MANDATORY_EXACT_NOMINAL_SRGB8_V2) "failed" else "ready", it.state())
            }
            expectCode("program_field_stale") { consume(runtime, 1uL, consumer) }
            if (policy == ProgramFinalRasterCleanlinessRequirement.LINT_EXACT_NOMINAL_SRGB8_V2) {
                consume(runtime, 2uL, consumer)
                assertArrayEquals(rgb(0, 199, 64), consumer.frames().last().pixels)
                assertEquals(ProgramFinalRasterAssessmentVerdict.VIOLATION, consumer.frames().last().cleanliness.verdict)
            }
            runtime.updateUnknown(3uL, 55u).use { assertEquals("stale", it.state()) }
            expectCode("program_field_stale") { consume(runtime, 2uL, consumer) }
            val newContext = ByteArray(32) { 8 }
            update(runtime, 4uL, source, clean, 1u, 1u, context = newContext).use { assertEquals("ready", it.state()) }
            val count = consumer.frames().size
            expectCode("program_field_context") { consume(runtime, 4uL, consumer) }
            assertEquals(count, consumer.frames().size)
            runtime.consumeFinalRaster(4uL, runtime.finalRasterContentIdentity(), 17u, newContext, consumer)
            assertArrayEquals(saved.pixels, consumer.frames().last().pixels)
            assertEquals(1uL, saved.revision); assertEquals(ProgramFinalRasterAssessmentVerdict.PASS, saved.cleanliness.verdict)
            assertArrayEquals(rgb(0, 100, 32, 128), source); assertArrayEquals(rgb(0, 199, 64, 255), dirty)
        }
    }

    @Test fun admissionBindingsCallbackFailuresAndReentryPreserveCurrentFrame() {
        compileProgramWire(wire(ProgramFinalRasterCleanlinessRequirement.LINT_EXACT_NOMINAL_SRGB8_V2), 1u).use { runtime ->
            val source = ByteArray(16); val background = ByteArray(16) { rgb(64, 64, 64, 255)[it % 4] }
            val consumer = RasterConsumer()
            update(runtime, 1uL, source, background).use { assertEquals("ready", it.state()) }
            update(runtime, 1uL, source, background).use { assertEquals("ready", it.state()) }
            expectCode("program_field_length") { update(runtime, 2uL, byteArrayOf(0), background).close() }
            expectCode("program_field_dpr") { update(runtime, 2uL, source, background, dpr = 5u).close() }
            expectCode("program_resource_exhausted") { update(runtime, 2uL, source, background, width = 65537u, height = 1u).close() }
            val altered = background.copyOf().apply { this[12] = 65 }
            expectCode("program_field_update") { update(runtime, 1uL, source, altered).close() }
            val identity = runtime.finalRasterContentIdentity()
            expectCode("program_field_program") { runtime.consumeFinalRaster(1uL, ByteArray(32), 17u, context, consumer) }
            expectCode("program_field_output") { runtime.consumeFinalRaster(1uL, identity, 18u, context, consumer) }
            expectCode("program_field_context") { runtime.consumeFinalRaster(1uL, identity, 17u, ByteArray(32) { 8 }, consumer) }
            assertTrue(consumer.frames().isEmpty())
            consumer.configure(owner = runtime); consume(runtime, 1uL, consumer)
            consumer.configure(rejected = true)
            expectCode("program_field_consumer_rejected") { consume(runtime, 1uL, consumer) }
            consumer.configure(failure = true)
            expectCode("program_field_consumer_failed") { consume(runtime, 1uL, consumer) }
            consumer.configure(); consume(runtime, 1uL, consumer)
            assertEquals(2, consumer.frames().size); assertArrayEquals(consumer.frames()[0].pixels, consumer.frames()[1].pixels)
            val transparent = ByteArray(16) { rgb(64, 64, 64, 128)[it % 4] }
            update(runtime, 2uL, source, transparent).use { assertEquals("failed", it.state()) }
            val refusal = requireNotNull(runtime.finalRasterRefusal())
            assertEquals("program_field_transparent", refusal.code); assertEquals(0uL, refusal.pixelIndex)
            assertNull(refusal.composite); assertNull(refusal.rejectedBlueInterval)
            expectCode("program_field_stale") { consume(runtime, 2uL, consumer) }
            assertEquals(2, consumer.frames().size)
            update(runtime, 3uL, source, background).use { assertEquals("ready", it.state()) }
            consume(runtime, 3uL, consumer)
            assertArrayEquals(consumer.frames().first().pixels, consumer.frames().last().pixels)
        }
    }
}
