use super::*;
use crate::runtime::PickerPhase;
use cueglint_local::screen::scoped::{PickerEvent, PickerSession};
use std::sync::mpsc::{Receiver, TrySendError};

pub(super) struct WindowPickerUi {
    id: String,
    native: Option<(PickerSession, Receiver<PickerEvent>)>,
    pending: Option<PickerEvent>,
    retiring: bool,
}

impl Cueglint {
    pub(super) fn poll_window_picker(&mut self, ctx: &egui::Context) {
        let Some(status) = self.runtime.window_picker.clone() else {
            self.window_picker = None;
            return;
        };
        if self.window_picker.is_none() {
            let mut picker = WindowPickerUi {
                id: status.id.clone(),
                native: None,
                pending: None,
                retiring: false,
            };
            if status.phase == PickerPhase::Choosing
                && !status.cancelled()
                && Instant::now() < status.deadline
            {
                ctx.request_repaint_after(
                    status.deadline.saturating_duration_since(Instant::now()),
                );
                let wake = ctx.clone();
                match PickerSession::present(status.id.clone(), move || wake.request_repaint()) {
                    Ok(native) => picker.native = Some(native),
                    Err(error) => {
                        picker.pending = Some(PickerEvent::Failed {
                            job_id: status.id.clone(),
                            message: format!("{error:#}"),
                        });
                    }
                }
            }
            self.window_picker = Some(picker);
        }
        let Some(picker) = &mut self.window_picker else {
            return;
        };
        if picker.id != status.id {
            self.notice = Some((
                "Window selection is still cleaning up. Restart Cueglint if it cannot close."
                    .into(),
                true,
            ));
            return;
        }
        if status.phase == PickerPhase::Retiring && !picker.retiring {
            picker.retiring = true;
            picker.pending = None;
            if let Some((native, _)) = &mut picker.native {
                if let Err(error) = native.retire(&picker.id) {
                    self.notice = Some((
                        format!("Could not close the window picker: {error:#}"),
                        true,
                    ));
                }
            } else {
                picker.pending = Some(PickerEvent::Retired {
                    job_id: picker.id.clone(),
                });
            }
        }
        if picker.pending.is_none()
            && let Some((_, events)) = &picker.native
        {
            while let Ok(event) = events.try_recv() {
                if !picker.retiring || matches!(event, PickerEvent::Retired { .. }) {
                    picker.pending = Some(event);
                    break;
                }
            }
        }
        if status.phase == PickerPhase::Choosing && status.cancelled() {
            picker.pending = Some(PickerEvent::Cancelled {
                job_id: picker.id.clone(),
            });
        } else if status.phase == PickerPhase::Choosing && Instant::now() >= status.deadline {
            picker.pending = Some(PickerEvent::Failed {
                job_id: picker.id.clone(),
                message: "Window selection timed out.".into(),
            });
        }
        if let Some(event) = picker.pending.take()
            && let Err(error) = self.worker.commands.try_send(Command::WindowPicker(event))
        {
            match error {
                TrySendError::Full(Command::WindowPicker(event)) => {
                    picker.pending = Some(event);
                    ctx.request_repaint_after(Duration::from_millis(25));
                }
                _ => {
                    self.notice = Some((
                        "Window picker could not reach the store worker.".into(),
                        true,
                    ))
                }
            }
        }
    }
}
