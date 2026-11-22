//! Per-request macOS window selection. Native grants and buffers are never serialized.
#[cfg(test)]
use super::{MAX_PIXELS, MAX_SIDE};
#[cfg(any(target_os = "macos", test))]
use super::{capture_dimensions, valid_frame_dimensions};
use anyhow::Result;
#[cfg(any(target_os = "macos", test))]
use anyhow::{Context, ensure};
#[cfg(any(target_os = "macos", test))]
use std::sync::{
    Arc,
    atomic::Ordering,
    mpsc::{self, SyncSender},
};
use std::{
    fmt,
    marker::PhantomData,
    rc::Rc,
    sync::{atomic::AtomicBool, mpsc::Receiver},
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowIdentity {
    pub window_id: u32,
    pub owner_pid: i32,
}

/// A one-use selection. Its exact retained filter is private to this module.
pub struct SelectedWindow {
    job_id: String,
    window: WindowIdentity,
    #[cfg(target_os = "macos")]
    native: native::Selection,
}
impl SelectedWindow {
    pub fn job_id(&self) -> &str {
        &self.job_id
    }
    pub fn window(&self) -> WindowIdentity {
        self.window
    }
}
impl fmt::Debug for SelectedWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SelectedWindow")
            .field("job_id", &self.job_id)
            .field("window", &self.window)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum PickerEvent {
    Selected(SelectedWindow),
    Cancelled {
        job_id: String,
    },
    Failed {
        job_id: String,
        message: String,
    },
    /// Emitted only after the main-queue observer/deactivation cleanup barrier.
    Retired {
        job_id: String,
    },
}
impl PickerEvent {
    pub fn job_id(&self) -> &str {
        match self {
            Self::Selected(s) => s.job_id(),
            Self::Cancelled { job_id } | Self::Failed { job_id, .. } | Self::Retired { job_id } => {
                job_id
            }
        }
    }
}

/// Must stay on the main thread. Keep it until capture has stopped, then retire it.
/// The returned receiver has capacity two: one outcome and one retirement ack.
pub struct PickerSession {
    #[cfg(target_os = "macos")]
    native: native::Picker,
    _main_thread: PhantomData<Rc<()>>,
}
impl PickerSession {
    /// `on_ready` must be lightweight and nonblocking, such as request_repaint.
    /// Setup failure after ownership is acquired is an event on the returned
    /// session, so retirement can still be acknowledged. No chooser timer is
    /// spawned here: the caller must retire the ticket at its own deadline.
    pub fn present(
        job_id: String,
        on_ready: impl Fn() + Send + Sync + 'static,
    ) -> Result<(Self, Receiver<PickerEvent>)> {
        #[cfg(target_os = "macos")]
        {
            let (native, events) = native::Picker::present(job_id, Arc::new(on_ready))?;
            Ok((
                Self {
                    native,
                    _main_thread: PhantomData,
                },
                events,
            ))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (job_id, on_ready);
            anyhow::bail!("Single-window capture requires macOS 15.2 or later")
        }
    }
    /// Invalidates the selection before queued callbacks can be accepted.
    /// Do not release the caller's operation slot until Retired is received.
    pub fn retire(&mut self, job_id: &str) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            self.native.retire(job_id)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = job_id;
            anyhow::bail!("Single-window capture is unavailable")
        }
    }
    pub fn retirement_complete(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.native.retired.load(Ordering::Acquire)
        }
        #[cfg(not(target_os = "macos"))]
        {
            true
        }
    }
}

pub struct WindowCapture {
    /// The canonical capture UUID supplied to PickerSession::present.
    pub id: String,
    pub window: WindowIdentity,
    pub captured_at: i64,
    pub width: u32,
    pub height: u32,
    /// SHA-256 of PNG bytes, matching the existing display capture semantics.
    pub sha256: String,
    /// SHA-256 of tightly packed decoded RGBA bytes.
    pub pixel_sha256: String,
    pub png: Vec<u8>,
    pub text: String,
}

/// Process-local stage marks for a successful opt-in selected-window capture.
pub struct WindowCaptureTiming {
    pub frame_observed: Instant,
    pub stream_stopped: Instant,
    pub rgba_ready: Instant,
    pub ocr_started: Instant,
    pub ocr_finished: Instant,
}

