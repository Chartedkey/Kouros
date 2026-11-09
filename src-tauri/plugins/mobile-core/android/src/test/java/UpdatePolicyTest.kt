package app.kouros.mobile

import org.junit.Assert.assertThrows
import org.junit.Test
import java.io.File

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
    @Test fun acceptsOnlyImmutableKourosRawArtifacts() {
        val commit = "a".repeat(40)
        UpdatePolicy.validateUrl("https://raw.githubusercontent.com/Chartedkey/Kouros/$commit/Kouros_2.0.0-alpha.2_android.apk", true)
        for (url in listOf(
            "https://raw.githubusercontent.com/Chartedkey/Kouros/main/Kouros_2_android.apk",
            "https://raw.githubusercontent.com/other/repo/$commit/Kouros_2_android.apk",
            "https://raw.githubusercontent.com/Chartedkey/Kouros/$commit/Kouros_2_android.apk?download=1",
            "https://raw.githubusercontent.com/Chartedkey/Kouros/$commit/secret.txt")) {
            assertThrows(IllegalArgumentException::class.java) { UpdatePolicy.validateUrl(url, true) }
        }
    }
    @Test fun verifiesActualFileBytesAndRejectsCorruption() {
        val file = File.createTempFile("kouros-checksum", ".apk")
        try {
            file.writeText("abc")
            val checksum = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            UpdatePolicy.verifyChecksum(file, checksum)
            UpdatePolicy.verifyChecksum(file, checksum.uppercase())
            file.appendText("tampered")
            assertThrows(IllegalArgumentException::class.java) { UpdatePolicy.verifyChecksum(file, checksum) }
            assertThrows(IllegalArgumentException::class.java) { UpdatePolicy.verifyChecksum(file, "invalid") }
        } finally { file.delete() }
    }

}
