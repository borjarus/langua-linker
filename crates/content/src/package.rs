//! Import packages: one NDJSON file whose first line is the manifest (`PackageHeader`) and whose
//! other lines are note records. Produced by `apkg-convert`, imported from the Transfer screen.

use std::collections::HashSet;
use std::io::{self, BufRead};
use std::sync::Arc;

use crate::check::{DeckChecker, Issues, Mode, check_manifest};
use crate::format::{DeckManifest, MAX_LINE_BYTES, PACKAGE_VERSION, PackageHeader, SourceKind};
use crate::reader::{NoteReader, sha256_hex};
use crate::source::ContentSource;

/// Largest package accepted (the whole file is held in memory between preview and import).
pub const MAX_PACKAGE_BYTES: usize = 64 * 1024 * 1024;

/// What a file turned out to be when it is not a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotAPackage {
    /// A ZIP archive — most likely the `.apkg` itself, which must be converted first.
    Zip,
    TooLarge,
    /// Anything else (no package header on the first line).
    Unknown,
}

/// A parsed package: the manifest and the bytes of the note lines.
#[derive(Debug, Clone)]
pub struct Package {
    pub manifest: DeckManifest,
    body: Arc<[u8]>,
}

/// Result of checking a whole package.
#[derive(Debug, Default)]
pub struct PackageCheck {
    pub issues: Issues,
    pub notes: u32,
    pub cards: u32,
    pub scheduled: u32,
    pub suspended: u32,
    pub reviews: u32,
    pub note_guids: HashSet<String>,
}

impl Package {
    /// Splits the header line from the notes. Fails with [`NotAPackage`] before any record is
    /// parsed; manifest and record problems are reported by [`Package::check`].
    pub fn parse(bytes: &[u8]) -> Result<Result<Self, Issues>, NotAPackage> {
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err(NotAPackage::TooLarge);
        }
        if bytes.starts_with(b"PK\x03\x04") {
            return Err(NotAPackage::Zip);
        }
        let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
        let end = bytes
            .iter()
            .position(|byte| *byte == b'\n')
            .unwrap_or(bytes.len());
        let first = &bytes[..end];
        if first.len() > MAX_LINE_BYTES {
            return Err(NotAPackage::Unknown);
        }
        let Ok(text) = std::str::from_utf8(first) else {
            return Err(NotAPackage::Unknown);
        };
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|_| NotAPackage::Unknown)?;
        if value.get("package").is_none() || value.get("manifest").is_none() {
            return Err(NotAPackage::Unknown);
        }
        let mut issues = Issues::default();
        let header: PackageHeader = match serde_json::from_value(value) {
            Ok(header) => header,
            Err(error) => {
                let code = if error.to_string().starts_with("unknown field") {
                    "manifest.unknownField"
                } else {
                    "manifest.malformed"
                };
                issues.error(code, "package", Some(1), error.to_string());
                return Ok(Err(issues));
            }
        };
        if header.package != PACKAGE_VERSION {
            issues.error(
                "package.unsupported",
                "package",
                Some(1),
                format!(
                    "package layout {} (this build reads {PACKAGE_VERSION})",
                    header.package
                ),
            );
            return Ok(Err(issues));
        }
        let body_start = (end + 1).min(bytes.len());
        Ok(Ok(Self {
            manifest: header.manifest,
            body: Arc::from(&bytes[body_start..]),
        }))
    }

    /// SHA-256 of the note lines, the value `manifest.sha256` must have.
    pub fn body_sha256(&self) -> String {
        sha256_hex(&self.body)
    }

    /// Serves the note lines under the manifest's `notesFile` name, so the importer reads a
    /// package like a deck of the content directory.
    pub fn source(&self) -> PackageSource {
        PackageSource {
            notes_file: self.manifest.notes_file.clone(),
            body: self.body.clone(),
        }
    }

    /// Every rule of the linter for a package: manifest (source `import`, no dependencies),
    /// records, scheduling and history, counts and checksum.
    pub fn check(&self) -> PackageCheck {
        let mut result = PackageCheck::default();
        let file = format!("{} (manifest)", self.manifest.notes_file);
        if self.manifest.source != SourceKind::Import {
            result.issues.error(
                "package.source",
                &file,
                Some(1),
                "a package declares source \"import\"",
            );
        }
        check_manifest(&self.manifest, &file, &[], Mode::Dev, &mut result.issues);
        let mut seen = HashSet::new();
        let none = HashSet::new();
        let mut checker = DeckChecker::new(&self.manifest, &mut seen, &none);
        let mut reader = NoteReader::new(&self.body[..]);
        for (line, record) in &mut reader {
            // Line numbers of the file: the header is line 1.
            match record {
                Ok(record) => checker.record(line + 1, &record, &mut result.issues),
                Err(error) => result.issues.error(
                    error.code,
                    &self.manifest.notes_file,
                    Some(line + 1),
                    error.message,
                ),
            }
        }
        result.notes = checker.notes;
        result.scheduled = checker.scheduled;
        result.suspended = checker.suspended;
        result.reviews = checker.reviews;
        result.note_guids = std::mem::take(&mut checker.note_guids);
        result.cards = self.manifest.expected_cards;
        checker.finish(&reader.sha256_hex(), &mut result.issues);
        result
    }
}

/// [`ContentSource`] over the note lines of a package.
#[derive(Debug, Clone)]
pub struct PackageSource {
    notes_file: String,
    body: Arc<[u8]>,
}

impl ContentSource for PackageSource {
    fn open(&self, name: &str) -> io::Result<Box<dyn BufRead + Send + '_>> {
        if name == self.notes_file {
            Ok(Box::new(&self.body[..]))
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, name.to_owned()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_and_foreign_files_are_not_packages() {
        assert_eq!(
            Package::parse(b"PK\x03\x04rest").unwrap_err(),
            NotAPackage::Zip
        );
        assert_eq!(
            Package::parse(b"front;back\n").unwrap_err(),
            NotAPackage::Unknown
        );
        assert_eq!(
            Package::parse(b"{\"guid\":\"x\"}\n").unwrap_err(),
            NotAPackage::Unknown
        );
        assert_eq!(Package::parse(b"").unwrap_err(), NotAPackage::Unknown);
    }

    #[test]
    fn unknown_package_layout_is_reported() {
        let issues = Package::parse(b"{\"package\":2,\"manifest\":{}}\n")
            .unwrap()
            .unwrap_err();
        assert!(issues.has_errors());
    }
}
