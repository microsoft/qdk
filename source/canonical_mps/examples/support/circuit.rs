// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A circuit: its steps on qubits, each gate as a matrix for the engine
//! and, when it has one, as the sparse simulator's native operations.

use super::C;
use qdk_simulators::SparseStateSim;

/// An operation of the sparse simulator, which runs a circuit only when
/// every gate has its native form.
#[derive(Clone, Copy, Debug)]
pub enum Native {
    X(usize),
    Y(usize),
    Z(usize),
    H(usize),
    S(usize),
    SAdj(usize),
    T(usize),
    TAdj(usize),
    Rx(f64, usize),
    Ry(f64, usize),
    Rz(f64, usize),
    Cx(usize, usize),
    Cy(usize, usize),
    Cz(usize, usize),
    /// A phase `e^{iθ}` on the target, controlled by the first qubit.
    ControlledPhase(usize, usize, f64),
}

impl Native {
    pub fn on(self, sim: &mut SparseStateSim) {
        match self {
            Self::X(q) => sim.x(q),
            Self::Y(q) => sim.y(q),
            Self::Z(q) => sim.z(q),
            Self::H(q) => sim.h(q),
            Self::S(q) => sim.s(q),
            Self::SAdj(q) => sim.sadj(q),
            Self::T(q) => sim.t(q),
            Self::TAdj(q) => sim.tadj(q),
            Self::Rx(theta, q) => sim.rx(theta, q),
            Self::Ry(theta, q) => sim.ry(theta, q),
            Self::Rz(theta, q) => sim.rz(theta, q),
            Self::Cx(a, b) => sim.mcx(&[a], b),
            Self::Cy(a, b) => sim.mcy(&[a], b),
            Self::Cz(a, b) => sim.mcz(&[a], b),
            Self::ControlledPhase(a, b, theta) => {
                sim.mcphase(&[a], C::from_polar(1.0, theta), b);
            }
        }
    }
}

/// One step of a circuit, on qubits.
pub enum Op {
    /// A gate on one or two qubits, its matrix row-major with `qubits[0]`
    /// the most significant; `natives` is the same gate on the sparse
    /// simulator, when it has one.
    Gate {
        qubits: Vec<usize>,
        matrix: Vec<C>,
        natives: Option<Vec<Native>>,
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

    pub fn one(&mut self, qubit: usize, matrix: [C; 4], natives: Option<Vec<Native>>) {
        self.ops.push(Op::Gate {
            qubits: vec![qubit],
            matrix: matrix.to_vec(),
            natives,
        });
    }

    pub fn two(&mut self, a: usize, b: usize, matrix: [C; 16], natives: Option<Vec<Native>>) {
        self.ops.push(Op::Gate {
            qubits: vec![a, b],
            matrix: matrix.to_vec(),
            natives,
        });
    }

    /// Whether the sparse simulator runs the circuit.
    pub fn is_native(&self) -> bool {
        self.ops.iter().all(|op| match op {
            Op::Gate { natives, .. } => natives.is_some(),
            Op::Project { .. } => true,
        })
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
