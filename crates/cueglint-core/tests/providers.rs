use cueglint_core::{
    ExportFormat, HostedInferenceMetadata, ProviderConfig, ProviderId, RequestStatus, Settings,
    Store, TokenUsage,
};

fn config() -> ProviderConfig {
    ProviderConfig {
        provider: ProviderId::OpenAiCompatible,
        model_id: "org/original-model:q4".to_owned(),
        base_url: Some("http://127.0.0.1:11434/v1".to_owned()),
    }
}

fn metadata(config: &ProviderConfig) -> HostedInferenceMetadata {
    HostedInferenceMetadata {
        provider: config.provider,
        requested_model: config.model_id.clone(),
        returned_model: None,
        model_version: None,
        endpoint: config.endpoint().unwrap(),
        request_id: None,
        usage: TokenUsage::default(),
        max_output_tokens: 128,
        elapsed_ms: 12,
        finish_reason: "stop".to_owned(),
        citations: Vec::new(),
        prompt_sha256: "a".repeat(64),
        template_revision: "cueglint-provider-chat-v1".to_owned(),
    }
}

#[test]
fn version_one_provider_backups_upgrade_notes_and_strict_rejection_is_atomic() -> anyhow::Result<()>
{
    let mut original = Store::open_in_memory()?;
    let session = original.start_session("Provider plus notes migration", None)?;
    let mut settings = original.settings()?;
    settings.answer_provider = config();
    original.update_settings(settings)?;
    original.update_notes(&session.id, "Human café notes survive every boundary.")?;
    original.upsert_transcript(
        &session.id,
        cueglint_core::TranscriptUpdate {
            id: "original-utterance".into(),
            speaker: cueglint_core::Speaker::You,
            text: "An original finalized utterance.".into(),
            finalized: true,
            source: cueglint_core::TranscriptSource::Manual,
        },
    )?;
    let request = original.begin_request(&session.id, "Original hosted fixture question")?;
    original.complete_request_with_hosted_metadata(
        &session.id,
        &request.id,
        "Mocked response",
        metadata(&request.provider),
    )?;
    original.stop_session(&session.id)?;
    let mut v1: serde_json::Value = serde_json::from_str(&original.backup_json()?)?;
    v1["version"] = 1.into();
    let object = v1["sessions"][0].as_object_mut().unwrap();
    for field in ["notes_revision", "notes_job", "generated_notes"] {
        object.remove(field);
    }
    let mut store = Store::open_in_memory()?;
    store.restore_backup(&serde_json::to_string(&v1)?, false)?;
    let migrated = store.session(&session.id)?;
    assert_eq!(migrated.notes_revision, 0);
    assert_eq!(migrated.notes, "Human café notes survive every boundary.");
    assert_eq!(migrated.requests[0].provider, request.provider);
    assert_eq!(
        migrated.suggestions[0].hosted_inference,
        Some(metadata(&request.provider))
    );
    let plan = cueglint_core::NotesPlan {
        source: store.notes_input(&session.id)?.source,
        chunks: vec![vec![0]],
    };
    let job = store.begin_notes(&session.id, plan)?;
    assert!(store.complete_notes(
        &session.id,
        &job.id,
        cueglint_core::NotesCompletion {
            selections: vec![vec![0]],
            provenance: cueglint_core::NotesProvenance::Supplied {}
        }
    )?);
    store.accept_notes(&session.id, &job.id)?;
    let before = store.session(&session.id)?;
    let mut v2: serde_json::Value = serde_json::from_str(&store.backup_json()?)?;
    assert_eq!(v2["version"], 2);
    v2["sessions"][0]["generated_notes"]["draft"]["highlights"][0]["text"] =
        "Invented replacement".into();
    assert!(
        store
            .restore_backup(&serde_json::to_string(&v2)?, true)
            .is_err()
    );
    assert_eq!(store.session(&session.id)?, before);
    let mut invalid_v1: serde_json::Value = serde_json::from_str(&store.backup_json()?)?;
    invalid_v1["version"] = 1.into();
    assert!(
        store
            .restore_backup(&serde_json::to_string(&invalid_v1)?, true)
            .is_err()
    );
    assert_eq!(store.session(&session.id)?, before);
    Ok(())
}

