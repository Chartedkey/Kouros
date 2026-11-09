"""Create the Android update-channel manifest alongside an already signed APK.

Usage: python tools/android/update_manifest.py <signed-apk> <output-json>
Publish APK + manifest together to the approved GitHub release tagged `android`.
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
out = {
    "version": config["version"],
    "versionCode": config["bundle"]["android"]["versionCode"],
    "url": "https://github.com/Chartedkey/Kouros/releases/download/android/" + apk.name,
    "sha256": hashlib.sha256(apk.read_bytes()).hexdigest(),
    "notes": "Android: защищённое хранилище, системный вход и обновление внутри приложения. Health Connect появится следующим этапом.",
}
Path(sys.argv[2]).write_text(json.dumps(out, ensure_ascii=False, indent=2) + "\n")
