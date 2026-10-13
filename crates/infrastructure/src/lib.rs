//! Infrastructure adapters. Domain persistence arrives after GUI acceptance (Phase 7);
//! diagnostics (logging, crash reports) and the OS secret store are part of the Phase 1 shell.

pub mod logging;
pub mod secrets;
