// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Parity expressions over a gadget's [parity references], and the GF(2)
//! system that solves them.
//!
//! [parity references]: https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/gadget.md#parity-equations-property-path-references

use std::ops::{BitXor, BitXorAssign};

/// GF(2) system with incremental elimination.
///
/// `rows` is iterated in INSERTION order, and `add` only appends. Each new row
/// is reduced by all earlier rows, so no row contains an earlier pivot, and one
/// in-order pass of `reduce` eliminates every pivot. With any other order (e.g.
/// a sorted or hash map), `value` can be wrong and `Inconsistent` can be missed.
#[derive(Clone, Debug, Default)]
pub struct BinarySystem {
    /// Pivot -> reduced row.
    rows: Vec<(Var, Parity)>,
}

impl BinarySystem {
    /// An empty system.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the equation `p == 0`.
    pub fn add(&mut self, p: Parity) -> Result<(), Inconsistent> {
        let q = self.reduce(p);
        if let Some(&pivot) = q.vars.first() {
            self.rows.push((pivot, q));
            Ok(())
        } else if q.constant {
            Err(Inconsistent)
        } else {
            Ok(())
        }
    }

    /// The value of `p`, if the system determines it.
    #[must_use]
    pub fn value(&self, p: Parity) -> Option<bool> {
        let q = self.reduce(p);
        q.is_constant().then_some(q.constant)
    }

    /// Eliminates every pivot from `p`.
    #[must_use]
    pub fn reduce(&self, mut p: Parity) -> Parity {
        for (pivot, row) in &self.rows {
            if p.contains(*pivot) {
                p ^= row;
            }
        }
        p
    }
}

/// XOR of `vars` XOR `constant`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Parity {
    /// Sorted, without duplicates.
    vars: Vec<Var>,
    /// The constant term.
    pub constant: bool,
}

impl Parity {
    /// A constant parity.
    #[must_use]
    pub fn constant(value: bool) -> Self {
        Self {
            vars: Vec::new(),
            constant: value,
        }
    }

    /// A single variable.
    #[must_use]
    pub fn var(var: Var) -> Self {
        Self {
            vars: vec![var],
            constant: false,
        }
    }

    /// The variables, sorted and without duplicates.
    #[must_use]
    pub fn vars(&self) -> &[Var] {
        &self.vars
    }

    /// Whether `var` is a term of this parity.
    #[must_use]
    pub fn contains(&self, var: Var) -> bool {
        self.vars.binary_search(&var).is_ok()
    }

    /// Whether this parity has no variables.
    #[must_use]
    pub fn is_constant(&self) -> bool {
        self.vars.is_empty()
    }
}

impl BitXorAssign for Parity {
    fn bitxor_assign(&mut self, rhs: Parity) {
        *self ^= &rhs;
    }
}

impl BitXor for Parity {
    type Output = Parity;
    fn bitxor(mut self, rhs: Parity) -> Parity {
        self ^= &rhs;
        self
    }
}

impl BitXor<&Parity> for Parity {
    type Output = Parity;
    fn bitxor(mut self, rhs: &Parity) -> Parity {
        self ^= rhs;
        self
    }
}

impl BitXorAssign<&Parity> for Parity {
    fn bitxor_assign(&mut self, rhs: &Parity) {
        if rhs.vars.is_empty() {
            self.constant ^= rhs.constant;
            return;
        }
        // Symmetric difference of two sorted lists.
        let mut merged = Vec::with_capacity(self.vars.len() + rhs.vars.len());
        let (mut i, mut j) = (0, 0);
        let (a, b) = (&self.vars, &rhs.vars);
        while i < a.len() && j < b.len() {
            match a[i].cmp(&b[j]) {
                std::cmp::Ordering::Less => {
                    merged.push(a[i]);
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    merged.push(b[j]);
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    i += 1;
                    j += 1;
                }
            }
        }
        merged.extend_from_slice(&a[i..]);
        merged.extend_from_slice(&b[j..]);
        self.vars = merged;
        self.constant ^= rhs.constant;
    }
}

/// A bit that a parity equation can reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Var {
    /// `circuit.readouts[i]`
    Rec(usize),
    /// `readouts[i]`
    Ro(usize),
    /// `in|out[e].stabilizers|x|z[k]`
    Enc {
        /// The boundary side.
        side: Side,
        /// The encoding entry on that side.
        e: usize,
        /// The kind of sign.
        basis: Basis,
        /// The index of the operator in the code.
        k: usize,
    },
}

/// The side of a gadget boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    /// `in[e]`
    In,
    /// `out[e]`
    Out,
}

/// The kind of sign of an encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Basis {
    /// A stabilizer generator sign.
    Stabilizers,
    /// A logical X sign.
    X,
    /// A logical Z sign.
    Z,
}

/// The parity evidence contradicts itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inconsistent;
