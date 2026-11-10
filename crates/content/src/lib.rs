//! Bundled content (plan Phase 8, ADR-14): format v2, streaming reader, shared checks and the
//! linter. The importer in `langualinker-application` and `content-lint` use the same checks, so
//! the gate in CI and the gate at install time cannot drift apart.

pub mod archive;
pub mod bundle;
pub mod check;
pub mod format;
pub mod package;
pub mod reader;
pub mod source;

pub use archive::ArchiveSource;
pub use bundle::{Bundle, lint};
pub use check::{Issue, Issues, Mode, Severity};
pub use package::{NotAPackage, Package, PackageSource};
pub use source::{ContentSource, DirSource, StaticSource};
