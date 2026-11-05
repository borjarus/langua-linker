//! Application layer: use cases, ports and the unit of work. Depends on the domain only;
//! infrastructure implements the ports and the Tauri shell wires them together.

pub mod content;
pub mod dashboard;
pub mod error;
pub mod jobs;
pub mod learning;
pub mod ports;
pub mod repositories;
pub mod study;

pub use error::{AppError, AppResult, DetailValue};
