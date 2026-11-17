//! Partial extractive-notes storage. Coverage is a caller-declared plan, not proof
//! of tokenizer capacity, model execution, salience, or meeting-summary quality.
use super::*;
use sha2::{Digest, Sha256};

pub const NOTES_MAX_SOURCE_BYTES: usize = 128 * 1024;
pub const NOTES_MAX_CHUNKS: usize = 16;
pub const NOTES_MAX_SELECTIONS_PER_CHUNK: usize = 8;
pub const NOTES_DIGEST_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NotesSource {
    pub session_id: String,
    pub digest_version: u32,
    pub segment_ids: Vec<String>,
    pub text_bytes: usize,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NotesInput {
    pub source: NotesSource,
    pub transcript: Vec<TranscriptSegment>,
}

/// Nonempty chunks must partition every ordered whole utterance exactly once.
/// No token-fit assertion is accepted or inferred by this core-only interface.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NotesPlan {
    pub source: NotesSource,
    pub chunks: Vec<Vec<usize>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NotesProvenance {
    /// Supplied selections make no model-execution or model-quality claim.
    // An empty struct rejects extra wire fields; serde unit variants ignore them.
    Supplied {},
    Local {
        runs: Vec<InferenceMetadata>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NotesCompletion {
    /// One result for each declared chunk; indices must belong to that chunk.
    pub selections: Vec<Vec<usize>>,
    pub provenance: NotesProvenance,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NotesHighlight {
    pub segment_id: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NotesDraft {
    pub highlights: Vec<NotesHighlight>,
    pub provenance: NotesProvenance,
    pub created_at: i64,
}

impl NotesDraft {
    /// Literal transcript text is escaped, never interpreted as generated Markdown.
    pub fn markdown(&self) -> String {
        let mut result = String::from("## Generated highlights\n\n");
        if self.highlights.is_empty() {
            result.push_str("No highlights were selected.\n");
        }
        for highlight in &self.highlights {
            for line in highlight.text.split('\n') {
                result.push_str("> ");
                for ch in line.chars() {
                    if ch.is_ascii_punctuation() {
                        result.push('\\');
                    }
                    result.push(ch);
                }
                result.push('\n');
            }
            result.push_str(&format!(
                "\nSource: `{}` (bytes {}–{})\n\n",
                highlight.segment_id, highlight.start_byte, highlight.end_byte
            ));
        }
        result
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotesJobStatus {
    Pending,
    Ready,
    Accepted,
    Discarded,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotesCancelReason {
    User,
    Resumed,
    SourceChanged,
    Interrupted,
    Restored,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NotesJob {
    pub id: String,
    pub plan: NotesPlan,
    pub status: NotesJobStatus,
    pub candidate: Option<NotesDraft>,
    pub cancel_reason: Option<NotesCancelReason>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GeneratedNotes {
    pub id: String,
    pub plan: NotesPlan,
    pub draft: NotesDraft,
    pub accepted_at: i64,
    pub applied_notes_revision: Option<u64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotesFreshness {
    Current,
    Historical,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum NotesAppendOutcome {
    Applied { session: Session },
    AlreadyApplied { session: Session },
    Conflict { session: Session },
}

impl Store {
    pub fn notes_input(&self, session_id: &str) -> Result<NotesInput> {
        let session = self.stored_session(session_id)?;
        ensure!(
            session.status == SessionStatus::Stopped,
            "stop and finalize the session before preparing highlights"
        );
        let segments: Vec<_> = session.transcript.iter().collect();
        let source = source_from_segments(&session.id, &segments)?;
        Ok(NotesInput {
            source,
            transcript: session.transcript,
        })
    }

    pub fn begin_notes(&mut self, session_id: &str, plan: NotesPlan) -> Result<NotesJob> {
        let session = self.stored_session(session_id)?;
        ensure!(
            session.status == SessionStatus::Stopped,
            "notes generation requires a stopped session"
        );
        ensure!(
            !session.notes_job.as_ref().is_some_and(|job| matches!(
                job.status,
                NotesJobStatus::Pending | NotesJobStatus::Ready
            )),
            "finish, accept or discard the current highlights job first"
        );
        validate_plan(&session, &plan)?;
        ensure!(
            source_is_current(&session, &plan.source)?,
            "notes source changed before generation began"
        );
        let job = NotesJob {
            id: id(),
            plan,
            status: NotesJobStatus::Pending,
            candidate: None,
            cancel_reason: None,
            created_at: now(),
        };
        self.change_session(session_id, |session| {
            session.notes_job = Some(job.clone());
            Ok(())
        })?;
        Ok(job)
    }

    /// False identifies a retired/deleted/cross-session request, without changing any record.
    pub fn complete_notes(
        &mut self,
        session_id: &str,
        job_id: &str,
        completion: NotesCompletion,
    ) -> Result<bool> {
        let Some(session) = self.get::<Session>("sessions", session_id)? else {
            return Ok(false);
        };
        let Some(job) = session
            .notes_job
            .as_ref()
            .filter(|job| job.id == job_id && job.status == NotesJobStatus::Pending)
        else {
            return Ok(false);
        };
        if !source_is_current(&session, &job.plan.source)? {
            return Ok(false);
        }
        let segments = validate_plan(&session, &job.plan)?;
        ensure!(
            completion.selections.len() == job.plan.chunks.len(),
            "notes completion does not cover every declared chunk"
        );
        validate_provenance(&completion.provenance, job.plan.chunks.len())?;
        let mut selected = BTreeSet::new();
        for (chunk, selections) in job.plan.chunks.iter().zip(&completion.selections) {
            ensure!(
                selections.len() <= NOTES_MAX_SELECTIONS_PER_CHUNK,
                "too many highlights selected from one chunk"
            );
            for index in selections {
                ensure!(
                    chunk.contains(index),
                    "highlight index is outside its declared chunk"
                );
                ensure!(selected.insert(*index), "duplicate selected utterance");
            }
        }
        let draft = NotesDraft {
            highlights: selected
                .into_iter()
                .map(|index| {
                    let segment = segments[index];
                    NotesHighlight {
                        segment_id: segment.id.clone(),
                        start_byte: 0,
                        end_byte: segment.text.len(),
                        text: segment.text.clone(),
                    }
                })
                .collect(),
            provenance: completion.provenance,
            created_at: now(),
        };
        self.change_session(session_id, |session| {
            let job = session.notes_job.as_mut().unwrap();
            job.status = NotesJobStatus::Ready;
            job.candidate = Some(draft);
            Ok(())
        })?;
        Ok(true)
    }

    /// Only a matching pending job is cancelled; reviewed and ready snapshots survive.
    pub fn cancel_notes(
        &mut self,
        session_id: &str,
        job_id: &str,
        reason: NotesCancelReason,
    ) -> Result<Session> {
        self.change_session_if(session_id, |session| {
            let changed = session
                .notes_job
                .as_ref()
                .is_some_and(|job| job.id == job_id)
                && cancel_pending(session, reason);
            Ok(((), changed))
        })
        .map(|(_, session)| session)
    }

    pub fn accept_notes(&mut self, session_id: &str, job_id: &str) -> Result<Session> {
        self.change_session_if(session_id, |session| {
            let job = session
                .notes_job
                .as_mut()
                .filter(|job| job.id == job_id)
                .context("highlights job is no longer current")?;
            if job.status == NotesJobStatus::Accepted {
                ensure!(
                    session
                        .generated_notes
                        .as_ref()
                        .is_some_and(|notes| notes.id == job_id),
                    "accepted highlights record is missing"
                );
                return Ok(((), false));
            }
            ensure!(
                job.status == NotesJobStatus::Ready,
                "only a ready draft can be accepted"
            );
            let draft = job
                .candidate
                .take()
                .context("ready highlights draft is missing")?;
            session.generated_notes = Some(GeneratedNotes {
                id: job.id.clone(),
                plan: job.plan.clone(),
                draft,
                accepted_at: now(),
                applied_notes_revision: None,
            });
            job.status = NotesJobStatus::Accepted;
            Ok(((), true))
        })
        .map(|(_, session)| session)
    }

    pub fn discard_notes(&mut self, session_id: &str, job_id: &str) -> Result<Session> {
        self.change_session_if(session_id, |session| {
            let job = session
                .notes_job
                .as_mut()
                .filter(|job| job.id == job_id)
                .context("highlights job is no longer current")?;
            if job.status == NotesJobStatus::Discarded {
                return Ok(((), false));
            }
            ensure!(
                job.status == NotesJobStatus::Ready,
                "only an unreviewed ready draft can be discarded"
            );
            job.candidate = None;
            job.status = NotesJobStatus::Discarded;
            Ok(((), true))
        })
        .map(|(_, session)| session)
    }

    pub fn append_generated_notes(
        &mut self,
        session_id: &str,
        generated_id: &str,
        expected_notes_revision: u64,
    ) -> Result<NotesAppendOutcome> {
        enum Decision {
            AlreadyApplied,
            Conflict,
            Applied,
        }
        let (outcome, session) = self.change_session_if(session_id, |session| {
            let generated = session
                .generated_notes
                .as_ref()
                .filter(|notes| notes.id == generated_id)
                .context("accepted highlights record is no longer current")?;
            if generated.applied_notes_revision.is_some() {
                return Ok((Decision::AlreadyApplied, false));
            }
            if session.notes_revision != expected_notes_revision {
                return Ok((Decision::Conflict, false));
            }
            ensure!(
                !generated.draft.highlights.is_empty(),
                "no highlights were selected to append"
            );
            let markdown = generated.draft.markdown();
            let text = if session.notes.is_empty() {
                markdown
            } else {
                format!("{}\n\n{markdown}", session.notes)
            };
            check_text(&text, "notes", MAX_DOCUMENT_BYTES, true)?;
            write_notes(session, &text)?;
            session
                .generated_notes
                .as_mut()
                .unwrap()
                .applied_notes_revision = Some(session.notes_revision);
            Ok((Decision::Applied, true))
        })?;
        Ok(match outcome {
            Decision::AlreadyApplied => NotesAppendOutcome::AlreadyApplied { session },
            Decision::Conflict => NotesAppendOutcome::Conflict { session },
            Decision::Applied => NotesAppendOutcome::Applied { session },
        })
    }

    pub fn notes_freshness(
        &self,
        session_id: &str,
        source: &NotesSource,
    ) -> Result<NotesFreshness> {
        let session = self.stored_session(session_id)?;
        validate_source(&session, source)?;
        Ok(if source_is_current(&session, source)? {
            NotesFreshness::Current
        } else {
            NotesFreshness::Historical
        })
    }
}

pub(super) fn write_notes(session: &mut Session, text: &str) -> Result<()> {
    if session.notes != text {
        session.notes_revision = session
            .notes_revision
            .checked_add(1)
            .context("notes revision exhausted; export notes before further editing")?;
        session.notes = text.to_owned();
    }
    Ok(())
}

pub(super) fn cancel_pending(session: &mut Session, reason: NotesCancelReason) -> bool {
    if let Some(job) = &mut session.notes_job
        && job.status == NotesJobStatus::Pending
    {
        job.status = NotesJobStatus::Cancelled;
        job.cancel_reason = Some(reason);
        return true;
    }
    false
}

fn source_from_segments(session_id: &str, segments: &[&TranscriptSegment]) -> Result<NotesSource> {
    ensure!(
        !segments.is_empty() && segments.len() <= 10_000,
        "notes need a nonempty bounded finalized transcript"
    );
    let mut text_bytes = 0usize;
    for segment in segments {
        ensure!(
            segment.finalized,
            "notes cannot include a partial transcript"
        );
        check_id(&segment.id)?;
        check_text(
            &segment.text,
            "notes source utterance",
            MAX_TRANSCRIPT_BYTES,
            false,
        )?;
        text_bytes = text_bytes
            .checked_add(segment.text.len())
            .context("notes source size overflow")?;
        ensure!(
            text_bytes <= NOTES_MAX_SOURCE_BYTES,
            "finalized transcript exceeds notes capacity of 128 KiB; no suffix is selected"
        );
    }
    // Digest v1 is SHA256 of this exact ordered JSON tuple; text is never normalized.
    let bytes = serde_json::to_vec(&(
        NOTES_DIGEST_VERSION,
        session_id,
        segments
            .iter()
            .map(|segment| (&segment.id, &segment.text, segment.speaker, segment.source))
            .collect::<Vec<_>>(),
    ))?;
    Ok(NotesSource {
        session_id: session_id.to_owned(),
        digest_version: NOTES_DIGEST_VERSION,
        segment_ids: segments.iter().map(|segment| segment.id.clone()).collect(),
        text_bytes,
        sha256: format!("{:x}", Sha256::digest(bytes)),
    })
}

fn validate_source<'a>(
    session: &'a Session,
    source: &NotesSource,
) -> Result<Vec<&'a TranscriptSegment>> {
    ensure!(
        source.session_id == session.id && source.digest_version == NOTES_DIGEST_VERSION,
        "notes source belongs to another session or digest version"
    );
    ensure!(
        !source.segment_ids.is_empty() && source.segment_ids.len() <= 10_000,
        "invalid notes source segment count"
    );
    let positions: HashMap<_, _> = session
        .transcript
        .iter()
        .enumerate()
        .map(|(index, segment)| (segment.id.as_str(), (index, segment)))
        .collect();
    let mut segments = Vec::with_capacity(source.segment_ids.len());
    let mut previous = None;
    for id in &source.segment_ids {
        let (index, segment) = positions
            .get(id.as_str())
            .context("notes refer to a missing transcript segment")?;
        ensure!(
            previous.is_none_or(|previous| previous < *index),
            "notes source IDs are duplicated or out of order"
        );
        previous = Some(*index);
        segments.push(*segment);
    }
    ensure!(
        &source_from_segments(&session.id, &segments)? == source,
        "notes source digest or byte count does not match the exact transcript"
    );
    Ok(segments)
}

fn source_is_current(session: &Session, source: &NotesSource) -> Result<bool> {
    if session.status != SessionStatus::Stopped {
        return Ok(false);
    }
    let segments: Vec<_> = session.transcript.iter().collect();
    if segments
        .iter()
        .map(|segment| segment.text.len())
        .sum::<usize>()
        > NOTES_MAX_SOURCE_BYTES
    {
        return Ok(false);
    }
    Ok(&source_from_segments(&session.id, &segments)? == source)
}

fn validate_plan<'a>(session: &'a Session, plan: &NotesPlan) -> Result<Vec<&'a TranscriptSegment>> {
    let segments = validate_source(session, &plan.source)?;
    ensure!(
        !plan.chunks.is_empty() && plan.chunks.len() <= NOTES_MAX_CHUNKS,
        "notes coverage must declare 1 to 16 chunks"
    );
    let mut expected = 0usize;
    for chunk in &plan.chunks {
        ensure!(!chunk.is_empty(), "notes coverage contains an empty chunk");
        for index in chunk {
            ensure!(
                *index == expected && *index < segments.len(),
                "notes coverage has gaps, duplicates, reordered or nonexistent utterances"
            );
            expected += 1;
        }
    }
    ensure!(
        expected == segments.len(),
        "notes coverage omits finalized utterances"
    );
    Ok(segments)
}

fn validate_provenance(provenance: &NotesProvenance, chunks: usize) -> Result<()> {
    if let NotesProvenance::Local { runs } = provenance {
        ensure!(
            runs.len() == chunks,
            "local notes metadata must cover every declared chunk"
        );
        for run in runs {
            validate_inference(run)?;
            ensure!(
                run.finish_reason == "end_of_turn"
                    && run.omitted_transcript_segments == 0
                    && run.omitted_chat_turns == 0
                    && !run.screen_truncated,
                "local highlights cannot be cancelled, truncated or omit input"
            );
            ensure!(
                run.prompt_sha256.is_some() && run.template_revision.is_some(),
                "local highlights need exact prompt/template identities"
            );
            let first = &runs[0];
            ensure!(
                run.model_profile == first.model_profile
                    && run.model_revision == first.model_revision
                    && run.model_sha256 == first.model_sha256
                    && run.tokenizer_sha256 == first.tokenizer_sha256
                    && run.backend == first.backend
                    && run.template_revision == first.template_revision
                    && run.decoder_policy_revision == first.decoder_policy_revision,
                "local notes chunks use inconsistent model or policy identities"
            );
        }
    }
    Ok(())
}

fn validate_draft(session: &Session, plan: &NotesPlan, draft: &NotesDraft) -> Result<()> {
    let segments = validate_plan(session, plan)?;
    validate_provenance(&draft.provenance, plan.chunks.len())?;
    ensure!(
        draft.created_at >= 0
            && draft.highlights.len() <= NOTES_MAX_SELECTIONS_PER_CHUNK * plan.chunks.len(),
        "invalid notes draft size or timestamp"
    );
    let mut per_chunk = vec![0usize; plan.chunks.len()];
    let mut previous = None;
    for highlight in &draft.highlights {
        let index = segments
            .iter()
            .position(|segment| segment.id == highlight.segment_id)
            .context("highlight is outside the frozen notes source")?;
        ensure!(
            previous.is_none_or(|previous| previous < index),
            "highlights are duplicated or out of source order"
        );
        previous = Some(index);
        let segment = segments[index];
        ensure!(
            highlight.start_byte == 0
                && highlight.end_byte == segment.text.len()
                && highlight.text == segment.text,
            "highlight must preserve the exact whole utterance and byte span"
        );
        let chunk = plan
            .chunks
            .iter()
            .position(|chunk| chunk.contains(&index))
            .unwrap();
        per_chunk[chunk] += 1;
        ensure!(
            per_chunk[chunk] <= NOTES_MAX_SELECTIONS_PER_CHUNK,
            "too many restored highlights in one chunk"
        );
    }
    Ok(())
}

pub(super) fn validate_session_notes(session: &Session) -> Result<()> {
    if let Some(generated) = &session.generated_notes {
        check_id(&generated.id)?;
        validate_draft(session, &generated.plan, &generated.draft)?;
        ensure!(
            generated.accepted_at >= 0,
            "invalid highlights acceptance timestamp"
        );
        if let Some(revision) = generated.applied_notes_revision {
            ensure!(
                revision > 0 && revision <= session.notes_revision,
                "invalid applied highlights revision"
            );
        }
    }
    if let Some(job) = &session.notes_job {
        check_id(&job.id)?;
        validate_plan(session, &job.plan)?;
        ensure!(job.created_at >= 0, "invalid notes job timestamp");
        ensure!(
            (job.status == NotesJobStatus::Cancelled) == job.cancel_reason.is_some(),
            "inconsistent notes cancellation state"
        );
        ensure!(
            (job.status == NotesJobStatus::Ready) == job.candidate.is_some(),
            "inconsistent notes candidate state"
        );
        if job.status == NotesJobStatus::Pending {
            ensure!(
                source_is_current(session, &job.plan.source)?,
                "pending notes source is no longer current"
            );
        }
        if let Some(candidate) = &job.candidate {
            validate_draft(session, &job.plan, candidate)?;
        }
        if job.status == NotesJobStatus::Accepted {
            ensure!(
                session
                    .generated_notes
                    .as_ref()
                    .is_some_and(|generated| generated.id == job.id && generated.plan == job.plan),
                "accepted notes job does not match its saved record"
            );
        } else {
            ensure!(
                !session
                    .generated_notes
                    .as_ref()
                    .is_some_and(|generated| generated.id == job.id),
                "unaccepted job reuses an accepted record identity"
            );
        }
    }
    Ok(())
}

pub(super) fn append_export(session: &Session, output: &mut String) -> Result<()> {
    validate_session_notes(session)?;
    if let Some(generated) = &session.generated_notes {
        output.push_str("\n## Accepted extractive highlights\n\n");
        append_provenance(&generated.draft.provenance, output);
        output.push_str(if source_is_current(session, &generated.plan.source)? {
            "Source: current finalized transcript.\n\n"
        } else {
            "Source: historical transcript snapshot.\n\n"
        });
        output.push_str(&generated.draft.markdown());
    }
    if let Some(job) = &session.notes_job
        && let Some(candidate) = &job.candidate
    {
        output.push_str("\n## Unreviewed extractive highlights draft\n\n");
        append_provenance(&candidate.provenance, output);
        output.push_str(if source_is_current(session, &job.plan.source)? {
            "Source: current finalized transcript.\n\n"
        } else {
            "Source: historical transcript snapshot.\n\n"
        });
        output.push_str(&candidate.markdown());
    }
    Ok(())
}

fn append_provenance(provenance: &NotesProvenance, output: &mut String) {
    output.push_str(match provenance {
        NotesProvenance::Supplied {} => "Supplied utterance selection; no model execution is claimed.\n\n",
        NotesProvenance::Local { .. } => "Local extractive selection; this is not an abstractive summary or a quality guarantee.\n\n",
    });
}
