use super::*;
use crate::{
    Secret,
    calendar::{CalendarEvent, EventTime},
};
use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, Instant},
};

const CLIENT: &str = "original-desktop-client";
const WAIT: Duration = Duration::from_secs(3);

#[derive(Clone, Default)]
struct Fake(Arc<Mutex<Script>>);

#[derive(Default)]
struct Script {
    profile: Option<String>,
    grant: Option<String>,
    pages: VecDeque<CalendarResult<UpcomingEvents>>,
    next_refresh: Option<CalendarResult<&'static str>>,
    revoke_error: Option<CalendarError>,
    authorize_error: Option<CalendarError>,
    profile_error: Option<CalendarError>,
    reads: usize,
    authorizations: usize,
    refreshes: usize,
    revocations: usize,
    saves: usize,
    connect_hold: Option<(SyncSender<()>, Receiver<()>)>,
    page_hold: Option<(SyncSender<()>, Receiver<()>)>,
}

fn grant(secret: &str) -> CalendarRefreshToken {
    CalendarRefreshToken {
        client_key: CLIENT.to_owned(),
        secret: Secret::new(secret.to_owned()).unwrap(),
    }
}

fn session(value: &str) -> CalendarSession {
    CalendarSession {
        access: Secret::new("synthetic-access".to_owned()).unwrap(),
        expires_at: Instant::now() + Duration::from_secs(3600),
        refresh: grant(value),
    }
}

fn page(title: &str) -> UpcomingEvents {
    UpcomingEvents {
        events: vec![CalendarEvent {
            id: "original-event".into(),
            title: Some(title.into()),
            time: EventTime::AllDay {
                starts_on: "2026-11-20".into(),
                ends_before: "2026-11-21".into(),
            },
        }],
        calendar_time_zone: Some("Europe/Lisbon".into()),
        has_more: false,
    }
}

fn hold(blocking: Option<(SyncSender<()>, Receiver<()>)>) {
    if let Some((entered, release)) = blocking {
        entered.send(()).unwrap();
        release.recv_timeout(WAIT).unwrap();
    }
}

impl Backend for Fake {
    type Client = String;

    fn profile(&self) -> CalendarResult<Option<String>> {
        let mut script = self.0.lock().unwrap();
        script.reads += 1;
        script
            .profile_error
            .map_or_else(|| Ok(script.profile.clone()), Err)
    }

    fn save_profile(&self, id: &str) -> CalendarResult<()> {
        self.0.lock().unwrap().profile = Some(id.to_owned());
        Ok(())
    }

    fn client(&self, id: &str) -> CalendarResult<Self::Client> {
        if id == CLIENT {
            Ok(id.to_owned())
        } else {
            Err(CalendarError::InvalidClient)
        }
    }

    fn saved_refresh(&self, _: &Self::Client) -> CalendarResult<Option<CalendarRefreshToken>> {
        Ok(self.0.lock().unwrap().grant.as_deref().map(grant))
    }

    fn save_refresh(&self, _: &Self::Client, token: &CalendarRefreshToken) -> CalendarResult<()> {
        let mut script = self.0.lock().unwrap();
        script.saves += 1;
        script.grant = Some(token.secret.expose().to_owned());
        Ok(())
    }

    fn forget_refresh(&self, _: &Self::Client) -> CalendarResult<()> {
        self.0.lock().unwrap().grant = None;
        Ok(())
    }

    fn authorize(&self, _: &Self::Client, _: &Gate) -> CalendarResult<CalendarSession> {
        let (blocking, error) = {
            let mut script = self.0.lock().unwrap();
            script.authorizations += 1;
            (script.connect_hold.take(), script.authorize_error.take())
        };
        hold(blocking);
        error.map_or_else(|| Ok(session("original-offline-grant")), Err)
    }

    fn refresh(
        &self,
        _: &Self::Client,
        _: &CalendarRefreshToken,
        _: &Gate,
    ) -> CalendarResult<CalendarSession> {
        let mut script = self.0.lock().unwrap();
        script.refreshes += 1;
        script.next_refresh.take().unwrap().map(session)
    }

    fn upcoming(
        &self,
        _: &Self::Client,
        _: &CalendarSession,
        _: SystemTime,
        _: &Gate,
    ) -> CalendarResult<UpcomingEvents> {
        let (blocking, result) = {
            let mut script = self.0.lock().unwrap();
            (
                script.page_hold.take(),
                script
                    .pages
                    .pop_front()
                    .unwrap_or_else(|| Ok(page("Original event"))),
            )
        };
        hold(blocking);
        result
    }

