package pics.lab.colors.consumer

import android.app.Application
import pics.lab.colors.*

/** Настоящий путь приложения после R8; тест не удерживает SDK через keep-правила. */
class ProbeApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        val status = try { exercise(); "ok" } catch (failure: Throwable) { failure.toString() }
        getSharedPreferences("labcolors", MODE_PRIVATE).edit().putString("result", status).commit()
    }
    private fun exercise() {
        val wire = ProgramWireBuilderV2().apply {
            finiteTarget(21u, listOf(ProgramCandidate(201u, byteArrayOf(0, 0, 0), 1.0),
                ProgramCandidate(202u, byteArrayOf(-1, -1, -1), 1.0)))
            selectionRelease(1uL, listOf(
                listOf(ProgramSelectionState(1u, listOf(ProgramSelectionChoice(21u, 201u)))),
                listOf(ProgramSelectionState(2u, listOf(ProgramSelectionChoice(21u, 202u))))))
            surfaceInputPort(31u); solidPaint(41u, 21u); inputSurface(51u, 31u)
            sourceOverOccurrence(61u, 41u, 51u, 64.0, 0.2, ProgramSurround.AVERAGE)
            presentationRoot(71u, 61u); presentationTarget(71u, 61u)
            wcag22VisibleUnary(true, 81u, 61u, Wcag22Criterion.SC143_TEXT_DEFAULT); output(91u, 41u)
        }.finish()
        var installed: ByteArray? = null
        val host = object : ProgramPointSinkHost {
            override fun tryInstall(intent: ProgramPointSinkIntent): Boolean {
                check(intent.sinkOutput == 501u)
                installed = intent.point?.source
                return true
            }
        }
        attachProgramWire(wire, UInt.MAX_VALUE, 91u, 501u, 71u, 61u, host).use { attachment ->
            attachment.updateObserved(1uL, listOf(1u), byteArrayOf(-1, -1, -1), 1u).use { first ->
                check(first.state() == "ready")
                check(first.outputRgb(0u).contentEquals(byteArrayOf(0, 0, 0)))
            }
            check(installed?.contentEquals(byteArrayOf(0, 0, 0)) == true)
            attachment.updateObserved(2uL, listOf(1u), byteArrayOf(0, 0, 0), 1u).close()
            attachment.materializationAuthority().use { check(it.terminalCompositeRgb().contentEquals(byteArrayOf(-1, -1, -1))) }
            attachment.updateUnknown(3uL, 7u).close(); check(installed == null)
            attachment.dispose(true)
        }
        check(evaluateWcag22("#000000", "#FFFFFF", Wcag22Criterion.SC143_TEXT_DEFAULT).decision == Wcag22Decision.PASS)
        check(numericalCapabilityManifest().sites.isNotEmpty())
        check(decodeCertificateEnvelope(issueSourceCertificateEnvelope()).producerContentIdentity.size == 32)
        exerciseAssignment(ProgramWireDomainV3.Gray(0u, 255u))
        exerciseAssignment(ProgramWireDomainV3.Box(byteArrayOf(0, 0, 0), byteArrayOf(-1, -1, -1)))
        exerciseFinalRaster()
    }

    private fun exerciseFinalRaster() {
        val policies = listOf(ProgramFinalRasterCleanlinessRequirement.OFF,
            ProgramFinalRasterCleanlinessRequirement.LINT_EXACT_NOMINAL_SRGB8_V2,
            ProgramFinalRasterCleanlinessRequirement.MANDATORY_EXACT_NOMINAL_SRGB8_V2)
        val context = ByteArray(32) { 7 }
        val source = byteArrayOf(64, 64, 64, -128, 0, 0, 0, 0)
        val background = byteArrayOf(-128, -128, -128, -1, 64, 64, 64, -1)
        for (policy in policies) {
            val wire = ProgramWireBuilderV4(1uL, 2uL, 3uL, 17u, policy).finish()
            compileProgramWire(wire, UInt.MAX_VALUE).use { runtime ->
                var received = 0
                var frame: ProgramFinalRasterBytes? = null
                val consumer = object : ProgramFinalRasterConsumer {
                    override fun consume(raster: ProgramFinalRasterBytes): Boolean {
                        received++; frame = raster; return true
                    }
                }
                runtime.updateFinalRaster(1uL, 2u, 1u, 2u, context, 1uL, source, 2uL, background).use {
                    check(it.state() == "ready" && it.assessmentReport() == null)
                }
                val identity = runtime.finalRasterContentIdentity()
                runtime.consumeFinalRaster(1uL, identity, 17u, context, consumer)
                val saved = requireNotNull(frame)
                check(saved.scope == "ModeledFinalRasterV1" && saved.width == 2u && saved.height == 1u)
                check(saved.devicePixelRatio == 2u.toUByte() && saved.outputSlot == 17u)
                check(saved.pixels.contentEquals(byteArrayOf(-128, -128, -128, 64, 64, 64)))
                check(saved.cleanliness.requirement == policy)
                val off = policy == ProgramFinalRasterCleanlinessRequirement.OFF
                check(saved.cleanliness.verdict == if (off) ProgramFinalRasterAssessmentVerdict.NOT_APPLIED else ProgramFinalRasterAssessmentVerdict.PASS)
                check(saved.cleanliness.checkedPixelCount == if (off) 0uL else 2uL)
                val dirty = byteArrayOf(64, 64, 64, -128, 0, -57, 64, -1)
                runtime.updateFinalRaster(2uL, 2u, 1u, 2u, context, 1uL, dirty, 2uL, background).use { snapshot ->
                    if (policy == ProgramFinalRasterCleanlinessRequirement.MANDATORY_EXACT_NOMINAL_SRGB8_V2) {
                        check(snapshot.state() == "failed")
                        val refusal = requireNotNull(runtime.finalRasterRefusal())
                        check(refusal.code == "program_field_convention" && refusal.pixelIndex == 1uL)
                        try { runtime.consumeFinalRaster(2uL, identity, 17u, context, consumer); error("failed raster consumed") }
                        catch (refused: LabColorsException.Refused) { check(refused.code == "program_field_stale") }
                        check(received == 1)
                    } else {
                        check(snapshot.state() == "ready" && runtime.finalRasterRefusal() == null)
                        runtime.consumeFinalRaster(2uL, identity, 17u, context, consumer)
                        val current = requireNotNull(frame)
                        check(current.pixels.contentEquals(byteArrayOf(-128, -128, -128, 0, -57, 64)))
                        if (!off) {
                            check(current.cleanliness.verdict == ProgramFinalRasterAssessmentVerdict.VIOLATION)
                            val witness = requireNotNull(current.cleanliness.firstViolation)
                            check(current.cleanliness.violationCount == 1uL && witness.pixelIndex == 1uL)
                            check(witness.composite.green == 199u.toUByte() && witness.rejectedBlueInterval.upper == 108u.toUByte())
                            check(current.cleanliness.profile == "ExactNominalSrgb8CleanSetV2" && current.cleanliness.profileRelease?.size == 32)
                        }
                    }
                }
                runtime.updateFinalRaster(3uL, 2u, 1u, 2u, context, 1uL, source, 2uL, background).close()
                runtime.consumeFinalRaster(3uL, identity, 17u, context, consumer)
                check(requireNotNull(frame).pixels.contentEquals(saved.pixels))
            }
        }
    }

    private fun exerciseAssignment(domain: ProgramWireDomainV3) {
        val wire = ProgramWireBuilderV3().computedTarget(21u, domain, byteArrayOf(64, 64, 64), 1.0).assignment(listOf(21u)).apply {
            surfaceInputPort(31u); opacityInput(32u, 0.5); solidPaint(41u, 21u); opacityPaint(42u, 41u, 32u)
            inputSurface(51u, 31u); sourceOverOccurrence(61u, 42u, 51u, 64.0, 0.2, ProgramSurround.DIM)
            presentationRoot(71u, 61u); presentationTarget(71u, 61u)
            exactVisibleUnary(true, 81u, 61u, byteArrayOf(96, 96, 96)); output(91u, 42u)
        }.finish()
        var installed: ByteArray? = null
        val host = object : ProgramPointSinkHost {
            override fun tryInstall(intent: ProgramPointSinkIntent): Boolean {
                check(intent.sinkOutput == 501u)
                intent.point?.let { check(it.slot == 91u && it.opacity == 0.5) }
                installed = intent.point?.source
                return true
            }
        }
        attachProgramWire(wire, UInt.MAX_VALUE, 91u, 501u, 71u, 61u, host).use { attachment ->
            attachment.updateObserved(1uL, listOf(1u), byteArrayOf(-128, -128, -128), 1u).use { first ->
                check(first.snapshotVersion() == 2u && first.revision() == 1uL && first.reason() == null)
                check(first.state() == "ready" && first.outputRgb(0u).contentEquals(byteArrayOf(64, 64, 64)))
                check(installed?.contentEquals(byteArrayOf(64, 64, 64)) == true)
                attachment.updateObserved(2uL, listOf(1u), byteArrayOf(-1, -1, -1), 1u).use {
                    check(it.snapshotVersion() == 2u && it.revision() == 2uL && it.reason() == null)
                    check(it.state() == "unsatisfied" && it.outputCount() == 0uL && !it.hasRender())
                }
                check(installed == null)
                attachment.updateObserved(3uL, listOf(1u), byteArrayOf(0, 0, 0), 1u).use {
                    check(it.state() == "ready" && it.reason() == null)
                    check(it.outputRgb(0u).contentEquals(byteArrayOf(-65, -65, -65)))
                }
                attachment.updateObserved(ULong.MAX_VALUE, listOf(1u), byteArrayOf(-128, -128, -128), 1u).use {
                    check(it.revision() == ULong.MAX_VALUE && it.state() == "ready" && it.reason() == null)
                    check(it.outputRgb(0u).contentEquals(byteArrayOf(64, 64, 64)))
                }
                attachment.materializationAuthority().use { check(it.terminalCompositeRgb().contentEquals(byteArrayOf(96, 96, 96))) }
                check(first.outputRgb(0u).contentEquals(byteArrayOf(64, 64, 64)))
            }
            installed = null; attachment.dispose(true)
        }
    }
}
