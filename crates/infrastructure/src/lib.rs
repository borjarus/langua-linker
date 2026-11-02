//! Infrastructure adapters: SQLite persistence (Phase 7), diagnostics (logging, crash reports),
//! the OS secret store, and the system clock and randomness.

pub mod diagnostics;
pub mod logging;
pub mod secrets;
pub mod sqlite;
pub mod system;
