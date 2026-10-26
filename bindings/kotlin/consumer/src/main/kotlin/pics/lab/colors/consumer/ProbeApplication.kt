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
    }
}
