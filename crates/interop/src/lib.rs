//! CSV and Anki interoperability (plan Phase 14 steps 1–3, ADR-19).
//!
//! - [`csv`]: decoding (BOM, UTF-8/UTF-16/Windows-1252), delimiter detection, an RFC 4180
//!   reader with limits, and a writer with an explicit spreadsheet-formula guard.
//! - [`anki`]: the card codec between the collection's card state and Anki's
//!   `type`/`queue`/`due`/`odue`/`odid`/`left` columns (version- and calendar-aware), and the
//!   schema 11 `.apkg` writer.
//!
//! Nothing here touches the collection: the application layer reads it, this crate converts.

pub mod anki;
pub mod csv;
