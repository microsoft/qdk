// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub mod circuit;
pub mod circuit_qasm;
pub mod circuit_qhdg;
pub mod fir_optimize;
pub mod fir_qhdg;
pub mod graph;
/// Program order over a QHDG graph, shared so the two stages that need it
/// cannot come to disagree about which schedule a graph describes.
mod graph_order;
pub mod graph_pass;
pub mod pass_anchor;
pub mod pass_pipeline;
pub mod qir;
pub mod qsharp;
pub mod rir_optimize;
pub mod rir_qhdg;

/// The measurement behind the FIR stage's rationale. Tests only: it drives both
/// stages side by side, through `fir_optimize` and `qir::fir_to_qir_at` rather
/// than through a second wiring of its own.
#[cfg(test)]
mod stage_yield;
