//! Native supervision self-check; this test-only executable is never packaged.
use anyhow::{Context, Result, ensure};
use cueglint_core::{
    MAX_DOCUMENT_BYTES, Store,
    knowledge::{IMPORT_TIMEOUT, pdf_worker, read_document},
};
use std::{
    fs::{self, File},
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[global_allocator]
static ALLOCATOR: pdf_worker::Allocator = pdf_worker::Allocator;

const PREFIX: &[u8] = b"%PDF-1.7\n%cueglint-supervision ";

fn controlled_worker() -> Result<()> {
    if std::env::args_os().nth(1).as_deref()
        != Some(std::ffi::OsStr::new("--cueglint-pdf-worker-v1"))
    {
        return Ok(());
    }
    let mut input = std::io::stdin().lock();
    if !input.fill_buf()?.starts_with(PREFIX) {
        return Ok(());
    }
    let mut limited = input.by_ref().take(1024);
    let mut header = String::new();
    limited.read_line(&mut header)?;
    let mut metadata = String::new();
    limited.read_line(&mut metadata)?;
    ensure!(
        header == "%PDF-1.7\n" && metadata.ends_with('\n'),
        "Invalid controlled fixture metadata"
    );
    let (mode, ready) = metadata
        .trim_end_matches(['\r', '\n'])
        .strip_prefix("%cueglint-supervision ")
        .context("Missing fixture marker")?
        .split_once(' ')
        .context("Missing readiness path")?;
    let pending = Path::new(ready).with_extension("pending");
    fs::write(&pending, std::process::id().to_string())?;
    fs::rename(&pending, ready)?;
    drop(limited);
    if matches!(mode, "invalid-utf8" | "nul" | "empty") {
        let length = std::io::copy(
            &mut input.by_ref().take(MAX_DOCUMENT_BYTES as u64 + 1),
            &mut std::io::sink(),
        )?;
        ensure!(
            length <= MAX_DOCUMENT_BYTES as u64,
            "Controlled worker input exceeds its bound"
        );
    }
    let mut stdout = std::io::stdout().lock();
    match mode {
        "invalid-utf8" => {
            stdout.write_all(&[0xff])?;
            stdout.flush()?;
            std::process::exit(0);
        }
        "nul" => {
            stdout.write_all(b"partial\0text")?;
            stdout.flush()?;
            std::process::exit(0);
        }
        "empty" => std::process::exit(0),
        "failure" | "budget-status" => {
            stdout.write_all(b"partial text")?;
            stdout.flush()?;
            std::process::exit(if mode == "failure" { 74 } else { 73 });
        }
        "oversized" => {
            let chunk = [b'x'; 8192];
            for _ in 0..MAX_DOCUMENT_BYTES / chunk.len() {
                stdout.write_all(&chunk)?;
            }
            stdout.write_all(b"x")?;
            stdout.flush()?;
        }
        "stall" => {
            stdout.write_all(b"partial text")?;
            stdout.flush()?;
        }
        _ => anyhow::bail!("Unknown controlled fixture"),
    }
    // Bound even a deliberately broken parent's fixture process; production dispatch has no test mode.
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(20) {
        thread::sleep(Duration::from_millis(20));
    }
    std::process::exit(75);
}

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture(directory: &Path, mode: &str, label: &str) -> Result<(PathBuf, PathBuf)> {
    let ready = directory.join(format!("{label}.ready"));
    let path = directory.join(format!("{label}.pdf"));
    let mut bytes = format!(
        "%PDF-1.7\n%cueglint-supervision {mode} {}\n",
        ready.to_str().context("UTF-8 fixture path")?
    )
    .into_bytes();
    bytes.extend(vec![b'x'; 256 * 1024]);
    fs::write(&path, bytes)?;
    Ok((path, ready))
}

fn pid(path: &Path) -> Result<u32> {
    let mut text = String::new();
    File::open(path)?.take(32).read_to_string(&mut text)?;
    Ok(text.parse()?)
}

#[cfg(unix)]
fn alive(pid: u32) -> bool {
    // SAFETY: signal zero only checks this recorded child PID and never sends a signal.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(windows)]
fn alive(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_TIMEOUT},
        System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
            WaitForSingleObject,
        },
    };
    // SAFETY: a query/synchronize-only owned handle is checked without waiting and closed exactly once.
    unsafe {
        let handle = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        );
        if handle.is_null() {
            return false;
        }
        let running = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
        CloseHandle(handle);
        running
    }
}

