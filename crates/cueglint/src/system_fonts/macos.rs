use super::{IDENTITY_TABLES, MAX_AXES, NativeFace, Request, Weight, hash};
use anyhow::{Context, Result, ensure};
use core_foundation::{
    base::{CFType, CFTypeRef, TCFType},
    data::{CFData, CFDataRef},
    dictionary::{CFDictionary, CFDictionaryGetValue, CFDictionaryRef},
    number::CFNumber,
    string::{CFString, CFStringRef},
    url::{CFURL, CFURLCopyScheme},
};
use objc2::rc::{Retained, autoreleasepool};
use objc2_app_kit::{NSFont, NSFontWeightMedium, NSFontWeightRegular, NSFontWeightSemibold};
use serde_json::json;
use std::ffi::c_void;

type CTFontRef = *const c_void;

// SDK27.2 CTFont.h declares NSFont/CTFont bridging. Copy results follow CF create ownership.
#[link(name = "CoreText", kind = "framework")]
unsafe extern "C" {
    static kCTFontURLAttribute: CFStringRef;
    static kCTFontWeightTrait: CFStringRef;
    fn CTFontCopyAttribute(font: CTFontRef, attribute: CFStringRef) -> CFTypeRef;
    fn CTFontCopyFamilyName(font: CTFontRef) -> CFStringRef;
    fn CTFontCopyPostScriptName(font: CTFontRef) -> CFStringRef;
    fn CTFontCopyVariation(font: CTFontRef) -> CFDictionaryRef;
    fn CTFontCopyTraits(font: CTFontRef) -> CFDictionaryRef;
    fn CTFontCopyTable(font: CTFontRef, tag: u32, options: u32) -> CFDataRef;
}

pub(super) fn query(request: Request) -> Result<NativeFace> {
    autoreleasepool(|_| {
        // All ObjC/CF objects stay on this thread; only owned Rust values escape.
        let weight = unsafe {
            match request.weight {
                Weight::Regular => NSFontWeightRegular,
                Weight::Medium => NSFontWeightMedium,
                Weight::Semibold => NSFontWeightSemibold,
            }
        };
        let font = NSFont::systemFontOfSize_weight(request.points as f64, weight);
        let ct = Retained::as_ptr(&font).cast::<c_void>();
        let url = copy_type(unsafe { CTFontCopyAttribute(ct, kCTFontURLAttribute) })?
            .downcast_into::<CFURL>()
            .context("native font URL unavailable")?;
        let scheme = copy_string(unsafe { CFURLCopyScheme(url.as_concrete_TypeRef()) })?;
        ensure!(scheme == "file", "native font URL is not local");
        let path = url.to_path().context("native font path unavailable")?;
        let family = copy_string(unsafe { CTFontCopyFamilyName(ct) })?;
        let postscript = copy_string(unsafe { CTFontCopyPostScriptName(ct) })?;

        let mut table_hashes = Vec::new();
        for tag in IDENTITY_TABLES {
            let data = unsafe { CTFontCopyTable(ct, u32::from_be_bytes(tag), 0) };
            ensure!(!data.is_null(), "native font identity table unavailable");
            let data = unsafe { CFData::wrap_under_create_rule(data) };
            ensure!(
                (1..=1024 * 1024).contains(&data.len()),
                "native font identity table limit exceeded"
            );
            table_hashes.push((tag, hash(data.bytes())));
        }

        let variation = unsafe { CTFontCopyVariation(ct) };
        let mut coords = Vec::new();
        if !variation.is_null() {
            let variation: CFDictionary =
                unsafe { CFDictionary::wrap_under_create_rule(variation) };
            ensure!(
                variation.len() <= MAX_AXES,
                "native font axis limit exceeded"
            );
            let (keys, values) = variation.get_keys_and_values();
            for (key, value) in keys.into_iter().zip(values) {
                let key = borrowed_number(key)?
                    .to_i64()
                    .context("invalid native font axis tag")?;
                let key = u32::try_from(key).context("invalid native font axis tag")?;
                let value = borrowed_number(value)?
                    .to_f64()
                    .context("invalid native font axis coordinate")?;
                ensure!(value.is_finite(), "invalid native font axis coordinate");
                coords.push((key.to_be_bytes(), value as f32));
            }
        }
        coords.sort_by_key(|(tag, _)| *tag);

        let traits = unsafe { CTFontCopyTraits(ct) };
        ensure!(!traits.is_null(), "native font traits unavailable");
        let traits: CFDictionary = unsafe { CFDictionary::wrap_under_create_rule(traits) };
        let resolved_weight = unsafe {
            CFDictionaryGetValue(traits.as_concrete_TypeRef(), kCTFontWeightTrait.cast())
        };
        let resolved_weight = (!resolved_weight.is_null())
            .then(|| {
                borrowed_number(resolved_weight)?
                    .to_f64()
                    .context("invalid native font weight")
            })
            .transpose()?;
        Ok(NativeFace {
            api: "appkit-system-font+coretext",
            path,
            family,
            postscript: Some(postscript),
            index: None,
            table_hashes,
            coords,
            variations_known: !variation.is_null(),
            weight: json!({"requested_appkit_weight": weight, "resolved_normalized_weight": resolved_weight}),
        })
    })
}

fn copy_type(raw: CFTypeRef) -> Result<CFType> {
    ensure!(!raw.is_null(), "native font attribute unavailable");
    Ok(unsafe { CFType::wrap_under_create_rule(raw) })
}

fn copy_string(raw: CFStringRef) -> Result<String> {
    ensure!(!raw.is_null(), "native font name unavailable");
    let value = unsafe { CFString::wrap_under_create_rule(raw) };
    ensure!(
        (1..=4096).contains(&value.char_len()),
        "native font name limit exceeded"
    );
    Ok(value.to_string())
}

fn borrowed_number(raw: CFTypeRef) -> Result<CFNumber> {
    ensure!(!raw.is_null(), "native font number unavailable");
    unsafe { CFType::wrap_under_get_rule(raw) }
        .downcast_into::<CFNumber>()
        .context("invalid native font number")
}
