// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What flows below the bottom ISA when a qodec is simulated: each bottom
//! call's action, step by step, on numbered qubits.
//!
//! A layer above (such as `qodec_runtime_pieces::ActionTranslator`) knows the
//! bottom ISA: its block types, its instructions and their actions, and which
//! qubits each live block holds. It answers a batch of bottom calls with one
//! batch of [`ActionOp`]s. An [`ActionBackend`] below knows none of that: it
//! only runs the steps, and answers with the outcomes they measure.
//!
//! ```text
//!   │ Vec<Call>   (bottom ISA)         ▲ Result<Vec<BasicReadout>, String>
//!  ┌▼──────────────────────────────────┴┐
//!  │ a translator (a Layer)             │
//!  └┬──────────────────────────────────▲┘
//!   │ Vec<ActionOp>                    │ Result<Vec<bool>, String>
//!  ┌▼──────────────────────────────────┴┐
//!  │ an ActionBackend: a simulator      │
//!  └────────────────────────────────────┘
//! ```
//!
//! The steps are the spec's five [action steps](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/instruction-set.md#actions),
//! one variant each, with every name resolved: qubits are numbers, Pauli
//! parameters are Paulis, angles are numbers, and a guard's `bit` parameters
//! are folded into it. Two more operations make a qubit's lifecycle explicit.

use super::{
    backend::Backend,
    pauli::{Clifford, DensePauli, Pauli},
};

/// A qubit of an action backend. The layer above numbers them, and reuses a
/// number once it has released the qubit.
pub type Qubit = usize;

/// One operation for an action backend: a qubit's lifecycle, or one of the
/// spec's five [action steps](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/instruction-set.md#actions),
/// in the spec's order, on backend qubits.
///
/// The steps say what the spec's say, with every name resolved: their
/// qubits are backend qubits in use, and a guard reads the batch's outcomes
/// (see [`Guard`]). They are guarded where the spec lets them be: every step
/// but `observe`.
///
/// The lifecycle is not a step. In the spec it is implicit in an
/// instruction's boundary: indices past its inputs are fresh, an
/// unconditional `stabilize` introduces temporaries, and whatever is absent
/// from its outputs is traced out at its end. Here it is explicit, since a
/// backend sees qubits, not instructions.
#[derive(Clone, Debug, PartialEq)]
pub enum ActionOp {
    // ── Lifecycle ──
    /// Brings `qubit` into use, in `|0⟩`. It must not be in use.
    Allocate(Qubit),
    /// Ends the use of `qubit`: it is traced out. It must be in use.
    Release(Qubit),

    // ── Steps ──
    /// `clifford`: applies the Clifford that maps each listed generator to
    /// its image, and every other generator to itself.
    Clifford {
        /// Generators and their images.
        images: Vec<(Generator, Pauli)>,
        /// When it applies; always if `None`.
        guard: Option<Guard>,
    },
    /// `pauli`: applies a Pauli. Its sign is a global phase.
    Pauli {
        /// The Pauli.
        pauli: Pauli,
        /// When it applies; always if `None`.
        guard: Option<Guard>,
    },
    /// `observe`: measures each Pauli in turn. Each outcome, `true` for the
    /// `-1` eigenvalue of the Pauli with its sign, is appended to the batch's
    /// outcomes. Never guarded: the number of outcomes is fixed.
    Observe(Vec<Pauli>),
    /// `rotate`: applies `exp(-i angle/2 P)`.
    Rotate {
        /// The Pauli `P`.
        pauli: Pauli,
        /// The angle, in radians.
        angle: f64,
        /// When it applies; always if `None`.
        guard: Option<Guard>,
    },
    /// `stabilize`: forces the state into the `+1` eigenspace of each Pauli,
    /// with its sign, in turn. The Paulis commute and are independent.
    Stabilize {
        /// The Paulis.
        paulis: Vec<Pauli>,
        /// When it applies; always if `None`.
        guard: Option<Guard>,
    },
}

