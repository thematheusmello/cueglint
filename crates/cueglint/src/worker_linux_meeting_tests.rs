use super::{Command, Response, Snapshot, Worker};
use anyhow::{Context, Result, bail, ensure};
use cueglint_core::{RequestStatus, SessionStatus, Speaker, Store, TranscriptSource};
use cueglint_local::models;
use serde_json::json;
use std::{
    env, fs,
    path::PathBuf,
    process,
    time::{Duration, Instant},
};

fn wait(
    worker: &Worker,
    state: &mut Snapshot,
    label: &str,
    ready: impl Fn(&Snapshot) -> bool,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(300);
    while !ready(state) {
        match worker
            .responses
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .with_context(|| format!("waiting for {label}"))?
        {
            Response::State(new, notice) | Response::Update(new, notice) => {
                if let Some(notice) = notice {
                    eprintln!("Original meeting worker: {notice}");
                }
                *state = new;
            }
            Response::Runtime(runtime) => state.runtime = runtime,
            Response::Error(message) | Response::Notice(message, true) => {
                bail!("{label}: {message}")
            }
            _ => (),
        }
        ensure!(
            !state
                .runtime
                .capture
                .as_ref()
                .is_some_and(|c| c.state == "error"),
            "{label}: capture failed"
        );
        ensure!(
            !state
                .runtime
                .generation
                .as_ref()
                .is_some_and(|r| r.state == "error"),
            "{label}: local generation failed"
        );
    }
    eprintln!("Original meeting reached {label}");
    Ok(())
}

