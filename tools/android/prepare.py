"""Prepare a generated Tauri Android project without committing generated files.

Disable OS backup of Keystore-bound data; apply release signing only from a local
keystore.properties file. Production signing material never belongs in Git.
"""
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET

root = Path(__file__).resolve().parents[2]
project = root / "src-tauri/gen/android"
manifest = project / "app/src/main/AndroidManifest.xml"
namespace = "http://schemas.android.com/apk/res/android"
ET.register_namespace("android", namespace)
tree = ET.parse(manifest)
app = tree.getroot().find("application")
assert app is not None
app.set(f"{{{namespace}}}allowBackup", "false")
app.set(f"{{{namespace}}}fullBackupContent", "false")
app.set(f"{{{namespace}}}dataExtractionRules", "@xml/kouros_data_extraction_rules")
tree.write(manifest, encoding="utf-8", xml_declaration=True)
if (project / "keystore.properties").exists():
    subprocess.run([sys.executable, str(root / "tools/android/patch_gradle.py")], cwd=root, check=True)
print("Android manifest prepared (backup disabled)")

# Pinned CLI 2.9.6 generates Gradle 8.14.3. Verify the distribution on clean machines.
wrapper = project / "gradle/wrapper/gradle-wrapper.properties"
text = wrapper.read_text()
if "gradle-8.14.3-bin.zip" not in text:
    raise SystemExit("Unexpected Gradle distribution: review its official checksum before building")
checksum = "bd71102213493060956ec229d946beee57158dbd89d0e62b91bca0fa2c5f3531"
lines = [line for line in text.splitlines() if not line.startswith("distributionSha256Sum=")]
wrapper.write_text("\n".join(lines) + "\ndistributionSha256Sum=" + checksum + "\n")

# Do not serialize signing passwords into Gradle's configuration cache.
properties = project / "gradle.properties"
text = properties.read_text()
lines = [line for line in text.splitlines() if not line.startswith("org.gradle.configuration-cache=")]
properties.write_text("\n".join(lines) + "\norg.gradle.configuration-cache=false\n")
