//! Linux playback capture through the local PulseAudio protocol (also served by pipewire-pulse).
use super::{AudioPacket, PACKET_FRAMES, SAMPLE_RATE, Source};
use anyhow::{Context, Result, bail, ensure};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    os::unix::ffi::OsStrExt,
    path::Path,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

const POLL: Duration = Duration::from_millis(8);
const START_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_BYTES: u32 = SAMPLE_RATE * 2; // One second of server-side mono PCM16.
const CONTEXT_READY: c_int = 4;
const STREAM_READY: c_int = 2;
const NO_AUTOSPAWN: u32 = 1;
const INTERPOLATE_TIMING: u32 = 2;
const AUTO_TIMING_UPDATE: u32 = 8;
const DONT_MOVE: u32 = 0x0200;
const ADJUST_LATENCY: u32 = 0x2000;

#[repr(C)]
struct SampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}

#[repr(C)]
struct BufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

#[repr(C)]
struct ChannelMap {
    channels: u8,
    positions: [c_int; 32],
}
#[repr(C)]
struct Volume {
    channels: u8,
    values: [u32; 32],
}
// Prefix of pa_sink_info through monitor_source_name, using PA_CHANNELS_MAX from sample.h.
#[repr(C)]
struct SinkInfo {
    name: *const c_char,
    index: u32,
    description: *const c_char,
    spec: SampleSpec,
    map: ChannelMap,
    owner_module: u32,
    volume: Volume,
    mute: c_int,
    monitor_source: u32,
    monitor_source_name: *const c_char,
}

// Resolve the versioned OS ABI only when recording is requested. Every function
// pointer stays owned by the same table as the library that supplies its code.
macro_rules! pulse_api {
    ($(fn $name:ident($($arg:ident: $ty:ty),* $(,)?) $(-> $ret:ty)?;)*) => {
        struct Pulse {
            $($name: unsafe extern "C" fn($($ty),*) $(-> $ret)?,)*
            _library: libloading::Library,
        }
        impl Pulse {
            fn load(name: &str) -> Result<Self> {
                // Callers use a constant OS library name or an original test library.
                let library = unsafe { libloading::Library::new(name) }
                    .context("loading the Linux playback monitor library (libpulse.so.0)")?;
                Self::from_library(library)
            }

            fn from_library(library: libloading::Library) -> Result<Self> {
                Ok(Self {
                    $($name: unsafe { *library.get(concat!(stringify!($name), "\0").as_bytes())
                        .with_context(|| concat!("missing PulseAudio symbol ", stringify!($name)))? },)*
                    _library: library,
                })
            }
        }
    }
}
pulse_api! {
    fn pa_mainloop_new() -> *mut c_void;
    fn pa_mainloop_free(p: *mut c_void);
    fn pa_mainloop_get_api(p: *mut c_void) -> *mut c_void;
    fn pa_mainloop_iterate(p: *mut c_void, block: c_int, retval: *mut c_int) -> c_int;
    fn pa_context_new(api: *mut c_void, name: *const c_char) -> *mut c_void;
    fn pa_context_connect(
        p: *mut c_void,
        server: *const c_char,
        flags: u32,
        api: *const c_void,
    ) -> c_int;
    fn pa_context_get_sink_info_by_name(
        p: *mut c_void,
        name: *const c_char,
        cb: unsafe extern "C" fn(*mut c_void, *const SinkInfo, c_int, *mut c_void),
        data: *mut c_void,
    ) -> *mut c_void;
    fn pa_operation_get_state(op: *const c_void) -> c_int;
    fn pa_operation_cancel(op: *mut c_void);
    fn pa_operation_unref(op: *mut c_void);
    fn pa_stream_get_device_index(s: *const c_void) -> u32;
    fn pa_context_get_state(p: *const c_void) -> c_int;
    fn pa_context_disconnect(p: *mut c_void);
    fn pa_context_unref(p: *mut c_void);
    fn pa_context_errno(p: *const c_void) -> c_int;
    fn pa_strerror(error: c_int) -> *const c_char;
    fn pa_stream_new(
        p: *mut c_void,
        name: *const c_char,
        spec: *const SampleSpec,
        map: *const c_void,
    ) -> *mut c_void;
    fn pa_stream_connect_record(
        s: *mut c_void,
        device: *const c_char,
        attr: *const BufferAttr,
        flags: u32,
    ) -> c_int;
    fn pa_stream_get_state(s: *const c_void) -> c_int;
    fn pa_stream_get_latency(s: *mut c_void, usec: *mut u64, negative: *mut c_int) -> c_int;
    fn pa_stream_cork(
        s: *mut c_void,
        cork: c_int,
        cb: unsafe extern "C" fn(*mut c_void, c_int, *mut c_void),
        data: *mut c_void,
    ) -> *mut c_void;
    fn pa_stream_readable_size(s: *const c_void) -> usize;
    fn pa_stream_peek(s: *mut c_void, data: *mut *const c_void, bytes: *mut usize) -> c_int;
    fn pa_stream_drop(s: *mut c_void) -> c_int;
    fn pa_stream_disconnect(s: *mut c_void) -> c_int;
    fn pa_stream_unref(s: *mut c_void);
}

