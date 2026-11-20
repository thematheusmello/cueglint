use cueglint_core::{
    NotesCompletion, NotesPlan, NotesProvenance, Speaker, Store, TranscriptSource, TranscriptUpdate,
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};
use uuid::Uuid;

const HUMAN: &str = "Original human notes\r\nKeep café 🦀 exactly.";
const SUMMARY: &str = "Legacy user summary remains unchanged.";

fn output(database: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cueglint-cli"))
        .arg("--db")
        .arg(database)
        .args(args)
        .output()
        .unwrap()
}

fn run(database: &Path, args: &[&str]) -> Value {
    let result = output(database, args);
    assert!(
        result.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

fn refused(database: &Path, args: &[&str]) -> Output {
    let result = output(database, args);
    assert!(!result.status.success(), "unexpected success: {args:?}");
    result
}

fn supplied() -> NotesCompletion {
    NotesCompletion {
        selections: vec![vec![1, 0]],
        provenance: NotesProvenance::Supplied {},
    }
}

struct Fixture {
    directory: PathBuf,
    database: PathBuf,
    session: String,
    job: String,
    markdown: String,
}

impl Fixture {
    fn ready() -> Self {
        let directory =
            std::env::temp_dir().join(format!("cueglint-cli-highlights-{}", Uuid::new_v4()));
        let database = directory.join("notes.sqlite3");
        let mut store = Store::open(&database).unwrap();
        let session = store
            .start_session("Original human title", None)
            .unwrap()
            .id;
        store.update_notes(&session, HUMAN).unwrap();
        store.update_summary(&session, SUMMARY).unwrap();
        for (index, text) in [
            "Maybe **someone** could help, if approved.\r\n<script>literal</script> 🦀",
            "Nobody committed to an owner or date.",
        ]
        .into_iter()
        .enumerate()
        {
            store
                .upsert_transcript(
                    &session,
                    TranscriptUpdate {
                        id: format!("utterance-{index}"),
                        speaker: Speaker::Unknown,
                        source: TranscriptSource::Manual,
                        text: text.into(),
                        finalized: true,
                    },
                )
                .unwrap();
        }
        store.stop_session(&session).unwrap();
        let job = store
            .begin_notes(
                &session,
                NotesPlan {
                    source: store.notes_input(&session).unwrap().source,
                    chunks: vec![vec![0, 1]],
                },
            )
            .unwrap()
            .id;
        store.complete_notes(&session, &job, supplied()).unwrap();
        let markdown = store
            .session(&session)
            .unwrap()
            .notes_job
            .unwrap()
            .candidate
            .unwrap()
            .markdown();
        drop(store);
        Self {
            directory,
            database,
            session,
            job,
            markdown,
        }
    }

    fn cleanup(self) {
        std::fs::remove_dir_all(self.directory).unwrap();
    }
}

#[test]
fn review_accept_append_conflicts_and_retries_preserve_exact_user_notes() {
    let fixture = Fixture::ready();
    let db = &fixture.database;
    let id = fixture.session.as_str();
    let job = fixture.job.as_str();
    let before = run(db, &["session", "show", id]);
    let review = run(db, &["highlights", "review", id]);
    assert_eq!(review["kind"], "extractive_highlights");
    assert_eq!(review["job"]["status"], "ready");
    assert_eq!(review["job"]["candidate"]["provenance"]["kind"], "supplied");
    assert_eq!(review["job_freshness"], "current");
    assert_eq!(review["user_notes"], HUMAN);
    assert_eq!(review["notes_revision"], 1);
    assert!(review["accepted"].is_null());
    refused(db, &["highlights", "accept", id, "wrong-job"]);
    assert_eq!(run(db, &["session", "show", id]), before);
    let accepted = run(db, &["highlights", "accept", id, job]);
    assert_eq!(accepted["notes"], HUMAN);
    assert_eq!(accepted["notes_revision"], 1);
    assert_eq!(accepted["summary"], SUMMARY);
    assert_eq!(accepted["title"], "Original human title");
    assert_eq!(accepted["generated_notes"]["id"], job);
    assert!(accepted["generated_notes"]["applied_notes_revision"].is_null());
    assert_eq!(run(db, &["highlights", "accept", id, job]), accepted);
    refused(db, &["highlights", "append", id, job]);
    assert_eq!(run(db, &["session", "show", id]), accepted);

    let edited = "Intervening human edit.\r\nDo not replace café 🦀.";
    let current = run(db, &["session", "notes", id, edited]);
    let revision = current["notes_revision"].as_u64().unwrap().to_string();
    refused(
        db,
        &[
            "highlights",
            "append",
            id,
            "wrong-record",
            "--expected-notes-revision",
            &revision,
        ],
    );
    assert_eq!(run(db, &["session", "show", id]), current);
    let conflict = refused(
        db,
        &[
            "highlights",
            "append",
            id,
            job,
            "--expected-notes-revision",
            "1",
        ],
    );
    let conflict: Value = serde_json::from_slice(&conflict.stdout).unwrap();
    assert_eq!(conflict["state"], "conflict");
    assert_eq!(conflict["session"], current);
    assert_eq!(run(db, &["session", "show", id]), current);
    let applied = run(
        db,
        &[
            "highlights",
            "append",
            id,
            job,
            "--expected-notes-revision",
            &revision,
        ],
    );
    assert_eq!(applied["state"], "applied");
    assert_eq!(
        applied["session"]["notes"],
        format!("{edited}\n\n{}", fixture.markdown)
    );
    assert_eq!(applied["session"]["summary"], SUMMARY);
    let again = run(
        db,
        &[
            "highlights",
            "append",
            id,
            job,
            "--expected-notes-revision",
            "0",
        ],
    );
    assert_eq!(again["state"], "already_applied");
    assert_eq!(again["session"], applied["session"]);
    assert_eq!(
        run(db, &["highlights", "accept", id, job]),
        applied["session"]
    );

    let export = run(db, &["export", id, "--format", "json"]);
    assert_eq!(export["format"], "cueglint-session");
    assert_eq!(export["version"], 2);
    assert_eq!(export["notes"], applied["session"]["notes"]);
    assert_eq!(
        export["generated_notes"],
        applied["session"]["generated_notes"]
    );
    let markdown = output(db, &["export", id]);
    assert!(markdown.status.success());
    let markdown = String::from_utf8(markdown.stdout).unwrap();
    assert!(markdown.contains("Cueglint session export v2"));
    assert!(markdown.contains("Accepted extractive highlights"));
    let backup = run(db, &["backup"]);
    assert_eq!(backup["version"], 2);
    let path = fixture.directory.join("backup-v2.json");
    std::fs::write(&path, serde_json::to_vec(&backup).unwrap()).unwrap();
    let restored = fixture.directory.join("restored.sqlite3");
    run(&restored, &["restore", path.to_str().unwrap()]);
    assert_eq!(run(&restored, &["session", "show", id]), applied["session"]);
    fixture.cleanup();
}

#[test]
fn discard_and_model_preflight_preserve_previous_accepted_highlights() {
    let fixture = Fixture::ready();
    let db = &fixture.database;
    let id = fixture.session.as_str();
    let missing = fixture.directory.join("missing-models");
    let blocked = refused(
        db,
        &[
            "--models-dir",
            missing.to_str().unwrap(),
            "highlights",
            "generate",
            id,
        ],
    );
    assert!(String::from_utf8_lossy(&blocked.stderr).contains("accept or discard"));
    let accepted = run(db, &["highlights", "accept", id, &fixture.job]);
    let next = {
        let mut store = Store::open(db).unwrap();
        let plan = NotesPlan {
            source: store.notes_input(id).unwrap().source,
            chunks: vec![vec![0, 1]],
        };
        let next = store.begin_notes(id, plan).unwrap().id;
        store.complete_notes(id, &next, supplied()).unwrap();
        next
    };
    let ready = run(db, &["session", "show", id]);
    refused(db, &["highlights", "discard", id, &fixture.job]);
    assert_eq!(run(db, &["session", "show", id]), ready);
    let discarded = run(db, &["highlights", "discard", id, &next]);
    assert_eq!(discarded["notes"], HUMAN);
    assert_eq!(discarded["generated_notes"], accepted["generated_notes"]);
    assert_eq!(discarded["notes_job"]["status"], "discarded");
    assert!(discarded["notes_job"]["candidate"].is_null());
    assert_eq!(run(db, &["highlights", "discard", id, &next]), discarded);
    refused(db, &["highlights", "accept", id, &next]);
    for profile in [None, Some(cueglint_local::models::TRANSCRIPTION_PROFILE)] {
        let mut args = vec![
            "--models-dir",
            missing.to_str().unwrap(),
            "highlights",
            "generate",
            id,
        ];
        if let Some(profile) = profile {
            args.extend(["--profile", profile]);
        }
        refused(db, &args);
        assert_eq!(
            run(db, &["session", "show", id]),
            discarded,
            "preflight must not create a pending job or alter notes"
        );
    }
    fixture.cleanup();
}

#[test]
fn cli_open_interrupts_persisted_notes_jobs_without_pausing_cli_recording() {
    let fixture = Fixture::ready();
    let db = &fixture.database;
    let id = fixture.session.as_str();
    let pending = {
        let mut store = Store::open(db).unwrap();
        store.discard_notes(id, &fixture.job).unwrap();
        let plan = NotesPlan {
            source: store.notes_input(id).unwrap().source,
            chunks: vec![vec![0, 1]],
        };
        store.begin_notes(id, plan).unwrap().id
    };
    // A persisted pending record models worker interruption; no model or signal is run here.
    let review = run(db, &["highlights", "review", id]);
    assert_eq!(review["job"]["id"], pending);
    assert_eq!(review["job"]["status"], "cancelled");
    assert_eq!(review["job"]["cancel_reason"], "interrupted");
    assert_eq!(review["user_notes"], HUMAN);
    run(db, &["session", "resume", id]);
    assert_eq!(run(db, &["session", "show", id])["status"], "recording");
    run(
        db,
        &[
            "transcript",
            id,
            "New source after explicit resume.",
            "--segment",
            "after-resume",
        ],
    );
    let recording = run(db, &["session", "show", id]);
    let missing = fixture.directory.join("missing-models");
    let error = refused(
        db,
        &[
            "--models-dir",
            missing.to_str().unwrap(),
            "highlights",
            "generate",
            id,
        ],
    );
    assert!(String::from_utf8_lossy(&error.stderr).contains("stop and finalize"));
    assert_eq!(run(db, &["session", "show", id]), recording);
    run(db, &["session", "stop", id]);
    let historical = run(db, &["highlights", "review", id]);
    assert_eq!(historical["job_freshness"], "historical");
    assert_eq!(historical["job"]["cancel_reason"], "interrupted");
    fixture.cleanup();
}

#[test]
fn legacy_v1_restore_and_manual_edits_export_v2_without_inventing_revisions() {
    let fixture = Fixture::ready();
    let id = fixture.session.as_str();
    let mut legacy = run(&fixture.database, &["backup"]);
    legacy["version"] = 1.into();
    let session = legacy["sessions"][0].as_object_mut().unwrap();
    for field in ["notes_revision", "notes_job", "generated_notes"] {
        session.remove(field);
    }
    let path = fixture.directory.join("legacy-v1.json");
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let restored = fixture.directory.join("legacy-restored.sqlite3");
    run(&restored, &["restore", path.to_str().unwrap()]);
    let original = run(&restored, &["session", "show", id]);
    assert_eq!(original["notes"], HUMAN);
    assert_eq!(original["summary"], SUMMARY);
    assert_eq!(original["notes_revision"], 0);
    assert!(original["notes_job"].is_null() && original["generated_notes"].is_null());
    assert_eq!(
        run(&restored, &["session", "notes", id, HUMAN])["notes_revision"],
        0
    );
    let changed = run(
        &restored,
        &["session", "notes", id, "New human edit\r\nCafé 🦀"],
    );
    assert_eq!(changed["notes_revision"], 1);
    assert_eq!(changed["summary"], SUMMARY);
    let export = run(&restored, &["export", id, "--format", "json"]);
    assert_eq!(export["format"], "cueglint-session");
    assert_eq!(export["version"], 2);
    assert_eq!(export["notes"], changed["notes"]);
    assert_eq!(run(&restored, &["backup"])["version"], 2);
    fixture.cleanup();
}

#[test]
fn fill_template_cli_preserves_edits_and_uses_reviewed_citations_once() {
    let fixture = Fixture::ready();
    let db = &fixture.database;
    let id = fixture.session.as_str();
    let job = fixture.job.as_str();
    run(db, &["highlights", "accept", id, job]);
    let edited = "## Saltmarsh\n{{highlights}}\n\n## My edit\nFrom the tide log.";
    let notes = run(db, &["session", "notes", id, edited]);
    let rev = notes["notes_revision"].to_string();
    let args = [
        "highlights",
        "fill-template",
        id,
        job,
        "--expected-notes-revision",
        &rev,
    ];
    let applied = run(db, &args);
    let expected = edited.replace(
        "{{highlights}}",
        fixture
            .markdown
            .strip_prefix("## Generated highlights\n\n")
            .unwrap()
            .trim_end_matches('\n'),
    );
    assert_eq!(applied["session"]["notes"], expected);
    assert_eq!(applied["state"], "applied");
    assert_eq!(run(db, &args)["state"], "already_applied");
    assert_eq!(
        run(db, &["export", id, "--format", "json"])["notes"],
        expected
    );
    fixture.cleanup();
}
