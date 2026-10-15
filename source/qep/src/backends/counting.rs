// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::QirBackend;

/// Call counts collected by a [`CountingBackend`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CountingBackendCounts {
    pub ccx: usize,
    pub cx: usize,
    pub cy: usize,
    pub cz: usize,
    pub rx: usize,
    pub rxx: usize,
    pub ry: usize,
    pub ryy: usize,
    pub rz: usize,
    pub rzz: usize,
    pub h: usize,
    pub s: usize,
    pub sadj: usize,
    pub sx: usize,
    pub t: usize,
    pub tadj: usize,
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub swap: usize,
    pub mz: usize,
    pub reset: usize,
    pub mresetz: usize,
    pub read_result: usize,
    pub write_result: usize,
    /// The total number of measurements issued by `mz` and `mresetz`.
    pub measurements: usize,
}

/// A backend that counts intrinsic calls and samples measurements without simulation.
#[derive(Debug, Default)]
pub struct CountingBackend {
    counts: CountingBackendCounts,
}

impl CountingBackend {
    /// Returns all intrinsic and measurement counts collected so far.
    #[must_use]
    pub fn counts(&self) -> &CountingBackendCounts {
        &self.counts
    }
}

impl QirBackend for CountingBackend {
    /// Counts and ignores a doubly controlled X gate.
    fn ccx(&mut self, _control1: usize, _control2: usize, _target: usize) -> Result<(), String> {
        self.counts.ccx += 1;
        Ok(())
    }

    /// Counts and ignores a controlled X gate.
    fn cx(&mut self, _control: usize, _target: usize) -> Result<(), String> {
        self.counts.cx += 1;
        Ok(())
    }

    /// Counts and ignores a controlled Y gate.
    fn cy(&mut self, _control: usize, _target: usize) -> Result<(), String> {
        self.counts.cy += 1;
        Ok(())
    }

    /// Counts and ignores a controlled Z gate.
    fn cz(&mut self, _control: usize, _target: usize) -> Result<(), String> {
        self.counts.cz += 1;
        Ok(())
    }

    /// Counts and ignores an X-axis rotation.
    fn rx(&mut self, _angle: f64, _target: usize) -> Result<(), String> {
        self.counts.rx += 1;
        Ok(())
    }

    /// Counts and ignores a two-qubit XX rotation.
    fn rxx(&mut self, _angle: f64, _target1: usize, _target2: usize) -> Result<(), String> {
        self.counts.rxx += 1;
        Ok(())
    }

    /// Counts and ignores a Y-axis rotation.
    fn ry(&mut self, _angle: f64, _target: usize) -> Result<(), String> {
        self.counts.ry += 1;
        Ok(())
    }

    /// Counts and ignores a two-qubit YY rotation.
    fn ryy(&mut self, _angle: f64, _target1: usize, _target2: usize) -> Result<(), String> {
        self.counts.ryy += 1;
        Ok(())
    }

    /// Counts and ignores a Z-axis rotation.
    fn rz(&mut self, _angle: f64, _target: usize) -> Result<(), String> {
        self.counts.rz += 1;
        Ok(())
    }

    /// Counts and ignores a two-qubit ZZ rotation.
    fn rzz(&mut self, _angle: f64, _target1: usize, _target2: usize) -> Result<(), String> {
        self.counts.rzz += 1;
        Ok(())
    }

    /// Counts and ignores a Hadamard gate.
    fn h(&mut self, _target: usize) -> Result<(), String> {
        self.counts.h += 1;
        Ok(())
    }

    /// Counts and ignores an S gate.
    fn s(&mut self, _target: usize) -> Result<(), String> {
        self.counts.s += 1;
        Ok(())
    }

    /// Counts and ignores an adjoint S gate.
    fn sadj(&mut self, _target: usize) -> Result<(), String> {
        self.counts.sadj += 1;
        Ok(())
    }

    /// Counts and ignores a square-root X gate.
    fn sx(&mut self, _target: usize) -> Result<(), String> {
        self.counts.sx += 1;
        Ok(())
    }

    /// Counts and ignores a T gate.
    fn t(&mut self, _target: usize) -> Result<(), String> {
        self.counts.t += 1;
        Ok(())
    }

    /// Counts and ignores an adjoint T gate.
    fn tadj(&mut self, _target: usize) -> Result<(), String> {
        self.counts.tadj += 1;
        Ok(())
    }

    /// Counts and ignores an X gate.
    fn x(&mut self, _target: usize) -> Result<(), String> {
        self.counts.x += 1;
        Ok(())
    }

    /// Counts and ignores a Y gate.
    fn y(&mut self, _target: usize) -> Result<(), String> {
        self.counts.y += 1;
        Ok(())
    }

    /// Counts and ignores a Z gate.
    fn z(&mut self, _target: usize) -> Result<(), String> {
        self.counts.z += 1;
        Ok(())
    }

    /// Counts and ignores a swap gate.
    fn swap(&mut self, _target1: usize, _target2: usize) -> Result<(), String> {
        self.counts.swap += 1;
        Ok(())
    }

    /// Counts a measurement issuance.
    fn mz(&mut self, _target: usize, _result: usize) -> Result<(), String> {
        self.counts.mz += 1;
        self.counts.measurements += 1;
        Ok(())
    }

    /// Counts and ignores a reset operation.
    fn reset(&mut self, _target: usize) -> Result<(), String> {
        self.counts.reset += 1;
        Ok(())
    }

    /// Counts a measurement-and-reset issuance.
    fn mresetz(&mut self, _target: usize, _result: usize) -> Result<(), String> {
        self.counts.mresetz += 1;
        self.counts.measurements += 1;
        Ok(())
    }

    /// Counts a result read and returns a random bit.
    fn read_result(&mut self, _result: usize) -> Result<bool, String> {
        self.counts.read_result += 1;
        Ok(rand::random())
    }
}