/// A Clifford generator, as a `clifford` step lists one: `X_q` or `Z_q`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Generator {
    /// `X` on a qubit.
    X(Qubit),
    /// `Z` on a qubit.
    Z(Qubit),
}

impl Generator {
    /// Its qubit.
    #[must_use]
    pub fn qubit(self) -> Qubit {
        match self {
            Generator::X(q) | Generator::Z(q) => q,
        }
    }
}

/// The Clifford of a `clifford` step's `images` as a tableau on the qubits
/// it touches, which it also returns, in increasing order: qubit `i` of the
/// tableau is the `i`-th of them.
pub fn tableau(images: &[(Generator, Pauli)]) -> Result<(Vec<Qubit>, Clifford), String> {
    let mut qubits: Vec<Qubit> = images
        .iter()
        .flat_map(|(g, image)| std::iter::once(g.qubit()).chain(image.support()))
        .collect();
    qubits.sort_unstable();
    qubits.dedup();
    let local = |q: Qubit| qubits.binary_search(&q).unwrap_or_default();
    let mut gate = Clifford::identity(qubits.len());
    for (g, image) in images {
        let image = Pauli::new(
            image.negative,
            image
                .terms()
                .iter()
                .map(|&(q, op)| (local(q), op))
                .collect(),
        )?;
        let image = DensePauli::from_pauli(&image, qubits.len())?;
        match *g {
            Generator::X(q) => gate.x_images[local(q)] = image,
            Generator::Z(q) => gate.z_images[local(q)] = image,
        }
    }
    Ok((qubits, gate))
}

/// When a guarded operation applies: the XOR parity of some outcomes the
/// batch measured earlier differs from `invert`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guard {
    /// Indices into the batch's outcomes, each measured before the
    /// operation.
    pub outcomes: Vec<usize>,
    /// Whether it applies when their parity is 0 rather than 1.
    pub invert: bool,
}

impl Guard {
    /// Whether it holds, given the outcomes measured so far. Fails if it
    /// reads an outcome not measured yet.
    pub fn holds(&self, outcomes: &[bool]) -> Result<bool, String> {
        let mut parity = false;
        for &i in &self.outcomes {
            parity ^= outcomes
                .get(i)
                .copied()
                .ok_or_else(|| format!("a guard reads outcome {i}, which is not measured yet"))?;
        }
        Ok(parity != self.invert)
    }
}

/// What runs a batch of [`ActionOp`]s: it answers with every outcome its
/// `Observe` operations measured, in order, or fails, which loses the shot.
pub trait ActionBackend: Backend<Vec<ActionOp>, Result<Vec<bool>, String>> {}

impl<B: Backend<Vec<ActionOp>, Result<Vec<bool>, String>> + ?Sized> ActionBackend for B {}

#[cfg(test)]
mod tests {
    use super::{Generator, Guard, tableau};
    use crate::pauli::{Clifford, Pauli};

    #[test]
    fn a_guard_reads_the_parity_of_outcomes_measured_before() {
        let guard = Guard {
            outcomes: vec![0, 2],
            invert: false,
        };
        assert_eq!(guard.holds(&[true, false, false]), Ok(true));
        assert_eq!(guard.holds(&[true, false, true]), Ok(false));
        let unless = Guard {
            invert: true,
            ..guard.clone()
        };
        assert_eq!(unless.holds(&[true, false, true]), Ok(true));
        assert!(guard.holds(&[true]).is_err());
    }

    #[test]
    fn a_clifford_map_is_a_tableau_on_the_qubits_it_touches() {
        let p = |t: &str| Pauli::parse(t).expect("a Pauli");
        // CX from qubit 7 to qubit 3.
        let (qubits, gate) = tableau(&[
            (Generator::X(7), p("X_3 X_7")),
            (Generator::Z(3), p("Z_3 Z_7")),
        ])
        .expect("a tableau");
        assert_eq!(qubits, [3, 7]);
        let cx = Clifford::named("cx", 2).expect("cx");
        assert_eq!(gate, cx.embed(&[1, 0], 2));
    }
}
