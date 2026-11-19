use super::{CalendarError, CalendarResult, CalendarSession, GoogleCalendar, http_error};
use cueglint_core::utc_date;
use reqwest::StatusCode;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

const WINDOW_SECONDS: u64 = 30 * 24 * 60 * 60;
const PAGE_SIZE: usize = 50;
const RESPONSE_BYTES: usize = 256 * 1024;
const MAX_UNIX_SECONDS: u64 = 253_402_300_799; // 9999-12-31T23:59:59Z

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UtcInstant {
    pub unix_seconds: i64,
    pub nanoseconds: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalendarDateTime {
    pub utc: UtcInstant,
    /// The numeric local offset supplied by Google, when known.
    pub offset_minutes: Option<i16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventTime {
    Timed {
        starts_at: CalendarDateTime,
        ends_at: Option<CalendarDateTime>,
    },
    AllDay {
        starts_on: String,
        ends_before: String,
    },
}

pub struct CalendarEvent {
    pub id: String,
    pub title: Option<String>,
    pub time: EventTime,
}

pub struct UpcomingEvents {
    pub events: Vec<CalendarEvent>,
    pub calendar_time_zone: Option<String>,
    pub has_more: bool,
}

impl GoogleCalendar {
    /// Get one page of the primary calendar for the 30 days starting at this instant.
    /// Google's interval filters also include in-progress events that end after this instant.
    pub async fn upcoming(
        &self,
        session: &CalendarSession,
        now: SystemTime,
    ) -> CalendarResult<UpcomingEvents> {
        self.check_client(&session.refresh.client_key)?;
        if session.expires_at <= std::time::Instant::now() {
            return Err(CalendarError::RefreshRequired);
        }
        let start_seconds = now
            .duration_since(UNIX_EPOCH)
            .map_err(|_| CalendarError::InvalidWindow)?
            .as_secs();
        let end_seconds = start_seconds
            .checked_add(WINDOW_SECONDS)
            .filter(|&seconds| seconds <= MAX_UNIX_SECONDS)
            .ok_or(CalendarError::InvalidWindow)?;
        let start = query_timestamp(start_seconds);
        let end = query_timestamp(end_seconds);
        let page_size = PAGE_SIZE.to_string();
        let response = self
            .http
            .get(self.endpoints.events.clone())
            .bearer_auth(session.access.expose())
            .query(&[
                ("timeMin", start.as_str()),
                ("timeMax", end.as_str()),
                ("singleEvents", "true"),
                ("orderBy", "startTime"),
                ("showDeleted", "false"),
                ("maxResults", page_size.as_str()),
                (
                    "fields",
                    "timeZone,nextPageToken,items(id,summary,status,start(date,dateTime),end(date,dateTime),endTimeUnspecified)",
                ),
            ])
            .send()
            .await
            .map_err(http_error)?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(CalendarError::RefreshRequired);
        }
        let page: Page = super::response_json(super::require_ok(response)?, RESPONSE_BYTES).await?;
        if page.items.len() > PAGE_SIZE {
            return Err(CalendarError::ResponseTooLarge);
        }
        let mut events = Vec::with_capacity(page.items.len());
        for event in page.items {
            if event.status.as_deref() != Some("cancelled") {
                events.push(event.into_event().ok_or(CalendarError::InvalidResponse)?);
            }
        }
        Ok(UpcomingEvents {
            events,
            calendar_time_zone: page.time_zone,
            has_more: page.next_page_token.is_some_and(|token| !token.is_empty()),
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    #[serde(default)]
    items: Vec<RawEvent>,
    time_zone: Option<String>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEvent {
    id: String,
    summary: Option<String>,
    status: Option<String>,
    start: Option<Boundary>,
    end: Option<Boundary>,
    #[serde(default)]
    end_time_unspecified: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Boundary {
    date: Option<String>,
    date_time: Option<String>,
}

impl RawEvent {
    fn into_event(self) -> Option<CalendarEvent> {
        if self.id.is_empty() || self.id.chars().any(|c| c.is_control()) {
            return None;
        }
        let start = self.start?;
        let end = self.end?;
        let time = match (start.date, end.date, start.date_time, end.date_time) {
            (None, None, Some(start), Some(end)) => {
                let starts_at = parse_timestamp(&start)?;
                let ends_at = parse_timestamp(&end)?;
                if !self.end_time_unspecified && ends_at.utc < starts_at.utc {
                    return None;
                }
                EventTime::Timed {
                    starts_at,
                    ends_at: (!self.end_time_unspecified).then_some(ends_at),
                }
            }
            (Some(starts_on), Some(ends_before), None, None) => {
                if self.end_time_unspecified || day_number(&starts_on)? >= day_number(&ends_before)?
                {
                    return None;
                }
                EventTime::AllDay {
                    starts_on,
                    ends_before,
                }
            }
            _ => return None,
        };
        Some(CalendarEvent {
            id: self.id,
            title: self.summary,
            time,
        })
    }
}

fn query_timestamp(seconds: u64) -> String {
    let date = utc_date(seconds as i64 * 1000);
    let time = seconds % 86_400;
    let hours = time / 3600;
    let minutes = (time / 60) % 60;
    let seconds = time % 60;
    format!("{date}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn parse_timestamp(value: &str) -> Option<CalendarDateTime> {
    let days = day_number(value.get(..10)?)?;
    let bytes = value.as_bytes();
    if !matches!(bytes.get(10), Some(b'T' | b't'))
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return None;
    }
    let (hours, minutes, seconds) = (
        number(bytes.get(11..13)?)?,
        number(bytes.get(14..16)?)?,
        number(bytes.get(17..19)?)?,
    );
    if hours > 23 || minutes > 59 || seconds > 59 {
        return None;
    }
    let mut zone = bytes.get(19..)?;
    let mut nanoseconds = 0;
    if let Some(fraction) = zone.strip_prefix(b".") {
        let count = fraction.iter().take_while(|b| b.is_ascii_digit()).count();
        if !(1..=9).contains(&count) {
            return None;
        }
        nanoseconds = number(&fraction[..count])? as u32 * 10_u32.pow(9 - count as u32);
        zone = &fraction[count..];
    }
    let offset_minutes = match zone {
        b"Z" | b"z" | b"-00:00" | b"-0000" => None,
        _ => Some(zone_offset_minutes(zone)?),
    };
    let unix_seconds = days * 86_400 + hours * 3600 + minutes * 60 + seconds
        - i64::from(offset_minutes.unwrap_or(0)) * 60;
    Some(CalendarDateTime {
        utc: UtcInstant {
            unix_seconds,
            nanoseconds,
        },
        offset_minutes,
    })
}

fn zone_offset_minutes(zone: &[u8]) -> Option<i16> {
    let (hours, minutes) = match zone {
        [b'+' | b'-', _, _, b':', _, _] => (&zone[1..3], &zone[4..6]),
        [b'+' | b'-', _, _, _, _] => (&zone[1..3], &zone[3..5]),
        _ => return None,
    };
    let (hours, minutes) = (number(hours)?, number(minutes)?);
    if hours > 23 || minutes > 59 {
        return None;
    }
    let offset = (hours * 60 + minutes) as i16;
    Some(if zone[0] == b'+' { offset } else { -offset })
}

fn day_number(value: &str) -> Option<i64> {
    let value = value.as_bytes();
    if value.len() != 10 || value[4] != b'-' || value[7] != b'-' {
        return None;
    }
    let (year, month, day) = (
        number(&value[..4])?,
        number(&value[5..7])?,
        number(&value[8..])?,
    );
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return None,
    };
    if year == 0 || !(1..=days_in_month).contains(&day) {
        return None;
    }

    // Convert Gregorian calendar fields to Unix days using the inverse of core's utc_date.
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month + 2) / 5 + day - 1;
    Some(
        era * 146_097 + year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year
            - 719_468,
    )
}

fn number(digits: &[u8]) -> Option<i64> {
    let mut result = 0;
    for &digit in digits {
        if !digit.is_ascii_digit() {
            return None;
        }
        result = result * 10 + i64::from(digit - b'0');
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_to_utc_validates_dates_and_offsets() {
        let before_epoch = parse_timestamp("1970-01-01T00:00:00.125+05:30").unwrap();
        assert_eq!(
            before_epoch.utc,
            UtcInstant {
                unix_seconds: -19_800,
                nanoseconds: 125_000_000
            }
        );
        assert_eq!(before_epoch.offset_minutes, Some(330));
        let local = parse_timestamp("2024-02-29T08:00:00-0500").unwrap();
        let utc = parse_timestamp("2024-02-29T13:00:00Z").unwrap();
        assert_eq!(local.utc, utc.utc);
        assert_eq!(local.offset_minutes, Some(-300));
        assert_eq!(utc.offset_minutes, None);
        assert_eq!(parse_timestamp("2024-02-29T13:00:00-00:00"), Some(utc));
        assert_eq!(
            parse_timestamp("2024-02-29T13:00:00+00:00")
                .unwrap()
                .offset_minutes,
            Some(0)
        );
        assert!(parse_timestamp("2100-02-29T13:00:00Z").is_none());
        assert!(parse_timestamp("2024-02-29T13:00:00").is_none());
        assert!(parse_timestamp("2024-02-29T13:00:00+00:60").is_none());
        assert!(parse_timestamp("2024-02-29T13:00:00.Z").is_none());
        assert_eq!(
            parse_timestamp(&query_timestamp(MAX_UNIX_SECONDS))
                .unwrap()
                .utc
                .unix_seconds,
            MAX_UNIX_SECONDS as i64
        );
    }

    #[test]
    fn timed_ranges_order_instants_across_a_fall_back() {
        let event = |start, end| {
            serde_json::from_value::<RawEvent>(serde_json::json!({
                "id": "original-fall-back",
                "start": {"dateTime": start},
                "end": {"dateTime": end}
            }))
            .unwrap()
            .into_event()
        };
        let first = "2024-11-03T01:50:00-04:00";
        let second = "2024-11-03T01:10:00-05:00";
        let EventTime::Timed {
            starts_at,
            ends_at: Some(ends_at),
        } = event(first, second).unwrap().time
        else {
            panic!("expected both timed endpoints");
        };
        assert_eq!(
            (starts_at.offset_minutes, ends_at.offset_minutes),
            (Some(-240), Some(-300))
        );
        assert_eq!(
            ends_at.utc.unix_seconds - starts_at.utc.unix_seconds,
            20 * 60
        );
        assert!(event(second, first).is_none());
    }
}
