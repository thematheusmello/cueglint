use super::*;
use sha2::{Digest, Sha256};

const MEETING_SEARCH_LIMIT: i64 = 64;
const SEARCH_FIELD_BYTES: usize = 16 * 1024;
const SEARCH_TRANSCRIPT_BYTES: usize = 64 * 1024;
const EXCERPT_CHARS: usize = 256;
const EXCERPTS_PER_MEETING: usize = 2;
const EXCERPT_LIMIT: usize = 4;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MeetingContext {
    Excerpts(Vec<MeetingExcerpt>),
    Withdrawn,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MeetingExcerpt {
    pub meeting_id: String,
    pub title: String,
    pub created_at: i64,
    pub source: MeetingExcerptSource,
    pub start_byte: usize,
    pub end_byte: usize,
    pub revision_sha256: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MeetingExcerptSource {
    Notes,
    Summary,
    Transcript(String),
}

impl MeetingExcerptSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Notes => "Notes",
            Self::Summary => "Summary",
            Self::Transcript(_) => "Transcript",
        }
    }

    fn original<'a>(&self, meeting: &'a Session) -> Option<&'a str> {
        match self {
            Self::Notes => Some(&meeting.notes),
            Self::Summary => meeting.summary.as_deref(),
            Self::Transcript(id) => meeting
                .transcript
                .iter()
                .find(|segment| &segment.id == id && segment.finalized)
                .map(|segment| segment.text.as_str()),
        }
    }
}

impl MeetingExcerpt {
    fn matches_source(&self, meeting: &Session) -> bool {
        meeting.saved_at.is_some()
            && !meeting.history_chat
            && self.meeting_id == meeting.id
            && self.created_at == meeting.created_at
            && self
                .source
                .original(meeting)
                .and_then(|original| original.get(self.start_byte..self.end_byte))
                == Some(self.text.as_str())
    }
}

fn revision(title: &str, text: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(title.as_bytes());
    hash.update([0]);
    hash.update(text.as_bytes());
    format!("{:x}", hash.finalize())
}

fn terms(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().take(2).count() == 2)
}

fn hits(text: &str, question: &BTreeSet<String>) -> usize {
    terms(text)
        .map(str::to_lowercase)
        .filter(|word| question.contains(word))
        .collect::<BTreeSet<_>>()
        .len()
}

pub(super) fn retrieve(store: &Store, prompt: &str) -> Result<Vec<MeetingExcerpt>> {
    let question: BTreeSet<String> = terms(prompt)
        .map(str::to_lowercase)
        .filter(|word| {
            !matches!(
                word.as_str(),
                "about"
                    | "and"
                    | "are"
                    | "can"
                    | "did"
                    | "does"
                    | "for"
                    | "from"
                    | "had"
                    | "have"
                    | "how"
                    | "in"
                    | "is"
                    | "it"
                    | "my"
                    | "of"
                    | "on"
                    | "our"
                    | "the"
                    | "to"
                    | "was"
                    | "we"
                    | "what"
                    | "when"
                    | "where"
                    | "which"
                    | "who"
                    | "with"
            )
        })
        .take(48)
        .collect();
    if question.is_empty() {
        return Ok(Vec::new());
    }
    let mut statement = store.connection.prepare(
        "SELECT data FROM sessions
         WHERE json_extract(data, '$.saved_at') IS NOT NULL
           AND json_extract(data, '$.status') = 'stopped'
           AND COALESCE(json_extract(data, '$.history_chat'), 0) = 0
         ORDER BY json_extract(data, '$.saved_at') DESC, id LIMIT ?1",
    )?;
    let mut rows = statement.query([MEETING_SEARCH_LIMIT])?;
    let mut candidates = Vec::new();
    while let Some(row) = rows.next()? {
        let meeting: Session = serde_json::from_str(&row.get::<_, String>(0)?)?;
        let title_hits = hits(&meeting.title, &question);
        // Keep the candidate text borrowed while ranking. Only the best passages are copied.
        let mut passages = Vec::new();
        let mut add_field = |source: MeetingExcerptSource, text: &str| {
            let text = &text[..text.floor_char_boundary(SEARCH_FIELD_BYTES)];
            for (start, end) in text_ranges(text, EXCERPT_CHARS) {
                let passage = text[start..end].trim_end();
                let trimmed = passage.trim_start();
                if trimmed.is_empty() {
                    continue;
                }
                let score = 2 * hits(trimmed, &question) + title_hits;
                if score > 0 {
                    passages.push((
                        score,
                        source.clone(),
                        start + passage.len() - trimmed.len(),
                        start + passage.len(),
                    ));
                }
            }
        };
        add_field(MeetingExcerptSource::Notes, &meeting.notes);
        if let Some(summary) = &meeting.summary {
            add_field(MeetingExcerptSource::Summary, summary);
        }
        let mut remaining = SEARCH_TRANSCRIPT_BYTES;
        for segment in meeting
            .transcript
            .iter()
            .rev()
            .filter(|part| part.finalized)
        {
            if remaining == 0 {
                break;
            }
            let count = segment.text.floor_char_boundary(remaining);
            add_field(
                MeetingExcerptSource::Transcript(segment.id.clone()),
                &segment.text[..count],
            );
            remaining -= count.max(1);
        }
        passages.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        for (score, source, start_byte, end_byte) in passages.into_iter().take(EXCERPTS_PER_MEETING)
        {
            let original = source
                .original(&meeting)
                .context("missing meeting source")?;
            let text = original[start_byte..end_byte].to_owned();
            candidates.push((
                score,
                meeting.saved_at,
                MeetingExcerpt {
                    meeting_id: meeting.id.clone(),
                    title: meeting.title.clone(),
                    created_at: meeting.created_at,
                    source,
                    start_byte,
                    end_byte,
                    revision_sha256: revision(&meeting.title, &text),
                    text,
                },
            ));
        }
    }
    candidates.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.1.cmp(&a.1))
            .then(a.2.meeting_id.cmp(&b.2.meeting_id))
            .then(a.2.source.cmp(&b.2.source))
            .then(a.2.start_byte.cmp(&b.2.start_byte))
    });
    Ok(candidates
        .into_iter()
        .take(EXCERPT_LIMIT)
        .map(|(_, _, excerpt)| excerpt)
        .collect())
}

