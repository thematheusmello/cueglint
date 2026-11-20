use super::*;
use cueglint_local::releases::PublishedRelease;
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};

type CheckResult = Result<Option<PublishedRelease>, String>;

#[derive(Default)]
pub(super) enum ReleaseCheck {
    #[default]
    Idle,
    Checking(Receiver<CheckResult>),
    Finished(CheckResult),
}

impl ReleaseCheck {
    pub(super) fn show(
        &mut self,
        ui: &mut Ui,
        enabled: bool,
        check: impl FnOnce() -> anyhow::Result<Option<PublishedRelease>> + Send + 'static,
    ) {
        if let Self::Checking(receiver) = self {
            match receiver.try_recv() {
                Ok(result) => *self = Self::Finished(result),
                Err(TryRecvError::Disconnected) => {
                    *self = Self::Finished(Err("The release check stopped. Try again.".into()));
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let waiting = matches!(self, Self::Checking(_));
        let subtitle = match &*self {
            Self::Idle => "Check GitHub for published releases",
            Self::Checking(_) => "Contacting GitHub…",
            Self::Finished(Ok(None)) => "GitHub lists no published releases",
            Self::Finished(Ok(Some(_))) => "Checked the public GitHub releases",
            Self::Finished(Err(error)) => error.as_str(),
        };
        let mut clicked = false;
        row(
            ui,
            concat!("Cueglint ", env!("CARGO_PKG_VERSION")),
            subtitle,
            |ui| {
                let response = ui.add_enabled(
                    enabled && !waiting,
                    egui::Button::new(if waiting { "Checking…" } else { "Check now" })
                        .min_size(vec2(100.0, 28.0)),
                );
                clicked = response.clicked();
            },
        );
        if clicked {
            let (sender, receiver) = sync_channel(1);
            let ctx = ui.ctx().clone();
            let viewport = ctx.viewport_id();
            *self = match std::thread::Builder::new()
                .name("cueglint-release-check".into())
                .spawn(move || {
                    let _ = sender.send(check().map_err(|error| error.to_string()));
                    ctx.request_repaint_of(viewport);
                }) {
                Ok(_) => Self::Checking(receiver),
                Err(_) => {
                    Self::Finished(Err("Could not start the release check. Try again.".into()))
                }
            };
        }
        if let Self::Finished(Ok(Some(release))) = self {
            rule(ui);
            row(
                ui,
                &release.tag,
                if release.prerelease {
                    "Published prerelease"
                } else {
                    "Published release"
                },
                |ui| {
                    if button(ui, "View release", false, vec2(100.0, 28.0)).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(&release.url));
                    }
                },
            );
        }
    }
}
