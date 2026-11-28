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

sealed class ProgramWireDomainV3 {
    data class Box(val lower: ByteArray, val upper: ByteArray) : ProgramWireDomainV3()
    data class Gray(val lower: UByte, val upper: UByte) : ProgramWireDomainV3()
}

enum class ProgramWireColorDomainV3(internal val tag: Int) { SRGB(0), NEUTRAL(1) }
data class ProgramWireColorIdentityV3(
    val maxHueDeviation: Double? = null,
    val chroma: Pair<Double, Double>? = null,
    val lightness: Pair<Double, Double>? = null,
)
data class ProgramWireToneV3(val from: UInt, val curve: List<Pair<Double, Double>>)
sealed interface ProgramWireToneCoordinateV3 {
    data class Lightness(val value: Double) : ProgramWireToneCoordinateV3
    class Swatch(val rgb: ByteArray) : ProgramWireToneCoordinateV3
}
data class ProgramWireColorOptionsV3(
    val domain: ProgramWireColorDomainV3 = ProgramWireColorDomainV3.SRGB,
    val identity: ProgramWireColorIdentityV3? = null,
    val maxChange: Double? = null,
    val appearance: UInt? = null,
    val tone: ProgramWireToneV3? = null,
    val referenceAppearance: UInt? = null,
) {
    // Скомпилированный SDK v0.3 вызывает эти JVM-сигнатуры вместе с default-mask bridges.
    @Deprecated("Binary compatibility with v0.3", level = DeprecationLevel.HIDDEN)
    constructor(
        domain: ProgramWireColorDomainV3 = ProgramWireColorDomainV3.SRGB,
        identity: ProgramWireColorIdentityV3? = null,
        maxChange: Double? = null,
        appearance: UInt? = null,
        tone: ProgramWireToneV3? = null,
    ) : this(domain, identity, maxChange, appearance, tone, null)

    @Deprecated("Binary compatibility with v0.3", level = DeprecationLevel.HIDDEN)
    fun copy(
        domain: ProgramWireColorDomainV3 = this.domain,
        identity: ProgramWireColorIdentityV3? = this.identity,
        maxChange: Double? = this.maxChange,
        appearance: UInt? = this.appearance,
        tone: ProgramWireToneV3? = this.tone,
    ): ProgramWireColorOptionsV3 = ProgramWireColorOptionsV3(domain, identity, maxChange, appearance, tone, referenceAppearance)
}

data class ProgramWireReferenceInputV3(val port: UInt, val rgb: ByteArray)

data class ProgramWireAssignmentOptionsV3(
    /** null omits the reference; an empty list explicitly declares a reference without input ports. */
    val reference: List<ProgramWireReferenceInputV3>? = null,
)

