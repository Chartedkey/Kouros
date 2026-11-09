"""Create the Android update-channel manifest alongside an already signed APK.

Usage: python tools/android/update_manifest.py <signed-apk> <output-json> <immutable-apk-url>
Publish the APK first, then the manifest on the android-updates branch.
This helper does not publish or read any signing credentials.
"""
import hashlib
import json
from pathlib import Path
import sys

config = json.loads(Path("src-tauri/tauri.android.conf.json").read_text())
apk = Path(sys.argv[1])
if not apk.is_file() or not apk.name.endswith("_android.apk"):
    raise SystemExit("Expected an existing *_android.apk")
import re
if not re.fullmatch(r"https://raw\.githubusercontent\.com/Chartedkey/Kouros/[a-f0-9]{40}/" + re.escape(apk.name), sys.argv[3]):
    raise SystemExit("Expected an immutable APK URL in Chartedkey/Kouros")
out = {
    "version": config["version"],
    "versionCode": config["bundle"]["android"]["versionCode"],
    "url": sys.argv[3],
    "sha256": hashlib.sha256(apk.read_bytes()).hexdigest(),
    "notes": "Синхронизация Windows и Android по домашнему Wi-Fi. Android открывается без пароля и отпечатка. Фирменная иконка K. Для обмена требуется Kouros 1.6.0 на Windows.",
}
Path(sys.argv[2]).write_text(json.dumps(out, ensure_ascii=False, indent=2) + "\n")
