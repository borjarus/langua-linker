//! Files the user picked with the native dialog (plan step 29, ADR-12). The GUI only ever sees an
//! opaque token; Rust keeps what the token stands for. Import files are read once, right after
//! the pick (Android `content://`, iOS security-scoped URLs stay readable only that long), and
//! held in memory until a newer pick replaces them.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use langualinker_application::AppError;
use langualinker_content::package::MAX_PACKAGE_BYTES;
use langualinker_domain::ValidationError;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};

/// Picks kept at once (older tokens become `NOT_FOUND`).
const KEPT: usize = 4;

#[derive(Debug, Clone)]
pub struct PickedFile {
    pub token: String,
    pub name: String,
    pub bytes: Arc<Vec<u8>>,
    /// A restore archive copied into the staging folder, or a picked backup folder (Phase 12);
    /// `bytes` is empty then.
    pub path: Option<PathBuf>,
}

#[derive(Default)]
pub struct PickedFiles(Mutex<VecDeque<PickedFile>>);

impl PickedFiles {
    /// Registers file contents under a new random token.
    pub fn register(&self, name: String, bytes: Vec<u8>) -> PickedFile {
        self.insert(name, bytes, None)
    }

    /// Registers a file or folder held at `path` (restore archives, backup folders).
    pub fn register_path(&self, name: String, path: PathBuf) -> PickedFile {
        self.insert(name, Vec::new(), Some(path))
    }

    fn insert(&self, name: String, bytes: Vec<u8>, path: Option<PathBuf>) -> PickedFile {
        let mut token = [0u8; 12];
        getrandom_token(&mut token);
        let file = PickedFile {
            token: token.iter().map(|byte| format!("{byte:02x}")).collect(),
            name,
            bytes: Arc::new(bytes),
            path,
        };
        let mut files = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        files.push_back(file.clone());
        while files.len() > KEPT {
            files.pop_front();
        }
        file
    }

    pub fn get(&self, token: &str) -> Option<PickedFile> {
        let files = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        files.iter().find(|file| file.token == token).cloned()
    }
}

fn getrandom_token(bytes: &mut [u8]) {
    use langualinker_application::ports::Rng;
    langualinker_infrastructure::system::OsRng.fill(bytes);
}

