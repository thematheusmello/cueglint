//! Exercises both native input paths on an isolated server with original, independently routed signals.
#![cfg(target_os = "linux")]

use cueglint_local::audio::{AudioPacket, Capture, CaptureConfig, SAMPLE_RATE, Source};
use std::{
    f64::consts::TAU,
    path::Path,
    process::{Child, Command, Stdio},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

const MIC_HZ: u32 = 400;
const OUTPUT_HZ: u32 = 800;

fn play(fixture: &Path, sink: &str, hz: u32) -> Child {
    let path = fixture.join(format!("original-{hz}.pcm"));
    let bytes: Vec<_> = (0..SAMPLE_RATE * 8 / 10)
        .flat_map(|frame| {
            let phase = TAU * f64::from(frame) * f64::from(hz) / f64::from(SAMPLE_RATE);
            ((phase.sin() * 8192.0) as i16).to_le_bytes()
        })
        .collect();
    std::fs::write(&path, bytes).unwrap();
    Command::new("paplay")
        .args([
            "--raw",
            "--latency-msec=40",
            "--rate=16000",
            "--channels=1",
            "--format=s16le",
            "--device",
            sink,
        ])
        .arg(path)
        .stdin(Stdio::null())
        .spawn()
        .unwrap()
}

#[derive(Default)]
struct Observation {
    rate: u32,
    previous: Option<(u64, u64)>,
    onset_ns: Option<u64>,
    samples: Vec<f32>,
}

impl Observation {
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
    let fixture = std::env::var_os("CUEGLINT_PULSE_FIXTURE").expect("use linux-monitor-fixture.sh");
    let fixture = Path::new(&fixture);
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

    let mut capture = Capture::start(&CaptureConfig {
        microphone: true,
        system_audio: true,
        ..CaptureConfig::default()
    })
    .unwrap();
    let mut children = [
        play(fixture, "cueglint_decoy", MIC_HZ),
        play(fixture, "cueglint_output", OUTPUT_HZ),
    ];
    let (mut mic, mut output) = (Observation::default(), Observation::default());
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if let Some(p) = capture.recv_timeout(Duration::from_millis(100)).unwrap() {
            match p.source {
                Source::Microphone => mic.accept(&p),
                Source::SystemAudio => output.accept(&p),
            }
        }
        if mic.complete()
            && output.complete()
            && children.iter_mut().all(|c| c.try_wait().unwrap().is_some())
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
    let delta = mic.onset_ns.unwrap().abs_diff(output.onset_ns.unwrap());
    eprintln!("original concurrent tone onset separation: {delta} ns");
    // Each signal lasts 800 ms. Require at least 200 ms of shared session time.
    assert!(delta <= 600_000_000);
}
