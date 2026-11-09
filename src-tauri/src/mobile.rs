//! Android bridge, callable from Rust only; credentials never enter the WebView.
#[cfg(target_os = "android")]
mod android {
    use std::sync::OnceLock;
    use tauri::AppHandle;
    use tauri_plugin_kouros_mobile::MobileCoreExt;
    use serde::Deserialize;
    static APP: OnceLock<AppHandle> = OnceLock::new();
    pub fn attach(app: AppHandle) { let _ = APP.set(app); }
    fn app() -> Result<&'static AppHandle, String> { APP.get().ok_or_else(|| "Android-модуль ещё не готов".into()) }
    // Android opens normally. Keystore encryption and existing keys are retained.
    pub fn session_open() -> bool { true }
    pub fn authenticate(_reason: &str) -> Result<bool, String> { Ok(true) }
    pub fn end_session() -> Result<(), String> { Ok(()) }
    pub fn secret(action: &str, name: &str, value: Option<&str>) -> Result<Option<String>, String> {
        #[derive(Deserialize)] struct Reply { value: Option<String> }
        let result: serde_json::Value = app()?.mobile_core().call("secret", serde_json::json!({"action":action,"name":name,"value":value}))?;
        if action == "get" { serde_json::from_value::<Reply>(result).map(|v| v.value).map_err(|_| "Неверный ответ хранилища".into()) } else { Ok(None) }
    }
    pub fn version_code() -> Result<u64, String> {
        #[derive(Deserialize)] #[serde(rename_all = "camelCase")] struct Info { version_code: u64 }
        app()?.mobile_core().call::<Info>("installedInfo", ()).map(|i| i.version_code)
    }
    pub fn install(url: &str, version_code: u64, sha256: &str) -> Result<(), String> { app()?.mobile_core().call("installUpdate", serde_json::json!({"url":url,"versionCode":version_code,"sha256":sha256})) }
}
#[cfg(target_os = "android")]
pub use android::*;
