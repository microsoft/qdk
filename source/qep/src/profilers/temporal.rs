// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Profiler, ProfilerResult, TemporalProfiles, TotalCounts};
use crate::interface::{BatchResult, QuantumBackend, QuantumBackendInstruction};

/// Prototype for a profiler that models how quantum instruction are processed on multiple
/// components, taking into account:
///   * Finite gate execution times.
///   * Finite rate of magic state generation.
///   * Requiremnt that some gates requre magic states.
///
/// It works by:
///   * Maintaining a "timer" - intgere that counts nanoseconds since execution start.
///   * Maintaining event queue, where each event has a timestamp. Example events are:
///      * Instruction received.
///      * Gate application started (on specific qubit).
///      * Gate application finished (on specific qubit).
///      * T-state production started (on specific factory).
///      * T-state production finished (on specific factory).
///
/// It then aggregates all the events in windows of specified duration (by default, 1ms).
/// For each window we remember:
///     * Number of instructions executed (of each type).
///     * Component-specific metrics. For example, for a T-factory - number of T-states produced and
///         consumed.
///
/// This is a prorotype which doens't model any specific quantum computer.
/// It's a proof of concept that we can model pipeline of resources (e.g. T-state production ->
/// distillation -> injection) and identify bottlenecks.
/// It's inspired by Pinnacle architecture (https://arxiv.org/pdf/2602.11457) in a sense that
/// it has T-factories (similar to Pinnacle's "magic engines") as separate "component".
///
/// Once we implement the infrastracture for temoral modelling (timer, queue, event loop), we will
/// refactor that to a common helper which can be used to build temporal profilers modelling
/// different specific quantum computers.
pub struct TemporalProfiler {
    aggregation_window_nanos: usize,
}

impl QuantumBackend for TemporalProfiler {
    fn execute_batch(&mut self, instructions: &[QuantumBackendInstruction]) -> BatchResult {
        todo!()
    }
}

impl Profiler for TemporalProfiler {
    fn get_result(&self) -> ProfilerResult {
        ProfilerResult {
            totals: TotalCounts::default(),
            profiles: TemporalProfiles::default(),
            infos: vec![],
        }
    }
}
