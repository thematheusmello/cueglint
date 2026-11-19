//! One device-local calendar connection, coordinated independently of inference.

mod native;
#[cfg(test)]
mod tests;

use super::{CalendarError, CalendarRefreshToken, CalendarResult, CalendarSession, UpcomingEvents};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::SystemTime,
};
use url::Url;

pub enum CalendarCommand {
    Inspect,
    Connect { client_id: String },
    Refresh,
    Reconnect,
    Disconnect { local_only: bool },
    Cancel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CalendarConnection {
    #[default]
    Uninspected,
    Unavailable,
    Disconnected,
    Connected,
    ReconnectRequired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarOperation {
    Inspecting,
    Connecting,
    Refreshing,
    Disconnecting,
    Cancelling,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarRevocation {
    Revoked,
    NoGrant,
    Skipped,
    Failed(CalendarError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalendarDisconnectReport {
    pub remote: CalendarRevocation,
    /// Absent when a remote failure leaves the credential available for a retry.
    pub local: Option<CalendarResult<()>>,
}

#[derive(Clone, Default)]
pub struct CalendarSnapshot {
    pub connection: CalendarConnection,
    pub configured: bool,
    pub operation: Option<CalendarOperation>,
    pub upcoming: Option<Arc<UpcomingEvents>>,
    pub refreshed_at: Option<SystemTime>,
    pub stale: bool,
    pub error: Option<CalendarError>,
    pub disconnect: Option<CalendarDisconnectReport>,
}

impl CalendarSnapshot {
    fn clear_events(&mut self) {
        self.upcoming = None;
        self.refreshed_at = None;
        self.stale = false;
    }
}

const RUNNING: u8 = 0;
const CANCELLED: u8 = 1;
const COMMITTING: u8 = 2;

#[derive(Default)]
struct Gate(AtomicU8);

impl Gate {
    fn cancel(&self) -> bool {
        self.0
            .compare_exchange(RUNNING, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    fn check(&self) -> CalendarResult<()> {
        if self.0.load(Ordering::Acquire) == CANCELLED {
            return Err(CalendarError::Cancelled);
        }
        Ok(())
    }

    fn commit(&self) -> CalendarResult<()> {
        self.0
            .compare_exchange(RUNNING, COMMITTING, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| CalendarError::Cancelled)
    }
}

struct Job {
    id: u64,
    command: CalendarCommand,
    gate: Arc<Gate>,
    before: CalendarSnapshot,
    reset_session: bool,
}

struct Completion {
    id: u64,
    snapshot: CalendarSnapshot,
}

struct Active {
    id: u64,
    gate: Arc<Gate>,
    operation: CalendarOperation,
    discard: bool,
}

pub struct CalendarWorker {
    jobs: SyncSender<Job>,
    results: Receiver<Completion>,
    active: Option<Active>,
    next_id: u64,
    reset_session: bool,
    snapshot: CalendarSnapshot,
}

impl CalendarWorker {
    /// The launcher receives only the Google authorization URL constructed inside the transport.
    pub fn spawn(
        launch: impl Fn(&Url) -> CalendarResult<()> + Send + 'static,
        wake: impl Fn() + Send + 'static,
    ) -> io::Result<Self> {
        Self::spawn_with(native::Native::new(launch)?, wake)
    }

    fn spawn_with(
        backend: impl Backend + Send + 'static,
        wake: impl Fn() + Send + 'static,
    ) -> io::Result<Self> {
        // Reserve one slot for an explicit disconnect following a retiring operation.
        let (jobs, queued) = mpsc::sync_channel::<Job>(2);
        let (finished, results) = mpsc::sync_channel(2);
        thread::Builder::new()
            .name("cueglint-calendar".to_owned())
            .spawn(move || {
                let mut session = None;
                while let Ok(job) = queued.recv() {
                    if job.reset_session {
                        session = None;
                    }
                    let snapshot =
                        perform(&backend, &mut session, job.command, &job.gate, job.before);
                    if finished
                        .send(Completion {
                            id: job.id,
                            snapshot,
                        })
                        .is_err()
                    {
                        break;
                    }
                    wake();
                }
            })?;
        Ok(Self {
            jobs,
            results,
            active: None,
            next_id: 0,
            reset_session: false,
            snapshot: CalendarSnapshot::default(),
        })
    }

    pub fn snapshot(&self) -> &CalendarSnapshot {
        &self.snapshot
    }

    pub fn submit(&mut self, command: CalendarCommand) -> CalendarResult<()> {
        let operation = match command {
            CalendarCommand::Inspect => CalendarOperation::Inspecting,
            CalendarCommand::Connect { .. } | CalendarCommand::Reconnect => {
                CalendarOperation::Connecting
            }
            CalendarCommand::Refresh => CalendarOperation::Refreshing,
            CalendarCommand::Disconnect { .. } => CalendarOperation::Disconnecting,
            CalendarCommand::Cancel => return self.cancel(),
        };
        if let Some(active) = &self.active
            && (operation != CalendarOperation::Disconnecting
                || active.operation == CalendarOperation::Disconnecting)
        {
            return Err(CalendarError::Busy);
        }
        let id = self
            .next_id
            .checked_add(1)
            .ok_or(CalendarError::WorkerUnavailable)?;
        let gate = Arc::new(Gate::default());
        let previous = self.active.as_ref();
        if let Some(active) = previous {
            active.gate.cancel();
        }
        self.jobs
            .try_send(Job {
                id,
                command,
                gate: gate.clone(),
                before: self.snapshot.clone(),
                reset_session: self.reset_session,
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => CalendarError::Busy,
                TrySendError::Disconnected(_) => CalendarError::WorkerUnavailable,
            })?;
        self.next_id = id;
        self.reset_session = false;
        self.active = Some(Active {
            id,
            gate,
            operation,
            discard: false,
        });
        self.snapshot.operation = Some(operation);
        self.snapshot.error = None;
        self.snapshot.disconnect = None;
        if operation == CalendarOperation::Disconnecting {
            self.snapshot.clear_events();
        }
        Ok(())
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.results.try_recv() {
                Ok(result) => {
                    if let Some(active) = &self.active
                        && active.id == result.id
                    {
                        if !active.discard {
                            self.snapshot = result.snapshot;
                        }
                        self.snapshot.operation = None;
                        self.active = None;
                        changed = true;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if self.active.take().is_some() {
                        self.snapshot.operation = None;
                        self.snapshot.error = Some(CalendarError::WorkerUnavailable);
                        changed = true;
                    }
                    break;
                }
            }
        }
        changed
    }

    /// Library restore cannot apply a pending authorization result to its replacement runtime.
    pub fn reset_after_restore(&mut self) {
        self.snapshot = CalendarSnapshot::default();
        self.reset_session = true;
        if let Some(active) = &mut self.active {
            active.gate.cancel();
            active.discard = true;
            self.snapshot.operation = Some(CalendarOperation::Cancelling);
        }
    }

    fn cancel(&mut self) -> CalendarResult<()> {
        if let Some(active) = &mut self.active {
            if self.snapshot.operation == Some(CalendarOperation::Cancelling) {
                return Ok(());
            }
            if active.operation == CalendarOperation::Disconnecting || !active.gate.cancel() {
                return Err(CalendarError::Busy);
            }
            active.discard = true;
            self.snapshot.operation = Some(CalendarOperation::Cancelling);
        }
        Ok(())
    }
}

impl Drop for CalendarWorker {
    fn drop(&mut self) {
        if let Some(active) = &self.active {
            active.gate.cancel();
        }
    }
}

trait Backend {
    type Client;

    fn profile(&self) -> CalendarResult<Option<String>>;
    fn save_profile(&self, id: &str) -> CalendarResult<()>;
    fn client(&self, id: &str) -> CalendarResult<Self::Client>;
    fn saved_refresh(&self, client: &Self::Client) -> CalendarResult<Option<CalendarRefreshToken>>;
    fn save_refresh(
        &self,
        client: &Self::Client,
        token: &CalendarRefreshToken,
    ) -> CalendarResult<()>;
    fn forget_refresh(&self, client: &Self::Client) -> CalendarResult<()>;
    fn authorize(&self, client: &Self::Client, gate: &Gate) -> CalendarResult<CalendarSession>;
    fn refresh(
        &self,
        client: &Self::Client,
        token: &CalendarRefreshToken,
        gate: &Gate,
    ) -> CalendarResult<CalendarSession>;
    fn upcoming(
        &self,
        client: &Self::Client,
        session: &CalendarSession,
        now: SystemTime,
        gate: &Gate,
    ) -> CalendarResult<UpcomingEvents>;
    fn revoke(&self, client: &Self::Client, token: &CalendarRefreshToken) -> CalendarResult<()>;
}

fn perform<B: Backend>(
    backend: &B,
    session: &mut Option<CalendarSession>,
    command: CalendarCommand,
    gate: &Gate,
    mut state: CalendarSnapshot,
) -> CalendarSnapshot {
    state.operation = None;
    state.error = None;
    state.disconnect = None;
    if matches!(command, CalendarCommand::Disconnect { .. }) {
        state.clear_events();
    }
    let result = execute(backend, session, command, gate, &mut state);
    if let Err(error) = result {
        match error {
            CalendarError::ReconnectRequired => {
                *session = None;
                state.clear_events();
                state.connection = CalendarConnection::ReconnectRequired;
            }
            CalendarError::NotConnected => {
                *session = None;
                state.clear_events();
                state.connection = CalendarConnection::Disconnected;
            }
            _ => state.stale = state.upcoming.is_some(),
        }
        state.error = Some(error);
    }
    state
}

fn execute<B: Backend>(
    backend: &B,
    session: &mut Option<CalendarSession>,
    command: CalendarCommand,
    gate: &Gate,
    state: &mut CalendarSnapshot,
) -> CalendarResult<()> {
    gate.check()?;
    let profile = backend.profile()?;
    state.configured = profile.is_some();
    let requested_id = match command {
        CalendarCommand::Connect { ref client_id } => Some(client_id.as_str()),
        _ => None,
    };
    let reconnect = matches!(command, CalendarCommand::Reconnect);
    let authorize = requested_id.is_some() || reconnect;
    let id = requested_id.or(profile.as_deref());
    let Some(id) = id else {
        state.connection = CalendarConnection::Disconnected;
        state.clear_events();
        return match command {
            CalendarCommand::Inspect => Ok(()),
            CalendarCommand::Disconnect { local_only } => {
                state.disconnect = Some(CalendarDisconnectReport {
                    remote: if local_only {
                        CalendarRevocation::Skipped
                    } else {
                        CalendarRevocation::NoGrant
                    },
                    local: Some(Ok(())),
                });
                Ok(())
            }
            _ => Err(CalendarError::NotConfigured),
        };
    };
    let client = backend.client(id)?;
    let saved = backend.saved_refresh(&client)?;
    if authorize {
        if let Some(other) = profile.as_deref().filter(|other| *other != id)
            && backend.saved_refresh(&backend.client(other)?)?.is_some()
        {
            return Err(CalendarError::AlreadyConnected);
        }
        if saved.is_some()
            && !(reconnect && state.connection == CalendarConnection::ReconnectRequired)
        {
            return Err(CalendarError::AlreadyConnected);
        }
        gate.check()?;
        if profile.as_deref() != Some(id) {
            backend.save_profile(id)?;
            state.configured = true;
        }
        state.connection = if saved.is_some() {
            CalendarConnection::ReconnectRequired
        } else {
            CalendarConnection::Disconnected
        };
        state.clear_events();
        *session = None;
        let opened = backend.authorize(&client, gate)?;
        if let Err(error) = gate.commit() {
            let _ = backend.revoke(&client, opened.refresh_token());
            return Err(error);
        }
        if let Err(error) = backend.save_refresh(&client, opened.refresh_token()) {
            let _ = backend.revoke(&client, opened.refresh_token());
            return Err(error);
        }
        *session = Some(opened);
        state.connection = CalendarConnection::Connected;
        return fetch(backend, &client, session, None, gate, state);
    }
    match command {
        CalendarCommand::Inspect => {
            if state.connection != CalendarConnection::ReconnectRequired {
                state.connection = if saved.is_some() {
                    CalendarConnection::Connected
                } else {
                    state.clear_events();
                    *session = None;
                    CalendarConnection::Disconnected
                };
            }
        }
        CalendarCommand::Refresh => {
            if state.connection == CalendarConnection::ReconnectRequired {
                return Err(CalendarError::ReconnectRequired);
            }
            let saved = saved.as_ref().ok_or(CalendarError::NotConnected)?;
            state.connection = CalendarConnection::Connected;
            fetch(backend, &client, session, Some(saved), gate, state)?;
        }
        CalendarCommand::Disconnect { local_only } => {
            gate.commit()?;
            state.clear_events();
            *session = None;
            let remote = if local_only {
                CalendarRevocation::Skipped
            } else if let Some(token) = saved.as_ref() {
                match backend.revoke(&client, token) {
                    Ok(()) => CalendarRevocation::Revoked,
                    Err(error) => CalendarRevocation::Failed(error),
                }
            } else {
                CalendarRevocation::NoGrant
            };
            let local = (!matches!(remote, CalendarRevocation::Failed(_)))
                .then(|| backend.forget_refresh(&client));
            state.disconnect = Some(CalendarDisconnectReport { remote, local });
            state.connection = if local == Some(Ok(())) {
                CalendarConnection::Disconnected
            } else if remote == CalendarRevocation::Revoked {
                CalendarConnection::ReconnectRequired
            } else if saved.is_some() && state.connection != CalendarConnection::ReconnectRequired {
                CalendarConnection::Connected
            } else {
                state.connection
            };
        }
        CalendarCommand::Connect { .. } | CalendarCommand::Reconnect | CalendarCommand::Cancel => {
            unreachable!()
        }
    }
    Ok(())
}

fn fetch<B: Backend>(
    backend: &B,
    client: &B::Client,
    session: &mut Option<CalendarSession>,
    saved: Option<&CalendarRefreshToken>,
    gate: &Gate,
    state: &mut CalendarSnapshot,
) -> CalendarResult<()> {
    let now = SystemTime::now();
    let page = if let Some(active) = session.as_ref() {
        backend.upcoming(client, active, now, gate)
    } else {
        Err(CalendarError::RefreshRequired)
    };
    let page = match page {
        Err(CalendarError::RefreshRequired) => {
            let refresh = session
                .as_ref()
                .map(CalendarSession::refresh_token)
                .or(saved)
                .ok_or(CalendarError::NotConnected)?;
            let next = backend.refresh(client, refresh, gate)?;
            // A rotation still belongs to this device connection, even if a later fetch is cancelled.
            backend.save_refresh(client, next.refresh_token())?;
            *session = Some(next);
            gate.check()?;
            match backend.upcoming(client, session.as_ref().unwrap(), now, gate) {
                Err(CalendarError::RefreshRequired) => {
                    return Err(CalendarError::ReconnectRequired);
                }
                other => other?,
            }
        }
        other => other?,
    };
    gate.check()?;
    state.connection = CalendarConnection::Connected;
    state.upcoming = Some(Arc::new(page));
    state.refreshed_at = Some(SystemTime::now());
    state.stale = false;
    Ok(())
}
