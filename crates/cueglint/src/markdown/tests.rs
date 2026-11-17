use super::*;
use eframe::egui::{self, Id};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn document(source: &str) -> Document {
    parse(Arc::from(source), Limits::default()).unwrap()
}
fn input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(690.0, 1600.0),
        )),
        ..Default::default()
    }
}
fn click(target: egui::accesskit::NodeId) -> egui::Event {
    egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
        action: egui::accesskit::Action::Click,
        target_tree: egui::accesskit::TreeId::ROOT,
        target_node: target,
        data: None,
    })
}

#[test]
fn nested_markdown_preserves_lists_quotes_code_and_source() {
    let original = "# Original workshop\n\nHello **bold** and *careful* [reader](https://example.test/docs).\n\n3. Outer\n   - Inner\n4. Next\n\n> A quote\n>\n> > A nested quote\n\n```rust\nlet original = 7;\n```\n";
    let doc = document(original);
    assert_eq!(doc.source().as_ref(), original);
    assert!(matches!(
        doc.blocks().first(),
        Some(Block::Heading { level: 1, .. })
    ));
    let list = doc
        .blocks()
        .iter()
        .find_map(|block| {
            if let Block::List { start, items } = block {
                Some((start, items))
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(*list.0, Some(3));
    assert_eq!(list.1.len(), 2);
    assert!(
        list.1[0]
            .iter()
            .any(|block| matches!(block, Block::List { start: None, .. }))
    );
    assert!(
        doc.blocks()
            .iter()
            .any(|block| matches!(block, Block::Quote(_)))
    );
    assert!(doc.blocks().iter().any(|block| matches!(block, Block::Code { language: Some(lang), text } if lang == "rust" && text == "let original = 7;\n")));
    let plain = doc.plain_text();
    assert!(plain.find("3. Outer").unwrap() < plain.find("Inner").unwrap());
    assert!(plain.find("Inner").unwrap() < plain.find("4. Next").unwrap());
}

#[test]
fn parser_limits_and_cancellation_are_explicit_and_never_modify_source() {
    let source: Arc<str> = "Original source must stay available".into();
    assert!(matches!(
        parse(
            source.clone(),
            Limits {
                source_bytes: 8,
                ..Limits::default()
            }
        ),
        Err(ParseError::SourceTooLarge { limit: 8 })
    ));
    assert_eq!(source.as_ref(), "Original source must stay available");
    assert_eq!(
        parse(
            "hello".into(),
            Limits {
                owned_text_bytes: 2,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        ParseError::Limit("text memory")
    );
    assert_eq!(
        parse(
            "a\n\nb\n\nc".into(),
            Limits {
                events: 3,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        ParseError::Limit("event")
    );
    assert_eq!(
        parse(format!("{}deep", "> ".repeat(40)).into(), Limits::default()).unwrap_err(),
        ParseError::Limit("nesting")
    );
    assert_eq!(
        parse(
            format!("{}deep", "> ".repeat(40)).into(),
            Limits {
                depth: usize::MAX,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        ParseError::Limit("nesting"),
        "callers cannot widen hard safety caps"
    );
    let checks = AtomicUsize::new(0);
    let result = parse_with_cancel(
        "**bounded**\n\n".repeat(5000).into(),
        Limits::default(),
        || checks.fetch_add(1, Ordering::Relaxed) > 1,
    );
    assert_eq!(result.unwrap_err(), ParseError::Cancelled);
}

#[test]
fn incomplete_streaming_markdown_is_valid_and_does_not_mutate_previous_documents() {
    let old = document("A previous **complete** answer.");
    for source in [
        "**unfinished",
        "**finished**\n\n```rust\nlet original",
        "**finished**\n\n```rust\nlet original = 7;\n```",
        "1. first\n2. **second",
    ] {
        let doc = document(source);
        assert_eq!(doc.source().as_ref(), source);
        assert!(!doc.blocks().is_empty());
    }
    assert_eq!(old.source().as_ref(), "A previous **complete** answer.");
    assert!(old.plain_text().contains("previous complete answer"));
}

#[test]
fn images_are_only_alt_text_html_is_literal_and_unsafe_targets_are_inactive() {
    for uri in [
        "javascript:alert(1)",
        "data:image/png;base64,AAAA",
        "file:///tmp/never-read",
        "mailto:person@example.test",
        "#heading",
        "https://user:secret@example.test",
        "https://example.test/\nunsafe",
    ] {
        assert!(HttpUrl::parse(uri).is_none(), "{uri}");
    }
    assert_eq!(
        HttpUrl::parse("HTTPS://example.test/docs")
            .unwrap()
            .as_str(),
        "https://example.test/docs"
    );
    let source = "![original alt](https://never-fetch.invalid/private.png)\n\n<img src=\"file:///tmp/never-read\">\n\n[unsafe](javascript:alert) and [safe](https://example.test/docs)";
    let doc = document(source);
    assert!(
        matches!(&doc.blocks()[0], Block::Paragraph(spans) if spans.iter().any(|span| span.image_alt && span.text == "original alt"))
    );
    assert!(doc.plain_text().contains("original alt"));
    assert!(!doc.plain_text().contains("private.png"));
    assert!(
        doc.plain_text()
            .contains("<img src=\"file:///tmp/never-read\">")
    );
    let context = egui::Context::default();
    context.enable_accesskit();
    let mut rendered = None;
    let output = context.run_ui(input(), |ui| {
        rendered = Some(show(
            ui,
            Id::new("resource-policy"),
            &doc,
            RenderOptions::default(),
        ));
    });
    assert!(
        rendered.unwrap().actions.is_empty(),
        "rendering itself must not open anything"
    );
    assert!(
        output.platform_output.commands.is_empty(),
        "renderer must not issue platform open/copy commands"
    );
    let tree = output.platform_output.accesskit_update.unwrap();
    let links: Vec<_> = tree
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == egui::accesskit::Role::Link)
        .collect();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].1.label(), Some("safe"));
    assert!(links[0].1.supports_action(egui::accesskit::Action::Click));
    assert!(
        !links[0].1.children().is_empty(),
        "link retains selectable text-run children"
    );
}

#[test]
fn nested_list_and_heading_accessibility_are_structural() {
    let doc = document("## Original heading\n\n1. Outer\n   - Inner\n2. Next");
    let context = egui::Context::default();
    context.enable_accesskit();
    let output = context.run_ui(input(), |ui| {
        show(ui, Id::new("semantics"), &doc, RenderOptions::default());
    });
    let tree = output.platform_output.accesskit_update.unwrap();
    let lists: Vec<_> = tree
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == egui::accesskit::Role::List)
        .collect();
    let items: Vec<_> = tree
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == egui::accesskit::Role::ListItem)
        .collect();
    assert_eq!(lists.len(), 2);
    assert_eq!(items.len(), 3);
    assert!(
        tree.nodes
            .iter()
            .any(|(_, node)| node.role() == egui::accesskit::Role::Heading
                && node.level() == Some(2))
    );
    for (_, item) in items {
        assert!(item.position_in_set().is_some());
    }
    assert!(lists.iter().all(|(_, list)| list.size_of_set().is_some()));
}

#[test]
fn only_an_explicit_safe_link_click_returns_an_open_action() {
    let doc = document(
        "[open original](https://example.test/path?q=original) and [inactive](file:///tmp/never-open)",
    );
    let context = egui::Context::default();
    context.enable_accesskit();
    let first = context.run_ui(input(), |ui| {
        show(ui, Id::new("links"), &doc, RenderOptions::default());
    });
    let tree = first.platform_output.accesskit_update.unwrap();
    let target = tree
        .nodes
        .iter()
        .find(|(_, node)| node.role() == egui::accesskit::Role::Link)
        .unwrap()
        .0;
    let mut next = input();
    next.events.push(click(target));
    let mut rendered = None;
    let output = context.run_ui(next, |ui| {
        rendered = Some(show(ui, Id::new("links"), &doc, RenderOptions::default()));
    });
    assert_eq!(
        rendered.unwrap().actions,
        [Action::Open(
            HttpUrl::parse("https://example.test/path?q=original").unwrap()
        )]
    );
    assert!(
        output.platform_output.commands.is_empty(),
        "host controls whether to open returned actions"
    );
}

#[test]
fn rendering_budget_is_explicit_but_copy_keeps_the_entire_original_source() {
    let source = "# Original\n\nA paragraph that exceeds the intentionally tiny display budget.\n\n![alt](file:///tmp/never-read)";
    let doc = document(source);
    let options = RenderOptions {
        text_byte_budget: 2,
        ..RenderOptions::default()
    };
    let context = egui::Context::default();
    context.enable_accesskit();
    let mut rendered = None;
    let first = context.run_ui(input(), |ui| {
        rendered = Some(show(ui, Id::new("copy"), &doc, options));
    });
    assert!(rendered.as_ref().unwrap().limited);
    let tree = first.platform_output.accesskit_update.unwrap();
    let target = tree
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Copy source"))
        .unwrap()
        .0;
    let mut next = input();
    next.events.push(click(target));
    let _ = context.run_ui(next, |ui| {
        rendered = Some(show(ui, Id::new("copy"), &doc, options));
    });
    assert_eq!(rendered.unwrap().actions, [Action::Copy(Arc::from(source))]);
    assert_eq!(doc.source().as_ref(), source);
}

#[test]
fn long_original_lists_and_code_render_with_wrapping_and_answer_metrics() {
    let source = format!("## Original notes\n\n{}\n```text\n{}\n```", (1..=80).map(|n| format!("{n}. Original item {n} with enough words to exercise natural wrapping across the 690 pixel view.\n")).collect::<String>(), "original-long-token-".repeat(50));
    let doc = document(&source);
    let context = egui::Context::default();
    context.enable_accesskit();
    let options = RenderOptions {
        typography: Typography::answers(),
        ..RenderOptions::default()
    };
    let mut rendered = None;
    let output = context.run_ui(input(), |ui| {
        rendered = Some(show(ui, Id::new("long"), &doc, options));
    });
    assert!(!rendered.unwrap().limited);
    let tree = output.platform_output.accesskit_update.unwrap();
    assert_eq!(
        tree.nodes
            .iter()
            .filter(|(_, node)| node.role() == egui::accesskit::Role::ListItem)
            .count(),
        80
    );
    fn sections(shapes: &[egui::epaint::ClippedShape], found: &mut bool) {
        for shape in shapes {
            if let egui::Shape::Text(text) = &shape.shape {
                if text.galley.job.sections.iter().any(|section| {
                    section.format.font_id.size == 14.0 && section.format.line_height.is_none()
                }) {
                    *found = true;
                }
            }
        }
    }
    let mut found = false;
    sections(&output.shapes, &mut found);
    assert!(found, "answers retain the reviewed 14-point body metrics");
}

#[test]
fn pointer_selection_and_copy_span_native_formatted_labels() {
    let doc = document("Original **formatted** text.");
    let context = egui::Context::default();
    context.enable_accesskit();
    let first = context.run_ui(input(), |ui| {
        show(ui, Id::new("selection"), &doc, RenderOptions::default());
    });
    let tree = first.platform_output.accesskit_update.unwrap();
    let run = |value: &str| {
        tree.nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::TextRun && node.value() == Some(value)
            })
            .unwrap()
            .1
            .bounds()
            .unwrap()
    };
    let first = run("Original ");
    let last = run(" text.");
    let start = egui::pos2(first.x0 as f32 + 0.1, ((first.y0 + first.y1) * 0.5) as f32);
    let end = egui::pos2(last.x1 as f32 - 0.1, ((last.y0 + last.y1) * 0.5) as f32);
    let frame = |events: Vec<egui::Event>| {
        let mut raw = input();
        raw.events = events;
        context.run_ui(raw, |ui| {
            show(ui, Id::new("selection"), &doc, RenderOptions::default());
        })
    };
    let _ = frame(vec![
        egui::Event::PointerMoved(start),
        egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    let _ = frame(vec![egui::Event::PointerMoved(end)]);
    let _ = frame(vec![egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    let output = frame(vec![egui::Event::Copy]);
    assert!(output.platform_output.commands.iter().any(|command| matches!(command, egui::OutputCommand::CopyText(text) if text == "Original formatted text.")), "native formatted selection must copy visible text across inline labels");
}

#[test]
fn mixed_formatting_in_one_markdown_link_keeps_one_accessible_target() {
    let doc = document("[**Original** and *formatted* link](https://example.test/original)");
    let context = egui::Context::default();
    context.enable_accesskit();
    let output = context.run_ui(input(), |ui| {
        show(ui, Id::new("mixed-link"), &doc, RenderOptions::default());
    });
    let tree = output.platform_output.accesskit_update.unwrap();
    let links: Vec<_> = tree
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == egui::accesskit::Role::Link)
        .collect();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].1.label(), Some("Original and formatted link"));
    assert!(!links[0].1.children().is_empty());
}

#[test]
fn unformatted_fallback_is_bounded_at_utf8_boundaries_without_changing_source() {
    let source = format!("{}🦀original suffix", "é".repeat(32 * 1024));
    let (preview, shortened) = source_preview(&source);
    assert!(shortened);
    assert_eq!(preview.len(), 64 * 1024);
    assert!(source.ends_with("🦀original suffix"));
    assert_eq!(
        source_preview("small original source"),
        ("small original source", false)
    );
}
