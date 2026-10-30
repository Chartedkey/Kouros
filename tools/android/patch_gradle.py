"""Adds release signing to the Android project `tauri android init` generates (2.0).

Reads src-tauri/gen/android/app/build.gradle.kts and, if src-tauri/gen/android/keystore.properties
exists, signs the release APK with that key (the key itself comes from the CI secret, never the repo).
"""
import pathlib
import sys

p = pathlib.Path("src-tauri/gen/android/app/build.gradle.kts")
s = p.read_text()
if "signingConfigs" in s:
    sys.exit(0)
if "import java.util.Properties" not in s:
    s = "import java.util.Properties\nimport java.io.FileInputStream\n" + s
block = '''
    signingConfigs {
        create("release") {
            val f = rootProject.file("keystore.properties")
            if (f.exists()) {
                val k = Properties()
                k.load(FileInputStream(f))
                keyAlias = k["keyAlias"] as String
                keyPassword = k["password"] as String
                storeFile = file(k["storeFile"] as String)
                storePassword = k["password"] as String
            }
        }
    }
'''
i = s.index("buildTypes {")
s = s[:i] + block.lstrip("\n") + "    " + s[i:]
j = s.index('getByName("release") {')
j = s.index("{", j) + 1
s = s[:j] + '\n            signingConfig = signingConfigs.getByName("release")' + s[j:]
p.write_text(s)
print("signing added")
