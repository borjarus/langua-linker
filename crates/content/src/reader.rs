//! Streaming NDJSON reader: one record per line, bounded line length, SHA-256 of every byte read.
//! Memory stays O(longest line) whatever the file size (plan step 8).

use std::io::BufRead;

use sha2::{Digest, Sha256};

use crate::format::{MAX_LINE_BYTES, NoteRecord};

/// Why a line could not be turned into a record. `code` is the linter/importer issue code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineError {
    pub code: &'static str,
    pub message: String,
}

impl LineError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Reads note records line by line. Blank lines are skipped; line numbers are 1-based.
pub struct NoteReader<R> {
    inner: R,
    line: u32,
    hasher: Sha256,
    finished: bool,
}

impl<R: BufRead> NoteReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            line: 0,
            hasher: Sha256::new(),
            finished: false,
        }
    }

    /// Lowercase hex SHA-256 of everything read so far (the whole file once exhausted).
    pub fn sha256_hex(&self) -> String {
        hex(&self.hasher.clone().finalize())
    }

    /// Reads one raw line (without the newline). `Ok(None)` at the end; a line longer than
    /// [`MAX_LINE_BYTES`] is consumed and reported as `line.tooLong`.
    fn raw_line(&mut self) -> Result<Option<Vec<u8>>, LineError> {
        let mut line = Vec::new();
        let mut too_long = false;
        let mut read_any = false;
        loop {
            let available = self
                .inner
                .fill_buf()
                .map_err(|error| LineError::new("file.read", error.to_string()))?;
            if available.is_empty() {
                break;
            }
            read_any = true;
            let (chunk, done) = match available.iter().position(|byte| *byte == b'\n') {
                Some(index) => (&available[..=index], true),
                None => (available, false),
            };
            self.hasher.update(chunk);
            if !too_long {
                if line.len() + chunk.len() > MAX_LINE_BYTES + 2 {
                    too_long = true;
                    line.clear();
                } else {
                    line.extend_from_slice(chunk);
                }
            }
            let consumed = chunk.len();
            self.inner.consume(consumed);
            if done {
                break;
            }
        }
        if !read_any {
            return Ok(None);
        }
        self.line += 1;
        if too_long {
            return Err(LineError::new(
                "line.tooLong",
                format!("line exceeds {MAX_LINE_BYTES} bytes"),
            ));
        }
        while matches!(line.last(), Some(b'\n' | b'\r')) {
            line.pop();
        }
        if line.len() > MAX_LINE_BYTES {
            return Err(LineError::new(
                "line.tooLong",
                format!("line exceeds {MAX_LINE_BYTES} bytes"),
            ));
        }
        Ok(Some(line))
    }
}

impl<R: BufRead> Iterator for NoteReader<R> {
    /// `(line number, record or error)`.
    type Item = (u32, Result<NoteRecord, LineError>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        loop {
            match self.raw_line() {
                Ok(None) => {
                    self.finished = true;
                    return None;
                }
                Ok(Some(line)) if line.iter().all(u8::is_ascii_whitespace) => continue,
                Ok(Some(line)) => return Some((self.line, parse(&line))),
                Err(error) if error.code == "file.read" => {
                    self.finished = true;
                    return Some((self.line, Err(error)));
                }
                Err(error) => return Some((self.line, Err(error))),
            }
        }
    }
}

fn parse(line: &[u8]) -> Result<NoteRecord, LineError> {
    let text = std::str::from_utf8(line)
        .map_err(|_| LineError::new("line.utf8", "line is not valid UTF-8"))?;
    serde_json::from_str(text).map_err(|error| {
        let message = error.to_string();
        let code = if message.starts_with("unknown field") {
            "record.unknownField"
        } else if error.is_data() {
            "record.invalid"
        } else {
            "record.malformed"
        };
        LineError::new(code, message)
    })
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// SHA-256 of a byte slice as lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(input: &[u8]) -> Vec<(u32, Result<NoteRecord, LineError>)> {
        NoteReader::new(input).collect()
    }

    #[test]
    fn reads_records_and_skips_blank_lines() {
        let input = b"{\"guid\":\"a\",\"front\":\"x\",\"back\":\"y\"}\r\n\n  \n{\"guid\":\"b\",\"front\":\"x\",\"back\":\"y\"}";
        let items = read(input);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].0, 1);
        assert_eq!(items[1].0, 4);
        assert_eq!(items[1].1.as_ref().unwrap().guid, "b");
    }

    #[test]
    fn classifies_bad_lines_and_keeps_going() {
        let input = b"{\"guid\":\"a\"\n{\"guid\":\"a\",\"front\":\"x\",\"back\":\"y\",\"extra\":1}\n{\"guid\":1,\"front\":\"x\",\"back\":\"y\"}\n\xff\xfe\n{\"guid\":\"z\",\"front\":\"x\",\"back\":\"y\"}\n";
        let codes: Vec<_> = read(input)
            .into_iter()
            .map(|(line, item)| (line, item.err().map(|error| error.code)))
            .collect();
        assert_eq!(
            codes,
            [
                (1, Some("record.malformed")),
                (2, Some("record.unknownField")),
                (3, Some("record.invalid")),
                (4, Some("line.utf8")),
                (5, None),
            ]
        );
    }

    #[test]
    fn overlong_lines_are_rejected_without_buffering_them() {
        let mut input = vec![b'x'; MAX_LINE_BYTES * 3];
        input.extend_from_slice(b"\n{\"guid\":\"a\",\"front\":\"x\",\"back\":\"y\"}\n");
        let items = read(&input);
        assert_eq!(items[0].1.as_ref().unwrap_err().code, "line.tooLong");
        assert_eq!(items[1].0, 2);
        assert!(items[1].1.is_ok());
    }

    #[test]
    fn checksum_covers_every_byte() {
        let input = b"{\"guid\":\"a\",\"front\":\"x\",\"back\":\"y\"}\n\n";
        let mut reader = NoteReader::new(&input[..]);
        for _ in &mut reader {}
        assert_eq!(reader.sha256_hex(), sha256_hex(input));
    }
}
