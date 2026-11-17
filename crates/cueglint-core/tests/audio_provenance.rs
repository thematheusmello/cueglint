use cueglint_core::{AudioMetadata, Speaker, Store, TranscriptSource, TranscriptUpdate};
use serde_json::Value;

fn audio() -> AudioMetadata {
    AudioMetadata {
        start_ns: 0,
        end_ns: 1_000_000_000,
        sample_rate: 16_000,
        model_profile: "fixture-whisper".into(),
        model_revision: "a".repeat(40),
        model_sha256: "b".repeat(64),
        language: "en".into(),
        backend: Some("fixture-cpu".into()),
        frontend_revision: Some("fixture-frontend-v1".into()),
        frontend_config_sha256: Some("c".repeat(64)),
    }
}

fn segment(text: &str) -> TranscriptUpdate {
    TranscriptUpdate {
        id: "utterance".into(),
        speaker: Speaker::Unknown,
        text: text.into(),
        finalized: true,
        source: TranscriptSource::Microphone,
    }
}

#[test]
fn backend_and_frontend_survive_frozen_context_and_backup_while_legacy_stays_unknown()
-> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let id = store.start_session("Original provenance fixture", None)?.id;
    store.upsert_transcript_with_audio(&id, segment("The workshop starts at nine."), audio())?;
    let request = store.begin_request(&id, "When does it start?")?;
    assert_eq!(request.context.transcript[0].audio, Some(audio()));
    store.cancel_request(&id, &request.id)?;
    let backup = store.backup_json()?;
    let mut restored = Store::open_in_memory()?;
    restored.restore_backup(&backup, false)?;
    let session = restored.session(&id)?;
    assert_eq!(session.transcript[0].audio, Some(audio()));
    assert_eq!(
        session.requests[0].context.transcript[0].audio,
        Some(audio())
    );

    let mut legacy: Value = serde_json::from_str(&backup)?;
    for metadata in [
        "/sessions/0/transcript/0/audio",
        "/sessions/0/requests/0/context/transcript/0/audio",
    ] {
        let object = legacy
            .pointer_mut(metadata)
            .unwrap()
            .as_object_mut()
            .unwrap();
        for key in ["backend", "frontend_revision", "frontend_config_sha256"] {
            object.remove(key);
        }
    }
    restored.restore_backup(&legacy.to_string(), true)?;
    let session = restored.session(&id)?;
    for segment in [
        &session.transcript[0],
        &session.requests[0].context.transcript[0],
    ] {
        let audio = segment.audio.as_ref().unwrap();
        assert!(audio.backend.is_none());
        assert!(audio.frontend_revision.is_none());
        assert!(audio.frontend_config_sha256.is_none());
    }
    Ok(())
}

#[test]
fn partial_or_malformed_provenance_is_rejected_before_writes_or_backup_replacement()
-> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let id = store.start_session("Preserve notes", None)?.id;
    store.update_notes(&id, "Keep these human notes.")?;
    let expected =
        store.upsert_transcript_with_audio(&id, segment("Original utterance."), audio())?;
    for present in 1..7 {
        let mut invalid = audio();
        if present & 1 == 0 {
            invalid.backend = None;
        }
        if present & 2 == 0 {
            invalid.frontend_revision = None;
        }
        if present & 4 == 0 {
            invalid.frontend_config_sha256 = None;
        }
        assert!(
            store
                .upsert_transcript_with_audio(
                    &id,
                    segment("Must not replace the original."),
                    invalid
                )
                .is_err()
        );
        assert_eq!(store.session(&id)?, expected);
    }
    let mut invalid = audio();
    invalid.frontend_config_sha256 = Some("not-a-hash".into());
    assert!(
        store
            .upsert_transcript_with_audio(&id, segment("Must not replace the original."), invalid)
            .is_err()
    );
    let mut invalid_backup: Value = serde_json::from_str(&store.backup_json()?)?;
    invalid_backup["sessions"][0]["transcript"][0]["audio"]["backend"] = Value::Null;
    assert!(
        store
            .restore_backup(&invalid_backup.to_string(), true)
            .is_err()
    );
    assert_eq!(store.session(&id)?, expected);
    Ok(())
}
