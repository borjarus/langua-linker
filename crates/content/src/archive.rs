//! Encrypted bundled content. `content/decks` holds a single 7-Zip archive (AES-256, encrypted
//! header, so neither file names nor text are readable without the password), never plain files.
//! The app embeds the archive and decrypts it in memory on the first read; the linter, the tests
//! and `content-pack` read it with the password from `LANGUA_DECKS_PASSWORD` or `.env`.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::io::{self, BufRead, Cursor};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use sevenz_rust2::encoder_options::AesEncoderOptions;
use sevenz_rust2::{ArchiveEntry, ArchiveReader, ArchiveWriter, EncoderMethod, Password};

use crate::source::{ContentSource, is_plain_file_name};

/// The archive inside a content directory (`content/decks/decks.7z`).
pub const ARCHIVE_FILE: &str = "decks.7z";
/// Environment variable (or `.env` key) holding the archive password.
pub const PASSWORD_VAR: &str = "LANGUA_DECKS_PASSWORD";

/// Key-derivation cost of the 7-Zip AES coder (2^19 SHA-256 rounds, the 7-Zip default).
const KDF_CYCLES_POWER: u8 = 19;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Decrypts and unpacks every file of an archive into memory.
pub fn unpack(archive: &[u8], password: &str) -> io::Result<BTreeMap<String, Vec<u8>>> {
    let mut reader = ArchiveReader::new(Cursor::new(archive), Password::from(password))
        .map_err(|error| invalid(format!("content archive: {error}")))?;
    let mut files = BTreeMap::new();
    reader
        .for_each_entries(|entry, data| {
            if entry.is_directory() {
                return Ok(true);
            }
            let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
            data.read_to_end(&mut bytes)?;
            files.insert(entry.name().to_owned(), bytes);
            Ok(true)
        })
        .map_err(|error| invalid(format!("content archive: {error}")))?;
    if let Some(name) = files.keys().find(|name| !is_plain_file_name(name)) {
        return Err(invalid(format!(
            "content archive: not a plain file name: {name}"
        )));
    }
    Ok(files)
}

/// Packs `files` into an encrypted 7-Zip archive (LZMA2 + AES-256, encrypted header).
pub fn pack(files: &BTreeMap<String, Vec<u8>>, password: &str) -> io::Result<Vec<u8>> {
    if password.is_empty() {
        return Err(invalid("refusing to pack content without a password"));
    }
    let mut aes = AesEncoderOptions::new(Password::from(password));
    aes.num_cycles_power = KDF_CYCLES_POWER;
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).map_err(io::Error::other)?;
    writer.set_content_methods(vec![aes.into(), EncoderMethod::LZMA2.into()]);
    writer.set_encrypt_header(true);
    for (name, bytes) in files {
        if !is_plain_file_name(name) {
            return Err(invalid(format!("not a plain file name: {name}")));
        }
        writer
            .push_archive_entry(ArchiveEntry::new_file(name), Some(bytes.as_slice()))
            .map_err(io::Error::other)?;
    }
    Ok(writer.finish()?.into_inner())
}

/// Unpacks the archive at `archive` into the directory `dest` (created if missing).
pub fn unpack_to_dir(archive: &Path, dest: &Path, password: &str) -> io::Result<()> {
    let files = unpack(&std::fs::read(archive)?, password)?;
    std::fs::create_dir_all(dest)?;
    for (name, bytes) in files {
        std::fs::write(dest.join(name), bytes)?;
    }
    Ok(())
}

/// The plain files (no subdirectories) of `dir`, by name.
pub fn read_dir_files(dir: &Path) -> io::Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            let name = entry.file_name().to_string_lossy().into_owned();
            files.insert(name, std::fs::read(entry.path())?);
        }
    }
    Ok(files)
}

/// The password from `LANGUA_DECKS_PASSWORD`, else from the first `.env` found in `start` or one
/// of its ancestors.
pub fn find_password(start: &Path) -> Option<String> {
    if let Ok(value) = std::env::var(PASSWORD_VAR)
        && !value.is_empty()
    {
        return Some(value);
    }
    start
        .ancestors()
        .map(|dir| dir.join(".env"))
        .find(|path| path.is_file())
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| dotenv_value(&text, PASSWORD_VAR))
}

/// The value of `key` in `.env` text (`KEY=value`, optional quotes, `#` comments).
pub fn dotenv_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let line = line.trim();
        let line = line.strip_prefix("export ").unwrap_or(line);
        let (name, value) = line.split_once('=')?;
        if name.trim() != key {
            return None;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        (!value.is_empty()).then(|| value.to_owned())
    })
}

