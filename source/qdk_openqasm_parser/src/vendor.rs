// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Vendored copies of the shared data-structure types used by the parser.
//!
//! These are minimal copies of the corresponding items from the in-repo
//! `qsc_data_structures` (and `index_map`) crates. They are only compiled when
//! the `internal` feature is disabled, allowing the parser crate to build
//! standalone without the rest of the compiler workspace.

pub mod display;
pub mod error;
pub mod index_map;
pub mod source;
pub mod span;
