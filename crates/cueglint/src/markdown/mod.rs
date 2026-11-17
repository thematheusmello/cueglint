//! Native read-only answer formatting. Parsing and caching run outside drawing;
//! images are alt text, HTML is literal and URLs only leave as validated click actions.
mod parser;
mod previews;
mod renderer;
mod worker;

use parser::{Block, Document, HttpUrl, Inline, Limits, LinkTarget, ParseError, parse_with_cancel};
pub use previews::{Preview, Previews};
pub use renderer::{Action, RenderOptions, show};
use worker::{DocumentKey, ParseWorker};

pub fn source_preview(source: &str) -> (&str, bool) {
    let mut end = source.len().min(64 * 1024);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    (&source[..end], end < source.len())
}

#[cfg(test)]
use parser::parse;
#[cfg(test)]
use renderer::Typography;
#[cfg(test)]
mod tests;
