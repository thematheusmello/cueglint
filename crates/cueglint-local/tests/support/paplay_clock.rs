//! Read the original writer's own playback clock. The stimulus begins at frame zero.
use std::{
    f64::consts::TAU,
    io::{BufRead, BufReader},
    path::Path,
    process::{Child, Command, Stdio},
    thread::{self, JoinHandle},
    time::Instant,
};

pub fn playback(fixture: &Path, sink: &str, hz: u32, verbose: bool) -> Command {
    const SAMPLE_RATE: u32 = 16000;
    let path = fixture.join(format!("original-{hz}.pcm"));
    let bytes: Vec<_> = (0..SAMPLE_RATE * 8 / 10)
        .flat_map(|frame| {
            let phase = TAU * f64::from(frame) * f64::from(hz) / f64::from(SAMPLE_RATE);
            ((phase.sin() * 8192.0) as i16).to_le_bytes()
        })
        .collect();
    std::fs::write(&path, bytes).unwrap();
    let mut command = Command::new("paplay");
    if verbose {
        command.arg("--verbose");
    }
    command
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
        .stdin(Stdio::null());
    command
}

pub struct RenderClock(JoinHandle<Vec<(i128, i128)>>);

impl RenderClock {
    pub fn observe(mut command: Command, epoch: Instant) -> (Child, Self) {
        // In verbose mode paplay queries and prints pa_stream_get_time every 50 ms.
        // This changes neither its input PCM, buffering parameters nor target sink.
        let mut child = command
            .env("LC_ALL", "C")
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = child.stderr.take().unwrap();
        let thread = thread::spawn(move || {
            let mut samples = Vec::new();
            for data in BufReader::new(stderr).split(b'\r') {
                let observed_ns = epoch.elapsed().as_nanos() as i128;
                let data = data.unwrap();
                let text = String::from_utf8_lossy(&data);
                let Some((_, time)) = text.rsplit_once("Time: ") else {
                    continue;
                };
                let (time, _) = time.split_once(" sec; Latency: ").unwrap();
                let (seconds, millis) = time.split_once('.').unwrap();
                assert_eq!(millis.len(), 3);
                let media_ms =
                    seconds.parse::<i128>().unwrap() * 1000 + millis.parse::<i128>().unwrap();
                // Ignore the clamped zero clock before playback and the draining edge.
                if (50..=750).contains(&media_ms) {
                    samples.push((observed_ns, media_ms));
                }
            }
            samples
        });
        (child, Self(thread))
    }

    pub fn onset(self, sink: &str) -> Result<i128, String> {
        let samples = self
            .0
            .join()
            .map_err(|_| format!("failed to read playback observations on {sink}"))?;
        eprintln!("{sink} paplay (test clock ns, played ms): {samples:?}");
        if samples.len() < 4 {
            return Err(format!("insufficient clock observations on {sink}"));
        }
        let span_ms = samples.last().unwrap().1 - samples[0].1;
        let progresses = samples
            .windows(2)
            .filter(|pair| pair[1].1 > pair[0].1)
            .count();
        if span_ms < 200 || progresses < 3 || samples.windows(2).any(|pair| pair[1].1 < pair[0].1) {
            return Err(format!("insufficient advancing playback clock on {sink}"));
        }
        let mut origins: Vec<_> = samples
            .iter()
            .map(|(wall, media)| wall - media * 1_000_000)
            .collect();
        origins.sort_unstable();
        let low = origins[0];
        let high = *origins.last().unwrap();
        let median = origins[origins.len() / 2];
        eprintln!("{sink} projected PCM frame zero ns: median={median}, min={low}, max={high}");
        // paplay reports ms precision. Reject biased/stalled/late observations rather
        // than using an unstable origin to excuse capture differences.
        if high - low > 50_000_000 {
            return Err(format!("uncertain playback origin on {sink}"));
        }
        Ok(median)
    }
}

#[test]
fn projected_origin_requires_an_advancing_consistent_writer_clock() {
    let clock = |observations| RenderClock(thread::spawn(move || observations));
    let mut steady = vec![
        (450_000_000, 50),
        (550_000_000, 150),
        (650_000_000, 250),
        (750_000_000, 350),
    ];
    assert_eq!(clock(steady.clone()).onset("fixture").unwrap(), 400_000_000);
    steady[2].0 += 90_000_000;
    assert!(clock(steady).onset("delayed reader").is_err());
    assert!(clock(vec![(450_000_000, 50); 4]).onset("stalled").is_err());
    assert!(
        clock(vec![
            (450_000_000, 50),
            (450_000_000, 50),
            (650_000_000, 250),
            (650_000_000, 250)
        ])
        .onset("only two distinct positions")
        .is_err()
    );
    assert!(
        clock(vec![
            (450_000_000, 50),
            (650_000_000, 250),
            (600_000_000, 200),
            (750_000_000, 350)
        ])
        .onset("backward clock")
        .is_err()
    );
    assert!(clock(vec![]).onset("absent").is_err());
}
