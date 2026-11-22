//! Touch ID (macOS) and Windows Hello: the system asks the user, TermF only hears yes or no. No signed app, so
//! no keychain item the system itself keeps behind it: a gate in front of the key, not a bond with it. Linux has
//! no standard way to ask.

use anyhow::Result;

/// What this machine offers right now: `touchId` or `windowsHello` once set up; `None` elsewhere.
pub fn available() -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    {
        mac::available().then_some("touchId")
    }
    #[cfg(windows)]
    {
        win::available().then_some("windowsHello")
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        None
    }
}

/// Asks the user, with `reason` in the system's window; an error unless they confirmed.
pub fn verify(reason: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        mac::verify(reason)
    }
    #[cfg(windows)]
    {
        win::verify(reason)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = reason;
        anyhow::bail!("no system biometrics here")
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::sync::mpsc;

    use anyhow::{Context, Result, ensure};
    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use objc2_foundation::NSString;

    #[link(name = "LocalAuthentication", kind = "framework")]
    unsafe extern "C" {}

    /// `LAPolicyDeviceOwnerAuthenticationWithBiometrics`: Touch ID, not the Mac's password.
    const BIOMETRICS: isize = 1;

    fn context() -> Option<Retained<AnyObject>> {
        let class = AnyClass::get(c"LAContext")?;
        // SAFETY: `new` on an NSObject subclass returns a retained instance.
        Some(unsafe { msg_send![class, new] })
    }

    pub fn available() -> bool {
        let Some(context) = context() else {
            return false;
        };
        let no_error = std::ptr::null_mut::<*mut AnyObject>();
        // SAFETY: `canEvaluatePolicy:error:` takes an LAPolicy (NSInteger) and an optional NSError**.
        unsafe { msg_send![&*context, canEvaluatePolicy: BIOMETRICS, error: no_error] }
    }

    pub fn verify(reason: &str) -> Result<()> {
        let context = context().context("LocalAuthentication is missing")?;
        let reason = NSString::from_str(reason);
        let (tx, rx) = mpsc::channel();
        let reply = RcBlock::new(move |ok: Bool, _error: *mut AnyObject| {
            let _ = tx.send(ok.as_bool());
        });
        // SAFETY: `evaluatePolicy:localizedReason:reply:` with an LAPolicy, an NSString and a block (BOOL,
        // NSError*); the context stays alive until the reply came.
        unsafe {
            let _: () = msg_send![
                &*context,
                evaluatePolicy: BIOMETRICS,
                localizedReason: &*reason,
                reply: &*reply
            ];
        }
        let ok = rx.recv().unwrap_or(false);
        drop(context);
        ensure!(ok, "Touch ID did not confirm");
        Ok(())
    }
}

#[cfg(windows)]
mod win {
    use anyhow::{Result, ensure};
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    };
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
    use windows::Win32::System::WinRT::IUserConsentVerifierInterop;
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    use windows::core::{HSTRING, factory};
    use windows_future::IAsyncOperation;

    fn apartment() {
        // SAFETY: joins this worker thread to the multithreaded apartment WinRT calls need; already in one is fine.
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    }

    pub fn available() -> bool {
        apartment();
        UserConsentVerifier::CheckAvailabilityAsync()
            .and_then(|op| op.join())
            .is_ok_and(|a| a == UserConsentVerifierAvailability::Available)
    }

    pub fn verify(reason: &str) -> Result<()> {
        apartment();
        let interop = factory::<UserConsentVerifier, IUserConsentVerifierInterop>()?;
        // SAFETY: the window in front is TermF's, whose button asked for it; Windows owns its dialog.
        let op: IAsyncOperation<UserConsentVerificationResult> = unsafe {
            interop
                .RequestVerificationForWindowAsync(GetForegroundWindow(), &HSTRING::from(reason))?
        };
        ensure!(
            op.join()? == UserConsentVerificationResult::Verified,
            "Windows Hello did not confirm"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_what_this_machine_offers_without_asking() {
        let kind = available();
        #[cfg(target_os = "linux")]
        assert_eq!(kind, None);
        #[cfg(target_os = "macos")]
        assert!(kind.is_none() || kind == Some("touchId"));
        #[cfg(windows)]
        assert!(kind.is_none() || kind == Some("windowsHello"));
    }

    /// Shows the system's prompt: run by hand on a machine with Touch ID or Windows Hello.
    #[test]
    #[ignore]
    fn asks_the_user() {
        verify("check TermF's biometrics test").unwrap();
    }
}
