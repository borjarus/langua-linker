//! CSV/TSV text files (plan Phase 14 step 1).
//!
//! - **Decoding.** A BOM decides (UTF-8, UTF-16 LE/BE); without one the bytes must be valid
//!   UTF-8, otherwise they are read as Windows-1252 (what spreadsheets on Windows save). The
//!   caller can force an encoding; a forced UTF-8 that is not valid is an error, never a guess.
//! - **Anki directives.** Leading `#key:value` lines of an Anki text export are read
//!   (`#separator`, `#columns`, `#html`) and skipped.
//! - **Delimiter detection.** Comma, semicolon and tab are tried on the first records (quotes
//!   respected); a delimiter is reported only when exactly one gives a consistent column count of
//!   two or more. Otherwise detection is uncertain and the caller shows the picker.
//! - **Reading.** RFC 4180: quoted fields may hold delimiters, `""` and line breaks; CRLF, LF and
//!   CR end a record; a quote inside an unquoted field is literal; blank lines are skipped. The
//!   number of records and columns and the size of a field are bounded.
//! - **Writing.** CRLF, UTF-8 with a BOM (spreadsheets otherwise guess a legacy code page),
//!   quotes only where needed. The formula guard is an explicit option: with it a field starting
//!   with `=`, `+`, `-`, `@`, tab or CR gets a leading `'` so a spreadsheet shows text instead of
//!   evaluating it; without it (the raw variant) every field is written byte for byte. Reading
//!   with the guard removes exactly that `'` again — so a text that itself starts with `'=` does
//!   not survive a guarded round trip; the raw variant is lossless.

use std::fmt;

/// Characters that make a spreadsheet evaluate a cell (OWASP CSV injection).
const FORMULA_STARTS: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];
/// Records sampled for delimiter detection.
const DETECTION_SAMPLE: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Windows1252,
}

impl Encoding {
    pub const ALL: [Self; 4] = [Self::Utf8, Self::Utf16Le, Self::Utf16Be, Self::Windows1252];

    /// Contract label (`ImportPreviewDto.encoding`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf16Le => "UTF-16LE",
            Self::Utf16Be => "UTF-16BE",
            Self::Windows1252 => "windows-1252",
        }
    }

    pub fn parse(label: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|encoding| encoding.label().eq_ignore_ascii_case(label))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Delimiter {
    Comma,
    Semicolon,
    Tab,
}

impl Delimiter {
    pub const ALL: [Self; 3] = [Self::Comma, Self::Semicolon, Self::Tab];

    pub fn as_char(self) -> char {
        match self {
            Self::Comma => ',',
            Self::Semicolon => ';',
            Self::Tab => '\t',
        }
    }

    pub fn from_char(value: char) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|delimiter| delimiter.as_char() == value)
    }

    /// Value of Anki's `#separator:` directive.
    fn from_directive(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "comma" | "," => Some(Self::Comma),
            "semicolon" | ";" => Some(Self::Semicolon),
            "tab" | "\t" => Some(Self::Tab),
            _ => None,
        }
    }
}

/// Why a file cannot be read at all (row problems are reported per row by the caller).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsvError {
    /// The bytes are not valid in the forced encoding.
    Encoding,
    /// A quoted field is still open at the end of the file; the line where it started.
    UnterminatedQuote {
        line: u32,
    },
    TooManyRows,
    TooManyColumns {
        line: u32,
    },
    FieldTooLarge {
        line: u32,
    },
    /// No record at all.
    Empty,
}

impl CsvError {
    /// Stable reason code (`ErrorDto.details.reason`).
    pub fn code(self) -> &'static str {
        match self {
            Self::Encoding => "encoding",
            Self::UnterminatedQuote { .. } => "unterminatedQuote",
            Self::TooManyRows => "tooManyRows",
            Self::TooManyColumns { .. } => "tooManyColumns",
            Self::FieldTooLarge { .. } => "fieldTooLarge",
            Self::Empty => "empty",
        }
    }

    pub fn line(self) -> Option<u32> {
        match self {
            Self::UnterminatedQuote { line }
            | Self::TooManyColumns { line }
            | Self::FieldTooLarge { line } => Some(line),
            _ => None,
        }
    }
}

