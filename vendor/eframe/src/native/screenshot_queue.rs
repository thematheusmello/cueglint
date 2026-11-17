use egui::{UserData, ViewportId};
use egui_wgpu::capture::{CaptureStage, trace_capture_status};
use egui_winit::ActionRequested;

const MAX_PENDING: usize = 16;

pub(super) struct PendingScreenshots {
    viewport: ViewportId,
    data: Vec<UserData>,
}

impl PendingScreenshots {
    pub(super) fn new(viewport: ViewportId) -> Self {
        Self {
            viewport,
            data: Vec::new(),
        }
    }

    pub(super) fn collect(&mut self, actions: &mut Vec<ActionRequested>) -> usize {
        let mut rejected = 0;
        actions.retain(|action| {
            if let ActionRequested::Screenshot(data) = action {
                if self.data.len() < MAX_PENDING {
                    self.data.push(data.clone());
                } else {
                    rejected += 1;
                    trace_capture_status(
                        std::slice::from_ref(data),
                        self.viewport,
                        CaptureStage::QueueOverflow,
                    );
                }
                false
            } else {
                true
            }
        });
        rejected
    }

    pub(super) fn requests(&mut self) -> &mut Vec<UserData> {
        &mut self.data
    }
}

impl Drop for PendingScreenshots {
    fn drop(&mut self) {
        trace_capture_status(&self.data, self.viewport, CaptureStage::ViewportDropped);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_wgpu::capture::CaptureTrace;
    use std::sync::{Arc, Mutex};

    #[test]
    fn per_viewport_owner_bounds_tokens_keeps_order_and_leaves_other_actions_intact() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut actions = vec![ActionRequested::Copy, ActionRequested::Paste];
        let tokens: Vec<_> = (0..MAX_PENDING + 2)
            .map(|id| {
                let seen = seen.clone();
                UserData::new(CaptureTrace::new(id as u128, move |stage, viewport| {
                    seen.lock().unwrap().push((id, stage, viewport));
                }))
            })
            .collect();
        actions.extend(tokens.iter().cloned().map(ActionRequested::Screenshot));
        let viewport = ViewportId::from_hash_of("capture-owner");
        let mut pending = PendingScreenshots::new(viewport);
        assert_eq!(pending.collect(&mut actions), 2);
        assert_eq!(actions, [ActionRequested::Copy, ActionRequested::Paste]);
        assert_eq!(pending.requests().as_slice(), &tokens[..MAX_PENDING]);
        assert_eq!(
            *seen.lock().unwrap(),
            [
                (MAX_PENDING, CaptureStage::QueueOverflow, viewport),
                (MAX_PENDING + 1, CaptureStage::QueueOverflow, viewport),
            ]
        );
        assert_eq!(pending.collect(&mut actions), 0);
        assert_eq!(
            pending.requests().as_slice(),
            &tokens[..MAX_PENDING],
            "a skipped paint keeps exact UserData identities"
        );
        drop(pending);
        assert_eq!(
            seen.lock()
                .unwrap()
                .iter()
                .filter(|(_, stage, _)| *stage == CaptureStage::ViewportDropped)
                .count(),
            MAX_PENDING
        );
    }

    #[test]
    fn dropping_the_viewport_releases_the_last_unsubmitted_token_owner() {
        let token = UserData::new(123_u128);
        let weak = Arc::downgrade(token.data.as_ref().unwrap());
        let mut actions = vec![ActionRequested::Screenshot(token)];
        let mut pending = PendingScreenshots::new(ViewportId::ROOT);
        assert_eq!(pending.collect(&mut actions), 0);
        assert_eq!(weak.strong_count(), 1);
        drop(pending);
        assert!(weak.upgrade().is_none());
    }
}