fn server() -> Result<CString> {
    let address = if let Some(explicit) = std::env::var_os("PULSE_SERVER") {
        explicit
            .as_bytes()
            .strip_prefix(b"unix:")
            .unwrap_or(explicit.as_bytes())
            .to_vec()
    } else {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .context("Linux system audio needs a local PulseAudio or pipewire-pulse session")?;
        Path::new(&runtime)
            .join("pulse/native")
            .as_os_str()
            .as_bytes()
            .to_vec()
    };
    ensure!(
        address.starts_with(b"/") && !address.iter().any(|c| c.is_ascii_whitespace()),
        "Linux system audio requires one local PulseAudio UNIX socket"
    );
    let mut name = b"unix:".to_vec();
    name.extend_from_slice(&address);
    CString::new(name).context("invalid local PulseAudio socket path")
}

struct Connection {
    api: Pulse,
    mainloop: *mut c_void,
    context: *mut c_void,
    stream: *mut c_void,
    monitor_index: u32,
}

impl Connection {
    fn open(server: &CStr) -> Result<Self> {
        // Each successful allocation is retained immediately, including on early error.
        let api = Pulse::load("libpulse.so.0")?;
        let mut conn = Self {
            mainloop: unsafe { (api.pa_mainloop_new)() },
            api,
            context: ptr::null_mut(),
            stream: ptr::null_mut(),
            monitor_index: u32::MAX,
        };
        ensure!(
            !conn.mainloop.is_null(),
            "could not allocate PulseAudio mainloop"
        );
        let deadline = Instant::now() + START_TIMEOUT;
        unsafe {
            conn.context = (conn.api.pa_context_new)(
                (conn.api.pa_mainloop_get_api)(conn.mainloop),
                c"Cueglint".as_ptr(),
            );
            ensure!(
                !conn.context.is_null(),
                "could not allocate PulseAudio context"
            );
            conn.check((conn.api.pa_context_connect)(
                conn.context,
                server.as_ptr(),
                NO_AUTOSPAWN,
                ptr::null(),
            ))?;
        }
        conn.await_ready(deadline, CONTEXT_READY, || unsafe {
            (conn.api.pa_context_get_state)(conn.context)
        })?;
        let (monitor_index, monitor_name) = conn.default_monitor(deadline)?;
        conn.monitor_index = monitor_index;
        let spec = SampleSpec {
            format: 3,
            rate: SAMPLE_RATE,
            channels: 1,
        }; // PA_SAMPLE_S16LE
        let attr = BufferAttr {
            maxlength: MAX_BYTES,
            tlength: u32::MAX,
            prebuf: u32::MAX,
            minreq: u32::MAX,
            fragsize: 640,
        }; // 20 ms
        unsafe {
            conn.stream = (conn.api.pa_stream_new)(
                conn.context,
                c"Playback monitor".as_ptr(),
                &spec,
                ptr::null(),
            );
            ensure!(
                !conn.stream.is_null(),
                "could not allocate PulseAudio monitor stream"
            );
            // Pin the sink's reported monitor and disallow stream migration.
            conn.check((conn.api.pa_stream_connect_record)(
                conn.stream,
                monitor_name.as_ptr(),
                &attr,
                DONT_MOVE | AUTO_TIMING_UPDATE | INTERPOLATE_TIMING | ADJUST_LATENCY,
            ))?;
        }
        conn.await_ready(deadline, STREAM_READY, || unsafe {
            (conn.api.pa_stream_get_state)(conn.stream)
        })?;
        conn.check_route()?;
        Ok(conn)
    }