/// Own this handle in the caller's retained screen slot. Dropping it is NOT a
/// native stop acknowledgement and deliberately cannot permit another capture.
pub struct CleanupUnconfirmed {
    job_id: String,
    #[cfg(target_os = "macos")]
    _stream: screencapturekit::stream::SCStream,
    #[cfg(target_os = "macos")]
    _slot: GateLease,
}
impl CleanupUnconfirmed {
    pub fn job_id(&self) -> &str {
        &self.job_id
    }
}
impl fmt::Debug for CleanupUnconfirmed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CleanupUnconfirmed")
            .field("job_id", &self.job_id)
            .finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub struct CaptureFailure {
    pub message: String,
    pub cleanup: Option<CleanupUnconfirmed>,
}
impl CaptureFailure {
    fn ordinary(error: impl fmt::Display) -> Self {
        Self {
            message: error.to_string(),
            cleanup: None,
        }
    }
}
impl fmt::Display for CaptureFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for CaptureFailure {}

pub fn is_supported() -> bool {
    #[cfg(target_os = "macos")]
    {
        native::supported()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Blocking worker operation, never called from UI drawing or a native callback.
/// Native start/stop wait at most 30s each; the first frame waits at most 5s.
/// Vision OCR remains a synchronous native operation; cancellation suppresses its
/// result and the caller must retain its owned worker until it returns.
pub fn capture(
    selection: SelectedWindow,
    cancel: &AtomicBool,
) -> std::result::Result<WindowCapture, CaptureFailure> {
    capture_with_timing(selection, cancel, false).map(|(frame, _)| frame)
}

/// Same capture contract with optional transient stage marks.
pub fn capture_with_timing(
    selection: SelectedWindow,
    cancel: &AtomicBool,
    measure: bool,
) -> std::result::Result<(WindowCapture, Option<WindowCaptureTiming>), CaptureFailure> {
    #[cfg(target_os = "macos")]
    {
        native::capture(selection, cancel, measure)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (selection, cancel, measure);
        Err(CaptureFailure::ordinary(
            "Single-window capture is unavailable",
        ))
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Geometry {
    width: u32,
    height: u32,
}
#[cfg(any(target_os = "macos", test))]
fn geometry(rect: (f64, f64, f64, f64), scale: f64) -> Result<Geometry> {
    let (x, y, w, h) = rect;
    ensure!(
        [x, y, w, h, scale].iter().all(|v| v.is_finite())
            && w > 0.0
            && h > 0.0
            && w <= 32768.0
            && h <= 32768.0
            && scale > 0.0
            && scale <= 8.0,
        "Selected window has invalid geometry"
    );
    let (width, height) = capture_dimensions(w * scale, h * scale)?;
    Ok(Geometry { width, height })
}
#[cfg(any(target_os = "macos", test))]
fn validate_selection(
    window_count: usize,
    display_count: usize,
    window_style: bool,
    window: WindowIdentity,
    own_pid: u32,
) -> Result<()> {
    ensure!(
        window_style && window_count == 1 && display_count == 0,
        "Choose exactly one window, not a display or application"
    );
    ensure!(
        window.window_id != 0 && window.owner_pid > 0 && window.owner_pid as u32 != own_pid,
        "Choose an available window owned by another application"
    );
    Ok(())
}
#[cfg(any(target_os = "macos", test))]
fn live(active: &AtomicBool, cancel: &AtomicBool) -> Result<()> {
    ensure!(
        active.load(Ordering::Acquire) && !cancel.load(Ordering::Acquire),
        "Window capture was cancelled"
    );
    Ok(())
}
#[cfg(any(target_os = "macos", test))]
struct GateLease {
    gate: Arc<AtomicBool>,
    release: bool,
}
#[cfg(any(target_os = "macos", test))]
impl GateLease {
    fn acquire(gate: &Arc<AtomicBool>) -> Result<Self> {
        ensure!(
            gate.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok(),
            "Window capture or cleanup still owns the slot"
        );
        Ok(Self {
            gate: gate.clone(),
            release: true,
        })
    }
    fn quarantine(mut self) -> Self {
        self.release = false;
        self
    }
}
#[cfg(any(target_os = "macos", test))]
impl Drop for GateLease {
    fn drop(&mut self) {
        if self.release {
            self.gate.store(false, Ordering::Release);
        }
    }
}
#[cfg(any(target_os = "macos", test))]
struct FirstSample<T> {
    claimed: AtomicBool,
    tx: SyncSender<T>,
}
#[cfg(any(target_os = "macos", test))]
impl<T> FirstSample<T> {
    fn offer(&self, sample: T) {
        if self
            .claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let _ = self.tx.try_send(sample);
        }
    }
}
#[cfg(any(target_os = "macos", test))]
fn cleanup_uncertain(start_timed_out: bool, stop_ok: bool) -> bool {
    start_timed_out || !stop_ok
}
#[cfg(any(target_os = "macos", test))]
fn encode_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Result<(Vec<u8>, String, String)> {
    use sha2::{Digest, Sha256};
    ensure!(
        valid_frame_dimensions(width as usize, height as usize),
        "Captured image exceeds bounds"
    );
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|v| v.checked_mul(4))
        .context("Captured image size overflow")?;
    ensure!(
        pixels.len() == expected,
        "Captured image has invalid pixels"
    );
    let pixel_hash = format!("{:x}", Sha256::digest(&pixels));
    let rgba =
        image::RgbaImage::from_raw(width, height, pixels).context("Invalid captured image")?;
    let mut png = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut png, image::ImageFormat::Png)?;
    let png = png.into_inner();
    ensure!(png.len() <= 32 * 1024 * 1024, "Captured PNG exceeds bounds");
    let hash = format!("{:x}", Sha256::digest(&png));
    Ok((png, hash, pixel_hash))
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use objc2::MainThreadMarker;
    use objc2_foundation::{NSOperatingSystemVersion, NSProcessInfo};
    use screencapturekit::{
        cm::{CMSampleBuffer, CMSampleBufferExt, CMSampleBufferSCExt, CMTime, SCFrameStatus},
        content_sharing_picker::{
            SCContentSharingPicker, SCContentSharingPickerConfiguration,
            SCContentSharingPickerMode, SCPickerEvent, SCPickerSubscription,
        },
        dispatch_queue::{DispatchQoS, DispatchQueue},
        screenshot_manager::CGImageExt,
        stream::{
            SCStream,
            configuration::{PixelFormat, SCStreamConfiguration},
            content_filter::{SCContentFilter, SCShareableContentStyle},
            delegate_trait::ErrorHandler,
            output_type::SCStreamOutputType,
        },
        utils::completion::{default_timeout, is_timeout_error},
    };
    use std::{
        ffi::c_void,
        sync::OnceLock,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };
    type Ready = Arc<dyn Fn() + Send + Sync>;
    static PICKER_GATE: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    static CAPTURE_GATE: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    fn gate(cell: &OnceLock<Arc<AtomicBool>>) -> &Arc<AtomicBool> {
        cell.get_or_init(|| Arc::new(AtomicBool::new(false)))
    }
    pub(super) fn supported() -> bool {
        NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(NSOperatingSystemVersion {
            majorVersion: 15,
            minorVersion: 2,
            patchVersion: 0,
        }) && SCContentSharingPicker::is_available()
    }
    fn finite_controls() -> Result<()> {
        ensure!(
            default_timeout().is_some_and(|d| !d.is_zero() && d <= Duration::from_secs(30)),
            "Window capture requires a finite native completion timeout of at most 30 seconds"
        );
        Ok(())
    }
    fn now() -> Result<i64> {
        Ok(i64::try_from(
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
        )?)
    }
    pub(super) struct Selection {
        filter: SCContentFilter,
        geometry: Geometry,
        selected_at: Instant,
        active: Arc<AtomicBool>,
    }
    pub(super) struct Picker {
        job_id: String,
        active: Arc<AtomicBool>,
        subscription: Option<SCPickerSubscription>,
        slot: Option<GateLease>,
        tx: SyncSender<PickerEvent>,
        ready: Ready,
        retirement_scheduled: bool,
        pub(super) retired: Arc<AtomicBool>,
    }
    impl Picker {
        pub(super) fn present(
            job_id: String,
            ready: Ready,
        ) -> Result<(Self, Receiver<PickerEvent>)> {
            ensure!(
                MainThreadMarker::new().is_some(),
                "Window picker must be presented on the main thread"
            );
            ensure!(
                supported(),
                "Single-window capture requires macOS 15.2 or later"
            );
            ensure!(
                uuid::Uuid::parse_str(&job_id).is_ok_and(|id| id.to_string() == job_id),
                "Invalid capture UUID"
            );
            finite_controls()?;
            ensure!(
                !gate(&CAPTURE_GATE).load(Ordering::Acquire),
                "Window capture or cleanup still owns the slot"
            );
            let slot = GateLease::acquire(gate(&PICKER_GATE))?;
            let (tx, rx) = mpsc::sync_channel(2);
            let active = Arc::new(AtomicBool::new(true));
            let mut owner = Self {
                job_id: job_id.clone(),
                active: active.clone(),
                subscription: None,
                slot: Some(slot),
                tx: tx.clone(),
                ready: ready.clone(),
                retirement_scheduled: false,
                retired: Arc::new(AtomicBool::new(false)),
            };
            let claimed = Arc::new(AtomicBool::new(false));
            let observer_claimed = claimed.clone();
            let setup = (|| -> Result<()> {
                let mut configuration = SCContentSharingPickerConfiguration::new()
                    .map_err(|_| anyhow::anyhow!("Window picker is unavailable"))?;
                configuration.set_allowed_picker_modes(&[SCContentSharingPickerMode::SingleWindow]);
                configuration.set_allows_changing_selected_content(false);
                SCContentSharingPicker::set_default_configuration(&configuration)
                    .map_err(|_| anyhow::anyhow!("Window picker configuration failed"))?;
                owner.subscription = Some(
                    SCContentSharingPicker::add_observer(move |event| {
                        if !active.load(Ordering::Acquire)
                            || observer_claimed
                                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                                .is_err()
                        {
                            return;
                        }
                        let outcome = match event {
                            SCPickerEvent::Updated { result, stream } => {
                                let selected = (|| -> Result<SelectedWindow> {
                                    ensure!(
                                        stream.is_none(),
                                        "Unexpected existing-stream picker update"
                                    );
                                    let windows = result.windows();
                                    let window = windows.first().context("No selected window")?;
                                    let identity = WindowIdentity {
                                        window_id: window.window_id(),
                                        owner_pid: window
                                            .owning_application()
                                            .map_or(0, |app| app.process_id()),
                                    };
                                    let filter = result.filter();
                                    validate_selection(
                                        windows.len(),
                                        result.displays().len(),
                                        filter.style().ok()
                                            == Some(SCShareableContentStyle::Window),
                                        identity,
                                        std::process::id(),
                                    )?;
                                    let geometry = geometry(result.rect(), result.scale())?;
                                    Ok(SelectedWindow {
                                        job_id: job_id.clone(),
                                        window: identity,
                                        native: Selection {
                                            filter,
                                            geometry,
                                            selected_at: Instant::now(),
                                            active: active.clone(),
                                        },
                                    })
                                })();
                                match selected {
                                    Ok(s) => PickerEvent::Selected(s),
                                    Err(e) => {
                                        active.store(false, Ordering::Release);
                                        PickerEvent::Failed {
                                            job_id: job_id.clone(),
                                            message: e.to_string(),
                                        }
                                    }
                                }
                            }
                            SCPickerEvent::Cancelled { .. } => {
                                active.store(false, Ordering::Release);
                                PickerEvent::Cancelled {
                                    job_id: job_id.clone(),
                                }
                            }
                            SCPickerEvent::Failed(_) => {
                                active.store(false, Ordering::Release);
                                PickerEvent::Failed {
                                    job_id: job_id.clone(),
                                    message: "System window picker failed".into(),
                                }
                            }
                        };
                        let _ = tx.try_send(outcome);
                        ready();
                    })
                    .map_err(|_| anyhow::anyhow!("Could not register window picker observer"))?,
                );
                SCContentSharingPicker::present_using_style(SCShareableContentStyle::Window)
                    .map_err(|_| anyhow::anyhow!("Could not present window picker"))?;
                Ok(())
            })();
            if let Err(error) = setup {
                setup_failure(
                    &owner.active,
                    &claimed,
                    &owner.tx,
                    &owner.ready,
                    &owner.job_id,
                    error.to_string(),
                );
            }
            Ok((owner, rx))
        }
        pub(super) fn retire(&mut self, job_id: &str) -> Result<()> {
            ensure!(
                MainThreadMarker::new().is_some(),
                "Window picker must be retired on the main thread"
            );
            ensure!(job_id == self.job_id, "Stale window-picker retirement");
            if self.retirement_scheduled {
                return Ok(());
            }
            self.retirement_scheduled = true;
            self.active.store(false, Ordering::Release);
            drop(self.subscription.take());
            let inactive = SCContentSharingPicker::set_active(false);
            SCContentSharingPicker::deactivate();
            let ack = Box::new(Retirement {
                job_id: self.job_id.clone(),
                retired: self.retired.clone(),
                tx: self.tx.clone(),
                ready: self.ready.clone(),
                _slot: self.slot.take(),
            });
            // SAFETY: the main queue consumes this Box exactly once; the trampoline
            // catches unwinding. FIFO places it after bridge removal/deactivation.
            unsafe {
                DispatchQueue::main().exec_async_f(Box::into_raw(ack).cast(), retired);
            }
            inactive.map_err(|_| anyhow::anyhow!("Could not deactivate window picker"))
        }
    }
    impl Drop for Picker {
        fn drop(&mut self) {
            let _ = self.retire(&self.job_id.clone());
        }
    }
    fn setup_failure(
        active: &AtomicBool,
        claimed: &AtomicBool,
        tx: &SyncSender<PickerEvent>,
        ready: &Ready,
        job_id: &str,
        message: String,
    ) -> bool {
        active.store(false, Ordering::Release);
        if claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        let _ = tx.try_send(PickerEvent::Failed {
            job_id: job_id.into(),
            message,
        });
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ready()));
        true
    }

    struct Retirement {
        job_id: String,
        retired: Arc<AtomicBool>,
        tx: SyncSender<PickerEvent>,
        ready: Ready,
        _slot: Option<GateLease>,
    }
    unsafe extern "C" fn retired(context: *mut c_void) {
        // SAFETY: context was transferred from Box::into_raw once above.
        let ack = unsafe { Box::from_raw(context.cast::<Retirement>()) };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ack.retired.store(true, Ordering::Release);
            let Retirement {
                job_id,
                tx,
                ready,
                _slot,
                ..
            } = *ack;
            drop(_slot);
            let _ = tx.try_send(PickerEvent::Retired { job_id });
            ready();
        }));
    }
    struct Frame {
        sample: CMSampleBuffer,
        observed: Instant,
        captured_at: i64,
    }
    pub(super) fn capture(
        selection: SelectedWindow,
        cancel: &AtomicBool,
        measure: bool,
    ) -> std::result::Result<(WindowCapture, Option<WindowCaptureTiming>), CaptureFailure> {
        let operation = || -> Result<_> {
            ensure!(
                MainThreadMarker::new().is_none(),
                "Window capture must run on an owned background worker"
            );
            finite_controls()?;
            live(&selection.native.active, cancel)?;
            GateLease::acquire(gate(&CAPTURE_GATE))
        };
        let slot = operation().map_err(CaptureFailure::ordinary)?;
        let dims = selection.native.geometry;
        let config = SCStreamConfiguration::new()
            .with_width(dims.width)
            .with_height(dims.height)
            .with_pixel_format(PixelFormat::BGRA)
            .with_queue_depth(3)
            .with_minimum_frame_interval(&CMTime::new(1, 2))
            .with_shows_cursor(false)
            .with_captures_audio(false)
            .with_captures_microphone(false)
            .map_err(|_| CaptureFailure::ordinary("Could not disable microphone capture"))?;
        let source_failed = Arc::new(AtomicBool::new(false));
        let failed = source_failed.clone();
        let mut stream = SCStream::new_with_delegate(
            &selection.native.filter,
            &config,
            ErrorHandler::new(move |_| {
                failed.store(true, Ordering::Release);
            }),
        )
        .map_err(|_| CaptureFailure::ordinary("Could not create selected-window stream"))?;
        let (tx, rx) = mpsc::sync_channel(1);
        let first = FirstSample {
            claimed: AtomicBool::new(false),
            tx,
        };
        let queue = DispatchQueue::new("org.cueglint.scoped-screen", DispatchQoS::UserInitiated);
        stream
            .add_output_handler_with_queue(
                move |sample: CMSampleBuffer, kind| {
                    if kind == SCStreamOutputType::Screen
                        && sample.is_valid()
                        && sample.is_data_ready()
                        && sample.frame_status() == Some(SCFrameStatus::Complete)
                    {
                        if let Ok(captured_at) = now() {
                            first.offer(Frame {
                                sample,
                                observed: Instant::now(),
                                captured_at,
                            });
                        }
                    }
                },
                SCStreamOutputType::Screen,
                Some(&queue),
            )
            .map_err(|_| CaptureFailure::ordinary("Could not register selected-window output"))?;
        // A late cancellation must not start a stream after it has been retired.
        live(&selection.native.active, cancel).map_err(CaptureFailure::ordinary)?;
        let start = stream.start_capture();
        let start_timeout = matches!(&start, Err(screencapturekit::error::SCError::CaptureStartFailed(message)) if is_timeout_error(message));
        let frame = if start.is_ok() {
            let deadline = Instant::now() + Duration::from_secs(5);
            (|| -> Result<Frame> {
                loop {
                    live(&selection.native.active, cancel)?;
                    ensure!(
                        !source_failed.load(Ordering::Acquire),
                        "Selected-window source stopped"
                    );
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    ensure!(
                        !remaining.is_zero(),
                        "Selected window produced no complete frame before the deadline"
                    );
                    match rx.recv_timeout(remaining.min(Duration::from_millis(50))) {
                        Ok(frame) => return Ok(frame),
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            anyhow::bail!("Selected-window frame delivery stopped")
                        }
                    }
                }
            })()
        } else {
            Err(anyhow::anyhow!("Selected-window stream start failed"))
        };
        // Never convert/OCR a buffer until native shutdown is acknowledged.
        let stop = stream.stop_capture();
        let stop_at = measure.then(Instant::now);
        if cleanup_uncertain(start_timeout, stop.is_ok()) {
            return Err(CaptureFailure { message: "Selected-window native cleanup is unconfirmed; the screen slot remains occupied".into(), cleanup: Some(CleanupUnconfirmed { job_id: selection.job_id, _stream: stream, _slot: slot.quarantine() }) });
        }
        drop(stream);
        let finish = || -> Result<_> {
            let frame = frame?;
            live(&selection.native.active, cancel)?;
            ensure!(
                !source_failed.load(Ordering::Acquire),
                "Selected-window source stopped"
            );
            ensure!(
                frame.observed >= selection.native.selected_at,
                "Stale selected-window frame"
            );
            let buffer = frame
                .sample
                .pixel_buffer()
                .context("Selected-window frame has no pixels")?;
            let (width, height, stride) = (buffer.width(), buffer.height(), buffer.bytes_per_row());
            ensure!(
                valid_frame_dimensions(width, height),
                "Selected-window pixel dimensions exceed bounds"
            );
            ensure!(
                buffer.pixel_format() == u32::from_be_bytes(*b"BGRA")
                    && stride >= width.checked_mul(4).context("Pixel stride overflow")?
                    && stride
                        .checked_mul(height)
                        .is_some_and(|n| n <= 32 * 1024 * 1024),
                "Selected-window pixel layout exceeds bounds"
            );
            let image = frame
                .sample
                .cg_image()
                .map_err(|_| anyhow::anyhow!("Could not convert selected-window pixels"))?;
            ensure!(
                image.width() == width && image.height() == height,
                "Selected-window image geometry changed"
            );
            let pixels = image
                .rgba_data()
                .map_err(|_| anyhow::anyhow!("Could not read selected-window pixels"))?;
            live(&selection.native.active, cancel)?;
            let rgba_at = measure.then(Instant::now);
            let (png, sha256, pixel_sha256) = encode_rgba(width as u32, height as u32, pixels)?;
            let ocr_at = measure.then(Instant::now);
            let text = super::super::recognize_png(&png)?;
            let timing = stop_at.zip(rgba_at).zip(ocr_at).map(
                |((stream_stopped, rgba_ready), ocr_started)| WindowCaptureTiming {
                    frame_observed: frame.observed,
                    stream_stopped,
                    rgba_ready,
                    ocr_started,
                    ocr_finished: Instant::now(),
                },
            );
            live(&selection.native.active, cancel)?;
            let frame = WindowCapture {
                id: selection.job_id,
                window: selection.window,
                captured_at: frame.captured_at,
                width: width as u32,
                height: height as u32,
                sha256,
                pixel_sha256,
                png,
                text,
            };
            Ok((frame, timing))
        };
        let result = finish().map_err(CaptureFailure::ordinary);
        drop(slot);
        result
    }

    #[cfg(test)]
    mod callback_tests {
        use super::*;
        use std::sync::atomic::AtomicUsize;

        #[test]
        fn scoped_retirement_ack_follows_outcome_and_releases_its_owned_slot() {
            let gate = Arc::new(AtomicBool::new(false));
            let slot = GateLease::acquire(&gate).unwrap();
            let done = Arc::new(AtomicBool::new(false));
            let wakes = Arc::new(AtomicUsize::new(0));
            let wake = wakes.clone();
            let (tx, rx) = mpsc::sync_channel(2);
            tx.try_send(PickerEvent::Cancelled {
                job_id: "original-job".into(),
            })
            .unwrap();
            let ack = Box::new(Retirement {
                job_id: "original-job".into(),
                retired: done.clone(),
                tx,
                ready: Arc::new(move || {
                    wake.fetch_add(1, Ordering::Relaxed);
                }),
                _slot: Some(slot),
            });
            assert!(gate.load(Ordering::Acquire));
            assert!(!done.load(Ordering::Acquire));
            // Invoke the exact owned callback without a native queue or permission.
            unsafe {
                retired(Box::into_raw(ack).cast());
            }
            assert!(matches!(
                rx.try_recv().unwrap(),
                PickerEvent::Cancelled { .. }
            ));
            assert!(
                matches!(rx.try_recv().unwrap(), PickerEvent::Retired { job_id } if job_id == "original-job")
            );
            assert!(!gate.load(Ordering::Acquire));
            assert!(done.load(Ordering::Acquire));
            assert_eq!(wakes.load(Ordering::Relaxed), 1);
        }

        #[test]
        fn scoped_owned_setup_failure_keeps_room_for_retirement_ack() {
            let active = AtomicBool::new(true);
            let claimed = AtomicBool::new(false);
            let (tx, rx) = mpsc::sync_channel(2);
            let ready: Ready = Arc::new(|| {});
            assert!(setup_failure(
                &active,
                &claimed,
                &tx,
                &ready,
                "original-job",
                "original setup failure".into()
            ));
            assert!(!setup_failure(
                &active,
                &claimed,
                &tx,
                &ready,
                "original-job",
                "duplicate failure".into()
            ));
            assert!(!active.load(Ordering::Acquire));
            let done = Arc::new(AtomicBool::new(false));
            let ack = Box::new(Retirement {
                job_id: "original-job".into(),
                retired: done.clone(),
                tx,
                ready,
                _slot: None,
            });
            unsafe {
                retired(Box::into_raw(ack).cast());
            }
            assert!(
                matches!(rx.try_recv().unwrap(), PickerEvent::Failed { message, .. } if message == "original setup failure")
            );
            assert!(matches!(
                rx.try_recv().unwrap(),
                PickerEvent::Retired { .. }
            ));
            assert!(rx.try_recv().is_err());
            assert!(done.load(Ordering::Acquire));
        }

        #[test]
        fn scoped_retirement_ready_panic_cannot_unwind_through_native_callback() {
            let gate = Arc::new(AtomicBool::new(false));
            let slot = GateLease::acquire(&gate).unwrap();
            let done = Arc::new(AtomicBool::new(false));
            let (tx, rx) = mpsc::sync_channel(2);
            let ack = Box::new(Retirement {
                job_id: "original-job".into(),
                retired: done.clone(),
                tx,
                ready: Arc::new(|| panic!("original injected wake failure")),
                _slot: Some(slot),
            });
            unsafe {
                retired(Box::into_raw(ack).cast());
            }
            assert!(matches!(
                rx.try_recv().unwrap(),
                PickerEvent::Retired { .. }
            ));
            assert!(done.load(Ordering::Acquire));
            assert!(!gate.load(Ordering::Acquire));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_geometry_preserves_aspect_and_limits_both_axes_and_area() {
        assert_eq!(
            geometry((0.0, 0.0, 269.0, 144.0), 2.0).unwrap(),
            Geometry {
                width: 538,
                height: 288
            }
        );
        for (w, h) in [(8000.0, 1000.0), (1000.0, 8000.0), (8000.0, 8000.0)] {
            let g = geometry((-40.0, 0.0, w, h), 2.0).unwrap();
            assert!(g.width <= MAX_SIDE && g.height <= MAX_SIDE);
            assert!(u64::from(g.width) * u64::from(g.height) <= MAX_PIXELS);
            assert!((g.width as f64 / g.height as f64 - w / h).abs() < 0.01);
        }
    }
    #[test]
    fn scoped_geometry_rejects_nonfinite_zero_and_oversized_source_before_casts() {
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, 32769.0] {
            assert!(geometry((0.0, 0.0, bad, 10.0), 2.0).is_err());
        }
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, 8.1] {
            assert!(geometry((0.0, 0.0, 10.0, 10.0), bad).is_err());
        }
    }
    #[test]
    fn scoped_selection_requires_one_foreign_window_and_no_display() {
        let w = WindowIdentity {
            window_id: 42,
            owner_pid: 71,
        };
        assert!(validate_selection(1, 0, true, w, 99).is_ok());
        for args in [(0, 0, true), (2, 0, true), (1, 1, true), (1, 0, false)] {
            assert!(validate_selection(args.0, args.1, args.2, w, 99).is_err());
        }
        for bad in [
            WindowIdentity { window_id: 0, ..w },
            WindowIdentity { owner_pid: 0, ..w },
            WindowIdentity { owner_pid: 99, ..w },
        ] {
            assert!(validate_selection(1, 0, true, bad, 99).is_err());
        }
    }
    #[test]
    fn scoped_first_sample_is_single_and_nonblocking_under_competing_callbacks() {
        let (tx, rx) = mpsc::sync_channel(1);
        let first = Arc::new(FirstSample {
            claimed: AtomicBool::new(false),
            tx,
        });
        std::thread::scope(|s| {
            for i in 0..16 {
                let f = first.clone();
                s.spawn(move || f.offer(i));
            }
        });
        assert!((0..16).contains(&rx.try_recv().unwrap()));
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn scoped_cancel_or_retirement_invalidates_an_existing_selection() {
        let active = AtomicBool::new(true);
        let cancel = AtomicBool::new(false);
        assert!(live(&active, &cancel).is_ok());
        cancel.store(true, Ordering::Release);
        assert!(live(&active, &cancel).is_err());
        cancel.store(false, Ordering::Release);
        active.store(false, Ordering::Release);
        assert!(live(&active, &cancel).is_err());
    }
    #[test]
    fn scoped_unconfirmed_start_or_failed_stop_cannot_release_ownership() {
        assert!(!cleanup_uncertain(false, true));
        assert!(cleanup_uncertain(true, true));
        assert!(cleanup_uncertain(false, false));
        assert!(cleanup_uncertain(true, false));
        let gate = Arc::new(AtomicBool::new(false));
        let owner = GateLease::acquire(&gate).unwrap();
        assert!(GateLease::acquire(&gate).is_err());
        drop(owner.quarantine());
        assert!(gate.load(Ordering::Acquire));
        assert!(GateLease::acquire(&gate).is_err());
    }
    #[test]
    fn scoped_confirmed_cleanup_releases_exactly_one_slot() {
        let gate = Arc::new(AtomicBool::new(false));
        drop(GateLease::acquire(&gate).unwrap());
        assert!(!gate.load(Ordering::Acquire));
        assert!(GateLease::acquire(&gate).is_ok());
    }
    #[test]
    fn scoped_original_rgba_round_trips_with_distinct_png_and_pixel_hashes() {
        use sha2::{Digest, Sha256};
        let original = vec![10, 20, 30, 255, 210, 180, 90, 255];
        let (png, hash, pixels) = encode_rgba(2, 1, original.clone()).unwrap();
        assert_eq!(
            image::load_from_memory(&png)
                .unwrap()
                .into_rgba8()
                .into_raw(),
            original
        );
        assert_eq!(pixels, format!("{:x}", Sha256::digest(&original)));
        assert_eq!(hash, format!("{:x}", Sha256::digest(&png)));
        assert_ne!(hash, pixels);
        assert!(encode_rgba(0, 1, vec![]).is_err());
        assert!(encode_rgba(2, 1, vec![0; 7]).is_err());
    }
}
