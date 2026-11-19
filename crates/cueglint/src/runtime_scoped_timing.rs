use cueglint_local::screen::scoped::WindowCaptureTiming;
use serde_json::{Map, Value, json};
use std::{
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

pub(crate) struct WindowWorkerTiming {
    pub native: WindowCaptureTiming,
    pub event_ready: Instant,
}

pub(crate) struct WindowTimingReport {
    pub(super) capture_id: String,
    pub(super) request_id: String,
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) capture: WindowWorkerTiming,
    pub(super) worker_received: Instant,
    pub(super) freeze_started: Instant,
    pub(super) request_persisted: Instant,
    pub(super) inference_queued: Instant,
}

impl WindowTimingReport {
    pub(crate) fn emit(self, snapshot_queued: Instant) {
        if let Some(record) = self.record(snapshot_queued) {
            let _ = writeln!(std::io::stderr().lock(), "{record}");
        }
    }

    fn record(&self, snapshot_queued: Instant) -> Option<Value> {
        if self.cancel.load(Ordering::Acquire) {
            return None;
        }
        let native = &self.capture.native;
        let start = native.frame_observed;
        let mut last = start;
        let mut elapsed = Map::new();
        for (name, instant) in [
            ("frame_observed", start),
            ("stream_stopped", native.stream_stopped),
            ("rgba_ready", native.rgba_ready),
            ("ocr_started", native.ocr_started),
            ("ocr_finished", native.ocr_finished),
            ("capture_event_ready", self.capture.event_ready),
            ("worker_received", self.worker_received),
            ("freeze_started", self.freeze_started),
            ("request_persisted", self.request_persisted),
            ("inference_queued", self.inference_queued),
            ("snapshot_queued", snapshot_queued),
        ] {
            instant.checked_duration_since(last)?;
            last = instant;
            let nanos = u64::try_from(instant.checked_duration_since(start)?.as_nanos()).ok()?;
            elapsed.insert(name.to_owned(), nanos.into());
        }
        Some(json!({
            "event": "native_scoped_window_timing",
            "schema_version": 1,
            "capture_id": self.capture_id,
            "request_id": self.request_id,
            "elapsed_ns": elapsed,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn scoped_timing_receipt_contains_only_ordered_elapsed_marks_and_opaque_ids() {
        let start = Instant::now();
        let at = |ns| start + Duration::from_nanos(ns);
        let mut report = WindowTimingReport {
            capture_id: "9513f296-4a7f-4ae3-a62c-92498df7f90e".into(),
            request_id: "77f772ba-26cb-4d51-b93c-fa667aab220b".into(),
            cancel: Arc::new(AtomicBool::new(false)),
            capture: WindowWorkerTiming {
                native: WindowCaptureTiming {
                    frame_observed: at(0),
                    stream_stopped: at(1_000_000_000),
                    rgba_ready: at(1_001_000_000),
                    ocr_started: at(1_004_000_000),
                    ocr_finished: at(61_004_000_000),
                },
                event_ready: at(61_005_000_000),
            },
            worker_received: at(61_045_000_000),
            freeze_started: at(61_050_000_000),
            request_persisted: at(61_070_000_000),
            inference_queued: at(61_071_000_000),
        };
        let line = report.record(at(61_091_000_000)).unwrap().to_string();
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap(),
            json!({
                "event":"native_scoped_window_timing",
                "schema_version":1,
                "capture_id":"9513f296-4a7f-4ae3-a62c-92498df7f90e",
                "request_id":"77f772ba-26cb-4d51-b93c-fa667aab220b",
                "elapsed_ns":{
                    "frame_observed":0_u64,
                    "stream_stopped":1_000_000_000_u64,
                    "rgba_ready":1_001_000_000_u64,
                    "ocr_started":1_004_000_000_u64,
                    "ocr_finished":61_004_000_000_u64,
                    "capture_event_ready":61_005_000_000_u64,
                    "worker_received":61_045_000_000_u64,
                    "freeze_started":61_050_000_000_u64,
                    "request_persisted":61_070_000_000_u64,
                    "inference_queued":61_071_000_000_u64,
                    "snapshot_queued":61_091_000_000_u64,
                }
            })
        );
        report.request_persisted = at(61_049_000_000);
        assert!(report.record(at(61_091_000_000)).is_none());
        report.request_persisted = at(61_070_000_000);
        report.cancel.store(true, Ordering::Release);
        assert!(report.record(at(61_091_000_000)).is_none());
    }
}
