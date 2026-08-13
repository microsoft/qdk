// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Vendored copies of the shared data-structure types used by the parser.
//!
//! These let the crate build standalone, without the rest of the compiler
//! workspace. Every module except [`error`] is a verbatim copy of its origin in
//! the in-repo `qsc_data_structures` and `index_map` crates, so that syncing a
//! change from the origin is a file copy and detecting drift is a file
//! comparison. Each file's origin, and the divergence for [`error`], is
//! recorded in `vendor-sync/manifest.json`.
//!
//! This module is private. What the crate publishes is decided by the wrapper
//! modules that re-export from here, [`crate::span`] and [`crate::source`],
//! which name individual items rather than re-exporting a whole module. An item
//! that no wrapper names stays internal to the crate.

// A verbatim copy carries the origin's whole surface, including items this
// crate has no use for. That unused remainder is the price of copying instead
// of trimming, and trimming is what would turn each sync back into a manual
// port.
#![allow(dead_code)]

pub mod display;
pub mod error;
pub mod index_map;
pub mod source;
pub mod span;