impl fmt::Display for CsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line() {
            Some(line) => write!(f, "{} (line {line})", self.code()),
            None => f.write_str(self.code()),
        }
    }
}

impl std::error::Error for CsvError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_records: usize,
    pub max_columns: usize,
    /// Bytes of one field (UTF-8); a guard against a runaway quote, not the note-field limit.
    pub max_field_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_records: 100_000,
            max_columns: 64,
            max_field_bytes: 256 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    pub encoding: Encoding,
    pub has_bom: bool,
}

/// Decodes a text file; see the module notes for the rules.
pub fn decode(bytes: &[u8], forced: Option<Encoding>) -> Result<Decoded, CsvError> {
    let (bom, rest) = if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        (Some(Encoding::Utf8), rest)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        (Some(Encoding::Utf16Le), rest)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        (Some(Encoding::Utf16Be), rest)
    } else {
        (None, bytes)
    };
    let encoding = match (forced, bom) {
        (Some(forced), _) => forced,
        (None, Some(bom)) => bom,
        (None, None) if std::str::from_utf8(bytes).is_ok() => Encoding::Utf8,
        (None, None) => Encoding::Windows1252,
    };
    // A forced encoding other than the BOM's reads the BOM bytes as text: refuse instead.
    if bom.is_some_and(|bom| bom != encoding) {
        return Err(CsvError::Encoding);
    }
    let text = match encoding {
        Encoding::Utf8 => std::str::from_utf8(rest)
            .map_err(|_| CsvError::Encoding)?
            .to_owned(),
        Encoding::Utf16Le | Encoding::Utf16Be => {
            if rest.len() % 2 != 0 {
                return Err(CsvError::Encoding);
            }
            let units = rest.chunks_exact(2).map(|pair| {
                if encoding == Encoding::Utf16Le {
                    u16::from_le_bytes([pair[0], pair[1]])
                } else {
                    u16::from_be_bytes([pair[0], pair[1]])
                }
            });
            char::decode_utf16(units)
                .collect::<Result<String, _>>()
                .map_err(|_| CsvError::Encoding)?
        }
        Encoding::Windows1252 => rest.iter().map(|&byte| windows_1252(byte)).collect(),
    };
    Ok(Decoded {
        text,
        encoding,
        has_bom: bom.is_some(),
    })
}

/// Windows-1252: Latin-1 except 0x80–0x9F; the five unassigned bytes map to their C1 code.
fn windows_1252(byte: u8) -> char {
    const HIGH: [u16; 32] = [
        0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
        0x2039, 0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022,
        0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
    ];
    match byte {
        0x80..=0x9F => {
            char::from_u32(u32::from(HIGH[usize::from(byte - 0x80)])).unwrap_or('\u{fffd}')
        }
        _ => char::from(byte),
    }
}

/// Header lines of an Anki text export.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Directives {
    pub separator: Option<Delimiter>,
    /// `#columns:` names, split with the separator.
    pub columns: Option<Vec<String>>,
    pub html: Option<bool>,
    /// Lines taken by the directives (skipped by the reader).
    pub lines: usize,
}

/// Reads the leading `#key:value` lines.
pub fn directives(text: &str) -> Directives {
    let mut found = Directives::default();
    let mut columns: Option<String> = None;
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let Some(body) = line.strip_prefix('#') else {
            break;
        };
        let Some((key, value)) = body.split_once(':') else {
            break;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "separator" => found.separator = Delimiter::from_directive(value),
            "columns" => columns = Some(value.to_owned()),
            "html" => found.html = Some(value.trim().eq_ignore_ascii_case("true")),
            _ => {}
        }
        found.lines += 1;
    }
    if let Some(columns) = columns {
        let separator = found.separator.unwrap_or(Delimiter::Tab).as_char();
        found.columns = Some(columns.split(separator).map(str::to_owned).collect());
    }
    found
}

/// One record with the 1-based file line it starts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub line: u32,
    pub fields: Vec<String>,
}

