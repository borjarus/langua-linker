//! Infrastructure adapters: SQLite persistence (Phase 7), backups and restore (Phase 12), diagnostics (logging, crash reports), opt-in usage metrics (Phase 16),
//! the OS secret store, and the system clock and randomness.

pub mod backup;
pub mod diagnostics;
pub mod llm;
pub mod logging;
pub mod ondevice;
pub mod secrets;
pub mod sqlite;
pub mod system;
pub mod telemetry;
pub mod transfer;
