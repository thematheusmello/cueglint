//! Complete-or-error text over lopdf objects; runs only in the bounded child.
use crate::MAX_DOCUMENT_BYTES;
use anyhow::{Context, Result, bail, ensure};
use lopdf::{
    Dictionary, Document, Encoding, LoadOptions, Object, ObjectId, Stream, content::Content,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
};

const MAX_STREAM_BYTES: usize = 4 * 1024 * 1024;
const MAX_DECODED_BYTES: usize = 16 * 1024 * 1024;
const MAX_OPERATIONS: usize = 200_000;
const MAX_MAPPINGS: usize = 65_536;

#[derive(Default)]
struct Work {
    decoded: usize,
    operations: usize,
    mappings: usize,
}

impl Work {
    fn decode(&mut self, stream: &Stream) -> Result<Vec<u8>> {
        let remaining = MAX_DECODED_BYTES
            .saturating_sub(self.decoded)
            .min(MAX_STREAM_BYTES);
        let filters = if stream.dict.has(b"Filter") {
            stream.filters()?
        } else {
            Vec::new()
        };
        ensure!(
            filters.is_empty() || filters == [b"FlateDecode".as_slice()],
            "Unsupported PDF stream compression"
        );
        if let Ok(parameters) = stream.dict.get(b"DecodeParms") {
            let parameters = if let Object::Array(values) = parameters {
                ensure!(values.len() == 1, "Unsupported PDF filter parameters");
                &values[0]
            } else {
                parameters
            };
            if !parameters.is_null() {
                let dict = parameters.as_dict()?;
                ensure!(
                    dict.is_empty() || (dict.len() == 1 && dict.get(b"Predictor")?.as_i64()? == 1),
                    "PDF stream predictors are unsupported"
                );
            }
        }
        let bytes = if filters.is_empty() {
            ensure!(
                stream.content.len() <= remaining,
                "PDF expanded stream exceeds its budget"
            );
            stream.content.clone()
        } else {
            let mut decoder = flate2::bufread::ZlibDecoder::new(stream.content.as_slice());
            let mut bytes = Vec::new();
            decoder
                .by_ref()
                .take(remaining as u64 + 1)
                .read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() <= remaining && decoder.total_in() == stream.content.len() as u64,
                "PDF Flate stream is incomplete, has trailing data or exceeds its budget"
            );
            bytes
        };
        self.decoded += bytes.len();
        ensure!(
            self.decoded <= MAX_DECODED_BYTES,
            "PDF expanded content exceeds 16 MiB"
        );
        Ok(bytes)
    }

    fn content(&mut self, bytes: &[u8]) -> Result<Content<Vec<lopdf::content::Operation>>> {
        let content = Content::decode_strict(bytes)?;
        self.operations += content.operations.len();
        ensure!(
            self.operations <= MAX_OPERATIONS,
            "PDF exceeds 200000 operations"
        );
        Ok(content)
    }
}

