//! Internal self-child PDF protocol. Install `Allocator` and call `dispatch` before app initialization.
use super::{Budget, read_bounded};
use crate::MAX_DOCUMENT_BYTES;
use anyhow::{Context, Result, bail, ensure};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    io::{Read, Write},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    thread::{self, JoinHandle},
    time::Duration,
};

#[path = "knowledge_pdf_text.rs"]
mod text;

const FLAG: &str = "--cueglint-pdf-worker-v1";
const ALLOCATION_BUDGET: usize = 256 * 1024 * 1024;
const BUDGET_EXIT: i32 = 73;
const DOCUMENT_EXIT: i32 = 74;
static BUDGET_ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

/// System allocation in normal app mode; cumulative Rust allocation work is capped only in the child.
pub struct Allocator;

fn charge(bytes: usize) {
    if BUDGET_ENABLED.load(Ordering::Relaxed)
        && ALLOCATED
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |total| {
                total
                    .checked_add(bytes)
                    .filter(|total| *total <= ALLOCATION_BUDGET)
            })
            .is_err()
    {
        exit(BUDGET_EXIT);
    }
}

// SAFETY: pointers, sizes and alignments are forwarded unchanged to System; charging never allocates.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        charge(layout.size());
        let pointer = unsafe { System.alloc(layout) };
        if pointer.is_null() && BUDGET_ENABLED.load(Ordering::Relaxed) {
            exit(BUDGET_EXIT);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        charge(layout.size());
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if pointer.is_null() && BUDGET_ENABLED.load(Ordering::Relaxed) {
            exit(BUDGET_EXIT);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        charge(size);
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        if pointer.is_null() && BUDGET_ENABLED.load(Ordering::Relaxed) {
            exit(BUDGET_EXIT);
        }
        pointer
    }
}

fn exit(code: i32) -> ! {
    // SAFETY: immediate process termination needs no pointer, allocation, logging or Rust cleanup.
    #[cfg(unix)]
    unsafe {
        libc::_exit(code)
    }
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Threading::ExitProcess(code as u32)
    }
    #[cfg(not(any(unix, windows)))]
    std::process::exit(code)
}

/// Handles only the exact internal argument before Clap, GUI, database, credential or model setup.
/// The child reads raw bounded stdin and writes text only after the whole document succeeds.
pub fn dispatch() {
    let mut args = std::env::args_os();
    let _ = args.next();
    if args.next().as_deref() != Some(std::ffi::OsStr::new(FLAG)) || args.next().is_some() {
        return;
    }
    BUDGET_ENABLED.store(true, Ordering::Relaxed);
    let result = (|| -> Result<()> {
        let cancel = AtomicBool::new(false);
        let budget = Budget::new(&cancel);
        let bytes = read_bounded(std::io::stdin().lock(), MAX_DOCUMENT_BYTES, &budget)?;
        let output = text::extract(&bytes)?;
        validate_text(&output)?;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(output.as_bytes())?;
        stdout.flush()?;
        Ok(())
    })();
    exit(if result.is_ok() { 0 } else { DOCUMENT_EXIT });
}

fn validate_text(text: &str) -> Result<()> {
    ensure!(
        text.len() <= MAX_DOCUMENT_BYTES && !text.trim().is_empty() && !text.contains('\0'),
        "PDF has no complete supported text or exceeds 2 MiB"
    );
    Ok(())
}

struct Running {
    child: Child,
    writer: Option<JoinHandle<std::io::Result<()>>>,
    reader: Option<JoinHandle<Result<Vec<u8>>>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

pub(super) fn extract(bytes: &[u8], budget: &Budget<'_>) -> Result<String> {
    budget.check()?;
    ensure!(bytes.starts_with(b"%PDF-"), "PDF must have a PDF header");
    let mut command = Command::new(std::env::current_exe().context("Locate native PDF worker")?);
    command
        .arg(FLAG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    let mut running = Running {
        child: command.spawn().context("Start native PDF worker")?,
        writer: None,
        reader: None,
    };
    let mut stdin = running
        .child
        .stdin
        .take()
        .context("Open PDF worker input")?;
    let mut stdout = running
        .child
        .stdout
        .take()
        .context("Open PDF worker output")?;
    let input = bytes.to_vec();
    running.writer = Some(
        thread::Builder::new()
            .name("cueglint-pdf-input".into())
            .spawn(move || stdin.write_all(&input))
            .context("Start PDF input pipe")?,
    );
    running.reader = Some(
        thread::Builder::new()
            .name("cueglint-pdf-output".into())
            .spawn(move || {
                let mut bytes = Vec::new();
                stdout
                    .by_ref()
                    .take(MAX_DOCUMENT_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)?;
                ensure!(
                    bytes.len() <= MAX_DOCUMENT_BYTES,
                    "PDF worker text exceeds 2 MiB"
                );
                Ok(bytes)
            })
            .context("Start PDF output pipe")?,
    );
    let mut output = None;
    let mut input_error = None;
    loop {
        budget.check()?;
        let status = running.child.try_wait().context("Wait for PDF worker")?;
        if let Some(status) = status {
            match status.code() {
                Some(0) => {}
                Some(BUDGET_EXIT) => bail!(
                    "PDF exceeded its 256 MiB Rust allocation work budget; use a smaller document"
                ),
                Some(DOCUMENT_EXIT) => bail!(
                    "PDF is malformed, encrypted, too complex or has unsupported or incomplete text; use a text-based unencrypted PDF"
                ),
                _ => bail!("Native PDF worker failed; the document was not added"),
            }
        }
        if running.writer.as_ref().is_some_and(JoinHandle::is_finished) {
            input_error = running
                .writer
                .take()
                .unwrap()
                .join()
                .map_err(|_| anyhow::anyhow!("PDF input worker stopped"))?
                .err();
        }
        if running.reader.as_ref().is_some_and(JoinHandle::is_finished) {
            output = Some(
                running
                    .reader
                    .take()
                    .unwrap()
                    .join()
                    .map_err(|_| anyhow::anyhow!("PDF output worker stopped"))??,
            );
        }
        if status.is_some() && running.writer.is_none() && running.reader.is_none() {
            if let Some(error) = input_error {
                return Err(error).context("Write PDF worker input");
            }
            let text = String::from_utf8(output.context("PDF worker returned no result")?)
                .context("PDF worker returned invalid UTF-8")?;
            validate_text(&text)?;
            budget.check()?;
            return Ok(text);
        }
        thread::sleep(Duration::from_millis(10));
    }
}
