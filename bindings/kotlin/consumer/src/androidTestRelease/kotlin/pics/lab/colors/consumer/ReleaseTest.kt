package pics.lab.colors.consumer

import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Test

class ReleaseTest {
    @Test fun optimizedApplicationExecutesTheInstalledNativePackage() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertEquals("ok", context.getSharedPreferences("labcolors", Context.MODE_PRIVATE).getString("result", "not executed"))
        // AndroidX instrumentation accesses this app-owned Kotlin ABI after R8.
        val reference = Class.forName("kotlin.jvm.internal.Ref\$ObjectRef")
        val instance = reference.getConstructor().newInstance()
        val element = reference.getField("element")
        element.set(instance, "shared runtime")
        assertEquals("shared runtime", element.get(instance))
    }
}
