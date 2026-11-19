use super::*;
use crate::app::calendar_reminders::{CalendarReminders, Occurrence};
use cueglint_providers::calendar::UpcomingEvents;
use sha2::{Digest, Sha256};
use std::time::SystemTime;

pub(super) const BANNER_HEIGHT: f32 = 60.0;
const ENABLED_KEY: &str = "cueglint.calendar-reminders.enabled.v1";
const SEEN_KEY: &str = "cueglint.calendar-reminders.seen.v1";
const PAGE_LIMIT: usize = 50;

pub(super) fn restore(storage: Option<&dyn eframe::Storage>) -> CalendarReminders {
    let enabled = storage
        .and_then(|storage| storage.get_string(ENABLED_KEY))
        .as_deref()
        == Some("true");
    let seen = storage
        .and_then(|storage| storage.get_string(SEEN_KEY))
        .unwrap_or_default();
    CalendarReminders::restore(enabled, &seen)
}

pub(super) fn save(reminders: &CalendarReminders, storage: &mut dyn eframe::Storage) {
    storage.set_string(ENABLED_KEY, reminders.enabled().to_string());
    let keys = reminders.saved_keys();
    if keys.is_empty() {
        storage.remove_string(SEEN_KEY);
    } else {
        storage.set_string(SEEN_KEY, keys);
    }
}

fn occurrence(scope: &[u8; 32], event: &CalendarEvent) -> Option<Occurrence> {
    let EventTime::Timed { starts_at, .. } = &event.time else {
        return None;
    };
    if event.id.is_empty() || event.id.len() > 1_024 || starts_at.utc.nanoseconds >= 1_000_000_000 {
        return None;
    }
    let starts_at_utc = UNIX_EPOCH.checked_add(Duration::new(
        u64::try_from(starts_at.utc.unix_seconds).ok()?,
        starts_at.utc.nanoseconds,
    ))?;
    let mut hash = Sha256::new();
    hash.update(b"cueglint-calendar-reminder-v1\0");
    hash.update(scope);
    hash.update(starts_at.utc.unix_seconds.to_be_bytes());
    hash.update(starts_at.utc.nanoseconds.to_be_bytes());
    hash.update(event.id.as_bytes());
    Some(Occurrence {
        key: format!("{:x}", hash.finalize()),
        starts_at: starts_at_utc,
    })
}

impl Cueglint {
    pub(super) fn calendar_reminder_preview(&self) -> bool {
        self.fixture.as_deref() == Some(crate::calendar_fixture::REMINDER)
    }

    pub(super) fn calendar_reminder_now(&self) -> SystemTime {
        if self.calendar_reminder_preview() {
            crate::calendar_fixture::reminder_time()
        } else {
            SystemTime::now()
        }
    }

    fn reminder_page(&self) -> Option<(&[u8; 32], &UpcomingEvents)> {
        let calendar = &self.runtime.calendar;
        if (self.fixture.is_some() && !self.calendar_reminder_preview())
            || self.quitting
            || self.backup_restore_blocks_controls()
            || !self.calendar_reminders.enabled()
            || calendar.connection != CalendarConnection::Connected
            || calendar.stale
            || calendar.error.is_some()
            || calendar.disconnect.is_some()
            || !matches!(
                calendar.operation,
                None | Some(CalendarOperation::Refreshing)
            )
        {
            return None;
        }
        calendar.refreshed_at?;
        Some((calendar.page_scope.as_ref()?, calendar.upcoming.as_deref()?))
    }

    pub(super) fn reconcile_reminders(&mut self, now: SystemTime, ctx: &egui::Context) -> bool {
        // These controls hide the banner without discarding the last successful page.
        if self.backup_restore_blocks_controls()
            || self.runtime.calendar.operation == Some(CalendarOperation::Cancelling)
        {
            return false;
        }
        let page = self.reminder_page().map(|(scope, page)| {
            page.events
                .iter()
                .take(PAGE_LIMIT)
                .filter_map(|event| occurrence(scope, event))
                .collect::<Vec<_>>()
        });
        let delay =
            self.calendar_reminders
                .reconcile(now, page.is_some(), page.into_iter().flatten());
        if let Some(delay) = delay
            && !self.calendar_reminder_preview()
            && (self.overlay_open || self.settings_visible)
        {
            ctx.request_repaint_after_for(delay, ViewportId::ROOT);
        }
        self.calendar_reminders.take_active_change()
    }

