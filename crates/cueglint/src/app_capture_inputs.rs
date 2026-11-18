use super::*;

impl Cueglint {
    pub(super) fn capture_input_settings(&mut self, ui: &mut Ui) {
        let Some(state) = &self.state else {
            return;
        };
        let mut settings = state.settings.clone();
        let editable = self.pending == 0 && !self.capture_busy() && !self.generating();
        let inputs = &self.runtime.capture_inputs;
        let mut refresh = false;
        let mut cancel = None;
        let before = (settings.microphone_device_id.clone(), settings.display_id);
        card(ui, |ui| {
            ui.add_space(12.0);
            let input_label = ui.label("Microphone input");
            let selected = match &settings.microphone_device_id {
                None => "Default — system input".into(),
                Some(id) => inputs
                    .microphones
                    .iter()
                    .find(|device| &device.id == id)
                    .map(|device| device.name.clone())
                    .unwrap_or_else(|| {
                        format!(
                            "{} input ({id})",
                            if inputs.microphone_error.is_none() && !inputs.refreshing {
                                "Missing saved"
                            } else {
                                "Saved"
                            }
                        )
                    }),
            };
            ui.add_enabled_ui(editable, |ui| {
                egui::ComboBox::from_id_salt((
                    "capture-microphone-input",
                    inputs.request_id.as_deref(),
                ))
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(
                            &mut settings.microphone_device_id,
                            None,
                            "Default — system input",
                        )
                        .clicked()
                    {
                        ui.close();
                    }
                    for device in &inputs.microphones {
                        if ui
                            .add_enabled_ui(
                                inputs.microphone_error.is_none() && !inputs.refreshing,
                                |ui| {
                                    ui.push_id(&device.id, |ui| {
                                        ui.selectable_value(
                                            &mut settings.microphone_device_id,
                                            Some(device.id.clone()),
                                            &device.name,
                                        )
                                        .on_hover_text(&device.id)
                                    })
                                    .inner
                                },
                            )
                            .inner
                            .clicked()
                        {
                            ui.close();
                        }
                    }
                })
                .response
                .labelled_by(input_label.id);
            });
            if let Some(error) = &inputs.microphone_error {
                ui.label(
                    RichText::new(format!("{error} Previous input list may be stale."))
                        .size(11.0)
                        .color(MUTED),
                );
            } else if settings
                .microphone_device_id
                .as_ref()
                .is_some_and(|id| !inputs.microphones.iter().any(|device| &device.id == id))
            {
                ui.label(RichText::new("Saved input is unavailable. Reconnect and refresh, or explicitly choose another input.").size(11.0).color(MUTED));
            }
            ui.add_space(8.0);
            let display_label = ui.label("Capture display");
            let selected = match settings.display_id {
                None => "Default — main display".into(),
                Some(id) => inputs
                    .displays
                    .iter()
                    .find(|display| display.id == id)
                    .map(|display| {
                        format!(
                            "Display {} · {} × {}",
                            display.id, display.width, display.height
                        )
                    })
                    .unwrap_or_else(|| {
                        format!(
                            "{} display {id}",
                            if inputs.display_error.is_none() && !inputs.refreshing {
                                "Missing saved"
                            } else {
                                "Saved"
                            }
                        )
                    }),
            };
            ui.add_enabled_ui(editable && cfg!(target_os = "macos"), |ui| {
                egui::ComboBox::from_id_salt(("capture-display", inputs.request_id.as_deref()))
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(
                                &mut settings.display_id,
                                None,
                                "Default — main display",
                            )
                            .clicked()
                        {
                            ui.close();
                        }
                        for display in &inputs.displays {
                            if ui
                                .add_enabled_ui(
                                    inputs.display_error.is_none() && !inputs.refreshing,
                                    |ui| {
                                        ui.push_id(display.id, |ui| {
                                            ui.selectable_value(
                                                &mut settings.display_id,
                                                Some(display.id),
                                                format!(
                                                    "Display {} · {} × {}",
                                                    display.id, display.width, display.height
                                                ),
                                            )
                                        })
                                        .inner
                                    },
                                )
                                .inner
                                .clicked()
                            {
                                ui.close();
                            }
                        }
                    })
                    .response
                    .labelled_by(display_label.id);
            });
            if cfg!(target_os = "macos") {
                ui.label(RichText::new("The same display is used for local screen text and system audio. Default resolves the OS main display when capture starts.").size(11.0).color(MUTED));
                if let Some(error) = &inputs.display_error {
                    ui.label(
                        RichText::new(format!("{error} Previous display list may be stale."))
                            .size(11.0)
                            .color(MUTED),
                    );
                } else if settings
                    .display_id
                    .is_some_and(|id| !inputs.displays.iter().any(|display| display.id == id))
                {
                    ui.label(RichText::new("Saved display is unavailable. Reconnect and refresh, or explicitly choose another display.").size(11.0).color(MUTED));
                }
            } else if cfg!(target_os = "windows") {
                ui.label(RichText::new("Display selection and local screen text are unavailable here. Windows system audio uses the default playback endpoint independently of displays. Any saved display ID is retained.").size(11.0).color(MUTED));
            } else {
                ui.label(RichText::new("Microphone selection is available. Display selection, local screen text and system audio are unavailable here. Any saved display ID is retained.").size(11.0).color(MUTED));
            }
            let malformed = settings.display_id == Some(0)
                || settings
                    .microphone_device_id
                    .as_ref()
                    .is_some_and(|id| cueglint_core::validate_microphone_device_id(id).is_err());
            if malformed
                && ui
                    .add_enabled(editable, egui::Button::new("Use default capture inputs"))
                    .clicked()
            {
                settings.microphone_device_id = None;
                settings.display_id = None;
            }
            ui.horizontal(|ui| {
                refresh = ui
                    .add_enabled(
                        editable && !inputs.refreshing && self.fixture.is_none(),
                        egui::Button::new("Refresh inputs"),
                    )
                    .clicked();
                if inputs.refreshing {
                    ui.spinner();
                    ui.label("Refreshing inputs…");
                    if ui
                        .push_id(
                            ("cancel-input-refresh", inputs.request_id.as_deref()),
                            |ui| ui.button("Cancel input refresh"),
                        )
                        .inner
                        .clicked()
                    {
                        cancel = inputs.request_id.clone();
                    }
                }
            });
            ui.add_space(12.0);
        });
        if before != (settings.microphone_device_id.clone(), settings.display_id) {
            self.send(Command::Settings(settings));
        }
        if refresh {
            self.send(Command::RefreshCaptureInputs);
        }
        if let Some(id) = cancel {
            self.send(Command::CancelCaptureInputs(id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{question_app, question_snapshot};
    use cueglint_local::audio::{CaptureConfig, InputDevice};

    #[test]
    fn legacy_selection_recovery_queues_explicit_defaults_without_render_time_rewriting() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, _, _, commands) = question_app(&context);
        let settings = &mut app.state.as_mut().unwrap().settings;
        settings.microphone_device_id = Some("legacy-unparseable-id".into());
        settings.display_id = Some(0);
        let input = |events| egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 1200.0))),
            events,
            ..Default::default()
        };
        let tree = context
            .run_ui(input(vec![]), |ui| app.capture_input_settings(ui))
            .platform_output
            .accesskit_update
            .unwrap();
        assert!(commands.try_recv().is_err());
        assert_eq!(app.state.as_ref().unwrap().settings.display_id, Some(0));
        let target = tree
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::Button
                    && node.label() == Some("Use default capture inputs")
            })
            .unwrap()
            .0;
        let event = egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Click,
            target_tree: egui::accesskit::TreeId::ROOT,
            target_node: target,
            data: None,
        });
        let _ = context.run_ui(input(vec![event]), |ui| app.capture_input_settings(ui));
        assert!(
            matches!(commands.try_recv().unwrap(), Command::Settings(settings) if settings.microphone_device_id.is_none() && settings.display_id.is_none())
        );
    }

    #[test]
    fn microphone_picker_queues_real_settings_and_the_store_supplies_capture_configuration() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, mut store, _, commands) = question_app(&context);
        app.runtime.capture_inputs.microphones = vec![InputDevice {
            id: "coreaudio:original:input".into(),
            name: "Original USB input".into(),
        }];
        app.runtime.capture_inputs.microphone_error = None;
        let render = |app: &mut Cueglint, events| {
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 1200.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| app.capture_input_settings(ui),
                )
                .platform_output
                .accesskit_update
                .unwrap()
        };
        let action = |target_node| {
            egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node,
                data: None,
            })
        };
        let tree = render(&mut app, vec![]);
        let picker = tree
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::ComboBox
                    && node.value() == Some("Default — system input")
            })
            .unwrap()
            .0;
        let _ = render(&mut app, vec![action(picker)]);
        let tree = render(&mut app, vec![]);
        let input = tree
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::Button
                    && node.label() == Some("Original USB input")
            })
            .unwrap()
            .0;
        let _ = render(&mut app, vec![action(input)]);
        assert!(!egui::Popup::is_any_open(&context));
        let command = commands.try_recv().unwrap();
        assert!(
            matches!(&command, Command::Settings(settings) if settings.microphone_device_id.as_deref() == Some("coreaudio:original:input"))
        );
        let mut runtime = crate::runtime::Runtime::new(&store, true, context.clone()).unwrap();
        let mut selected = app.selected().map(|session| session.id.clone());
        crate::worker::execute(&mut store, &mut selected, &mut runtime, command).unwrap();
        let saved = store.settings().unwrap();
        assert_eq!(runtime.snapshot.capture_config, CaptureConfig::from(&saved));
        assert_eq!(
            runtime
                .snapshot
                .capture_config
                .microphone_device_id
                .as_deref(),
            Some("coreaudio:original:input")
        );
        assert!(saved.answer_provider.provider.is_local());
        app.runtime = runtime.snapshot.clone();
        app.pending = 0;
        app.acknowledgement_kinds.clear();
        let state = question_snapshot(&app, &store);
        app.apply_snapshot(state, None, false);
        app.runtime.capture_inputs.microphones.clear();
        app.runtime.capture_inputs.microphone_error = None;
        let tree = render(&mut app, vec![]);
        assert!(tree.nodes.iter().any(|(_, node)| {
            node.value()
                .is_some_and(|value| value.starts_with("Missing saved input"))
        }));
        assert!(
            commands.try_recv().is_err(),
            "rendering a missing input must not replace it with default"
        );
        assert_eq!(store.settings().unwrap(), saved);
        if cfg!(target_os = "macos") {
            app.runtime.capture_inputs.displays = vec![cueglint_local::screen::Display {
                id: 42,
                width: 1920,
                height: 1080,
            }];
            app.runtime.capture_inputs.display_error = None;
            let tree = render(&mut app, vec![]);
            let picker = tree
                .nodes
                .iter()
                .find(|(_, node)| {
                    node.role() == egui::accesskit::Role::ComboBox
                        && node.value() == Some("Default — main display")
                })
                .unwrap()
                .0;
            let _ = render(&mut app, vec![action(picker)]);
            let tree = render(&mut app, vec![]);
            let display = tree
                .nodes
                .iter()
                .find(|(_, node)| {
                    node.role() == egui::accesskit::Role::Button
                        && node.label() == Some("Display 42 · 1920 × 1080")
                })
                .unwrap()
                .0;
            let _ = render(&mut app, vec![action(display)]);
            let command = commands.try_recv().unwrap();
            assert!(
                matches!(&command, Command::Settings(settings) if settings.display_id == Some(42))
            );
            crate::worker::execute(&mut store, &mut selected, &mut runtime, command).unwrap();
            let saved = store.settings().unwrap();
            assert_eq!(runtime.snapshot.capture_config, CaptureConfig::from(&saved));
            assert_eq!(runtime.snapshot.capture_config.display_id, Some(42));
            assert_eq!(
                saved.microphone_device_id.as_deref(),
                Some("coreaudio:original:input")
            );
        }
    }
}