/// Update dependent exchanges in the source transaction, including stored copies in follow-up contexts.
pub(super) fn withdraw_affected(
    tx: &rusqlite::Transaction<'_>,
    source_id: &str,
    current: Option<&Session>,
) -> Result<()> {
    let mut statement = tx.prepare(
        "SELECT id FROM sessions
         WHERE json_extract(data, '$.history_chat') = 1
           AND EXISTS (
               SELECT 1 FROM json_each(sessions.data, '$.requests') request,
                   json_each(request.value, '$.context.past_meetings.excerpts') excerpt
               WHERE json_extract(excerpt.value, '$.meeting_id') = ?1
           )",
    )?;
    let candidates = statement
        .query_map([source_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in candidates {
        let data: String = tx.query_row("SELECT data FROM sessions WHERE id=?1", [&id], |row| {
            row.get(0)
        })?;
        let mut chat: Session = serde_json::from_str(&data)?;
        let invalidated = chat.requests.iter().any(|request| {
            matches!(&request.context.past_meetings, Some(MeetingContext::Excerpts(excerpts))
                if excerpts.iter().any(|excerpt| excerpt.meeting_id == source_id
                    && current.is_none_or(|source| !excerpt.matches_source(source))))
        });
        if invalidated {
            advance_context_revision(&mut chat)?;
            chat.active_request_id = None;
            chat.suggestions.clear();
            for request in &mut chat.requests {
                request.status = RequestStatus::Cancelled;
                request.context.past_meetings = Some(MeetingContext::Withdrawn);
                request.context.chat.clear();
                request.context.omitted_chat_turns = 0;
                request.context.diagnostics.clear();
            }
            chat.updated_at = now();
            tx.execute(
                "UPDATE sessions SET data=?1 WHERE id=?2",
                params![serde_json::to_string(&chat)?, chat.id],
            )?;
        }
    }
    Ok(())
}

/// Recheck the source strings when a follow-up copies an earlier answer's excerpts.
pub(super) fn ensure_sources_current(store: &Store, excerpts: &[MeetingExcerpt]) -> Result<()> {
    for excerpt in excerpts {
        let source = store.stored_session(&excerpt.meeting_id)?;
        ensure!(
            excerpt.matches_source(&source),
            "The saved meeting excerpt is no longer current. Try a distinctive project name or term."
        );
    }
    Ok(())
}

pub(super) fn validate_request(
    session: &Session,
    request: &GenerationRequest,
    sources: &HashMap<&str, &Session>,
) -> Result<()> {
    ensure!(
        session.history_chat == request.context.past_meetings.is_some(),
        "request meeting context does not match its conversation"
    );
    let Some(context) = &request.context.past_meetings else {
        return Ok(());
    };
    ensure!(
        request.provider == ProviderConfig::default()
            && request.context.screen.is_none()
            && request.context.transcript.is_empty()
            && request.context.knowledge.is_empty(),
        "saved meeting requests use a local model and saved excerpts only"
    );
    let MeetingContext::Excerpts(excerpts) = context else {
        ensure!(
            request.status == RequestStatus::Cancelled && request.context.chat.is_empty(),
            "withdrawn meeting context must have no active request or previous response text"
        );
        return Ok(());
    };
    ensure!(
        (1..=EXCERPT_LIMIT).contains(&excerpts.len()),
        "invalid saved meeting excerpt count"
    );
    let mut seen = BTreeSet::new();
    let mut per_meeting = HashMap::new();
    for excerpt in excerpts {
        ensure!(
            seen.insert((&excerpt.meeting_id, &excerpt.source, excerpt.start_byte)),
            "duplicate saved meeting excerpt"
        );
        let count = per_meeting.entry(excerpt.meeting_id.as_str()).or_insert(0);
        *count += 1;
        ensure!(
            *count <= EXCERPTS_PER_MEETING,
            "too many excerpts from one meeting"
        );
        let source = sources
            .get(excerpt.meeting_id.as_str())
            .context("saved meeting excerpt references a missing meeting")?;
        check_text(
            &excerpt.text,
            "saved meeting excerpt",
            4 * EXCERPT_CHARS,
            false,
        )?;
        check_text(&excerpt.title, "saved meeting title", 200, false)?;
        ensure!(
            excerpt.text.chars().count() <= EXCERPT_CHARS
                && excerpt.matches_source(source)
                && excerpt.revision_sha256 == revision(&excerpt.title, &excerpt.text),
            "saved meeting excerpt does not match its source revision"
        );
    }
    Ok(())
}