#[test]
fn existing_settings_and_requests_default_to_local_without_migration_or_network()
-> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Original compatibility fixture", None)?;
    let request = store.begin_request(&session.id, "What was agreed?")?;
    let mut settings = serde_json::to_value(Settings::default())?;
    settings.as_object_mut().unwrap().remove("answer_provider");
    assert_eq!(
        serde_json::from_value::<Settings>(settings)?.answer_provider,
        ProviderConfig::default()
    );
    let mut json = serde_json::to_value(request)?;
    json.as_object_mut().unwrap().remove("provider");
    let request: cueglint_core::GenerationRequest = serde_json::from_value(json)?;
    assert_eq!(request.provider, ProviderConfig::default());
    assert!(store.settings()?.answer_provider.provider.is_local());
    Ok(())
}

#[test]
fn provider_model_and_origin_are_frozen_and_changes_cancel_pending_completions()
-> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Original frozen provider fixture", None)?;
    store.update_notes(&session.id, "Keep these original user notes.")?;
    let mut settings = store.settings()?;
    settings.answer_provider = config();
    store.update_settings(settings.clone())?;
    let request = store.begin_request(&session.id, "Give a short answer.")?;
    assert_eq!(request.provider, settings.answer_provider);
    let mut wrong = metadata(&request.provider);
    wrong.requested_model = "different-model".to_owned();
    assert!(
        store
            .complete_request_with_hosted_metadata(&session.id, &request.id, "Rejected", wrong)
            .is_err()
    );
    assert_eq!(
        store.session(&session.id)?.requests[0].status,
        RequestStatus::Pending
    );
    let mut wrong = metadata(&request.provider);
    wrong.endpoint = "https://unconfigured.invalid/v1/".to_owned();
    assert!(
        store
            .complete_request_with_hosted_metadata(&session.id, &request.id, "Rejected", wrong)
            .is_err()
    );
    settings.answer_provider.model_id = "different-model".to_owned();
    store.update_settings(settings)?;
    assert_eq!(
        store.session(&session.id)?.requests[0].status,
        RequestStatus::Cancelled
    );
    assert!(!store.complete_request_with_hosted_metadata(
        &session.id,
        &request.id,
        "Stale",
        metadata(&request.provider)
    )?);
    assert_eq!(
        store.session(&session.id)?.notes,
        "Keep these original user notes."
    );
    Ok(())
}

#[test]
fn hosted_provenance_roundtrips_without_fabricated_artifact_identities_and_invalid_restore_preserves_notes()
-> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Original hosted provenance fixture", None)?;
    store.update_notes(&session.id, "Notes survive invalid backups.")?;
    let mut settings = store.settings()?;
    settings.answer_provider = config();
    store.update_settings(settings)?;
    let request = store.begin_request(&session.id, "Answer from this original fixture.")?;
    assert!(store.complete_request_with_hosted_metadata(
        &session.id,
        &request.id,
        "Original mocked answer.",
        metadata(&request.provider)
    )?);
    let export = store.export_session(&session.id, ExportFormat::Json)?;
    assert!(export.contains("\"model_version\": null"));
    assert!(!export.contains("model_sha256"));
    assert!(
        store.session(&session.id)?.suggestions[0]
            .inference
            .is_none()
    );
    let backup = store.backup_json()?;
    let mut restored = Store::open_in_memory()?;
    restored.restore_backup(&backup, false)?;
    assert_eq!(restored.settings()?.answer_provider, config());
    assert_eq!(
        restored.session(&session.id)?.suggestions[0].hosted_inference,
        Some(metadata(&request.provider))
    );
    let mut bad: serde_json::Value = serde_json::from_str(&backup)?;
    bad["sessions"][0]["suggestions"][0]["hosted_inference"]["endpoint"] =
        "https://changed.invalid/".into();
    assert!(
        restored
            .restore_backup(&serde_json::to_string(&bad)?, true)
            .is_err()
    );
    assert_eq!(
        restored.session(&session.id)?.notes,
        "Notes survive invalid backups."
    );
    Ok(())
}

