// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ErrorBudget {
    /// Probability of at least one logical error
    logical: f64,
    /// Probability of at least one faulty magic state distillation
    magic_states: f64,
    /// Probability of at least one failed rotation synthesis
    rotations: f64,
}

impl ErrorBudget {
    #[must_use]
    pub fn new(logical: f64, magic_states: f64, rotations: f64) -> Self {
        Self {
            logical,
            magic_states,
            rotations,
        }
    }

    #[must_use]
    pub fn from_uniform(total_error: f64) -> Self {
        Self {
            logical: total_error / 3.0,
            magic_states: total_error / 3.0,
            rotations: total_error / 3.0,
        }
    }

    /// Get the error budget's plogical.
    #[must_use]
    pub fn logical(&self) -> f64 {
        self.logical
    }

    pub fn set_logical(&mut self, logical: f64) {
        self.logical = logical;
    }

    /// Get the error budget's tstates.
    #[must_use]
    pub fn magic_states(&self) -> f64 {
        self.magic_states
    }

    pub fn set_magic_states(&mut self, magic_states: f64) {
        self.magic_states = magic_states;
    }

    /// Get the error budget's rotations.
    #[must_use]
    pub fn rotations(&self) -> f64 {
        self.rotations
    }

    pub fn set_rotations(&mut self, rotations: f64) {
        self.rotations = rotations;
    }
}

#[derive(Default, Copy, Clone)]
pub enum ErrorBudgetStrategy {
    #[default]
    Static,
    PruneLogicalAndRotations,
}
