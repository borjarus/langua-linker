//! Model download (ADR-23): HTTPS from GitHub Releases into `<file>.part`, resumed with a
//! `Range` request, then checked against the catalog's size and SHA-256 and renamed into place.
//! A file that fails the check is deleted; nothing unverified is ever opened by the engine.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use langualinker_application::llm::local::MODEL_HOSTS;
use sha2::{Digest, Sha256};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Longest wait for the next bytes of the body; a stalled transfer fails and can be resumed.
const READ_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_REDIRECTS: usize = 5;

/// Why a download stopped; also the `error` code the settings show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadError {
    /// No connection, or the transfer broke off (the `.part` file is kept for a resume).
    Network,
    /// The server answered with an error status.
    Http,
    /// The URL or a redirect left the allowed hosts.
    Refused,
    /// The file is not the catalog's size.
    Size,
    /// The file is the right size but not the catalog's file.
    Checksum,
    /// Writing to the app's storage failed (full disk).
    Storage,
    Cancelled,
}

impl DownloadError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Http => "http",
            Self::Refused => "refused",
            Self::Size => "size",
            Self::Checksum => "checksum",
            Self::Storage => "storage",
            Self::Cancelled => "cancelled",
        }
    }
}

/// What one download needs; `done` and `cancel` are shared with the store.
pub struct Request<'a> {
    pub url: &'a str,
    pub part: &'a Path,
    pub target: &'a Path,
    pub bytes: u64,
    pub sha256: &'a str,
    pub done: &'a AtomicU64,
    pub cancel: &'a AtomicBool,
    /// Called after each chunk; the store throttles what it reports.
    pub progress: &'a (dyn Fn() + Send + Sync),
    /// Tests serve the file from 127.0.0.1 over plain HTTP.
    pub allow_loopback_http: bool,
}

fn host_allowed(url: &reqwest::Url, loopback: bool) -> bool {
    match (url.scheme(), url.host_str()) {
        ("https", Some(host)) => MODEL_HOSTS.contains(&host),
        ("http", Some("127.0.0.1")) => loopback,
        _ => false,
    }
}

fn client(loopback: bool) -> Result<reqwest::Client, DownloadError> {
    let policy = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS || !host_allowed(attempt.url(), loopback) {
            attempt.stop()
        } else {
            attempt.follow()
        }
    });
    reqwest::Client::builder()
        .tls_backend_preconfigured((*crate::llm::tls_config()).clone())
        .https_only(!loopback)
        .redirect(policy)
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .build()
        .map_err(|_| DownloadError::Network)
}

/// Downloads (or resumes) and verifies; on success the model is at `target`.
pub fn fetch(request: &Request<'_>) -> Result<(), DownloadError> {
    let url = reqwest::Url::parse(request.url).map_err(|_| DownloadError::Refused)?;
    if !host_allowed(&url, request.allow_loopback_http) {
        return Err(DownloadError::Refused);
    }
    if let Some(parent) = request.part.parent() {
        fs::create_dir_all(parent).map_err(|_| DownloadError::Storage)?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| DownloadError::Network)?;
    runtime.block_on(transfer(request, url))?;
    verify(request)?;
    fs::rename(request.part, request.target).map_err(|_| DownloadError::Storage)
}

async fn transfer(request: &Request<'_>, url: reqwest::Url) -> Result<(), DownloadError> {
    let have = fs::metadata(request.part).map_or(0, |meta| meta.len());
    if have == request.bytes {
        request.done.store(have, Ordering::Relaxed);
        return Ok(());
    }
    if have > request.bytes {
        fs::remove_file(request.part).map_err(|_| DownloadError::Storage)?;
        return Box::pin(transfer(request, url)).await;
    }
    let client = client(request.allow_loopback_http)?;
    let mut builder = client.get(url);
    if have > 0 {
        builder = builder.header("range", format!("bytes={have}-"));
    }
    let mut response = builder.send().await.map_err(|error| {
        if error.is_redirect() {
            DownloadError::Refused
        } else {
            DownloadError::Network
        }
    })?;
    let status = response.status().as_u16();
    // 206: the server continues where the part ends; 200: it sends the whole file again.
    let append = match status {
        206 if have > 0 => true,
        200 => false,
        300..=399 => return Err(DownloadError::Refused),
        _ => return Err(DownloadError::Http),
    };
    let mut file = if append {
        OpenOptions::new().append(true).open(request.part)
    } else {
        File::create(request.part)
    }
    .map_err(|_| DownloadError::Storage)?;
    let mut done = if append { have } else { 0 };
    request.done.store(done, Ordering::Relaxed);
    loop {
        if request.cancel.load(Ordering::Relaxed) {
            return Err(DownloadError::Cancelled);
        }
        let Some(chunk) = response.chunk().await.map_err(|_| DownloadError::Network)? else {
            break;
        };
        done += chunk.len() as u64;
        if done > request.bytes {
            drop(file);
            let _ = fs::remove_file(request.part);
            return Err(DownloadError::Size);
        }
        file.write_all(&chunk).map_err(|_| DownloadError::Storage)?;
        request.done.store(done, Ordering::Relaxed);
        (request.progress)();
    }
    file.sync_all().map_err(|_| DownloadError::Storage)?;
    if done != request.bytes {
        // Cut short: keep the part for a resume.
        return Err(DownloadError::Network);
    }
    Ok(())
}

/// Size and SHA-256 of the part; a mismatch deletes it (a resume would only repeat it).
fn verify(request: &Request<'_>) -> Result<(), DownloadError> {
    let reject = |error| {
        let _ = fs::remove_file(request.part);
        Err(error)
    };
    let mut file = File::open(request.part).map_err(|_| DownloadError::Storage)?;
    if file.metadata().map_err(|_| DownloadError::Storage)?.len() != request.bytes {
        return reject(DownloadError::Size);
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        if request.cancel.load(Ordering::Relaxed) {
            return Err(DownloadError::Cancelled);
        }
        let read = file.read(&mut buffer).map_err(|_| DownloadError::Storage)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if digest != request.sha256 {
        return reject(DownloadError::Checksum);
    }
    Ok(())
}

/// Shared counters of a running download.
#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub cancel: AtomicBool,
}

impl Progress {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}
