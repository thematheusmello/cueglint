//! Selected-screen acquisition and local OCR. Captures stay in memory unless explicitly exported.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
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
        let content = SCShareableContent::get()
            .map_err(|e| anyhow::anyhow!("Screen Recording permission required: {e}"))?;
        Ok(content
            .displays()
            .iter()
            .map(|d| Display {
                id: d.display_id(),
                width: d.width(),
                height: d.height(),
            })
            .collect())
    }

    pub fn capture(display_id: Option<u32>) -> Result<ScreenCapture> {
        crate::permissions::ensure_screen()?;
        let content = SCShareableContent::get()
            .map_err(|e| anyhow::anyhow!("Screen Recording permission required: {e}"))?;
        let display = content
            .displays()
            .into_iter()
            .find(|d| display_id.is_none_or(|id| d.display_id() == id))
            .context("selected display unavailable")?;
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
    fn rejects_unbounded_image_before_native_decode() {
        assert!(super::recognize_png(&vec![0; 32 * 1024 * 1024 + 1]).is_err());
    }
}
