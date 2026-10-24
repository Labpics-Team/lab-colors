package pics.lab.colors

/** Ошибка авторинга не меняет ранее принятую декларацию. */
sealed class ProgramWireException(message: String) : IllegalArgumentException(message) {
    class InvalidDeclaration(message: String) : ProgramWireException(message)
    class TooManyEntries : ProgramWireException("Program wire exceeds its encoding limits")
}

data class ProgramCandidate(val id: UInt, val rgb: ByteArray, val opacity: Double)
data class ProgramSelectionChoice(val target: UInt, val candidate: UInt)
data class ProgramSelectionState(val id: UInt, val choices: List<ProgramSelectionChoice>)
enum class ProgramSurround(internal val tag: Int) { AVERAGE(1), DIM(2), DARK(3) }

private class WireBytes {
    private var bytes = ByteArray(32)
    var size = 0
        private set
    fun truncate(length: Int) { size = length }
    fun raw(value: ByteArray) {
        reserve(value.size)
        value.copyInto(bytes, size)
        size += value.size
    }
    private fun reserve(count: Int) {
        val needed = size.toLong() + count
        if (needed > Int.MAX_VALUE) throw ProgramWireException.TooManyEntries()
        if (needed > bytes.size) {
            bytes = bytes.copyOf(maxOf(needed, minOf(Int.MAX_VALUE.toLong(), bytes.size.toLong() * 2)).toInt())
        }
    }
    fun u8(value: Int) { reserve(1); bytes[size++] = value.toByte() }
    fun u16(value: Int) { repeat(2) { u8(value ushr (it * 8)) } }
    fun u32(value: UInt) { repeat(4) { u8((value shr (it * 8)).toInt()) } }
    fun u64(value: ULong) { repeat(8) { u8((value shr (it * 8)).toInt()) } }
    fun f64(value: Double) { u64(value.toRawBits().toULong()) }
    fun rgb(value: ByteArray) {
        if (value.size != 3) throw ProgramWireException.InvalidDeclaration("RGB requires three channels")
        raw(value)
    }
    fun copyInto(target: ByteArray, offset: Int) { bytes.copyInto(target, offset, 0, size) }
}

private class WireSection { var count = 0; val bytes = WireBytes() }

/** Кодирует LCPW v1; семантикой графа и выбором цветов владеет Rust Core.
 * Один изменяемый builder принадлежит одному вызывающему контексту.
 */
open class ProgramWireBuilderV1 internal constructor(private val wireVersion: Int) {
    constructor() : this(1)
    private val sections = Array(14) { WireSection() }
    private var length = 66
    private var writing = false

