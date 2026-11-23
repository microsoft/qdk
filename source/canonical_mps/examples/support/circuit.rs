// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A circuit: its steps on qubits, each gate as a matrix for the engine,
//! and each projection on a recorded value.

use num_complex::Complex64;

/// One step of a circuit, on qubits.
pub enum Op {
    /// A gate on one or two qubits, its matrix row-major with `qubits[0]`
    /// the most significant.
    Gate {
        qubits: Vec<usize>,
        matrix: Vec<Complex64>,
    },
    /// A projection on the value a recorded shot drew, with the
    /// probability the recording read.
    Project {
        qubit: usize,
        one: bool,
        probability: f64,
    },
}

/// A circuit: its qubits, its steps and the qubits it reads out at the end.
pub struct Circuit {
    pub qubits: usize,
    pub ops: Vec<Op>,
    pub readout: Vec<usize>,
}

impl Circuit {
    pub fn new(qubits: usize) -> Self {
        Self {
            qubits,
            ops: Vec::new(),
            readout: Vec::new(),
        }
    }

    pub fn one(&mut self, qubit: usize, matrix: [Complex64; 4]) {
        self.ops.push(Op::Gate {
            qubits: vec![qubit],
            matrix: matrix.to_vec(),
        });
    }

    pub fn two(&mut self, a: usize, b: usize, matrix: [Complex64; 16]) {
        self.ops.push(Op::Gate {
            qubits: vec![a, b],
            matrix: matrix.to_vec(),
        });
    }

    pub fn two_qubit_gates(&self) -> usize {
        self.ops
            .iter()
            .filter(|op| matches!(op, Op::Gate { qubits, .. } if qubits.len() == 2))
            .count()
    }

    pub fn projections(&self) -> usize {
        self.ops
            .iter()
            .filter(|op| matches!(op, Op::Project { .. }))
            .count()
    }
}
