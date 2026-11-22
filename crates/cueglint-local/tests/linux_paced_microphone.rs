//! A virtual microphone whose first PCM bytes cannot arrive before the fixture writes them.
#![cfg(target_os = "linux")]
use cueglint_local::audio::{Capture, CaptureConfig, Source};
use std::{
    f64::consts::TAU,
    fs::OpenOptions,
    io::Write,
    os::unix::fs::FileTypeExt,
    path::PathBuf,
    process::Command,
    sync::{atomic::Ordering, mpsc},
    thread,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires only the private FIFO source in linux-monitor-fixture.sh fifo-microphone-*"]
fn paced_microphone_clock() {
    let fixture = PathBuf::from(std::env::var_os("CUEGLINT_PULSE_FIXTURE").unwrap());
    let server = format!("unix:{}/native", fixture.display());
    let fifo = fixture.join("microphone.pcm");
    let config = fixture.join("asound.conf");
    assert!(
        std::fs::symlink_metadata(&fifo)
            .unwrap()
            .file_type()
            .is_fifo()
    );
    assert_eq!(std::env::var("PULSE_SERVER").unwrap(), server);
    assert_eq!(
        PathBuf::from(std::env::var_os("ALSA_CONFIG_PATH").unwrap()),
        config
    );
    assert_eq!(
        std::fs::read_to_string(config).unwrap(),
        format!(
            "pcm.!default {{\n    type pulse\n    server \"{server}\"\n    device \"cueglint_microphone\"\n}}\n"
        )
    );
    let chosen = Command::new("pactl")
        .arg("get-default-source")
        .output()
        .unwrap();
    assert!(chosen.status.success());
    assert_eq!(
        String::from_utf8(chosen.stdout).unwrap().trim(),
        "cueglint_microphone"
    );
    let mode = std::env::var("CUEGLINT_FIFO_SCENARIO").unwrap();
    let (lead_silence_chunks, idle, delay) = match mode.as_str() {
        "cold" => (0, Duration::from_millis(1500), false),
        "paced-silence" => (60, Duration::ZERO, false),
        "late-consumer" => (20, Duration::ZERO, true),
        _ => panic!("unknown microphone timing scenario"),
    };
    let library = libloading::os::unix::Library::this();
    let trace = std::env::var_os("CUEGLINT_TRACE_ALSA").is_some();
    let clock = trace.then(|| {
        // The fixture supplies these two symbols with LD_PRELOAD to this process only.
        unsafe { library.get::<unsafe extern "C" fn() -> u64>(b"cueglint_test_alsa_now_ns") }
            .unwrap()
    });
    let epoch_ns = clock.as_ref().map(|clock| unsafe { clock() });
    let epoch = Instant::now();
    let mut capture = Capture::start(&CaptureConfig {
        microphone: true,
        system_audio: false,
        ..Default::default()
    })
    .unwrap();
    let (written, marker) = mpsc::channel();
    let writer = thread::spawn(move || {
        let mut pipe = OpenOptions::new().write(true).open(fifo).unwrap();
        // At 16 kHz this chunk carries 20 ms of PCM. The 400 Hz marker has eight entire cycles.
        let silent = [0u8; 640];
        let mut tone = [0u8; 640];
        for (i, out) in tone.chunks_exact_mut(2).enumerate() {
            out.copy_from_slice(
                &((8192. * (TAU * 400. * i as f64 / 16000.).sin()) as i16).to_le_bytes(),
            );
        }
        let start = Instant::now() + idle;
        let mut largest_lag = Duration::ZERO;
        for chunk in 0..lead_silence_chunks + 50 {
            let target = start + Duration::from_millis(20 * chunk);
            thread::sleep(target.saturating_duration_since(Instant::now()));
            largest_lag = largest_lag.max(Instant::now().saturating_duration_since(target));
            let bytes = if (lead_silence_chunks..lead_silence_chunks + 40).contains(&chunk) {
                &tone
            } else {
                &silent
            };
            let before = epoch.elapsed().as_nanos() as i128;
            pipe.write_all(bytes).unwrap();
            if chunk == lead_silence_chunks {
                written
                    .send((before, epoch.elapsed().as_nanos() as i128))
                    .unwrap();
            }
        }
        largest_lag
    });
    let early = if delay {
        let times = marker.recv_timeout(Duration::from_secs(5)).unwrap();
        thread::sleep(Duration::from_millis(300));
        Some(times)
    } else {
        None
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let (received, assigned, sequence, rate) = loop {
        assert!(Instant::now() < deadline, "no microphone marker from FIFO");
        let Some(p) = capture.recv_timeout(Duration::from_millis(25)).unwrap() else {
            continue;
        };
        let received = epoch.elapsed().as_nanos() as i128;
        assert_eq!(p.source, Source::Microphone);
        if let Some(i) = p.samples[..p.len]
            .iter()
            .position(|value| value.abs() > 0.07)
        {
            break (
                received,
                i128::from(p.start_ns) + i as i128 * 1_000_000_000 / i128::from(p.sample_rate),
                p.sequence,
                p.sample_rate,
            );
        }
    };
    let (before, after) =
        early.unwrap_or_else(|| marker.recv_timeout(Duration::from_secs(1)).unwrap());
    let lag = writer.join().unwrap();
    capture.stop().unwrap();
    let reference = before + (after - before) / 2;
    if let Some(epoch_ns) = epoch_ns {
        assert_ne!(epoch_ns, 0, "native monotonic clock unavailable");
        let mark = epoch_ns + u64::try_from(reference).unwrap();
        let dump = unsafe {
            library.get::<unsafe extern "C" fn(u64) -> usize>(b"cueglint_test_alsa_dump")
        }
        .unwrap();
        assert!(
            unsafe { dump(mark) } > 0,
            "the trace observed no ALSA reads"
        );
        eprintln!("Native monotonic alignment: epoch_ns={epoch_ns} marker_ns={mark}");
    }
    eprintln!(
        "FIFO microphone {mode}: write_before_ns={before} write_after_ns={after} first_dequeue_ns={received} assigned_ns={assigned} sequence={sequence} rate={rate} submit_lag_ns={} assigned_minus_write_ns={} dequeue_minus_write_ns={}",
        lag.as_nanos(),
        assigned - reference,
        received - reference
    );
    assert!(
        after - before < 20_000_000 && lag < Duration::from_millis(25),
        "fixture scheduling uncertainty"
    );
    assert!(
        received >= before,
        "input reported before the original marker existed"
    );
    if delay {
        assert!(
            received - reference >= 200_000_000,
            "consumer failed to retain the queued marker"
        );
    }
    assert!(
        assigned.abs_diff(reference) <= 50_000_000,
        "microphone clock missed its original FIFO writer by {} ns",
        assigned - reference
    );
    assert_eq!(capture.dropped_packets.load(Ordering::Relaxed), 0);
    assert_eq!(capture.device_errors.load(Ordering::Relaxed), 0);
}
