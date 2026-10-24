package pics.lab.colors.consumer

import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Test

class ReleaseTest {
    @Test fun optimizedApplicationExecutesTheInstalledNativePackage() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertEquals("ok", context.getSharedPreferences("labcolors", Context.MODE_PRIVATE).getString("result", "not executed"))
    }
}
