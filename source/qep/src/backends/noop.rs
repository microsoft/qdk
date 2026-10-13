// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::QirBackend;
use rand::{RngExt, SeedableRng, rngs::StdRng};
use rustc_hash::FxHashMap;

/// A backend that ignores gates and samples independent, uniformly random measurements.
///
/// This backend does not simulate quantum state. Reset is also a no-op, so it
/// does not affect subsequent measurements. Each measurement immediately stores
/// a fresh random bit; reads return that stored bit without resampling.
/// Reading a result slot that has not been measured or written is an error.
#[derive(Debug)]
pub struct NoopBackend {
    rng: StdRng,
    results: FxHashMap<usize, bool>,
}

impl NoopBackend {
    /// Creates a backend with deterministic randomness for tests.
    #[cfg(test)]
    pub(crate) fn with_rng(rng: StdRng) -> Self {
        Self {
            rng,
            results: FxHashMap::default(),
        }
    }
}

impl Default for NoopBackend {
    /// Creates a backend with fresh randomness and no result values.
    fn default() -> Self {
        Self {
            rng: StdRng::from_rng(&mut rand::rng()),
            results: FxHashMap::default(),
        }
    }
}

impl QirBackend for NoopBackend {
    /// Ignores a doubly controlled X gate.
    fn ccx(&mut self, _control1: usize, _control2: usize, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores a controlled X gate.
    fn cx(&mut self, _control: usize, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores a controlled Y gate.
    fn cy(&mut self, _control: usize, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores a controlled Z gate.
    fn cz(&mut self, _control: usize, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores an X-axis rotation.
    fn rx(&mut self, _angle: f64, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores a two-qubit XX rotation.
    fn rxx(&mut self, _angle: f64, _target1: usize, _target2: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores a Y-axis rotation.
    fn ry(&mut self, _angle: f64, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores a two-qubit YY rotation.
    fn ryy(&mut self, _angle: f64, _target1: usize, _target2: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores a Z-axis rotation.
    fn rz(&mut self, _angle: f64, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores a two-qubit ZZ rotation.
    fn rzz(&mut self, _angle: f64, _target1: usize, _target2: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores a Hadamard gate.
    fn h(&mut self, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores an S gate.
    fn s(&mut self, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores an adjoint S gate.
    fn sadj(&mut self, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores a square-root X gate.
    fn sx(&mut self, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores a T gate.
    fn t(&mut self, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores an adjoint T gate.
    fn tadj(&mut self, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores an X gate.
    fn x(&mut self, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores a Y gate.
    fn y(&mut self, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores a Z gate.
    fn z(&mut self, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Ignores a swap gate.
    fn swap(&mut self, _target1: usize, _target2: usize) -> Result<(), String> {
        Ok(())
    }

    /// Samples a random bit into the requested result slot.
    fn mz(&mut self, _target: usize, result: usize) -> Result<(), String> {
        self.results.insert(result, self.rng.random_bool(0.5));
        Ok(())
    }

    /// Ignores a reset operation.
    fn reset(&mut self, _target: usize) -> Result<(), String> {
        Ok(())
    }

    /// Samples a random bit while ignoring the reset operation.
    fn mresetz(&mut self, target: usize, result: usize) -> Result<(), String> {
        self.mz(target, result)
    }

    /// Returns the value stored in a previously written result slot.
    fn read_result(&mut self, result: usize) -> Result<bool, String> {
        self.results
            .get(&result)
            .copied()
            .ok_or_else(|| format!("Result ID {result} has not been written"))
    }

    /// Stores an explicit classical value in a result slot.
    fn write_result(&mut self, result: usize, value: bool) -> Result<(), String> {
        self.results.insert(result, value);
        Ok(())
    }
}