    fn revoke(&self, _: &Self::Client, _: &CalendarRefreshToken) -> CalendarResult<()> {
        let mut script = self.0.lock().unwrap();
        script.revocations += 1;
        script.revoke_error.map_or(Ok(()), Err)
    }
}

fn worker(fake: &Fake) -> (CalendarWorker, Receiver<()>) {
    let (wake, updates) = mpsc::sync_channel(4);
    let worker = CalendarWorker::spawn_with(fake.clone(), move || {
        let _ = wake.try_send(());
    })
    .unwrap();
    (worker, updates)
}

fn settle(worker: &mut CalendarWorker, updates: &Receiver<()>) {
    let deadline = Instant::now() + WAIT;
    while worker.snapshot().operation.is_some() {
        updates
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        worker.poll();
    }
}

fn submit(worker: &mut CalendarWorker, updates: &Receiver<()>, command: CalendarCommand) {
    worker.submit(command).unwrap();
    settle(worker, updates);
}

fn connect() -> CalendarCommand {
    CalendarCommand::Connect {
        client_id: CLIENT.into(),
    }
}

#[test]
fn connect_and_refresh_rotate_a_grant_and_preserve_only_transient_stale_data() {
    let fake = Fake::default();
    let (mut worker, updates) = worker(&fake);
    assert_eq!(fake.0.lock().unwrap().reads, 0);
    submit(&mut worker, &updates, CalendarCommand::Inspect);
    assert_eq!(
        worker.snapshot().connection,
        CalendarConnection::Disconnected
    );
    assert_eq!(fake.0.lock().unwrap().authorizations, 0);
    submit(&mut worker, &updates, connect());
    assert!(worker.snapshot().configured && worker.snapshot().upcoming.is_some());
    assert_eq!(worker.snapshot().connection, CalendarConnection::Connected);

    {
        let mut script = fake.0.lock().unwrap();
        script.pages.extend([
            Err(CalendarError::RefreshRequired),
            Ok(page("Refreshed fixture")),
            Err(CalendarError::Transport),
            Err(CalendarError::RefreshRequired),
        ]);
        script.next_refresh = Some(Ok("rotated-offline-grant"));
    }
    submit(&mut worker, &updates, CalendarCommand::Refresh);
    let recent = worker.snapshot().upcoming.as_ref().unwrap().clone();
    let refreshed_at = worker.snapshot().refreshed_at;
    assert_eq!(recent.events[0].title.as_deref(), Some("Refreshed fixture"));
    {
        let script = fake.0.lock().unwrap();
        assert_eq!(script.refreshes, 1);
        assert_eq!(script.saves, 2);
        assert_eq!(script.grant.as_deref(), Some("rotated-offline-grant"));
    }
    submit(&mut worker, &updates, CalendarCommand::Refresh);
    assert_eq!(worker.snapshot().error, Some(CalendarError::Transport));
    assert!(worker.snapshot().stale);
    assert_eq!(worker.snapshot().refreshed_at, refreshed_at);
    assert!(Arc::ptr_eq(
        worker.snapshot().upcoming.as_ref().unwrap(),
        &recent
    ));

    fake.0.lock().unwrap().next_refresh = Some(Err(CalendarError::ReconnectRequired));
    submit(&mut worker, &updates, CalendarCommand::Refresh);
    assert_eq!(
        worker.snapshot().connection,
        CalendarConnection::ReconnectRequired
    );
    assert!(worker.snapshot().upcoming.is_none());
    assert_eq!(fake.0.lock().unwrap().authorizations, 1);
    fake.0.lock().unwrap().authorize_error = Some(CalendarError::AuthorizationDenied);
    submit(&mut worker, &updates, CalendarCommand::Reconnect);
    assert_eq!(
        worker.snapshot().connection,
        CalendarConnection::ReconnectRequired
    );
    assert_eq!(
        worker.snapshot().error,
        Some(CalendarError::AuthorizationDenied)
    );
    submit(&mut worker, &updates, CalendarCommand::Reconnect);
    assert_eq!(worker.snapshot().connection, CalendarConnection::Connected);
    assert_eq!(fake.0.lock().unwrap().authorizations, 3);
}