/// A content source backed by an encrypted archive, decrypted in memory on the first `open`.
#[derive(Debug)]
pub struct ArchiveSource {
    archive: Cow<'static, [u8]>,
    password: Cow<'static, str>,
    files: OnceLock<Result<HashMap<String, Vec<u8>>, String>>,
}

impl ArchiveSource {
    /// An archive compiled into the binary.
    pub const fn embedded(archive: &'static [u8], password: &'static str) -> Self {
        Self {
            archive: Cow::Borrowed(archive),
            password: Cow::Borrowed(password),
            files: OnceLock::new(),
        }
    }

    pub fn new(archive: Vec<u8>, password: String) -> Self {
        Self {
            archive: Cow::Owned(archive),
            password: Cow::Owned(password),
            files: OnceLock::new(),
        }
    }

    /// Reads the archive of a content directory (`<dir>/decks.7z`) or an archive file.
    pub fn open_path(path: &Path, password: String) -> io::Result<Self> {
        let file: PathBuf = if path.is_dir() {
            path.join(ARCHIVE_FILE)
        } else {
            path.to_owned()
        };
        Ok(Self::new(std::fs::read(file)?, password))
    }

    fn files(&self) -> io::Result<&HashMap<String, Vec<u8>>> {
        self.files
            .get_or_init(|| {
                unpack(&self.archive, &self.password)
                    .map(|files| files.into_iter().collect())
                    .map_err(|error| error.to_string())
            })
            .as_ref()
            .map_err(|error| invalid(error.clone()))
    }

    /// Names of the files inside the archive, sorted.
    pub fn names(&self) -> io::Result<Vec<String>> {
        let mut names: Vec<String> = self.files()?.keys().cloned().collect();
        names.sort_unstable();
        Ok(names)
    }
}

impl ContentSource for ArchiveSource {
    fn open(&self, name: &str) -> io::Result<Box<dyn BufRead + Send + '_>> {
        if !is_plain_file_name(name) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("not a plain file name: {name}"),
            ));
        }
        self.files()?
            .get(name)
            .map(|bytes| Box::new(bytes.as_slice()) as Box<dyn BufRead + Send>)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, name.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    fn sample() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            ("catalog.json".to_owned(), b"{\"decks\":[]}".to_vec()),
            (
                "a.ndjson".to_owned(),
                "zażółć gęślą jaźń\n".repeat(50).into_bytes(),
            ),
        ])
    }

    #[test]
    fn round_trip_is_encrypted_and_hides_names() {
        let archive = pack(&sample(), "secret").unwrap();
        assert!(archive.starts_with(b"7z\xBC\xAF\x27\x1C"));
        for needle in [&b"catalog"[..], b"ndjson", "zażółć".as_bytes()] {
            assert!(!archive.windows(needle.len()).any(|w| w == needle));
        }
        assert_eq!(unpack(&archive, "secret").unwrap(), sample());
    }

    #[test]
    fn wrong_password_is_refused() {
        let archive = pack(&sample(), "secret").unwrap();
        assert!(unpack(&archive, "other").is_err());
        let source = ArchiveSource::new(archive, "other".into());
        assert_eq!(
            source.open("catalog.json").err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn source_serves_files_from_the_archive() {
        let source = ArchiveSource::new(pack(&sample(), "secret").unwrap(), "secret".into());
        let mut text = String::new();
        source
            .open("catalog.json")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "{\"decks\":[]}");
        assert_eq!(source.names().unwrap(), ["a.ndjson", "catalog.json"]);
        assert_eq!(
            source.open("missing.json").err().unwrap().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            source.open("../catalog.json").err().unwrap().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn empty_password_is_refused() {
        assert!(pack(&sample(), "").is_err());
    }

    #[test]
    fn dotenv_values() {
        let text = "# c\nOTHER=1\nexport LANGUA_DECKS_PASSWORD=\"p w\"\n";
        assert_eq!(dotenv_value(text, PASSWORD_VAR).as_deref(), Some("p w"));
        assert_eq!(
            dotenv_value("LANGUA_DECKS_PASSWORD='x'", PASSWORD_VAR).as_deref(),
            Some("x")
        );
        assert_eq!(dotenv_value("LANGUA_DECKS_PASSWORD=", PASSWORD_VAR), None);
        assert_eq!(dotenv_value("X=1", PASSWORD_VAR), None);
    }
}
