//! Original, fixed Calendar state for the two isolated native previews.
use cueglint_providers::calendar::{
    CalendarConnection, CalendarDateTime, CalendarEvent, CalendarSnapshot, EventTime,
    UpcomingEvents, UtcInstant,
};
use std::sync::Arc;

pub const SETTINGS: &str = "native-v2-calendar-settings";
pub const HISTORY: &str = "native-v2-calendar-history";

pub fn snapshot(view: &str) -> Option<CalendarSnapshot> {
    let upcoming = match view {
        SETTINGS => None,
        HISTORY => Some(Arc::new(upcoming())),
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
