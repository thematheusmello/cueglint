use super::{MAX_AXES, NativeFace, Request};
use anyhow::{Context, Result, ensure};
use serde_json::json;
use std::{os::windows::ffi::OsStringExt, path::PathBuf};
use windows::{
    Win32::{
        Graphics::DirectWrite::{
            DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_AXIS_VALUE, DWRITE_FONT_SIMULATIONS_NONE,
            DWRITE_INFORMATIONAL_STRING_POSTSCRIPT_NAME, DWriteCreateFactory, IDWriteFactory,
            IDWriteFontFace5, IDWriteFontFile, IDWriteLocalFontFileLoader, IDWriteLocalizedStrings,
        },
        UI::WindowsAndMessaging::{
            NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
            SystemParametersInfoW,
        },
    },
    core::Interface,
};

pub(super) fn query(request: Request) -> Result<NativeFace> {
    // DirectWrite-owned interfaces/reference keys remain alive on this thread until copied.
    unsafe {
        let mut metrics = NONCLIENTMETRICSW {
            cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32,
            ..Default::default()
        };
        SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            metrics.cbSize,
            Some((&mut metrics as *mut NONCLIENTMETRICSW).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .context("OS UI font unavailable")?;
        let mut logfont = metrics.lfMessageFont;
        logfont.lfWeight = request.weight.css() as i32;
        logfont.lfItalic = 0;
        let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)
            .context("DirectWrite font factory unavailable")?;
        let font = factory
            .GetGdiInterop()?
            .CreateFontFromLOGFONT(&logfont)
            .context("OS UI font matching failed")?;
        ensure!(
            font.GetSimulations() == DWRITE_FONT_SIMULATIONS_NONE,
            "native font would require synthetic styling"
        );
        let family = localized(&font.GetFontFamily()?.GetFamilyNames()?)?;
        let mut postscript_strings = None;
        let mut exists = Default::default();
        font.GetInformationalStrings(
            DWRITE_INFORMATIONAL_STRING_POSTSCRIPT_NAME,
            &mut postscript_strings,
            &mut exists,
        )?;
        let postscript = if exists.as_bool() {
            postscript_strings
                .as_ref()
                .map(|strings| localized(strings))
                .transpose()?
        } else {
            None
        };
        let face = font.CreateFontFace()?;
        let mut count = 0;
        face.GetFiles(&mut count, None)?;
        ensure!(count == 1, "native font must use one local file");
        let mut files: [Option<IDWriteFontFile>; 1] = [None];
        face.GetFiles(&mut count, Some(files.as_mut_ptr()))?;
        ensure!(count == 1, "native font file identity changed");
        let file = files[0].take().context("native font file unavailable")?;
        let loader: IDWriteLocalFontFileLoader = file
            .GetLoader()?
            .cast()
            .context("native font is not backed by a local file")?;
        let mut key = std::ptr::null_mut();
        let mut key_size = 0;
        file.GetReferenceKey(&mut key, &mut key_size)?;
        ensure!(
            !key.is_null() && (1..=4096).contains(&key_size),
            "native font file key limit exceeded"
        );
        let length = loader.GetFilePathLengthFromKey(key, key_size)?;
        ensure!(
            length > 0 && length <= 32767,
            "native font path limit exceeded"
        );
        let mut path = vec![0; length as usize + 1];
        loader.GetFilePathFromKey(key, key_size, &mut path)?;
        ensure!(path.last() == Some(&0), "invalid native font path");
        path.pop();
        let path = PathBuf::from(std::ffi::OsString::from_wide(&path));
        // A file-backed system font must not turn a font read into a network-share read.
        ensure!(
            path.to_string_lossy().get(..2) != Some("\\\\"),
            "native font network paths are unsupported"
        );

        let mut coords = Vec::new();
        let variations_known = if let Ok(variable) = face.cast::<IDWriteFontFace5>() {
            let count = variable.GetFontAxisValueCount() as usize;
            ensure!(count <= MAX_AXES, "native font axis limit exceeded");
            let mut axes = vec![DWRITE_FONT_AXIS_VALUE::default(); count];
            if count > 0 {
                variable.GetFontAxisValues(&mut axes)?;
            }
            // DWrite axis tags use little-endian DWRITE_MAKE_OPENTYPE_TAG.
            coords.extend(
                axes.into_iter()
                    .map(|axis| (axis.axisTag.0.to_le_bytes(), axis.value)),
            );
            true
        } else {
            false
        };
        coords.sort_by_key(|(tag, _)| *tag);
        Ok(NativeFace {
            api: "windows-ui-message-font+directwrite",
            path,
            family,
            postscript,
            index: Some(face.GetIndex()),
            table_hashes: Vec::new(),
            coords,
            variations_known,
            weight: json!({"resolved_directwrite_weight": font.GetWeight().0,
                "optical_size": "native face coordinates; CSS optical behavior unverified"}),
        })
    }
}

unsafe fn localized(strings: &IDWriteLocalizedStrings) -> Result<String> {
    unsafe {
        ensure!(strings.GetCount() > 0, "native font name unavailable");
        let length = strings.GetStringLength(0)?;
        ensure!(
            length > 0 && length <= 4096,
            "native font name limit exceeded"
        );
        let mut value = vec![0; length as usize + 1];
        strings.GetString(0, &mut value)?;
        value.pop();
        String::from_utf16(&value).context("native font name is not valid UTF-16")
    }
}