/// Guesses the delimiter; `None` when the guess is uncertain (see the module notes).
pub fn detect_delimiter(text: &str, skip_lines: usize) -> Option<Delimiter> {
    let mut best: Option<(Delimiter, usize)> = None;
    let mut tie = false;
    for delimiter in Delimiter::ALL {
        let limits = Limits {
            max_records: DETECTION_SAMPLE,
            ..Limits::default()
        };
        let records = match read(text, skip_lines, delimiter, limits, true) {
            Ok(records) => records,
            Err(_) => continue,
        };
        let Some(first) = records.first() else {
            continue;
        };
        let columns = first.fields.len();
        if columns < 2 || records.iter().any(|record| record.fields.len() != columns) {
            continue;
        }
        match best {
            Some((_, known)) if known == columns => tie = true,
            Some((_, known)) if known > columns => {}
            _ => {
                best = Some((delimiter, columns));
                tie = false;
            }
        }
    }
    if tie {
        None
    } else {
        best.map(|(delimiter, _)| delimiter)
    }
}

/// Reads every record; see the module notes for the rules.
pub fn parse(
    text: &str,
    skip_lines: usize,
    delimiter: Delimiter,
    limits: Limits,
) -> Result<Vec<Record>, CsvError> {
    let records = read(text, skip_lines, delimiter, limits, false)?;
    if records.is_empty() {
        return Err(CsvError::Empty);
    }
    Ok(records)
}

/// The reader behind [`parse`] and detection; `sample` stops quietly at `max_records`.
fn read(
    text: &str,
    skip_lines: usize,
    delimiter: Delimiter,
    limits: Limits,
    sample: bool,
) -> Result<Vec<Record>, CsvError> {
    let delimiter = delimiter.as_char();
    let mut records = Vec::new();
    let mut line: u32 = 1;
    let mut chars = text.chars().peekable();
    // Directive lines.
    let mut skipped = 0;
    while skipped < skip_lines {
        match chars.next() {
            None => break,
            Some('\n') => {
                skipped += 1;
                line += 1;
            }
            Some('\r') => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                skipped += 1;
                line += 1;
            }
            Some(_) => {}
        }
    }

    let mut fields: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut in_quotes = false;
    let mut start_line = line;
    let mut quote_line = line;
    let mut record_started = false;

    let end_field = |fields: &mut Vec<String>, field: &mut String, quoted: &mut bool| {
        fields.push(std::mem::take(field));
        *quoted = false;
    };

    loop {
        let next = chars.next();
        if in_quotes {
            match next {
                None => return Err(CsvError::UnterminatedQuote { line: quote_line }),
                Some('"') if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                Some('"') => in_quotes = false,
                Some(c) => {
                    if c == '\n' || (c == '\r' && chars.peek() != Some(&'\n')) {
                        line += 1;
                    }
                    field.push(c);
                }
            }
        } else {
            match next {
                Some('"') if field.is_empty() && !quoted => {
                    in_quotes = true;
                    quoted = true;
                    quote_line = line;
                    record_started = true;
                }
                Some(c) if c == delimiter => {
                    end_field(&mut fields, &mut field, &mut quoted);
                    record_started = true;
                }
                None | Some('\n' | '\r') => {
                    if next == Some('\r') && chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    let blank = !record_started && field.is_empty() && fields.is_empty();
                    if !blank {
                        end_field(&mut fields, &mut field, &mut quoted);
                        if fields.len() > limits.max_columns {
                            return Err(CsvError::TooManyColumns { line: start_line });
                        }
                        if records.len() == limits.max_records {
                            if sample {
                                return Ok(records);
                            }
                            return Err(CsvError::TooManyRows);
                        }
                        records.push(Record {
                            line: start_line,
                            fields: std::mem::take(&mut fields),
                        });
                    }
                    record_started = false;
                    if next.is_none() {
                        return Ok(records);
                    }
                    line += 1;
                    start_line = line;
                }
                Some(c) => {
                    field.push(c);
                    record_started = true;
                }
            }
        }
        if field.len() > limits.max_field_bytes {
            return Err(CsvError::FieldTooLarge { line: start_line });
        }
    }
}

