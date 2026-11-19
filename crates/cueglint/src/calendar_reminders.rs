//! Local scheduling from an already fetched Calendar page.

use std::{
    collections::VecDeque,
    time::{Duration, SystemTime},
};

const LEAD: Duration = Duration::from_secs(5 * 60);
const RECHECK: Duration = Duration::from_secs(60);
const SEEN_LIMIT: usize = 256;
const DIGEST_LEN: usize = 64;

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct Occurrence {
    pub key: String,
    pub starts_at: SystemTime,
}

impl Occurrence {
    pub(crate) fn is_due(&self, now: SystemTime) -> bool {
        self.starts_at
            .duration_since(now)
            .is_ok_and(|until| !until.is_zero() && until <= LEAD)
    }
}

#[derive(Default)]
pub(crate) struct CalendarReminders {
    enabled: bool,
    active: Option<Occurrence>,
    active_changed: bool,
    seen: VecDeque<String>,
}

impl CalendarReminders {
    pub(crate) fn restore(enabled: bool, seen: &str) -> Self {
        let mut reminders = Self {
            enabled,
            ..Self::default()
        };
        if seen.len() <= SEEN_LIMIT * (DIGEST_LEN + 1) {
            for key in seen.split_ascii_whitespace() {
                if key.len() == DIGEST_LEN && key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    reminders.remember(&key.to_ascii_lowercase());
                }
            }
        }
        reminders
    }

    pub(crate) fn saved_keys(&self) -> String {
        self.seen
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.set_active(None);
        }
    }

    pub(crate) fn active(&self) -> Option<&Occurrence> {
        self.active.as_ref()
    }

    pub(crate) fn take_active_change(&mut self) -> bool {
        std::mem::take(&mut self.active_changed)
    }

    pub(crate) fn reconcile(
        &mut self,
        now: SystemTime,
        page_current: bool,
        occurrences: impl Iterator<Item = Occurrence>,
    ) -> Option<Duration> {
        if !self.enabled || !page_current {
            self.set_active(None);
            return None;
        }
        let mut retain_active = false;
        let mut next_due = None;
        let mut next_wake: Option<Duration> = None;
        for occurrence in occurrences {
            let Ok(until) = occurrence.starts_at.duration_since(now) else {
                continue;
            };
            if until.is_zero() {
                continue;
            }
            if until <= LEAD && self.active.as_ref() == Some(&occurrence) {
                retain_active = true;
                continue;
            }
            if self.seen.contains(&occurrence.key) {
                continue;
            }
            if until <= LEAD {
                if next_due.as_ref().is_none_or(|other: &Occurrence| {
                    (&occurrence.starts_at, &occurrence.key) < (&other.starts_at, &other.key)
                }) {
                    next_due = Some(occurrence);
                }
            } else {
                let wait = until - LEAD;
                next_wake = Some(next_wake.map_or(wait, |next| next.min(wait)));
            }
        }
        if !retain_active {
            self.set_active(next_due);
        }
        if let Some(active) = &self.active
            && let Ok(until) = active.starts_at.duration_since(now)
        {
            next_wake = Some(next_wake.map_or(until, |next| next.min(until)));
        }
        next_wake.map(|wait| wait.min(RECHECK))
    }

    /// Rendering spends this occurrence, while keeping its visible prompt actionable.
    pub(crate) fn mark_presented(&mut self, key: &str) {
        if self.active.as_ref().is_some_and(|active| active.key == key) {
            self.remember(key);
        }
    }

    pub(crate) fn dismiss(&mut self, key: &str) {
        self.mark_presented(key);
        if self.active.as_ref().is_some_and(|active| active.key == key) {
            self.set_active(None);
        }
    }

    fn set_active(&mut self, active: Option<Occurrence>) {
        if self.active != active {
            self.active = active;
            self.active_changed = true;
        }
    }

    fn remember(&mut self, key: &str) {
        if !self.seen.iter().any(|seen| seen == key) {
            if self.seen.len() == SEEN_LIMIT {
                self.seen.pop_front();
            }
            self.seen.push_back(key.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    fn at(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    fn occurrence(id: usize, seconds: u64) -> Occurrence {
        Occurrence {
            key: format!("{id:064x}"),
            starts_at: at(seconds),
        }
    }

    #[test]
    fn calendar_reminder_due_window_uses_precise_utc_and_no_late_catch_up() {
        let mut reminders = CalendarReminders::default();
        let meeting = occurrence(1, 1_000);
        let page = [meeting.clone(), occurrence(2, 300)];
        assert_eq!(
            reminders.reconcile(at(699), true, page.clone().into_iter()),
            None
        );
        assert!(reminders.active().is_none());
        reminders.set_enabled(true);
        assert!(reminders.enabled());
        assert_eq!(
            reminders.reconcile(at(699), true, page.clone().into_iter()),
            Some(Duration::from_secs(1))
        );
        assert!(reminders.active().is_none());
        reminders.reconcile(at(700), true, page.clone().into_iter());
        assert!(reminders.take_active_change());
        assert_eq!(reminders.active().map(|item| &item.key), Some(&meeting.key));
        assert!(meeting.is_due(at(1_000) - Duration::from_nanos(1)));
        reminders.reconcile(at(1_000), true, page.clone().into_iter());
        assert!(reminders.active().is_none());
        assert!(reminders.take_active_change());
        assert!(!meeting.is_due(at(1_000)));
        // A later wake cannot revive the expired prompt, even on a fresh process.
        let mut woke_before_start = CalendarReminders::restore(true, "");
        woke_before_start.reconcile(at(999), true, page.clone().into_iter());
        assert_eq!(
            woke_before_start.active().map(|item| &item.key),
            Some(&meeting.key)
        );
        let mut resumed = CalendarReminders::restore(true, "");
        assert_eq!(resumed.reconcile(at(1_200), true, page.into_iter()), None);
        assert!(resumed.active().is_none());
    }

    #[test]
    fn calendar_reminder_dedupe_survives_dismiss_and_restart_but_reschedule_rearms() {
        let mut reminders = CalendarReminders::restore(true, "");
        let first = occurrence(1, 1_000);
        let second = occurrence(2, 1_001);
        let same_page = || [second.clone(), first.clone(), first.clone()].into_iter();
        reminders.reconcile(at(710), true, same_page());
        assert!(reminders.take_active_change());
        assert_eq!(reminders.active().map(|item| &item.key), Some(&first.key));
        reminders.mark_presented(&first.key);
        reminders.reconcile(at(720), true, same_page());
        assert_eq!(reminders.active().map(|item| &item.key), Some(&first.key));
        reminders.dismiss(&first.key);
        assert!(reminders.take_active_change());
        reminders.reconcile(at(721), true, same_page());
        assert_eq!(reminders.active().map(|item| &item.key), Some(&second.key));
        reminders.dismiss(&second.key);
        let mut reopened = CalendarReminders::restore(true, &reminders.saved_keys());
        reopened.reconcile(at(722), true, same_page());
        assert!(reopened.active().is_none());

        let rescheduled = occurrence(3, 1_100);
        assert_eq!(
            reopened.reconcile(at(799), true, [rescheduled.clone()].into_iter()),
            Some(Duration::from_secs(1))
        );
        reopened.reconcile(at(800), true, [rescheduled.clone()].into_iter());
        assert_eq!(
            reopened.active().map(|item| &item.key),
            Some(&rescheduled.key)
        );
    }

    #[test]
    fn calendar_reminders_invalidate_stale_or_removed_events_and_never_record_undisplayed_ones() {
        let mut reminders = CalendarReminders::restore(true, "");
        let meeting = occurrence(1, 1_000);
        let latest = || [meeting.clone()].into_iter();
        reminders.reconcile(at(700), true, latest());
        assert!(reminders.active().is_some());
        reminders.reconcile(at(701), false, latest());
        assert!(reminders.active().is_none());
        assert!(reminders.saved_keys().is_empty());
        reminders.reconcile(at(702), true, latest());
        reminders.mark_presented(&meeting.key);
        reminders.reconcile(at(703), true, [].into_iter());
        assert!(reminders.active().is_none());
        reminders.reconcile(at(704), true, latest());
        assert!(
            reminders.active().is_none(),
            "the same occurrence has already shown"
        );

        for id in 2..=SEEN_LIMIT + 10 {
            let next = occurrence(id, 1_001);
            reminders.reconcile(at(705), true, [next.clone()].into_iter());
            reminders.dismiss(&next.key);
        }
        assert_eq!(reminders.saved_keys().lines().count(), SEEN_LIMIT);
        assert_eq!(
            CalendarReminders::restore(true, &reminders.saved_keys()).saved_keys(),
            reminders.saved_keys()
        );
        assert!(
            CalendarReminders::restore(true, &"0".repeat(100_000))
                .saved_keys()
                .is_empty()
        );
    }
}
