//! rarma-server: the launcher's logic as a plain blocking library.
//!
//! Blocking on purpose. The work here is filesystem syscalls and a handful of
//! one-shot HTTP requests, so there is nothing for an async runtime to
//! overlap. Staying synchronous keeps the crate callable from a test, a
//! terminal binary, or a Tauri command wrapped in `spawn_blocking`, without
//! forcing any of them to bring a runtime along.
//!
//! Module layout mirrors the Node implementation in ../server one to one, so
//! the two can be compared directly while the port proceeds.

/// Steam Workshop app id for Arma 3.
pub const ARMA_APP_ID: &str = "107410";

pub mod api;
pub mod cache;
pub mod config;
pub mod launch;
pub mod launcher;
pub mod mods;
pub mod profiles;
pub mod resolve;
pub mod scrape;
pub mod session;
pub mod steam;
