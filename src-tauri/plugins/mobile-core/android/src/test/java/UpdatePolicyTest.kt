package app.kouros.mobile

import org.junit.Assert.assertThrows
import org.junit.Test

class UpdatePolicyTest {
    @Test fun acceptsOfficialDownloadAndHttpsRedirect() {
        UpdatePolicy.validateUrl("https://github.com/Chartedkey/Kouros/releases/download/android/Kouros_android.apk", true)
        UpdatePolicy.validateUrl("https://release-assets.githubusercontent.com/file?token=example")
    }
    @Test fun rejectsDowngradeCredentialsOtherRepoAndHost() {
        for (url in listOf("http://github.com/Chartedkey/Kouros/releases/download/x/a.apk",
            "https://github.com@evil.example/a.apk", "https://github.com/other/repo/releases/download/x/a.apk",
            "https://github.com:8443/Chartedkey/Kouros/releases/download/x/a.apk")) {
            assertThrows(IllegalArgumentException::class.java) { UpdatePolicy.validateUrl(url, true) }
        }
        assertThrows(IllegalArgumentException::class.java) { UpdatePolicy.validateUrl("http://release-assets.githubusercontent.com/file") }
    }
    @Test fun requiresSamePackageSameSignerAndHigherVersion() {
        UpdatePolicy.validatePackage("app.kouros", "app.kouros", 1, 2, setOf("a"), setOf("a"))
        for ((pkg, version, signers) in listOf(Triple("other", 2L, setOf("a")), Triple("app.kouros", 1L, setOf("a")),
            Triple("app.kouros", 2L, setOf("b")), Triple("app.kouros", 2L, emptySet<String>()))) {
            assertThrows(IllegalArgumentException::class.java) { UpdatePolicy.validatePackage("app.kouros", pkg, 1, version, setOf("a"), signers) }
        }
    }
}
