// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A Pauli operator ([`Pauli`]), in the canonical form two instruction
//! sets compare: what a [`Meaning`](super::Meaning)'s steps and a `pauli`
//! argument are written with.
//!
//! Its qubits are numbered from 0 across an instruction's blocks, as Qodec
//! numbers them (its flat index space). Its form is canonical, so equal
//! operators compare equal: factors sorted by qubit, identities dropped,
//! and the phase a power of i.

use std::cmp::Ordering;
use std::fmt;
use std::ops::{Mul, Neg};

/// A Pauli operator: a phase (a power of i) times one X, Y or Z factor per
/// qubit it acts on, Qodec's `X_0 Z_2`, `-Z_0` or `i Y_1`.
///
/// Built from single-qubit factors and products: `Pauli::x(0) *
/// Pauli::x(1)` is `X_0 X_1`, `-Pauli::z(0)` is `-Z_0`; a qubit's factors
/// multiply into one (`X_0 · Y_0` is `i Z_0`). A `pauli` argument is
/// parsed from Qodec's text by [`Argument::pauli`](crate::Argument::pauli).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pauli {
    /// The power of i it is multiplied by, from 0 to 3: 2 is −1.
    phase: u8,
    /// One factor per qubit acted on, sorted by qubit, none the identity.
    factors: Vec<(u32, Axis)>,
}

impl Pauli {
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

    /// Parses Qodec's sparse form: an optional sign (`-`, `+`), an
    /// optional `i`, then factors `X_k`, `Y_k`, `Z_k` (or `I`, the
    /// identity), separated by spaces, each qubit at most once: `-X_0
    /// Z_2`, `i Y_1`, `-I`, `i`. `None` for any other text, the empty one
    /// included.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let mut rest = text.trim();
        let mut phase = 0;
        if let Some(after) = rest.strip_prefix('-') {
            phase = 2;
            rest = after;
        } else if let Some(after) = rest.strip_prefix('+') {
            rest = after;
        }
        let mut tokens = rest.split_whitespace().peekable();
        let mut any = false;
        if tokens.peek() == Some(&"i") {
            phase += 1;
            any = true;
            tokens.next();
        }
        let mut factors: Vec<(u32, Axis)> = Vec::new();
        // Every qubit named, `I_k`'s included, which add no factor.
        let mut named: Vec<u32> = Vec::new();
        for token in tokens {
            any = true;
            if token == "I" {
                continue;
            }
            let (axis, qubit) = token.split_once('_')?;
            if !qubit.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let qubit: u32 = qubit.parse().ok()?;
            let axis = match axis {
                "I" => None,
                "X" => Some(Axis::X),
                "Y" => Some(Axis::Y),
                "Z" => Some(Axis::Z),
                _ => return None,
            };
            if named.contains(&qubit) {
                return None;
            }
            named.push(qubit);
            if let Some(axis) = axis {
                factors.push((qubit, axis));
            }
        }
        if !any {
            return None;
        }
        factors.sort_unstable();
        Some(Self { phase, factors })
    }
}

/// A single-qubit Pauli other than the identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
        if self.phase >= 2 {
            f.write_str("-")?;
        }
        let imaginary = self.phase % 2 == 1;
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
