"""Materialize CI signing secrets without exposing them in arguments or logs.

Expected environment: KS (base64 JKS), KP (password). Both must already exist.
Never creates a new production signing identity. Generated paths are ignored.
"""
import base64
import os
from pathlib import Path


def escape_property(value: str) -> str:
    return (value.replace('\\', '\\\\').replace('\n', '\\n').replace('\r', '\\r')
            .replace('\t', '\\t').replace('\f', '\\f').replace(' ', '\\ '))


def configure(project: Path, encoded: str, password: str) -> None:
    if not encoded or not password:
        raise ValueError('Both Android signing secrets are required')
    # Validate before creating any files; accept line-wrapped base64 exports.
    raw = base64.b64decode(''.join(encoded.split()), validate=True)
    if not raw:
        raise ValueError('Empty Android keystore')
    project.mkdir(parents=True, exist_ok=True)
    key = project / 'kouros-android.jks'
    properties = project / 'keystore.properties'
    if key.exists() or properties.exists():
        raise FileExistsError('Signing files already exist; refusing to overwrite them')
    for path, content in ((key, raw), (properties, (
        f'keyAlias=kouros\npassword={escape_property(password)}\nstoreFile={escape_property(str(key.resolve()))}\n'
    ).encode('utf-8'))):
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, 'wb') as f:
            f.write(content)


if __name__ == '__main__':
    configure(Path('src-tauri/gen/android'), os.environ.get('KS', ''), os.environ.get('KP', ''))
