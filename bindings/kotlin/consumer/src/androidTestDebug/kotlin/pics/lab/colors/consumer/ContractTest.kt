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

private class Host : ProgramPointSinkHost {
    var owner = WeakReference<ProgramAttachment>(null)
    var reject = false
    var fail = false
    var reenter = false
    var point: ByteArray? = null
    var sequence = 0uL
    var epoch: ULong? = null
    @Synchronized override fun tryInstall(intent: ProgramPointSinkIntent): Boolean {
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
            ProgramPointSinkOperation.SET_ALL, ProgramPointSinkOperation.CONFIRM_EXACT -> {
                val paint = requireNotNull(intent.point)
                assertEquals(91u, paint.slot); assertEquals(3, paint.source.size); assertEquals(1.0, paint.opacity, 0.0)
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
                    if (row.has("rgb")) {
                        assertEquals(1uL, snapshot.outputCount())
                        assertEquals(91u, snapshot.outputSlot(0u))
                        assertArrayEquals(row.getJSONArray("rgb").rgb(), snapshot.outputRgb(0u))
                    }
                }
                val first = held.first()
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
}
