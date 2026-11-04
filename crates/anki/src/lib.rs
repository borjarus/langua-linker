//! Anki `.apkg` → LanguaLinker import package converter (plan Phase 8, ADR-14).
//!
//! A desktop tool: the app never links this crate (no zstd/ZIP code on the phones). The output is
//! an import package (format v2, `source: "import"`) that the Transfer screen imports, with the
//! Anki scheduling state and review history.

pub mod collection;
pub mod convert;
pub mod html;

pub use collection::{Collection, ReadError, read_apkg};
pub use convert::{Conversion, FieldRef, Options, Report, convert};
