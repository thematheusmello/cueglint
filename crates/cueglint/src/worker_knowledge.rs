use anyhow::{Context, Result, ensure};
use cueglint_core::{KnowledgeDocument, Store, knowledge};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
    time::Instant,
};

#[derive(Clone)]
pub struct ImportCommand {
    pub id: String,
    pub path: PathBuf,
    cancel: Arc<AtomicBool>,
    started: Instant,
}

impl ImportCommand {
    pub fn new(path: PathBuf) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            path,
            cancel: Arc::new(AtomicBool::new(false)),
            started: Instant::now(),
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    fn check(&self) -> Result<()> {
        ensure!(!self.cancelled(), "Document import cancelled");
        ensure!(
            self.started.elapsed() < knowledge::IMPORT_TIMEOUT,
            "Document import timed out"
        );
        Ok(())
    }
}

struct Job {
    command: ImportCommand,
    result: Receiver<Result<String, String>>,
}

#[derive(Default)]
pub(super) struct Imports {
    job: Option<Job>,
}

impl Imports {
    pub fn is_busy(&self) -> bool {
        self.job.is_some()
    }

    pub fn start(
        &mut self,
        command: ImportCommand,
        on_ready: impl FnOnce() + Send + 'static,
    ) -> Result<()> {
        ensure!(
            !self.is_busy(),
            "A document import is still running; cancel or wait for it to finish"
        );
        command.check()?;
        let (tx, result) = mpsc::sync_channel(1);
        let work = command.clone();
        thread::Builder::new()
            .name("cueglint-document".into())
            .spawn(move || {
                let result = knowledge::read_document(&work.path, &work.cancel)
                    .and_then(|text| {
                        work.check()?;
                        Ok(text)
                    })
                    .map_err(|error| format!("{error:#}"));
                let _ = tx.send(result);
                on_ready();
            })
            .context("Start local document worker")?;
        self.job = Some(Job { command, result });
        Ok(())
    }

    pub fn poll(&mut self, store: &mut Store) -> Option<(String, Result<KnowledgeDocument>)> {
        let job = self.job.as_ref()?;
        let result = match job.result.try_recv() {
            Ok(result) => result.map_err(anyhow::Error::msg),
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                Err(anyhow::anyhow!("Document worker stopped without a result"))
            }
        };
        let job = self.job.take().unwrap();
        let result = job.command.check().and_then(|()| {
            let text = result?;
            let title = job
                .command
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Document");
            store.ingest_knowledge(title, &job.command.path.to_string_lossy(), &text)
        });
        Some((job.command.id, result))
    }
}

impl Drop for Imports {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.command.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::Duration};

    #[test]
    fn one_slot_cancellation_rejects_a_finished_stale_result_without_losing_knowledge() {
        let path =
            std::env::temp_dir().join(format!("cueglint-import-{}.txt", uuid::Uuid::new_v4()));
        fs::write(&path, "The fictional saffron workshop opens on Thursday.").unwrap();
        let mut store = Store::open_in_memory().unwrap();
        let existing = store
            .ingest_knowledge(
                "Existing",
                "existing.txt",
                "Keep original notes and knowledge.",
            )
            .unwrap();
        let mut imports = Imports::default();
        let command = ImportCommand::new(path.clone());
        let (ready, done) = mpsc::sync_channel(1);
        imports
            .start(command.clone(), move || {
                let _ = ready.send(());
            })
            .unwrap();
        done.recv_timeout(Duration::from_secs(3)).unwrap();
        command.cancel();
        assert!(
            imports
                .start(ImportCommand::new(path.clone()), || {})
                .is_err(),
            "cancellation retains the occupied slot until its result is reaped"
        );
        let (id, result) = imports.poll(&mut store).unwrap();
        assert_eq!(id, command.id);
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert_eq!(store.knowledge().unwrap(), vec![existing.clone()]);
        let (ready, done) = mpsc::sync_channel(1);
        imports
            .start(ImportCommand::new(path.clone()), move || {
                let _ = ready.send(());
            })
            .unwrap();
        done.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(imports.poll(&mut store).unwrap().1.is_ok());
        assert_eq!(store.knowledge().unwrap().len(), 2);
        assert!(store.knowledge().unwrap().contains(&existing));
        fs::remove_file(path).unwrap();
    }
}
