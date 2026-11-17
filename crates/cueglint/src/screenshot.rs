//! Native screenshot encoding/export runs on the existing bounded screenshot worker.
use anyhow::{Context, Result, ensure};
use eframe::egui;
use image::ImageEncoder;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

pub struct Request {
    id: uuid::Uuid,
    token: egui::UserData,
}

impl Request {
    pub fn new(fixture: Option<String>) -> Self {
        let id = uuid::Uuid::new_v4();
        let token = egui::UserData::new(eframe::egui_wgpu::capture::CaptureTrace::new(
            id.as_u128(),
            move |stage, viewport| {
                eprintln!(
                    "{}",
                    json!({"event":"native_screenshot_transport",
                    "request_id":id.to_string(), "fixture":fixture,
                    "viewport":if viewport == egui::ViewportId::ROOT { "root" } else { "other" },
                    "stage":stage.label()})
                );
            },
        ));
        Self { id, token }
    }

    pub fn id(&self) -> uuid::Uuid {
        self.id
    }
    pub fn token(&self) -> egui::UserData {
        self.token.clone()
    }

    pub fn matches(&self, viewport: egui::ViewportId, token: &egui::UserData) -> bool {
        viewport == egui::ViewportId::ROOT
            && token
                .data
                .as_ref()
                .and_then(|data| data.downcast_ref::<eframe::egui_wgpu::capture::CaptureTrace>())
                .is_some_and(|trace| trace.request_id() == self.id.as_u128())
    }
}

pub struct Metadata {
    pub request_id: String,
    pub fixture: Option<String>,
    pub pixels_per_point: f32,
    pub zoom_factor: f32,
    pub adapter: Option<Value>,
}

pub struct Job {
    pub path: PathBuf,
    pub image: Arc<egui::ColorImage>,
    pub metadata: Metadata,
}

pub struct Receipt {
    path: PathBuf,
    width: u32,
    height: u32,
    metadata: Metadata,
}

impl Receipt {
    pub fn json(&self) -> Value {
        json!({
            "event": "native_screenshot_saved",
            "request_id": self.metadata.request_id,
            "fixture": self.metadata.fixture,
            "path": self.path.to_string_lossy(),
            "image": {"width": self.width, "height": self.height},
            "pixels_per_point": self.metadata.pixels_per_point,
            "zoom_factor": self.metadata.zoom_factor,
            "adapter": self.metadata.adapter,
        })
    }
}

pub fn save(job: Job) -> Result<Receipt> {
    let [width, height] = job.image.size;
    ensure!(
        width > 0 && height > 0 && width.checked_mul(height) == Some(job.image.pixels.len()),
        "invalid native screenshot dimensions"
    );
    let width = u32::try_from(width).context("native screenshot width exceeds PNG limits")?;
    let height = u32::try_from(height).context("native screenshot height exceeds PNG limits")?;
    let rgba: Vec<u8> = job
        .image
        .pixels
        .iter()
        .flat_map(egui::Color32::to_srgba_unmultiplied)
        .collect();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&rgba, width, height, image::ExtendedColorType::Rgba8)
        .context("encode native screenshot as PNG")?;
    cueglint_core::files::write_private_export_bytes(&job.path, &png, false)
        .context("save native screenshot privately without overwriting")?;
    Ok(Receipt {
        path: job.path,
        width,
        height,
        metadata: job.metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_receipt_reports_actual_pixels_and_private_atomic_export_refuses_existing_targets() {
        let dir =
            std::env::temp_dir().join(format!("cueglint-native-png-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("original.png");
        let pixel = egui::Color32::from_rgba_unmultiplied(255, 0, 0, 128);
        let job = || Job {
            path: path.clone(),
            image: Arc::new(egui::ColorImage::filled([3, 2], pixel)),
            metadata: Metadata {
                request_id: "00000000-0000-0000-0000-000000000123".into(),
                fixture: Some("markdown".into()),
                pixels_per_point: 2.0,
                zoom_factor: 1.25,
                adapter: None,
            },
        };
        let receipt = save(job()).unwrap().json();
        assert_eq!(receipt["event"], "native_screenshot_saved");
        assert_eq!(
            receipt["request_id"],
            "00000000-0000-0000-0000-000000000123"
        );
        assert_eq!(receipt["fixture"], "markdown");
        assert_eq!(receipt["image"], json!({"width":3,"height":2}));
        assert_eq!(receipt["pixels_per_point"], 2.0);
        assert_eq!(receipt["zoom_factor"], 1.25);
        assert!(receipt["adapter"].is_null());
        let original = std::fs::read(&path).unwrap();
        let decoded = image::load_from_memory(&original).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (3, 2));
        assert_eq!(decoded.get_pixel(0, 0).0, pixel.to_srgba_unmultiplied());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(save(job()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            1,
            "no temporary/partial files may remain"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
