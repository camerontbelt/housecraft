//! Chief Architect file interoperability.
//!
//! The `.plan`, `.layout` and `.calib(z)` formats are undocumented. This crate
//! starts with the tools used to document them from sample files:
//!
//! - [`probe`] identifies the container (ZIP, SQLite, zlib streams, XML...),
//!   maps entropy, finds compressed streams and pulls out strings;
//! - [`diff`] compares two files (or their decompressed payloads) so that a
//!   one-property change in Chief Architect points at the bytes that store it.
//!
//! Readers are written from the spec in `docs/formats/`, never from
//! decompiled code (see `docs/formats/README.md`).

pub mod diff;
pub mod probe;
