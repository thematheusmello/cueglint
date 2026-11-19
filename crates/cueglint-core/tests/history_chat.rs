use anyhow::Result;
use cueglint_core::{
    ExportFormat, GenerationRequest, MeetingContext, MeetingExcerpt, MeetingExcerptSource,
    ProviderConfig, ProviderId, RequestStatus, Speaker, Store, TranscriptSource, TranscriptUpdate,
};
use serde_json::{Value, json};

const FIRST_ANSWER: &str = "Lena handled the Río route approval [M1].";

fn meeting(store: &mut Store, title: &str, notes: &str, spoken: &str) -> Result<String> {
    let meeting = store.start_session(title, Some("meeting"))?;
    store.update_notes(&meeting.id, notes)?;
    if !spoken.is_empty() {
        store.upsert_transcript(
            &meeting.id,
            TranscriptUpdate {
                id: "original-segment".to_owned(),
                speaker: Speaker::Other,
                text: spoken.to_owned(),
                finalized: true,
                source: TranscriptSource::Manual,
            },
        )?;
    }
    Ok(store.finish_session(&meeting.id)?.id)
}

fn excerpts(request: &GenerationRequest) -> &[MeetingExcerpt] {
    match request.context.past_meetings.as_ref() {
        Some(MeetingContext::Excerpts(excerpts)) => excerpts,
        _ => panic!("expected frozen meeting excerpts"),
    }
}

fn select_hosted(store: &mut Store) -> Result<()> {
    let mut settings = store.settings()?;
    settings.answer_provider = ProviderConfig {
        provider: ProviderId::OpenAi,
        model_id: "original-dummy-model".into(),
        base_url: None,
    };
    store.update_settings(settings)?;
    Ok(())
}

#[test]
fn history_chat_freezes_relevant_saved_meeting_spans_and_retains_frozen_titles() -> Result<()> {
    let mut store = Store::open_in_memory()?;
    let first = meeting(
        &mut store,
        "Approval call",
        "Lena approved the Río route.",
        "",
    )?;
    let second = meeting(
        &mut store,
        "Logistics call",
        "",
        "The Río route opens on Friday.",
    )?;
    meeting(
        &mut store,
        "Off-topic meeting",
        "Photographs of greenhouse orchids.",
        "",
    )?;
    let live = store.start_session("Live Río route", None)?;
    store.update_notes(
        &live.id,
        "This is still private draft content about the Río route.",
    )?;
    let ordinary = store.start_chat(None)?;
    store.update_notes(&ordinary.id, "Unsaved Río route")?;
    store.ingest_knowledge("Document", "original.txt", "Río route document content")?;

    let before = store.sessions("")?.len();
    assert!(
        store
            .start_history_chat(None, "Unmatched-hyacinth")
            .is_err()
    );
    assert_eq!(store.sessions("")?.len(), before);
    let question = "  What did we decide about the RÍO route?  ";
    let request = store.start_history_chat(Some("meeting"), question)?;
    assert_eq!(request.prompt, question);
    assert_eq!(request.provider, ProviderConfig::default());
    assert!(request.context.chat.is_empty());
    assert!(request.context.knowledge.is_empty());
    assert!(request.context.transcript.is_empty());
    assert!(request.context.screen.is_none());
    let cited = excerpts(&request);
    assert_eq!(cited.len(), 2);
    let note = cited
        .iter()
        .find(|excerpt| excerpt.meeting_id == first)
        .unwrap();
    assert_eq!(note.source, MeetingExcerptSource::Notes);
    assert_eq!(note.text, "Lena approved the Río route.");
    assert_eq!(
        &store.session(&first)?.notes[note.start_byte..note.end_byte],
        note.text
    );
    assert!(cited.iter().any(|excerpt| {
        excerpt.meeting_id == second
            && excerpt.source == MeetingExcerptSource::Transcript("original-segment".into())
            && excerpt.text == "The Río route opens on Friday."
    }));
    let chat = &request.session_id;
    assert!(store.session(chat)?.history_chat && store.session(chat)?.saved_at.is_some());
    assert!(store.complete_request(chat, &request.id, FIRST_ANSWER)?);
    store.rename_session(&first, "Approval call, renamed")?;
    assert_eq!(
        store.session(chat)?.requests,
        [GenerationRequest {
            status: RequestStatus::Completed,
            ..request.clone()
        }]
    );
    let follow_up = store.begin_request(chat, "Can you elaborate?")?;
    assert_eq!(excerpts(&follow_up), cited);
    assert_eq!(follow_up.context.chat[0].response, FIRST_ANSWER);
    let export: Value = serde_json::from_str(&store.export_session(chat, ExportFormat::Json)?)?;
    assert_eq!(export["version"], 4);
    Ok(())
}

