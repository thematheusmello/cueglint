use cueglint_core::{ScreenInput, Store, validate_microphone_device_id};

#[test]
fn capture_ids_validate_without_resolving_or_replacing_missing_hardware() -> anyhow::Result<()> {
    for id in [
        "coreaudio:original:uid",
        "wasapi:{original-endpoint}",
        "alsa:hw:2,0",
        "WaSaPi: é:😀 ",
    ] {
        validate_microphone_device_id(id)?;
    }
    for id in [
        "",
        "input-without-host",
        ":input",
        "core audio:input",
        "coreaudio:",
        "coreaudio:  ",
        "coreaudio:a\0b",
    ] {
        assert!(validate_microphone_device_id(id).is_err(), "{id:?}");
    }
    assert!(validate_microphone_device_id(&format!("coreaudio:{}", "x".repeat(4_096))).is_err());
    let directory =
        std::env::temp_dir().join(format!("cueglint-input-settings-{}", uuid::Uuid::new_v4()));
    let database = directory.join("state.sqlite3");
    let (session, saved) = {
        let mut store = Store::open(&database)?;
        let session = store.start_session("Original capture selection", None)?.id;
        store.update_notes(&session, "Keep exact human notes. Café 🦀")?;
        let mut settings = store.settings()?;
        let legacy = serde_json::to_value(&settings)?;
        assert!(legacy.get("screen_input").is_none());
        assert_eq!(
            serde_json::from_value::<cueglint_core::Settings>(legacy)?.screen_input,
            ScreenInput::Display
        );
        settings.microphone_device_id = Some("coreaudio:missing-device".into());
        settings.display_id = Some(42);
        settings.screen_input = ScreenInput::Window;
        store.update_settings(settings.clone())?;
        for (microphone, display) in [
            (Some("bad-id"), Some(42)),
            (Some("coreaudio:missing-device"), Some(0)),
        ] {
            let mut invalid = settings.clone();
            invalid.microphone_device_id = microphone.map(str::to_owned);
            invalid.display_id = display;
            assert!(store.update_settings(invalid).is_err());
            assert_eq!(store.settings()?, settings);
        }
        (session, settings)
    };
    {
        let store = Store::open(&database)?;
        assert_eq!(store.settings()?, saved);
        let mut restored = Store::open_in_memory()?;
        restored.restore_backup(&store.backup_json()?, false)?;
        assert_eq!(restored.settings()?, saved);
        assert_eq!(
            restored.session(&session)?.notes,
            "Keep exact human notes. Café 🦀"
        );
        let mut invalid_backup: serde_json::Value = serde_json::from_str(&store.backup_json()?)?;
        invalid_backup["settings"]["display_id"] = 0.into();
        assert!(
            restored
                .restore_backup(&invalid_backup.to_string(), true)
                .is_err()
        );
        assert_eq!(restored.settings()?, saved);
        assert_eq!(
            restored.session(&session)?.notes,
            "Keep exact human notes. Café 🦀"
        );
        let mut defaults = saved.clone();
        defaults.microphone_device_id = None;
        defaults.display_id = None;
        restored.update_settings(defaults.clone())?;
        assert_eq!(restored.settings()?, defaults);
    }
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn legacy_malformed_capture_ids_remain_readable_and_explicit_defaults_preserve_notes()
-> anyhow::Result<()> {
    let directory = std::env::temp_dir().join(format!(
        "cueglint-legacy-input-settings-{}",
        uuid::Uuid::new_v4()
    ));
    let database = directory.join("state.sqlite3");
    let session = {
        let mut store = Store::open(&database)?;
        let id = store.start_session("Original legacy recovery", None)?.id;
        store.update_notes(&id, "Preserve existing notes during explicit recovery.")?;
        id
    };
    let mut legacy = cueglint_core::Settings::default();
    legacy.microphone_device_id = Some("legacy-unparseable-id".into());
    legacy.display_id = Some(0);
    {
        let connection = rusqlite::Connection::open(&database)?;
        connection.execute("INSERT INTO settings(id,data) VALUES ('main',?1) ON CONFLICT(id) DO UPDATE SET data=excluded.data", [serde_json::to_string(&legacy)?])?;
    }
    {
        let mut store = Store::open(&database)?;
        assert_eq!(store.settings()?, legacy);
        let mut defaults = store.settings()?;
        defaults.microphone_device_id = None;
        defaults.display_id = None;
        store.update_settings(defaults.clone())?;
        assert_eq!(store.settings()?, defaults);
        assert_eq!(
            store.session(&session)?.notes,
            "Preserve existing notes during explicit recovery."
        );
    }
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
