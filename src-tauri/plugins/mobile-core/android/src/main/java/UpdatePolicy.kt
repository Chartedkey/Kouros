package app.kouros.mobile

import java.net.URI

/** Shared by the downloader and its JVM tests; no Android dependency. */
internal object UpdatePolicy {
    private val downloadHosts = setOf("github.com", "release-assets.githubusercontent.com", "objects.githubusercontent.com")
    fun validateUrl(url: String, initial: Boolean = false) {
        val u = URI(url)
        require(u.scheme == "https" && u.userInfo == null && (u.port == -1 || u.port == 443)) { "Недопустимый адрес обновления" }
        require(u.host in downloadHosts) { "Недопустимый сервер обновления" }
        if (initial) require(u.host == "github.com" && u.path.startsWith("/Chartedkey/Kouros/releases/download/")) { "Обновление не из репозитория Kouros" }
    }
    fun validatePackage(installedPackage: String, newPackage: String, installedVersion: Long,
                        newVersion: Long, installedSigners: Set<String>, newSigners: Set<String>) {
        require(newPackage == installedPackage) { "APK относится к другому приложению" }
        require(newVersion > installedVersion) { "APK не новее установленной версии" }
        require(installedSigners.isNotEmpty() && newSigners == installedSigners) { "Подпись APK не совпадает с установленным Kouros" }
    }
}
