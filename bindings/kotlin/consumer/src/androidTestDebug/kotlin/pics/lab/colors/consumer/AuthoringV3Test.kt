package pics.lab.colors.consumer

import org.json.JSONObject
import org.junit.Test
import pics.lab.colors.conformance.verifyProgramAuthoringV3

class AuthoringV3Test {
    @Test fun installedPackageAuthorsAndExecutesComputedColors() {
        val fixture = JSONObject(asset("program-authoring-v3.json"))
        check(fixture.getInt("schemaVersion") == 1)
        verifyProgramAuthoringV3(listOf("computedBox", "computedGray", "contextual", "encodingOnly", "fixedOnly",
            "referenceSingleton", "referenceHierarchy", "referenceEmpty")
            .associateWith { fixture.getString(it) })
    }
}
