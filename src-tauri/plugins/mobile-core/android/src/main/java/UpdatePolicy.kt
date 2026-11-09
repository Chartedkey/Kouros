package app.kouros.mobile

import java.net.URI
import java.io.File
import java.security.MessageDigest

/** Shared by the downloader and its JVM tests; no Android dependency. */
internal object UpdatePolicy {
    private val downloadHosts = setOf("github.com", "release-assets.githubusercontent.com", "objects.githubusercontent.com", "raw.githubusercontent.com")
    fun validateUrl(url: String, initial: Boolean = false) {
        val u = URI(url)
        require(u.scheme == "https" && u.userInfo == null && (u.port == -1 || u.port == 443)) { "Недопустимый адрес обновления" }
        require(u.host in downloadHosts) { "Недопустимый сервер обновления" }
        val immutableRaw = u.host == "raw.githubusercontent.com" && u.query == null && u.fragment == null &&
            Regex("/Chartedkey/Kouros/[a-f0-9]{40}/Kouros_[A-Za-z0-9._-]+_android\\.apk").matches(u.path)
        if (u.host == "raw.githubusercontent.com") require(immutableRaw) { "Нужен неизменяемый APK из репозитория Kouros" }
        if (initial) require(immutableRaw || (u.host == "github.com" && u.path.startsWith("/Chartedkey/Kouros/releases/download/"))) { "Обновление не из репозитория Kouros" }
    }
    fun verifyChecksum(file: File, expected: String) {
        require(Regex("[a-fA-F0-9]{64}").matches(expected)) { "Неверная контрольная сумма обновления" }
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input ->
            val buffer = ByteArray(64 * 1024)
            while (true) { val count = input.read(buffer); if (count < 0) break; digest.update(buffer, 0, count) }
        }
        val actual = digest.digest().joinToString("") { "%02x".format(it) }
        require(actual.equals(expected, ignoreCase = true)) { "APK загружен с ошибкой: контрольная сумма не совпадает" }
    }
    fun validatePackage(installedPackage: String, newPackage: String, installedVersion: Long,
                        newVersion: Long, installedSigners: Set<String>, newSigners: Set<String>) {
        require(newPackage == installedPackage) { "APK относится к другому приложению" }
        require(newVersion > installedVersion) { "APK не новее установленной версии" }
        require(installedSigners.isNotEmpty() && newSigners == installedSigners) { "Подпись APK не совпадает с установленным Kouros" }
    }
}
