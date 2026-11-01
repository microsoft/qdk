// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an instruction does ([`Action`]), in the form Qodec declares it: so
//! that two instruction sets of other names can be matched by what their
//! instructions do, not by what they call them (Stim's `R` is a toy's
//! `reset`).
//!
//! An action is the sizes of the blocks an instruction takes and gives, and
//! a list of steps on the qubits those blocks encode, each step written with
//! [`Pauli`] operators: prepare a stabilizer state, apply a Clifford or a
//! Pauli, observe Paulis, rotate about a Pauli by an [`Angle`]. Two actions
//! are the same when their block sizes and steps are equal. Equal is
//! syntactic, on a canonical form: a Pauli's factors are sorted by qubit and
//! its identities dropped, and a Clifford lists only the generators it
//! changes; two actions that do the same thing written otherwise (another
//! choice of stabilizer generators) differ.
//!
//! Parsing Qodec's syntax (`[clifford: {X_0: X_0 X_1}]`) is a loader's job:
//! the crate holds the parsed form.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::ops::{Mul, Neg};

/// A Pauli operator on the qubits of an instruction's blocks: a phase (a
/// power of i) times one X, Y or Z factor per qubit it acts on. The qubits
/// are numbered from 0 across the blocks the instruction takes, those of
/// its first block first, or across the blocks it gives when it gives more
/// (a preparation, which takes none), as Qodec numbers them.
///
/// Built from single-qubit factors and products: `Pauli::x(0) *
/// Pauli::x(1)` is Qodec's `X_0 X_1`, `-Pauli::z(0)` its `-Z_0`. Its form
/// is canonical, so that equal operators compare equal: `X_1 X_0` is
/// `X_0 X_1`, and a qubit's factors multiply into one (`X_0 Y_0` is
/// `i Z_0`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pauli {
    /// The power of i it is multiplied by, from 0 to 3: 2 is −1.
    phase: u8,
    /// One factor per qubit acted on, sorted by qubit, none the identity.
    factors: Vec<(u32, Axis)>,
}

/// A single-qubit Pauli other than the identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// The product `self · other`: the power of i it carries, and its
    /// factor, `None` for the identity (`X · X = I`, `X · Y = i Z`,
    /// `Y · X = −i Z`).
    fn times(self, other: Axis) -> (u8, Option<Axis>) {
        use Axis::{X, Y, Z};
        match (self, other) {
            (X, X) | (Y, Y) | (Z, Z) => (0, None),
            (X, Y) => (1, Some(Z)),
            (Y, Z) => (1, Some(X)),
            (Z, X) => (1, Some(Y)),
            (Y, X) => (3, Some(Z)),
            (Z, Y) => (3, Some(X)),
            (X, Z) => (3, Some(Y)),
        }
    }
}

impl Pauli {
    /// The identity, Qodec's `I`.
    #[must_use]
    pub fn identity() -> Self {
        Self {
            phase: 0,
            factors: Vec::new(),
        }
    }

    /// The identity times i, Qodec's `i`, for a phase: `Pauli::i() *
    /// Pauli::y(1)` is `i Y_1`.
    #[must_use]
    pub fn i() -> Self {
        Self {
            phase: 1,
            factors: Vec::new(),
        }
    }

    /// X on qubit `qubit`, Qodec's `X_qubit`.
    #[must_use]
    pub fn x(qubit: u32) -> Self {
        Self::on(qubit, Axis::X)
    }

    /// Y on qubit `qubit`, Qodec's `Y_qubit`.
    #[must_use]
    pub fn y(qubit: u32) -> Self {
        Self::on(qubit, Axis::Y)
    }

    /// Z on qubit `qubit`, Qodec's `Z_qubit`.
    #[must_use]
    pub fn z(qubit: u32) -> Self {
        Self::on(qubit, Axis::Z)
    }

    fn on(qubit: u32, axis: Axis) -> Self {
        Self {
            phase: 0,
            factors: vec![(qubit, axis)],
        }
    }
}

impl Mul for Pauli {
    type Output = Pauli;

    /// The product `self · other`, in canonical form: the factors of both,
    /// by qubit, those on the same qubit multiplied into one.
    fn mul(self, other: Pauli) -> Pauli {
        let mut phase = self.phase + other.phase;
        let mut factors = Vec::with_capacity(self.factors.len() + other.factors.len());
        let mut left = self.factors.into_iter().peekable();
        let mut right = other.factors.into_iter().peekable();
        loop {
            let next = match (left.peek(), right.peek()) {
                (None, None) => break,
                (Some(_), None) => left.next(),
                (None, Some(_)) => right.next(),
                (Some((a, _)), Some((b, _))) => match a.cmp(b) {
                    Ordering::Less => left.next(),
                    Ordering::Greater => right.next(),
                    Ordering::Equal => {
                        let (qubit, a) = left.next().expect("peeked");
                        let (_, b) = right.next().expect("peeked");
                        let (power, axis) = a.times(b);
                        // Reduced at each qubit, so that many cannot overflow.
                        phase = (phase + power) % 4;
                        axis.map(|axis| (qubit, axis))
                    }
                },
            };
            factors.extend(next);
        }
        Pauli {
            phase: phase % 4,
            factors,
        }
    }
}

impl Neg for Pauli {
    type Output = Pauli;

    /// −1 times it, Qodec's `-` before a Pauli.
    fn neg(mut self) -> Pauli {
        self.phase = (self.phase + 2) % 4;
        self
    }
}

