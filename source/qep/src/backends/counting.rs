// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::NoopBackend;
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
    backend: NoopBackend,
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
    fn ccx(&mut self, control1: usize, control2: usize, target: usize) -> Result<(), String> {
        self.counts.ccx += 1;
        self.backend.ccx(control1, control2, target)
    }

    /// Counts and ignores a controlled X gate.
    fn cx(&mut self, control: usize, target: usize) -> Result<(), String> {
        self.counts.cx += 1;
        self.backend.cx(control, target)
    }

    /// Counts and ignores a controlled Y gate.
    fn cy(&mut self, control: usize, target: usize) -> Result<(), String> {
        self.counts.cy += 1;
        self.backend.cy(control, target)
    }

    /// Counts and ignores a controlled Z gate.
    fn cz(&mut self, control: usize, target: usize) -> Result<(), String> {
        self.counts.cz += 1;
        self.backend.cz(control, target)
    }

    /// Counts and ignores an X-axis rotation.
    fn rx(&mut self, angle: f64, target: usize) -> Result<(), String> {
        self.counts.rx += 1;
        self.backend.rx(angle, target)
    }

    /// Counts and ignores a two-qubit XX rotation.
    fn rxx(&mut self, angle: f64, target1: usize, target2: usize) -> Result<(), String> {
        self.counts.rxx += 1;
        self.backend.rxx(angle, target1, target2)
    }

    /// Counts and ignores a Y-axis rotation.
    fn ry(&mut self, angle: f64, target: usize) -> Result<(), String> {
        self.counts.ry += 1;
        self.backend.ry(angle, target)
    }

    /// Counts and ignores a two-qubit YY rotation.
    fn ryy(&mut self, angle: f64, target1: usize, target2: usize) -> Result<(), String> {
        self.counts.ryy += 1;
        self.backend.ryy(angle, target1, target2)
    }

    /// Counts and ignores a Z-axis rotation.
    fn rz(&mut self, angle: f64, target: usize) -> Result<(), String> {
        self.counts.rz += 1;
        self.backend.rz(angle, target)
    }

    /// Counts and ignores a two-qubit ZZ rotation.
    fn rzz(&mut self, angle: f64, target1: usize, target2: usize) -> Result<(), String> {
        self.counts.rzz += 1;
        self.backend.rzz(angle, target1, target2)
    }

    /// Counts and ignores a Hadamard gate.
    fn h(&mut self, target: usize) -> Result<(), String> {
        self.counts.h += 1;
        self.backend.h(target)
    }

    /// Counts and ignores an S gate.
    fn s(&mut self, target: usize) -> Result<(), String> {
        self.counts.s += 1;
        self.backend.s(target)
    }

    /// Counts and ignores an adjoint S gate.
    fn sadj(&mut self, target: usize) -> Result<(), String> {
        self.counts.sadj += 1;
        self.backend.sadj(target)
    }

    /// Counts and ignores a square-root X gate.
    fn sx(&mut self, target: usize) -> Result<(), String> {
        self.counts.sx += 1;
        self.backend.sx(target)
    }

    /// Counts and ignores a T gate.
    fn t(&mut self, target: usize) -> Result<(), String> {
        self.counts.t += 1;
        self.backend.t(target)
    }

    /// Counts and ignores an adjoint T gate.
    fn tadj(&mut self, target: usize) -> Result<(), String> {
        self.counts.tadj += 1;
        self.backend.tadj(target)
    }

    /// Counts and ignores an X gate.
    fn x(&mut self, target: usize) -> Result<(), String> {
        self.counts.x += 1;
        self.backend.x(target)
    }

    /// Counts and ignores a Y gate.
    fn y(&mut self, target: usize) -> Result<(), String> {
        self.counts.y += 1;
        self.backend.y(target)
    }

    /// Counts and ignores a Z gate.
    fn z(&mut self, target: usize) -> Result<(), String> {
        self.counts.z += 1;
        self.backend.z(target)
    }

    /// Counts and ignores a swap gate.
    fn swap(&mut self, target1: usize, target2: usize) -> Result<(), String> {
        self.counts.swap += 1;
        self.backend.swap(target1, target2)
    }

    /// Counts and samples a random measurement result.
    fn mz(&mut self, target: usize, result: usize) -> Result<(), String> {
        self.counts.mz += 1;
        self.counts.measurements += 1;
        self.backend.mz(target, result)
    }

    /// Counts and ignores a reset operation.
    fn reset(&mut self, target: usize) -> Result<(), String> {
        self.counts.reset += 1;
        self.backend.reset(target)
    }

    /// Counts and samples a random measurement while ignoring the reset.
    fn mresetz(&mut self, target: usize, result: usize) -> Result<(), String> {
        self.counts.mresetz += 1;
        self.counts.measurements += 1;
        self.backend.mresetz(target, result)
    }

    /// Counts and reads a previously written result.
    fn read_result(&mut self, result: usize) -> Result<bool, String> {
        self.counts.read_result += 1;
        self.backend.read_result(result)
    }

    /// Counts and stores an explicit classical result.
    fn write_result(&mut self, result: usize, value: bool) -> Result<(), String> {
        self.counts.write_result += 1;
        self.backend.write_result(result, value)
    }
}
