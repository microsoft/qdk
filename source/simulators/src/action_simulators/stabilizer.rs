// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`StabilizerActionSimulator`]: an action simulator that keeps a stabilizer
//! state as a `paulimer` Clifford frame.

use super::paulis::{clifford, duals, letter, sparse};
use paulimer::{
    Clifford as _, CliffordMutable as _, CliffordUnitary,
    pauli::{Pauli as _, PauliBinaryOps as _, PauliMutable as _},
};
use qodec_runtime_interface::{
    actions::{ActionOp, Generator, Guard, tableau},
    backend::Backend,
    pauli::Pauli,
};
use rand::{RngExt as _, SeedableRng as _, rngs::StdRng};
use std::f64::consts::FRAC_PI_2;

/// An [`ActionBackend`](qodec_runtime_interface::ActionBackend) that keeps a
/// stabilizer state as a Clifford frame `C`, the state being `C|0…0⟩`.
///
/// It runs Paulis, Cliffords, measurements and preparations of any Paulis,
/// and rotations by multiples of π/2, which are Cliffords. It rejects any
/// other rotation. It grows as qubits are allocated, and resets a qubit to
/// `|0⟩` when it is released.
pub struct StabilizerActionSimulator {
    frame: CliffordUnitary,
    live: Vec<bool>,
    rng: StdRng,
}

impl StabilizerActionSimulator {
    /// A simulator with no qubit, its randomness from `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            frame: CliffordUnitary::identity(0),
            live: Vec::new(),
            rng: StdRng::seed_from_u64(seed),
        }
    }

    fn check(&self, p: &Pauli) -> Result<(), String> {
        match p
            .support()
            .find(|&q| !self.live.get(q).copied().unwrap_or(false))
        {
            Some(q) => Err(format!("qubit {q} is not in use")),
            None => Ok(()),
        }
    }

    /// Measures `p`: `true` for its `-1` eigenvalue.
    fn measure(&mut self, p: &Pauli) -> Result<bool, String> {
        self.check(p)?;
        let observable = sparse(p);
        let preimage = self.frame.preimage(&observable);
        let Some(position) = (0..self.live.len()).find(|&q| letter(&preimage, q).0) else {
            // `C† P C` is diagonal: `|0…0⟩` is one of its eigenstates.
            return Ok(preimage.xz_phase_exponent() == 2);
        };
        // Random: project onto the `+1` eigenspace, then flip with a Pauli
        // that anticommutes with `p` if the outcome is `-1` (as
        // `qdk_simulators`' branching state does).
        let outcome: bool = self.rng.random();
        let hint = self.frame.image_z(position);
        let mut update = observable;
        update.mul_assign_right(&hint);
        update.add_assign_phase_exp(3);
        self.frame.left_mul_pauli_exp(&update);
        if outcome {
            self.frame.left_mul_pauli(&hint);
        }
        Ok(outcome)
    }

    fn clifford(&mut self, images: &[(Generator, Pauli)]) -> Result<(), String> {
        let (qubits, gate) = tableau(images)?;
        if let Some(&q) = qubits
            .iter()
            .find(|&&q| !self.live.get(q).copied().unwrap_or(false))
        {
            return Err(format!("qubit {q} is not in use"));
        }
        self.frame.left_mul_clifford(&clifford(&gate)?, &qubits);
        Ok(())
    }

    fn rotate(&mut self, pauli: &Pauli, angle: f64) -> Result<(), String> {
        self.check(pauli)?;
        let turns = angle / FRAC_PI_2;
        if (turns - turns.round()).abs() > 1e-9 {
            return Err(format!(
                "a stabilizer simulator can't rotate about `{pauli}` by {angle}: only by multiples of π/2"
            ));
        }
        // `exp(-i k π/4 P)`, for the sign-free `P`: a negative `P`
        // turns the other way.
        #[allow(clippy::cast_possible_truncation)]
        let mut k = (turns.round() as i64).rem_euclid(4);
        if pauli.negative {
            k = (4 - k) % 4;
        }
        let unsigned = Pauli::new(false, pauli.terms().to_vec())?;
        let p = sparse(&unsigned);
        // `paulimer` turns by `exp(+iπ/4 P)`; up to global phases, a
        // quarter turn is that and `P`, a half turn `P`, and three
        // quarters that alone.
        if k % 2 == 1 {
            self.frame.left_mul_pauli_exp(&p);
        }
        if k == 1 || k == 2 {
            self.frame.left_mul_pauli(&p);
        }
        Ok(())
    }

    fn stabilize(&mut self, paulis: &[Pauli]) -> Result<(), String> {
        let fixes = duals(paulis)?;
        for (p, fix) in paulis.iter().zip(&fixes) {
            if self.measure(p)? {
                self.frame.left_mul_pauli(&sparse(fix));
            }
        }
        Ok(())
    }

    fn allocate(&mut self, q: usize) -> Result<(), String> {
        if q >= self.live.len() {
            self.frame.resize(q + 1);
            self.live.resize(q + 1, false);
        }
        if self.live[q] {
            return Err(format!("qubit {q} is already in use"));
        }
        self.live[q] = true;
        Ok(())
    }

    fn release(&mut self, q: usize) -> Result<(), String> {
        let z = Pauli::parse(&format!("Z_{q}"))?;
        if self.measure(&z)? {
            self.frame.left_mul_x(q);
        }
        self.live[q] = false;
        Ok(())
    }

    fn run(&mut self, ops: &[ActionOp]) -> Result<Vec<bool>, String> {
        let mut outcomes = Vec::new();
        let holds = |guard: &Option<Guard>, outcomes: &[bool]| {
            guard.as_ref().map_or(Ok(true), |g| g.holds(outcomes))
        };
        for op in ops {
            match op {
                ActionOp::Allocate(q) => self.allocate(*q)?,
                ActionOp::Release(q) => self.release(*q)?,
                ActionOp::Clifford { images, guard } => {
                    if holds(guard, &outcomes)? {
                        self.clifford(images)?;
                    }
                }
                ActionOp::Pauli { pauli, guard } => {
                    if holds(guard, &outcomes)? {
                        self.check(pauli)?;
                        self.frame.left_mul_pauli(&sparse(pauli));
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

impl Backend<Vec<ActionOp>, Result<Vec<bool>, String>> for StabilizerActionSimulator {
    fn execute(&mut self, ops: Vec<ActionOp>) -> Result<Vec<bool>, String> {
        self.run(&ops)
    }
}
