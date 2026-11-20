//! Observe the same unread frame before/after a server clock update, and again with a real backlog.
use super::{Connection, MAX_BYTES, server};
use crate::audio::{Capture, CaptureConfig, SAMPLE_RATE};
use std::{
    ffi::{c_int, c_long, c_void},
    path::PathBuf,
    ptr,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[path = "paplay_clock.rs"]
mod paplay_clock;

// 64-bit x86 Linux prefix through read_index, checked against the installed Pulse C header.
#[repr(C)]
struct Timing {
    seconds: c_long,
    micros: c_long,
    synchronized_clocks: c_int,
    sink_us: u64,
    source_us: u64,
    transport_us: u64,
    playing: c_int,
    write_corrupt: c_int,
    written: i64,
    read_corrupt: c_int,
    read: i64,
}
type TimingFn = unsafe extern "C" fn(*mut c_void) -> *const Timing;
type UpdateFn = unsafe extern "C" fn(
    *mut c_void,
    unsafe extern "C" fn(*mut c_void, c_int, *mut c_void),
    *mut c_void,
) -> *mut c_void;

#[derive(Debug)]
struct Sample {
    elapsed: i128,
    queued_bytes: usize,
    lag_us: Option<i128>,
    timing_age_ns: Option<i128>,
    raw_origin: Option<i128>,
}

impl Sample {
    fn take(conn: &Connection, epoch: Instant, timing: TimingFn) -> Self {
        let elapsed = epoch.elapsed().as_nanos() as i128;
        let wall_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i128;
        let mut latency_us = 0;
        let mut negative = 0;
        let ok = unsafe {
            (conn.api.pa_stream_get_latency)(conn.stream, &mut latency_us, &mut negative)
        };
        let lag_us = (ok >= 0).then(|| {
            if negative == 0 {
                i128::from(latency_us)
            } else {
                -i128::from(latency_us)
            }
        });
        let queued_bytes = unsafe { (conn.api.pa_stream_readable_size)(conn.stream) };
        assert!(queued_bytes <= MAX_BYTES as usize);
        let raw = unsafe { timing(conn.stream).as_ref() };
        let mut raw_origin = None;
        let mut timing_age_ns = None;
        if let Some(raw) = raw.filter(|r| r.write_corrupt == 0 && r.read_corrupt == 0) {
            let at_ns = i128::from(raw.seconds) * 1_000_000_000 + i128::from(raw.micros) * 1_000;
            let age = wall_ns - at_ns;
            timing_age_ns = Some(age);
            // timing.timestamp is the estimated server observation instant. Record samples
            // precede delivery by source delay; monitor playback follows by sink delay.
            let queue_ns = bytes_ns(i128::from(raw.written) - i128::from(raw.read));
            raw_origin = Some(
                elapsed - age - queue_ns
                    + (i128::from(raw.sink_us) - i128::from(raw.source_us)) * 1_000,
            );
            eprintln!(
                "Pulse raw: written={} read={} sink_us={} source_us={} transport_us={} synchronized={}",
                raw.written,
                raw.read,
                raw.sink_us,
                raw.source_us,
                raw.transport_us,
                raw.synchronized_clocks
            );
        }
        Self {
            elapsed,
            queued_bytes,
            lag_us,
            timing_age_ns,
            raw_origin,
        }
    }

    fn report(&self, label: &str, frame_offset_ns: i128, playback_origin: i128) {
        let error = |origin: i128| origin + frame_offset_ns - playback_origin;
        eprintln!(
            "Pulse shadow {label}: delivery={} ns queued_bytes={} native_lag_us={:?} timing_age_ns={:?} native_error_ns={:?} raw_error_ns={:?} queue_error_ns={}",
            self.elapsed,
            self.queued_bytes,
            self.lag_us,
            self.timing_age_ns,
            self.lag_us.map(|lag| error(self.elapsed - lag * 1000)),
            self.raw_origin.map(error),
            error(self.elapsed - bytes_ns(self.queued_bytes as i128))
        );
    }
}

fn bytes_ns(bytes: i128) -> i128 {
    bytes * 1_000_000_000 / (2 * i128::from(SAMPLE_RATE))
}

fn refresh(conn: &Connection, update: UpdateFn) {
    unsafe extern "C" fn completed(_: *mut c_void, ok: c_int, data: *mut c_void) {
        unsafe { *data.cast::<bool>() = ok != 0 };
    }
    let mut success = false;
    let op = unsafe { update(conn.stream, completed, ptr::from_mut(&mut success).cast()) };
    assert!(!op.is_null());
    conn.wait_operation(op, Instant::now() + Duration::from_millis(250))
        .unwrap();
    assert!(success, "server clock refresh failed");
}

#[test]
#[ignore = "requires fresh private Pulse server, virtual ALSA source and both playback witnesses"]
fn first_marker_native_clocks_and_backlog() {
    assert_eq!(std::mem::offset_of!(Timing, sink_us), 24);
    assert_eq!(std::mem::offset_of!(Timing, written), 56);
    assert_eq!(std::mem::offset_of!(Timing, read), 72);
    let fixture = PathBuf::from(std::env::var_os("CUEGLINT_PULSE_FIXTURE").unwrap());
    assert_eq!(
        server().unwrap().to_str().unwrap(),
        format!("unix:{}/native", fixture.display())
    );
    let config = fixture.join("asound.conf");
    assert_eq!(
        PathBuf::from(std::env::var_os("ALSA_CONFIG_PATH").unwrap()),
        config
    );
    assert_eq!(
        std::fs::read_to_string(config).unwrap(),
        format!(
            "pcm.!default {{\n    type pulse\n    server \"unix:{}/native\"\n    device \"cueglint_decoy.monitor\"\n}}\n",
            fixture.display()
        )
    );
    let epoch = Instant::now();
    let mut mic = Capture::start(&CaptureConfig {
        microphone: true,
        system_audio: false,
        ..Default::default()
    })
    .unwrap();
    let conn = Connection::open(&server().unwrap()).unwrap();
    let api = &conn.api._library;
    let timing = unsafe { *api.get::<TimingFn>(b"pa_stream_get_timing_info\0").unwrap() };
    let update = unsafe {
        *api.get::<UpdateFn>(b"pa_stream_update_timing_info\0")
            .unwrap()
    };
    let (mut mic_writer, mic_clock) = paplay_clock::RenderClock::observe(
        paplay_clock::playback(&fixture, "cueglint_decoy", 400, true),
        epoch,
    );
    let (mut output_writer, output_clock) = paplay_clock::RenderClock::observe(
        paplay_clock::playback(&fixture, "cueglint_output", 800, true),
        epoch,
    );
    let until = Instant::now() + Duration::from_secs(6);
    let frame_offset_ns = loop {
        assert!(Instant::now() < until, "system marker missing");
        conn.iterate().unwrap();
        let (mut data, mut bytes) = (ptr::null(), 0);
        conn.check(unsafe { (conn.api.pa_stream_peek)(conn.stream, &mut data, &mut bytes) })
            .unwrap();
        if bytes == 0 {
            std::thread::sleep(Duration::from_millis(2));
            continue;
        }
        assert!(bytes <= MAX_BYTES as usize && bytes % 2 == 0);
        if !data.is_null() {
            let pcm = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), bytes) };
            if let Some(i) = pcm
                .chunks_exact(2)
                .position(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs() > 2294)
            {
                break bytes_ns((2 * i) as i128);
            }
        }
        conn.check(unsafe { (conn.api.pa_stream_drop)(conn.stream) })
            .unwrap();
    };
    let before = Sample::take(&conn, epoch, timing);
    refresh(&conn, update);
    let after = Sample::take(&conn, epoch, timing);
    let hold = Instant::now() + Duration::from_millis(300);
    while Instant::now() < hold {
        conn.iterate().unwrap();
        std::thread::sleep(Duration::from_millis(2));
    }
    // The original peek cursor still points to the same first tone frame.
    let delayed = Sample::take(&conn, epoch, timing);
    refresh(&conn, update);
    let delayed_fresh = Sample::take(&conn, epoch, timing);
    conn.check(unsafe { (conn.api.pa_stream_drop)(conn.stream) })
        .unwrap();
    let writers_done = Instant::now() + Duration::from_secs(5);
    for writer in [&mut mic_writer, &mut output_writer] {
        while writer.try_wait().unwrap().is_none() {
            if Instant::now() >= writers_done {
                writer.kill().unwrap();
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(writer.wait().unwrap().success(), "playback writer failed");
    }
    let _ = mic_clock.onset("cueglint_decoy").unwrap();
    let origin = output_clock.onset("cueglint_output").unwrap();
    for (label, sample) in [
        ("immediate", &before),
        ("fresh", &after),
        ("held", &delayed),
        ("held_fresh", &delayed_fresh),
    ] {
        sample.report(label, frame_offset_ns, origin);
    }
    let grown = delayed
        .queued_bytes
        .checked_sub(before.queued_bytes)
        .unwrap();
    assert!(
        bytes_ns(grown as i128) >= 200_000_000,
        "backlog control did not retain 200 ms"
    );
    mic.stop().unwrap();
}