data class ProgramWireComposeOptionsV3(
    /** null omits the reference; an empty list explicitly declares a reference without input ports. */
    val reference: List<ProgramWireReferenceInputV3>? = null,
)

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
        val size = count(candidates.size).toInt()
        out.u32(id); out.u8(2); out.u32(size.toUInt())
        for (index in 0 until size) {
            val candidate = candidates[index]
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
    fun nominalSrgb8Visible(hard: Boolean, id: UInt, occurrence: UInt) = entry(if (hard) 11 else 12) {
        it.u32(id); it.u8(12); it.u32(occurrence)
    }
    fun exactIntrinsicRelationHard(id: UInt, reference: UInt, candidates: List<UInt>) = entry(11) {
        val size = count(candidates.size).toInt()
        it.u32(id); it.u8(4); it.u32(reference); it.u32(size.toUInt())
        for (index in 0 until size) it.u32(candidates[index])
    }
    fun output(slot: UInt, paint: UInt) = entry(13) { it.u32(slot); it.u32(paint) }

    internal fun selection(revision: ULong, rankGroups: List<List<ProgramSelectionState>>) {
        if (wireVersion != 2) throw ProgramWireException.InvalidDeclaration("selection release requires wire version 2")
        if (sections[3].count != 0) throw ProgramWireException.InvalidDeclaration("selection release already exists")
        entry(3) { out ->
            val groupCount = count(rankGroups.size).toInt()
            out.u64(revision); out.u32(groupCount.toUInt())
            var states = 0
            var choices = 0
            for (groupIndex in 0 until groupCount) {
                val group = rankGroups[groupIndex]
                val stateCount = count(group.size).toInt()
                out.u32(stateCount.toUInt()); states += stateCount
                if (states > 4096) throw ProgramWireException.TooManyEntries()
                for (stateIndex in 0 until stateCount) {
                    val state = group[stateIndex]
                    val choiceCount = count(state.choices.size).toInt()
                    out.u32(state.id); out.u32(choiceCount.toUInt()); choices += choiceCount
                    if (choices > 4096) throw ProgramWireException.TooManyEntries()
                    for (choiceIndex in 0 until choiceCount) {
                        val choice = state.choices[choiceIndex]
                        out.u32(choice.target); out.u32(choice.candidate)
                    }
                }
            }
        }
    }

    private fun v3Entry(index: Int, encode: (WireBytes) -> Unit) {
        if (wireVersion != 3) throw ProgramWireException.InvalidDeclaration("computed declarations require wire version 3")
        entry(index, encode)
    }
    private fun bounded(value: Double, maximum: Double, name: String): Double {
        if (!value.isFinite() || value < 0 || value > maximum) {
            throw ProgramWireException.InvalidDeclaration("$name must be in [0, $maximum]")
        }
        return if (value == 0.0) 0.0 else value
    }
    private fun writeRange(out: WireBytes, pair: Pair<Double, Double>, name: String) {
        val lower = bounded(pair.first, 1.0, name)
        val upper = bounded(pair.second, 1.0, name)
        if (lower > upper) throw ProgramWireException.InvalidDeclaration("$name bounds must be increasing")
        out.f64(lower); out.f64(upper)
    }
    private fun writeToneCoordinates(out: WireBytes, from: UInt?,
                                     points: List<Pair<ProgramWireToneCoordinateV3, ProgramWireToneCoordinateV3>>) {
        val size = count(points.size).toInt()
        if (size < 2) throw ProgramWireException.InvalidDeclaration("tone curve requires at least two points")
        val sampled = points.any { it.first is ProgramWireToneCoordinateV3.Swatch || it.second is ProgramWireToneCoordinateV3.Swatch }
        val numericInputs = points.all { it.first is ProgramWireToneCoordinateV3.Lightness }
        out.u8((if (from == null) 2 else 1) + (if (sampled) 2 else 0))
        if (from != null) out.u32(from)
        out.u32(size.toUInt())
        fun write(coordinate: ProgramWireToneCoordinateV3) {
            when (coordinate) {
                is ProgramWireToneCoordinateV3.Lightness -> out.f64(bounded(coordinate.value, 1.0, "tone coordinate"))
                is ProgramWireToneCoordinateV3.Swatch -> out.rgb(coordinate.rgb)
            }
        }
        var previous = -1.0
        for ((index, point) in points.withIndex()) {
            val (x, y) = point
            if (numericInputs && x is ProgramWireToneCoordinateV3.Lightness) {
                if (x.value <= previous || (index == 0 && x.value != 0.0) || (index == size - 1 && x.value != 1.0)) {
                    throw ProgramWireException.InvalidDeclaration("tone inputs must increase from 0 to 1")
                }
                previous = x.value
            }
            if (sampled) out.u8((if (x is ProgramWireToneCoordinateV3.Swatch) 1 else 0) or
                (if (y is ProgramWireToneCoordinateV3.Swatch) 2 else 0))
            write(x); write(y)
        }
    }

    internal fun colorTargetV3(id: UInt, anchor: ByteArray, options: ProgramWireColorOptionsV3,
                               backdropTone: List<Pair<Double, Double>>?,
                               coordinates: List<Pair<ProgramWireToneCoordinateV3, ProgramWireToneCoordinateV3>>? = null,
                               from: UInt? = null) = v3Entry(1) { out ->
        val identity = options.identity
        val hue = identity?.maxHueDeviation
        val chroma = identity?.chroma
        val lightness = identity?.lightness
        out.u32(id); out.u8(4); out.rgb(anchor); out.u8(options.domain.tag)
        out.u8((if (hue != null) 1 else 0) or (if (chroma != null) 2 else 0) or (if (lightness != null) 4 else 0))
        if (hue != null) out.f64(bounded(hue, 180.0, "identity maxHueDeviation"))
        if (chroma != null) writeRange(out, chroma, "identity chroma")
        if (lightness != null) writeRange(out, lightness, "identity lightness")
        // Omitted identity fields remain absent; Core owns the color defaults.
        out.f64(bounded(options.maxChange ?: 2.0, 2.0, "color maxChange"))
        val appearance = options.appearance
        val referenceAppearance = options.referenceAppearance
        if (appearance != null && referenceAppearance != null) {
            throw ProgramWireException.InvalidDeclaration("appearance and referenceAppearance are mutually exclusive")
        }
        out.u8(if (referenceAppearance != null) 2 else if (appearance != null) 1 else 0)
        val visible = referenceAppearance ?: appearance
        if (visible != null) out.u32(visible)
        val tone = options.tone
        if (coordinates != null) {
            if (tone != null || (from == null && visible == null)) {
                throw ProgramWireException.InvalidDeclaration("declare one tone source and an appearance for backdrop tone")
            }
            writeToneCoordinates(out, from, coordinates)
            return@v3Entry
        }
        if (backdropTone != null && (visible == null || tone != null)) {
            throw ProgramWireException.InvalidDeclaration("backdrop tone requires an appearance and only one tone source")
        }
        out.u8(if (backdropTone != null) 2 else if (tone != null) 1 else 0)
        if (tone != null) out.u32(tone.from)
        val curve = backdropTone ?: tone?.curve
        if (curve != null) {
            val size = count(curve.size).toInt()
            if (size < 2) throw ProgramWireException.InvalidDeclaration("tone curve requires at least two points")
            out.u32(size.toUInt())
            var previous = -1.0
            for (index in 0 until size) {
                val point = curve[index]
                val input = bounded(point.first, 1.0, "tone input")
                val output = bounded(point.second, 1.0, "tone output")
                if (input <= previous || (index == 0 && input != 0.0) || (index == size - 1 && input != 1.0)) {
                    throw ProgramWireException.InvalidDeclaration("tone curve must span 0..1 with increasing input coordinates")
                }
                out.f64(input); out.f64(output)
                previous = input
            }
        }
    }
    internal fun pointOccurrenceV3(id: UInt, paint: UInt, surface: UInt) = v3Entry(8) {
        it.u32(id); it.u32(paint); it.u32(surface); it.f64(0.0); it.f64(0.0); it.u8(0)
    }
    private fun writeSeparationBounds(out: WireBytes, minimum: Double, maximum: Double) {
        val lower = bounded(minimum, 2.0, "visible separation minimum")
        val upper = bounded(maximum, 2.0, "visible separation maximum")
        if (lower > upper) throw ProgramWireException.InvalidDeclaration("visible separation bounds must be increasing")
        out.f64(lower); out.f64(upper)
    }
    internal fun visibleSeparationV3(id: UInt, occurrence: UInt, minimum: Double, maximum: Double) = v3Entry(11) {
        it.u32(id); it.u8(11); it.u32(occurrence); writeSeparationBounds(it, minimum, maximum)
    }
    internal fun visiblePairSeparationV3(id: UInt, left: UInt, right: UInt, minimum: Double, maximum: Double) = v3Entry(11) {
        if (left == right) throw ProgramWireException.InvalidDeclaration("pair occurrences must be distinct")
        it.u32(id); it.u8(14); it.u32(left); it.u32(right); writeSeparationBounds(it, minimum, maximum)
    }
    internal fun computedTargetV3(id: UInt, domain: ProgramWireDomainV3, anchor: ByteArray, maxDelta: Double) = v3Entry(1) {
        it.u32(id); it.u8(3)
        when (domain) {
            is ProgramWireDomainV3.Box -> {
                if (domain.lower.size != 3 || domain.upper.size != 3) {
                    throw ProgramWireException.InvalidDeclaration("computed box requires three channels per bound")
                }
                for (index in 0..2) if ((domain.lower[index].toInt() and 255) > (domain.upper[index].toInt() and 255)) {
                    throw ProgramWireException.InvalidDeclaration("computed box lower bounds must not exceed upper bounds")
                }
                it.u8(1); it.rgb(domain.lower); it.rgb(domain.upper)
            }
            is ProgramWireDomainV3.Gray -> {
                if (domain.lower > domain.upper) throw ProgramWireException.InvalidDeclaration("computed gray lower bound must not exceed upper bound")
                it.u8(2); it.u8(domain.lower.toInt()); it.u8(domain.upper.toInt())
            }
        }
        if (!maxDelta.isFinite() || maxDelta < 0 || !(maxDelta * maxDelta).isFinite()) {
            throw ProgramWireException.InvalidDeclaration("computed maxDelta must be nonnegative with a finite square")
        }
        it.rgb(anchor); it.f64(if (maxDelta == 0.0) 0.0 else maxDelta)
    }
    internal fun preserveTonalHierarchyV3(id: UInt, occurrences: List<UInt>) = v3Entry(11) { out ->
        val size = count(occurrences.size).toInt()
        if (size < 2) throw ProgramWireException.InvalidDeclaration("hierarchy requires at least two occurrences")
        val seen = HashSet<UInt>(size)
        out.u32(id); out.u8(13); out.u32(size.toUInt())
        for (index in 0 until size) {
            val occurrence = occurrences[index]
            if (!seen.add(occurrence)) throw ProgramWireException.InvalidDeclaration("hierarchy occurrences must be distinct")
            out.u32(occurrence)
        }
    }
    internal fun assignmentV3(targets: List<UInt>, options: ProgramWireAssignmentOptionsV3) {
        selectionV3(targets, options.reference, 3, "assignment")
    }
    internal fun composeV3(targets: List<UInt>, options: ProgramWireComposeOptionsV3) {
        selectionV3(targets, options.reference, 5, "compose")
    }
    internal fun selectionV3(targets: List<UInt>, reference: List<ProgramWireReferenceInputV3>?, tag: Int, operation: String) {
        if (sections[3].count != 0) throw ProgramWireException.InvalidDeclaration("$operation is already declared")
        v3Entry(3) { out ->
            val size = targets.size
            if (size < 0) throw ProgramWireException.InvalidDeclaration("invalid $operation length")
            if (size > 4096) throw ProgramWireException.TooManyEntries()
            out.u8(if (reference != null) tag + 1 else tag); out.u8(0); out.u32(size.toUInt())
            for (index in 0 until size) out.u32(targets[index])
            if (reference != null) {
                val referenceSize = reference.size
                if (referenceSize < 0) throw ProgramWireException.InvalidDeclaration("invalid reference length")
                if (referenceSize > 4096) throw ProgramWireException.TooManyEntries()
                val ports = HashSet<UInt>(referenceSize)
                out.u32(referenceSize.toUInt())
                for (index in 0 until referenceSize) {
                    val input = reference[index]
                    if (!ports.add(input.port)) throw ProgramWireException.InvalidDeclaration("reference input ports must be distinct")
                    out.u32(input.port); out.rgb(input.rgb)
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

/** LCPW v3 передаёт Core характер цвета и контекст; выбор и композиция принадлежат Core. */
class ProgramWireBuilderV3 : ProgramWireBuilderV1(3) {
    fun colorTarget(id: UInt, anchor: ByteArray, options: ProgramWireColorOptionsV3 = ProgramWireColorOptionsV3()): ProgramWireBuilderV3 {
        colorTargetV3(id, anchor, options, null)
        return this
    }
    /** Кривая от видимой подложки occurrence в options.appearance или options.referenceAppearance. */
    fun colorTarget(id: UInt, anchor: ByteArray, options: ProgramWireColorOptionsV3,
                    backdropTone: List<Pair<Double, Double>>): ProgramWireBuilderV3 {
        colorTargetV3(id, anchor, options, backdropTone)
        return this
    }
    /** Координата кривой задаётся Oklab L или образцом sRGB8, который проектирует Core. */
    fun colorTarget(id: UInt, anchor: ByteArray, options: ProgramWireColorOptionsV3,
                    tone: List<Pair<ProgramWireToneCoordinateV3, ProgramWireToneCoordinateV3>>,
                    from: UInt? = null): ProgramWireBuilderV3 {
        colorTargetV3(id, anchor, options, null, tone, from)
        return this
    }
    fun pointOccurrence(id: UInt, paint: UInt, surface: UInt): ProgramWireBuilderV3 {
        pointOccurrenceV3(id, paint, surface)
        return this
    }
    fun visibleSeparationHard(id: UInt, occurrence: UInt, minimum: Double, maximum: Double): ProgramWireBuilderV3 {
        visibleSeparationV3(id, occurrence, minimum, maximum)
        return this
    }
    fun visiblePairSeparationHard(id: UInt, left: UInt, right: UInt, minimum: Double, maximum: Double): ProgramWireBuilderV3 {
        visiblePairSeparationV3(id, left, right, minimum, maximum)
        return this
    }
    fun computedTarget(id: UInt, domain: ProgramWireDomainV3, anchor: ByteArray, maxDelta: Double): ProgramWireBuilderV3 {
        computedTargetV3(id, domain, anchor, maxDelta)
        return this
    }
    fun preserveTonalHierarchy(id: UInt, occurrences: List<UInt>): ProgramWireBuilderV3 {
        preserveTonalHierarchyV3(id, occurrences)
        return this
    }
    fun assignment(targetsInPreferenceOrder: List<UInt>): ProgramWireBuilderV3 =
        assignment(targetsInPreferenceOrder, ProgramWireAssignmentOptionsV3())

    fun assignment(targetsInPreferenceOrder: List<UInt>, options: ProgramWireAssignmentOptionsV3): ProgramWireBuilderV3 {
        assignmentV3(targetsInPreferenceOrder, options)
        return this
    }

    /** Every computed target appears once. Core composes whole physical components;
     * declaration order only preserves compatibility for implicit-reference components. */
    fun compose(targets: List<UInt>, options: ProgramWireComposeOptionsV3 = ProgramWireComposeOptionsV3()): ProgramWireBuilderV3 {
        composeV3(targets, options)
        return this
    }

    /** Balances reference uses on physically independent backdrops. */
    fun composeReference(targets: List<UInt>, reference: List<ProgramWireReferenceInputV3>): ProgramWireBuilderV3 {
        selectionV3(targets, reference, 6, "composeReference")
        return this
    }
}
