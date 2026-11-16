// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`FullStateActionSimulator`]: an action simulator that keeps a state
//! vector, with this crate's CPU full-state simulator.

use super::paulis::{clifford as to_paulimer, duals, letter};
use crate::{
    MeasurementResult, Simulator as _, cpu_full_state_simulator::FullStateSimulator as Cpu,
};
use paulimer::{Clifford as _, CliffordMutable as _, CliffordUnitary, pauli::Pauli as _};
use qodec_runtime_interface::{
    actions::{ActionOp, Generator, Guard, tableau},
    backend::Backend,
    pauli::{Pauli, PauliOp},
};

/// An [`ActionBackend`](qodec_runtime_interface::ActionBackend) that keeps
/// the full state vector, so it runs any action, rotations included.
///
/// It drives the CPU full-state simulator through its gates: a Clifford is
/// decomposed into gates, and a Pauli is measured or rotated about by
/// changing each of its qubits' basis to Z and gathering their parity on one
/// of them with CNOTs. Its qubits are fixed when it is made, and it resets a
/// qubit to `|0⟩` when it is released.
pub struct FullStateActionSimulator {
    sim: Cpu,
    live: Vec<bool>,
}

/// A gate of the CPU simulator, on local qubits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gate {
    H(usize),
    S(usize),
    SAdj(usize),
    Sx(usize),
    SxAdj(usize),
    X(usize),
    Z(usize),
    Cx(usize, usize),
    Swap(usize, usize),
}

impl Gate {
    fn inverse(self) -> Gate {
        match self {
            Gate::S(q) => Gate::SAdj(q),
            Gate::SAdj(q) => Gate::S(q),
            Gate::Sx(q) => Gate::SxAdj(q),
            Gate::SxAdj(q) => Gate::Sx(q),
            g => g,
        }
    }

    /// Left-multiplies `c` by it.
    pub(crate) fn on(self, c: &mut CliffordUnitary) {
        match self {
            Gate::H(q) => c.left_mul_hadamard(q),
            Gate::S(q) => c.left_mul_root_z(q),
            Gate::SAdj(q) => c.left_mul_root_z_inverse(q),
            Gate::Sx(q) => c.left_mul_root_x(q),
            Gate::SxAdj(q) => c.left_mul_root_x_inverse(q),
            Gate::X(q) => c.left_mul_x(q),
            Gate::Z(q) => c.left_mul_z(q),
            Gate::Cx(a, b) => c.left_mul_cx(a, b),
            Gate::Swap(a, b) => c.left_mul_swap(a, b),
        }
    }
}

