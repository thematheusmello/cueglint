//! Bounded local text and DOCX ingestion. This module never resolves document relationships.
use crate::MAX_DOCUMENT_BYTES;
use anyhow::{Context, Result, bail, ensure};
use quick_xml::{
    NsReader, XmlVersion,
    events::{BytesRef, BytesStart, Event},
    name::ResolveResult,
};
use std::{
    cell::Cell,
    collections::BTreeSet,
    fs::{self, File},
    io::{self, Cursor, Read, Seek, SeekFrom},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use zip::{CompressionMethod, ZipReadOptions, unstable::read::ZipIterable};

pub const IMPORT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PACKAGE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_XML_BYTES: usize = 4 * 1024 * 1024;
const MAX_METADATA_BYTES: usize = 64 * 1024;
const WORD: &[u8] = b"http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const STRICT_WORD: &[u8] = b"http://purl.oclc.org/ooxml/wordprocessingml/main";
const TYPES: &[u8] = b"http://schemas.openxmlformats.org/package/2006/content-types";
const RELS: &[u8] = b"http://schemas.openxmlformats.org/package/2006/relationships";
const MAIN_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

struct Budget<'a> {
    cancel: &'a AtomicBool,
    deadline: Instant,
    io_bytes: Cell<usize>,
    io_operations: Cell<usize>,
}

impl<'a> Budget<'a> {
    fn new(cancel: &'a AtomicBool) -> Self {
        Self {
            cancel,
            deadline: Instant::now() + IMPORT_TIMEOUT,
            io_bytes: Cell::new(0),
            io_operations: Cell::new(0),
        }
    }

    fn check(&self) -> io::Result<()> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(io::Error::other("Document import cancelled"));
        }
        if Instant::now() >= self.deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Document import timed out",
            ));
        }
        Ok(())
    }

    fn io(&self, bytes: usize) -> io::Result<()> {
        self.check()?;
        let operations = self.io_operations.get().saturating_add(1);
        let total = self.io_bytes.get().saturating_add(bytes);
        if operations > 65_536 || total > 32 * 1024 * 1024 {
            return Err(io::Error::other("Document import I/O budget exceeded"));
        }
        self.io_operations.set(operations);
        self.io_bytes.set(total);
        Ok(())
    }
}

struct CheckedReader<'a, 'b, R> {
    inner: R,
    budget: &'a Budget<'b>,
}

impl<R: Read> Read for CheckedReader<'_, '_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.budget.io(buffer.len())?;
        self.inner.read(buffer)
    }
}

impl<R: Seek> Seek for CheckedReader<'_, '_, R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.budget.io(0)?;
        self.inner.seek(position)
    }
}

fn read_bounded(mut reader: impl Read, limit: usize, budget: &Budget<'_>) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0; 8 * 1024];
    loop {
        budget.check()?;
        let length = reader.read(&mut buffer)?;
        ensure!(
            output.len() + length <= limit,
            "Document part exceeds {limit} bytes"
        );
        if length == 0 {
            return Ok(output);
        }
        output.extend_from_slice(&buffer[..length]);
    }
}

/// Call outside drawing/audio callbacks. Cancellation/deadlines are cooperative around OS file I/O.
/// UTF-8 files preserve their bytes; DOCX stores extracted main-body text with paragraph separators.
pub fn read_document(path: &Path, cancel: &AtomicBool) -> Result<String> {
    let budget = Budget::new(cancel);
    budget.check()?;
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    ensure!(
        !matches!(
            extension.as_str(),
            "pdf" | "doc" | "ppt" | "pptx" | "xls" | "xlsx" | "odt" | "rtf"
        ),
        "Unsupported document format: use UTF-8 text, Markdown or DOCX; PDF is not supported"
    );
    let metadata = fs::metadata(path).context("Inspect local document")?;
    ensure!(metadata.is_file(), "Choose a regular local file");
    ensure!(
        metadata.len() <= MAX_DOCUMENT_BYTES as u64,
        "Document exceeds 2 MiB"
    );
    budget.check()?;
    let file = File::open(path).context("Open local document")?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() <= MAX_DOCUMENT_BYTES as u64,
        "Choose a regular local file up to 2 MiB"
    );
    let bytes = read_bounded(&file, MAX_DOCUMENT_BYTES, &budget)?;
    let after = file.metadata()?;
    ensure!(
        bytes.len() as u64 == before.len()
            && before.len() == after.len()
            && before.modified().ok() == after.modified().ok(),
        "Document changed while being read; retry the import"
    );
    budget.check()?;
    let text = if extension == "docx" {
        extract_docx(&bytes, &budget)?
    } else {
        ensure!(
            !bytes.starts_with(b"%PDF-") && !bytes.starts_with(b"PK\x03\x04"),
            "Unsupported binary document: PDF is not supported; DOCX requires a .docx extension"
        );
        String::from_utf8(bytes)
            .context("Knowledge documents must be UTF-8 text, Markdown or DOCX")?
    };
    budget.check()?;
    ensure!(
        !text.trim().is_empty(),
        "Document has no supported text; images, scanned pages and embedded content are not extracted"
    );
    ensure!(
        text.len() <= MAX_DOCUMENT_BYTES && !text.contains('\0'),
        "Document text exceeds 2 MiB or contains a NUL character"
    );
    Ok(text)
}

