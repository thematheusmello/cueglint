//! Original, fixed Calendar state for isolated native previews.
use cueglint_providers::calendar::{
    CalendarConnection, CalendarDateTime, CalendarEvent, CalendarSnapshot, EventTime,
    UpcomingEvents, UtcInstant,
};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const SETTINGS: &str = "native-v2-calendar-settings";
pub const HISTORY: &str = "native-v2-calendar-history";
pub const REMINDER: &str = "native-v2-calendar-reminder";
pub const REMINDER_SIZE: [f32; 2] = [690.0, 164.0];

pub fn reminder_time() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_791_278_880)
}

pub fn snapshot(view: &str) -> Option<CalendarSnapshot> {
    let upcoming = match view {
        SETTINGS => None,
        HISTORY => Some(Arc::new(upcoming())),
        REMINDER => {
            let mut page = upcoming();
            // Use only the first original timed event, two minutes after the simulated clock.
            page.events.truncate(1);
            return Some(CalendarSnapshot {
                connection: CalendarConnection::Connected,
                configured: true,
                upcoming: Some(Arc::new(page)),
                page_scope: Some([0x61; 32]),
                refreshed_at: Some(reminder_time()),
                ..CalendarSnapshot::default()
            });
        }
        _ => return None,
    };
    Some(CalendarSnapshot {
        connection: CalendarConnection::Connected,
        configured: true,
        upcoming,
        ..CalendarSnapshot::default()
    })
}

fn upcoming() -> UpcomingEvents {
    let instant = |unix_seconds| CalendarDateTime {
        utc: UtcInstant {
            unix_seconds,
            nanoseconds: 0,
        },
        offset_minutes: None,
    };
    let events = [
        (
            "fixture-exhibit",
            "Harbor exhibit walk-through",
            EventTime::Timed {
                starts_at: instant(1_791_279_000),
                ends_at: Some(instant(1_791_281_700)),
            },
        ),
        (
            "fixture-studio",
            "Proofreading studio notes",
            EventTime::AllDay {
                starts_on: "2026-10-07".into(),
                ends_before: "2026-10-09".into(),
            },
        ),
        (
            "fixture-map",
            "Community map handoff",
            EventTime::Timed {
                starts_at: instant(1_791_554_400),
                ends_at: Some(instant(1_791_557_100)),
            },
        ),
        (
            "fixture-signage",
            "Workshop signage pickup",
            EventTime::Timed {
                starts_at: instant(1_791_646_200),
                ends_at: None,
            },
        ),
    ]
    .into_iter()
    .map(|(id, title, time)| CalendarEvent {
        id: id.into(),
        title: Some(title.into()),
        time,
    })
    .collect();
    UpcomingEvents {
        events,
        calendar_time_zone: Some("UTC".into()),
        has_more: false,
    }
}
