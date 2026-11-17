use cueglint_core::{Speaker, Store, TranscriptSource, TranscriptUpdate};
use cueglint_local::generation::{
    FinishReason, GenerationOptions, LocalGenerator, PROMPT_TEMPLATE_REVISION,
};
use cueglint_local::models;
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

#[test]
#[ignore = "requires the verified local Qwen profile in CUEGLINT_TEST_MODELS_DIR; never downloads models"]
fn real_local_generation_streams_records_identity_and_honors_cancellation() -> anyhow::Result<()> {
    let directory =
        PathBuf::from(std::env::var_os("CUEGLINT_TEST_MODELS_DIR").ok_or_else(|| {
            anyhow::anyhow!("set CUEGLINT_TEST_MODELS_DIR to the installed model root")
        })?);
    let profile = std::env::var("CUEGLINT_TEST_GENERATION_PROFILE")
        .unwrap_or_else(|_| models::GENERATION_PROFILE.to_owned());
    let mut generator = LocalGenerator::load_profile(&directory, &profile)?;
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Local inference smoke fixture", Some("meeting"))?;
    store.upsert_transcript(
        &session.id,
        TranscriptUpdate {
            id: "fixture-final".to_owned(),
            speaker: Speaker::Other,
            source: TranscriptSource::Manual,
            text: "The project launch deadline is Friday.".to_owned(),
            finalized: true,
        },
    )?;
    let request = store.begin_request(
        &session.id,
        "What is the project launch deadline? Answer in one short sentence.",
    )?;
    let cancel = AtomicBool::new(false);
    let mut streamed = String::new();
    let mut chunks = 0;
    let prompt = generator.render_prompt(&request, GenerationOptions { max_new_tokens: 32 })?;
    let prompt_sha256 = format!("{:x}", Sha256::digest(prompt.as_bytes()));
    let output = generator.generate(
        &request,
        GenerationOptions { max_new_tokens: 32 },
        &cancel,
        |text| {
            streamed.push_str(text);
            chunks += 1;
        },
    )?;
    assert!(!output.text.trim().is_empty());
    assert_eq!(streamed, output.text);
    assert!(chunks > 0 && output.prompt_tokens > 0 && output.generated_tokens <= 32);
    assert_ne!(output.finish_reason, FinishReason::Cancelled);
    assert_eq!(output.prompt_sha256, prompt_sha256);
    assert_eq!(output.template_revision, PROMPT_TEMPLATE_REVISION);
    assert_eq!(output.decoder_policy_revision, None);
    assert_eq!(
        output.model.sha256,
        models::profile(&profile)?.files[0].sha256
    );
    assert!(store.complete_request_with_metadata(
        &session.id,
        &request.id,
        &output.text,
        output.metadata()
    )?);
    let cancelled = store.begin_request(
        &session.id,
        "Explain the entire project launch process in detail.",
    )?;
    let cancelled_output = generator.generate(
        &cancelled,
        GenerationOptions { max_new_tokens: 64 },
        &cancel,
        |_| cancel.store(true, Ordering::Relaxed),
    )?;
    assert_eq!(cancelled_output.finish_reason, FinishReason::Cancelled);
    assert!(cancelled_output.generated_tokens > 0 && cancelled_output.generated_tokens < 64);
    assert!(
        store
            .complete_request_with_metadata(
                &session.id,
                &cancelled.id,
                &cancelled_output.text,
                cancelled_output.metadata()
            )
            .is_err()
    );
    store.cancel_request(&session.id, &cancelled.id)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "fixture": "hand-authored launch-deadline sentence", "architecture": std::env::consts::ARCH,
            "os": std::env::consts::OS, "stream_chunks": chunks, "output": output,
            "cancellation": {"finish_reason": cancelled_output.finish_reason, "generated_tokens": cancelled_output.generated_tokens},
            "saved_inference": store.session(&session.id)?.suggestions[0].inference,
        }))?
    );
    Ok(())
}