/// Gates whose product, applied in order, is `c` up to a global phase.
///
/// Elimination, one qubit at a time: gates on the left of `c` turn the image
/// of `X_i` into `X_i`, then that of `Z_i` into `Z_i`, without disturbing the
/// qubits done before. What is left is a Pauli, removed last. `c` is the
/// inverse of all of that, in reverse.
#[allow(clippy::many_single_char_names)]
pub(crate) fn synthesize(c: &CliffordUnitary) -> Vec<Gate> {
    let n = c.num_qubits();
    let mut w = c.clone();
    let mut gates = Vec::new();
    let mut apply = |g: Gate, w: &mut CliffordUnitary| {
        g.on(w);
        gates.push(g);
    };
    for i in 0..n {
        // The image of X_i: X on every qubit it touches, ...
        for q in i..n {
            match letter(&w.image_x(i), q) {
                (false, true) => apply(Gate::H(q), &mut w),
                (true, true) => apply(Gate::SAdj(q), &mut w),
                _ => {}
            }
        }
        // ... with X_i among them, ...
        let p = w.image_x(i);
        if !letter(&p, i).0 {
            let q = (i + 1..n)
                .find(|&q| letter(&p, q).0)
                .expect("the image of X_i is not the identity");
            apply(Gate::Swap(i, q), &mut w);
        }
        // ... then gathered on X_i.
        let p = w.image_x(i);
        for q in i + 1..n {
            if letter(&p, q).0 {
                apply(Gate::Cx(i, q), &mut w);
            }
        }
        // The image of Z_i, which anticommutes with X_i: Z on every other
        // qubit it touches, ...
        for q in i + 1..n {
            match letter(&w.image_z(i), q) {
                (true, false) => apply(Gate::H(q), &mut w),
                (true, true) => {
                    apply(Gate::SAdj(q), &mut w);
                    apply(Gate::H(q), &mut w);
                }
                _ => {}
            }
        }
        // ... gathered on qubit i, which leaves X_i alone, ...
        let p = w.image_z(i);
        for q in i + 1..n {
            if letter(&p, q).1 {
                apply(Gate::Cx(q, i), &mut w);
            }
        }
        // ... and a Y there turned into a Z (up to sign), which also leaves
        // X_i alone.
        if letter(&w.image_z(i), i) == (true, true) {
            apply(Gate::Sx(i), &mut w);
        }
    }
    // The signs.
    for i in 0..n {
        if w.image_x(i).xz_phase_exponent() == 2 {
            apply(Gate::Z(i), &mut w);
        }
        if w.image_z(i).xz_phase_exponent() == 2 {
            apply(Gate::X(i), &mut w);
        }
    }
    debug_assert!(w.is_identity(), "the elimination ends at the identity");
    gates.into_iter().rev().map(Gate::inverse).collect()
}

impl FullStateActionSimulator {
    /// A simulator for at most `qubits` qubits at once, its randomness from
    /// `seed`.
    #[must_use]
    pub fn new(qubits: usize, seed: u32) -> Self {
        Self {
            sim: Cpu::new(qubits, 1, seed, Default::default()),
            live: vec![false; qubits],
        }
    }

    fn check(&self, qubits: impl IntoIterator<Item = usize>) -> Result<(), String> {
        match qubits
            .into_iter()
            .find(|&q| !self.live.get(q).copied().unwrap_or(false))
        {
            Some(q) => Err(format!("qubit {q} is not in use")),
            None => Ok(()),
        }
    }

    fn gate(&mut self, g: Gate, targets: &[usize]) {
        let s = &mut self.sim;
        match g {
            Gate::H(q) => s.h(targets[q]),
            Gate::S(q) => s.s(targets[q]),
            Gate::SAdj(q) => s.s_adj(targets[q]),
            Gate::Sx(q) => s.sx(targets[q]),
            Gate::SxAdj(q) => s.sx_adj(targets[q]),
            Gate::X(q) => s.x(targets[q]),
            Gate::Z(q) => s.z(targets[q]),
            Gate::Cx(a, b) => s.cx(targets[a], targets[b]),
            Gate::Swap(a, b) => s.swap(targets[a], targets[b]),
        }
    }

    fn pauli(&mut self, p: &Pauli) {
        for &(q, op) in p.terms() {
            match op {
                PauliOp::X => self.sim.x(q),
                PauliOp::Y => self.sim.y(q),
                PauliOp::Z => self.sim.z(q),
            }
        }
    }

    /// Turns `p` into `Z` on its last qubit, which it returns: each qubit's
    /// basis to Z, then their parity onto the last one.
    fn gather(&mut self, p: &Pauli) -> usize {
        let terms = p.terms();
        for &(q, op) in terms {
            match op {
                PauliOp::X => self.sim.h(q),
                PauliOp::Y => {
                    self.sim.s_adj(q);
                    self.sim.h(q);
                }
                PauliOp::Z => {}
            }
        }
        let (last, _) = terms[terms.len() - 1];
        for &(q, _) in &terms[..terms.len() - 1] {
            self.sim.cx(q, last);
        }
        last
    }

