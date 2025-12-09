// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// When running `build.py` on the repo, clippy fails in this module with
// `clippy::large_stack_arrays`. Note that the `build.py` script runs the command
// `cargo clippy --all-targets --all-features -- -D warnings`. Just running
// `cargo clippy` won't trigger the failure. If you want to reproduce the failure
// with the minimal command possible, you can run `cargo clippy --test -- -D warnings`.
//
// We tried to track down the error, but it is non-deterministic. Our assumpution
// is that clippy is running out of stack memory because of how many and how large
// the static strings in the test modules are.
//
// Decision: Based on this, we decided to disable the `clippy::large_stack_arrays` lint.
#![allow(clippy::large_stack_arrays)]

mod convert;
pub mod display_utils;
pub mod error;
pub mod io;
mod keyword;
mod lex;
pub mod parser;
pub mod semantic;
pub mod stdlib;

#[cfg(test)]
pub(crate) mod tests;
