//! Resumable downloads with progress and SHA-256 verification (blocking; run on a thread).

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

pub struct Progress {
    pub done: u64,
    pub total: u64,
    pub bytes_per_sec: f64,
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("picalum-qwenimage/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .timeout(None)
        .build()
        .expect("http client")
}

/// Downloads `url` to `dest`, resuming from `dest.part` if it exists.
/// `expected_size` (when known) is used for resume and for the final check.
pub fn fetch(
    url: &str,
    dest: &Path,
    expected_size: Option<u64>,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<(), String> {
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let part = dest.with_extension(format!(
        "{}.part",
        dest.extension().and_then(|e| e.to_str()).unwrap_or("bin")
    ));
    let mut have = part.metadata().map(|m| m.len()).unwrap_or(0);
    if expected_size.is_some_and(|s| have > s) {
        have = 0; // corrupt partial: start over
    }

    let mut attempt = 0;
    loop {
        attempt += 1;
        let mut req = client().get(url);
        if have > 0 {
            req = req.header("Range", format!("bytes={have}-"));
        }
        let resp = match req.send() {
            Ok(r) => r,
            Err(e) if attempt < 5 => {
                std::thread::sleep(Duration::from_secs(2 * attempt));
                let _ = e;
                continue;
            }
            Err(e) => return Err(format!("download failed: {e}")),
        };
        let status = resp.status();
        if status.as_u16() == 416 && expected_size == Some(have) {
            break; // already complete
        }
        if !status.is_success() {
            return Err(format!("download failed: HTTP {status} for {url}"));
        }
        // Server ignored the Range header: restart from zero.
        let resumed = status.as_u16() == 206;
        if !resumed {
            have = 0;
        }
        let total = expected_size
            .or_else(|| resp.content_length().map(|l| l + have))
            .unwrap_or(0);
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(&part)
            .map_err(|e| e.to_string())?;

        let mut reader = resp;
        let mut buf = vec![0u8; 1 << 20];
        let started = Instant::now();
        let start_have = have;
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let result: Result<(), String> = loop {
            match reader.read(&mut buf) {
                Ok(0) => break Ok(()),
                Ok(n) => {
                    if let Err(e) = file.write_all(&buf[..n]) {
                        break Err(format!("write failed: {e} (disk full?)"));
                    }
                    have += n as u64;
                    if last_emit.elapsed() >= Duration::from_millis(250) {
                        last_emit = Instant::now();
                        let secs = started.elapsed().as_secs_f64().max(0.001);
                        on_progress(Progress {
                            done: have,
                            total,
                            bytes_per_sec: (have - start_have) as f64 / secs,
                        });
                    }
                }
                Err(e) => break Err(e.to_string()),
            }
        };
        file.flush().map_err(|e| e.to_string())?;
        drop(file);
        match result {
            Ok(()) => break,
            Err(_) if attempt < 5 => {
                std::thread::sleep(Duration::from_secs(2 * attempt));
                continue; // resume from where the connection dropped
            }
            Err(e) => return Err(e),
        }
    }

    let got = part.metadata().map(|m| m.len()).unwrap_or(0);
    if let Some(s) = expected_size {
        if got != s {
            return Err(format!("incomplete download: {got} of {s} bytes"));
        }
    }
    std::fs::rename(&part, dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// SHA-256 of a file, reporting progress.
pub fn sha256(path: &Path, on_progress: &mut dyn FnMut(Progress)) -> Result<String, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let total = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 4 << 20];
    let mut done = 0u64;
    let started = Instant::now();
    let mut last = Instant::now();
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        done += n as u64;
        if last.elapsed() >= Duration::from_millis(250) {
            last = Instant::now();
            on_progress(Progress {
                done,
                total,
                bytes_per_sec: done as f64 / started.elapsed().as_secs_f64().max(0.001),
            });
        }
    }
    Ok(hex::encode(h.finalize()))
}

/// Extracts a zip. `strip_top` drops the archive's single top-level folder (GitHub zips have one).
pub fn unzip(archive: &Path, dest: &Path, strip_top: bool) -> Result<(), String> {
    let f = File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let rel = if strip_top {
            rel.components().skip(1).collect::<std::path::PathBuf>()
        } else {
            rel
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut w = File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut w).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn human_bytes(b: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let b = b as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else {
        format!("{:.0} MB", b / MB)
    }
}
