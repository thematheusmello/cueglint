use cueglint_core::{Settings, Store, TranscriptionBackend};
use serde_json::Value;

#[test]
fn saved_backend_reopens_and_legacy_backups_default_to_cpu_without_changing_notes()
-> anyhow::Result<()> {
    let directory =
        std::env::temp_dir().join(format!("cueglint-asr-settings-{}", uuid::Uuid::new_v4()));
    let database = directory.join("state.sqlite3");
    let notes = "Keep exact human notes.\r\nCafé 🦀";
    let session = {
        let mut store = Store::open(&database)?;
        assert_eq!(
            store.settings()?.transcription_backend,
            TranscriptionBackend::Cpu
        );
        let session = store.start_session("Portable ASR selection", None)?.id;
        store.update_notes(&session, notes)?;
        let mut settings = store.settings()?;
        settings.transcription_backend = TranscriptionBackend::Metal;
        store.update_settings(settings)?;
        session
    };
    {
        let store = Store::open(&database)?;
        assert_eq!(
            store.settings()?.transcription_backend,
            TranscriptionBackend::Metal
        );
        let backup = store.backup_json()?;
        let mut restored = Store::open_in_memory()?;
        restored.restore_backup(&backup, false)?;
        assert_eq!(
            restored.settings()?.transcription_backend,
            TranscriptionBackend::Metal
        );
        assert_eq!(restored.session(&session)?.notes, notes);

        let mut legacy: Value = serde_json::from_str(&backup)?;
        legacy["settings"]
            .as_object_mut()
            .unwrap()
            .remove("transcription_backend");
        restored.restore_backup(&legacy.to_string(), true)?;
        assert_eq!(
            restored.settings()?.transcription_backend,
            TranscriptionBackend::Cpu
        );
        assert_eq!(restored.session(&session)?.notes, notes);

        for invalid in [Value::String("cuda".into()), Value::Null, Value::Bool(true)] {
            legacy["settings"]["transcription_backend"] = invalid;
            assert!(restored.restore_backup(&legacy.to_string(), true).is_err());
            assert_eq!(
                restored.settings()?.transcription_backend,
                TranscriptionBackend::Cpu
            );
            assert_eq!(restored.session(&session)?.notes, notes);
        }
    }
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn backend_parsing_is_explicit_and_does_not_cancel_answer_requests() -> anyhow::Result<()> {
    assert_eq!(
        "cpu".parse::<TranscriptionBackend>()?,
        TranscriptionBackend::Cpu
    );
    assert_eq!(
        "metal".parse::<TranscriptionBackend>()?,
        TranscriptionBackend::Metal
    );
    for invalid in ["", "auto", "cuda", "Metal", " cpu", "metal\n"] {
        assert!(invalid.parse::<TranscriptionBackend>().is_err());
    }
    assert_eq!(
        Settings::default().transcription_backend,
        TranscriptionBackend::Cpu
    );
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Independent answer request", None)?.id;
    let request = store.begin_request(&session, "Keep answering")?;
    let mut settings = store.settings()?;
    settings.transcription_backend = TranscriptionBackend::Metal;
    store.update_settings(settings)?;
    assert_eq!(
        store.session(&session)?.active_request_id.as_deref(),
        Some(request.id.as_str())
    );
    assert!(store.complete_request(&session, &request.id, "Still current")?);
    Ok(())
}