#[test]
fn local_restore_prepares_effective_settings_and_preserves_hosted_history() -> anyhow::Result<()> {
    let mut source = Store::open_in_memory()?;
    let mut settings = source.settings()?;
    settings.answer_provider = config();
    settings.generation_profile = Some("fixture-local-profile".to_owned());
    source.update_settings(settings.clone())?;
    let session = source.start_session("Synthetic hosted history", None)?;
    source.update_notes(&session.id, "Preserve the original notes.")?;
    let request = source.begin_request(&session.id, "What was decided?")?;
    source.complete_request_with_hosted_metadata(
        &session.id,
        &request.id,
        "A synthetic answer.",
        metadata(&request.provider),
    )?;
    source.stop_session(&session.id)?;

    let mut target = Store::open_in_memory()?;
    settings.answer_provider = ProviderConfig::default();
    let prepared = target.restore_local_backup(&source.backup_json()?, false, |effective| {
        assert_eq!(effective, &settings);
        Ok("prepared runtime")
    })?;
    assert_eq!(prepared, "prepared runtime");
    assert_eq!(target.settings()?, settings);
    assert_eq!(target.session(&session.id)?, source.session(&session.id)?);
    Ok(())
}

#[test]
fn local_restore_validates_before_prepare_and_failed_prepare_preserves_existing_data()
-> anyhow::Result<()> {
    let mut source = Store::open_in_memory()?;
    let imported = source.start_session("Synthetic imported session", None)?;
    let backup = source.backup_json()?;
    let mut target = Store::open_in_memory()?;
    let existing = target.start_session("Synthetic existing session", None)?;
    target.update_notes(&existing.id, "Keep these existing notes.")?;
    let existing = target.session(&existing.id)?;
    let mut settings = target.settings()?;
    settings.answer_provider = config();
    target.update_settings(settings.clone())?;

    let consent = target.restore_local_backup::<()>(&backup, false, |_| {
        panic!("prepare without replacement consent")
    });
    assert!(
        consent
            .unwrap_err()
            .to_string()
            .contains("explicit replacement")
    );

    let mut invalid: serde_json::Value = serde_json::from_str(&backup)?;
    invalid["settings"]["answer_provider"]["model_id"] = "invalid-for-local".into();
    assert!(
        target
            .restore_local_backup::<()>(&invalid.to_string(), true, |_| {
                panic!("prepare before validating the original provider")
            })
            .is_err()
    );

    let error = target
        .restore_local_backup::<()>(&backup, true, |_| {
            anyhow::bail!("runtime preparation failed")
        })
        .unwrap_err();
    assert_eq!(error.to_string(), "runtime preparation failed");
    assert_eq!(target.settings()?, settings);
    assert_eq!(target.session(&existing.id)?, existing);
    assert!(target.session(&imported.id).is_err());
    Ok(())
}

#[test]
fn endpoint_and_metadata_trust_boundaries_reject_credentials_redirect_targets_and_unbounded_values()
{
    let mut selected = config();
    assert_eq!(selected.endpoint().unwrap(), "http://127.0.0.1:11434/v1/");
    let old_scope = selected.credential_ref().unwrap();
    selected.base_url = Some("http://127.0.0.1:1234/v1".to_owned());
    assert_ne!(old_scope, selected.credential_ref().unwrap());
    for url in [
        "http://public.example/v1",
        "https://user:password@example.com/v1",
        "https://example.com/v1?key=private",
        "https://example.com/v1#fragment",
        "file:///tmp/server",
        "https://example.com/v1\n",
    ] {
        selected.base_url = Some(url.to_owned());
        assert!(selected.validate().is_err());
    }
    for provider in ProviderId::ALL
        .into_iter()
        .filter(|provider| !provider.is_local() && *provider != ProviderId::OpenAiCompatible)
    {
        let mut builtin = ProviderConfig {
            provider,
            model_id: "manual-model-id".to_owned(),
            base_url: None,
        };
        assert!(builtin.endpoint().unwrap().starts_with("https://"));
        builtin.base_url = Some("https://redirect.invalid/".to_owned());
        assert!(builtin.validate().is_err());
    }
    let config = config();
    let mut bad = metadata(&config);
    bad.citations.push(cueglint_core::Citation {
        url: "javascript:bad()".to_owned(),
        title: None,
    });
    assert!(bad.validate_for(&config).is_err());
    let mut bad = metadata(&config);
    bad.returned_model = Some("x".repeat(257));
    assert!(bad.validate_for(&config).is_err());
    let mut bad = metadata(&config);
    bad.finish_reason = "cancelled".to_owned();
    assert!(bad.validate_for(&config).is_err());
}