    fn default_monitor(&self, deadline: Instant) -> Result<(u32, CString)> {
        // This non-threaded mainloop invokes the callback only inside iterate(). Cancel before
        // dropping its stack data even when the server disappears or times out.
        unsafe extern "C" fn on_sink(
            _: *mut c_void,
            info: *const SinkInfo,
            end: c_int,
            data: *mut c_void,
        ) {
            if end == 0 && !info.is_null() {
                let info = unsafe { &*info };
                if info.monitor_source != u32::MAX && !info.monitor_source_name.is_null() {
                    let target = unsafe { &mut *data.cast::<Option<(u32, CString)>>() };
                    *target = Some((
                        info.monitor_source,
                        unsafe { CStr::from_ptr(info.monitor_source_name) }.to_owned(),
                    ));
                }
            }
        }
        let mut monitor: Option<(u32, CString)> = None;
        let op = unsafe {
            (self.api.pa_context_get_sink_info_by_name)(
                self.context,
                c"@DEFAULT_SINK@".as_ptr(),
                on_sink,
                ptr::from_mut(&mut monitor).cast(),
            )
        };
        ensure!(!op.is_null(), "could not request the default playback sink");
        self.wait_operation(op, deadline)?;
        monitor.context("default playback sink has no available monitor")
    }

    fn wait_operation(&self, op: *mut c_void, deadline: Instant) -> Result<()> {
        let waited = (|| -> Result<()> {
            while unsafe { (self.api.pa_operation_get_state)(op) } == 0 {
                ensure!(Instant::now() < deadline, "PulseAudio operation timed out");
                self.iterate()?;
                thread::sleep(POLL);
            }
            Ok(())
        })();
        unsafe {
            (self.api.pa_operation_cancel)(op);
            (self.api.pa_operation_unref)(op);
        }
        waited
    }

    fn finish(&self, audio: &mut Packets) -> Result<()> {
        unsafe extern "C" fn stopped(_: *mut c_void, success: c_int, data: *mut c_void) {
            unsafe {
                *data.cast::<bool>() = success != 0;
            }
        }
        let mut success = false;
        let deadline = Instant::now() + Duration::from_millis(250);
        // The cork acknowledgement delimits this recording. Data that preceded the
        // acknowledgement belongs to this session, even if the worker hasn't read it yet.
        let op = unsafe {
            (self.api.pa_stream_cork)(self.stream, 1, stopped, ptr::from_mut(&mut success).cast())
        };
        ensure!(!op.is_null(), "could not stop playback monitor");
        self.wait_operation(op, deadline)?;
        ensure!(success, "server refused to stop playback monitor");
        while unsafe { (self.api.pa_stream_readable_size)(self.stream) } != 0 {
            ensure!(
                Instant::now() < deadline,
                "timed out draining the final playback samples"
            );
            self.read(audio)?;
        }
        Ok(())
    }

    fn check_route(&self) -> Result<()> {
        ensure!(
            unsafe { (self.api.pa_stream_get_device_index)(self.stream) } == self.monitor_index,
            "playback monitor changed during capture"
        );
        Ok(())
    }