#[test]
#[ignore = "isolated Linux null sinks, network namespace and verified Whisper/Qwen weights required"]
fn local_meeting_round_trip() -> Result<()> {
    ensure!(cfg!(target_os = "linux"), "requires Linux");
    let scratch = PathBuf::from(env::var("CUEGLINT_PULSE_FIXTURE")?);
    ensure!(
        env::var("PULSE_SERVER")? == format!("unix:{}/native", scratch.display()),
        "isolated server required"
    );
    let sources = process::Command::new("pactl")
        .args(["list", "short", "sources"])
        .output()?;
    ensure!(sources.status.success());
    let source_rows = String::from_utf8(sources.stdout)?;
    let mut names: Vec<_> = source_rows
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect();
    names.sort_unstable();
    ensure!(
        names == ["cueglint_decoy.monitor", "cueglint_output.monitor"],
        "requires exactly two original virtual monitors"
    );
    let report = PathBuf::from(env::var("CUEGLINT_MEETING_REPORT")?);
    ensure!(!report.exists(), "report already exists");
    let db = scratch.join("meeting.sqlite3");
    ensure!(!db.exists(), "fresh meeting store required");
    let model_root = PathBuf::from(env::var("CUEGLINT_MODEL_DIR")?);
    let asr = models::verify(&model_root, models::TRANSCRIPTION_PROFILE)?.profile;
    let gen_model = models::verify(&model_root, models::GENERATION_PROFILE)?.profile;
    let asr_weights = asr
        .files
        .iter()
        .find(|f| f.name == "model.safetensors")
        .unwrap();
    let generation_weights = gen_model
        .files
        .iter()
        .find(|f| f.name.ends_with(".gguf"))
        .unwrap();
    {
        let mut store = Store::open(&db)?;
        let mut settings = store.settings()?;
        settings.microphone_enabled = false;
        settings.system_audio_enabled = true;
        settings.screen_enabled = false;
        settings.transcription_language = "en".into();
        settings.transcription_profile = Some(asr.id.clone());
        settings.generation_profile = Some(gen_model.id.clone());
        ensure!(settings.answer_provider.provider.is_local());
        store.update_settings(settings)?;
    }
    let worker = Worker::spawn(Some(db.clone()), None, eframe::egui::Context::default());
    let Response::State(mut state, _) = worker.responses.recv_timeout(Duration::from_secs(60))?
    else {
        bail!("could not open the original meeting store");
    };
    wait(&worker, &mut state, "model verification", |s| {
        [&asr.id, &gen_model.id]
            .iter()
            .all(|id| s.runtime.installed_profiles.contains(id))
    })?;
    let send = |command| {
        worker
            .commands
            .try_send(command)
            .map_err(|_| anyhow::anyhow!("meeting worker command queue refused a command"))
    };
    let began = Instant::now();
    send(Command::Start {
        title: "Original Linux monitor meeting".into(),
        mode: Some("meeting".into()),
    })?;
    wait(&worker, &mut state, "listening", |s| {
        s.runtime
            .capture
            .as_ref()
            .is_some_and(|c| c.state == "listening")
    })?;
    let id = state
        .selected
        .as_ref()
        .context("no selected meeting on Start")?
        .id
        .clone();
    ensure!(state.runtime.capture.as_ref().unwrap().session_id == id);
    let listening_ms = began.elapsed().as_millis();
    let playback_at = Instant::now();
    ensure!(
        process::Command::new("timeout")
            .args([
                "20s",
                "paplay",
                "--device=cueglint_output",
                "parity/fixtures/clean-english-v2.wav"
            ])
            .status()?
            .success(),
        "original playback failed"
    );
    let playback_ms = playback_at.elapsed().as_millis();
    wait(&worker, &mut state, "final transcript", |s| {
        s.selected
            .as_ref()
            .is_some_and(|m| m.id == id && m.transcript.iter().any(|t| t.finalized))
    })?;
    let observed_final_ms = playback_at.elapsed().as_millis();
    send(Command::Stop(id.clone()))?;
    wait(&worker, &mut state, "saved meeting", |s| {
        s.selected.as_ref().is_some_and(|m| {
            m.id == id && m.status == SessionStatus::Stopped && m.saved_at.is_some()
        })
    })?;
    let record = state.selected.as_ref().unwrap().clone();
    let cap = state
        .runtime
        .capture
        .as_ref()
        .context("missing capture metrics")?;
    ensure!(
        cap.session_id == id
            && cap.state == "stopped"
            && cap.device_errors == 0
            && cap.dropped_packets == 0
    );
    ensure!(record.transcript.iter().all(|s| s.finalized
        && s.source == TranscriptSource::SystemAudio
        && s.speaker == Speaker::Unknown
        && s.audio.as_ref().is_some_and(|a| a.start_ns < a.end_ns
            && a.sample_rate == 16_000
            && a.model_profile == asr.id
            && a.model_sha256 == asr_weights.sha256
            && a.model_revision == asr_weights.revision)));
    let db_record = Store::open(&db)?.session(&id)?;
    ensure!(db_record == record);
    send(Command::NewChat)?;
    wait(&worker, &mut state, "cleared selection", |s| {
        s.selected.is_none()
    })?;
    send(Command::Open(id.clone()))?;
    wait(&worker, &mut state, "reopened meeting", |s| {
        s.selected.as_ref() == Some(&record)
    })?;
    let mut receipt = json!({"schema_version":1,"fixture":"clean-english-v2-001","source_commit":env::var("CUEGLINT_TEST_REVISION")?,
        "meeting_id":id,"capture_route":"private PulseAudio null-sink playback monitor","microphone":false,"screen":false,
        "model_profiles":[asr,gen_model],"listening_after_start_ms":listening_ms,"playback_command_ms":playback_ms,
        "observed_final_after_playback_start_ms":observed_final_ms,"saved_at":record.saved_at,"reopened_equal":true,
        "dropped_packets":0,"device_errors":0,"transcript":record.transcript,"answer_status":"pending"});
    fs::write(&report, serde_json::to_vec_pretty(&receipt)?)?;
    let question = "According to this meeting's transcript, at what hour does the meeting start? Reply with the hour only, as a number or word.";
    let asked = Instant::now();
    send(Command::Ask {
        session: Some(id.clone()),
        prompt: question.into(),
        new_chat_mode: None,
    })?;
    wait(&worker, &mut state, "grounded answer", |s| {
        s.selected
            .as_ref()
            .is_some_and(|m| m.id == id && !m.suggestions.is_empty())
    })?;
    let completed = Store::open(&db)?.session(&id)?;
    let request = completed.requests.last().context("missing request")?;
    let answer = completed.suggestions.last().context("missing answer")?;
    let metadata = answer
        .inference
        .as_ref()
        .context("missing local inference metadata")?;
    ensure!(
        request.id == answer.request_id
            && request.status == RequestStatus::Completed
            && request.provider.provider.is_local()
            && request.prompt == question
            && request.context.transcript == record.transcript
            && request.context.screen.is_none()
            && answer.hosted_inference.is_none()
            && metadata.model_profile == gen_model.id
            && metadata.model_sha256 == generation_weights.sha256
    );
    let spoken = record
        .transcript
        .iter()
        .flat_map(|s| s.text.split(|c: char| !c.is_alphanumeric()))
        .any(|w| w == "9" || w.eq_ignore_ascii_case("nine"));
    let hour = answer
        .text
        .trim()
        .trim_end_matches(['.', '!'])
        .trim()
        .to_lowercase();
    let grounded = spoken && matches!(hour.as_str(), "9" | "nine" | "9:00" | "09:00");
    receipt["answer_status"] = json!(if grounded { "passed" } else { "failed" });
    receipt["answer"] = json!({"prompt":question,"text":answer.text,"context_revision":request.context.revision,
        "request_id":request.id,"inference":metadata,"elapsed_ms":asked.elapsed().as_millis(),"transcript_contains_hour":spoken});
    fs::write(&report, serde_json::to_vec_pretty(&receipt)?)?;
    ensure!(
        grounded,
        "see the saved answer and transcript in the receipt"
    );
    Ok(())
}
