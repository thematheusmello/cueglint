use cueglint_core::{MAX_DOCUMENT_BYTES, Store};
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

const ORIGINAL: &[u8] =
    include_bytes!("../../cueglint-core/tests/fixtures/knowledge/original-workshop.pdf");
const EXPECTED: &str =
    include_str!("../../cueglint-core/tests/fixtures/knowledge/expected-pdf.txt");
const FONT: &[u8] =
    b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>";

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("cueglint-pdf-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn command(database: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cueglint-cli"))
        .arg("--db")
        .arg(database)
        .args(args)
        .output()
        .unwrap()
}

fn json(database: &Path, args: &[&str]) -> Value {
    let output = command(database, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

// Original fixture writer only: no application or parser code is duplicated here.
fn pdf(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        write!(bytes, "{} 0 obj\n", index + 1).unwrap();
        bytes.extend_from_slice(object);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let start = bytes.len();
    write!(
        bytes,
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    )
    .unwrap();
    for offset in offsets {
        writeln!(bytes, "{offset:010} 00000 n ").unwrap();
    }
    write!(
        bytes,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n",
        objects.len() + 1
    )
    .unwrap();
    bytes
}

fn stream(bytes: &[u8], filter: &str) -> Vec<u8> {
    let mut result = format!("<< /Length {} {filter} >>\nstream\n", bytes.len()).into_bytes();
    result.extend_from_slice(bytes);
    result.extend_from_slice(b"\nendstream");
    result
}

fn objects(content: Vec<u8>, font: &[u8], extra: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_vec(),
        font.to_vec(), content,
    ];
    objects.extend(extra);
    objects
}

fn page(content: &[u8]) -> Vec<u8> {
    pdf(&objects(stream(content, ""), FONT, vec![]))
}

fn multipage(contents: &[Vec<u8>]) -> Vec<u8> {
    let font = contents.len() + 3;
    let kids = (3..font)
        .map(|id| format!("{id} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!(
            "<< /Type /Pages /Kids [{kids}] /Count {} >>",
            contents.len()
        )
        .into_bytes(),
    ];
    for index in 0..contents.len() {
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 {font} 0 R >> >> /Contents {} 0 R >>", font + 1 + index).into_bytes());
    }
    objects.push(FONT.to_vec());
    objects.extend(contents.iter().cloned());
    pdf(&objects)
}

fn cmap(mapping: &str, width: usize) -> Vec<u8> {
    let high = "ff".repeat(width);
    let low = "00".repeat(width);
    format!("/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /OriginalCueglint def\n/CMapType 2 def\n1 begincodespacerange\n<{low}> <{high}>\nendcodespacerange\n{mapping}\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n").into_bytes()
}

fn mapped(mapping: &[u8], text: &[u8]) -> Vec<u8> {
    pdf(&objects(stream(text, ""), b"<< /Type /Font /Subtype /Type0 /BaseFont /OriginalFixture /Encoding /Identity-H /ToUnicode 6 0 R /DescendantFonts [7 0 R] >>", vec![stream(mapping, ""), b"<< /Type /Font /Subtype /CIDFontType2 /BaseFont /OriginalFixture /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 8 0 R /CIDToGIDMap /Identity /DW 500 >>".to_vec(), b"<< /Type /FontDescriptor /FontName /OriginalFixture /Flags 32 /FontBBox [0 -200 1000 900] /ItalicAngle 0 /Ascent 900 /Descent -200 /CapHeight 700 /StemV 80 >>".to_vec()]))
}

fn normalized_backup(store: &Store) -> Value {
    let mut value: Value = serde_json::from_str(&store.backup_json().unwrap()).unwrap();
    assert!(
        value
            .as_object_mut()
            .unwrap()
            .remove("exported_at")
            .unwrap()
            .as_i64()
            .unwrap()
            > 0
    );
    value
}

#[test]
fn actual_cli_pdf_preserves_unicode_retrieval_deletion_backup_and_existing_notes() {
    let directory = Directory::new();
    let database = directory.0.join("data.sqlite3");
    let mut store = Store::open(&database).unwrap();
    let session = store.start_session("Original PDF review", None).unwrap();
    store
        .update_notes(&session.id, "Keep the user's original notes.")
        .unwrap();
    let existing = store
        .ingest_knowledge(
            "Existing UTF-8",
            "original.txt",
            "Keep café and €240 unchanged.",
        )
        .unwrap();
    drop(store);
    let docx_path = directory.write(
        "existing.docx",
        include_bytes!("../../cueglint-core/tests/fixtures/knowledge/original-workshop.docx"),
    );
    let docx = json(
        &database,
        &["knowledge", "add", docx_path.to_str().unwrap()],
    );
    let path = directory.write("original.PDF", ORIGINAL);
    let added = json(
        &database,
        &[
            "knowledge",
            "add",
            path.to_str().unwrap(),
            "--title",
            "Original PDF",
        ],
    );
    assert_eq!(added["text"], EXPECTED);
    let found = json(
        &database,
        &["knowledge", "search", "COPPER-731 copper lantern"],
    );
    assert!(!found.as_array().unwrap().is_empty());
    for snippet in found.as_array().unwrap() {
        assert_eq!(snippet["document_id"], added["id"]);
        assert_eq!(snippet["source"], path.to_str().unwrap());
        let start = snippet["start_byte"].as_u64().unwrap() as usize;
        let end = snippet["end_byte"].as_u64().unwrap() as usize;
        assert_eq!(snippet["text"], &EXPECTED[start..end]);
    }
    let mut store = Store::open(&database).unwrap();
    let request = store
        .begin_request(&session.id, "What is COPPER-731?")
        .unwrap();
    assert!(!request.context.knowledge.is_empty());
    let backup = store.backup_json().unwrap();
    let mut restored = Store::open_in_memory().unwrap();
    restored.restore_backup(&backup, false).unwrap();
    assert_eq!(restored.knowledge().unwrap(), store.knowledge().unwrap());
    assert_eq!(
        restored.session(&session.id).unwrap().notes,
        "Keep the user's original notes."
    );
    drop(store);
    json(
        &database,
        &["knowledge", "delete", added["id"].as_str().unwrap()],
    );
    let mut store = Store::open(&database).unwrap();
    let remaining = store.knowledge().unwrap();
    assert_eq!(remaining.len(), 2);
    assert!(remaining.contains(&existing));
    assert!(
        remaining
            .iter()
            .any(|document| document.id == docx["id"].as_str().unwrap()
                && document.text
                    == include_str!("../../cueglint-core/tests/fixtures/knowledge/expected.txt"))
    );
    assert!(store.search_knowledge("COPPER-731", 5).unwrap().is_empty());
    assert!(
        !store
            .complete_request(&session.id, &request.id, "Discard this stale PDF answer")
            .unwrap()
    );
    assert_eq!(
        store.session(&session.id).unwrap().notes,
        "Keep the user's original notes."
    );
}

#[test]
fn supported_long_strings_and_fixed_width_font_mappings_are_complete() {
    let directory = Directory::new();
    let database = directory.0.join("data.sqlite3");
    let source = format!("BT /F1 12 Tf ({0}) Tj ({0}) Tj ET", "x".repeat(20_000));
    let path = directory.write("long.pdf", &page(source.as_bytes()));
    let document = json(&database, &["knowledge", "add", path.to_str().unwrap()]);
    assert_eq!(document["text"], format!("{}\n", "x".repeat(40_000)));
    let mapping = cmap(
        "3 beginbfchar\n<0001> <00e9>\n<0002> <20ac>\n<0003> <d83cdf3f>\nendbfchar",
        2,
    );
    let path = directory.write(
        "unicode.pdf",
        &mapped(&mapping, b"BT /F1 12 Tf <000100020003> Tj ET"),
    );
    let document = json(&database, &["knowledge", "add", path.to_str().unwrap()]);
    assert_eq!(document["text"], "é€🌿\n");
    let mut modern = lopdf::Document::load_mem(ORIGINAL).unwrap();
    let mut bytes = Vec::new();
    modern.save_modern(&mut bytes).unwrap();
    let path = directory.write("modern.pdf", &bytes);
    let document = json(&database, &["knowledge", "add", path.to_str().unwrap()]);
    assert_eq!(document["text"], EXPECTED);
}

#[test]
fn page_object_operation_decoding_input_and_output_limits_reject_whole_documents() {
    let directory = Directory::new();
    let database = directory.0.join("data.sqlite3");
    let content = stream(b"BT /F1 12 Tf (A) Tj ET", "");
    let pages = multipage(&vec![content; 129]);
    let mut many_objects = objects(stream(b"BT /F1 12 Tf (A) Tj ET", ""), FONT, vec![]);
    many_objects.extend((5..4097).map(|_| b"null".to_vec()));
    let operations = page(format!("BT /F1 12 Tf (A) Tj ET {}", "q Q ".repeat(100_001)).as_bytes());
    let mut large_text = b"BT /F1 12 Tf ".to_vec();
    for _ in 0..65 {
        write!(large_text, "({}) Tj ", "a".repeat(32_767)).unwrap();
    }
    large_text.extend_from_slice(b"ET");
    let mut text_stream = lopdf::Stream::new(lopdf::Dictionary::new(), large_text);
    text_stream.compress().unwrap();
    let output_limit = pdf(&objects(
        stream(&text_stream.content, "/Filter /FlateDecode"),
        FONT,
        vec![],
    ));
    let mut large_page = b"BT /F1 12 Tf (A) Tj ET\n%".to_vec();
    large_page.extend(vec![b'x'; 3_500_000]);
    large_page.push(b'\n');
    let mut large_page = lopdf::Stream::new(lopdf::Dictionary::new(), large_page);
    large_page.compress().unwrap();
    let total_decoding = multipage(&vec![
        stream(&large_page.content, "/Filter /FlateDecode");
        5
    ]);
    for (name, bytes) in [
        ("pages", pages),
        ("objects", pdf(&many_objects)),
        ("operations", operations),
        ("output", output_limit),
        ("decoded", total_decoding),
        (
            "single-string",
            page(format!("BT /F1 12 Tf ({}) Tj ET", "x".repeat(32_768)).as_bytes()),
        ),
        ("input", vec![b'x'; MAX_DOCUMENT_BYTES + 1]),
    ] {
        let path = directory.write(&format!("{name}.pdf"), &bytes);
        let output = command(&database, &["knowledge", "add", path.to_str().unwrap()]);
        assert!(!output.status.success(), "{name} bound was not enforced");
        assert!(output.stdout.is_empty());
        assert!(
            json(&database, &["knowledge", "list"])
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn malformed_unsupported_exhausted_and_partial_pdf_paths_preserve_all_stored_data() {
    let directory = Directory::new();
    let database = directory.0.join("data.sqlite3");
    let mut store = Store::open(&database).unwrap();
    let session = store.start_session("Preserve original work", None).unwrap();
    store
        .update_notes(&session.id, "Do not erase these notes.")
        .unwrap();
    store
        .ingest_knowledge("Original", "original.md", "Preserve the original library.")
        .unwrap();
    let before = normalized_backup(&store);
    drop(store);
    let mut cycle = objects(stream(b"BT /F1 12 Tf (A) Tj ET", ""), FONT, vec![]);
    cycle[1] = b"<< /Type /Pages /Kids [2 0 R] /Count 1 >>".to_vec();
    let nested = format!("{}0{}", "[".repeat(65), "]".repeat(65)).into_bytes();
    let mut compressed =
        lopdf::Stream::new(lopdf::Dictionary::new(), vec![b'x'; 4 * 1024 * 1024 + 1]);
    compressed.compress().unwrap();
    let mut bad_flate = lopdf::Stream::new(
        lopdf::Dictionary::new(),
        b"BT /F1 12 Tf (Complete text) Tj ET\n".repeat(32),
    );
    bad_flate.compress().unwrap();
    let mut bad_crc = bad_flate.content.clone();
    *bad_crc.last_mut().unwrap() ^= 1;
    let mut cases = vec![
        ("malformed", b"%PDF-1.7\n".to_vec()),
        ("missing-font", page(b"BT /Missing 12 Tf (partial) Tj ET")),
        ("unselected-font", page(b"BT (partial) Tj ET")),
        ("unknown-region", page(b"BT /F1 12 Tf (partial) Tj ET /Form Do")),
        ("scanned-page", page(b"/Image Do")),
        ("hidden-text", page(b"BT /F1 12 Tf 3 Tr (hidden) Tj ET")),
        ("broken-content", page(b"BT /F1 12 Tf (partial) Tj ET (unterminated")),
        ("cycle", pdf(&cycle)),
        ("nesting", pdf(&objects(stream(b"BT /F1 12 Tf (A) Tj ET", ""), FONT, vec![nested]))),
        ("flate-bomb", pdf(&objects(stream(&compressed.content, "/Filter /FlateDecode"), FONT, vec![]))),
        ("flate-crc", pdf(&objects(stream(&bad_crc, "/Filter /FlateDecode"), FONT, vec![]))),
        ("flate-truncated", pdf(&objects(stream(&bad_flate.content[..bad_flate.content.len()-2], "/Filter /FlateDecode"), FONT, vec![]))),
        ("huge-cmap-range", mapped(&cmap("1 beginbfrange\n<00000000> <ffffffff> <0041>\nendbfrange", 4), b"BT /F1 12 Tf <00000041> Tj ET")),
        ("partial-cmap", mapped(&cmap("1 beginbfrange\n<0001> <0003> [<0041>]\nendbfrange", 2), b"BT /F1 12 Tf <0001> Tj ET")),
        ("missing-cmap-code", mapped(&cmap("1 beginbfchar\n<0001> <0041>\nendbfchar", 2), b"BT /F1 12 Tf <00010002> Tj ET")),
        ("bad-cmap-count", mapped(&cmap("2 beginbfchar\n<0001> <0041>\nendbfchar", 2), b"BT /F1 12 Tf <0001> Tj ET")),
        ("bad-cmap-fallback", mapped(b"malformed Unicode map", b"BT /F1 12 Tf (partial) Tj ET")),
        ("unknown-difference", pdf(&objects(stream(b"BT /F1 12 Tf (A) Tj ET", ""), b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding << /Type /Encoding /BaseEncoding /WinAnsiEncoding /Differences [65 /CueglintNotAGlyph] >> >>", vec![]))),
    ];
    let mut encrypted = lopdf::Document::load_mem(ORIGINAL).unwrap();
    encrypted.trailer.set(
        "ID",
        vec![
            lopdf::Object::string_literal(b"OriginalFixtureID".to_vec()),
            lopdf::Object::string_literal(b"OriginalFixtureID".to_vec()),
        ],
    );
    let state = lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V1 {
        document: &encrypted,
        owner_password: "original-owner",
        user_password: "original-user",
        permissions: lopdf::Permissions::all(),
    })
    .unwrap();
    encrypted.encrypt(&state).unwrap();
    let mut encrypted_bytes = Vec::new();
    encrypted.save_to(&mut encrypted_bytes).unwrap();
    cases.push(("encrypted", encrypted_bytes));
    for (name, bytes) in cases {
        let path = directory.write(&format!("{name}.pdf"), &bytes);
        let output = command(&database, &["knowledge", "add", path.to_str().unwrap()]);
        assert!(!output.status.success(), "{name} was accepted");
        assert!(output.stdout.is_empty(), "{name} leaked a partial result");
        assert_eq!(
            normalized_backup(&Store::open(&database).unwrap()),
            before,
            "{name} changed existing data"
        );
    }
}

#[test]
fn child_allocation_work_exhaustion_has_a_distinct_exit_and_no_partial_stdout() {
    let source = format!(
        "{}{}{}",
        "(".repeat(90),
        "x".repeat(1_800_000),
        ")".repeat(90)
    )
    .into_bytes();
    let bytes = pdf(&objects(
        stream(b"BT /F1 12 Tf (A) Tj ET", ""),
        FONT,
        vec![source],
    ));
    assert!(bytes.len() < MAX_DOCUMENT_BYTES);
    let mut child = Command::new(env!("CARGO_BIN_EXE_cueglint-cli"))
        .arg("--cueglint-pdf-worker-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let writer = thread::spawn(move || {
        let _ = input.write_all(&bytes);
    });
    let mut stdout = child.stdout.take().unwrap();
    let reader = thread::spawn(move || {
        let mut output = Vec::new();
        stdout
            .by_ref()
            .take(MAX_DOCUMENT_BYTES as u64 + 1)
            .read_to_end(&mut output)
            .unwrap();
        output
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() >= Duration::from_secs(12) {
            child.kill().unwrap();
            break child.wait().unwrap();
        }
        thread::sleep(Duration::from_millis(10));
    };
    let _ = child.kill();
    let _ = child.wait();
    writer.join().unwrap();
    let output = reader.join().unwrap();
    assert_eq!(
        status.code(),
        Some(73),
        "allocation exhaustion must bypass parser/panic/GUI cleanup"
    );
    assert!(output.is_empty());
}
