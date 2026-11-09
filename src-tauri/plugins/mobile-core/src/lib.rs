use tauri::{plugin::{Builder, PluginHandle, TauriPlugin}, Manager, Runtime};
use serde::de::DeserializeOwned;
use serde::Serialize;

pub struct MobileCore<R: Runtime>(PluginHandle<R>);
impl<R: Runtime> MobileCore<R> {
    pub fn call<T: DeserializeOwned>(&self, command: &str, args: impl Serialize) -> Result<T, String> {
        self.0.run_mobile_plugin(command, args).map_err(|e| e.to_string())
    }
}
pub trait MobileCoreExt<R: Runtime> {
    fn mobile_core(&self) -> &MobileCore<R>;
}
impl<R: Runtime, T: Manager<R>> MobileCoreExt<R> for T {
    fn mobile_core(&self) -> &MobileCore<R> { self.state::<MobileCore<R>>().inner() }
}
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("kouros-mobile")
        // Tauri can forward unhandled plugin IPC directly to Android. Explicitly reject
        // every WebView invocation; only run_mobile_plugin from trusted Rust may enter.
        .invoke_handler(|invoke| {
            invoke.resolver.reject("Native Android bridge is not available to the WebView");
            true
        })
        .setup(|app, api| {
        let handle = api.register_android_plugin("app.kouros.mobile", "MobileCorePlugin")?;
        app.manage(MobileCore(handle));
        Ok(())
    }).build()
}
