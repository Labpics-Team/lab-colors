package pics.lab.colors.consumer

import android.system.Os
import android.system.OsConstants
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Test

class PageSizeTest {
    @Test fun applicationUsesTheRequestedMemoryPageSize() {
        val expected = requireNotNull(InstrumentationRegistry.getArguments().getString("expectedPageSize")).toLong()
        assertEquals(expected, Os.sysconf(OsConstants._SC_PAGESIZE))
    }
}
