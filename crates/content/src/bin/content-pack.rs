//! Packs and unpacks the encrypted bundled content (`content/decks/decks.7z`).
//!
//! ```text
//! cargo run -p langualinker-content --bin content-pack -- unpack                 # -> content/decks-src
//! cargo run -p langualinker-content --bin content-pack -- pack                   # <- content/decks-src
//! cargo run -p langualinker-content --bin content-pack -- unpack <archive.7z> <dir>
//! cargo run -p langualinker-content --bin content-pack -- pack <dir> <archive.7z>
//! ```
//!
//! The password comes from `LANGUA_DECKS_PASSWORD` or the repository `.env`. `content/decks-src`
//! is git-ignored: edit there, run `content-lint content/decks-src --update-manifests`, then
//! `pack`. `pack` leaves the archive untouched when its content is unchanged (every pack draws a
//! fresh IV and salt, so the bytes would differ).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use langualinker_content::archive::{
    ARCHIVE_FILE, PASSWORD_VAR, find_password, pack, read_dir_files, unpack, unpack_to_dir,
};

const USAGE: &str = "usage: content-pack unpack [<archive.7z> <dir>] | pack [<dir> <archive.7z>]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let default_archive = PathBuf::from("content/decks").join(ARCHIVE_FILE);
    let default_dir = PathBuf::from("content/decks-src");
    let Some(password) = find_password(&std::env::current_dir().unwrap_or_default()) else {
        eprintln!("error: no password — set {PASSWORD_VAR} or add it to .env");
        return ExitCode::FAILURE;
    };
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["unpack"] => run_unpack(&default_archive, &default_dir, &password),
        ["unpack", archive, dir] => run_unpack(Path::new(archive), Path::new(dir), &password),
        ["pack"] => run_pack(&default_dir, &default_archive, &password),
        ["pack", dir, archive] => run_pack(Path::new(dir), Path::new(archive), &password),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_unpack(archive: &Path, dir: &Path, password: &str) -> Result<String, String> {
    unpack_to_dir(archive, dir, password).map_err(|e| format!("{}: {e}", archive.display()))?;
    Ok(format!(
        "unpacked {} -> {}",
        archive.display(),
        dir.display()
    ))
}

fn run_pack(dir: &Path, archive: &Path, password: &str) -> Result<String, String> {
    let files = read_dir_files(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if files.is_empty() {
        return Err(format!("{}: no files to pack", dir.display()));
    }
    if let Ok(existing) = std::fs::read(archive)
        && unpack(&existing, password).is_ok_and(|current| current == files)
    {
        return Ok(format!("{} unchanged", archive.display()));
    }
    let bytes = pack(&files, password).map_err(|e| e.to_string())?;
    std::fs::write(archive, bytes).map_err(|e| format!("{}: {e}", archive.display()))?;
    Ok(format!(
        "packed {} file(s) from {} -> {}",
        files.len(),
        dir.display(),
        archive.display()
    ))
}
