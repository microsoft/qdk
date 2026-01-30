// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{fmt::Display, ops::Deref};

use rustc_hash::FxHashMap;

use crate::{ParetoFrontier2D, ParetoItem2D};

#[derive(Default)]
pub struct EstimationResult {
    qubits: u64,
    runtime: u64,
    error: f64,
    factories: FxHashMap<u64, FactoryResult>,
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

pub struct EstimationCollection(ParetoFrontier2D<EstimationResult>);

impl Deref for EstimationCollection {
    type Target = ParetoFrontier2D<EstimationResult>;

    fn deref(&self) -> &Self::Target {
        &self.0
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