/// Display name of a picked file: the last path segment, percent-decoded (Android document ids
/// such as `primary:Download/deck.ndjson` keep only `deck.ndjson`).
pub fn display_name(path: &FilePath) -> String {
    let raw = match path {
        FilePath::Path(path) => path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        FilePath::Url(url) => {
            let last = url
                .path_segments()
                .and_then(|mut segments| segments.next_back())
                .unwrap_or_default();
            percent_decode(last)
        }
    };
    let name = raw
        .rsplit(['/', ':'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    if name.is_empty() {
        "file".to_owned()
    } else {
        name.chars().take(120).collect()
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(hex) = text.get(index + 1..index + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Shows the system picker and reads the chosen file. `Ok(None)` when the user closed it.
pub fn pick_import_file<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Option<(String, Vec<u8>)>, AppError> {
    if app.try_state::<tauri_plugin_dialog::Dialog<R>>().is_none() {
        return Err(AppError::PermissionDenied {
            reason: "filePicker",
        });
    }
    let mut dialog = app.dialog().file();
    // Mobile pickers filter by MIME type, which `.ndjson` has none of; filter on desktop only.
    if cfg!(not(any(target_os = "android", target_os = "ios"))) {
        dialog = dialog.add_filter(
            "LanguaLinker deck package",
            &["ndjson", "json", "apkg", "csv", "txt"],
        );
    }
    let Some(path) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    let name = display_name(&path);
    let mut options = OpenOptions::new();
    options.read(true);
    let mut bytes = Vec::new();
    let read = app.fs().open(path.clone(), options).and_then(|file| {
        file.take(MAX_PACKAGE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
    });
    release_access(app, path);
    read.map_err(|_| AppError::PermissionDenied { reason: "fileRead" })?;
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(ValidationError::new("file", "tooLarge").into());
    }
    Ok(Some((name, bytes)))
}

fn dialog_available<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    if app.try_state::<tauri_plugin_dialog::Dialog<R>>().is_none() {
        return Err(AppError::PermissionDenied {
            reason: "filePicker",
        });
    }
    Ok(())
}

/// Shows the picker for a backup archive and copies it into `staging` right away (the picked
/// URI may stop being readable). Returns the display name, the staged copy and its size.
pub fn pick_restore_file<R: Runtime>(
    app: &AppHandle<R>,
    staging: &Path,
) -> Result<Option<(String, PathBuf, u64)>, AppError> {
    use langualinker_infrastructure::backup::{BACKUP_EXTENSION, MAX_BACKUP_BYTES};
    dialog_available(app)?;
    let mut dialog = app.dialog().file();
    if cfg!(not(any(target_os = "android", target_os = "ios"))) {
        dialog = dialog.add_filter("LanguaLinker backup", &[BACKUP_EXTENSION]);
    }
    let Some(path) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    let name = display_name(&path);
    let mut token = [0u8; 12];
    getrandom_token(&mut token);
    let target = staging.join(format!(
        "picked-{}.{BACKUP_EXTENSION}",
        token
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ));
    let mut options = OpenOptions::new();
    options.read(true);
    let copied = app.fs().open(path.clone(), options).and_then(|file| {
        let mut out = std::fs::File::create(&target)?;
        let copied = std::io::copy(&mut file.take(MAX_BACKUP_BYTES + 1), &mut out)?;
        out.flush()?;
        Ok(copied)
    });
    release_access(app, path);
    let copied = copied.map_err(|error| {
        let _ = std::fs::remove_file(&target);
        match error.kind() {
            std::io::ErrorKind::StorageFull => AppError::StorageFull,
            _ => AppError::PermissionDenied { reason: "fileRead" },
        }
    })?;
    if copied > MAX_BACKUP_BYTES {
        let _ = std::fs::remove_file(&target);
        return Err(ValidationError::new("file", "tooLarge").into());
    }
    Ok(Some((name, target, copied)))
}

/// Shows the folder picker for external backups. Desktop only: mobile folder grants (Android
/// tree URIs, iOS bookmarks) are not plain paths, so mobile users save copies with *Share*.
pub fn pick_backup_folder<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Option<(String, PathBuf)>, AppError> {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        return Err(AppError::PermissionDenied {
            reason: "externalFolder",
        });
    }
    dialog_available(app)?;
    let Some(folder) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    match folder {
        FilePath::Path(path) => Ok(Some((display_name(&FilePath::Path(path.clone())), path))),
        FilePath::Url(_) => Err(AppError::PermissionDenied {
            reason: "externalFolder",
        }),
    }
}

/// Saves a copy of `source` where the user chooses (outside the app sandbox). `Ok(false)` when
/// the user closed the dialog.
pub fn save_copy<R: Runtime>(
    app: &AppHandle<R>,
    source: &Path,
    file_name: &str,
) -> Result<bool, AppError> {
    dialog_available(app)?;
    let Some(path) = app
        .dialog()
        .file()
        .set_file_name(file_name)
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    let written = std::fs::File::open(source).and_then(|mut input| {
        let mut out = app.fs().open(path.clone(), options)?;
        std::io::copy(&mut input, &mut out)?;
        out.flush()?;
        out.sync_all()
    });
    release_access(app, path);
    written.map_err(|error| match error.kind() {
        std::io::ErrorKind::StorageFull => AppError::StorageFull,
        std::io::ErrorKind::NotFound => AppError::NotFound { entity: "backup" },
        _ => AppError::PermissionDenied {
            reason: "fileWrite",
        },
    })?;
    Ok(true)
}

/// iOS keeps security-scoped access open until it is released explicitly.
#[cfg(target_os = "ios")]
fn release_access<R: Runtime>(app: &AppHandle<R>, path: FilePath) {
    if matches!(&path, FilePath::Url(url) if url.scheme() == "file") {
        let _ = app.fs().stop_accessing_security_scoped_resource(path);
    }
}

#[cfg(not(target_os = "ios"))]
fn release_access<R: Runtime>(_app: &AppHandle<R>, _path: FilePath) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_come_from_the_last_segment() {
        let url: FilePath = "content://com.android.providers.downloads.documents/document/primary%3ADownload%2Fmoje%20s%C5%82%C3%B3wka.ndjson".parse().unwrap();
        assert_eq!(display_name(&url), "moje słówka.ndjson");
        let path = FilePath::from(std::path::PathBuf::from("/tmp/deck.ndjson"));
        assert_eq!(display_name(&path), "deck.ndjson");
    }

    #[test]
    fn tokens_are_opaque_and_old_picks_expire() {
        let files = PickedFiles::default();
        let first = files.register("a.ndjson".into(), b"a".to_vec());
        assert_eq!(first.token.len(), 24);
        for n in 0..KEPT {
            files.register(format!("{n}.ndjson"), Vec::new());
        }
        assert!(files.get(&first.token).is_none());
    }
}
