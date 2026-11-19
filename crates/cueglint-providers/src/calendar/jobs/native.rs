use super::{Backend, Gate};
use crate::calendar::{
    CalendarError, CalendarRefreshToken, CalendarResult, CalendarSession, GoogleCalendar,
    UpcomingEvents,
};
use std::{
    future::Future,
    io,
    time::{Duration, SystemTime},
};
use url::Url;

const CANCEL_POLL: Duration = Duration::from_millis(50);

pub(super) struct Native<F> {
    runtime: tokio::runtime::Runtime,
    launch: F,
}

impl<F> Native<F> {
    pub fn new(launch: F) -> io::Result<Self> {
        Ok(Self {
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .max_blocking_threads(1)
                .thread_name("cueglint-calendar-dns")
                .build()?,
            launch,
        })
    }

    fn run<T>(
        &self,
        job: impl Future<Output = CalendarResult<T>>,
        gate: &Gate,
    ) -> CalendarResult<T> {
        self.runtime.block_on(async {
            let mut job = std::pin::pin!(job);
            loop {
                gate.check()?;
                if let Ok(result) = tokio::time::timeout(CANCEL_POLL, job.as_mut()).await {
                    return result;
                }
            }
        })
    }
}

impl<F: Fn(&Url) -> CalendarResult<()>> Backend for Native<F> {
    type Client = GoogleCalendar;

    fn profile(&self) -> CalendarResult<Option<String>> {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            match profile_entry()?.get_password() {
                Ok(id) => Ok(Some(id)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Err(CalendarError::VaultUnavailable),
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        Err(CalendarError::VaultUnavailable)
    }

    fn save_profile(&self, id: &str) -> CalendarResult<()> {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            profile_entry()?
                .set_password(id)
                .map_err(|_| CalendarError::VaultUnavailable)
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = id;
            Err(CalendarError::VaultUnavailable)
        }
    }

    fn client(&self, id: &str) -> CalendarResult<Self::Client> {
        GoogleCalendar::new(id.to_owned(), None)
    }

    fn saved_refresh(&self, client: &Self::Client) -> CalendarResult<Option<CalendarRefreshToken>> {
        client.load_saved_refresh_token()
    }

    fn save_refresh(
        &self,
        client: &Self::Client,
        token: &CalendarRefreshToken,
    ) -> CalendarResult<()> {
        client.save_refresh_token(token)
    }

    fn forget_refresh(&self, client: &Self::Client) -> CalendarResult<()> {
        client.forget_saved_refresh_token()
    }

    fn authorize(&self, client: &Self::Client, gate: &Gate) -> CalendarResult<CalendarSession> {
        self.run(
            async {
                let pending = client.begin_connect().await?;
                gate.check()?;
                (self.launch)(pending.authorization_url())?;
                client.finish_connect(pending).await
            },
            gate,
        )
    }

    fn refresh(
        &self,
        client: &Self::Client,
        token: &CalendarRefreshToken,
        gate: &Gate,
    ) -> CalendarResult<CalendarSession> {
        self.run(client.refresh(token), gate)
    }

    fn upcoming(
        &self,
        client: &Self::Client,
        session: &CalendarSession,
        now: SystemTime,
        gate: &Gate,
    ) -> CalendarResult<UpcomingEvents> {
        self.run(client.upcoming(session, now), gate)
    }

    fn revoke(&self, client: &Self::Client, token: &CalendarRefreshToken) -> CalendarResult<()> {
        self.runtime.block_on(client.revoke(token))
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn profile_entry() -> CalendarResult<keyring::Entry> {
    keyring::Entry::new("org.cueglint.google-calendar-client.v1", "desktop")
        .map_err(|_| CalendarError::VaultUnavailable)
}
