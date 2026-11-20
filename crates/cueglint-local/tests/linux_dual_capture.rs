//! Exercises both native input paths on an isolated server with original, independently routed signals.
#![cfg(target_os = "linux")]

#[path = "support/paplay_clock.rs"]
mod paplay_clock;
use paplay_clock::playback;

use cueglint_local::audio::{AudioPacket, Capture, CaptureConfig, Source};
use std::{
    f64::consts::TAU,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

const MIC_HZ: u32 = 400;
const OUTPUT_HZ: u32 = 800;

#[derive(Default)]
struct Observation {
    rate: u32,
    previous: Option<(u64, u64)>,
    onset_ns: Option<u64>,
    samples: Vec<f32>,
}

impl Observation {
    fn observe_delivery(&mut self, p: &AudioPacket, received_ns: u128) {
        let had_marker = self.onset_ns.is_some();
        self.accept(p);
        if !had_marker && let Some(assigned_ns) = self.onset_ns {
            eprintln!(
                "marker delivery source={:?} received_ns={received_ns} assigned_ns={assigned_ns} packet_start_ns={} packet_frames={} input_rate={} sequence={}",
                p.source, p.start_ns, p.len, p.sample_rate, p.sequence
            );
        }
    }

    fn accept(&mut self, p: &AudioPacket) {
        if let Some((sequence, time)) = self.previous {
            assert_eq!(p.sequence, sequence + 1);
            assert!(p.start_ns >= time, "source time went backwards");
            assert_eq!(p.sample_rate, self.rate);
        }
        self.rate = p.sample_rate;
        self.previous = Some((p.sequence, p.start_ns));
        for (i, &v) in p.samples[..p.len].iter().enumerate() {
            if self.onset_ns.is_none() && v.abs() > 0.07 {
                self.onset_ns = Some(p.start_ns + i as u64 * 1_000_000_000 / u64::from(self.rate));
            }
            if self.onset_ns.is_some() && !self.complete() {
                self.samples.push(v);
            }
        }
    }

    fn complete(&self) -> bool {
        self.rate != 0 && self.samples.len() == self.rate as usize / 4
    }

    fn amplitude(&self, frequency: u32) -> f64 {
        assert!(self.complete(), "missing a full quarter-second marker");
        let (mut sin, mut cos) = (0.0, 0.0);
        for (i, v) in self.samples.iter().enumerate() {
            let phase = TAU * i as f64 * f64::from(frequency) / f64::from(self.rate);
            let (s, c) = phase.sin_cos();
            sin += f64::from(*v) * s;
            cos += f64::from(*v) * c;
        }
        2.0 * sin.hypot(cos) / self.samples.len() as f64
    }
}

#[test]
#[ignore = "requires the isolated ALSA and PulseAudio setup in .github/scripts/linux-monitor-fixture.sh"]
fn independent_virtual_microphone_and_playback() {
    run_dual(false);
}

#[test]
#[ignore = "requires the isolated ALSA and PulseAudio setup in .github/scripts/linux-monitor-fixture.sh"]
fn virtual_microphone_and_playback_with_render_clock() {
    run_dual(true);
}

fn isolated_fixture() -> PathBuf {
    let fixture = PathBuf::from(
        std::env::var_os("CUEGLINT_PULSE_FIXTURE").expect("use linux-monitor-fixture.sh"),
    );
    let server = format!("unix:{}/native", fixture.display());
    assert_eq!(std::env::var("PULSE_SERVER").unwrap(), server);
    let config = fixture.join("asound.conf");
    assert_eq!(std::env::var_os("ALSA_CONFIG_PATH").unwrap(), config);
    assert_eq!(
        std::fs::read_to_string(config).unwrap(),
        format!(
            "pcm.!default {{\n    type pulse\n    server \"{server}\"\n    device \"cueglint_decoy.monitor\"\n}}\n"
        )
    );
    for (action, expected) in [
        ("get-default-sink", "cueglint_output"),
        ("get-default-source", "cueglint_decoy.monitor"),
    ] {
        let output = Command::new("pactl").arg(action).output().unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected);
    }

    fixture
}

