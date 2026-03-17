// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
};

use rustc_hash::FxHashMap;

use crate::{ISA, ParetoFrontier2D, ParetoItem2D, Property};

#[derive(Clone, Default)]
pub struct EstimationResult {
    qubits: u64,
    runtime: u64,
    error: f64,
    factories: FxHashMap<u64, FactoryResult>,
    isa: ISA,
    isa_index: Option<usize>,
    trace_index: Option<usize>,
    properties: FxHashMap<u64, Property>,
}

impl EstimationResult {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn qubits(&self) -> u64 {
        self.qubits
    }

    #[must_use]
    pub fn runtime(&self) -> u64 {
        self.runtime
    }

    #[must_use]
    pub fn error(&self) -> f64 {
        self.error
    }

    #[must_use]
    pub fn factories(&self) -> &FxHashMap<u64, FactoryResult> {
        &self.factories
    }

    pub fn set_qubits(&mut self, qubits: u64) {
        self.qubits = qubits;
    }

    pub fn set_runtime(&mut self, runtime: u64) {
        self.runtime = runtime;
    }

    pub fn set_error(&mut self, error: f64) {
        self.error = error;
    }

    /// Adds to the current qubit count and returns the new value.
    pub fn add_qubits(&mut self, qubits: u64) -> u64 {
        self.qubits += qubits;
        self.qubits
    }

    /// Adds to the current runtime and returns the new value.
    pub fn add_runtime(&mut self, runtime: u64) -> u64 {
        self.runtime += runtime;
        self.runtime
    }

    /// Adds to the current error and returns the new value.
    pub fn add_error(&mut self, error: f64) -> f64 {
        self.error += error;
        self.error
    }

    pub fn add_factory_result(&mut self, id: u64, result: FactoryResult) {
        self.factories.insert(id, result);
    }

    pub fn set_isa(&mut self, isa: ISA) {
        self.isa = isa;
    }

    #[must_use]
    pub fn isa(&self) -> &ISA {
        &self.isa
    }

    pub fn set_isa_index(&mut self, index: usize) {
        self.isa_index = Some(index);
    }

    #[must_use]
    pub fn isa_index(&self) -> Option<usize> {
        self.isa_index
    }

    pub fn set_trace_index(&mut self, index: usize) {
        self.trace_index = Some(index);
    }

    #[must_use]
    pub fn trace_index(&self) -> Option<usize> {
        self.trace_index
    }

    pub fn set_property(&mut self, key: u64, value: Property) {
        self.properties.insert(key, value);
    }

    #[must_use]
    pub fn get_property(&self, key: u64) -> Option<&Property> {
        self.properties.get(&key)
    }

    #[must_use]
    pub fn has_property(&self, key: u64) -> bool {
        self.properties.contains_key(&key)
    }

    #[must_use]
    pub fn properties(&self) -> &FxHashMap<u64, Property> {
        &self.properties
    }
}

impl Display for EstimationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Qubits: {}, Runtime: {}, Error: {}",
            self.qubits, self.runtime, self.error
        )?;

        if !self.factories.is_empty() {
            for (id, factory) in &self.factories {
                write!(
                    f,
                    ", {id}: {} runs x {} copies",
                    factory.runs(),
                    factory.copies()
                )?;
            }
        }

        Ok(())
    }
}

impl ParetoItem2D for EstimationResult {
    type Objective1 = u64; // qubits
    type Objective2 = u64; // runtime

    fn objective1(&self) -> Self::Objective1 {
        self.qubits
    }

    fn objective2(&self) -> Self::Objective2 {
        self.runtime
    }
}

/// Lightweight summary of a successful estimation, used to identify
/// post-processing candidates without storing full results.
#[derive(Clone, Copy)]
pub struct ResultSummary {
    pub trace_index: usize,
    pub isa_index: usize,
    pub qubits: u64,
    pub runtime: u64,
}

#[derive(Default)]
pub struct EstimationCollection {
    frontier: ParetoFrontier2D<EstimationResult>,
    /// Lightweight summaries of ALL successful estimates (not just Pareto).
    all_summaries: Vec<ResultSummary>,
    total_jobs: usize,
    successful_estimates: usize,
}

impl EstimationCollection {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn total_jobs(&self) -> usize {
        self.total_jobs
    }

    pub fn set_total_jobs(&mut self, total_jobs: usize) {
        self.total_jobs = total_jobs;
    }

    #[must_use]
    pub fn successful_estimates(&self) -> usize {
        self.successful_estimates
    }

    pub fn set_successful_estimates(&mut self, successful_estimates: usize) {
        self.successful_estimates = successful_estimates;
    }

    pub fn push_summary(&mut self, summary: ResultSummary) {
        self.all_summaries.push(summary);
    }

    #[must_use]
    pub fn all_summaries(&self) -> &[ResultSummary] {
        &self.all_summaries
    }
}

impl Deref for EstimationCollection {
    type Target = ParetoFrontier2D<EstimationResult>;

    fn deref(&self) -> &Self::Target {
        &self.frontier
    }
}

impl DerefMut for EstimationCollection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.frontier
    }
}

#[derive(Clone)]
pub struct FactoryResult {
    copies: u64,
    runs: u64,
    states: u64,
    error_rate: f64,
}

impl FactoryResult {
    #[must_use]
    pub fn new(copies: u64, runs: u64, states: u64, error_rate: f64) -> Self {
        Self {
            copies,
            runs,
            states,
            error_rate,
        }
    }

    #[must_use]
    pub fn copies(&self) -> u64 {
        self.copies
    }

    #[must_use]
    pub fn runs(&self) -> u64 {
        self.runs
    }

    #[must_use]
    pub fn states(&self) -> u64 {
        self.states
    }

    #[must_use]
    pub fn error_rate(&self) -> f64 {
        self.error_rate
    }
}
