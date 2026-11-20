use super::{Command, Response, Snapshot, Worker};
use anyhow::{Context, Result, bail, ensure};
use cueglint_core::{RequestStatus, SessionStatus, Speaker, Store, TranscriptSource};
use cueglint_local::models;
use serde_json::json;
use std::{
    env, fs,
    path::PathBuf,
    process,
    sync::mpsc::RecvTimeoutError,
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

fn shutdown(worker: Worker) -> Result<()> {
    drop(worker.commands);
    drop(worker.screenshot);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match worker
            .responses
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
            Err(RecvTimeoutError::Timeout) => bail!("meeting worker failed to shut down"),
            Ok(_) => (),
        }
    }
}

#[test]
fn closing_worker_releases_saved_meeting_for_independent_reopen() -> Result<()> {
    let directory = env::temp_dir().join(format!("cueglint-worker-lock-{}", uuid::Uuid::new_v4()));
    let db = directory.join("original.sqlite3");
    let mut store = Store::open(&db)?;
    let mut settings = store.settings()?;
    settings.model_directory = Some(
        directory
            .join("empty-models")
            .to_string_lossy()
            .into_owned(),
    );
    settings.microphone_enabled = false;
    settings.system_audio_enabled = false;
    settings.screen_enabled = false;
    store.update_settings(settings)?;
    let meeting = store.start_session("Original shutdown fixture", None)?;
    let saved = store.finish_session(&meeting.id)?;
    drop(store);
    let worker = Worker::spawn(Some(db.clone()), None, eframe::egui::Context::default());
    ensure!(matches!(
        worker.responses.recv_timeout(Duration::from_secs(10))?,
        Response::State(_, _)
    ));
    ensure!(
        Store::open(&db).is_err(),
        "a running worker must own its database exclusively"
    );
    shutdown(worker)?;
    ensure!(Store::open(&db)?.session(&meeting.id)? == saved);
    fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
#[ignore = "isolated Linux null sinks, network namespace and verified Whisper/Qwen weights required"]
fn local_meeting_round_trip() -> Result<()> {
    meeting_round_trip(false)
}

#[test]
#[ignore = "isolated ALSA/Pulse routes, network namespace and verified Whisper/Qwen weights required"]
fn dual_source_meeting_round_trip() -> Result<()> {
    meeting_round_trip(true)
}

fn playback(sink: &str, audio: &str) -> Result<process::Child> {
    Ok(process::Command::new("timeout")
        .args(["20s", "paplay", "--device", sink, audio])
        .spawn()?)
}

fn meeting_round_trip(microphone: bool) -> Result<()> {
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
    if microphone {
        ensure!(PathBuf::from(env::var("ALSA_CONFIG_PATH")?) == scratch.join("asound.conf"));
    }
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
        settings.microphone_enabled = microphone;
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
    let mut system_playback = playback("cueglint_output", "parity/fixtures/clean-english-v2.wav")?;
    let mic_playback = microphone
        .then(|| {
            playback(
                "cueglint_decoy",
                "parity/fixtures/original-microphone-v1.wav",
            )
        })
        .transpose()?;
    ensure!(system_playback.wait()?.success(), "system playback failed");
    if let Some(mut mic) = mic_playback {
        ensure!(mic.wait()?.success(), "microphone fixture playback failed");
    }
    let playback_ms = playback_at.elapsed().as_millis();
    wait(&worker, &mut state, "final transcript", |s| {
        s.selected.as_ref().is_some_and(|m| {
            let finalized = |source| {
                m.transcript
                    .iter()
                    .any(|t| t.finalized && t.source == source)
            };
            m.id == id
                && finalized(TranscriptSource::SystemAudio)
                && (!microphone || finalized(TranscriptSource::Microphone))
        })
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
        && (s.source == TranscriptSource::SystemAudio
            || (microphone && s.source == TranscriptSource::Microphone))
        && s.speaker == Speaker::Unknown
        && s.audio.as_ref().is_some_and(|a| a.start_ns < a.end_ns
            && a.sample_rate == 16_000
            && a.model_profile == asr.id
            && a.model_sha256 == asr_weights.sha256
            && a.model_revision == asr_weights.revision)));
    let segment_ids: std::collections::HashSet<_> =
        record.transcript.iter().map(|s| &s.id).collect();
    ensure!(segment_ids.len() == record.transcript.len());
    let mut receipt = json!({"schema_version":1,"fixture":"clean-english-v2-001","source_commit":env::var("CUEGLINT_TEST_REVISION")?,
        "meeting_id":id,"capture_route":"private PulseAudio null-sink playback monitor","microphone":microphone,"screen":false,
        "model_profiles":[asr,gen_model],"listening_after_start_ms":listening_ms,"playback_command_ms":playback_ms,
        "observed_final_after_playback_start_ms":observed_final_ms,"saved_at":record.saved_at,"reopened_equal":false,"disk_verified_after_shutdown":false,
        "dropped_packets":0,"device_errors":0,"transcript":record.transcript,"answer_status":"pending"});
    if microphone {
        receipt["microphone_fixture"] = json!("original-microphone-v1");
        receipt["microphone_route"] = json!("ALSA default → private PulseAudio decoy monitor");
    }
    fs::write(&report, serde_json::to_vec_pretty(&receipt)?)?;
    send(Command::NewChat)?;
    wait(&worker, &mut state, "cleared selection", |s| {
        s.selected.is_none()
    })?;
    send(Command::Open(id.clone()))?;
    wait(&worker, &mut state, "reopened meeting", |s| {
        s.selected.as_ref() == Some(&record)
    })?;
    receipt["reopened_equal"] = json!(true);
    fs::write(&report, serde_json::to_vec_pretty(&receipt)?)?;
    let question = if microphone {
        "According to this meeting's transcript, what color is the lantern and at what hour does the meeting start? Reply with just the color followed by the hour."
    } else {
        "According to this meeting's transcript, at what hour does the meeting start? Reply with the hour only, as a number or word."
    };
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
    let completed = state
        .selected
        .as_ref()
        .context("missing answered meeting")?;
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
    let has_word = |source, term: &str| {
        record
            .transcript
            .iter()
            .filter(|s| s.source == source)
            .flat_map(|s| s.text.split(|c: char| !c.is_alphanumeric()))
            .any(|word| word.eq_ignore_ascii_case(term))
    };
    let spoken = has_word(TranscriptSource::SystemAudio, "nine")
        || has_word(TranscriptSource::SystemAudio, "9");
    let valid_hour = |s: &str| matches!(s, "9" | "nine" | "9:00" | "09:00");
    let reply = answer
        .text
        .trim()
        .trim_end_matches(['.', '!'])
        .trim()
        .to_lowercase();
    let grounded = if microphone {
        let attributed = has_word(TranscriptSource::Microphone, "lantern")
            && has_word(TranscriptSource::Microphone, "green")
            && !has_word(TranscriptSource::Microphone, "kite")
            && has_word(TranscriptSource::SystemAudio, "kite")
            && !has_word(TranscriptSource::SystemAudio, "lantern");
        let overlap = record
            .transcript
            .iter()
            .filter(|s| s.source == TranscriptSource::Microphone)
            .filter_map(|s| s.audio.as_ref())
            .any(|mic| {
                record
                    .transcript
                    .iter()
                    .filter(|s| s.source == TranscriptSource::SystemAudio)
                    .filter_map(|s| s.audio.as_ref())
                    .any(|system| mic.start_ns < system.end_ns && system.start_ns < mic.end_ns)
            });
        receipt["overlapping_source_intervals"] = json!(overlap);
        receipt["distinct_source_facts"] = json!(attributed);
        let parts: Vec<_> = reply
            .split(|c: char| c != ':' && !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        spoken
            && attributed
            && overlap
            && matches!(parts.as_slice(), ["green", hour] if valid_hour(hour))
    } else {
        spoken && valid_hour(&reply)
    };
    receipt["answer_status"] = json!(if grounded { "passed" } else { "failed" });
    receipt["answer"] = json!({"prompt":question,"text":answer.text,"context_revision":request.context.revision,
        "request_id":request.id,"inference":metadata,"elapsed_ms":asked.elapsed().as_millis(),"transcript_contains_hour":spoken});
    fs::write(&report, serde_json::to_vec_pretty(&receipt)?)?;
    shutdown(worker)?;
    ensure!(
        Store::open(&db)?.session(&id)? == *completed,
        "saved meeting differs after worker shutdown"
    );
    receipt["disk_verified_after_shutdown"] = json!(true);
    fs::write(&report, serde_json::to_vec_pretty(&receipt)?)?;
    ensure!(
        grounded,
        "see the saved answer and transcript in the receipt"
    );
    Ok(())
}
