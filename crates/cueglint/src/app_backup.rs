use super::*;

pub(super) struct BackupReview {
    path: PathBuf,
    replace: bool,
    focus_cancel: bool,
}

impl Cueglint {
    fn backup_restore_blocker(&self) -> Option<&'static str> {
        let Some(state) = &self.state else {
            return Some("Local data is unavailable.");
        };
        if self.capture_busy()
            || state
                .sessions
                .iter()
                .any(|session| session.status == SessionStatus::Recording)
        {
            return Some("Stop recording before restoring a backup.");
        }
        if self.runtime.window_picker.is_some() || self.runtime.window_cleanup_unconfirmed {
            return Some("Finish window capture and its cleanup before restoring a backup.");
        }
        if self.pending != 0
            || self.after_notes.is_some()
            || self.notes_saving
            || self.quitting
            || self.generating()
            || self.model_job_busy()
            || self.provider_busy()
            || self.highlights_busy()
            || self.highlights.append_after_notes.is_some()
            || self.knowledge_import.is_some()
        {
            return Some("Wait for the current operation to finish before restoring a backup.");
        }
        None
    }

    fn backup_restore_pending(&self) -> bool {
        matches!(self.after_notes, Some(Command::RestoreBackup { .. }))
            || self
                .acknowledgement_kinds
                .contains(&Acknowledgement::Restore)
    }

    pub(super) fn backup_restore_blocks_controls(&self) -> bool {
        self.backup_review.is_some() || self.backup_restore_pending()
    }

    pub(super) fn backup_settings(&mut self, ui: &mut Ui) {
        section(ui, "Backup");
        card(ui, |ui| {
            ui.add_space(12.0);
            let label = ui.label(RichText::new("Backup file path").size(12.0));
            ui.add(
                egui::TextEdit::singleline(&mut self.backup_path)
                    .id(Id::new("backup-file-path"))
                    .hint_text("Path to a .json file")
                    .desired_width(f32::INFINITY),
            )
            .labelled_by(label.id);
            let has_path = !self.backup_path.trim().is_empty();
            ui.horizontal_wrapped(|ui| {
                let ready = self.state.is_some()
                    && self.pending == 0
                    && self.after_notes.is_none()
                    && !self.highlights_busy()
                    && self.highlights.append_after_notes.is_none()
                    && !self.quitting;
                if ui
                    .add_enabled(
                        ready && has_path,
                        egui::Button::new("Export backup").min_size(vec2(112.0, 28.0)),
                    )
                    .clicked()
                {
                    self.after_saving_notes(Command::ExportBackup(PathBuf::from(
                        self.backup_path.as_str(),
                    )));
                }
                let blocker = self.backup_restore_blocker();
                if ui
                    .add_enabled(
                        blocker.is_none() && has_path,
                        egui::Button::new("Review restore").min_size(vec2(112.0, 28.0)),
                    )
                    .on_disabled_hover_text(blocker.unwrap_or("Enter a backup file path."))
                    .clicked()
                {
                    self.backup_review = Some(BackupReview {
                        path: PathBuf::from(self.backup_path.as_str()),
                        replace: false,
                        focus_cancel: true,
                    });
                }
            });
            ui.add_space(12.0);
        });
    }

    pub(super) fn backup_restore_dialog(&mut self, ctx: &egui::Context) {
        if self.backup_restore_pending() {
            egui::Modal::new(Id::new("backup-restore-progress")).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(if self.after_notes.is_some() {
                        "Saving notes before restore…"
                    } else {
                        "Restoring backup…"
                    });
                });
            });
            return;
        }
        let Some(mut review) = self.backup_review.take() else {
            return;
        };
        let blocker = self.backup_restore_blocker();
        let dialog = egui::Modal::new(Id::new("backup-restore-review")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.0).clamp(240.0, 420.0));
            let heading = ui.heading("Restore backup");
            ctx.accesskit_node_builder(ui.unique_id(), |node| {
                node.set_role(egui::accesskit::Role::Dialog);
                node.set_modal();
                node.set_labelled_by([heading.id.accesskit_id()]);
            });
            ui.label("Backup file");
            ui.add(
                egui::Label::new(review.path.display().to_string())
                    .wrap()
                    .selectable(true),
            );
            ui.add_space(4.0);
            ui.label("Restore into an empty workspace or explicitly replace its current data.");
            ui.checkbox(
                &mut review.replace,
                "Replace current sessions, modes, knowledge, and settings",
            );
            ui.label("Answers will use Local mode after restore.");
            if let Some(blocker) = blocker {
                ui.label(RichText::new(blocker).color(MUTED));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let cancel = ui.button("Cancel");
                if review.focus_cancel && !ui.is_sizing_pass() {
                    cancel.request_focus();
                    review.focus_cancel = false;
                }
                let confirm = ui.add_enabled(
                    blocker.is_none(),
                    egui::Button::new(if review.replace {
                        "Replace and restore"
                    } else {
                        "Restore backup"
                    }),
                );
                (cancel.clicked(), confirm.clicked())
            })
            .inner
        });
        let (cancel, confirm) = dialog.inner;
        if cancel || dialog.should_close() {
            ctx.request_repaint();
        } else if confirm {
            self.after_saving_notes(Command::RestoreBackup {
                path: review.path,
                replace: review.replace,
            });
            if self.notes_dirty && !self.notes_saving {
                self.after_notes = None;
            }
            ctx.request_repaint();
        } else {
            self.backup_review = Some(review);
        }
    }

    pub(super) fn backup_restored(
        &mut self,
        result: Result<Snapshot, String>,
        ctx: &egui::Context,
    ) {
        self.pending = self.pending.saturating_sub(1);
        let acknowledgement = self.acknowledgement_kinds.pop_front();
        debug_assert_eq!(acknowledgement, Some(Acknowledgement::Restore));

        self.state = None;
        self.selected_mode = None;
        self.mode_menu = false;
        self.mode_draft = None;
        self.overlay_panel = OverlayPanel::Compact;
        self.chat_tab = ChatTab::Notes;
        self.history_search.clear();
        self.question.clear();
        self.question_submission = None;
        self.notes.clear();
        self.notes_dirty = false;
        self.notes_saving = false;
        self.notes_failed = false;
        self.highlights = HighlightsUi::default();
        self.after_notes = None;
        self.transcript.clear();
        self.session_title.clear();
        self.delete_session = None;
        self.delete_document = None;
        self.import_path.clear();
        self.export_path.clear();
        self.model_path.clear();
        self.model_import_path.clear();
        self.provider_draft = None;
        self.provider_key.zeroize();
        self.provider_key_submission = None;
        self.recording_shortcut = None;
        self.retry_shortcut = None;
        self.reply_navigation = None;
        self.response_scroll = 0.0;
        ctx.data_mut(|data| data.remove_by_type::<egui::text_edit::TextEditState>());

        match result {
            Ok(state) => self.apply_snapshot(
                state,
                Some("Backup restored. Answers use Local mode.".into()),
                false,
            ),
            Err(error) => {
                self.runtime = RuntimeSnapshot {
                    capture_inputs: self.runtime.capture_inputs.clone(),
                    permissions: self.runtime.permissions,
                    window_picker: self.runtime.window_picker.clone(),
                    window_cleanup_unconfirmed: self.runtime.window_cleanup_unconfirmed,
                    ..RuntimeSnapshot::default()
                };
                self.notice = Some((
                    format!(
                        "Backup restored. Cueglint could not reload local data. Restart Cueglint. {error}"
                    ),
                    true,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{question_app, question_snapshot};
    use egui::accesskit::{self, NodeId, TreeUpdate};

    fn target(tree: &TreeUpdate, label: &str) -> NodeId {
        tree.nodes
            .iter()
            .find(|(_, node)| node.label() == Some(label))
            .unwrap()
            .0
    }

    fn click(target_node: NodeId) -> egui::Event {
        egui::Event::AccessKitActionRequest(accesskit::ActionRequest {
            action: accesskit::Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node,
            data: None,
        })
    }

    #[test]
    fn restore_review_can_be_cancelled_and_requires_confirmation_for_the_frozen_path() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, mut store, replies, commands) = question_app(&context);
        app.visit_settings_page(Page::Security);
        let path = "chosen backup.json";
        app.backup_path = path.into();
        let draw = |app: &mut Cueglint, events| {
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 670.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        app.keyboard(ui.ctx());
                        app.main_window(ui);
                    },
                )
                .platform_output
                .accesskit_update
                .unwrap()
        };
        let initial = draw(&mut app, vec![]);
        let back = target(&initial, "Back in Settings");
        let review = target(&initial, "Review restore");
        let _ = draw(&mut app, vec![click(review)]);
        let tree = draw(&mut app, vec![]);
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.role() == accesskit::Role::Dialog)
        );
        assert!(
            tree.nodes.iter().any(|(_, node)| {
                node.label() == Some("Back in Settings") && node.is_disabled()
            })
        );
        let _ = draw(&mut app, vec![click(back)]);
        assert!(app.page == Page::Security && app.backup_review.is_some());
        assert!(!app.backup_review.as_ref().unwrap().replace);
        let replacement = "Replace current sessions, modes, knowledge, and settings";
        let _ = draw(&mut app, vec![click(target(&tree, replacement))]);
        assert!(app.backup_review.as_ref().unwrap().replace);
        let _ = draw(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: Some(egui::Key::Escape),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(app.backup_review.is_none());
        assert_eq!(app.backup_path, path);
        assert!(commands.try_recv().is_err());

        let review = target(&draw(&mut app, vec![]), "Review restore");
        let _ = draw(&mut app, vec![click(review)]);
        let tree = draw(&mut app, vec![]);
        assert!(!app.backup_review.as_ref().unwrap().replace);
        app.backup_path = "different backup.json".into();
        let _ = draw(&mut app, vec![click(target(&tree, replacement))]);
        let confirm = target(&draw(&mut app, vec![]), "Replace and restore");
        assert!(commands.try_recv().is_err());

        app.notes = "Save these notes first.".into();
        app.notes_dirty = true;
        let _ = draw(&mut app, vec![click(confirm)]);
        let Command::Notes(session, notes) = commands.try_recv().unwrap() else {
            panic!("restore must wait for the note save");
        };
        app.dispatch_shortcut(Action::NewChat, &context);
        app.after_saving_notes(Command::NewChat);
        assert!(!app.send(Command::Open(session.clone())));
        assert!(commands.try_recv().is_err());

        store.update_notes(&session, &notes).unwrap();
        replies
            .send(Response::State(question_snapshot(&app, &store), None))
            .unwrap();
        app.poll(&context);
        assert!(matches!(
            commands.try_recv().unwrap(),
            Command::RestoreBackup { path: frozen, replace: true } if frozen == PathBuf::from(path)
        ));
        assert!(!app.send(Command::NewChat));
        assert_eq!(app.selected().unwrap().notes, notes);

        let error = "Could not restore the selected backup.".to_owned();
        replies.send(Response::Error(error.clone())).unwrap();
        app.poll(&context);
        assert!(!app.backup_restore_blocks_controls());
        assert_eq!(app.notice, Some((error, true)));
        assert_eq!(app.backup_path, "different backup.json");
        assert_eq!(app.selected().unwrap().notes, notes);
    }

    #[test]
    fn committed_restore_with_reload_failure_retires_the_old_workspace() {
        let context = egui::Context::default();
        let (mut app, _, replies, _commands) = question_app(&context);
        app.backup_path = "chosen backup.json".into();
        app.overlay_panel = OverlayPanel::Detail;
        app.question = "Old question.".into();
        app.provider_key = "synthetic secret".into();
        app.runtime.permissions.screen = PermissionStatus::Granted;
        assert!(app.send(Command::RestoreBackup {
            path: PathBuf::from(&app.backup_path),
            replace: true,
        }));
        replies
            .send(Response::Restored(Err("snapshot read failed".into())))
            .unwrap();
        app.poll(&context);
        assert!(!app.backup_restore_blocks_controls());
        assert!(app.state.is_none());
        assert!(app.reply_window.replies().is_empty());
        assert!(app.overlay_panel == OverlayPanel::Compact);
        assert!(app.question.is_empty() && app.provider_key.is_empty());
        assert_eq!(app.backup_path, "chosen backup.json");
        assert_eq!(app.runtime.permissions.screen, PermissionStatus::Granted);
        let (notice, error) = app.notice.as_ref().unwrap();
        assert!(
            *error
                && notice.contains("Backup restored.")
                && notice.contains("snapshot read failed")
        );
    }
}
