package app.kouros.mobile

import android.app.Activity
import android.app.Application
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.webkit.WebView
import android.view.WindowManager
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.KeyStore
import java.security.MessageDigest
import java.util.concurrent.Executors
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

@InvokeArg
class SecretArgs { lateinit var action: String; lateinit var name: String; var value: String? = null }
@InvokeArg
class InstallArgs { lateinit var url: String; lateinit var sha256: String; var versionCode: Long = 0 }

@TauriPlugin
class MobileCorePlugin(private val activity: Activity) : Plugin(activity) {
    private val executor = Executors.newSingleThreadExecutor()
    @Volatile private var session = false
    @Volatile private var stopped = false
    private val alias = "kouros.vault.v1"
    private val vault get() = AtomicFile(File(activity.noBackupFilesDir, "kouros-vault.v1"))

    init {
        activity.application.registerActivityLifecycleCallbacks(object : Application.ActivityLifecycleCallbacks {
            override fun onActivityStopped(a: Activity) { if (a === activity) { stopped = true; session = false } }
            override fun onActivityCreated(a: Activity, b: Bundle?) {}
            override fun onActivityStarted(a: Activity) {}
            override fun onActivityResumed(a: Activity) { if (a === activity) stopped = false }
            override fun onActivityPaused(a: Activity) {}
            override fun onActivitySaveInstanceState(a: Activity, b: Bundle) {}
            override fun onActivityDestroyed(a: Activity) { if (a === activity) { stopped = true; session = false } }
        })
    }