    private fun entry(index: Int, encode: (WireBytes) -> Unit): ProgramWireBuilderV1 {
        if (writing) throw ProgramWireException.InvalidDeclaration("reentrant authoring")
        val section = sections[index]
        if (section.count >= 4096) throw ProgramWireException.TooManyEntries()
        val previous = section.bytes.size
        writing = true
        try {
            encode(section.bytes)
            val next = length.toLong() + section.bytes.size - previous
            if (next > Int.MAX_VALUE) throw ProgramWireException.TooManyEntries()
            length = next.toInt()
            section.count++
        } catch (error: Throwable) {
            section.bytes.truncate(previous)
            throw error
        } finally { writing = false }
        return this
    }
    private fun count(value: Int): UInt {
        if (value <= 0) throw ProgramWireException.InvalidDeclaration("list must not be empty")
        if (value > 4096) throw ProgramWireException.TooManyEntries()
        return value.toUInt()
    }
    private fun opacity(value: Double): Double {
        if (!value.isFinite() || value < 0 || value > 1) {
            throw ProgramWireException.InvalidDeclaration("opacity must be in [0, 1]")
        }
        return if (value == 0.0) 0.0 else value
    }
    fun source(id: UInt, rgb: ByteArray) = entry(0) { it.u32(id); it.rgb(rgb) }
    fun fixedTarget(id: UInt, source: UInt) = entry(1) { it.u32(id); it.u8(1); it.u32(source) }
    fun finiteTarget(id: UInt, candidates: List<ProgramCandidate>) = entry(1) { out ->
        out.u32(id); out.u8(2); out.u32(count(candidates.size))
        for (candidate in candidates) {
            out.u32(candidate.id); out.rgb(candidate.rgb); out.f64(opacity(candidate.opacity))
        }
    }
    fun family(id: UInt, releaseBytes: ByteArray) = entry(2) {
        if (releaseBytes.size != 32) throw ProgramWireException.InvalidDeclaration("family release requires 32 bytes")
        it.u32(id); it.raw(releaseBytes)
    }
    fun surfaceInputPort(id: UInt) = entry(4) { it.u32(id) }
    fun opacityInput(id: UInt, value: Double) = entry(5) { it.u32(id); it.f64(opacity(value)) }
    fun solidPaint(id: UInt, target: UInt) = entry(6) { it.u32(id); it.u8(1); it.u32(target) }
    fun opacityPaint(id: UInt, source: UInt, opacity: UInt) = entry(6) {
        it.u32(id); it.u8(2); it.u32(source); it.u32(opacity)
    }
    fun inputSurface(id: UInt, input: UInt) = entry(7) { it.u32(id); it.u8(1); it.u32(input) }
    fun occurrenceSurface(id: UInt, occurrence: UInt) = entry(7) { it.u32(id); it.u8(2); it.u32(occurrence) }
    fun sourceOverOccurrence(id: UInt, subject: UInt, against: UInt,
                             adaptingLuminance: Double, backgroundRatio: Double, surround: ProgramSurround) = entry(8) {
        if (!adaptingLuminance.isFinite() || adaptingLuminance <= 0 || !backgroundRatio.isFinite() ||
            backgroundRatio <= 0 || backgroundRatio > 1) {
            throw ProgramWireException.InvalidDeclaration("invalid viewing conditions")
        }
        it.u32(id); it.u32(subject); it.u32(against)
        it.f64(adaptingLuminance); it.f64(backgroundRatio); it.u8(surround.tag)
    }
    fun presentationRoot(id: UInt, terminal: UInt) = entry(9) { it.u32(id); it.u32(terminal) }
    fun presentationTarget(root: UInt, occurrence: UInt) = entry(10) { it.u32(root); it.u32(occurrence) }
    fun exactVisibleUnary(hard: Boolean, id: UInt, occurrence: UInt, expectedRgb: ByteArray) = entry(if (hard) 11 else 12) {
        it.u32(id); it.u8(1); it.u32(occurrence); it.rgb(expectedRgb)
    }
    fun wcag22VisibleUnary(hard: Boolean, id: UInt, occurrence: UInt, criterion: Wcag22Criterion) = entry(if (hard) 11 else 12) {
        val tag = when (criterion) {
            Wcag22Criterion.SC143_TEXT_DEFAULT -> 1
            Wcag22Criterion.SC143_TEXT_LARGE_SCALE -> 2
            Wcag22Criterion.SC1411_UI_COMPONENT_OR_STATE -> 3
            Wcag22Criterion.SC1411_GRAPHICAL_OBJECT -> 4
        }
        it.u32(id); it.u8(9); it.u32(occurrence); it.u8(tag)
    }
    fun declaredSrgb8CleanSet(hard: Boolean, id: UInt, root: UInt, occurrence: UInt) = entry(if (hard) 11 else 12) {
        it.u32(id); it.u8(10); it.u32(root); it.u32(occurrence)
    }
    fun exactIntrinsicRelationHard(id: UInt, reference: UInt, candidates: List<UInt>) = entry(11) {
        it.u32(id); it.u8(4); it.u32(reference); it.u32(count(candidates.size))
        for (candidate in candidates) it.u32(candidate)
    }
    fun output(slot: UInt, paint: UInt) = entry(13) { it.u32(slot); it.u32(paint) }

    protected fun selection(revision: ULong, rankGroups: List<List<ProgramSelectionState>>) {
        if (sections[3].count != 0) throw ProgramWireException.InvalidDeclaration("selection release already exists")
        entry(3) { out ->
            out.u64(revision); out.u32(count(rankGroups.size))
            var states = 0
            var choices = 0
            for (group in rankGroups) {
                out.u32(count(group.size)); states += group.size
                if (states > 4096) throw ProgramWireException.TooManyEntries()
                for (state in group) {
                    out.u32(state.id); out.u32(count(state.choices.size)); choices += state.choices.size
                    if (choices > 4096) throw ProgramWireException.TooManyEntries()
                    for (choice in state.choices) { out.u32(choice.target); out.u32(choice.candidate) }
                }
            }
        }
    }
    fun finish(): ByteArray {
        if (writing) throw ProgramWireException.InvalidDeclaration("reentrant authoring")
        val out = ByteArray(length)
        val header = WireBytes().apply { raw(byteArrayOf(76, 67, 80, 87)); u16(wireVersion); u32(length.toUInt()) }
        header.copyInto(out, 0)
        var offset = 10
        for (section in sections) {
            repeat(4) { out[offset++] = (section.count ushr (it * 8)).toByte() }
            section.bytes.copyInto(out, offset)
            offset += section.bytes.size
        }
        return out
    }
}

/** LCPW v2 добавляет предпочтения полных состояний, не второй решатель. */
class ProgramWireBuilderV2 : ProgramWireBuilderV1(2) {
    fun selectionRelease(revision: ULong, rankGroups: List<List<ProgramSelectionState>>): ProgramWireBuilderV2 {
        selection(revision, rankGroups)
        return this
    }
}