pub(super) fn extract(bytes: &[u8]) -> Result<String> {
    ensure!(
        bytes.starts_with(b"%PDF-") && bytes.len() <= MAX_DOCUMENT_BYTES,
        "Invalid PDF input"
    );
    ensure!(
        !hybrid_marker(bytes),
        "Hybrid PDF cross-references are unsupported"
    );
    let document = Document::load_mem_with_options(
        bytes,
        LoadOptions {
            strict: true,
            max_decompressed_size: Some(MAX_STREAM_BYTES),
            ..Default::default()
        },
    )?;
    ensure!(
        document.encryption_state.is_none() && !document.trailer.has(b"Encrypt"),
        "Encrypted PDFs are unsupported"
    );
    ensure!(
        document.xref_recovery.is_none(),
        "Repaired PDF cross-references are unsupported"
    );
    ensure!(
        document.xref_section_count == 1 && document.linearization.is_none(),
        "Incremental and linearized PDF revisions need further validation"
    );
    ensure!(
        document.objects.len() <= 4096 && document.reference_table.entries.len() <= 4097,
        "PDF exceeds 4096 objects"
    );
    for (&number, entry) in &document.reference_table.entries {
        let id = match entry {
            lopdf::xref::XrefEntry::Normal { generation, .. } => (number, *generation),
            lopdf::xref::XrefEntry::Compressed { .. } => (number, 0),
            _ => continue,
        };
        ensure!(
            document.has_object(id),
            "PDF cross-reference object is missing"
        );
    }
    let mut work = Work::default();
    for object in document.objects.values() {
        check_object(object, 0)?;
        if let Object::Stream(stream) = object {
            if stream.dict.has_type(b"ObjStm") || stream.dict.has_type(b"XRef") {
                work.decode(stream)?;
            }
        }
    }
    if matches!(
        document.reference_table.cross_reference_type,
        lopdf::xref::XrefType::CrossReferenceStream
    ) {
        let has_xref = document
            .reference_table
            .entries
            .iter()
            .any(|(&number, entry)| {
                if let lopdf::xref::XrefEntry::Normal { offset, generation } = entry {
                    *offset as usize == document.xref_start
                        && document
                            .objects
                            .get(&(number, *generation))
                            .is_some_and(|object| {
                                object
                                    .as_stream()
                                    .is_ok_and(|stream| stream.dict.has_type(b"XRef"))
                            })
                } else {
                    false
                }
            });
        ensure!(
            has_xref,
            "PDF cross-reference stream could not be integrity-checked"
        );
    }
    let root = document.catalog()?.get(b"Pages")?.as_reference()?;
    let mut pages = Vec::new();
    walk_pages(
        &document,
        root,
        None,
        None,
        &mut BTreeSet::new(),
        &mut pages,
        0,
    )?;
    ensure!(!pages.is_empty(), "PDF has no pages");
    let mut output = String::new();
    for (page, resources) in pages {
        let fonts = fonts(&document, resources, &mut work)?;
        let encodings = fonts
            .iter()
            .map(|(name, (font, profile))| {
                let encoding = font.get_font_encoding_with_limit(&document, MAX_STREAM_BYTES)?;
                ensure!(
                    matches!(
                        (&encoding, profile),
                        (Encoding::UnicodeMapEncoding(_), Some(_))
                            | (
                                Encoding::OneByteEncoding(_) | Encoding::Differences(_),
                                None
                            )
                    ),
                    "PDF font encoding fallback is unsupported"
                );
                if let Ok(object) = font.get(b"Encoding") {
                    if document.dereference(object)?.1.as_dict().is_ok() {
                        ensure!(
                            matches!(encoding, Encoding::Differences(_)),
                            "PDF font differences could not be decoded"
                        );
                    }
                }
                Ok((name.clone(), (encoding, profile.as_ref())))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let page = document.get_dictionary(page)?;
        ensure!(
            !page.has(b"Annots"),
            "PDF annotation text regions are unsupported"
        );
        let contents = document
            .dereference(
                page.get(b"Contents")
                    .context("PDF page has no extractable text content")?,
            )?
            .1;
        let streams: Vec<&Object> = match contents {
            Object::Stream(_) => vec![contents],
            Object::Array(streams) => streams.iter().collect(),
            _ => bail!("Invalid PDF page contents"),
        };
        let mut data = Vec::new();
        for object in streams {
            let stream = document.dereference(object)?.1.as_stream()?;
            let decoded = work.decode(stream)?;
            ensure!(
                data.len() + decoded.len() + 1 <= MAX_STREAM_BYTES,
                "PDF page content exceeds 4 MiB"
            );
            data.extend_from_slice(&decoded);
            data.push(b'\n');
        }
        let content = work.content(&data)?;
        let before = output.len();
        collect(&content, &encodings, &mut output)?;
        ensure!(
            !output[before..].trim().is_empty(),
            "PDF has an image-only, scanned or textless page"
        );
        line(&mut output)?;
    }
    Ok(output)
}

fn hybrid_marker(bytes: &[u8]) -> bool {
    // shortcut: reject this unsupported name even inside strings/compressed bytes, replace the guard when the parser preserves hybrid metadata.
    bytes.iter().enumerate().any(|(start, byte)| {
        if *byte != b'/' {
            return false;
        }
        let mut rest = &bytes[start + 1..];
        for expected in b"XRefStm" {
            if rest.first() == Some(&b'#') {
                let hex = |byte: &u8| (*byte as char).to_digit(16);
                let value = rest
                    .get(1)
                    .and_then(hex)
                    .zip(rest.get(2).and_then(hex))
                    .map(|(a, b)| (a * 16 + b) as u8);
                if value != Some(*expected) {
                    return false;
                }
                rest = &rest[3..];
            } else {
                if rest.first() != Some(expected) {
                    return false;
                }
                rest = &rest[1..];
            }
        }
        rest.first()
            .is_none_or(|byte| byte.is_ascii_whitespace() || b"\0()<>[]{}/%".contains(byte))
    })
}

fn check_object(object: &Object, depth: usize) -> Result<()> {
    ensure!(depth <= 64, "PDF object nesting exceeds 64");
    match object {
        Object::Array(items) => {
            for item in items {
                check_object(item, depth + 1)?;
            }
        }
        Object::Dictionary(dict) => {
            for (_, item) in dict.iter() {
                check_object(item, depth + 1)?;
            }
        }
        Object::Stream(stream) => {
            for (_, item) in stream.dict.iter() {
                check_object(item, depth + 1)?;
            }
        }
        Object::Real(value) => ensure!(value.is_finite(), "Non-finite PDF number"),
        _ => {}
    }
    Ok(())
}

fn walk_pages<'a>(
    document: &'a Document,
    id: ObjectId,
    parent: Option<ObjectId>,
    resources: Option<&'a Dictionary>,
    seen: &mut BTreeSet<ObjectId>,
    pages: &mut Vec<(ObjectId, Option<&'a Dictionary>)>,
    depth: usize,
) -> Result<usize> {
    ensure!(
        depth <= 64 && seen.insert(id),
        "PDF page tree is cyclic or too deep"
    );
    let node = document.get_dictionary(id)?;
    if let Some(parent) = parent {
        ensure!(
            node.get(b"Parent")?.as_reference()? == parent,
            "Inconsistent PDF page parent"
        );
    }
    let resources = if node.has(b"Resources") {
        Some(document.dereference(node.get(b"Resources")?)?.1.as_dict()?)
    } else {
        resources
    };
    match node.get(b"Type")?.as_name()? {
        b"Page" => {
            ensure!(pages.len() < 128, "PDF exceeds 128 pages");
            pages.push((id, resources));
            Ok(1)
        }
        b"Pages" => {
            let mut count = 0;
            for child in document.dereference(node.get(b"Kids")?)?.1.as_array()? {
                count += walk_pages(
                    document,
                    child.as_reference()?,
                    Some(id),
                    resources,
                    seen,
                    pages,
                    depth + 1,
                )?;
            }
            ensure!(
                node.get(b"Count")?.as_i64()? == count as i64,
                "Inconsistent PDF page count"
            );
            Ok(count)
        }
        _ => bail!("Invalid PDF page tree node"),
    }
}

struct CMap {
    width: usize,
    spaces: Vec<(Vec<u8>, Vec<u8>)>,
}

fn fonts(
    document: &Document,
    resources: Option<&Dictionary>,
    work: &mut Work,
) -> Result<BTreeMap<Vec<u8>, (Dictionary, Option<CMap>)>> {
    let mut result = BTreeMap::new();
    let Some(resources) = resources else {
        return Ok(result);
    };
    let Ok(fonts) = resources.get(b"Font") else {
        return Ok(result);
    };
    for (name, object) in document.dereference(fonts)?.1.as_dict()?.iter() {
        let mut font = document.dereference(object)?.1.as_dict()?.clone();
        ensure!(font.has_type(b"Font"), "Invalid PDF font dictionary");
        let subtype = font.get(b"Subtype")?.as_name()?;
        font.get(b"BaseFont")?.as_name()?;
        ensure!(
            matches!(subtype, b"Type1" | b"TrueType" | b"Type0"),
            "Unsupported PDF font subtype"
        );
        let profile = if font.has(b"ToUnicode") {
            let bytes = work.decode(
                document
                    .dereference(font.get(b"ToUnicode")?)?
                    .1
                    .as_stream()?,
            )?;
            let profile = cmap(&bytes, work)?;
            ensure!(
                subtype == b"Type0" || profile.width == 1,
                "Simple PDF fonts require one-byte character codes"
            );
            if subtype == b"Type0" {
                ensure!(
                    font.get(b"Encoding")?.as_name()? == b"Identity-H" && profile.width == 2,
                    "Composite PDF fonts require horizontal two-byte identity encoding"
                );
                let descendants = document
                    .dereference(font.get(b"DescendantFonts")?)?
                    .1
                    .as_array()?;
                ensure!(
                    descendants.len() == 1,
                    "PDF composite font needs one descendant"
                );
                let descendant = document.dereference(&descendants[0])?.1.as_dict()?;
                ensure!(
                    descendant.has_type(b"Font")
                        && matches!(
                            descendant.get(b"Subtype")?.as_name()?,
                            b"CIDFontType0" | b"CIDFontType2"
                        ),
                    "Unsupported PDF descendant font"
                );
                document
                    .dereference(descendant.get(b"FontDescriptor")?)?
                    .1
                    .as_dict()?;
            }
            // ToUnicode defines extraction independently of the glyph encoding; avoid the library's encoding-first path.
            font.remove(b"Encoding");
            font.set("ToUnicode", Stream::new(Dictionary::new(), bytes));
            Some(profile)
        } else {
            ensure!(subtype != b"Type0", "Composite PDF font has no Unicode map");
            if let Ok(encoding) = font.get(b"Encoding") {
                let encoding = document.dereference(encoding)?.1;
                if let Ok(dict) = encoding.as_dict() {
                    if dict.has(b"Type") {
                        ensure!(
                            dict.has_type(b"Encoding"),
                            "Invalid PDF encoding dictionary"
                        );
                    }
                    check_base_encoding(dict.get(b"BaseEncoding")?.as_name()?)?;
                    let mut next = None;
                    for item in dict.get(b"Differences")?.as_array()? {
                        match item {
                            Object::Integer(code) if (0..=255).contains(code) => next = Some(*code),
                            Object::Name(_) => {
                                let code =
                                    next.context("PDF font differences lack a starting code")?;
                                ensure!(code <= 255, "PDF font differences overflow");
                                next = Some(code + 1);
                            }
                            _ => bail!("Invalid PDF font differences"),
                        }
                    }
                } else {
                    check_base_encoding(encoding.as_name()?)?;
                }
            } else {
                ensure!(
                    subtype == b"Type1"
                        && matches!(
                            font.get(b"BaseFont")?.as_name()?,
                            b"Helvetica"
                                | b"Helvetica-Bold"
                                | b"Helvetica-Oblique"
                                | b"Helvetica-BoldOblique"
                                | b"Courier"
                                | b"Courier-Bold"
                                | b"Courier-Oblique"
                                | b"Courier-BoldOblique"
                                | b"Times-Roman"
                                | b"Times-Bold"
                                | b"Times-Italic"
                                | b"Times-BoldItalic"
                        ),
                    "PDF font needs an explicit supported encoding or Unicode map"
                );
            }
            None
        };
        result.insert(name.clone(), (font, profile));
    }
    Ok(result)
}

fn check_base_encoding(name: &[u8]) -> Result<()> {
    ensure!(
        matches!(
            name,
            b"StandardEncoding" | b"WinAnsiEncoding" | b"MacRomanEncoding" | b"MacExpertEncoding"
        ),
        "Unsupported PDF font encoding"
    );
    Ok(())
}

fn code(object: &Object) -> Result<&[u8]> {
    match object {
        Object::String(bytes, lopdf::StringFormat::Hexadecimal)
            if (1..=4).contains(&bytes.len()) =>
        {
            Ok(bytes)
        }
        _ => bail!("Invalid PDF CMap character code"),
    }
}

fn utf16(bytes: &[u8]) -> Result<String> {
    ensure!(
        bytes.len() % 2 == 0 && !bytes.is_empty(),
        "Invalid PDF Unicode destination"
    );
    Ok(String::from_utf16(
        &bytes
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>(),
    )?)
}

fn cmap(bytes: &[u8], work: &mut Work) -> Result<CMap> {
    let content = work.content(bytes)?;
    let mut result = CMap {
        width: 0,
        spaces: Vec::new(),
    };
    let mut section = None;
    let mut begun = false;
    let mut ended = false;
    for operation in content.operations {
        let operands = &operation.operands;
        let operator = operation.operator.as_str();
        if let Some((kind, count)) = section.take() {
            let arity = if kind == "beginbfrange" { 3 } else { 2 };
            let end = match kind {
                "beginbfrange" => "endbfrange",
                "beginbfchar" => "endbfchar",
                _ => "endcodespacerange",
            };
            ensure!(
                operator == end && operands.len() == count * arity,
                "Inconsistent PDF CMap block count"
            );
            for item in operands.chunks_exact(arity) {
                let low = code(&item[0])?;
                if kind == "begincodespacerange" {
                    let high = code(&item[1])?;
                    ensure!(
                        low.len() == high.len() && low.iter().zip(high).all(|(a, b)| a <= b),
                        "Invalid PDF CMap codespace"
                    );
                    ensure!(
                        result.width == 0 || result.width == low.len(),
                        "Variable-width PDF CMaps are unsupported"
                    );
                    result.width = low.len();
                    result.spaces.push((low.to_vec(), high.to_vec()));
                    continue;
                }
                let span = if kind == "beginbfrange" {
                    let high = code(&item[1])?;
                    ensure!(
                        low.len() == high.len() && low <= high,
                        "Invalid PDF CMap range"
                    );
                    let number = |value: &[u8]| {
                        value
                            .iter()
                            .fold(0u64, |n, byte| n * 256 + u64::from(*byte))
                    };
                    number(high) - number(low) + 1
                } else {
                    1
                };
                ensure!(
                    span <= MAX_MAPPINGS as u64
                        && work.mappings as u64 + span <= MAX_MAPPINGS as u64,
                    "PDF CMap exceeds 65536 mappings"
                );
                work.mappings += span as usize;
                match &item[arity - 1] {
                    Object::String(bytes, lopdf::StringFormat::Hexadecimal) => {
                        utf16(bytes)?;
                        ensure!(
                            bytes.len() <= 512
                                && u64::from(*bytes.last().unwrap()) + span - 1 <= 255,
                            "PDF CMap Unicode range overflows its final byte"
                        );
                    }
                    Object::Array(values) if kind == "beginbfrange" => {
                        ensure!(
                            values.len() as u64 == span,
                            "PDF CMap destination array is incomplete"
                        );
                        for value in values {
                            utf16(value.as_str()?)?;
                        }
                    }
                    _ => bail!("Unsupported PDF CMap destination"),
                }
            }
            continue;
        }
        match operator {
            "begincmap" => {
                ensure!(!begun && !ended, "Repeated PDF CMap");
                begun = true;
            }
            "endcmap" => {
                ensure!(begun && !ended, "Invalid PDF CMap end");
                ended = true;
            }
            "begincodespacerange" | "beginbfchar" | "beginbfrange" => {
                ensure!(
                    begun && !ended && operands.len() == 1,
                    "Invalid PDF CMap block"
                );
                let count = operands[0].as_i64()?;
                ensure!(
                    (0..=MAX_MAPPINGS as i64).contains(&count),
                    "PDF CMap block exceeds its budget"
                );
                section = Some((
                    match operator {
                        "beginbfchar" => "beginbfchar",
                        "beginbfrange" => "beginbfrange",
                        _ => "begincodespacerange",
                    },
                    count as usize,
                ));
            }
            "def" => {
                let name = operands
                    .first()
                    .context("Invalid PDF CMap metadata")?
                    .as_name()?;
                ensure!(
                    matches!(
                        name,
                        b"CIDSystemInfo"
                            | b"CMapName"
                            | b"CMapType"
                            | b"CMapVersion"
                            | b"WMode"
                            | b"UIDOffset"
                            | b"XUID"
                    ),
                    "Unsupported PDF CMap metadata"
                );
                if name == b"CMapType" {
                    ensure!(
                        operands.get(1).context("Missing PDF CMap type")?.as_i64()? == 2,
                        "Unsupported PDF CMap type"
                    );
                }
                if name == b"WMode" {
                    ensure!(
                        operands
                            .get(1)
                            .context("Missing PDF CMap writing mode")?
                            .as_i64()?
                            == 0,
                        "Vertical PDF CMaps are unsupported"
                    );
                }
            }
            "findresource" | "begin" | "dict" | "CMapName" | "currentdict" | "defineresource"
            | "pop" | "end" => {}
            _ => bail!("Unsupported PDF CMap operator"),
        }
    }
    ensure!(
        section.is_none() && begun && ended && result.width > 0 && !result.spaces.is_empty(),
        "Incomplete PDF Unicode map"
    );
    Ok(result)
}

type Fonts<'a> = BTreeMap<Vec<u8>, (Encoding<'a>, Option<&'a CMap>)>;

fn append(output: &mut String, text: &str) -> Result<()> {
    ensure!(
        output.len() + text.len() <= MAX_DOCUMENT_BYTES && !text.contains(['\0', '\u{fffd}']),
        "PDF text is incomplete, contains NUL or exceeds 2 MiB"
    );
    output.push_str(text);
    Ok(())
}

fn line(output: &mut String) -> Result<()> {
    if !output.is_empty() && !output.ends_with('\n') {
        append(output, "\n")?;
    }
    Ok(())
}

fn show(output: &mut String, bytes: &[u8], font: &(Encoding<'_>, Option<&CMap>)) -> Result<()> {
    ensure!(
        bytes.len() <= 32_767,
        "PDF content string exceeds 32767 bytes"
    );
    if let (Encoding::UnicodeMapEncoding(map), Some(profile)) = (&font.0, font.1) {
        ensure!(
            bytes.len() % profile.width == 0,
            "Truncated PDF character code"
        );
        for bytes in bytes.chunks_exact(profile.width) {
            ensure!(
                profile.spaces.iter().any(|(low, high)| bytes
                    .iter()
                    .zip(low.iter().zip(high))
                    .all(|(byte, (low, high))| low <= byte && byte <= high)),
                "PDF character lies outside its codespace"
            );
            let code = bytes
                .iter()
                .fold(0u32, |value, byte| value * 256 + u32::from(*byte));
            let units = map
                .get(code, profile.width as u8)
                .context("PDF character has no Unicode mapping")?;
            append(output, &String::from_utf16(&units)?)?;
        }
    } else {
        for byte in bytes {
            let text = font.0.bytes_to_string(&[*byte])?;
            ensure!(!text.is_empty(), "PDF character has no supported mapping");
            append(output, &text)?;
        }
    }
    Ok(())
}

fn numeric(operands: &[Object], count: usize) -> Result<()> {
    ensure!(operands.len() == count, "Wrong PDF operator operand count");
    for operand in operands {
        ensure!(operand.as_float()?.is_finite(), "Invalid PDF number");
    }
    Ok(())
}

fn collect(
    content: &Content<Vec<lopdf::content::Operation>>,
    fonts: &Fonts<'_>,
    output: &mut String,
) -> Result<()> {
    let mut in_text = false;
    let mut font: Option<&[u8]> = None;
    let mut stack = Vec::new();
    for operation in &content.operations {
        let operands = operation.operands.as_slice();
        match operation.operator.as_str() {
            "BT" => {
                ensure!(!in_text && operands.is_empty(), "Invalid PDF text object");
                in_text = true;
                line(output)?;
            }
            "ET" => {
                ensure!(
                    in_text && operands.is_empty(),
                    "Invalid PDF text object end"
                );
                in_text = false;
                line(output)?;
            }
            "Tf" => {
                ensure!(
                    in_text && operands.len() == 2 && operands[1].as_float()?.is_finite(),
                    "Invalid PDF font selection"
                );
                let name = operands[0].as_name()?;
                ensure!(fonts.contains_key(name), "PDF text uses a missing font");
                font = Some(name);
            }
            "Tj" | "TJ" | "'" | "\"" => {
                ensure!(in_text, "PDF text appears outside a text object");
                let font = fonts
                    .get(font.context("PDF text has no selected font")?)
                    .context("PDF font is unavailable")?;
                if matches!(operation.operator.as_str(), "'" | "\"") {
                    line(output)?;
                }
                if operation.operator == "\"" {
                    ensure!(operands.len() == 3, "Invalid PDF quoted text");
                    numeric(&operands[..2], 2)?;
                    show(output, operands[2].as_str()?, font)?;
                } else {
                    ensure!(operands.len() == 1, "Invalid PDF text operands");
                    if operation.operator == "TJ" {
                        for item in operands[0].as_array()? {
                            match item {
                                Object::String(bytes, _) => show(output, bytes, font)?,
                                Object::Integer(_) | Object::Real(_) => {
                                    if item.as_float()? < -100.0 {
                                        append(output, " ")?;
                                    }
                                }
                                _ => bail!("Invalid PDF adjusted text"),
                            }
                        }
                    } else {
                        show(output, operands[0].as_str()?, font)?;
                    }
                }
            }
            "T*" => {
                ensure!(in_text && operands.is_empty(), "Invalid PDF line advance");
                line(output)?;
            }
            "Td" | "TD" | "Tm" => {
                ensure!(in_text, "PDF text positioning outside a text object");
                numeric(operands, if operation.operator == "Tm" { 6 } else { 2 })?;
                line(output)?;
            }
            "Tc" | "Tw" | "Tz" | "TL" | "Ts" => numeric(operands, 1)?,
            "Tr" => {
                numeric(operands, 1)?;
                ensure!(
                    operands[0].as_i64()? == 0,
                    "Hidden, clipped or stroked PDF text is unsupported"
                );
            }
            "q" => {
                ensure!(
                    operands.is_empty() && stack.len() < 64,
                    "PDF graphics state exceeds its budget"
                );
                stack.push(font);
            }
            "Q" => {
                ensure!(operands.is_empty(), "Invalid PDF graphics restore");
                font = stack.pop().context("Unbalanced PDF graphics restore")?;
            }
            "cm" | "c" => numeric(operands, 6)?,
            "m" | "l" => numeric(operands, 2)?,
            "v" | "y" | "re" | "K" | "k" => numeric(operands, 4)?,
            "RG" | "rg" => numeric(operands, 3)?,
            "G" | "g" | "w" | "J" | "j" | "M" | "i" => numeric(operands, 1)?,
            "h" | "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" | "W" | "W*" => {
                ensure!(operands.is_empty(), "Invalid PDF path operator")
            }
            "d" => {
                ensure!(operands.len() == 2, "Invalid PDF dash pattern");
                for value in operands[0].as_array()? {
                    numeric(std::slice::from_ref(value), 1)?;
                }
                numeric(&operands[1..], 1)?;
            }
            "ri" => {
                ensure!(operands.len() == 1, "Invalid PDF render intent");
                operands[0].as_name()?;
            }
            // shortcut: forms/images/marked content/extended graphics states can hide text, support them only with complete recursive extraction and visibility validation.
            _ => bail!("PDF contains an unsupported content region"),
        }
    }
    ensure!(
        !in_text && stack.is_empty(),
        "Unclosed PDF text or graphics state"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn hybrid_marker_includes_escaped_names_without_prefix_matches() {
        for bytes in [
            b"/XRefStm 10".as_slice(),
            b"/#58Ref#53tm 10",
            b"/#58#52#65#66#53#74#6d>",
        ] {
            assert!(super::hybrid_marker(bytes));
        }
        for bytes in [
            b"/XRefStmExtra 10".as_slice(),
            b"/XRef#0 10",
            b"/Other 10",
            b"/",
        ] {
            assert!(!super::hybrid_marker(bytes));
        }
    }
    #[test]
    fn trusted_original_pdf_preserves_all_pages_and_unicode() {
        // Only this tiny authored positive fixture is parsed in the test process; attacks use the CLI child.
        let text = super::extract(include_bytes!(
            "../tests/fixtures/knowledge/original-workshop.pdf"
        ))
        .unwrap();
        assert_eq!(
            text,
            include_str!("../tests/fixtures/knowledge/expected-pdf.txt")
        );
    }
}