fn extract_docx(bytes: &[u8], budget: &Budget<'_>) -> Result<String> {
    budget.check()?;
    ensure!(
        bytes.starts_with(b"PK\x03\x04"),
        "DOCX must be an unencrypted ZIP document"
    );
    // shortcut: conservatively reject ZIP64 signatures anywhere, use a preallocation-bounded ZIP64 reader before supporting ZIP64.
    ensure!(
        !bytes
            .windows(4)
            .any(|b| b == b"PK\x06\x06" || b == b"PK\x06\x07"),
        "ZIP64 DOCX documents are not supported"
    );
    let mut archive = ZipIterable::try_new(
        CheckedReader {
            inner: Cursor::new(bytes),
            budget,
        },
        Default::default(),
    )?
    .enforce_header_consistency(true);
    ensure!(
        (1..=128).contains(&archive.len()),
        "DOCX must contain at most 128 entries"
    );
    let entries = archive.files()?;
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    let mut document = None;
    let mut types = None;
    let mut relationships = None;
    while let Some(entry) = entries.next() {
        budget.check()?;
        let entry = entry?;
        let name = entry.name()?.into_owned();
        ensure!(
            name.len() <= 512
                && !name.contains(['\\', ':', '\0'])
                && !name.split('/').any(|part| part == "." || part == "..")
                && entry.enclosed_name().is_some(),
            "Unsafe DOCX entry path"
        );
        ensure!(names.insert(name.clone()), "Duplicate DOCX entry");
        ensure!(
            !entry.encrypted() && !entry.is_symlink(),
            "Encrypted or linked DOCX entries are not supported"
        );
        ensure!(
            entry.compressed_size() <= bytes.len() as u64
                && entry.header_start() < bytes.len() as u64,
            "DOCX entry exceeds its container"
        );
        ensure!(
            matches!(
                entry.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            ),
            "Unsupported DOCX compression"
        );
        total = total
            .checked_add(entry.size())
            .context("DOCX size overflow")?;
        ensure!(
            total <= MAX_PACKAGE_BYTES,
            "DOCX expanded package exceeds 16 MiB"
        );
        let (target, limit) = match name.as_str() {
            "word/document.xml" => (&mut document, MAX_XML_BYTES),
            "[Content_Types].xml" => (&mut types, MAX_METADATA_BYTES),
            "_rels/.rels" => (&mut relationships, MAX_METADATA_BYTES),
            _ => continue,
        };
        let declared = entry.size();
        ensure!(
            declared <= limit as u64,
            "DOCX XML part exceeds {limit} bytes"
        );
        let file = entry.with_reader(entries.reader_mut(), ZipReadOptions::new())?;
        let xml = read_bounded(file, limit, budget)?;
        ensure!(
            xml.len() as u64 == declared,
            "DOCX XML size does not match its declaration"
        );
        *target = Some(String::from_utf8(xml).context("DOCX XML must be UTF-8")?);
    }
    validate_types(
        types.as_deref().context("DOCX is missing content types")?,
        budget,
    )?;
    validate_relationships(
        relationships
            .as_deref()
            .context("DOCX is missing package relationships")?,
        budget,
    )?;
    extract_body(
        document
            .as_deref()
            .context("DOCX is missing word/document.xml")?,
        budget,
    )
}

fn bound(namespace: &ResolveResult<'_>, value: &[u8]) -> bool {
    matches!(namespace, ResolveResult::Bound(ns) if ns.as_ref() == value)
}

fn attribute(element: &BytesStart<'_>, key: &[u8]) -> Result<Option<String>> {
    for attr in element.attributes() {
        let attr = attr?;
        if attr.key.as_ref() == key {
            return Ok(Some(
                attr.normalized_value(XmlVersion::Implicit1_0)?.into_owned(),
            ));
        }
    }
    Ok(None)
}

fn reference(reference: &BytesRef<'_>) -> Result<String> {
    if let Some(character) = reference.resolve_char_ref()? {
        return Ok(character.to_string());
    }
    Ok(quick_xml::escape::resolve_xml_entity(&reference.decode()?)
        .context("Custom XML entities are not supported")?
        .to_owned())
}

