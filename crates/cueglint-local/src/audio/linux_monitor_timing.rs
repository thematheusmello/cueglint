//! First readable monitor sample on the shared session clock.
use super::{MAX_BYTES, pcm_duration_ns};
use std::ffi::{c_int, c_long};

// x86-64 Linux pa_timing_info prefix through read_index, checked against the C header in CI.
#[repr(C)]
#[derive(Default)]
pub(super) struct Timing {
    pub(super) seconds: c_long,
    pub(super) micros: c_long,
    pub(super) synchronized_clocks: c_int,
    pub(super) sink_us: u64,
    pub(super) source_us: u64,
    pub(super) transport_us: u64,
    playing: c_int,
    pub(super) write_corrupt: c_int,
    pub(super) written: i64,
    pub(super) read_corrupt: c_int,
    pub(super) read: i64,
}

impl Timing {
    pub(super) fn read_position_ns(&self, elapsed_ns: u64, wall_ns: u128) -> Option<u64> {
        if self.synchronized_clocks == 0
            || self.write_corrupt != 0
            || self.read_corrupt != 0
            || self.seconds < 0
            || !(0..1_000_000).contains(&self.micros)
            || self.sink_us == u64::MAX
            || self.source_us == u64::MAX
        {
            return None;
        }
        let snapshot_ns = i128::from(self.seconds) * 1_000_000_000 + i128::from(self.micros) * 1000;
        let age = i128::try_from(wall_ns).ok()? - snapshot_ns;
        // Auto timing refreshes at up to 1.5 s. Discard stale/future snapshots after clock changes.
        if !(0..=2_000_000_000).contains(&age) {
            return None;
        }
        // The server last sampled its write index at timestamp. libpulse advances the read
        // index locally on drop, so its signed distance can be negative between snapshots.
        let pending = pcm_duration_ns(i128::from(self.written) - i128::from(self.read));
        if pending > pcm_duration_ns(i128::from(MAX_BYTES)) || pending < -age {
            return None;
        }
        // Input delay precedes the recorded instant. Monitor playback follows it by sink delay.
        let device_ns = (i128::from(self.sink_us) - i128::from(self.source_us)) * 1000;
        (i128::from(elapsed_ns) - age - pending + device_ns)
            .try_into()
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Timing {
        Timing {
            seconds: 5,
            synchronized_clocks: 1,
            ..Default::default()
        }
    }

    #[test]
    fn actual_first_fragment_accepts_signed_device_delay_and_unread_backlog() {
        let raw = Timing {
            sink_us: 211084,
            ..snapshot()
        };
        assert_eq!(
            raw.read_position_ns(1_692_055_009, 5_223_074_603),
            Some(1_680_064_406)
        );
        let queued = Timing {
            written: 9920,
            source_us: 4000,
            sink_us: 1000,
            ..snapshot()
        };
        assert_eq!(
            queued.read_position_ns(1_000_000_000, 5_010_000_000),
            Some(677_000_000)
        );
        let advanced = Timing {
            read: 320,
            ..snapshot()
        };
        assert_eq!(
            advanced.read_position_ns(100_000_000, 5_030_000_000),
            Some(80_000_000)
        );
    }

    #[test]
    fn unusable_snapshot_leaves_existing_fallback_available() {
        for invalid in [
            Timing {
                synchronized_clocks: 0,
                ..snapshot()
            },
            Timing {
                write_corrupt: 1,
                ..snapshot()
            },
            Timing {
                read_corrupt: 1,
                ..snapshot()
            },
            Timing {
                micros: 1_000_000,
                ..snapshot()
            },
            Timing {
                sink_us: u64::MAX,
                ..snapshot()
            },
            Timing {
                written: i64::MAX,
                read: i64::MIN,
                ..snapshot()
            },
            Timing {
                read: i64::MAX,
                written: i64::MIN,
                ..snapshot()
            },
        ] {
            assert_eq!(invalid.read_position_ns(500_000_000, 5_010_000_000), None);
        }
        let raw = snapshot();
        for wall in [4_999_999_999, 7_000_000_001, u128::MAX] {
            assert_eq!(raw.read_position_ns(500_000_000, wall), None);
        }
        assert_eq!(raw.read_position_ns(1, 5_010_000_000), None);
    }
}
