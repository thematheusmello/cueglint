//! Selected-screen acquisition and local OCR. Captures stay in memory unless explicitly exported.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// Explicit, per-invocation single-window capture; independent of display capture.
pub mod scoped;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Display {
    pub id: u32,
    pub width: u32,
    pub height: u32,
}

pub struct ScreenCapture {
    pub id: String,
    pub display_id: u32,
    pub captured_at: i64,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
    pub png: Vec<u8>,
    pub text: String,
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn select_display<T>(
    mut displays: impl Iterator<Item = (u32, T)>,
    requested: Option<u32>,
    primary: u32,
) -> Result<T> {
    use anyhow::Context;
    let id = requested.unwrap_or(primary);
    displays
        .find_map(|(candidate, display)| (candidate == id).then_some(display))
        .with_context(|| {
            format!("Display {id} is unavailable; reconnect it or select another display")
        })
}

pub fn displays() -> Result<Vec<Display>> {
    #[cfg(target_os = "macos")]
    {
        mac::displays()
    }
    #[cfg(not(target_os = "macos"))]
    {
        anyhow::bail!("screen capture is not implemented for this platform yet")
    }
}

/// List only after an existing grant, without explicitly requesting permission.
pub fn authorized_displays() -> Result<Vec<Display>> {
    #[cfg(target_os = "macos")]
    {
        mac::authorized_displays()
    }
    #[cfg(not(target_os = "macos"))]
    {
        anyhow::bail!("screen capture is not implemented for this platform yet")
    }
}

pub fn capture(display_id: Option<u32>) -> Result<ScreenCapture> {
    #[cfg(target_os = "macos")]
    {
        mac::capture(display_id)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = display_id;
        anyhow::bail!("screen capture is not implemented for this platform yet")
    }
}

pub fn recognize_png(png: &[u8]) -> Result<String> {
    ensure!(png.len() <= 32 * 1024 * 1024, "image exceeds 32 MiB");
    #[cfg(target_os = "macos")]
    {
        mac::recognize_png(png)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = png;
        anyhow::bail!("local OCR is not implemented for this platform yet")
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use anyhow::Context;
    use objc2::{AnyThread, rc::autoreleasepool};
    use objc2_core_graphics::CGMainDisplayID;
    use objc2_foundation::{NSArray, NSData, NSDictionary};
    use objc2_vision::{
        VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
    };
    use screencapturekit::{
        prelude::*,
        screenshot_manager::{CGImageExt, SCScreenshotManager},
    };
    use sha2::{Digest, Sha256};
    use std::io::Cursor;

    pub fn displays() -> Result<Vec<Display>> {
        crate::permissions::ensure_screen()?;
        authorized_displays()
    }

    pub fn authorized_displays() -> Result<Vec<Display>> {
        ensure!(
            crate::permissions::status().screen == crate::permissions::Status::Granted,
            "Screen Recording access is required to list displays; grant access in Settings and refresh inputs"
        );
        let content = SCShareableContent::get()
            .map_err(|e| anyhow::anyhow!("Screen Recording permission required: {e}"))?;
        // shortcut: cap the native catalog at 32 displays; raise for a verified larger setup.
        let displays: Vec<_> = content
            .displays()
            .iter()
            .take(33)
            .map(|display| Display {
                id: display.display_id(),
                width: display.width(),
                height: display.height(),
            })
            .collect();
        ensure!(displays.len() <= 32, "display catalog exceeds 32 displays");
        ensure!(
            displays
                .iter()
                .all(|display| display.id != 0 && display.width > 0 && display.height > 0),
            "display catalog contains invalid geometry or IDs"
        );
        Ok(displays)
    }

    pub fn capture(display_id: Option<u32>) -> Result<ScreenCapture> {
        crate::permissions::ensure_screen()?;
        let content = SCShareableContent::get()
            .map_err(|e| anyhow::anyhow!("Screen Recording permission required: {e}"))?;
        let display = select_display(
            content
                .displays()
                .into_iter()
                .map(|display| (display.display_id(), display)),
            display_id,
            CGMainDisplayID(),
        )?;
        let own_windows = content
            .windows()
            .into_iter()
            .filter(|w| {
                w.owning_application()
                    .is_some_and(|a| a.process_id() == std::process::id() as i32)
            })
            .collect::<Vec<_>>();
        let own_refs = own_windows.iter().collect::<Vec<_>>();
        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&own_refs)
            .build()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let width = display.width().min(2560);
        let height = ((display.height() as f64 * width as f64 / display.width().max(1) as f64)
            .round() as u32)
            .max(1);
        let config = SCStreamConfiguration::new()
            .with_width(width)
            .with_height(height);
        let image = SCScreenshotManager::capture_image(&filter, &config)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let captured_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as i64;
        let width = image.width() as u32;
        let height = image.height() as u32;
        let pixels = image
            .rgba_data()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let rgba = image::RgbaImage::from_raw(width, height, pixels)
            .context("invalid captured pixel buffer")?;
        let mut png = Cursor::new(Vec::new());
        rgba.write_to(&mut png, image::ImageFormat::Png)?;
        let png = png.into_inner();
        let text = recognize_png(&png)?;
        Ok(ScreenCapture {
            id: uuid::Uuid::new_v4().to_string(),
            display_id: display.display_id(),
            captured_at,
            width,
            height,
            sha256: format!("{:x}", Sha256::digest(&png)),
            png,
            text,
        })
    }

    pub fn recognize_png(png: &[u8]) -> Result<String> {
        autoreleasepool(|_| {
            let data = NSData::with_bytes(png);
            let handler = VNImageRequestHandler::initWithData_options(
                VNImageRequestHandler::alloc(),
                &data,
                &NSDictionary::new(),
            );
            let request = VNRecognizeTextRequest::new();
            request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
            request.setUsesLanguageCorrection(true);
            let requests = NSArray::<VNRequest>::from_slice(&[&request]);
            handler
                .performRequests_error(&requests)
                .map_err(|e| anyhow::anyhow!("local OCR failed: {e}"))?;
            let mut text = String::new();
            if let Some(results) = request.results() {
                for observation in &results {
                    if let Some(candidate) = observation.topCandidates(1).firstObject() {
                        let line = candidate.string().to_string();
                        if text.len() + line.len() + 1 > 32_768 {
                            break;
                        }
                        text.push_str(&line);
                        text.push('\n');
                    }
                }
            }
            Ok(text)
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_uses_the_primary_display_and_disconnection_never_selects_another_screen() {
        let displays = || [(3, "second monitor"), (1, "built-in"), (2, "primary")].into_iter();
        assert_eq!(
            super::select_display(displays(), None, 2).unwrap(),
            "primary"
        );
        assert_eq!(
            super::select_display(displays(), Some(3), 2).unwrap(),
            "second monitor"
        );
        assert!(super::select_display(displays(), Some(99), 2).is_err());
        assert!(super::select_display(displays(), None, 0).is_err());
        assert!(super::select_display(std::iter::empty::<(u32, ())>(), None, 2).is_err());
    }

    #[test]
    fn rejects_unbounded_image_before_native_decode() {
        assert!(super::recognize_png(&vec![0; 32 * 1024 * 1024 + 1]).is_err());
    }
}
