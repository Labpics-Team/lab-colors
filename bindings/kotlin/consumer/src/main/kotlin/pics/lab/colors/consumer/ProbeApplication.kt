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
        exerciseAssignment(ProgramWireDomainV3.Gray(0u, 255u), "unsatisfied", null)
        exerciseAssignment(ProgramWireDomainV3.Box(byteArrayOf(0, 0, 0), byteArrayOf(-1, -1, -1)), "incomplete", "limit")
    }

    private fun exerciseAssignment(domain: ProgramWireDomainV3, emptyState: String, reason: String?) {
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
                    check(it.snapshotVersion() == 2u && it.revision() == 2uL && it.reason() == reason)
                    check(it.state() == emptyState && it.outputCount() == 0uL && !it.hasRender())
                }
                check(installed == null)
                attachment.updateObserved(ULong.MAX_VALUE, listOf(1u), byteArrayOf(0, 0, 0), 1u).use {
                    check(it.revision() == ULong.MAX_VALUE && it.state() == "ready")
                    check(it.outputRgb(0u).contentEquals(byteArrayOf(-65, -65, -65)))
                }
                attachment.materializationAuthority().use { check(it.terminalCompositeRgb().contentEquals(byteArrayOf(96, 96, 96))) }
                check(first.outputRgb(0u).contentEquals(byteArrayOf(64, 64, 64)))
            }
            installed = null; attachment.dispose(true)
        }
    }
}
