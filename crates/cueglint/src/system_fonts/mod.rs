//! Source-only prototype: native font resolution and bounded in-memory loading.
//! Call from a loader job, never drawing. No font bytes are exported or embedded.
use anyhow::{Context, Result, ensure};
use eframe::egui;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use skrifa::{FontRef, MetadataProvider, Tag, raw::FileRef, string::StringId};
use std::{fs::File, io::Read, path::PathBuf};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;
const MAX_SET_BYTES: usize = 96 * 1024 * 1024;
const MAX_REQUESTS: usize = 16;
const MAX_FACES: u32 = 256;
const MAX_AXES: usize = 64;
const IDENTITY_TABLES: [[u8; 4]; 3] = [*b"head", *b"name", *b"maxp"];

#[derive(Clone, Copy, Debug)]
pub enum Weight {
    Regular,
    Medium,
    Semibold,
}

impl Weight {
    fn css(self) -> u16 {
        match self {
            Self::Regular => 400,
            Self::Medium => 500,
            Self::Semibold => 600,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Request {
    pub role: &'static str,
    pub points: f32,
    pub weight: Weight,
}

// No Debug/Serialize on font data: receipts contain identities, never payloads.
pub struct Attempt {
    pub font: Option<egui::FontData>,
    pub receipt: Value,
}

struct NativeFace {
    api: &'static str,
    path: PathBuf,
    family: String,
    postscript: Option<String>,
    index: Option<u32>,
    table_hashes: Vec<([u8; 4], String)>,
    coords: Vec<([u8; 4], f32)>,
    variations_known: bool,
    weight: Value,
}

/// One bounded role set; native objects are created and dropped on this caller's thread.
pub fn load_set(requests: &[Request]) -> Result<Vec<Attempt>> {
    ensure!(
        requests.len() <= MAX_REQUESTS,
        "native font role limit exceeded"
    );
    let mut remaining = MAX_SET_BYTES;
    let fallback = fallback_chain();
    Ok(requests
        .iter()
        .map(
            |&request| match load(request, remaining.min(MAX_FILE_BYTES)) {
                Ok((font, mut receipt)) => {
                    remaining -= font.font.len();
                    receipt["embedded_fallback_chain"] = fallback.clone();
                    Attempt {
                        font: Some(font),
                        receipt,
                    }
                }
                Err(error) => Attempt {
                    font: None,
                    receipt: json!({
                        "role": request.role,
                        "requested_points": request.points,
                        "requested_css_weight": request.weight.css(),
                        "source": "egui-embedded-fallback",
                        "fallback_reason": error.to_string(),
                        "reference_font_match": "unverified",
                        "embedded_fallback_chain": fallback,
                    }),
                },
            },
        )
        .collect())
}

fn load(request: Request, byte_limit: usize) -> Result<(egui::FontData, Value)> {
    ensure!(
        request.points.is_finite() && (4.0..=96.0).contains(&request.points),
        "invalid native font size"
    );
    ensure!(byte_limit > 0, "native font role-set byte limit exceeded");
    let native = query(request)?;
    ensure!(
        native.path.is_absolute(),
        "native font is not a local absolute path"
    );
    let file = File::open(&native.path).context("native font file unavailable")?;
    ensure!(
        file.metadata()?.is_file(),
        "native font is not a regular file"
    );
    let bytes = read_bounded(file, byte_limit)?;
    let index = select_face(&bytes, native.index, &native.table_hashes)?;
    let face = FontRef::from_index(&bytes, index).context("native font face is invalid")?;
    ensure!(
        face.axes().is_empty() || native.variations_known,
        "native variable-font coordinates unavailable"
    );
    ensure!(
        native.coords.len() <= MAX_AXES,
        "native font axis limit exceeded"
    );
    for &(tag, value) in &native.coords {
        let axis = face
            .axes()
            .get_by_tag(Tag::new(&tag))
            .context("native font axis missing from file")?;
        ensure!(
            value.is_finite() && (axis.min_value()..=axis.max_value()).contains(&value),
            "native font coordinate outside file axis range"
        );
    }
    let sfnt_postscript = face
        .localized_strings(StringId::POSTSCRIPT_NAME)
        .english_or_first()
        .map(|name| name.to_string());
    let receipt = json!({
        "role": request.role,
        "requested_points": request.points,
        "requested_css_weight": request.weight.css(),
        "source": native.api,
        "file_name": native.path.file_name().map(|name| name.to_string_lossy()),
        "file_sha256": hash(&bytes),
        "file_bytes": bytes.len(),
        "face_index": index,
        "native_family": native.family,
        "native_postscript": native.postscript,
        "sfnt_postscript": sfnt_postscript,
        "native_weight": native.weight,
        "native_variations_known": native.variations_known,
        "variations": native.coords.iter().map(|(tag, value)| json!({
            "tag": String::from_utf8_lossy(tag), "value": value,
        })).collect::<Vec<_>>(),
        "fallback_reason": null,
        "reference_font_match": "unverified",
    });
    let mut font = egui::FontData::from_owned(bytes);
    font.index = index;
    for (tag, value) in native.coords {
        font.tweak.coords.push(&tag, value);
    }
    Ok((font, receipt))
}

fn read_bounded(reader: impl Read, byte_limit: usize) -> Result<Vec<u8>> {
    ensure!(
        byte_limit <= MAX_FILE_BYTES,
        "invalid native font byte limit"
    );
    let mut bytes = Vec::new();
    reader
        .take(byte_limit as u64 + 1)
        .read_to_end(&mut bytes)
        .context("native font read failed")?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= byte_limit,
        "native font byte limit exceeded or empty file"
    );
    Ok(bytes)
}

fn select_face(
    bytes: &[u8],
    native_index: Option<u32>,
    tables: &[([u8; 4], String)],
) -> Result<u32> {
    let file = FileRef::new(bytes).context("native font container is invalid")?;
    let count = match &file {
        FileRef::Font(_) => 1,
        FileRef::Collection(collection) => collection.len(),
    };
    ensure!(
        count > 0 && count <= MAX_FACES,
        "native font collection face limit exceeded"
    );
    if let Some(index) = native_index {
        ensure!(index < count, "native font face index outside collection");
        FontRef::from_index(bytes, index).context("native font face is invalid")?;
        return Ok(index);
    }
    ensure!(
        tables.len() == IDENTITY_TABLES.len(),
        "native font has no exact face identity"
    );
    let mut matched = None;
    for (index, face) in file.fonts().enumerate() {
        let face = face.context("native font collection contains an invalid face")?;
        if tables.iter().all(|(tag, expected)| {
            face.table_data(Tag::new(tag))
                .is_some_and(|data| hash(data.as_bytes()) == *expected)
        }) {
            ensure!(matched.is_none(), "native font face identity is ambiguous");
            matched = Some(index as u32);
        }
    }
    matched.context("native font face does not match resolved file")
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn fallback_chain() -> Value {
    let fonts = egui::FontDefinitions::default();
    json!(
        fonts.families[&egui::FontFamily::Proportional]
            .iter()
            .map(|name| {
                let data = &fonts.font_data[name];
                json!({"name": name, "face_index": data.index, "sha256": hash(&data.font),
            "source": "egui0.35.0-embedded-default"})
            })
            .collect::<Vec<_>>()
    )
}

#[cfg(target_os = "macos")]
fn query(request: Request) -> Result<NativeFace> {
    macos::query(request)
}
#[cfg(target_os = "windows")]
fn query(request: Request) -> Result<NativeFace> {
    windows::query(request)
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn query(_: Request) -> Result<NativeFace> {
    anyhow::bail!("native system-font resolution unavailable on this host; embedded fonts retained")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn bounded_read_rejects_oversize_empty_and_invalid_payloads() {
        assert_eq!(read_bounded(Cursor::new([1, 2, 3]), 3).unwrap(), [1, 2, 3]);
        let mut growing = Cursor::new([1_u8; 100]);
        assert!(read_bounded(&mut growing, 3).is_err());
        assert_eq!(
            growing.position(),
            4,
            "read at most the byte limit plus one"
        );
        assert!(read_bounded(Cursor::new([]), 3).is_err());
        assert!(select_face(b"not a font", Some(0), &[]).is_err());
        let failed = load_set(&[Request {
            role: "invalid-size",
            points: -1.0,
            weight: Weight::Regular,
        }])
        .unwrap();
        assert!(failed[0].font.is_none());
        assert_eq!(failed[0].receipt["source"], "egui-embedded-fallback");
        assert_eq!(
            failed[0].receipt["fallback_reason"],
            "invalid native font size"
        );
        let too_many = [Request {
            role: "bounded-set",
            points: 15.0,
            weight: Weight::Medium,
        }; MAX_REQUESTS + 1];
        assert_eq!(
            load_set(&too_many).err().unwrap().to_string(),
            "native font role limit exceeded"
        );
    }

    #[test]
    fn face_selection_requires_native_index_or_unique_exact_table_identity() {
        let fonts = egui::FontDefinitions::default();
        let bytes = fonts.font_data.values().next().unwrap().font.as_ref();
        let face = FontRef::from_index(bytes, 0).unwrap();
        let mut tables: Vec<_> = IDENTITY_TABLES
            .iter()
            .map(|tag| {
                (
                    *tag,
                    hash(face.table_data(Tag::new(tag)).unwrap().as_bytes()),
                )
            })
            .collect();
        assert_eq!(select_face(bytes, Some(0), &[]).unwrap(), 0);
        assert!(select_face(bytes, Some(1), &[]).is_err());
        assert!(select_face(bytes, None, &[]).is_err());
        assert_eq!(select_face(bytes, None, &tables).unwrap(), 0);
        let duplicate = duplicate_collection(bytes);
        assert!(
            select_face(&duplicate, None, &tables)
                .unwrap_err()
                .to_string()
                .contains("ambiguous")
        );
        assert_eq!(select_face(&duplicate, Some(1), &[]).unwrap(), 1);
        tables[0].1 = "wrong".into();
        assert!(select_face(bytes, None, &tables).is_err());
    }

    fn duplicate_collection(font: &[u8]) -> Vec<u8> {
        let padded = font.len().next_multiple_of(4);
        let mut bytes = vec![0; 20 + 2 * padded];
        bytes[..4].copy_from_slice(b"ttcf");
        bytes[4..8].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
        bytes[8..12].copy_from_slice(&2_u32.to_be_bytes());
        for index in 0..2 {
            let start = 20 + index * padded;
            bytes[12 + index * 4..16 + index * 4].copy_from_slice(&(start as u32).to_be_bytes());
            bytes[start..start + font.len()].copy_from_slice(font);
            let count = u16::from_be_bytes(font[4..6].try_into().unwrap()) as usize;
            for table in 0..count {
                let field = start + 12 + table * 16 + 8;
                let offset = u32::from_be_bytes(bytes[field..field + 4].try_into().unwrap());
                bytes[field..field + 4].copy_from_slice(&(offset + start as u32).to_be_bytes());
            }
        }
        bytes
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    #[ignore = "explicit developer native OS font probe; no window or font export"]
    fn explicit_native_font_probe() {
        let requests = [
            Request {
                role: "body-prototype",
                points: 14.0,
                weight: Weight::Regular,
            },
            Request {
                role: "assistant-prototype",
                points: 15.0,
                weight: Weight::Medium,
            },
            Request {
                role: "title-prototype",
                points: 32.0,
                weight: Weight::Semibold,
            },
        ];
        let fonts = load_set(&requests).unwrap();
        for font in &fonts {
            eprintln!("{}", font.receipt);
        }
        assert!(
            fonts.iter().all(|font| font.font.is_some()),
            "native font probe fell back; retained receipts explain why"
        );
        let mut definitions = egui::FontDefinitions::default();
        const SAMPLE: &str = "Cueglint 0123456789";
        for (request, attempt) in requests.iter().zip(fonts) {
            let name = request.role.to_owned();
            let font = attempt.font.unwrap();
            let face = FontRef::from_index(&font.font, font.index).unwrap();
            assert!(
                SAMPLE.chars().all(|ch| face.charmap().map(ch).is_some()),
                "native face lacks the original probe glyphs"
            );
            definitions.font_data.insert(name.clone(), font.into());
            definitions
                .families
                .insert(egui::FontFamily::Name(name.clone().into()), vec![name]);
        }
        let context = egui::Context::default();
        context.set_fonts(definitions);
        let _ = context.run_ui(egui::RawInput::default(), |ui| {
            for request in requests {
                let galley = ui.fonts_mut(|fonts| fonts.layout_no_wrap(
                    SAMPLE.into(),
                    egui::FontId::new(request.points, egui::FontFamily::Name(request.role.into())),
                    egui::Color32::WHITE,
                ));
                assert!(galley.size().is_finite() && galley.size().x > 0.0 && galley.size().y > 0.0);
                eprintln!("{}", json!({"event":"native_font_layout_probe", "role":request.role,
                    "logical_size":[galley.size().x,galley.size().y], "reference_match":"unverified"}));
            }
        });
    }
}
