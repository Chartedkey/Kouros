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
    pub fn session_open() -> bool {
        #[derive(Deserialize)] struct Status { open: bool }
        app().and_then(|a| a.mobile_core().call::<Status>("sessionStatus", ())).map(|s| s.open).unwrap_or(false)
    }
    pub fn authenticate(reason: &str) -> Result<bool, String> {
        use tauri_plugin_biometric::{AuthOptions, BiometricExt};
        let app = app()?;
        app.biometric().authenticate(reason.into(), AuthOptions {
            allow_device_credential: true, title: Some("Kouros".into()), ..Default::default()
        }).map_err(|_| "Вход не подтверждён. Используйте отпечаток или PIN-код телефона.".to_string())?;
        app.mobile_core().call::<()>("startSession", ())?;
        Ok(true)
    }
    pub fn end_session() -> Result<(), String> { app()?.mobile_core().call("endSession", ()) }
    pub fn secret(action: &str, name: &str, value: Option<&str>) -> Result<Option<String>, String> {
        #[derive(Deserialize)] struct Reply { value: Option<String> }
        let result: serde_json::Value = app()?.mobile_core().call("secret", serde_json::json!({"action":action,"name":name,"value":value}))?;
        if action == "get" { serde_json::from_value::<Reply>(result).map(|v| v.value).map_err(|_| "Неверный ответ хранилища".into()) } else { Ok(None) }
    }
    pub fn version_code() -> Result<u64, String> {
        #[derive(Deserialize)] #[serde(rename_all = "camelCase")] struct Info { version_code: u64 }
        app()?.mobile_core().call::<Info>("installedInfo", ()).map(|i| i.version_code)
    }
    pub fn install(url: &str) -> Result<(), String> { app()?.mobile_core().call("installUpdate", serde_json::json!({"url":url})) }
}
#[cfg(target_os = "android")]
pub use android::*;
