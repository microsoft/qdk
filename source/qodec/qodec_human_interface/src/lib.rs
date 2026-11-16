// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This is throw-away code! Its only purpose is to serve as an interactive
//! tool to aid development and reviews.

//! The qodec workbench as a library: the gadget analysis, the text model of a
//! qodec, the traced runtime, and the interface itself, so other tools (such
//! as `qodec-explore`) can build on them and embed the workbench.

pub mod analysis;
pub mod app;
pub mod bundle;
pub mod circuit;
pub mod codes;
pub mod faults;
pub mod gf2;
pub mod shot;
pub mod stabilizer;
pub mod state;
pub mod syntax;
pub mod trace;
pub mod ui;
pub mod views;
pub mod vocab;
pub mod workbench;

#[cfg(test)]
mod tests;
