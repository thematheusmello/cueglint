use super::*;
use cueglint_core::{NotesAppendOutcome, NotesCancelReason};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightsCommand {
    pub id: String,
    pub session: String,
    pub action: HighlightsAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HighlightsAction {
    Review,
    Generate,
    Cancel,
    Accept(String),
    Discard(String),
    Append {
        generated: String,
        expected_notes_revision: u64,
    },
}

pub struct HighlightsAck {
    pub command: HighlightsCommand,
    pub result: Result<(Snapshot, Option<NotesAppendOutcome>), String>,
}

pub(super) fn execute_highlights(
    store: &mut Store,
    selected: Option<&str>,
    runtime: &mut Runtime,
    command: &HighlightsCommand,
) -> Result<Option<NotesAppendOutcome>> {
    anyhow::ensure!(
        selected == Some(command.session.as_str()),
        "this session is no longer selected"
    );
    anyhow::ensure!(
        !command.id.is_empty() && command.id.len() <= 100,
        "invalid highlights operation"
    );
    match &command.action {
        HighlightsAction::Review => {
            store.session(&command.session)?;
        }
        HighlightsAction::Generate => {
            runtime.start_highlights(store, command.id.clone(), &command.session)?
        }
        HighlightsAction::Cancel => runtime.cancel_highlights(store, NotesCancelReason::User)?,
        HighlightsAction::Accept(job) => {
            store.accept_notes(&command.session, job)?;
        }
        HighlightsAction::Discard(job) => {
            store.discard_notes(&command.session, job)?;
        }
        HighlightsAction::Append {
            generated,
            expected_notes_revision,
        } => {
            return store
                .append_generated_notes(&command.session, generated, *expected_notes_revision)
                .map(Some);
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_fixture_worker_keeps_typed_highlights_errors_separate_from_manual_notes_ack() {
        let worker = Worker::spawn(None, Some("session".into()), egui::Context::default());
        let Response::State(initial, _) = worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("initial fixture state")
        };
        let session = initial.selected.unwrap();
        let wrong = HighlightsCommand {
            id: "original-wrong-session-operation".into(),
            session: "unselected-session".into(),
            action: HighlightsAction::Review,
        };
        assert!(
            worker
                .commands
                .send(Command::Highlights(wrong.clone()))
                .is_ok()
        );
        let notes = "Exact human café notes\r\nNo automatic replacement.";
        assert!(
            worker
                .commands
                .send(Command::Notes(session.id.clone(), notes.into()))
                .is_ok()
        );
        let Response::Highlights(error) = worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("highlights error needs its typed ACK")
        };
        assert_eq!(error.command, wrong);
        assert!(error.result.err().unwrap().contains("no longer selected"));
        let Response::State(saved, _) = worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("manual Notes needs its ordinary ACK")
        };
        assert_eq!(saved.selected.unwrap().notes, notes);
        let review = HighlightsCommand {
            id: "original-review-operation".into(),
            session: session.id,
            action: HighlightsAction::Review,
        };
        assert!(
            worker
                .commands
                .send(Command::Highlights(review.clone()))
                .is_ok()
        );
        let Response::Highlights(ack) = worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("review needs its typed ACK")
        };
        assert_eq!(ack.command, review);
        let (state, outcome) = ack.result.unwrap();
        assert!(outcome.is_none());
        let selected = state.selected.unwrap();
        assert_eq!(selected.notes, notes);
        assert_eq!(selected.summary, session.summary);
        assert!(selected.notes_job.is_none() && selected.generated_notes.is_none());
    }
}
