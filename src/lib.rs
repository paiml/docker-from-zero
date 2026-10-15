//! imgscope: read-only inspector for `docker save` / OCI image tarballs.
//! The core works on bytes only: no daemon, no network.

pub mod cli;
pub mod diff;
pub mod image;

pub use cli::run;
