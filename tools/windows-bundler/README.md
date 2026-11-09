# Local Windows packaging

This tool packages an already compiled Windows binary with the official Tauri
NSIS template. It reads product metadata and icons from tauri.conf.json, keeps
CurrentUser installation, and includes the WebView2 loader alongside the program.
It does not upload source code, read application data, or create updater signatures.

Build the frontend and Windows executable first. A GNU cross build needs the Rust
x86_64-pc-windows-gnu target, a matching C/C++/resource toolchain, and Windows
OpenSSL libraries for SQLCipher. Use the production custom-protocol feature:

```
npm run build
cargo build --manifest-path src-tauri/Cargo.toml --release --target x86_64-pc-windows-gnu --features tauri/custom-protocol
cargo run --manifest-path tools/windows-bundler/Cargo.toml -- /absolute/repository /absolute/windows/release
```

The Windows release directory must contain kouros.exe and WebView2Loader.dll.
NSIS must be available on PATH with its resource directory configured. The helper
places downloaded bundler tools under the workspace's toolchains directory.

Tauri patches its bundle-kind marker from UNK to NSS in the packaged executable.
That expected change must be accounted for when verifying extracted installer
payloads. Check architecture, imported DLLs, product identifier, installation mode,
and payload hashes before publishing. Building on Linux does not replace a
physical Windows install/upgrade test.