/// Plain text of an HTML field from an Anki text export (`#html:true`): `<br>` and block ends
/// become line breaks, other tags are dropped, the common entities decoded. Scripts and styles
/// are dropped with their content — imported HTML is never rendered.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mut skip_until: Option<&'static str> = None;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some(close) = rest.find('>')
        {
            let end = close + 1;
            let tag = rest[1..close].trim().to_ascii_lowercase();
            let name: String = tag
                .trim_start_matches('/')
                .chars()
                .take_while(char::is_ascii_alphanumeric)
                .collect();
            if let Some(closing) = skip_until {
                if tag.starts_with('/') && name == closing {
                    skip_until = None;
                }
            } else if !tag.starts_with('/') && (name == "script" || name == "style") {
                skip_until = Some(if name == "script" { "script" } else { "style" });
            } else if name == "br"
                || (tag.starts_with('/') && matches!(name.as_str(), "div" | "p" | "li"))
            {
                out.push('\n');
            }
            rest = &rest[end..];
            continue;
        }
        if skip_until.is_some() {
            rest = &rest[c.len_utf8()..];
            continue;
        }
        if c == '&'
            && let Some(end) = rest.find(';').filter(|end| *end <= 8)
        {
            let decoded = match &rest[1..end] {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some(' '),
                _ => None,
            };
            if let Some(decoded) = decoded {
                out.push(decoded);
                rest = &rest[end + 1..];
                continue;
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out.trim_end_matches('\n').to_owned()
}

/// Whether a spreadsheet would evaluate this text.
pub fn looks_like_formula(field: &str) -> bool {
    field.starts_with(FORMULA_STARTS)
}

/// Removes the guard `'` written by [`Writer`] with the formula guard on.
pub fn unguard(field: &str) -> &str {
    match field.strip_prefix('\'') {
        Some(rest) if looks_like_formula(rest) => rest,
        _ => field,
    }
}

/// CSV writer; see the module notes.
pub struct Writer {
    delimiter: char,
    formula_guard: bool,
    out: String,
    /// Fields that started like a formula (guarded or, in the raw variant, written as they are).
    pub formula_like: u32,
}

impl Writer {
    pub fn new(delimiter: Delimiter, formula_guard: bool) -> Self {
        Self {
            delimiter: delimiter.as_char(),
            formula_guard,
            out: String::from('\u{feff}'),
            formula_like: 0,
        }
    }

    pub fn record<S: AsRef<str>>(&mut self, fields: &[S]) {
        for (index, field) in fields.iter().enumerate() {
            if index > 0 {
                self.out.push(self.delimiter);
            }
            let field = field.as_ref();
            let guarded;
            let field = if looks_like_formula(field) {
                self.formula_like += 1;
                if self.formula_guard {
                    guarded = format!("'{field}");
                    guarded.as_str()
                } else {
                    field
                }
            } else {
                field
            };
            let needs_quotes = field.contains([self.delimiter, '"', '\n', '\r'])
                || field.starts_with(' ')
                || field.ends_with(' ');
            if needs_quotes {
                self.out.push('"');
                self.out.push_str(&field.replace('"', "\"\""));
                self.out.push('"');
            } else {
                self.out.push_str(field);
            }
        }
        self.out.push_str("\r\n");
    }

    pub fn finish(self) -> Vec<u8> {
        self.out.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(text: &str, delimiter: Delimiter) -> Vec<Vec<String>> {
        parse(text, 0, delimiter, Limits::default())
            .unwrap()
            .into_iter()
            .map(|record| record.fields)
            .collect()
    }

    #[test]
    fn quoted_fields_hold_delimiters_quotes_and_line_breaks() {
        let text = "front,back\r\n\"a, b\",\"say \"\"hi\"\"\"\n\"two\nlines\",x\n";
        let records = parse(text, 0, Delimiter::Comma, Limits::default()).unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[1].fields, ["a, b", "say \"hi\""]);
        assert_eq!(records[2].fields, ["two\nlines", "x"]);
        assert_eq!(
            records.iter().map(|record| record.line).collect::<Vec<_>>(),
            [1, 2, 3]
        );
    }

    #[test]
    fn line_numbers_count_breaks_inside_quotes() {
        let text = "\"a\r\nb\",1\r\nc,2\r\n";
        let records = parse(text, 0, Delimiter::Comma, Limits::default()).unwrap();
        assert_eq!(records[1].line, 3);
    }

    #[test]
    fn blank_lines_are_skipped_and_empty_fields_kept() {
        assert_eq!(
            rows("a;;c\n\n\n;b;\n", Delimiter::Semicolon),
            [vec!["a", "", "c"], vec!["", "b", ""]]
        );
    }

    #[test]
    fn a_quote_inside_an_unquoted_field_is_literal() {
        assert_eq!(
            rows("5\" disk,x\n", Delimiter::Comma),
            [vec!["5\" disk", "x"]]
        );
    }

    #[test]
    fn an_open_quote_at_the_end_names_its_line() {
        let error = parse(
            "a,b\nc,\"open\nmore",
            0,
            Delimiter::Comma,
            Limits::default(),
        )
        .unwrap_err();
        assert_eq!(error, CsvError::UnterminatedQuote { line: 2 });
        assert_eq!(error.code(), "unterminatedQuote");
    }

    #[test]
    fn limits_are_enforced() {
        let limits = Limits {
            max_records: 2,
            max_columns: 3,
            max_field_bytes: 8,
        };
        assert_eq!(
            parse("a\nb\nc\n", 0, Delimiter::Comma, limits),
            Err(CsvError::TooManyRows)
        );
        assert_eq!(
            parse("a,b,c,d\n", 0, Delimiter::Comma, limits),
            Err(CsvError::TooManyColumns { line: 1 })
        );
        assert_eq!(
            parse("123456789\n", 0, Delimiter::Comma, limits),
            Err(CsvError::FieldTooLarge { line: 1 })
        );
        assert_eq!(
            parse("\n\n", 0, Delimiter::Comma, limits),
            Err(CsvError::Empty)
        );
    }

    #[test]
    fn boms_and_encodings_are_decoded() {
        let utf8 = decode(b"\xEF\xBB\xBFz\xC3\xB3\xC5\x82w", None).unwrap();
        assert_eq!(
            (utf8.text.as_str(), utf8.encoding, utf8.has_bom),
            ("zółw", Encoding::Utf8, true)
        );
        let le = decode(&[0xFF, 0xFE, b'a', 0, 0x42, 0x01], None).unwrap();
        assert_eq!((le.text.as_str(), le.encoding), ("ał", Encoding::Utf16Le));
        let be = decode(&[0xFE, 0xFF, 0, b'a', 0x01, 0x42], None).unwrap();
        assert_eq!((be.text.as_str(), be.encoding), ("ał", Encoding::Utf16Be));
        let legacy = decode(b"caf\xE9 \x80", None).unwrap();
        assert_eq!(
            (legacy.text.as_str(), legacy.encoding, legacy.has_bom),
            ("café €", Encoding::Windows1252, false)
        );
    }

    #[test]
    fn a_forced_encoding_never_guesses() {
        assert_eq!(
            decode(b"caf\xE9", Some(Encoding::Utf8)),
            Err(CsvError::Encoding)
        );
        assert_eq!(
            decode(b"\xEF\xBB\xBFa", Some(Encoding::Windows1252)),
            Err(CsvError::Encoding)
        );
        assert_eq!(decode(&[0xFF, 0xFE, b'a'], None), Err(CsvError::Encoding));
        assert_eq!(
            decode(b"abc", Some(Encoding::Windows1252)).unwrap().text,
            "abc"
        );
    }

    #[test]
    fn delimiter_detection_needs_one_consistent_candidate() {
        assert_eq!(detect_delimiter("a,b\nc,d\n", 0), Some(Delimiter::Comma));
        assert_eq!(
            detect_delimiter("a;b;c\nd;e;f\n", 0),
            Some(Delimiter::Semicolon)
        );
        assert_eq!(
            detect_delimiter("a\tb\n\"c\td\"\te\n", 0),
            Some(Delimiter::Tab)
        );
        // A comma inside a semicolon file does not win: its column count is inconsistent.
        assert_eq!(
            detect_delimiter("Hund;dog, hound\nKatze;cat\n", 0),
            Some(Delimiter::Semicolon)
        );
        // Both give two consistent columns: uncertain.
        assert_eq!(detect_delimiter("a,b;c\nd,e;f\n", 0), None);
        // One column only.
        assert_eq!(detect_delimiter("word\nother\n", 0), None);
    }

    #[test]
    fn anki_directives_are_read_and_skipped() {
        let text = "#separator:tab\n#html:true\n#columns:Front\tBack\nHund\tdog\n";
        let found = directives(text);
        assert_eq!(found.separator, Some(Delimiter::Tab));
        assert_eq!(found.html, Some(true));
        assert_eq!(
            found.columns.as_deref(),
            Some(&["Front".to_owned(), "Back".to_owned()][..])
        );
        assert_eq!(found.lines, 3);
        let records = parse(text, found.lines, Delimiter::Tab, Limits::default()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            (records[0].line, records[0].fields.clone()),
            (4, vec!["Hund".into(), "dog".into()])
        );
        assert_eq!(directives("front,back\n").lines, 0);
    }

    #[test]
    fn writer_quotes_only_where_needed_and_guards_formulas_explicitly() {
        let mut raw = Writer::new(Delimiter::Comma, false);
        raw.record(&["plain", "a,b", "say \"x\"", " pad", "=1+1"]);
        let raw_text = String::from_utf8(raw.finish()).unwrap();
        assert_eq!(
            raw_text,
            "\u{feff}plain,\"a,b\",\"say \"\"x\"\"\",\" pad\",=1+1\r\n"
        );

        let mut guarded = Writer::new(Delimiter::Semicolon, true);
        guarded.record(&["=SUM(A1)", "-5", "@x", "+1", "a-b"]);
        assert_eq!(guarded.formula_like, 4);
        let text = String::from_utf8(guarded.finish()).unwrap();
        assert_eq!(text, "\u{feff}'=SUM(A1);'-5;'@x;'+1;a-b\r\n");
        assert_eq!(unguard("'=SUM(A1)"), "=SUM(A1)");
        assert_eq!(unguard("'quoted"), "'quoted");
    }

    #[test]
    fn html_fields_become_plain_text_without_scripts() {
        assert_eq!(html_to_text("der Hund<br>the dog"), "der Hund\nthe dog");
        assert_eq!(html_to_text("<div>a</div><div>b &amp; c</div>"), "a\nb & c");
        assert_eq!(
            html_to_text("x<script>alert(1)</script><img src=x onerror=alert(2)>y"),
            "xy"
        );
        assert_eq!(
            html_to_text("1 &lt; 2 &unknown; &nbsp;"),
            "1 < 2 &unknown;  "
        );
    }

    #[test]
    fn written_records_read_back_unchanged() {
        let fields = vec![
            "Hund".to_owned(),
            "line\nbreak, \"quoted\"".to_owned(),
            String::new(),
            "\u{1f600} emoji".to_owned(),
        ];
        let mut writer = Writer::new(Delimiter::Tab, false);
        writer.record(&fields);
        let bytes = writer.finish();
        let decoded = decode(&bytes, None).unwrap();
        assert!(decoded.has_bom);
        let records = parse(&decoded.text, 0, Delimiter::Tab, Limits::default()).unwrap();
        assert_eq!(records[0].fields, fields);
    }

    proptest::proptest! {
        #[test]
        fn any_fields_round_trip(fields in proptest::collection::vec("[a-z ,;\"\\n\\r=+@-]{0,12}", 1..6), guard: bool) {
            let mut writer = Writer::new(Delimiter::Comma, guard);
            writer.record(&fields);
            let decoded = decode(&writer.finish(), None).unwrap();
            let records = parse(&decoded.text, 0, Delimiter::Comma, Limits::default());
            // A record of one empty field is a blank line: nothing to read back.
            if fields.len() == 1 && fields[0].is_empty() {
                proptest::prop_assert_eq!(records, Err(CsvError::Empty));
            } else {
                let read: Vec<String> = records.unwrap()[0]
                    .fields
                    .iter()
                    .map(|field| if guard { unguard(field).to_owned() } else { field.clone() })
                    .collect();
                proptest::prop_assert_eq!(read, fields);
            }
        }
    }
}
