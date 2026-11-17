//! Optional, finite diagnostics carried by an explicit screenshot request.
use egui::{UserData, ViewportId};
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CaptureStage {
    ViewportNotVisible,
    NoRenderState,
    NoWindow,
    NoSurface,
    RecreationFailed,
    SurfaceOutdated,
    SurfaceLost,
    SurfaceOccluded,
    SurfaceTimeout,
    SurfaceUnavailable,
    CopySubmitted,
    MapFailed,
    FormatUnsupported,
    ReadbackReady,
    DeliveryFailed,
    QueueOverflow,
    ViewportDropped,
}

impl CaptureStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::ViewportNotVisible => "viewport_not_visible",
            Self::NoRenderState => "no_render_state",
            Self::NoWindow => "no_window",
            Self::NoSurface => "no_surface",
            Self::RecreationFailed => "recreation_failed",
            Self::SurfaceOutdated => "surface_outdated",
            Self::SurfaceLost => "surface_lost",
            Self::SurfaceOccluded => "surface_occluded",
            Self::SurfaceTimeout => "surface_timeout",
            Self::SurfaceUnavailable => "surface_unavailable",
            Self::CopySubmitted => "copy_submitted",
            Self::MapFailed => "map_failed",
            Self::FormatUnsupported => "format_unsupported",
            Self::ReadbackReady => "readback_ready",
            Self::DeliveryFailed => "delivery_failed",
            Self::QueueOverflow => "queue_overflow",
            Self::ViewportDropped => "viewport_dropped",
        }
    }
}

/// Put this in `UserData` only when explicit screenshot diagnostics are wanted.
pub struct CaptureTrace {
    request_id: u128,
    seen: AtomicU32,
    emit: Box<dyn Fn(CaptureStage, ViewportId) + Send + Sync>,
}

impl CaptureTrace {
    pub fn new(
        request_id: u128,
        emit: impl Fn(CaptureStage, ViewportId) + Send + Sync + 'static,
    ) -> Self {
        Self {
            request_id,
            seen: AtomicU32::new(0),
            emit: Box::new(emit),
        }
    }

    pub fn request_id(&self) -> u128 {
        self.request_id
    }
}

pub fn trace_capture_status(data: &[UserData], viewport: ViewportId, stage: CaptureStage) {
    for token in data {
        if let Some(trace) = token
            .data
            .as_ref()
            .and_then(|data| data.downcast_ref::<CaptureTrace>())
        {
            let bit = 1 << stage as u8;
            if trace.seen.fetch_or(bit, Ordering::Relaxed) & bit == 0 {
                (trace.emit)(stage, viewport);
            }
        }
    }
}

/// Queue submission is the ownership boundary, including when it unwinds.
pub(crate) fn submit_with_capture<R>(
    pending: &mut Vec<UserData>,
    viewport: ViewportId,
    has_copy: bool,
    submit: impl FnOnce() -> R,
) -> Vec<UserData> {
    submit();
    if has_copy {
        let submitted = std::mem::take(pending);
        trace_capture_status(&submitted, viewport, CaptureStage::CopySubmitted);
        submitted
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex, atomic::AtomicUsize, mpsc};

    #[test]
    fn handoff_occurs_only_after_copy_submission_and_keeps_exact_token_on_unwind() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        let token = UserData::new(CaptureTrace::new(77, move |stage, _| {
            record.lock().unwrap().push(stage)
        }));
        let mut pending = vec![token.clone()];
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            submit_with_capture(&mut pending, ViewportId::ROOT, true, || {
                panic!("before submit")
            });
        }));
        assert!(panicked.is_err());
        assert_eq!(pending, [token.clone()]);
        assert!(seen.lock().unwrap().is_empty());

        let submissions = AtomicUsize::new(0);
        assert!(
            submit_with_capture(&mut pending, ViewportId::ROOT, false, || {
                submissions.fetch_add(1, Ordering::Relaxed);
            })
            .is_empty()
        );
        assert_eq!(
            pending,
            [token.clone()],
            "an unrelated submission is not a capture acknowledgement"
        );
        let submitted = submit_with_capture(&mut pending, ViewportId::ROOT, true, || {
            submissions.fetch_add(1, Ordering::Relaxed);
        });
        assert!(pending.is_empty());
        let (tx, rx) = mpsc::channel();
        tx.send(submitted).unwrap();
        assert_eq!(rx.recv().unwrap(), [token]);
        assert_eq!(*seen.lock().unwrap(), [CaptureStage::CopySubmitted]);
        assert!(submit_with_capture(&mut pending, ViewportId::ROOT, true, || {}).is_empty());
        assert_eq!(*seen.lock().unwrap(), [CaptureStage::CopySubmitted]);
        assert_eq!(submissions.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn request_diagnostics_are_deduplicated_across_callback_threads() {
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let token = UserData::new(CaptureTrace::new(22, move |_, _| {
            count.fetch_add(1, Ordering::Relaxed);
        }));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let token = token.clone();
                std::thread::spawn(move || {
                    for _ in 0..100 {
                        trace_capture_status(
                            std::slice::from_ref(&token),
                            ViewportId::ROOT,
                            CaptureStage::NoSurface,
                        );
                        trace_capture_status(
                            std::slice::from_ref(&token),
                            ViewportId::ROOT,
                            CaptureStage::MapFailed,
                        );
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }
}