fn run_dual(clocked: bool) {
    let fixture = isolated_fixture();
    let epoch = Instant::now();
    let mut capture = Capture::start(&CaptureConfig {
        microphone: true,
        system_audio: true,
        ..CaptureConfig::default()
    })
    .unwrap();
    eprintln!(
        "capture ready at {} ns on the test clock",
        epoch.elapsed().as_nanos()
    );
    let mut writer_clocks = None;
    let mut children = if clocked {
        let receipt = std::fs::File::create(fixture.join("render-clock.tsv")).unwrap();
        vec![
            Command::new(fixture.join("clocked-tones"))
                .stdin(Stdio::null())
                .stdout(receipt)
                .spawn()
                .unwrap(),
        ]
    } else {
        let (mic, mic_clock) = paplay_clock::RenderClock::observe(
            playback(&fixture, "cueglint_decoy", MIC_HZ, true),
            epoch,
        );
        let (output, output_clock) = paplay_clock::RenderClock::observe(
            playback(&fixture, "cueglint_output", OUTPUT_HZ, true),
            epoch,
        );
        writer_clocks = Some((mic_clock, output_clock));
        vec![mic, output]
    };
    let (mut mic, mut output) = (Observation::default(), Observation::default());
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        if let Some(p) = capture.recv_timeout(Duration::from_millis(100)).unwrap() {
            // Receipt is sampled immediately after dequeue, before marker analysis.
            let received_ns = epoch.elapsed().as_nanos();
            match p.source {
                Source::Microphone => mic.observe_delivery(&p, received_ns),
                Source::SystemAudio => output.observe_delivery(&p, received_ns),
            }
        }
        if mic.complete()
            && output.complete()
            && children
                .iter_mut()
                .all(|child| child.try_wait().unwrap().is_some())
        {
            break;
        }
    }
    for child in &mut children {
        if child.try_wait().unwrap().is_none() {
            child.kill().unwrap();
        }
        assert!(child.wait().unwrap().success());
    }
    capture.stop().unwrap();
    assert_eq!(capture.device_errors.load(Ordering::Relaxed), 0);
    assert_eq!(capture.dropped_packets.load(Ordering::Relaxed), 0);
    for (route, data, hz, other) in [
        ("microphone", &mic, MIC_HZ, OUTPUT_HZ),
        ("system", &output, OUTPUT_HZ, MIC_HZ),
    ] {
        let signal = data.amplitude(hz);
        let leakage = data.amplitude(other);
        eprintln!(
            "{route}: {}Hz input, {hz}Hz marker amplitude={signal:.4}, opposite marker={leakage:.4}",
            data.rate
        );
        assert!(
            (0.17..0.32).contains(&signal) && leakage < 0.015,
            "wrong or mixed route"
        );
    }
    let capture_delta = i128::from(mic.onset_ns.unwrap()) - i128::from(output.onset_ns.unwrap());
    eprintln!(
        "clocked={clocked}: captured onset mic={} ns, system={} ns, signed difference={capture_delta} ns",
        mic.onset_ns.unwrap(),
        output.onset_ns.unwrap()
    );
    if !clocked {
        let (mic_clock, output_clock) = writer_clocks.unwrap();
        let mic_origin = mic_clock.onset("cueglint_decoy");
        let output_origin = output_clock.onset("cueglint_output");
        if let (Ok(mic), Ok(output)) = (&mic_origin, &output_origin) {
            let playback_delta = mic - output;
            eprintln!(
                "independent paplay onset: capture={capture_delta} ns, playback={playback_delta} ns, residual={} ns",
                capture_delta - playback_delta
            );
        }
        // Preserve the original two-process result, including when a clock read fails.
        assert!(
            capture_delta.unsigned_abs() <= 600_000_000,
            "cold overlap failed: {capture_delta} ns; playback origins {mic_origin:?}, {output_origin:?}"
        );
        assert!(
            mic_origin.is_ok() && output_origin.is_ok(),
            "incomplete playback clock: {mic_origin:?}, {output_origin:?}"
        );
        return;
    }
    let witness = std::fs::read_to_string(fixture.join("render-clock.tsv")).unwrap();
    eprintln!(
        "Pulse playback render clocks (target, before marker, after minus duration):\n{witness}"
    );
    let mut clocks = witness.lines();
    let mut render = |expected| {
        let mut row = clocks.next().unwrap().split_whitespace();
        assert_eq!(row.next(), Some(expected));
        let times: Vec<i128> = row.map(|v| v.parse().unwrap()).collect();
        assert_eq!(times.len(), 3);
        times
    };
    let m = render("cueglint_decoy");
    let s = render("cueglint_output");
    assert!(clocks.next().is_none());
    let render_delta = m[2] - s[2];
    eprintln!(
        "captured onset mic={} ns, system={} ns, signed difference={capture_delta} ns, playback clock difference={render_delta} ns",
        mic.onset_ns.unwrap(),
        output.onset_ns.unwrap()
    );
    // Both playback streams must substantiate the stimulus timing.
    const WITNESS_NS: u128 = 50_000_000;
    assert_eq!(m[0], s[0]);
    for r in [&m, &s] {
        assert!(
            r[1].abs_diff(r[0]) <= WITNESS_NS,
            "writer missed the shared target"
        );
        assert!(
            r[2].abs_diff(r[1]) <= WITNESS_NS,
            "unstable playback render clock"
        );
    }
    assert!(
        render_delta.unsigned_abs() <= WITNESS_NS,
        "unsynchronized stimulus"
    );
    // The two 800 ms captured markers must overlap by at least the original 200 ms.
    assert!(capture_delta.unsigned_abs() <= 600_000_000);
}