    override fun load(webView: WebView) {
        activity.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
    }
    @Suppress("DEPRECATION")
    @Command fun installedInfo(invoke: Invoke) {
        val info = activity.packageManager.getPackageInfo(activity.packageName, 0)
        invoke.resolve(JSObject().apply { put("versionCode", info.longVersionCode) })
    }
    @Command fun sessionStatus(invoke: Invoke) { invoke.resolve(JSObject().apply { put("open", session) }) }
    // Invoked only from Rust after the system authentication succeeds. No JS command is exported.
    @Command fun startSession(invoke: Invoke) {
        if (stopped || activity.isFinishing || activity.isDestroyed) { invoke.reject("Вернитесь в Kouros и подтвердите вход"); return }
        session = true; invoke.resolve()
    }
    @Command fun endSession(invoke: Invoke) { session = false; invoke.resolve() }

    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(alias, null) as? SecretKey)?.let { return it }
        check(!vault.baseFile.exists() && !File(vault.baseFile.path + ".bak").exists()) {
            "Ключ хранилища утрачен. Восстановите данные из резервной копии."
        }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setRandomizedEncryptionRequired(true).build())
        return generator.generateKey()
    }
    private fun load(): JSONObject {
        val file = vault
        val bytes = try { file.openRead().use { it.readBytes() } } catch (e: java.io.FileNotFoundException) { return JSONObject() }
        check(bytes.size > 12) { "Повреждено хранилище ключей" }
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes.copyOfRange(0, 12)))
        cipher.updateAAD(activity.packageName.toByteArray(Charsets.UTF_8))
        return JSONObject(String(cipher.doFinal(bytes.copyOfRange(12, bytes.size)), Charsets.UTF_8))
    }
    private fun save(data: JSONObject) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key())
        cipher.updateAAD(activity.packageName.toByteArray(Charsets.UTF_8))
        val bytes = cipher.iv + cipher.doFinal(data.toString().toByteArray(Charsets.UTF_8))
        val file = vault
        val out = file.startWrite()
        try { out.write(bytes); file.finishWrite(out) } catch (e: Exception) { file.failWrite(out); throw e }
    }
    @Command fun secret(invoke: Invoke) {
        val args = invoke.parseArgs(SecretArgs::class.java)
        executor.execute {
            try {
                val data = load()
                when (args.action) {
                    "get" -> invoke.resolve(JSObject().apply { put("value", if (data.has(args.name)) data.getString(args.name) else JSONObject.NULL) })
                    "set" -> { data.put(args.name, requireNotNull(args.value)); save(data); invoke.resolve() }
                    "delete" -> { data.remove(args.name); save(data); invoke.resolve() }
                    else -> invoke.reject("Неизвестная операция хранилища")
                }
            } catch (_: Exception) { invoke.reject("Защищённое хранилище недоступно или повреждено. Данные не сброшены.") }
        }
    }

    private fun sha256(bytes: ByteArray) = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
    private fun signers(p: PackageInfo): Set<String> = p.signingInfo?.apkContentsSigners?.map { sha256(it.toByteArray()) }?.toSet() ?: emptySet()
    @Suppress("DEPRECATION")
    private fun verifyApk(file: File, expectedVersion: Long) {
        val pm = activity.packageManager
        val installed = pm.getPackageInfo(activity.packageName, PackageManager.GET_SIGNING_CERTIFICATES)
        val update = pm.getPackageArchiveInfo(file.path, PackageManager.GET_SIGNING_CERTIFICATES) ?: error("Не удалось прочитать APK")
        require(update.longVersionCode == expectedVersion) { "Номер сборки APK не совпадает с объявленным обновлением" }
        UpdatePolicy.validatePackage(installed.packageName, update.packageName, installed.longVersionCode,
            update.longVersionCode, signers(installed), signers(update))
    }
    private fun download(url: String, destination: File, expectedVersion: Long, checksum: String) {
        UpdatePolicy.validateUrl(url, initial = true)
        var next = url
        repeat(6) {
            UpdatePolicy.validateUrl(next)
            val connection = URL(next).openConnection() as HttpURLConnection
            connection.instanceFollowRedirects = false
            connection.connectTimeout = 30_000; connection.readTimeout = 30_000
            try {
                val code = connection.responseCode
                if (code in setOf(301, 302, 303, 307, 308)) {
                    val location = connection.getHeaderField("Location") ?: error("Нет адреса скачивания")
                    next = URL(URL(next), location).toString()
                } else {
                    check(code == 200) { "Сервер обновлений вернул HTTP $code" }
                    val expected = connection.contentLengthLong
                    check(expected <= 256L * 1024 * 1024) { "APK слишком большой" }
                    val temporary = File(destination.parentFile, destination.nameWithoutExtension + ".part.apk")
                    try {
                        var total = 0L
                        connection.inputStream.use { input -> temporary.outputStream().use { output ->
                            val buffer = ByteArray(64 * 1024)
                            while (true) {
                                val count = input.read(buffer); if (count == -1) break
                                total += count
                                check(total <= 256L * 1024 * 1024) { "APK слишком большой" }
                                output.write(buffer, 0, count)
                            }
                        } }
                        check(total > 0 && (expected < 0 || total == expected)) { "APK загружен не полностью" }
                        UpdatePolicy.verifyChecksum(temporary, checksum)
                        verifyApk(temporary, expectedVersion)
                        check(temporary.renameTo(destination)) { "Не удалось сохранить обновление" }
                    } finally { temporary.delete() }
                    return
                }
            } finally { connection.disconnect() }
        }
        error("Слишком много перенаправлений")
    }
    @Command fun installUpdate(invoke: Invoke) {
        val args = invoke.parseArgs(InstallArgs::class.java)
        if (!session) { invoke.reject("Сначала войдите в Kouros"); return }
        executor.execute {
            try {
                UpdatePolicy.validateUrl(args.url, initial = true)
                val dir = File(activity.cacheDir, "updates").apply { mkdirs() }
                val apk = File(dir, sha256(args.url.toByteArray()) + ".apk")
                if (!apk.exists()) download(args.url, apk, args.versionCode, args.sha256)
                try {
                    UpdatePolicy.verifyChecksum(apk, args.sha256)
                    verifyApk(apk, args.versionCode)
                } catch (e: Exception) { apk.delete(); throw e }
                activity.runOnUiThread {
                    try {
                        if (!session) { invoke.reject("Сначала войдите в Kouros и повторите обновление"); return@runOnUiThread }
                        if (!activity.packageManager.canRequestPackageInstalls()) {
                            activity.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:" + activity.packageName)))
                            invoke.reject("Разрешите установку обновлений для Kouros, вернитесь, войдите и нажмите «Обновить» ещё раз. APK уже скачан.")
                        } else {
                            val uri = FileProvider.getUriForFile(activity, activity.packageName + ".kouros.updates", apk)
                            activity.startActivity(Intent(Intent.ACTION_VIEW).setDataAndType(uri, "application/vnd.android.package-archive")
                                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION))
                            invoke.resolve()
                        }
                    } catch (_: Exception) { invoke.reject("Не удалось открыть системный установщик") }
                }
            } catch (e: Exception) { invoke.reject(if (e is IllegalArgumentException || e is IllegalStateException) e.message ?: "Обновление не прошло проверку" else "Не удалось скачать обновление. Проверьте сеть и свободное место.") }
        }
    }
}
