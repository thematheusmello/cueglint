use super::*;
use crate::runtime::HighlightsPhase;
use crate::worker::{HighlightsAck, HighlightsAction, HighlightsCommand};
use cueglint_core::{
    NotesAppendOutcome, NotesDraft, NotesFreshness, NotesJobStatus, NotesProvenance,
};

#[derive(Default)]
pub(super) struct HighlightsUi {
    pub open: bool,
    pub pending: Option<HighlightsCommand>,
    pub append_after_notes: Option<(String, String)>,
    pub conflict: bool,
}

impl Cueglint {
    pub(super) fn highlights_busy(&self) -> bool {
        self.runtime.highlights.as_ref().is_some_and(|status| {
            matches!(
                status.phase,
                HighlightsPhase::Preparing
                    | HighlightsPhase::Generating
                    | HighlightsPhase::Cancelling
            )
        })
    }

    pub(super) fn highlights_appending(&self) -> bool {
        self.highlights
            .pending
            .as_ref()
            .is_some_and(|command| matches!(command.action, HighlightsAction::Append { .. }))
    }

    pub(super) fn send_highlights(&mut self, session: &str, action: HighlightsAction) {
        if self.highlights.pending.is_some() || self.quitting {
            return;
        }
        let command = HighlightsCommand {
            id: uuid::Uuid::new_v4().to_string(),
            session: session.to_owned(),
            action,
        };
        if self.send(Command::Highlights(command.clone())) {
            self.highlights.pending = Some(command);
        }
    }

    pub(super) fn acknowledge_highlights(&mut self, ack: HighlightsAck) {
        let expected = Acknowledgement::Highlights(ack.command.clone());
        let Some(index) = self
            .acknowledgement_kinds
            .iter()
            .position(|kind| *kind == expected)
        else {
            return;
        };
        self.acknowledgement_kinds.remove(index);
        self.pending = self.pending.saturating_sub(1);
        if self.highlights.pending.as_ref() != Some(&ack.command) {
            return;
        }
        self.highlights.pending = None;
        if self.selected().map(|session| session.id.as_str()) != Some(ack.command.session.as_str())
        {
            return;
        }
        match ack.result {
            Ok((state, outcome)) => {
                let conflict = matches!(outcome, Some(NotesAppendOutcome::Conflict { .. }));
                let review = ack.command.action == HighlightsAction::Review;
                self.apply_snapshot(state, None, false);
                if conflict {
                    self.highlights.conflict = true;
                    self.notice = Some(("Notes changed before append. Your notes were preserved. Review the current notes before trying again.".into(), true));
                } else if review {
                    self.highlights.conflict = false;
                }
            }
            Err(error) => {
                self.notice = Some((format!("Highlights: {error}"), true));
            }
        }
    }

    pub(super) fn reconcile_highlights(&mut self) {
        let Some((session, generated)) = self.highlights.append_after_notes.clone() else {
            return;
        };
        if self.selected().map(|selected| selected.id.as_str()) != Some(session.as_str())
            || self.quitting
            || self.notes_failed
        {
            self.highlights.append_after_notes = None;
            return;
        }
        if self.notes_dirty {
            if !self.notes_saving {
                self.save_notes();
            }
            return;
        }
        if self.notes_saving || self.pending != 0 || self.highlights_busy() {
            return;
        }
        let revision = self.selected().unwrap().notes_revision;
        self.send_highlights(
            &session,
            HighlightsAction::Append {
                generated,
                expected_notes_revision: revision,
            },
        );
        if self.highlights_appending() {
            self.highlights.append_after_notes = None;
        }
    }

    pub(super) fn drain_after_notes(&mut self) {
        if self.after_notes.is_some()
            && self.highlights_busy()
            && !self
                .runtime
                .highlights
                .as_ref()
                .is_some_and(|status| status.phase == HighlightsPhase::Cancelling)
            && !self
                .acknowledgement_kinds
                .contains(&Acknowledgement::Cancel)
        {
            self.send(Command::Cancel);
        }
        if !self.notes_dirty
            && !self.notes_saving
            && !self.highlights_busy()
            && self.highlights.pending.is_none()
            && self.highlights.append_after_notes.is_none()
            && let Some(command) = self.after_notes.take()
        {
            self.send(command);
        }
    }

    pub(super) fn request_highlights_append(&mut self, session: &str, generated: &str) {
        if self.highlights.pending.is_none() && !self.highlights.conflict && !self.quitting {
            self.highlights.append_after_notes = Some((session.to_owned(), generated.to_owned()));
            self.reconcile_highlights();
        }
    }