fn xml_characters(text: &str) -> Result<()> {
    ensure!(
        text.chars()
            .all(|c| matches!(c, '\t' | '\n' | '\r')
                || (c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}')),
        "Invalid XML character"
    );
    Ok(())
}

fn visit_xml(
    xml: &str,
    budget: &Budget<'_>,
    mut visit: impl FnMut(usize, &ResolveResult<'_>, &Event<'_>) -> Result<()>,
) -> Result<()> {
    xml_characters(xml)?;
    let mut reader = NsReader::from_str(xml);
    reader.config_mut().check_comments = true;
    reader.resolver_mut().set_max_declarations_per_element(64);
    let mut depth = 0;
    let mut root_seen = false;
    let mut root_closed = false;
    let mut declaration_seen = false;
    for _ in 0..200_000 {
        budget.check()?;
        let event = reader.read_event()?;
        let namespace = match &event {
            Event::Start(element) | Event::Empty(element) => {
                reader.resolver().resolve_element(element.name()).0
            }
            Event::End(element) => reader.resolver().resolve_element(element.name()).0,
            _ => ResolveResult::Unbound,
        };
        ensure!(
            !matches!(namespace, ResolveResult::Unknown(_)),
            "Undeclared XML namespace"
        );
        match &event {
            Event::Start(element) | Event::Empty(element) => {
                ensure!(!root_closed, "XML has multiple root elements");
                depth += 1;
                ensure!(depth <= 64, "DOCX XML nesting exceeds 64 elements");
                root_seen = true;
                for (index, attr) in element.attributes().enumerate() {
                    ensure!(index < 64, "DOCX XML has too many attributes");
                    let attr = attr?;
                    if attr.key.as_ref() != b"xmlns" && !attr.key.as_ref().starts_with(b"xmlns:") {
                        ensure!(
                            !matches!(
                                reader.resolver().resolve_attribute(attr.key).0,
                                ResolveResult::Unknown(_)
                            ),
                            "Undeclared XML attribute namespace"
                        );
                    }
                    xml_characters(&attr.normalized_value(XmlVersion::Implicit1_0)?)?;
                }
            }
            Event::End(_) => ensure!(depth > 0, "Unexpected XML end element"),
            Event::Text(text) => {
                if depth == 0 {
                    ensure!(text.decode()?.trim().is_empty(), "Text outside XML root");
                }
            }
            Event::GeneralRef(value) => {
                ensure!(depth > 0, "XML reference outside root");
                xml_characters(&reference(value)?)?;
            }
            Event::CData(_) => ensure!(depth > 0, "CDATA outside XML root"),
            Event::Decl(declaration) => {
                ensure!(
                    !declaration_seen && !root_seen && declaration.version()?.as_ref() == b"1.0",
                    "Only one XML 1.0 declaration is supported"
                );
                declaration_seen = true;
                if let Some(encoding) = declaration.encoding() {
                    ensure!(
                        encoding?.eq_ignore_ascii_case(b"UTF-8"),
                        "DOCX XML must declare UTF-8"
                    );
                }
            }
            Event::DocType(_) | Event::PI(_) => {
                bail!("XML document types and processing instructions are not supported")
            }
            Event::Eof => {
                ensure!(root_seen && depth == 0, "Incomplete XML document");
                return Ok(());
            }
            _ => {}
        }
        visit(depth, &namespace, &event)?;
        if matches!(event, Event::End(_) | Event::Empty(_)) {
            depth -= 1;
            root_closed |= depth == 0;
        }
    }
    bail!("DOCX XML exceeds 200000 events")
}

fn validate_types(xml: &str, budget: &Budget<'_>) -> Result<()> {
    let mut main = false;
    visit_xml(xml, budget, |depth, namespace, event| {
        if let Event::Start(element) | Event::Empty(element) = event {
            ensure!(
                bound(namespace, TYPES),
                "Invalid DOCX content-type namespace"
            );
            ensure!(
                depth <= 2,
                "Nested DOCX content-type declarations are not supported"
            );
            if depth == 1 {
                ensure!(
                    element.local_name().as_ref() == b"Types",
                    "Invalid DOCX content-types root"
                );
            } else if element.local_name().as_ref() == b"Override"
                && attribute(element, b"PartName")?.as_deref() == Some("/word/document.xml")
            {
                ensure!(
                    !main && attribute(element, b"ContentType")?.as_deref() == Some(MAIN_TYPE),
                    "DOCX must have one macro-free Word main part"
                );
                main = true;
            }
        }
        Ok(())
    })?;
    ensure!(main, "DOCX does not declare a Word main part");
    Ok(())
}

fn validate_relationships(xml: &str, budget: &Budget<'_>) -> Result<()> {
    let mut main = false;
    visit_xml(xml, budget, |depth, namespace, event| {
        if let Event::Start(element) | Event::Empty(element) = event {
            ensure!(
                bound(namespace, RELS),
                "Invalid DOCX relationship namespace"
            );
            ensure!(depth <= 2, "Nested DOCX relationships are not supported");
            if depth == 1 {
                ensure!(
                    element.local_name().as_ref() == b"Relationships",
                    "Invalid DOCX relationships root"
                );
            } else if element.local_name().as_ref() == b"Relationship"
                && matches!(
                    attribute(element, b"Type")?.as_deref(),
                    Some(
                        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
                            | "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument"
                    )
                )
            {
                ensure!(
                    !main
                        && attribute(element, b"Target")?.as_deref() == Some("word/document.xml")
                        && attribute(element, b"TargetMode")?.is_none_or(|v| v == "Internal"),
                    "Only an internal word/document.xml main part is supported"
                );
                main = true;
            }
        }
        Ok(())
    })?;
    ensure!(main, "DOCX has no supported main relationship");
    Ok(())
}

fn extract_body(xml: &str, budget: &Budget<'_>) -> Result<String> {
    let mut output = String::new();
    let mut body_seen = false;
    let mut in_body = false;
    let mut text_depth = None;
    let mut run_depth = None;
    let mut excluded = None;
    visit_xml(xml, budget, |depth, namespace, event| {
        let word = bound(namespace, WORD) || bound(namespace, STRICT_WORD);
        match event {
            Event::Start(element) | Event::Empty(element) => {
                let local = element.local_name();
                let name = local.as_ref();
                if depth == 1 {
                    ensure!(word && name == b"document", "Invalid Word document root");
                }
                if depth == 2 && word && name == b"body" {
                    ensure!(!body_seen, "Duplicate Word body");
                    body_seen = true;
                    in_body = matches!(event, Event::Start(_));
                }
                ensure!(
                    text_depth.is_none(),
                    "Word text cannot contain nested elements"
                );
                if in_body
                    && excluded.is_none()
                    && matches!(event, Event::Start(_))
                    && (matches!(
                        name,
                        b"drawing"
                            | b"object"
                            | b"pict"
                            | b"del"
                            | b"moveFrom"
                            | b"AlternateContent"
                    ))
                {
                    excluded = Some(depth);
                }
                if in_body && word && excluded.is_none() {
                    match name {
                        b"r" if matches!(event, Event::Start(_)) => run_depth = Some(depth),
                        b"t" if matches!(event, Event::Start(_)) => text_depth = Some(depth),
                        b"p" if matches!(event, Event::Empty(_)) => output.push('\n'),
                        b"tab" if run_depth == Some(depth - 1) => output.push('\t'),
                        b"br" | b"cr" if run_depth == Some(depth - 1) => output.push('\n'),
                        _ => {}
                    }
                }
            }
            Event::End(element) => {
                if run_depth == Some(depth) {
                    run_depth = None;
                }
                if text_depth == Some(depth) {
                    text_depth = None;
                }
                if in_body && word && excluded.is_none() && element.local_name().as_ref() == b"p" {
                    output.push('\n');
                }
                if excluded == Some(depth) {
                    excluded = None;
                }
                if depth == 2 && word && element.local_name().as_ref() == b"body" {
                    in_body = false;
                }
            }
            Event::Text(text) if text_depth.is_some() => output.push_str(&text.xml10_content()?),
            Event::CData(text) if text_depth.is_some() => output.push_str(&text.xml10_content()?),
            Event::GeneralRef(value) if text_depth.is_some() => output.push_str(&reference(value)?),
            _ => {}
        }
        ensure!(
            output.len() <= MAX_DOCUMENT_BYTES,
            "Extracted DOCX text exceeds 2 MiB"
        );
        Ok(())
    })?;
    ensure!(body_seen, "DOCX is missing a Word body");
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_and_mid_xml_cancellation_stop_before_a_result() {
        let cancel = AtomicBool::new(false);
        let mut budget = Budget::new(&cancel);
        budget.deadline = Instant::now();
        assert!(
            extract_docx(b"PK\x03\x04", &budget)
                .unwrap_err()
                .to_string()
                .contains("timed out")
        );
        let budget = Budget::new(&cancel);
        let result = visit_xml(
            "<root><child>text</child></root>",
            &budget,
            |_, _, event| {
                if matches!(event, Event::Text(_)) {
                    cancel.store(true, Ordering::Release);
                }
                Ok(())
            },
        );
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }
}