impl fmt::Display for Pauli {
    /// In Qodec's syntax: `X_0 X_1`, `-Z_0`, `i Y_1`, `-i X_0`, `I`, `-I`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.phase >= 2 { "-" } else { "" };
        let imaginary = self.phase % 2 == 1;
        f.write_str(sign)?;
        if self.factors.is_empty() {
            return f.write_str(if imaginary { "i" } else { "I" });
        }
        if imaginary {
            f.write_str("i ")?;
        }
        for (k, (qubit, axis)) in self.factors.iter().enumerate() {
            if k > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{axis:?}_{qubit}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Pauli {
    /// As [`Display`](fmt::Display) writes it, so that a failed comparison
    /// reads as Qodec does.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// The angle of a rotation: a number, or one of the instruction's
/// parameters.
///
/// Angles compare by value, bit for bit (so that a set's instructions stay
/// `Eq`): `0.5` equals `0.5`, a NaN equals itself, and `0.0` differs from
/// `-0.0`; a loader that reads angles from text writes each the same way.
#[derive(Clone, Copy, Debug)]
pub enum Angle {
    /// An angle fixed by the instruction, in radians.
    Literal(f64),
    /// The instruction's parameter at this index, counted among its
    /// parameters (its [`Real`](crate::OperandKind::Real) and
    /// [`Integer`](crate::OperandKind::Integer) operands) from 0, not among
    /// all its operands: by position, not by name, so that `rotate(theta)`
    /// and `rotate(angle)` are the same instruction.
    Parameter(usize),
}

impl PartialEq for Angle {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Angle::Literal(a), Angle::Literal(b)) => a.to_bits() == b.to_bits(),
            (Angle::Parameter(a), Angle::Parameter(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Angle {}

/// One step of an action, in Qodec's terms.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Step {
    /// Prepares the state these Paulis stabilize.
    Stabilize(Vec<Pauli>),
    /// Maps each Pauli to its image; a generator not listed is fixed.
    Clifford(BTreeMap<Pauli, Pauli>),
    /// Applies a Pauli.
    Pauli(Pauli),
    /// Measures these Paulis, one readout each, in order.
    Observe(Vec<Pauli>),
    /// Rotates about a Pauli by an angle.
    Rotate { pauli: Pauli, angle: Angle },
}

/// What an instruction does, as Qodec declares it in its `in`, `out` and
/// `action`: the sizes of the blocks it takes and gives, and its steps, in
/// order.
///
/// Built from its blocks, then step by step: Qodec's
/// `{in: [qubit, qubit], out: [qubit, qubit], action: [clifford: {X_0: X_0
/// X_1, Z_1: Z_0 Z_1}]}`, where a `qubit` block encodes one qubit, is
///
/// ```
/// # use executor_protocol::{Action, Pauli};
/// let cx = Action::new([1, 1], [1, 1]).clifford([
///     (Pauli::x(0), Pauli::x(0) * Pauli::x(1)),
///     (Pauli::z(1), Pauli::z(0) * Pauli::z(1)),
/// ]);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    inputs: Vec<u32>,
    outputs: Vec<u32>,
    steps: Vec<Step>,
}

impl Action {
    /// The action of an instruction that takes blocks encoding `inputs`
    /// qubits each and gives blocks encoding `outputs` qubits each, in
    /// order, with no steps yet. A preparation takes none (`new([], [1])`),
    /// a measurement gives none (`new([1], [])`).
    ///
    /// The sizes, not the block types' names: two sets may name the same
    /// block (one qubit) differently.
    #[must_use]
    pub fn new(
        inputs: impl IntoIterator<Item = u32>,
        outputs: impl IntoIterator<Item = u32>,
    ) -> Self {
        Self {
            inputs: inputs.into_iter().collect(),
            outputs: outputs.into_iter().collect(),
            steps: Vec::new(),
        }
    }

    /// This action, then the preparation of the state `generators`
    /// stabilize, Qodec's `stabilize`.
    #[must_use]
    pub fn stabilize(self, generators: impl IntoIterator<Item = Pauli>) -> Self {
        self.then(Step::Stabilize(generators.into_iter().collect()))
    }

    /// This action, then the Clifford that maps each generator to its
    /// image, Qodec's `clifford`. A generator mapped to itself is dropped,
    /// as one not listed is fixed.
    #[must_use]
    pub fn clifford(self, images: impl IntoIterator<Item = (Pauli, Pauli)>) -> Self {
        let changed = images
            .into_iter()
            .filter(|(generator, image)| generator != image)
            .collect();
        self.then(Step::Clifford(changed))
    }

    /// This action, then `pauli`, Qodec's `pauli`.
    #[must_use]
    pub fn pauli(self, pauli: Pauli) -> Self {
        self.then(Step::Pauli(pauli))
    }

    /// This action, then the measurement of `observables`, one readout
    /// each, in order, Qodec's `observe`.
    #[must_use]
    pub fn observe(self, observables: impl IntoIterator<Item = Pauli>) -> Self {
        self.then(Step::Observe(observables.into_iter().collect()))
    }

    /// This action, then the rotation about `pauli` by `angle`, Qodec's
    /// `rotate`: `exp(-iθP/2)` for a Pauli product `P` and an angle `θ`,
    /// so that `θ = π/4` about `Z` is a `T` gate up to a global phase.
    #[must_use]
    pub fn rotate(self, pauli: Pauli, angle: Angle) -> Self {
        self.then(Step::Rotate { pauli, angle })
    }

    fn then(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }
}
