//! Existing platform-file fonts, with bounded loading and explicit adoption.
use eframe::egui::{self, FontDefinitions, ViewportId};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use skrifa::{FontRef, MetadataProvider, string::StringId};
use std::{
    fs::File,
    io::Read,
    sync::mpsc::{self, Receiver, TryRecvError},
};

pub const DETAIL_TITLE_FONT: &str = "cueglint-detail-title";
const MAX_FONT_BYTES: usize = 32 * 1024 * 1024;

// No Debug/Serialize: only the identity, never font payloads, enters receipts.
struct Loaded {
    definitions: FontDefinitions,
    identity: Value,
}

pub struct LegacyFonts {
    pending: Option<Receiver<Loaded>>,
    loaded: Loaded,
    queued_pass: u64,
    observed_pass: Option<u64>,
    ready: bool,
}

impl LegacyFonts {
    pub fn start(ctx: &egui::Context, native: bool) -> Self {
        let loaded = embedded(if native {
            "initial_fallback"
        } else {
            "headless_context"
        });
        ctx.set_fonts(loaded.definitions.clone());
        let mut state = Self {
            pending: None,
            loaded,
            queued_pass: ctx.cumulative_pass_nr_for(ViewportId::ROOT),
            observed_pass: None,
            ready: false,
        };
        if native {
            let (tx, rx) = mpsc::sync_channel(1);
            let repaint = ctx.clone();
            if std::thread::Builder::new()
                .name("cueglint-fonts".into())
                .spawn(move || {
                    let loaded = load_platform().unwrap_or_else(embedded);
                    let _ = tx.send(loaded);
                    repaint.request_repaint_of(ViewportId::ROOT);
                })
                .is_ok()
            {
                state.pending = Some(rx);
            } else {
                state.loaded = embedded("loader_unavailable");
            }
        }
        state
    }

    /// Called from application logic, never drawing; the worker cannot set context fonts.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let completed = self.pending.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(loaded) => Some(loaded),
            Err(TryRecvError::Disconnected) => Some(embedded("loader_disconnected")),
            Err(TryRecvError::Empty) => None,
        });
        if let Some(loaded) = completed {
            self.pending = None;
            ctx.set_fonts(loaded.definitions.clone());
            self.loaded = loaded;
            self.queued_pass = ctx.cumulative_pass_nr_for(ViewportId::ROOT);
            self.observed_pass = None;
            self.ready = false;
        }
        let pass = ctx.cumulative_pass_nr_for(ViewportId::ROOT);
        if self.pending.is_none() && !self.ready {
            if let Some(observed) = self.observed_pass {
                self.ready = pass > observed;
            } else if pass > self.queued_pass
                && ctx.fonts_mut(|fonts| fonts.definitions() == &self.loaded.definitions)
            {
                self.observed_pass = Some(pass);
            }
            ctx.request_repaint_of(ViewportId::ROOT);
        }
    }

    pub fn ready(&self) -> bool {
        self.ready
    }

    pub fn receipt(&self) -> Value {
        let state = if self.pending.is_some() {
            "loading"
        } else if self.ready {
            "ready"
        } else if self.observed_pass.is_some() {
            "waiting_frame"
        } else {
            "waiting_application"
        };
        json!({"state":state, "queued_root_pass":self.queued_pass,
            "active_observed_root_pass":self.observed_pass, "identity":self.loaded.identity})
    }
}

fn embedded(reason: &'static str) -> Loaded {
    let mut definitions = FontDefinitions::default();
    definitions.families.insert(
        egui::FontFamily::Name(DETAIL_TITLE_FONT.into()),
        definitions.families[&egui::FontFamily::Proportional].clone(),
    );
    describe(definitions, "embedded-fallback", None, Some(reason))
}

fn describe(
    definitions: FontDefinitions,
    source: &str,
    file_name: Option<&str>,
    reason: Option<&str>,
) -> Loaded {
    let faces: Vec<_> = definitions.font_data.iter().map(|(name, data)| {
        let postscript = FontRef::from_index(&data.font, data.index).ok().and_then(|face|
            face.localized_strings(StringId::POSTSCRIPT_NAME).english_or_first().map(|s| s.to_string()));
        json!({"name":name, "index":data.index, "sha256":hash(&data.font), "postscript":postscript,
            "scale":data.tweak.scale, "y_offset":data.tweak.y_offset,
            "y_offset_factor":data.tweak.y_offset_factor,
            "hinting":data.tweak.hinting, "hinting_target":format!("{:?}", data.tweak.hinting_target),
            "subpixel_binning":data.tweak.subpixel_binning,
            "thin_space_width":data.tweak.thin_space_width, "tab_size":data.tweak.tab_size,
            "coordinates":data.tweak.coords.as_ref().iter().map(|(tag,value)|
                json!({"tag":tag.to_string(),"value":value})).collect::<Vec<_>>()})
    }).collect();
    let families: Vec<_> = definitions
        .families
        .iter()
        .map(|(name, faces)| json!({"family":name.to_string(),"faces":faces}))
        .collect();
    let mut identity = json!({"source":source,"file_name":file_name,"fallback_reason":reason,
        "faces":faces,"families":families,"reference_font_match":"unverified",
        "os_resolver":"not_used_legacy_loader"});
    let id = hash(&serde_json::to_vec(&identity).expect("font identity is JSON"));
    identity["font_set_id"] = json!(id);
    Loaded {
        definitions,
        identity,
    }
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn bounded_read(reader: impl Read, limit: usize) -> Result<Vec<u8>, &'static str> {
    if limit == 0 || limit > MAX_FONT_BYTES {
        return Err("font_size_limit");
    }
    let mut data = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|_| "read_failed")?;
    if data.is_empty() || data.len() > limit {
        return Err("font_size_limit");
    }
    Ok(data)
}

