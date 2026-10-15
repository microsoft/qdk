// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{MessageIn, MessageOut, QirInstruction, QuantumBackend, QuantumBackendEndpoint};

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

impl QuantumBackend for CountingBackend {
    /// Counts streamed instructions and returns random measurement results.
    async fn run(&mut self, mut endpoint: QuantumBackendEndpoint) {
        while let Some(MessageIn::QirInstruction(instruction)) =
            endpoint.instructions_receiver.recv().await
        {
            match instruction {
                QirInstruction::ccx(..) => self.counts.ccx += 1,
                QirInstruction::cx(..) => self.counts.cx += 1,
                QirInstruction::cy(..) => self.counts.cy += 1,
                QirInstruction::cz(..) => self.counts.cz += 1,
                QirInstruction::rx(..) => self.counts.rx += 1,
                QirInstruction::rxx(..) => self.counts.rxx += 1,
                QirInstruction::ry(..) => self.counts.ry += 1,
                QirInstruction::ryy(..) => self.counts.ryy += 1,
                QirInstruction::rz(..) => self.counts.rz += 1,
                QirInstruction::rzz(..) => self.counts.rzz += 1,
                QirInstruction::h(..) => self.counts.h += 1,
                QirInstruction::s(..) => self.counts.s += 1,
                QirInstruction::sadj(..) => self.counts.sadj += 1,
                QirInstruction::sx(..) => self.counts.sx += 1,
                QirInstruction::t(..) => self.counts.t += 1,
                QirInstruction::tadj(..) => self.counts.tadj += 1,
                QirInstruction::x(..) => self.counts.x += 1,
                QirInstruction::y(..) => self.counts.y += 1,
                QirInstruction::z(..) => self.counts.z += 1,
                QirInstruction::swap(..) => self.counts.swap += 1,
                QirInstruction::reset(..) => self.counts.reset += 1,
                QirInstruction::mz(..) => {
                    self.counts.mz += 1;
                    self.counts.measurements += 1;
                }
                QirInstruction::mresetz(..) => {
                    self.counts.mresetz += 1;
                    self.counts.measurements += 1;
                }
                QirInstruction::read_result(..) => {
                    self.counts.read_result += 1;
                    if endpoint
                        .results_sender
                        .send(MessageOut::MeasurementResult(rand::random()))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
        }
    }
}
