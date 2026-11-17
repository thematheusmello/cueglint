// Original Cueglint implementation. SPDX-License-Identifier: MIT OR Apache-2.0
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::{fmt, sync::Arc};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub source_bytes: usize,
    pub events: usize,
    pub nodes: usize,
    pub depth: usize,
    pub owned_text_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            source_bytes: 2 * 1024 * 1024,
            events: 100_000,
            nodes: 25_000,
            depth: 32,
            owned_text_bytes: 4 * 1024 * 1024,
        }
    }
}
impl Limits {
    pub(crate) fn bounded(self) -> Self {
        let cap = Self::default();
        Self {
            source_bytes: self.source_bytes.min(cap.source_bytes),
            events: self.events.min(cap.events),
            nodes: self.nodes.min(cap.nodes),
            depth: self.depth.min(cap.depth),
            owned_text_bytes: self.owned_text_bytes.min(cap.owned_text_bytes),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    SourceTooLarge { limit: usize },
    Limit(&'static str),
    Cancelled,
    Closed,
    Malformed(&'static str),
}
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceTooLarge { limit } => write!(
                f,
                "Markdown exceeds the {limit}-byte preview limit; original source is unchanged"
            ),
            Self::Limit(name) => write!(
                f,
                "Markdown preview reached its {name} limit; original source is unchanged"
            ),
            Self::Cancelled => f.write_str("Markdown parsing was superseded or cancelled"),
            Self::Closed => f.write_str("Markdown parse worker is closed"),
            Self::Malformed(name) => write!(f, "Unexpected Markdown parser structure: {name}"),
        }
    }
}
impl std::error::Error for ParseError {}

/// Only an absolute, validated HTTP(S) target can reach an Open action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpUrl(String);
impl HttpUrl {
    pub fn parse(value: &str) -> Option<Self> {
        if value.len() > 4096 || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return None;
        }
        let parsed = url::Url::parse(value).ok()?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return None;
        }
        Some(Self(parsed.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkTarget {
    Http(HttpUrl),
    Inactive,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InlineStyle {
    pub strong: bool,
    pub emphasis: bool,
    pub code: bool,
}

#[derive(Clone, Debug)]
pub struct Inline {
    pub text: String,
    pub style: InlineStyle,
    pub link: Option<Arc<LinkTarget>>,
    pub image_alt: bool,
}

#[derive(Clone, Debug)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading {
        level: u8,
        content: Vec<Inline>,
    },
    List {
        start: Option<u64>,
        items: Vec<Vec<Block>>,
    },
    Quote(Vec<Block>),
    Code {
        language: Option<String>,
        text: String,
    },
    Rule,
}

#[derive(Debug)]
pub struct Document {
    source: Arc<str>,
    blocks: Vec<Block>,
}
impl Document {
    pub fn source(&self) -> &Arc<str> {
        &self.source
    }
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }
    #[cfg(test)]
    pub fn plain_text(&self) -> String {
        fn blocks_text(blocks: &[Block], output: &mut String) {
            for block in blocks {
                match block {
                    Block::Paragraph(spans) | Block::Heading { content: spans, .. } => {
                        for span in spans {
                            output.push_str(&span.text);
                        }
                    }
                    Block::Code { text, .. } => output.push_str(text),
                    Block::Quote(blocks) => blocks_text(blocks, output),
                    Block::List { start, items } => {
                        for (index, item) in items.iter().enumerate() {
                            if let Some(start) = start {
                                output
                                    .push_str(&format!("{}. ", start.saturating_add(index as u64)));
                            } else {
                                output.push_str("• ");
                            }
                            blocks_text(item, output);
                        }
                    }
                    Block::Rule => output.push_str("---"),
                }
                output.push('\n');
            }
        }
        let mut text = String::new();
        blocks_text(&self.blocks, &mut text);
        text
    }
}

#[derive(Clone, Copy)]
enum ContainerKind {
    Root,
    Quote,
    Item,
}
struct Container {
    kind: ContainerKind,
    blocks: Vec<Block>,
    pending: Vec<Inline>,
}
enum Frame {
    Container(Container),
    Paragraph(Vec<Inline>),
    Heading(u8, Vec<Inline>),
    List(Option<u64>, Vec<Vec<Block>>),
    Code(Option<String>, String),
    Html(String),
}
impl Frame {
    fn container(kind: ContainerKind) -> Self {
        Self::Container(Container {
            kind,
            blocks: Vec::new(),
            pending: Vec::new(),
        })
    }
}

struct Budget {
    limits: Limits,
    nodes: usize,
    text: usize,
}
impl Budget {
    fn node(&mut self) -> Result<(), ParseError> {
        self.nodes += 1;
        if self.nodes > self.limits.nodes {
            Err(ParseError::Limit("node"))
        } else {
            Ok(())
        }
    }
    fn text(&mut self, bytes: usize) -> Result<(), ParseError> {
        self.text = self
            .text
            .checked_add(bytes)
            .ok_or(ParseError::Limit("text memory"))?;
        if self.text > self.limits.owned_text_bytes {
            Err(ParseError::Limit("text memory"))
        } else {
            Ok(())
        }
    }
}

fn flush(frame: &mut Frame, budget: &mut Budget) -> Result<(), ParseError> {
    if let Frame::Container(container) = frame {
        if !container.pending.is_empty() {
            budget.node()?;
            container
                .blocks
                .push(Block::Paragraph(std::mem::take(&mut container.pending)));
        }
    }
    Ok(())
}

fn push_block(stack: &mut [Frame], block: Block, budget: &mut Budget) -> Result<(), ParseError> {
    let parent = stack
        .last_mut()
        .ok_or(ParseError::Malformed("missing parent"))?;
    flush(parent, budget)?;
    budget.node()?;
    if let Frame::Container(parent) = parent {
        parent.blocks.push(block);
        Ok(())
    } else {
        Err(ParseError::Malformed("block parent"))
    }
}

fn same_link(a: &Option<Arc<LinkTarget>>, b: &Option<Arc<LinkTarget>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}

fn append(
    stack: &mut [Frame],
    text: &str,
    style: InlineStyle,
    link: &Option<Arc<LinkTarget>>,
    image_alt: bool,
    budget: &mut Budget,
) -> Result<(), ParseError> {
    budget.text(text.len())?;
    let target = match stack
        .last_mut()
        .ok_or(ParseError::Malformed("text parent"))?
    {
        Frame::Code(_, code) | Frame::Html(code) => {
            code.push_str(text);
            return Ok(());
        }
        Frame::Paragraph(spans) | Frame::Heading(_, spans) => spans,
        Frame::Container(parent) => &mut parent.pending,
        Frame::List(..) => return Err(ParseError::Malformed("text directly inside list")),
    };
    if let Some(last) = target.last_mut() {
        if last.style == style && last.image_alt == image_alt && same_link(&last.link, link) {
            last.text.push_str(text);
            return Ok(());
        }
    }
    budget.node()?;
    target.push(Inline {
        text: text.into(),
        style,
        link: link.clone(),
        image_alt,
    });
    Ok(())
}

#[cfg(test)]
pub fn parse(source: Arc<str>, limits: Limits) -> Result<Document, ParseError> {
    parse_with_cancel(source, limits, || false)
}

/// Pure CPU parsing. Long inputs belong on ParseWorker, never inside a draw callback.
pub fn parse_with_cancel(
    source: Arc<str>,
    limits: Limits,
    cancelled: impl Fn() -> bool,
) -> Result<Document, ParseError> {
    let limits = limits.bounded();
    if source.len() > limits.source_bytes {
        return Err(ParseError::SourceTooLarge {
            limit: limits.source_bytes,
        });
    }
    if cancelled() {
        return Err(ParseError::Cancelled);
    }
    let mut budget = Budget {
        limits,
        nodes: 0,
        text: 0,
    };
    let mut stack = vec![Frame::container(ContainerKind::Root)];
    let mut depth = 0usize;
    let mut strong = 0usize;
    let mut emphasis = 0usize;
    let mut image_depth = 0usize;
    let mut link = None;
    let mut links = Vec::new();
    for (index, event) in Parser::new_ext(&source, Options::empty()).enumerate() {
        if index >= limits.events {
            return Err(ParseError::Limit("event"));
        }
        if index % 64 == 0 && cancelled() {
            return Err(ParseError::Cancelled);
        }
        let style = InlineStyle {
            strong: strong > 0,
            emphasis: emphasis > 0,
            code: false,
        };
        match event {
            Event::Start(tag) => {
                depth += 1;
                if depth > limits.depth {
                    return Err(ParseError::Limit("nesting"));
                }
                let frame = match tag {
                    Tag::Paragraph => Some(Frame::Paragraph(Vec::new())),
                    Tag::Heading { level, .. } => Some(Frame::Heading(level as u8, Vec::new())),
                    Tag::BlockQuote(_) => Some(Frame::container(ContainerKind::Quote)),
                    Tag::List(start) => Some(Frame::List(start, Vec::new())),
                    Tag::Item => Some(Frame::container(ContainerKind::Item)),
                    Tag::CodeBlock(kind) => {
                        let language = if let CodeBlockKind::Fenced(info) = kind {
                            info.split_whitespace()
                                .next()
                                .filter(|name| name.len() <= 128)
                                .map(str::to_owned)
                        } else {
                            None
                        };
                        Some(Frame::Code(language, String::new()))
                    }
                    Tag::HtmlBlock => Some(Frame::Html(String::new())),
                    Tag::Strong => {
                        strong += 1;
                        None
                    }
                    Tag::Emphasis => {
                        emphasis += 1;
                        None
                    }
                    Tag::Link { dest_url, .. } => {
                        let target = HttpUrl::parse(&dest_url)
                            .map(LinkTarget::Http)
                            .unwrap_or(LinkTarget::Inactive);
                        if let LinkTarget::Http(url) = &target {
                            budget.text(url.as_str().len())?;
                        }
                        links.push(link.take());
                        link = Some(Arc::new(target));
                        None
                    }
                    Tag::Image { .. } => {
                        image_depth += 1;
                        None
                    }
                    _ => return Err(ParseError::Malformed("unsupported enabled tag")),
                };
                if let Some(frame) = frame {
                    flush(stack.last_mut().unwrap(), &mut budget)?;
                    stack.push(frame);
                }
            }
            Event::End(tag) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or(ParseError::Malformed("unbalanced tags"))?;
                match tag {
                    TagEnd::Strong => strong -= 1,
                    TagEnd::Emphasis => emphasis -= 1,
                    TagEnd::Link => {
                        link = links.pop().ok_or(ParseError::Malformed("link stack"))?
                    }
                    TagEnd::Image => image_depth -= 1,
                    _ => {
                        let mut frame =
                            stack.pop().ok_or(ParseError::Malformed("missing frame"))?;
                        flush(&mut frame, &mut budget)?;
                        let block = match (tag, frame) {
                            (TagEnd::Paragraph, Frame::Paragraph(spans)) => Block::Paragraph(spans),
                            (TagEnd::Heading(_), Frame::Heading(level, content)) => {
                                Block::Heading { level, content }
                            }
                            (TagEnd::List(_), Frame::List(start, items)) => {
                                Block::List { start, items }
                            }
                            (TagEnd::BlockQuote(_), Frame::Container(container))
                                if matches!(container.kind, ContainerKind::Quote) =>
                            {
                                Block::Quote(container.blocks)
                            }
                            (TagEnd::CodeBlock, Frame::Code(language, text)) => {
                                Block::Code { language, text }
                            }
                            (TagEnd::HtmlBlock, Frame::Html(text)) => {
                                Block::Paragraph(vec![Inline {
                                    text,
                                    style: InlineStyle::default(),
                                    link: None,
                                    image_alt: false,
                                }])
                            }
                            (TagEnd::Item, Frame::Container(container))
                                if matches!(container.kind, ContainerKind::Item) =>
                            {
                                budget.node()?;
                                if let Some(Frame::List(_, items)) = stack.last_mut() {
                                    items.push(container.blocks);
                                    continue;
                                }
                                return Err(ParseError::Malformed("item parent"));
                            }
                            _ => return Err(ParseError::Malformed("mismatched frame")),
                        };
                        push_block(&mut stack, block, &mut budget)?;
                    }
                }
            }
            Event::Text(text)
            | Event::Html(text)
            | Event::InlineHtml(text)
            | Event::InlineMath(text)
            | Event::DisplayMath(text) => append(
                &mut stack,
                &text,
                style,
                &link,
                image_depth > 0,
                &mut budget,
            )?,
            Event::Code(text) => append(
                &mut stack,
                &text,
                InlineStyle {
                    code: true,
                    ..style
                },
                &link,
                image_depth > 0,
                &mut budget,
            )?,
            Event::SoftBreak => {
                append(&mut stack, " ", style, &link, image_depth > 0, &mut budget)?
            }
            Event::HardBreak => {
                append(&mut stack, "\n", style, &link, image_depth > 0, &mut budget)?
            }
            Event::Rule => push_block(&mut stack, Block::Rule, &mut budget)?,
            Event::FootnoteReference(text) => append(
                &mut stack,
                &text,
                style,
                &link,
                image_depth > 0,
                &mut budget,
            )?,
            Event::TaskListMarker(checked) => append(
                &mut stack,
                if checked { "[x] " } else { "[ ] " },
                style,
                &link,
                image_depth > 0,
                &mut budget,
            )?,
        }
    }
    if cancelled() {
        return Err(ParseError::Cancelled);
    }
    if stack.len() != 1 || depth != 0 {
        return Err(ParseError::Malformed("unfinished document"));
    }
    let mut root = stack.pop().unwrap();
    flush(&mut root, &mut budget)?;
    let Frame::Container(root) = root else {
        unreachable!()
    };
    Ok(Document {
        source,
        blocks: root.blocks,
    })
}
