//! Where content files come from: a directory (linter, tests) or bytes compiled into the app
//! (the shell embeds `content/decks`, so mobile builds need no asset path at runtime).

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;

/// Read access to the files of one content directory. Names are plain file names.
pub trait ContentSource: Send + Sync {
    /// Opens `name` for streaming.
    fn open(&self, name: &str) -> io::Result<Box<dyn BufRead + Send + '_>>;
}

/// A file name without directories, so a manifest cannot point outside its content directory.
pub fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn refuse(name: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("not a plain file name: {name}"),
    )
}

/// Files of a directory on disk.
#[derive(Debug, Clone)]
pub struct DirSource {
    root: PathBuf,
}

impl DirSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl ContentSource for DirSource {
    fn open(&self, name: &str) -> io::Result<Box<dyn BufRead + Send + '_>> {
        if !is_plain_file_name(name) {
            return Err(refuse(name));
        }
        Ok(Box::new(BufReader::new(File::open(self.root.join(name))?)))
    }
}

/// Files compiled into the binary (`include_bytes!`).
#[derive(Debug, Clone, Copy)]
pub struct StaticSource {
    files: &'static [(&'static str, &'static [u8])],
}

impl StaticSource {
    pub const fn new(files: &'static [(&'static str, &'static [u8])]) -> Self {
        Self { files }
    }
}

impl ContentSource for StaticSource {
    fn open(&self, name: &str) -> io::Result<Box<dyn BufRead + Send + '_>> {
        if !is_plain_file_name(name) {
            return Err(refuse(name));
        }
        self.files
            .iter()
            .find(|(file, _)| *file == name)
            .map(|(_, bytes)| Box::new(*bytes) as Box<dyn BufRead + Send>)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, name.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    #[test]
    fn plain_names_only() {
        assert!(is_plain_file_name("de_words_a1.ndjson"));
        for bad in ["", ".", "..", "../x", "a/b", "a\\b", "C:x", "ä.json"] {
            assert!(!is_plain_file_name(bad), "{bad}");
        }
    }

    #[test]
    fn static_source_serves_its_files() {
        static FILES: &[(&str, &[u8])] = &[("a.json", b"{}")];
        let source = StaticSource::new(FILES);
        let mut text = String::new();
        source
            .open("a.json")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "{}");
        assert_eq!(
            source.open("b.json").err().unwrap().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            source.open("../a.json").err().unwrap().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn dir_source_refuses_paths() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.json"), "{}").unwrap();
        let source = DirSource::new(dir.path());
        assert!(source.open("a.json").is_ok());
        assert_eq!(
            source.open("../a.json").err().unwrap().kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