fn load_platform() -> Result<Loaded, &'static str> {
    #[cfg(target_os = "macos")]
    let path = "/System/Library/Fonts/SFNS.ttf";
    #[cfg(target_os = "windows")]
    let path = "C:/Windows/Fonts/segoeui.ttf";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let path = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";
    let file = File::open(path).map_err(|_| "file_unavailable")?;
    if !file
        .metadata()
        .map_err(|_| "file_metadata_unavailable")?
        .is_file()
    {
        return Err("not_a_file");
    }
    let data = bounded_read(file, MAX_FONT_BYTES)?;
    platform_bytes(
        data,
        path.rsplit(['/', '\\']).next().unwrap_or("platform-font"),
    )
}

fn platform_bytes(data: Vec<u8>, file_name: &str) -> Result<Loaded, &'static str> {
    FontRef::from_index(&data, 0).map_err(|_| "invalid_font")?;
    let font = egui::FontData::from_owned(data);
    let title = font
        .variation_axes()
        .into_iter()
        .find(|axis| axis.tag.to_string() == "wght")
        .map(|axis| {
            let mut title = font.clone();
            title
                .tweak
                .coords
                .push(b"wght", 700.0_f32.clamp(axis.range.min, axis.range.max));
            title
        });
    let mut definitions = FontDefinitions::default();
    definitions
        .font_data
        .insert("platform-sans".into(), font.into());
    definitions
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "platform-sans".into());
    let mut title_family = definitions.families[&egui::FontFamily::Proportional].clone();
    if let Some(title) = title {
        definitions
            .font_data
            .insert("platform-title".into(), title.into());
        title_family.insert(0, "platform-title".into());
    }
    definitions.families.insert(
        egui::FontFamily::Name(DETAIL_TITLE_FONT.into()),
        title_family,
    );
    Ok(describe(
        definitions,
        "legacy-platform-file",
        Some(file_name),
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn bounded_font_read_rejects_empty_invalid_and_oversized_data() {
        let mut reader = Cursor::new([1_u8; 100]);
        assert_eq!(bounded_read(&mut reader, 3).unwrap_err(), "font_size_limit");
        assert_eq!(reader.position(), 4);
        assert!(bounded_read(Cursor::new([]), 3).is_err());
        assert!(platform_bytes(vec![1, 2, 3], "invalid-synthetic.ttf").is_err());
    }

    #[test]
    fn headless_fallback_and_late_font_result_wait_for_actual_application_and_a_frame() {
        let ctx = egui::Context::default();
        let mut state = LegacyFonts::start(&ctx, false);
        assert!(
            state.pending.is_none(),
            "headless setup starts no filesystem job"
        );
        assert_eq!(state.loaded.identity["fallback_reason"], "headless_context");
        assert!(!state.ready());
        for _ in 0..3 {
            let _ = ctx.run_ui(Default::default(), |ui| state.poll(ui.ctx()));
        }
        assert!(state.ready());
        let previous = state.loaded.identity["font_set_id"].clone();
        let (tx, rx) = mpsc::sync_channel(1);
        state.pending = Some(rx);
        state.ready = false;
        state.observed_pass = None;
        let _ = ctx.run_ui(Default::default(), |ui| state.poll(ui.ctx()));
        assert!(!state.ready());
        let defaults = FontDefinitions::default();
        let data = defaults.font_data["Ubuntu-Light"].font.to_vec();
        let expected_hash = hash(&data);
        tx.send(platform_bytes(data, "original-embedded-probe.ttf").unwrap())
            .ok()
            .unwrap();
        let _ = ctx.run_ui(Default::default(), |ui| {
            state.poll(ui.ctx());
            assert!(!state.ready());
        });
        assert!(!state.ready(), "channel acknowledgement is not application");
        let _ = ctx.run_ui(Default::default(), |ui| {
            state.poll(ui.ctx());
            assert!(!state.ready());
        });
        assert!(!state.ready(), "the new-font UI pass must complete");
        let _ = ctx.run_ui(Default::default(), |ui| state.poll(ui.ctx()));
        assert!(state.ready());
        assert_ne!(state.loaded.identity["font_set_id"], previous);
        assert_eq!(
            state.loaded.identity["file_name"],
            "original-embedded-probe.ttf"
        );
        assert_eq!(state.receipt()["state"], "ready");
        let receipt = state.receipt();
        let face = receipt["identity"]["faces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|face| face["name"] == "platform-sans")
            .unwrap();
        assert_eq!(face["sha256"], expected_hash);
        assert_eq!(face["index"], 0);
        assert!(
            serde_json::to_vec(&receipt).unwrap().len() < 16 * 1024,
            "font provenance must remain scalar identity metadata, not a font payload"
        );
        let (tx, rx) = mpsc::sync_channel(1);
        state.pending = Some(rx);
        state.ready = false;
        state.observed_pass = None;
        drop(tx);
        for _ in 0..3 {
            let _ = ctx.run_ui(Default::default(), |ui| state.poll(ui.ctx()));
        }
        assert!(state.ready());
        assert_eq!(
            state.receipt()["identity"]["fallback_reason"],
            "loader_disconnected"
        );
    }
}