    fn check(&self, result: c_int) -> Result<()> {
        if result < 0 {
            let message =
                unsafe { (self.api.pa_strerror)((self.api.pa_context_errno)(self.context)) };
            if !message.is_null() {
                bail!(
                    "PulseAudio playback monitor: {}",
                    unsafe { CStr::from_ptr(message) }.to_string_lossy()
                );
            }
            bail!("PulseAudio playback monitor failed");
        }
        Ok(())
    }

    fn iterate(&self) -> Result<()> {
        self.check(unsafe { (self.api.pa_mainloop_iterate)(self.mainloop, 0, ptr::null_mut()) })
    }

    fn await_ready(
        &self,
        deadline: Instant,
        ready: c_int,
        state: impl Fn() -> c_int,
    ) -> Result<()> {
        loop {
            self.iterate()?;
            let state = state();
            if state == ready {
                return Ok(());
            }
            if state > ready {
                return self.check(-1);
            }
            ensure!(
                Instant::now() < deadline,
                "timed out connecting to the local playback monitor"
            );
            thread::sleep(POLL);
        }
    }

    fn read(&self, audio: &mut Packets) -> Result<()> {
        self.iterate()?;
        ensure!(
            unsafe { (self.api.pa_stream_get_state)(self.stream) } == STREAM_READY,
            "playback monitor disconnected"
        );
        self.check_route()?;
        for _ in 0..16 {
            let (mut data, mut bytes) = (ptr::null(), 0);
            self.check(unsafe { (self.api.pa_stream_peek)(self.stream, &mut data, &mut bytes) })?;
            if bytes == 0 {
                break;
            }
            ensure!(
                bytes <= MAX_BYTES as usize && bytes % 2 == 0,
                "invalid playback fragment length"
            );
            let (mut latency_us, mut negative) = (0, 0);
            let latency = unsafe {
                (self.api.pa_stream_get_latency)(self.stream, &mut latency_us, &mut negative)
            };
            let clock = if latency >= 0 {
                Some((latency_us, negative != 0))
            } else {
                if unsafe { (self.api.pa_context_errno)(self.context) } != 16 {
                    // PA_ERR_NODATA
                    self.check(latency)?;
                }
                None
            };
            let buffered = unsafe { (self.api.pa_stream_readable_size)(self.stream) };
            ensure!(
                buffered >= bytes && buffered <= MAX_BYTES as usize,
                "invalid playback buffer size"
            );
            let (start, end) = fragment_times(
                u64::try_from(audio.origin.elapsed().as_nanos())?,
                audio.next_ns,
                bytes,
                buffered,
                clock,
            )?;
            audio.next_ns = end;
            if data.is_null() {
                // A PulseAudio hole has duration but no captured samples. Keep the gap visible.
                let lost = (bytes / 2).div_ceil(PACKET_FRAMES) as u64;
                audio.sequence += lost;
                audio.dropped.fetch_add(lost, Ordering::Relaxed);
            } else {
                // Pulse owns this exact readable slice until pa_stream_drop below.
                let pcm = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), bytes) };
                audio.push(pcm, start);
            }
            self.check(unsafe { (self.api.pa_stream_drop)(self.stream) })?;
        }
        Ok(())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            if !self.stream.is_null() {
                (self.api.pa_stream_disconnect)(self.stream);
                (self.api.pa_stream_unref)(self.stream);
            }
            if !self.context.is_null() {
                (self.api.pa_context_disconnect)(self.context);
                (self.api.pa_context_unref)(self.context);
            }
            if !self.mainloop.is_null() {
                (self.api.pa_mainloop_free)(self.mainloop);
            }
        }
    }
}