fn snapshot(store: &Store) -> Result<serde_json::Value> {
    let mut value: serde_json::Value = serde_json::from_str(&store.backup_json()?)?;
    value
        .as_object_mut()
        .context("Backup object")?
        .remove("exported_at")
        .context("Backup timestamp")?;
    Ok(value)
}

fn main() -> Result<()> {
    controlled_worker()?;
    pdf_worker::dispatch();
    let directory =
        std::env::temp_dir().join(format!("cueglint-pdf-supervision-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory)?;
    let directory = Directory(directory);
    let original = directory.0.join("original.pdf");
    fs::write(
        &original,
        include_bytes!("fixtures/knowledge/original-workshop.pdf"),
    )?;
    let text = read_document(&original, &AtomicBool::new(false))?;
    assert_eq!(text, include_str!("fixtures/knowledge/expected-pdf.txt"));
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Keep existing data", None)?;
    store.update_notes(&session.id, "Preserve these user notes.")?;
    store.ingest_knowledge("Original", "original.txt", "Keep the library unchanged.")?;
    let before = snapshot(&store)?;
    for (mode, message) in [
        ("oversized", "exceeds 2 MiB"),
        ("invalid-utf8", "invalid UTF-8"),
        ("nul", "no complete supported text"),
        ("empty", "no complete supported text"),
        ("failure", "PDF is malformed"),
        ("budget-status", "allocation work budget"),
    ] {
        let (path, ready) = fixture(&directory.0, mode, mode)?;
        let result = read_document(&path, &AtomicBool::new(false))
            .and_then(|text| store.ingest_knowledge("Rejected", "fixture.pdf", &text));
        let error = format!("{:#}", result.unwrap_err());
        assert!(error.contains(message), "mode={mode}: {error}");
        assert!(!alive(pid(&ready)?), "mode={mode}: child survived return");
        assert_eq!(
            snapshot(&store)?,
            before,
            "mode={mode}: partial data was stored"
        );
    }
    let (path, ready) = fixture(&directory.0, "stall", "cancel")?;
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&cancel);
    let ready_path = ready.clone();
    let cancelled = thread::spawn(move || -> Result<Instant> {
        let started = Instant::now();
        while !ready_path.exists() {
            ensure!(
                started.elapsed() < Duration::from_secs(5),
                "Child readiness timed out"
            );
            thread::sleep(Duration::from_millis(5));
        }
        pid(&ready_path)?;
        let at = Instant::now();
        signal.store(true, Ordering::Release);
        Ok(at)
    });
    let result = read_document(&path, &cancel)
        .and_then(|text| store.ingest_knowledge("Cancelled", "cancel.pdf", &text));
    let cancelled_at = cancelled
        .join()
        .map_err(|_| anyhow::anyhow!("Readiness thread panicked"))??;
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    assert!(cancelled_at.elapsed() < Duration::from_secs(2));
    assert!(!alive(pid(&ready)?), "cancelled child survived return");
    assert_eq!(snapshot(&store)?, before);
    let (path, ready) = fixture(&directory.0, "stall", "deadline")?;
    let started = Instant::now();
    let result = read_document(&path, &AtomicBool::new(false))
        .and_then(|text| store.ingest_knowledge("Expired", "deadline.pdf", &text));
    assert!(result.unwrap_err().to_string().contains("timed out"));
    assert!(
        started.elapsed() >= IMPORT_TIMEOUT
            && started.elapsed() < IMPORT_TIMEOUT + Duration::from_secs(5)
    );
    assert!(!alive(pid(&ready)?), "expired child survived return");
    assert_eq!(snapshot(&store)?, before);
    println!(
        "PDF supervision self-check passed: actual parser dispatch, six IPC failures, cancellation, ten-second deadline, reaping and data preservation"
    );
    Ok(())
}
