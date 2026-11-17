//! Standard OS permission checks. A configured source is not evidence that capture is allowed.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    #[default]
    Unknown,
    NotDetermined,
    Granted,
    Required,
    Denied,
    Restricted,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Permissions {
    pub screen: Status,
    pub microphone: Status,
}

pub fn status() -> Permissions {
    #[cfg(target_os = "macos")]
    {
        mac::status()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Permissions {
            screen: Status::Unsupported,
            microphone: Status::Unknown,
        }
    }
}

pub fn ensure_screen() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        mac::ensure_screen()
    }
    #[cfg(not(target_os = "macos"))]
    {
        anyhow::bail!("screen capture is not implemented for this platform")
    }
}

pub fn ensure_microphone(cancel: &AtomicBool) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        mac::ensure_microphone(cancel)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = cancel;
        Ok(())
    }
}

pub fn open_settings() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        ensure!(
            std::process::Command::new("/usr/bin/open")
                .args(["-b", "com.apple.systempreferences"])
                .status()?
                .success(),
            "could not open System Settings"
        );
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        anyhow::bail!("open your operating system privacy settings manually")
    }
}

#[cfg(any(target_os = "macos", test))]
fn wait_for_microphone(rx: std::sync::mpsc::Receiver<bool>, cancel: &AtomicBool) -> Result<()> {
    use std::{
        sync::{atomic::Ordering, mpsc::RecvTimeoutError},
        time::{Duration, Instant},
    };
    let started = Instant::now();
    loop {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "microphone request cancelled before capture started"
        );
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(granted) => {
                ensure!(
                    granted,
                    "Microphone permission denied. Enable Cueglint in System Settings → Privacy & Security → Microphone, then resume."
                );
                return Ok(());
            }
            Err(RecvTimeoutError::Disconnected) => {
                anyhow::bail!("microphone permission request was interrupted")
            }
            Err(RecvTimeoutError::Timeout) => ensure!(
                started.elapsed() < Duration::from_secs(60),
                "Microphone approval is still required. Grant access in System Settings, then resume."
            ),
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    pub fn status() -> Permissions {
        let microphone = unsafe {
            AVMediaTypeAudio.map(|media| AVCaptureDevice::authorizationStatusForMediaType(media))
        };
        let microphone = match microphone {
            Some(AVAuthorizationStatus::Authorized) => Status::Granted,
            Some(AVAuthorizationStatus::NotDetermined) => Status::NotDetermined,
            Some(AVAuthorizationStatus::Denied) => Status::Denied,
            Some(AVAuthorizationStatus::Restricted) => Status::Restricted,
            _ => Status::Unknown,
        };
        Permissions {
            screen: if unsafe { CGPreflightScreenCaptureAccess() } {
                Status::Granted
            } else {
                Status::Required
            },
            microphone,
        }
    }
    pub fn ensure_screen() -> Result<()> {
        ensure!(
            unsafe { CGPreflightScreenCaptureAccess() || CGRequestScreenCaptureAccess() },
            "Screen Recording permission is not granted. Enable Cueglint in System Settings → Privacy & Security → Screen & System Audio Recording. Restart Cueglint if macOS requests it, then retry."
        );
        Ok(())
    }
    pub fn ensure_microphone(cancel: &AtomicBool) -> Result<()> {
        match status().microphone {
            Status::Granted => Ok(()),
            Status::NotDetermined => {
                let (tx, rx) = std::sync::mpsc::sync_channel(1);
                let callback = block2::RcBlock::new(move |allowed: Bool| {
                    let _ = tx.try_send(allowed.as_bool());
                });
                let media = unsafe { AVMediaTypeAudio }
                    .ok_or_else(|| anyhow::anyhow!("microphone authorization API unavailable"))?;
                // The OS owns this asynchronous request; no audio is acquired while waiting.
                unsafe {
                    AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &callback);
                }
                wait_for_microphone(rx, cancel)
            }
            _ => anyhow::bail!(
                "Microphone permission is not granted. Enable Cueglint in System Settings → Privacy & Security → Microphone, then resume."
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_is_responsive_without_granting_or_starting_capture() {
        let (_tx, rx) = std::sync::mpsc::channel();
        assert!(
            wait_for_microphone(rx, &AtomicBool::new(true))
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );
    }
    #[test]
    fn denial_and_interrupted_authorization_fail_closed() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(false).unwrap();
        assert!(wait_for_microphone(rx, &AtomicBool::new(false)).is_err());
        let (tx, rx) = std::sync::mpsc::channel();
        drop(tx);
        assert!(wait_for_microphone(rx, &AtomicBool::new(false)).is_err());
    }
}
