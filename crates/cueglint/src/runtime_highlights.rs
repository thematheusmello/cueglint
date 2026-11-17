use super::*;
use cueglint_core::{NotesCancelReason, NotesCompletion, NotesInput, NotesPlan};
use cueglint_local::notes::{NotesProgress, PreparedNotes};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightsBinding {
    pub operation_id: String,
    pub session_id: String,
    pub profile: String,
    pub root: PathBuf,
    pub source_sha256: String,
    pub job_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HighlightsPhase {
    Preparing,
    Generating,
    Cancelling,
    Ready,
    Cancelled,
    Failed,
}

#[derive(Clone)]
pub struct HighlightsStatus {
    pub binding: HighlightsBinding,
    pub phase: HighlightsPhase,
    pub completed_chunks: usize,
    pub total_chunks: usize,
}

pub(super) struct HighlightsOperation {
    binding: HighlightsBinding,
    pub(super) cancel: Arc<AtomicBool>,
    retired: bool,
}

pub(super) struct PrepareHighlights {
    pub binding: HighlightsBinding,
    pub input: NotesInput,
    pub cancel: Arc<AtomicBool>,
}

pub(super) struct RunHighlights {
    pub binding: HighlightsBinding,
    pub cancel: Arc<AtomicBool>,
}

pub(super) struct PreparedHighlights {
    binding: HighlightsBinding,
    cancel: Arc<AtomicBool>,
    prepared: PreparedNotes,
}

pub(super) fn prepare(
    model: &mut Option<(PathBuf, String, LocalGenerator)>,
    job: PrepareHighlights,
) -> Result<(PreparedHighlights, NotesPlan)> {
    ensure!(!job.cancel.load(Ordering::Relaxed), "highlights cancelled");
    if model.as_ref().is_none_or(|(root, profile, _)| {
        root != &job.binding.root || profile != &job.binding.profile
    }) {
        *model = None;
        let loaded = LocalGenerator::load_profile_with_cancel(
            &job.binding.root,
            &job.binding.profile,
            &job.cancel,
        )?;
        *model = Some((
            job.binding.root.clone(),
            job.binding.profile.clone(),
            loaded,
        ));
    }
    let prepared = model
        .as_ref()
        .unwrap()
        .2
        .plan_notes(&job.input, &job.cancel)?;
    let plan = prepared.plan().clone();
    Ok((
        PreparedHighlights {
            binding: job.binding,
            cancel: job.cancel,
            prepared,
        },
        plan,
    ))
}

pub(super) fn run(
    model: &mut Option<(PathBuf, String, LocalGenerator)>,
    prepared: Option<PreparedHighlights>,
    job: RunHighlights,
    on_progress: impl FnMut(NotesProgress),
) -> Result<NotesCompletion> {
    ensure!(!job.cancel.load(Ordering::Relaxed), "highlights cancelled");
    let prepared = prepared.context("prepared highlights are no longer available")?;
    let mut expected = prepared.binding;
    expected.job_id = job.binding.job_id.clone();
    ensure!(
        expected == job.binding
            && job.binding.job_id.is_some()
            && Arc::ptr_eq(&prepared.cancel, &job.cancel),
        "highlights operation changed before generation"
    );
    let (root, profile, model) = model.as_mut().context("highlights model was reset")?;
    ensure!(
        root == &job.binding.root && profile == &job.binding.profile,
        "highlights model changed after preparation"
    );
    model.generate_notes(&prepared.prepared, &job.cancel, on_progress)
}

impl Runtime {
    pub fn highlights_busy(&self) -> bool {
        self.highlights_operation.is_some()
    }

    pub fn start_highlights(
        &mut self,
        store: &Store,
        operation_id: String,
        session: &str,
    ) -> Result<()> {
        ensure!(
            !self.fixture,
            "highlight generation is disabled in deterministic fixtures"
        );
        ensure!(
            !operation_id.is_empty() && operation_id.len() <= 100,
            "invalid highlights operation"
        );
        ensure!(
            !self.highlights_busy(),
            "wait for highlights work to finish or cancel"
        );
        ensure!(
            self.capture.is_none(),
            "stop and finalize capture before generating highlights"
        );
        ensure!(
            self.install_id.is_none(),
            "wait for the model file operation to finish"
        );
        ensure!(
            self.provider_job.is_none(),
            "wait for the provider operation to finish"
        );
        ensure!(
            !self
                .snapshot
                .generation
                .as_ref()
                .is_some_and(|status| matches!(
                    status.state.as_str(),
                    "generating" | "capturing screen"
                )),
            "finish or cancel the answer before generating highlights"
        );
        ensure!(
            self.snapshot
                .installed_profiles
                .contains(&self.snapshot.generation_profile),
            "install the selected local answer model before generating highlights"
        );
        let current = store.session(session)?;
        ensure!(
            !current.notes_job.as_ref().is_some_and(|job| matches!(
                job.status,
                cueglint_core::NotesJobStatus::Pending | cueglint_core::NotesJobStatus::Ready
            )),
            "accept or discard the current highlights draft first"
        );
        let input = store.notes_input(session)?;
        let binding = HighlightsBinding {
            operation_id,
            session_id: session.to_owned(),
            profile: self.snapshot.generation_profile.clone(),
            root: self.snapshot.model_root.clone(),
            source_sha256: input.source.sha256.clone(),
            job_id: None,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.generator
            .try_send(InferenceCommand::PrepareHighlights(PrepareHighlights {
                binding: binding.clone(),
                input,
                cancel: cancel.clone(),
            }))
            .map_err(|_| {
                anyhow::anyhow!("inference queue is full; no highlights job was created")
            })?;
        self.snapshot.highlights = Some(HighlightsStatus {
            binding: binding.clone(),
            phase: HighlightsPhase::Preparing,
            completed_chunks: 0,
            total_chunks: 0,
        });
        self.highlights_operation = Some(HighlightsOperation {
            binding,
            cancel,
            retired: false,
        });
        Ok(())
    }

    pub fn cancel_highlights(
        &mut self,
        store: &mut Store,
        reason: NotesCancelReason,
    ) -> Result<()> {
        if let Some(operation) = &mut self.highlights_operation {
            operation.cancel.store(true, Ordering::Relaxed);
            operation.retired = true;
            if let Some(status) = &mut self.snapshot.highlights {
                status.phase = HighlightsPhase::Cancelling;
            }
            if let Some(job) = &operation.binding.job_id {
                store.cancel_notes(&operation.binding.session_id, job, reason)?;
            }
        }
        Ok(())
    }

    pub fn cancel_all(&mut self, store: &mut Store) -> Result<()> {
        self.cancel(store)?;
        self.cancel_highlights(store, NotesCancelReason::User)
    }

    pub(super) fn apply_highlights(
        &mut self,
        store: &mut Store,
        event: Event,
    ) -> Result<(bool, Option<String>)> {
        match event {
            Event::HighlightsPrepared(binding, result) => {
                let Some(operation) = self
                    .highlights_operation
                    .as_ref()
                    .filter(|operation| operation.binding == binding)
                else {
                    return Ok((false, None));
                };
                if operation.retired {
                    self.finish_highlights(HighlightsPhase::Cancelled);
                    return Ok((false, None));
                }
                let outcome = (|| -> Result<()> {
                    ensure!(
                        binding.root == self.snapshot.model_root
                            && binding.profile == self.snapshot.generation_profile,
                        "highlights model selection changed"
                    );
                    let plan = result.map_err(anyhow::Error::msg)?;
                    ensure!(
                        plan.source.session_id == binding.session_id
                            && plan.source.sha256 == binding.source_sha256,
                        "prepared highlights do not match the frozen source"
                    );
                    let job = store.begin_notes(&binding.session_id, plan)?;
                    let operation = self.highlights_operation.as_mut().unwrap();
                    operation.binding.job_id = Some(job.id.clone());
                    let run = RunHighlights {
                        binding: operation.binding.clone(),
                        cancel: operation.cancel.clone(),
                    };
                    if self
                        .generator
                        .try_send(InferenceCommand::RunHighlights(run))
                        .is_err()
                    {
                        store.cancel_notes(
                            &binding.session_id,
                            &job.id,
                            NotesCancelReason::Failed,
                        )?;
                        anyhow::bail!("inference queue is full; highlights job was cancelled");
                    }
                    let status = self.snapshot.highlights.as_mut().unwrap();
                    status.binding = operation.binding.clone();
                    status.phase = HighlightsPhase::Generating;
                    status.total_chunks = job.plan.chunks.len();
                    Ok(())
                })();
                if let Err(error) = outcome {
                    self.finish_highlights(HighlightsPhase::Failed);
                    return Ok((
                        true,
                        Some(format!("Could not prepare highlights: {error:#}")),
                    ));
                }
                Ok((true, None))
            }
            Event::HighlightsProgress(binding, progress) => {
                if self
                    .highlights_operation
                    .as_ref()
                    .is_some_and(|operation| operation.binding == binding && !operation.retired)
                    && let Some(status) = &mut self.snapshot.highlights
                    && progress.total_chunks == status.total_chunks
                    && progress.completed_chunks <= progress.total_chunks
                    && progress.completed_chunks >= status.completed_chunks
                {
                    status.completed_chunks = progress.completed_chunks;
                }
                Ok((false, None))
            }
            Event::HighlightsDone(binding, result) => {
                let Some(operation) = self
                    .highlights_operation
                    .as_ref()
                    .filter(|operation| operation.binding == binding)
                else {
                    return Ok((false, None));
                };
                if operation.retired {
                    self.finish_highlights(HighlightsPhase::Cancelled);
                    return Ok((true, None));
                }
                let Some(job) = binding.job_id.as_deref() else {
                    self.finish_highlights(HighlightsPhase::Failed);
                    return Ok((
                        false,
                        Some(
                            "Highlights completion had no job identity; no draft was published."
                                .into(),
                        ),
                    ));
                };
                let outcome = (|| -> Result<bool> {
                    ensure!(
                        binding.root == self.snapshot.model_root
                            && binding.profile == self.snapshot.generation_profile,
                        "highlights model selection changed"
                    );
                    let completion = result.map_err(anyhow::Error::msg)?;
                    store.complete_notes(&binding.session_id, job, completion)
                })();
                match outcome {
                    Ok(true) => {
                        self.finish_highlights(HighlightsPhase::Ready);
                        Ok((true, None))
                    }
                    Ok(false) => {
                        self.finish_highlights(HighlightsPhase::Cancelled);
                        Ok((
                            false,
                            Some(
                                "Highlights are no longer current; no draft was published.".into(),
                            ),
                        ))
                    }
                    Err(error) => {
                        let retirement =
                            store.cancel_notes(&binding.session_id, job, NotesCancelReason::Failed);
                        self.finish_highlights(HighlightsPhase::Failed);
                        retirement?;
                        Ok((
                            true,
                            Some(format!("Could not generate highlights: {error:#}")),
                        ))
                    }
                }
            }
            _ => unreachable!("non-highlights event"),
        }
    }

    fn finish_highlights(&mut self, phase: HighlightsPhase) {
        self.highlights_operation = None;
        if let Some(status) = &mut self.snapshot.highlights {
            status.phase = phase;
        }
    }
}