    pub(super) fn highlights_ui(&mut self, ctx: &egui::Context, session: &Session) {
        if !self.highlights.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Transcript highlights")
            .id(Id::new("session-highlights"))
            .open(&mut open)
            .default_width(480.0)
            .default_height(380.0)
            .show(ctx, |ui| {
                ui.label("Selects original finalized utterances using the local answer model. This is an extractive draft, not an automatic summary.");
                ui.label(RichText::new("Review carefully. Nothing is appended to your notes automatically.").color(MUTED));
                let busy = self.highlights_busy();
                let pending = self.highlights.pending.is_some() || self.highlights.append_after_notes.is_some();
                if let Some(status) = &self.runtime.highlights
                    && status.binding.session_id == session.id
                {
                    let text = match status.phase {
                        HighlightsPhase::Preparing => "Preparing the complete transcript…".to_owned(),
                        HighlightsPhase::Generating => format!("Selecting highlights: {} / {} chunks", status.completed_chunks, status.total_chunks),
                        HighlightsPhase::Cancelling => "Cancelling; waiting for the inference worker…".to_owned(),
                        HighlightsPhase::Ready => "Draft ready for review.".to_owned(),
                        HighlightsPhase::Cancelled => "Generation cancelled. No partial draft was published.".to_owned(),
                        HighlightsPhase::Failed => "Generation failed. No partial draft was published.".to_owned(),
                    };
                    ui.label(text);
                }
                ui.horizontal(|ui| {
                    let unresolved = session.notes_job.as_ref().is_some_and(|job| matches!(job.status, NotesJobStatus::Pending | NotesJobStatus::Ready));
                    let enabled = !pending && !busy && !unresolved && self.fixture.is_none() && session.status == SessionStatus::Stopped && !session.transcript.is_empty() && !self.capture_busy() && !self.model_job_busy() && !self.generating() && !self.quitting;
                    if ui.add_enabled(enabled, egui::Button::new("Generate locally")).clicked() { self.send_highlights(&session.id, HighlightsAction::Generate); }
                    if ui.add_enabled(busy && !pending, egui::Button::new("Cancel generation")).clicked() { self.send_highlights(&session.id, HighlightsAction::Cancel); }
                });
                if session.status != SessionStatus::Stopped { ui.label("Stop and finalize this session before generating highlights."); }
                if !self.runtime.installed_profiles.contains(&self.runtime.generation_profile) { ui.label("Install the selected local answer model in Settings first."); }
                if self.highlights.conflict {
                    ui.label("Append paused because notes changed. Review the current editor text, then refresh before appending.");
                    if ui.add_enabled(!pending, egui::Button::new("I reviewed the current notes; refresh")).clicked() { self.send_highlights(&session.id, HighlightsAction::Review); }
                }
                egui::ScrollArea::vertical().max_height(330.0).show(ui, |ui| {
                    if let Some(job) = &session.notes_job
                        && job.status == NotesJobStatus::Ready
                        && let Some(draft) = &job.candidate
                    {
                        ui.separator();
                        ui.strong("Draft");
                        freshness(ui, self.state.as_ref().and_then(|state| state.draft_freshness));
                        draft_ui(ui, draft);
                        ui.horizontal(|ui| {
                            if ui.add_enabled(!pending, egui::Button::new("Accept draft")).clicked() { self.send_highlights(&session.id, HighlightsAction::Accept(job.id.clone())); }
                            if ui.add_enabled(!pending, egui::Button::new("Discard draft")).clicked() { self.send_highlights(&session.id, HighlightsAction::Discard(job.id.clone())); }
                        });
                    }
                    if let Some(notes) = &session.generated_notes {
                        ui.separator();
                        ui.strong("Accepted highlights");
                        freshness(ui, self.state.as_ref().and_then(|state| state.accepted_freshness));
                        draft_ui(ui, &notes.draft);
                        if notes.applied_notes_revision.is_some() { ui.label("Already appended. Your later note edits are preserved."); }
                        if ui.add_enabled(!pending && !self.highlights.conflict && notes.applied_notes_revision.is_none() && !self.quitting, egui::Button::new("Append to notes")).clicked() { self.request_highlights_append(&session.id, &notes.id); }
                    }
                });
            });
        self.highlights.open = open;
    }
}

fn freshness(ui: &mut Ui, freshness: Option<NotesFreshness>) {
    ui.label(match freshness {
        Some(NotesFreshness::Current) => "Source: complete current transcript.",
        Some(NotesFreshness::Historical) => {
            "Historical source: the transcript has changed since this draft."
        }
        None => "Stored source-bound draft.",
    });
}

fn draft_ui(ui: &mut Ui, draft: &NotesDraft) {
    ui.label(match &draft.provenance {
        NotesProvenance::Local { .. } => {
            "Selected with a local model; model quality has not been evaluated."
        }
        NotesProvenance::Supplied {} => "Supplied selections; no model execution is claimed.",
    });
    for highlight in &draft.highlights {
        ui.group(|ui| {
            ui.label(&highlight.text);
            ui.small(format!(
                "Source {} · bytes {}–{}",
                highlight.segment_id, highlight.start_byte, highlight.end_byte
            ));
        });
    }
    if draft.highlights.is_empty() {
        ui.label("No highlights were selected.");
    }
    if ui.small_button("Copy highlights").clicked() {
        ui.ctx().copy_text(draft.markdown());
    }
}