    fn current_reminder(&self, now: SystemTime) -> Option<(&Occurrence, &CalendarEvent)> {
        let active = self.calendar_reminders.active()?;
        if !active.is_due(now) {
            return None;
        }
        let (scope, page) = self.reminder_page()?;
        let event = page
            .events
            .iter()
            .take(PAGE_LIMIT)
            .find(|event| occurrence(scope, event).as_ref() == Some(active))?;
        Some((active, event))
    }

    pub(super) fn has_calendar_reminder(&self, now: SystemTime) -> bool {
        self.current_reminder(now).is_some()
    }

    pub(super) fn calendar_reminder_settings(&mut self, ui: &mut Ui) {
        if self.fixture.is_some()
            || self.runtime.calendar.connection == CalendarConnection::Unavailable
        {
            return;
        }
        ui.add_space(18.0);
        section(ui, "Meeting reminders");
        card(ui, |ui| {
            ui.add_space(10.0);
            let mut enabled = self.calendar_reminders.enabled();
            if ui
                .checkbox(&mut enabled, "Remind me five minutes before meetings")
                .changed()
            {
                self.calendar_reminders.set_enabled(enabled);
                ui.ctx().request_repaint_of(ViewportId::ROOT);
            }
            ui.label(
                RichText::new("Shown in Cueglint using the last successful Calendar refresh.")
                    .size(11.0)
                    .color(MUTED),
            );
            ui.add_space(10.0);
        });
    }

    pub(super) fn calendar_reminder_banner(&mut self, ui: &mut Ui, now: SystemTime) -> bool {
        if ui.ctx().input(|input| input.viewport().visible()) == Some(false) {
            return false;
        }
        let Some((active, event)) = self.current_reminder(now) else {
            return false;
        };
        let EventTime::Timed { starts_at, .. } = &event.time else {
            return false;
        };
        let key = active.key.clone();
        let title = calendar_event_title(event);
        let title = title.trim();
        let title = title[..title.floor_char_boundary(200)].trim_end();
        let (date, time, zone) = calendar_clock(starts_at);
        let preview = self.calendar_reminder_preview();
        let caption = if preview {
            "Sample reminder"
        } else {
            "Starts soon"
        };
        let can_start = !preview
            && self.pending == 0
            && self.after_notes.is_none()
            && self.state.is_some()
            && self.active_session().is_none()
            && !self.capture_busy()
            && !self.highlights_busy();
        let mut start = false;
        let mut dismiss = false;
        Frame::new()
            .fill(CARD)
            .corner_radius(12)
            .inner_margin(Margin::symmetric(12, 7))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        dismiss = calendar_button(ui, "Dismiss", !preview, false);
                        start = calendar_button(ui, "Start", can_start, true);
                        ui.vertical(|ui| {
                            ui.add(
                                egui::Label::new(RichText::new(title).size(14.0).strong())
                                    .truncate()
                                    .selectable(false),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!("{caption} · {date} · {time} {zone}"))
                                        .size(11.0)
                                        .color(MUTED),
                                )
                                .truncate()
                                .selectable(false),
                            );
                        });
                    });
                });
            });
        if ui.is_enabled() {
            self.calendar_reminders.mark_presented(&key);
        }
        if start
            && let Some((current, _)) = self.current_reminder(SystemTime::now())
            && current.key == key
        {
            self.start_named_session(title.to_owned());
            if self.pending > 0 || self.after_notes.is_some() {
                self.calendar_reminders.dismiss(&key);
            }
        } else if dismiss {
            self.calendar_reminders.dismiss(&key);
        } else {
            return true;
        }
        ui.ctx().request_repaint_of(ViewportId::ROOT);
        true
    }
}
