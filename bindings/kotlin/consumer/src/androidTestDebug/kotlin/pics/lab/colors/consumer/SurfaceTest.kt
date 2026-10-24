package pics.lab.colors.consumer

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import pics.lab.colors.numericalCapabilityManifest
import java.lang.reflect.Modifier

class SurfaceTest {
    private fun camel(name: String) = Regex("_([a-z0-9])").replace(name) { it.groupValues[1].uppercase() }
    private fun required(): Map<String, Int> {
        val native = JSONObject(asset("native-surface.json"))
        val authoring = JSONObject(asset("authoring-surface.json"))
        assertEquals(1, native.getInt("schemaVersion")); assertEquals(1, authoring.getInt("schemaVersion"))
        val result = mutableMapOf<String, Int>()
        for (inventory in listOf(native.getJSONObject("operations"), authoring.getJSONObject("operations"))) {
            for (key in inventory.keys()) {
                val (owner, method) = key.split("::")
                val normalized = owner + "::" + camel(method)
                assertNull(result.put(normalized, inventory.getInt(key)))
            }
        }
        assertEquals(2, result["fn::compileProgramWire"])
        assertEquals(4, result["ProgramRuntime::updateObserved"])
        assertEquals(2, result["ProgramWireBuilderV2::selectionRelease"])
        return result
    }
    private fun discover(expected: Map<String, Int>): Map<String, Set<Int>> {
        val result = mutableMapOf<String, MutableSet<Int>>()
        for (owner in expected.keys.map { it.substringBefore("::") }.toSet()) {
            val type = Class.forName("pics.lab.colors." + if (owner == "fn") "LabcolorsKt" else owner)
            for (method in type.methods) {
                if (!Modifier.isPublic(method.modifiers) || Modifier.isStatic(method.modifiers) != (owner == "fn")) continue
                val name = method.name.substringBefore('-')
                result.getOrPut(owner + "::" + name) { mutableSetOf() }.add(method.parameterTypes.size)
            }
        }
        return result
    }
    private fun verify(expected: Map<String, Int>, actual: Map<String, Set<Int>>) {
        val missing = expected.filter { (key, arity) -> actual[key]?.contains(arity) != true }
        check(missing.isEmpty()) { "compiled Kotlin API is incomplete: $missing" }
    }
    @Test fun compiledApiIncludesEveryNativeAndAuthoringOperation() {
        val expected = required()
        val actual = discover(expected)
        assertTrue(expected.size > 70)
        verify(expected, actual)
        try { verify(expected, actual - "ProgramRuntime::updateObserved"); fail("missing method accepted") }
        catch (_: IllegalStateException) { }
        try { verify(expected + ("fn::futureOperation" to 1), actual); fail("new missing operation accepted") }
        catch (_: IllegalStateException) { }
        try { verify(expected, actual + ("ProgramRuntime::updateObserved" to setOf(3))); fail("wrong arity accepted") }
        catch (_: IllegalStateException) { }
    }
    private fun value(input: Any?): Any = when (input) {
        null -> JSONObject.NULL
        is String, is Number, is Boolean -> input
        is UInt -> input.toLong()
        is ULong -> input.toString()
        is Collection<*> -> JSONArray(input.map { value(it) })
        else -> JSONObject().apply {
            for (field in input.javaClass.declaredFields) {
                if (Modifier.isStatic(field.modifiers) || field.isSynthetic) continue
                field.isAccessible = true
                put(field.name, value(field.get(input)))
            }
        }
    }
    private fun compare(expected: Any, actual: Any) {
        when (expected) {
            is JSONObject -> {
                require(actual is JSONObject)
                assertEquals(expected.keys().asSequence().toSet(), actual.keys().asSequence().toSet())
                for (key in expected.keys()) compare(expected.get(key), actual.get(key))
            }
            is JSONArray -> {
                require(actual is JSONArray); assertEquals(expected.length(), actual.length())
                repeat(expected.length()) { compare(expected.get(it), actual.get(it)) }
            }
            is Number -> { require(actual is Number); assertEquals(expected.toLong(), actual.toLong()) }
            else -> assertEquals(expected, actual)
        }
    }
    @Test fun allCapabilityFieldsMatchTheCanonicalManifest() {
        compare(JSONObject(asset("manifest.json")).getJSONObject("numericalCapabilities"), value(numericalCapabilityManifest()))
    }
}