#[test]
#[ignore = "requires the isolated ALSA and PulseAudio setup in .github/scripts/linux-monitor-fixture.sh"]
fn restarted_microphone_retains_queued_marker_after_two_stops() {
    let fixture = isolated_fixture();
    let mic = CaptureConfig {
        microphone: true,
        ..CaptureConfig::default()
    };
    let mut first = Capture::start(&mic).unwrap();
    assert!(
        first
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .is_some()
    );
    first.stop().unwrap();
    drop(first);
    let unavailable = Capture::start(&CaptureConfig {
        microphone: true,
        microphone_device_id: Some("alsa:cueglint_absent_input".into()),
        ..CaptureConfig::default()
    });
    let error = unavailable.err().expect("missing input must fail");
    assert!(
        error
            .to_string()
            .contains("selected microphone is unavailable")
    );

    // Restart microphone-only on the same route. Leave its original tone in the
    // callback queue until the CPAL worker has joined and both Stop calls return.
    let mut tail = Capture::start(&mic).unwrap();
    assert!(tail.recv_timeout(Duration::from_secs(5)).unwrap().is_some());
    let mut playback = playback(&fixture, "cueglint_decoy", MIC_HZ, false)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while playback.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            playback.kill().unwrap();
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(playback.wait().unwrap().success());
    tail.stop().unwrap();
    tail.stop().unwrap();
    let mut queued = Observation::default();
    for packet in tail.stopped_packets() {
        let packet = packet.unwrap();
        assert_eq!(packet.source, Source::Microphone);
        queued.accept(&packet);
    }
    let amplitude = queued.amplitude(MIC_HZ);
    assert!((0.17..0.32).contains(&amplitude));
    assert_eq!(tail.dropped_packets.load(Ordering::Relaxed), 0);
    assert_eq!(tail.device_errors.load(Ordering::Relaxed), 0);
    assert!(tail.stopped_packets().next().is_none());
    eprintln!(
        "restarted microphone drained 250 ms of {MIC_HZ} Hz after Stop, amplitude={amplitude:.4}"
    );
}
