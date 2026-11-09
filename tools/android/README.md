# Android stage 1

The base is the existing `android` branch (Windows 1.5.0 engine). Health Connect is
**not implemented in this stage**. The Android configuration has its own version
and explicit `bundle.android.versionCode`; increment both for the next Android
build, without changing the Windows version as a side effect.

## Reproducible build

Requirements: Node 22+, JDK 17+, Rust with `aarch64-linux-android`, Android SDK 36,
build-tools 35.0.0 and NDK 27.2.12479018. Keep TLS/checksum verification enabled.
Set ANDROID_HOME, NDK_HOME and the normal Rust environment. Set AR and RANLIB for
OpenSSL to the NDK's llvm tools as shown in `.github/workflows/android.yml`.

```
npm ci
npm run build
npx tauri android init --ci
npx tauri icon src-tauri/icons/icon.png
python3 tools/android/prepare.py
npx tauri android build --apk true --aab false --target aarch64 --ci
```

For a release, provide `src-tauri/gen/android/keystore.properties` **before**
prepare.py. This ignored file contains `keyAlias=kouros`, `password`, and the
absolute `storeFile` path. Keep it and the keystore outside Git, never in logs.
CI reads the existing ANDROID_KEYSTORE (base64 JKS) and ANDROID_KEY_PASSWORD
secrets; it fails if missing, rather than producing an unusable unsigned release.
Do not generate a replacement production key if one already exists.

The pipeline uploads artifacts only: it does not push main, create releases or
publish automatically. Verify the APK with apksigner before distribution.

## Native implementation

`src-tauri/plugins/mobile-core` is a Rust-only Tauri plugin bridge; it explicitly rejects every
WebView invocation (including Tauri's mobile-command fallback). Android Keystore owns a non-exportable AES key, used to
wrap the credential JSON using AES-GCM with package-name AAD and a random IV.
The ciphertext is atomically saved in noBackupFilesDir. OS backup/transfer of app
data is disabled because Keystore keys cannot be restored on another device.
Existing prototype secrets.json is imported with conflict and read-back checks,
then removed. Corrupt data/missing keys fail without replacing the database key.

The Rust unlock command performs system biometric/device credential verification;
JS cannot report success to it. Native activity lifecycle invalidates the session
when the main activity stops; guarded Rust commands check the native session.
The UI clears data on document hiding and refreshes status on resume. FLAG_SECURE
prevents health data appearing in screenshots/app-switcher previews. Android entry
protection is mandatory; Windows keeps its current lock setting.

The updater fetches the independent manifest from:
`https://raw.githubusercontent.com/Chartedkey/Kouros/refs/heads/android-updates/android-update.json`.
A missing channel (404) means no published Android update. Errors are not treated
as a successful check. A newer numeric versionCode is required. Downloads allow
only HTTPS on immutable commit URLs in Chartedkey/Kouros, the repository's release path and known GitHub asset redirect hosts.
Before opening the system installer, Android parses the APK and checks matching
package, matching current signing certificates, SHA-256, and a strictly higher versionCode matching the manifest.
The system installer performs final signature verification and asks for consent.
No silent installation is promised. If installation permission is missing,
settings open and the user returns, authenticates and retries the cached APK.

## Update-channel publication

The user approved alpha.2 to enable in-app updates and include the layout fixes.
Publication of application sources is not authorized. Publish artifacts only.

Rename the verified signed artifact to `Kouros_<android-version>_android.apk`.
Publish it with SHA256SUMS.txt in an isolated APK-only orphan branch, then verify
its public download. Use the immutable commit URL, never a mutable branch URL:

```
python tools/android/update_manifest.py Kouros_2.0.0-alpha.2_android.apk android-update.json https://raw.githubusercontent.com/Chartedkey/Kouros/<40-character-commit>/Kouros_2.0.0-alpha.2_android.apk
```

Publish the manifest last as `android-update.json` on the independent
`android-updates` branch. Preserve any existing channel branch history. This branch
contains update metadata only. The Android prerelease links directly to the APK.
Windows latest remains independent. APK signature comparison establishes signer
continuity; SHA-256 detects incomplete/corrupt downloads. Keep the same signing key
and app identifier. Never upload keys, passwords, source trees or personal data.

Alpha.1 used an unavailable release-asset manifest path, so alpha.2 needs one last
manual installation over alpha.1. Subsequent approved builds use this channel.
Automatic checks run at startup when due and every 30 minutes while open. Users
can disable checks or check manually under Settings / About. Installation always
requires Android's confirmation; this is not silent/background installation.

## Validation

```
cd src-tauri/gen/android
./gradlew :tauri-plugin-kouros-mobile:testDebugUnitTest
```

JVM tests cover URL policy, HTTPS downgrade, foreign repositories/packages,
signer mismatch and downgrade rejection. Device checks remain necessary:

- Fresh install, system fingerprint/PIN success, cancellation and no enrollment.
- Background/resume, process death, no bypass on direct unlock invocation.
- Encrypted vault survives restart; no plaintext secret file; failed migration
  retains the original file. Backup/transfer does not copy an unusable vault.
- Update A to B with the same key preserves profile, history, notes and settings.
- Different signing key, same/older version, interrupted/invalid download,
  insufficient storage, denied install permission and cancellation are handled.
- Validate a Windows build independently; Android compilation is not Windows CI.

Health Connect data, background ingestion, complete file import/export and Windows
exchange belong to the later agreed stages. Existing screens are retained but
must not be described as device-validated until their checks actually run.
