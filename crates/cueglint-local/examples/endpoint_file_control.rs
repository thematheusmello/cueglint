//! File-only endpoint alignment experiment. No capture backend or model is opened.
use anyhow::{Context, Result, ensure};
use cueglint_local::audio::{AudioPacket, Endpoint, PACKET_FRAMES, SAMPLE_RATE, Source, load_wav};
use serde_json::json;
use std::{env, fs, path::Path};

const FRAMES: usize = 320;

fn save(path: &Path, samples: &[f32]) -> Result<()> {
    let format = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut out = hound::WavWriter::create(path, format)?;
    for &sample in samples {
        let value = (sample * 32768.).round() as i16;
        ensure!(
            f32::from(value) / 32768. == sample,
            "16-bit PCM round trip changed a sample"
        );
        out.write_sample(value)?;
    }
    out.finalize()?;
    Ok(())
}

fn main() -> Result<()> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    ensure!(
        args.len() == 2,
        "usage: endpoint_file_control INPUT_16K_PCM16.wav NEW_DIRECTORY"
    );
    let signal = load_wav(Path::new(&args[0]))?;
    let output = Path::new(&args[1]);
    fs::create_dir(output).context("create a new output directory")?;
    let mut trace = Vec::new();
    for offset in [0, 80, 160, 240] {
        let mut stream = vec![0.0; SAMPLE_RATE as usize + offset];
        stream.extend_from_slice(&signal);
        stream.resize(stream.len() + SAMPLE_RATE as usize, 0.0);
        let mut endpoint = Endpoint::new(Source::SystemAudio);
        let mut finals = Vec::new();
        for (sequence, chunk) in stream.chunks(FRAMES).enumerate() {
            let mut samples = [0.0; PACKET_FRAMES];
            samples[..chunk.len()].copy_from_slice(chunk);
            let packet = AudioPacket {
                source: Source::SystemAudio,
                sequence: sequence as u64,
                start_ns: sequence as u64 * 20_000_000,
                sample_rate: SAMPLE_RATE,
                len: chunk.len(),
                samples,
            };
            finals.extend(endpoint.push(&packet)?.into_iter().filter(|u| u.finalized));
        }
        finals.extend(endpoint.finish());
        ensure!(
            endpoint.gaps == 0 && finals.len() == 1,
            "unexpected gap or segmentation"
        );
        let u = &finals[0];
        let start = (u.start_ns * u64::from(SAMPLE_RATE) / 1_000_000_000) as usize;
        let end = start + u.samples.len();
        ensure!(
            stream.get(start..end) == Some(u.samples.as_slice()),
            "endpoint output differed from the source interval"
        );
        let left = start.saturating_sub(FRAMES);
        let right = (start + FRAMES).min(end);
        let anchor = stream[left..right]
            .iter()
            .position(|s| s.abs() >= 0.008)
            .map(|i| left + i);
        let candidate_start = anchor.map_or(start, |a| a.saturating_sub(FRAMES));
        save(&output.join(format!("current-{offset}.wav")), &u.samples)?;
        save(
            &output.join(format!("candidate-{offset}.wav")),
            &stream[candidate_start..end],
        )?;
        trace.push(
            json!({"offset_frames":offset,"current_start":start,"candidate_start":candidate_start,
            "anchor":anchor,"end":end,"current_start_ns":u.start_ns,"current_end_ns":u.end_ns,
            "final_segments":1,"packet_gaps":endpoint.gaps}),
        );
    }
    fs::write(
        output.join("alignment.json"),
        serde_json::to_vec_pretty(&trace)?,
    )?;
    Ok(())
}
