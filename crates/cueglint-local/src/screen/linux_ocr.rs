//! English screen OCR through the optional Tesseract 5 C ABI.
use anyhow::{Context, Result, ensure};
use image::RgbImage;
use libloading::Library;
use std::{
    ffi::{c_char, c_int, c_void},
    ptr::{self, NonNull},
    sync::OnceLock,
};

static TESSERACT: OnceLock<Library> = OnceLock::new();
const MAX_TEXT_BYTES: usize = 32_768;
const RECOGNITION_DEADLINE_MS: c_int = 5_000;
// TessPageSegMode::PSM_AUTO reads the page layout without orientation detection.
const PSM_AUTO: c_int = 3;

type Create = unsafe extern "C" fn() -> *mut c_void;
type Delete = unsafe extern "C" fn(*mut c_void);
type Init = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int;
type SetImage = unsafe extern "C" fn(*mut c_void, *const u8, c_int, c_int, c_int, c_int);
type SetInt = unsafe extern "C" fn(*mut c_void, c_int);
type Recognize = unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int;
type GetText = unsafe extern "C" fn(*mut c_void) -> *mut c_char;
type DeleteText = unsafe extern "C" fn(*const c_char);

struct Owned<T, F: Fn(*mut T)> {
    ptr: NonNull<T>,
    delete: F,
}

impl<T, F: Fn(*mut T)> Owned<T, F> {
    fn new(ptr: *mut T, delete: F) -> Option<Self> {
        Some(Self {
            ptr: NonNull::new(ptr)?,
            delete,
        })
    }
}

impl<T, F: Fn(*mut T)> Drop for Owned<T, F> {
    fn drop(&mut self) {
        (self.delete)(self.ptr.as_ptr());
    }
}

pub(super) fn recognize(image: &RgbImage) -> Result<String> {
    let (width, height) = image.dimensions();
    ensure!(
        super::valid_frame_dimensions(width as usize, height as usize),
        "Screen OCR image dimensions exceed bounds"
    );

    // Native OpenMP workers can outlive a recognition call. Keep their library
    // mapped until process exit while releasing the page, monitor and engine per request.
    unsafe {
        let lib = match TESSERACT.get() {
            Some(loaded) => loaded,
            None => {
                let loaded = Library::new("libtesseract.so.5")
                    .context("Linux screen OCR requires the local Tesseract 5 library")?;
                TESSERACT.get_or_init(|| loaded)
            }
        };
        let create = lib.get::<Create>(b"TessBaseAPICreate\0")?;
        let delete = lib.get::<Delete>(b"TessBaseAPIDelete\0")?;
        let init = lib.get::<Init>(b"TessBaseAPIInit3\0")?;
        let set_image = lib.get::<SetImage>(b"TessBaseAPISetImage\0")?;
        let set_layout = lib.get::<SetInt>(b"TessBaseAPISetPageSegMode\0")?;
        let monitor_create = lib.get::<Create>(b"TessMonitorCreate\0")?;
        let monitor_delete = lib.get::<Delete>(b"TessMonitorDelete\0")?;
        let set_deadline = lib.get::<SetInt>(b"TessMonitorSetDeadlineMSecs\0")?;
        let run = lib.get::<Recognize>(b"TessBaseAPIRecognize\0")?;
        let get_text = lib.get::<GetText>(b"TessBaseAPIGetUTF8Text\0")?;
        let delete_text = lib.get::<DeleteText>(b"TessDeleteText\0")?;

        let engine = Owned::new(create(), |p| delete(p)).context("cannot allocate OCR engine")?;
        let api = engine.ptr.as_ptr();
        ensure!(
            init(api, ptr::null(), c"eng".as_ptr()) == 0,
            "Linux screen OCR requires local English Tesseract data (eng.traineddata)"
        );
        let monitor = Owned::new(monitor_create(), |p| monitor_delete(p))
            .context("cannot allocate OCR deadline monitor")?;
        set_layout(api, PSM_AUTO);
        // RgbImage stores three contiguous bytes per pixel; the preflight
        // bound keeps both dimensions and the row stride inside c_int.
        set_image(
            api,
            image.as_ptr(),
            width as c_int,
            height as c_int,
            3,
            (width * 3) as c_int,
        );
        set_deadline(monitor.ptr.as_ptr(), RECOGNITION_DEADLINE_MS);
        ensure!(
            run(api, monitor.ptr.as_ptr()) == 0,
            "Local screen OCR failed or exceeded its recognition deadline"
        );
        let text = Owned::new(get_text(api), |p| delete_text(p))
            .context("Tesseract returned no text buffer")?;
        bounded_utf8(text.ptr)
    }
}

/// The caller owns a valid NUL-terminated native string for the full read.
unsafe fn bounded_utf8(text: NonNull<c_char>) -> Result<String> {
    unsafe extern "C" {
        fn strnlen(text: *const c_char, max_len: usize) -> usize;
    }
    let len = unsafe { strnlen(text.as_ptr(), MAX_TEXT_BYTES + 1) };
    ensure!(len <= MAX_TEXT_BYTES, "Screen OCR text exceeds 32 KiB");
    let bytes = unsafe { std::slice::from_raw_parts(text.as_ptr().cast::<u8>(), len) };
    Ok(std::str::from_utf8(bytes)
        .context("Tesseract returned invalid UTF-8")?
        .to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn rejects_invalid_dimensions_before_loading_tesseract() {
        for (width, height) in [(0, 1), (2561, 1), (1, 2561)] {
            let error = recognize(&RgbImage::new(width, height)).unwrap_err();
            assert!(error.to_string().contains("dimensions"));
        }
    }

    #[test]
    fn native_text_preserves_utf8_and_enforces_the_byte_limit() {
        fn read(bytes: &[u8]) -> Result<String> {
            let text = CString::new(bytes).unwrap();
            unsafe { bounded_utf8(NonNull::new(text.as_ptr().cast_mut()).unwrap()) }
        }
        assert_eq!(read(b"").unwrap(), "");
        let limit = "é".repeat(MAX_TEXT_BYTES / 2);
        assert_eq!(read(limit.as_bytes()).unwrap(), limit);
        assert!(read(&[0xff]).is_err());
        assert!(read(&vec![b'x'; MAX_TEXT_BYTES + 1]).is_err());
    }
}
