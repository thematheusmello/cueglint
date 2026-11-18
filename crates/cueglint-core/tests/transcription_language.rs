use cueglint_core::{
    AudioMetadata, Settings, Speaker, Store, TRANSCRIPTION_LANGUAGES, TranscriptSource,
    TranscriptUpdate, validate_transcription_language,
};
use serde_json::Value;

#[test]
fn explicit_language_reopens_and_legacy_backups_keep_notes_and_audio() -> anyhow::Result<()> {
    let directory =
        std::env::temp_dir().join(format!("cueglint-language-{}", uuid::Uuid::new_v4()));
    let database = directory.join("state.sqlite3");
    let notes = "Keep human notes.\r\nCafé 🦀";
    let session = {
        let mut store = Store::open(&database)?;
        assert_eq!(store.settings()?.transcription_language, "en");
        let session = store.start_session("Original speech", None)?.id;
        store.update_notes(&session, notes)?;
        store.upsert_transcript_with_audio(
            &session,
            TranscriptUpdate {
                id: "original-utterance".into(),
                speaker: Speaker::Unknown,
                text: "Keep the original transcript.".into(),
                finalized: true,
                source: TranscriptSource::Microphone,
            },
            AudioMetadata {
                start_ns: 0,
                end_ns: 1_000_000_000,
                sample_rate: 16_000,
                model_profile: "original-fixture".into(),
                model_revision: "a".repeat(40),
                model_sha256: "b".repeat(64),
                language: "en".into(),
                backend: None,
                frontend_revision: None,
                frontend_config_sha256: None,
            },
        )?;
        let mut settings = store.settings()?;
        settings.transcription_language = "pt".into();
        store.update_settings(settings)?;
        session
    };
    {
        let store = Store::open(&database)?;
        assert_eq!(store.settings()?.transcription_language, "pt");
        let backup = store.backup_json()?;
        let mut restored = Store::open_in_memory()?;
        restored.restore_backup(&backup, false)?;
        assert_eq!(restored.settings()?.transcription_language, "pt");
        assert_eq!(restored.session(&session)?.notes, notes);
        assert_eq!(
            restored.session(&session)?.transcript[0]
                .audio
                .as_ref()
                .unwrap()
                .language,
            "en"
        );
        let mut legacy: Value = serde_json::from_str(&backup)?;
        assert_eq!(legacy["version"], 2);
        legacy["settings"]
            .as_object_mut()
            .unwrap()
            .remove("transcription_language");
        restored.restore_backup(&legacy.to_string(), true)?;
        assert_eq!(restored.settings()?.transcription_language, "en");
        for invalid in [
            Value::String("auto".into()),
            Value::String("en-US".into()),
            Value::Null,
            Value::Bool(true),
        ] {
            legacy["settings"]["transcription_language"] = invalid;
            assert!(restored.restore_backup(&legacy.to_string(), true).is_err());
            assert_eq!(restored.settings()?.transcription_language, "en");
            assert_eq!(restored.session(&session)?.notes, notes);
            assert_eq!(
                restored.session(&session)?.transcript,
                store.session(&session)?.transcript
            );
        }
    }
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn language_validation_preserves_frozen_answers_and_context() -> anyhow::Result<()> {
    let mut codes = std::collections::BTreeSet::new();
    for (code, name) in TRANSCRIPTION_LANGUAGES {
        assert!(codes.insert(code));
        assert!(!name.is_empty());
        validate_transcription_language(code)?;
    }
    assert_eq!(codes.len(), 99);
    assert_eq!(Settings::default().transcription_language, "en");
    for invalid in [
        "",
        "auto",
        "translate",
        "EN",
        "en-US",
        " pt",
        "pt\n",
        "<|pt|>",
    ] {
        assert!(validate_transcription_language(invalid).is_err());
    }
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Independent frozen answer", None)?.id;
    let request = store.begin_request(&session, "Keep answering")?;
    let before = store.session(&session)?;
    let mut settings = store.settings()?;
    settings.transcription_language = "pt".into();
    store.update_settings(settings.clone())?;
    assert_eq!(store.session(&session)?, before);
    settings.transcription_language = "auto".into();
    assert!(store.update_settings(settings).is_err());
    assert_eq!(store.settings()?.transcription_language, "pt");
    assert!(store.complete_request(&session, &request.id, "Still current")?);
    Ok(())
}
