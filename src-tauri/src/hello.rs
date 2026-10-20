//! Windows Hello (PIN, fingerprint, face) via UserConsentVerifier.
//! On other platforms everything reports "unavailable" and verification passes.

#[cfg(windows)]
mod imp {
    use tauri::WebviewWindow;
    use windows::core::{factory, HSTRING};
    use windows::Foundation::IAsyncOperation;
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::WinRT::IUserConsentVerifierInterop;

    pub fn availability() -> &'static str {
        match UserConsentVerifier::CheckAvailabilityAsync().and_then(|op| op.get()) {
            Ok(a) if a == UserConsentVerifierAvailability::Available => "available",
            Ok(a) if a == UserConsentVerifierAvailability::NotConfiguredForUser => "not_configured",
            _ => "unavailable",
        }
    }

    pub fn verify(window: &WebviewWindow, message: &str) -> Result<bool, String> {
        if availability() != "available" {
            return Ok(true);
        }
        // Parent the system dialog to our window so it appears in front of it.
        let raw = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
        let hwnd = HWND(raw as *mut core::ffi::c_void);
        let interop = factory::<UserConsentVerifier, IUserConsentVerifierInterop>()
            .map_err(|e| format!("Windows Hello недоступен: {e}"))?;
        let op: IAsyncOperation<UserConsentVerificationResult> =
            unsafe { interop.RequestVerificationForWindowAsync(hwnd, &HSTRING::from(message)) }
                .map_err(|e| format!("Windows Hello: {e}"))?;
        let result = op.get().map_err(|e| format!("Windows Hello: {e}"))?;
        Ok(result == UserConsentVerificationResult::Verified)
    }
}

#[cfg(not(windows))]
mod imp {
    use tauri::WebviewWindow;

    pub fn availability() -> &'static str {
        "unavailable"
    }

    pub fn verify(_window: &WebviewWindow, _message: &str) -> Result<bool, String> {
        Ok(true)
    }
}

pub use imp::{availability, verify};
