// Original Cueglint implementation. SPDX-License-Identifier: MIT OR Apache-2.0
use super::{Block, Document, HttpUrl, Inline, LinkTarget};
use eframe::egui::{self, Align, FontId, Id, Label, Layout, RichText, Stroke, Ui, UiBuilder, vec2};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct Typography {
    pub body_px: f32,
    pub heading_px: f32,
    pub line_height_px: Option<f32>,
    pub heading_gap_px: f32,
    pub paragraph_gap_px: f32,
    pub list_pitch_px: f32,
}
impl Typography {
    /// Screenshot-based sizes, not a verified physical-pixel/DPI conversion.
    pub fn answers() -> Self {
        Self {
            body_px: 14.0,
            heading_px: 18.0,
            line_height_px: None,
            heading_gap_px: 16.0,
            paragraph_gap_px: 8.0,
            list_pitch_px: 32.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RenderOptions {
    pub typography: Typography,
    /// Immutable parsed text revision; replacing source must retire its interactive link IDs.
    pub link_revision: u64,
    /// Disable only when the host already exposes the complete original-source copy action.
    pub copy_source_button: bool,
    /// Callers may lower these budgets; hard caps prevent an unbounded draw pass.
    pub node_budget: usize,
    pub text_byte_budget: usize,
}
impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            typography: Typography::answers(),
            link_revision: 0,
            copy_source_button: true,
            node_budget: 2048,
            text_byte_budget: 256 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Copy(Arc<str>),
    Open(HttpUrl),
}
#[derive(Default, Debug)]
pub struct RenderOutput {
    pub actions: Vec<Action>,
    pub limited: bool,
    pub rendered_nodes: usize,
    pub rendered_text_bytes: usize,
}

/// Pure projection of an already parsed document. Never parses, fetches a resource or opens a URL.
/// Keep `id` stable for a session/request. Selection can reset when streamed structure changes.
pub fn show(ui: &mut Ui, id: Id, document: &Document, options: RenderOptions) -> RenderOutput {
    let mut view = View {
        options,
        output: RenderOutput::default(),
        document_id: id,
        link_slot: 0,
    };
    ui.scope_builder(UiBuilder::new().id(id), |ui| {
        ui.style_mut().interaction.selectable_labels = true;
        ui.style_mut().text_styles.insert(egui::TextStyle::Body, FontId::proportional(options.typography.body_px));
        if options.copy_source_button && ui.button(RichText::new("Copy source").size(11.0)).on_hover_text("Copy the complete original Markdown, including content outside the preview budget").clicked() {
            view.output.actions.push(Action::Copy(document.source().clone()));
        }
        view.blocks(ui, document.blocks());
        if view.output.limited {
            ui.label(RichText::new("Formatted preview reached its rendering limit. The full-response copy action includes the complete original text.").small().weak());
        }
    });
    view.output
}

struct View {
    options: RenderOptions,
    output: RenderOutput,
    document_id: Id,
    link_slot: usize,
}
impl View {
    fn allow(&mut self, bytes: usize) -> bool {
        if self.output.limited {
            return false;
        }
        // shortcut: stop oversized previews here; add block pagination only when complete long-form previews are needed.
        if self.output.rendered_nodes >= self.options.node_budget.min(2048)
            || bytes
                > self
                    .options
                    .text_byte_budget
                    .min(256 * 1024)
                    .saturating_sub(self.output.rendered_text_bytes)
        {
            self.output.limited = true;
            return false;
        }
        self.output.rendered_nodes += 1;
        self.output.rendered_text_bytes += bytes;
        true
    }

    fn blocks(&mut self, ui: &mut Ui, blocks: &[Block]) {
        for (index, block) in blocks.iter().enumerate() {
            if !self.allow(0) {
                break;
            }
            ui.push_id(index, |ui| match block {
                Block::Paragraph(spans) => self.inlines(ui, spans, false),
                Block::Heading { level, content } => {
                    if index > 0 {
                        ui.add_space(self.options.typography.heading_gap_px);
                    }
                    ui.scope_builder(UiBuilder::new(), |ui| {
                        ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
                            node.set_role(egui::accesskit::Role::Heading);
                            node.set_level(usize::from(*level));
                        });
                        self.inlines(ui, content, true);
                    });
                }
                Block::Quote(blocks) => {
                    let inner = ui.indent("quote", |ui| self.blocks(ui, blocks));
                    ui.painter().vline(
                        inner.response.rect.left() - 6.0,
                        inner.response.rect.y_range(),
                        Stroke::new(2.0, ui.visuals().weak_text_color()),
                    );
                }
                Block::List { start, items } => {
                    ui.scope_builder(UiBuilder::new(), |ui| {
                        ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
                            node.set_role(egui::accesskit::Role::List);
                            node.set_label(if start.is_some() {
                                "Ordered list"
                            } else {
                                "Unordered list"
                            });
                            node.set_size_of_set(items.len());
                        });
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (number, item) in items.iter().enumerate() {
                            if !self.allow(0) {
                                break;
                            }
                            ui.scope_builder(UiBuilder::new().id_salt(number), |ui| {
                                ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
                                    node.set_role(egui::accesskit::Role::ListItem);
                                    node.set_position_in_set(number + 1);
                                });
                                ui.set_min_height(self.options.typography.list_pitch_px);
                                ui.horizontal_top(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    let prefix = start
                                        .map(|start| {
                                            format!("{}.", start.saturating_add(number as u64))
                                        })
                                        .unwrap_or_else(|| "•".into());
                                    ui.add_sized(
                                        vec2(
                                            28.0,
                                            self.options.typography.line_height_px.unwrap_or(20.0),
                                        ),
                                        Label::new(
                                            RichText::new(prefix)
                                                .size(self.options.typography.body_px),
                                        )
                                        .selectable(true),
                                    );
                                    ui.vertical(|ui| {
                                        ui.set_width(ui.available_width());
                                        self.blocks(ui, item);
                                    });
                                });
                            });
                        }
                    });
                }
                Block::Code { language, text } => {
                    if !self.allow(text.len()) {
                        return;
                    }
                    egui::Frame::new()
                        .fill(ui.visuals().extreme_bg_color)
                        .corner_radius(6)
                        .inner_margin(8)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if let Some(language) = language {
                                    ui.label(RichText::new(language).small().weak());
                                }
                                if ui.button(RichText::new("Copy code").size(11.0)).clicked() {
                                    self.output
                                        .actions
                                        .push(Action::Copy(Arc::from(text.as_str())));
                                }
                            });
                            ui.add(
                                Label::new(
                                    RichText::new(text)
                                        .monospace()
                                        .size(self.options.typography.body_px)
                                        .line_height(self.options.typography.line_height_px),
                                )
                                .wrap()
                                .selectable(true),
                            );
                        });
                }
                Block::Rule => {
                    ui.separator();
                }
            });
            if index + 1 < blocks.len() && !self.output.limited {
                ui.add_space(self.options.typography.paragraph_gap_px);
            }
        }
    }

    fn inlines(&mut self, ui: &mut Ui, spans: &[Inline], heading: bool) {
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), 0.0),
            Layout::left_to_right(Align::BOTTOM).with_main_wrap(true),
            |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                if let Some(height) = self.options.typography.line_height_px {
                    ui.set_row_height(height);
                }
                let mut spans = spans.iter().peekable();
                while let Some(span) = spans.next() {
                    if !self.allow(span.text.len()) {
                        break;
                    }
                    let text = self.rich_text(ui, span, heading);
                    match span.link.as_deref() {
                        Some(LinkTarget::Http(url)) => {
                            let mut job = egui::text::LayoutJob::default();
                            text.append_to(
                                &mut job,
                                ui.style(),
                                egui::FontSelection::Default,
                                Align::BOTTOM,
                            );
                            while let Some(next) = spans.peek().filter(|next| {
                                next.link
                                    .as_ref()
                                    .zip(span.link.as_ref())
                                    .is_some_and(|(left, right)| Arc::ptr_eq(left, right))
                            }) {
                                if !self.allow(next.text.len()) {
                                    break;
                                }
                                self.rich_text(ui, spans.next().unwrap(), heading)
                                    .append_to(
                                        &mut job,
                                        ui.style(),
                                        egui::FontSelection::Default,
                                        Align::BOTTOM,
                                    );
                            }
                            let label = job.text.clone();
                            let link_id = self.document_id.with((
                                "link",
                                self.options.link_revision,
                                self.link_slot,
                            ));
                            self.link_slot += 1;
                            let response = ui
                                .scope_builder(UiBuilder::new().id(link_id), |ui| {
                                    ui.add(egui::Link::new(job))
                                })
                                .inner
                                .on_hover_text(url.as_str());
                            // egui 0.35's selectable-text path overwrites Link with Label; retain both link and text actions.
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Link,
                                    ui.is_enabled(),
                                    &label,
                                )
                            });
                            if response.clicked() {
                                self.output.actions.push(Action::Open(url.clone()));
                            }
                        }
                        _ => {
                            ui.add(Label::new(text).wrap().selectable(true));
                        }
                    }
                }
            },
        );
    }

    fn rich_text(&self, ui: &Ui, span: &Inline, heading: bool) -> RichText {
        let mut text = RichText::new(&span.text)
            .size(if heading {
                self.options.typography.heading_px
            } else {
                self.options.typography.body_px
            })
            .line_height(self.options.typography.line_height_px);
        if heading || span.style.strong {
            text = text.strong();
        }
        if span.style.emphasis {
            text = text.italics();
        }
        if span.style.code {
            text = text
                .monospace()
                .background_color(ui.visuals().extreme_bg_color);
        }
        text
    }
}
