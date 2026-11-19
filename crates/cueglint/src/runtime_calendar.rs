use super::Runtime;
use anyhow::{Result, ensure};
use cueglint_providers::calendar::{
    CalendarCommand, CalendarConnection, CalendarError, CalendarResult, CalendarSnapshot,
    CalendarWorker, GoogleCalendar,
};
use url::Url;

pub(super) fn initial_snapshot() -> CalendarSnapshot {
    CalendarSnapshot {
        connection: if GoogleCalendar::native_vault_supported() {
            CalendarConnection::Uninspected
        } else {
            CalendarConnection::Unavailable
        },
        ..CalendarSnapshot::default()
    }
}

impl Runtime {
    pub fn calendar_command(&mut self, command: CalendarCommand) -> Result<()> {
        if matches!(command, CalendarCommand::Cancel) && self.calendar.is_none() {
            return Ok(());
        }
        ensure!(
            !self.fixture,
            "Calendar operations are disabled in deterministic fixtures"
        );
        ensure!(
            GoogleCalendar::native_vault_supported(),
            "The native Calendar credential store is unavailable on this platform"
        );
        if self.calendar.is_none() {
            let ctx = self.ctx.clone();
            self.calendar = Some(CalendarWorker::spawn(open_authorization, move || {
                ctx.request_repaint()
            })?);
        }
        let calendar = self.calendar.as_mut().unwrap();
        calendar.submit(command)?;
        self.snapshot.calendar = calendar.snapshot().clone();
        Ok(())
    }

    pub fn poll_calendar(&mut self) -> bool {
        if let Some(calendar) = &mut self.calendar
            && calendar.poll()
        {
            self.snapshot.calendar = calendar.snapshot().clone();
            return true;
        }
        false
    }

    pub fn transfer_calendar_after_restore(&mut self, restored: &mut Self) {
        if let Some(mut calendar) = self.calendar.take() {
            calendar.reset_after_restore();
            restored.snapshot.calendar = calendar.snapshot().clone();
            restored.calendar = Some(calendar);
        }
    }
}

fn open_authorization(url: &Url) -> CalendarResult<()> {
    if url.scheme() != "https"
        || url.host_str() != Some("accounts.google.com")
        || url.path() != "/o/oauth2/v2/auth"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return Err(CalendarError::BrowserUnavailable);
    }
    open_default_browser(url.as_str())
}

#[cfg(target_os = "macos")]
fn open_default_browser(url: &str) -> CalendarResult<()> {
    use std::{
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    // Use the system launcher without shell interpolation, output or URL logging.
    let mut child = Command::new("/usr/bin/open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| CalendarError::BrowserUnavailable)?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err(CalendarError::BrowserUnavailable),
            Ok(None) if started.elapsed() < Duration::from_secs(3) => {
                thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CalendarError::BrowserUnavailable);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn open_default_browser(url: &str) -> CalendarResult<()> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::{
        System::Com::{
            COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
        },
        UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    };

    let wide: Vec<u16> = url.encode_utf16().chain([0]).collect();
    // This dedicated worker calls ShellExecuteW with a null-terminated HTTPS URL.
    // Initialize its COM apartment before opening the default handler.
    let result = unsafe {
        if CoInitializeEx(
            null(),
            (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
        ) < 0
        {
            return Err(CalendarError::BrowserUnavailable);
        }
        let result = ShellExecuteW(
            null_mut(),
            null(),
            wide.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        );
        CoUninitialize();
        result as isize
    };
    if result > 32 {
        Ok(())
    } else {
        Err(CalendarError::BrowserUnavailable)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn open_default_browser(_: &str) -> CalendarResult<()> {
    Err(CalendarError::BrowserUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_fixture_rejects_inspection_and_connection_without_starting_a_worker() {
        let mut store = cueglint_core::Store::open_in_memory().unwrap();
        let mut runtime = Runtime::new(&store, true, eframe::egui::Context::default()).unwrap();
        let mut selected = None;
        use crate::worker::{Command, execute};
        for action in [
            CalendarCommand::Inspect,
            CalendarCommand::Connect {
                client_id: "fixture-desktop-client".into(),
            },
            CalendarCommand::Refresh,
            CalendarCommand::Reconnect,
            CalendarCommand::Disconnect { local_only: false },
        ] {
            let error = execute(
                &mut store,
                &mut selected,
                &mut runtime,
                Command::Calendar(action),
            )
            .unwrap_err();
            assert!(error.to_string().contains("deterministic fixtures"));
            assert!(runtime.calendar.is_none());
        }
    }

    #[test]
    fn calendar_launcher_rejects_non_authorization_urls_without_opening_them() {
        for input in [
            "https://example.test/o/oauth2/v2/auth",
            "http://accounts.google.com/o/oauth2/v2/auth",
            "https://accounts.google.com/other",
            "https://accounts.google.com:567/o/oauth2/v2/auth",
            "https://user@accounts.google.com/o/oauth2/v2/auth",
        ] {
            let url = Url::parse(input).unwrap();
            assert_eq!(
                open_authorization(&url),
                Err(CalendarError::BrowserUnavailable)
            );
        }
    }
}
