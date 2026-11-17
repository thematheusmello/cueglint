use cueglint_core::{Store, validate_shortcut_binding};
use serde_json::Value;

#[test]
fn shortcut_overrides_survive_reopening_and_backup_with_legacy_defaults() -> anyhow::Result<()> {
    let directory =
        std::env::temp_dir().join(format!("cueglint-shortcuts-{}", uuid::Uuid::new_v4()));
    let database = directory.join("state.sqlite3");
    let expected = {
        let mut store = Store::open(&database)?;
        let mut settings = store.settings()?;
        settings.shortcut_bindings.insert(
            "assistant_visibility".into(),
            Some("CommandOrControl+Alt+Backslash".into()),
        );
        settings.shortcut_bindings.insert("ask".into(), None);
        store.update_settings(settings)?
    };
    {
        let store = Store::open(&database)?;
        assert_eq!(store.settings()?, expected);
        let backup = store.backup_json()?;
        let mut restored = Store::open_in_memory()?;
        restored.restore_backup(&backup, false)?;
        assert_eq!(restored.settings()?, expected);

        let mut legacy: Value = serde_json::from_str(&backup)?;
        legacy["settings"]
            .as_object_mut()
            .unwrap()
            .remove("shortcut_bindings");
        restored.restore_backup(&legacy.to_string(), true)?;
        assert!(restored.settings()?.shortcut_bindings.is_empty());

        let mut invalid = expected.clone();
        invalid
            .shortcut_bindings
            .insert("unknown_action".into(), None);
        legacy["settings"] = serde_json::to_value(&invalid)?;
        assert!(restored.restore_backup(&legacy.to_string(), true).is_err());
        assert!(restored.settings()?.shortcut_bindings.is_empty());
    }
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn malformed_shortcuts_are_rejected_without_replacing_preferences() -> anyhow::Result<()> {
    for valid in [
        "CommandOrControl+Shift+Backslash",
        "Control+Meta+Alt+F20",
        "Meta+1",
        "Control+PageDown",
        "Control+F1",
    ] {
        validate_shortcut_binding(valid)?;
    }
    let mut store = Store::open_in_memory()?;
    let original = store.settings()?;
    for invalid in [
        "",
        "R",
        "Shift+R",
        "Alt+R",
        "CommandOrControl+Control+R",
        "CommandOrControl+Meta+R",
        "Control+Control+R",
        "Shift+Control+R",
        "Control++R",
        "Control+",
        "Control+Escape",
        "Control+Delete",
        "Control+F0",
        "Control+F21",
        "Control+F01",
        "Control+r",
        "Control+R+S",
        "Control+KeyR",
        "Control+é",
        "Control+R\n",
        "Control+R\0",
    ] {
        assert!(
            validate_shortcut_binding(invalid).is_err(),
            "accepted {invalid:?}"
        );
        let mut settings = original.clone();
        settings
            .shortcut_bindings
            .insert("new_chat".into(), Some(invalid.into()));
        assert!(store.update_settings(settings).is_err());
        assert_eq!(store.settings()?, original);
    }
    assert!(validate_shortcut_binding(&format!("Control+{}", "R".repeat(97))).is_err());
    Ok(())
}