#[test]
fn a_late_committed_connection_cannot_restore_events_after_disconnect() {
    let fake = Fake::default();
    let (started, entered) = mpsc::sync_channel(1);
    let (release, permit) = mpsc::sync_channel(1);
    fake.0.lock().unwrap().page_hold = Some((started, permit));
    let (mut worker, updates) = worker(&fake);
    worker.submit(connect()).unwrap();
    entered.recv_timeout(WAIT).unwrap();
    assert_eq!(fake.0.lock().unwrap().saves, 1);
    assert_eq!(
        worker.submit(CalendarCommand::Cancel),
        Err(CalendarError::Busy)
    );
    fake.0.lock().unwrap().revoke_error = Some(CalendarError::Transport);
    worker
        .submit(CalendarCommand::Disconnect { local_only: false })
        .unwrap();
    assert!(worker.snapshot().upcoming.is_none());
    release.send(()).unwrap();
    settle(&mut worker, &updates);
    assert!(worker.snapshot().upcoming.is_none());
    assert_eq!(
        worker.snapshot().disconnect,
        Some(CalendarDisconnectReport {
            remote: CalendarRevocation::Failed(CalendarError::Transport),
            local: None,
        })
    );
    assert!(fake.0.lock().unwrap().grant.is_some());
    submit(
        &mut worker,
        &updates,
        CalendarCommand::Disconnect { local_only: true },
    );
    assert_eq!(
        worker.snapshot().connection,
        CalendarConnection::Disconnected
    );
    assert_eq!(
        worker.snapshot().disconnect,
        Some(CalendarDisconnectReport {
            remote: CalendarRevocation::Skipped,
            local: Some(Ok(()))
        })
    );
    assert!(fake.0.lock().unwrap().grant.is_none());
}

#[test]
fn restore_retires_authorization_before_a_late_callback_can_save_its_grant() {
    let fake = Fake::default();
    let (started, entered) = mpsc::sync_channel(1);
    let (release, permit) = mpsc::sync_channel(1);
    fake.0.lock().unwrap().connect_hold = Some((started, permit));
    let (mut worker, updates) = worker(&fake);
    worker.submit(connect()).unwrap();
    entered.recv_timeout(WAIT).unwrap();
    worker.reset_after_restore();
    assert_eq!(
        worker.snapshot().operation,
        Some(CalendarOperation::Cancelling)
    );
    release.send(()).unwrap();
    settle(&mut worker, &updates);
    assert_eq!(
        worker.snapshot().connection,
        CalendarConnection::Uninspected
    );
    assert!(worker.snapshot().upcoming.is_none());
    {
        let script = fake.0.lock().unwrap();
        assert_eq!(script.saves, 0);
        assert_eq!(script.revocations, 1);
        assert!(script.grant.is_none());
    }
    submit(&mut worker, &updates, CalendarCommand::Inspect);
    assert_eq!(
        worker.snapshot().connection,
        CalendarConnection::Disconnected
    );
    assert!(worker.snapshot().configured);
    assert_eq!(fake.0.lock().unwrap().authorizations, 1);
}

#[test]
fn reopening_requires_explicit_refresh_and_disconnect_errors_keep_events_hidden() {
    let fake = Fake::default();
    {
        let mut script = fake.0.lock().unwrap();
        script.profile = Some(CLIENT.into());
        script.grant = Some("existing-offline-grant".into());
        script.next_refresh = Some(Ok("renewed-offline-grant"));
    }
    let (mut worker, updates) = worker(&fake);
    submit(&mut worker, &updates, CalendarCommand::Inspect);
    assert_eq!(worker.snapshot().connection, CalendarConnection::Connected);
    assert!(worker.snapshot().upcoming.is_none());
    assert_eq!(fake.0.lock().unwrap().refreshes, 0);
    submit(&mut worker, &updates, CalendarCommand::Refresh);
    assert!(worker.snapshot().upcoming.is_some());
    assert_eq!(
        fake.0.lock().unwrap().grant.as_deref(),
        Some("renewed-offline-grant")
    );
    assert_eq!(fake.0.lock().unwrap().authorizations, 0);
    fake.0.lock().unwrap().profile_error = Some(CalendarError::VaultUnavailable);
    submit(
        &mut worker,
        &updates,
        CalendarCommand::Disconnect { local_only: false },
    );
    assert_eq!(
        worker.snapshot().error,
        Some(CalendarError::VaultUnavailable)
    );
    assert!(worker.snapshot().upcoming.is_none());
    assert!(!worker.snapshot().stale);
    assert_eq!(fake.0.lock().unwrap().revocations, 0);
}