    /// Undoes [`Self::gather`].
    fn scatter(&mut self, p: &Pauli) {
        let terms = p.terms();
        let (last, _) = terms[terms.len() - 1];
        for &(q, _) in terms[..terms.len() - 1].iter().rev() {
            self.sim.cx(q, last);
        }
        for &(q, op) in terms {
            match op {
                PauliOp::X => self.sim.h(q),
                PauliOp::Y => {
                    self.sim.h(q);
                    self.sim.s(q);
                }
                PauliOp::Z => {}
            }
        }
    }

    /// Measures `p`: `true` for its `-1` eigenvalue.
    fn measure(&mut self, p: &Pauli) -> Result<bool, String> {
        self.check(p.support())?;
        if p.is_identity() {
            return Ok(p.negative);
        }
        let last = self.gather(p);
        self.sim.mz(last, 0);
        let result = self.sim.measurements()[0];
        self.scatter(p);
        match result {
            MeasurementResult::Zero => Ok(p.negative),
            MeasurementResult::One => Ok(!p.negative),
            MeasurementResult::Loss => Err(format!("qubit {last} was lost")),
        }
    }

    fn clifford(&mut self, images: &[(Generator, Pauli)]) -> Result<(), String> {
        let (qubits, gate) = tableau(images)?;
        self.check(qubits.iter().copied())?;
        for g in synthesize(&to_paulimer(&gate)?) {
            self.gate(g, &qubits);
        }
        Ok(())
    }

    fn rotate(&mut self, pauli: &Pauli, angle: f64) -> Result<(), String> {
        self.check(pauli.support())?;
        if !pauli.is_identity() {
            let last = self.gather(pauli);
            // `exp(-iθ/2 Z)`, which turns the other way about a negative
            // Pauli.
            self.sim
                .rz(if pauli.negative { -angle } else { angle }, last);
            self.scatter(pauli);
        }
        Ok(())
    }

    fn stabilize(&mut self, paulis: &[Pauli]) -> Result<(), String> {
        let fixes = duals(paulis)?;
        for (p, fix) in paulis.iter().zip(&fixes) {
            if self.measure(p)? {
                self.pauli(fix);
            }
        }
        Ok(())
    }

    fn run(&mut self, ops: &[ActionOp]) -> Result<Vec<bool>, String> {
        let mut outcomes = Vec::new();
        let holds = |guard: &Option<Guard>, outcomes: &[bool]| {
            guard.as_ref().map_or(Ok(true), |g| g.holds(outcomes))
        };
        for op in ops {
            match op {
                ActionOp::Allocate(q) => match self.live.get_mut(*q) {
                    None => {
                        return Err(format!(
                            "qubit {q} is past the {} qubits this simulator has",
                            self.live.len()
                        ));
                    }
                    Some(true) => return Err(format!("qubit {q} is already in use")),
                    Some(live) => *live = true,
                },
                ActionOp::Release(q) => {
                    self.check([*q])?;
                    self.sim.resetz(*q);
                    self.live[*q] = false;
                }
                ActionOp::Clifford { images, guard } => {
                    if holds(guard, &outcomes)? {
                        self.clifford(images)?;
                    }
                }
                ActionOp::Pauli { pauli, guard } => {
                    if holds(guard, &outcomes)? {
                        self.check(pauli.support())?;
                        self.pauli(pauli);
                    }
                }
                ActionOp::Rotate {
                    pauli,
                    angle,
                    guard,
                } => {
                    if holds(guard, &outcomes)? {
                        self.rotate(pauli, *angle)?;
                    }
                }
                ActionOp::Observe(paulis) => {
                    for p in paulis {
                        let outcome = self.measure(p)?;
                        outcomes.push(outcome);
                    }
                }
                ActionOp::Stabilize { paulis, guard } => {
                    if holds(guard, &outcomes)? {
                        self.stabilize(paulis)?;
                    }
                }
            }
        }
        Ok(outcomes)
    }
}

impl Backend<Vec<ActionOp>, Result<Vec<bool>, String>> for FullStateActionSimulator {
    fn execute(&mut self, ops: Vec<ActionOp>) -> Result<Vec<bool>, String> {
        self.run(&ops)
    }
}