fn fragment_times(
    now_ns: u64,
    previous_end_ns: u64,
    bytes: usize,
    buffered: usize,
    latency_us: Option<(u64, bool)>,
) -> Result<(u64, u64)> {
    let ns = |bytes: usize| bytes as i128 * 1_000_000_000 / (2 * i128::from(SAMPLE_RATE));
    // A negative record latency means the monitor data precedes playback. When the server
    // has no clock yet, backdate the whole readable queue so later fragments retain spacing.
    let lag_ns = match latency_us {
        Some((us, true)) => -i128::from(us) * 1000,
        Some((us, false)) => i128::from(us) * 1000,
        None => ns(buffered),
    };
    let start = (i128::from(now_ns) - lag_ns).max(i128::from(previous_end_ns));
    Ok((u64::try_from(start)?, u64::try_from(start + ns(bytes))?))
}

struct Packets {
    origin: Instant,
    sequence: u64,
    next_ns: u64,
    tx: SyncSender<AudioPacket>,
    dropped: Arc<AtomicU64>,
    muted: Arc<AtomicBool>,
}

impl Packets {
    fn push(&mut self, pcm: &[u8], start_ns: u64) {
        for (i, bytes) in pcm.chunks(PACKET_FRAMES * 2).enumerate() {
            let sequence = self.sequence;
            self.sequence += 1;
            if self.muted.load(Ordering::Relaxed) {
                continue;
            }
            let mut packet = AudioPacket {
                source: Source::SystemAudio,
                sequence,
                start_ns: start_ns
                    + (i * PACKET_FRAMES) as u64 * 1_000_000_000 / u64::from(SAMPLE_RATE),
                sample_rate: SAMPLE_RATE,
                len: bytes.len() / 2,
                samples: [0.0; PACKET_FRAMES],
            };
            for (out, sample) in packet.samples.iter_mut().zip(bytes.chunks_exact(2)) {
                *out = f32::from(i16::from_le_bytes([sample[0], sample[1]])) / 32768.;
            }
            if self.tx.try_send(packet).is_err() {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

pub(super) struct Monitor {
    stop: mpsc::Sender<()>,
    worker: Option<thread::JoinHandle<Result<()>>>,
}

impl Monitor {
    pub(super) fn start(
        tx: SyncSender<AudioPacket>,
        dropped: Arc<AtomicU64>,
        errors: Arc<AtomicU64>,
        muted: Arc<AtomicBool>,
        origin: Instant,
    ) -> Result<Self> {
        let server = server()?;
        let (ready, started) = mpsc::sync_channel(1);
        let (stop, stopping) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("cueglint-pulse-monitor".into())
            .spawn(move || {
                let conn = match Connection::open(&server) {
                    Ok(conn) => conn,
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return Ok(());
                    }
                };
                if ready.send(Ok(())).is_err() {
                    return Ok(());
                }
                let mut audio = Packets {
                    origin,
                    sequence: 0,
                    next_ns: 0,
                    tx,
                    dropped,
                    muted,
                };
                loop {
                    if let Err(error) = conn.read(&mut audio) {
                        errors.fetch_add(1, Ordering::Relaxed);
                        return Err(error);
                    }
                    match stopping.recv_timeout(POLL) {
                        Err(mpsc::RecvTimeoutError::Timeout) => (),
                        _ => return conn.finish(&mut audio),
                    }
                }
            })?;
        let mut capture = Self {
            stop,
            worker: Some(worker),
        };
        if let Err(error) = started
            .recv_timeout(START_TIMEOUT + Duration::from_secs(1))
            .context("starting Linux playback monitor")
            .and_then(|r| r)
        {
            capture.stop()?;
            return Err(error);
        }
        Ok(capture)
    }

    pub(super) fn stop(&mut self) -> Result<()> {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("playback monitor worker panicked"))??;
        }
        Ok(())
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{Capture, CaptureConfig};
    use std::process::{Command, Stdio};

    #[test]
    fn missing_library_or_required_symbol_is_actionable() {
        let missing = Pulse::load("libcueglint_nonexistent_fixture.so.0")
            .err()
            .unwrap();
        assert!(missing.to_string().contains("libpulse.so.0"));
        let no_symbols = Pulse::from_library(libloading::os::unix::Library::this().into())
            .err()
            .unwrap();
        assert!(
            no_symbols
                .to_string()
                .contains("missing PulseAudio symbol pa_mainloop_new")
        );
    }

    #[test]
    #[ignore = "CI injects an unreadable libpulse.so.0 through a private library path"]
    fn system_capture_reports_unavailable_library() {
        let dir = std::env::var_os("CUEGLINT_PULSE_FIXTURE").expect("use linux-monitor-fixture.sh");
        let dir = Path::new(&dir);
        assert_eq!(
            server().unwrap().as_bytes(),
            format!("unix:{}/native", dir.display()).as_bytes()
        );
        assert!(dir.join("missing/libpulse.so.0").is_file());
        let error = Capture::start(&CaptureConfig {
            system_audio: true,
            ..CaptureConfig::default()
        })
        .err()
        .unwrap();
        assert!(
            error
                .to_string()
                .contains("loading the Linux playback monitor library"),
            "{error:#}"
        );
    }

    #[test]
    fn signed_latency_and_buffered_fallback_share_session_time() {
        // Each 32-byte block lasts one millisecond.
        let now = 10_000_000;
        assert_eq!(
            fragment_times(now, 0, 32, 96, Some((1000, false))).unwrap(),
            (9_000_000, 10_000_000)
        );
        assert_eq!(
            fragment_times(now, 0, 32, 96, Some((1000, true))).unwrap(),
            (11_000_000, 12_000_000)
        );
        let first = fragment_times(now, 0, 32, 96, None).unwrap();
        let second = fragment_times(now, first.1, 32, 64, None).unwrap();
        assert_eq!(first, (7_000_000, 8_000_000));
        assert_eq!(second, (first.1, 9_000_000));
        assert_eq!(
            fragment_times(0, 0, 32, 32, Some((1000, false))).unwrap().0,
            0
        );
        assert_eq!(
            fragment_times(now, now, 32, 32, Some((2000, false)))
                .unwrap()
                .0,
            now
        );
        assert!(fragment_times(u64::MAX, 0, 32, 32, Some((1, true))).is_err());
        assert!(fragment_times(u64::MAX, 0, 32, 32, Some((0, false))).is_err());
    }

    fn pactl(args: &[&str]) -> String {
        let out = Command::new("pactl").args(args).output().unwrap();
        assert!(
            out.status.success(),
            "pactl {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    #[test]
    #[ignore = "requires the isolated server created by .github/scripts/linux-monitor-fixture.sh"]
    fn isolated_monitor_captures_output_and_reports_removal() {
        let dir = std::env::var_os("CUEGLINT_PULSE_FIXTURE").expect("use linux-monitor-fixture.sh");
        let dir = Path::new(&dir);
        assert_eq!(
            server().unwrap().as_bytes(),
            format!("unix:{}/native", dir.display()).as_bytes()
        );
        assert_eq!(pactl(&["get-default-sink"]), "cueglint_output");
        assert_eq!(pactl(&["get-default-source"]), "cueglint_decoy.monitor");
        let sinks = pactl(&["list", "short", "sinks"]);
        let names: Vec<_> = sinks
            .lines()
            .map(|l| l.split_whitespace().nth(1).unwrap())
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"cueglint_output") && names.contains(&"cueglint_decoy"));

        let mut capture = Capture::start(&CaptureConfig {
            system_audio: true,
            ..CaptureConfig::default()
        })
        .unwrap();
        let raw = dir.join("original.pcm");
        // 800 Hz square wave, 0.5 s. The unrelated default source has no signal.
        let wave: Vec<_> = (0..8000)
            .flat_map(|i| (if i % 20 < 10 { 8192i16 } else { -8192i16 }).to_le_bytes())
            .collect();
        std::fs::write(&raw, wave).unwrap();
        let mut playback = Command::new("paplay")
            .args([
                "--raw",
                "--device=cueglint_output",
                "--rate=16000",
                "--channels=1",
                "--format=s16le",
            ])
            .arg(&raw)
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(6);
        let mut samples = Vec::new();
        let mut previous: Option<(u64, u64)> = None;
        while Instant::now() < deadline && samples.len() < 3200 {
            if let Some(packet) = capture.recv_timeout(Duration::from_millis(100)).unwrap() {
                assert_eq!(packet.source, Source::SystemAudio);
                assert_eq!(packet.sample_rate, SAMPLE_RATE);
                if let Some((sequence, end_ns)) = previous {
                    assert_eq!(packet.sequence, sequence + 1);
                    assert!(packet.start_ns >= end_ns);
                }
                previous = Some((
                    packet.sequence,
                    packet.start_ns + packet.len as u64 * 1_000_000_000 / u64::from(SAMPLE_RATE),
                ));
                for &v in &packet.samples[..packet.len] {
                    if !samples.is_empty() || v.abs() > 0.1 {
                        samples.push(v);
                    }
                }
            }
        }
        if playback.try_wait().unwrap().is_none() && Instant::now() >= deadline {
            let _ = playback.kill();
        }
        assert!(playback.wait().unwrap().success());
        assert!(samples.len() >= 3200, "missing original playback");
        let tested = &samples[..3200];
        let crossings = tested
            .windows(2)
            .filter(|p| p[0] <= 0.0 && p[1] > 0.0)
            .count();
        let rms = (tested.iter().map(|s| s * s).sum::<f32>() / tested.len() as f32).sqrt();
        assert!(
            (155..=165).contains(&crossings) && (0.18..0.32).contains(&rms),
            "crossings={crossings}, rms={rms}"
        );
        assert_eq!(capture.dropped_packets.load(Ordering::Relaxed), 0);
        assert_eq!(capture.device_errors.load(Ordering::Relaxed), 0);

        capture.stop().unwrap();
        // Stop a fresh recording immediately after original playback completes. No
        // consumer has removed its final packets from the bounded queue yet.
        let mut ended = Capture::start(&CaptureConfig {
            system_audio: true,
            ..CaptureConfig::default()
        })
        .unwrap();
        let tail: Vec<_> = (0..4800)
            .flat_map(|i| {
                let level = if i >= 4000 { 12288i16 } else { 0 };
                (if i % 20 < 10 { level } else { -level }).to_le_bytes()
            })
            .collect();
        std::fs::write(&raw, tail).unwrap();
        let status = Command::new("paplay")
            .args([
                "--raw",
                "--device=cueglint_output",
                "--rate=16000",
                "--channels=1",
                "--format=s16le",
            ])
            .arg(&raw)
            .stdin(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        ended.stop().unwrap();
        let mut final_marker_frames = 0;
        for packet in ended.stopped_packets() {
            let packet = packet.unwrap();
            final_marker_frames += packet.samples[..packet.len]
                .iter()
                .filter(|v| v.abs() > 0.3)
                .count();
        }
        assert!(
            (770..=830).contains(&final_marker_frames),
            "missing final marker samples: {final_marker_frames}"
        );
        assert_eq!(ended.dropped_packets.load(Ordering::Relaxed), 0);
        let mut capture = Capture::start(&CaptureConfig {
            system_audio: true,
            ..CaptureConfig::default()
        })
        .unwrap();
        let modules = pactl(&["list", "short", "modules"]);
        let id = modules
            .lines()
            .find(|l| l.contains("module-null-sink") && l.contains("sink_name=cueglint_output"))
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        pactl(&["unload-module", id]);
        let deadline = Instant::now() + Duration::from_secs(2);
        while capture.device_errors.load(Ordering::Relaxed) == 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            capture.device_errors.load(Ordering::Relaxed) > 0,
            "lost monitor must terminate capture"
        );
        assert!(
            capture.stop().is_err(),
            "capture must surface the monitor failure"
        );
        assert!(capture.stop().is_ok(), "second stop releases nothing");
        eprintln!(
            "isolated Linux monitor: {crossings} rising crossings in 3200 frames, rms={rms:.3}; monitor removal surfaced"
        );
    }
}