#[test]
fn history_chat_stays_local_after_provider_switch_and_backup_restore() -> Result<()> {
    let mut store = Store::open_in_memory()?;
    let source = meeting(&mut store, "Route call", "Lena approved the Río route.", "")?;
    let request = store.start_history_chat(None, "Río route decision")?;
    assert!(store.complete_request(&request.session_id, &request.id, FIRST_ANSWER)?);
    let saved = store.session(&request.session_id)?;
    select_hosted(&mut store)?;
    assert!(
        store
            .start_history_chat(None, "Río route decision")
            .is_err()
    );
    assert!(
        store
            .begin_request(&saved.id, "Río route follow-up")
            .is_err()
    );
    assert_eq!(store.session(&saved.id)?.requests, saved.requests);
    let ordinary = store.start_chat(None)?;
    let hosted = store.begin_request(&ordinary.id, "Río route follow-up")?;
    assert_eq!(hosted.provider.provider, ProviderId::OpenAi);
    assert!(hosted.context.past_meetings.is_none() && hosted.context.chat.is_empty());

    let backup = store.backup_json()?;
    let mut restored = Store::open_in_memory()?;
    restored.restore_backup(&backup, false)?;
    assert_eq!(
        restored.settings()?.answer_provider.provider,
        ProviderId::OpenAi
    );
    assert!(
        restored
            .begin_request(&saved.id, "Continue about Río")
            .is_err()
    );
    assert_eq!(restored.session(&saved.id)?.suggestions, saved.suggestions);
    let mut local = restored.settings()?;
    local.answer_provider = ProviderConfig::default();
    restored.update_settings(local)?;
    let reopened = restored.begin_request(&saved.id, "Continue about Río")?;
    assert_eq!(reopened.provider, ProviderConfig::default());
    assert_eq!(excerpts(&reopened)[0].meeting_id, source);
    assert_eq!(reopened.context.chat[0].response, FIRST_ANSWER);

    let backup: Value = serde_json::from_str(&backup)?;
    assert_eq!(backup["version"], 4);
    let reject = |changed: Value| -> Result<()> {
        assert!(
            Store::open_in_memory()?
                .restore_backup(&changed.to_string(), false)
                .is_err()
        );
        Ok(())
    };
    let mut older = backup.clone();
    older["version"] = json!(3);
    reject(older)?;
    for (field, value) in [
        ("meeting_id", json!("missing-meeting")),
        ("end_byte", json!(500)),
        ("revision_sha256", json!("a".repeat(64))),
        ("text", json!("A forged original quote.")),
    ] {
        let mut altered = backup.clone();
        let chat = altered["sessions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|session| session["id"] == saved.id)
            .unwrap();
        chat["requests"][0]["context"]["past_meetings"]["excerpts"][0][field] = value;
        reject(altered)?;
    }
    let mut altered = backup;
    let chat = altered["sessions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|session| session["id"] == saved.id)
        .unwrap();
    chat["requests"][0]["provider"] = serde_json::to_value(store.settings()?.answer_provider)?;
    reject(altered)?;
    Ok(())
}

#[test]
fn editing_or_deleting_a_source_withdraws_dependent_answers_and_rejects_late_results() -> Result<()>
{
    let directory =
        std::env::temp_dir().join(format!("cueglint-history-test-{}", uuid::Uuid::new_v4()));
    let database = directory.join("state.sqlite3");
    let mut store = Store::open(&database)?;
    assert!(Store::open(&database).is_err());
    let source = meeting(&mut store, "Route call", "Lena approved the Río route.", "")?;
    let other = meeting(
        &mut store,
        "Weather call",
        "Cinder storms passed on Monday.",
        "",
    )?;
    let request = store.start_history_chat(None, "Río route decision")?;
    let chat = &request.session_id;
    store.update_notes(chat, "My notes in the History chat.")?;
    assert!(store.complete_request(chat, &request.id, FIRST_ANSWER)?);
    let pending = store.begin_request(chat, "Can you elaborate?")?;
    let independent = store.start_history_chat(None, "Cinder storms")?;
    assert!(store.complete_request(&independent.session_id, &independent.id, "Monday [M1].")?);
    store.update_notes(&source, "Lena postponed the Río route.")?;
    assert!(!store.complete_request(chat, &pending.id, "A late answer")?);
    let withdrawn = store.session(chat)?;
    assert_eq!(withdrawn.requests.len(), 2);
    assert_eq!(withdrawn.requests[1].prompt, "Can you elaborate?");
    assert!(withdrawn.history_chat && withdrawn.active_request_id.is_none());
    assert!(withdrawn.suggestions.is_empty());
    assert!(withdrawn.requests.iter().all(|request| {
        request.status == RequestStatus::Cancelled
            && request.context.chat.is_empty()
            && request.context.past_meetings == Some(MeetingContext::Withdrawn)
    }));
    assert_eq!(withdrawn.notes, "My notes in the History chat.");
    let retry = store.begin_request(chat, "Río route decision")?;
    assert_eq!(excerpts(&retry)[0].text, "Lena postponed the Río route.");
    assert!(retry.context.chat.is_empty());
    store.delete_session(&source)?;
    drop(store);
    let mut store = Store::open(&database)?;
    assert!(!store.complete_request(chat, &retry.id, "Another late answer")?);
    assert!(store.begin_request(chat, "Could you elaborate?").is_err());
    assert!(store.session(&source).is_err());
    assert!(store.session(&other).is_ok());
    assert_eq!(store.session(&independent.session_id)?.suggestions.len(), 1);
    let backup = store.backup_json()?;
    assert!(!backup.contains("Lena approved") && !backup.contains("Lena postponed"));
    let mut restored = Store::open_in_memory()?;
    restored.restore_backup(&backup, false)?;
    assert_eq!(restored.session(chat)?.notes, withdrawn.notes);
    assert_eq!(restored.session(chat)?.requests.len(), 3);
    assert!(
        restored
            .session(chat)?
            .requests
            .iter()
            .all(|request| { request.context.past_meetings == Some(MeetingContext::Withdrawn) })
    );
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
