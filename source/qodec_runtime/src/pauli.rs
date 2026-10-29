// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Pauli and Clifford algebra used by the runtime. `qodec` stores Paulis as
//! text ([`qodec::PauliString`]); this module gives them algebraic meaning.

use qodec::PauliString;
use std::{collections::BTreeMap, fmt};

/// A Clifford tableau: the images of every `X_q` and `Z_q`, with signs.
/// Equality is equality up to a global phase.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Clifford {
    /// `C X_q C†` per qubit.
    pub x_images: Vec<DensePauli>,
    /// `C Z_q C†` per qubit.
    pub z_images: Vec<DensePauli>,
}

impl Clifford {
    /// The Clifford named by a generic operation acting on `arity` qubits, or
    /// `None` if the operation is not a known Clifford of that arity.
    #[must_use]
    pub fn named(name: &str, arity: usize) -> Option<Clifford> {
        let images: &[(&str, &str)] = match (name, arity) {
            ("i" | "id", 1) => &[],
            ("x", 1) => &[("Z_0", "-Z_0")],
            ("y", 1) => &[("X_0", "-X_0"), ("Z_0", "-Z_0")],
            ("z", 1) => &[("X_0", "-X_0")],
            ("h", 1) => &[("X_0", "Z_0"), ("Z_0", "X_0")],
            ("s", 1) => &[("X_0", "Y_0")],
            ("s_adj", 1) => &[("X_0", "-Y_0")],
            ("sx", 1) => &[("Z_0", "-Y_0")],
            ("sx_adj", 1) => &[("Z_0", "Y_0")],
            ("cx", 2) => &[("X_0", "X_0 X_1"), ("Z_1", "Z_0 Z_1")],
            ("cy", 2) => &[("X_0", "X_0 Y_1"), ("X_1", "Z_0 X_1"), ("Z_1", "Z_0 Z_1")],
            ("cz", 2) => &[("X_0", "X_0 Z_1"), ("X_1", "Z_0 X_1")],
            ("swap", 2) => &[
                ("X_0", "X_1"),
                ("Z_0", "Z_1"),
                ("X_1", "X_0"),
                ("Z_1", "Z_0"),
            ],
            _ => return None,
        };
        let images = images
            .iter()
            .map(|(g, i)| (PauliString::from(*g), PauliString::from(*i)))
            .collect();
        Clifford::from_images(&images, arity).ok()
    }

    /// The Clifford of a `clifford` action step (`{generator: image}`).
    /// Generators that are not listed map to themselves.
    pub fn from_images(
        images: &BTreeMap<PauliString, PauliString>,
        width: usize,
    ) -> Result<Self, String> {
        let mut c = Self::identity(width);
        for (generator, image) in images {
            let (q, is_x) = clifford_generator(&Pauli::from_text(generator)?)?;
            if q >= width {
                return Err(format!("Clifford generator outside {width} qubits"));
            }
            let image = DensePauli::from_pauli(&Pauli::from_text(image)?, width)?;
            if is_x {
                c.x_images[q] = image;
            } else {
                c.z_images[q] = image;
            }
        }
        Ok(c)
    }

    /// Conjugation by a Pauli: `P X_q P†` and `P Z_q P†`.
    pub fn from_pauli(p: &Pauli, width: usize) -> Result<Self, String> {
        let mut c = Self::identity(width);
        for &(q, op) in p.terms() {
            if q >= width {
                return Err(format!("Pauli `{p}` acts outside {width} qubits"));
            }
            if op.has_z() {
                c.x_images[q].phase = 2;
            }
            if op.has_x() {
                c.z_images[q].phase = 2;
            }
        }
        Ok(c)
    }

    /// Embeds this Clifford into `width` qubits, with qubit `i` at `positions[i]`.
    #[must_use]
    pub fn embed(&self, positions: &[usize], width: usize) -> Clifford {
        let place = |p: &DensePauli| {
            let mut d = DensePauli::identity(width);
            d.phase = p.phase;
            for (i, &pos) in positions.iter().enumerate() {
                d.x[pos] = p.x[i];
                d.z[pos] = p.z[i];
            }
            d
        };
        let mut c = Clifford::identity(width);
        for (i, &pos) in positions.iter().enumerate() {
            c.x_images[pos] = place(&self.x_images[i]);
            c.z_images[pos] = place(&self.z_images[i]);
        }
        c
    }

    /// The identity on `width` qubits.
    #[must_use]
    pub fn identity(width: usize) -> Self {
        Self {
            x_images: (0..width)
                .map(|q| DensePauli::single(width, q, true, false))
                .collect(),
            z_images: (0..width)
                .map(|q| DensePauli::single(width, q, false, true))
                .collect(),
        }
    }

    /// `after ∘ self`: apply `self`, then `after`.
    #[must_use]
    pub fn then(&self, after: &Clifford) -> Clifford {
        Clifford {
            x_images: self.x_images.iter().map(|p| after.conjugate(p)).collect(),
            z_images: self.z_images.iter().map(|p| after.conjugate(p)).collect(),
        }
    }

    /// `C P C†`.
    #[must_use]
    pub fn conjugate(&self, p: &DensePauli) -> DensePauli {
        let width = self.width();
        let mut acc = DensePauli::identity(width);
        acc.phase = p.phase;
        for q in 0..width {
            if p.x[q] {
                acc = acc.mul(&self.x_images[q]);
            }
        }
        for q in 0..width {
            if p.z[q] {
                acc = acc.mul(&self.z_images[q]);
            }
        }
        acc
    }

    /// The number of qubits.
    #[must_use]
    pub fn width(&self) -> usize {
        self.x_images.len()
    }
}

/// A Clifford generator key must be a single unsigned `X_q` or `Z_q`.
/// Returns `(q, is_x)`.
fn clifford_generator(p: &Pauli) -> Result<(usize, bool), String> {
    match p.terms() {
        [(q, PauliOp::X)] if !p.negative => Ok((*q, true)),
        [(q, PauliOp::Z)] if !p.negative => Ok((*q, false)),
        _ => Err(format!("`{p}` is not a Clifford generator (X_q or Z_q)")),
    }
}

/// A dense Pauli `i^phase · X^x · Z^z`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DensePauli {
    /// X exponent per qubit.
    pub x: Vec<bool>,
    /// Z exponent per qubit.
    pub z: Vec<bool>,
    /// Power of `i`, modulo 4.
    pub phase: u8,
}

impl DensePauli {
    /// Fails if the Pauli acts outside `0..width`.
    pub fn from_pauli(p: &Pauli, width: usize) -> Result<Self, String> {
        let mut d = Self::identity(width);
        if p.negative {
            d.phase = 2;
        }
        for &(q, op) in p.terms() {
            if q >= width {
                return Err(format!("Pauli `{p}` acts outside {width} qubits"));
            }
            d.x[q] = op.has_x();
            d.z[q] = op.has_z();
            if op == PauliOp::Y {
                // Y = i·X·Z
                d.phase = (d.phase + 1) % 4;
            }
        }
        Ok(d)
    }

    /// `X^x Z^z` on `qubit`, on `width` qubits.
    #[must_use]
    pub fn single(width: usize, qubit: usize, x: bool, z: bool) -> Self {
        let mut d = Self::identity(width);
        d.x[qubit] = x;
        d.z[qubit] = z;
        d
    }

    /// The identity on `width` qubits.
    #[must_use]
    pub fn identity(width: usize) -> Self {
        Self {
            x: vec![false; width],
            z: vec![false; width],
            phase: 0,
        }
    }

    /// The Pauli on `qubit`, ignoring the phase, or `None` for the identity.
    #[must_use]
    pub fn op(&self, qubit: usize) -> Option<PauliOp> {
        PauliOp::from_bits(self.x[qubit], self.z[qubit])
    }

    /// The qubits it acts on non-trivially, in increasing order.
    pub fn support(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.width()).filter(|&q| self.x[q] || self.z[q])
    }

    /// The number of qubits.
    #[must_use]
    pub fn width(&self) -> usize {
        self.x.len()
    }

    /// `self · other`.
    #[must_use]
    pub fn mul(&self, other: &DensePauli) -> DensePauli {
        // X^x1 Z^z1 X^x2 Z^z2 = (-1)^{z1·x2} X^{x1+x2} Z^{z1+z2}
        let swaps = self
            .z
            .iter()
            .zip(&other.x)
            .filter(|(z, x)| **z && **x)
            .count();
        #[allow(clippy::cast_possible_truncation)]
        let phase = ((usize::from(self.phase) + usize::from(other.phase) + 2 * swaps) % 4) as u8;
        DensePauli {
            x: self.x.iter().zip(&other.x).map(|(a, b)| a ^ b).collect(),
            z: self.z.iter().zip(&other.z).map(|(a, b)| a ^ b).collect(),
            phase,
        }
    }

    /// Concatenated `x` and `z` bits, ignoring the phase.
    #[must_use]
    pub fn bits(&self) -> Vec<bool> {
        self.x.iter().chain(&self.z).copied().collect()
    }
}

/// A sparse, signed Pauli operator, such as `-X_0 Z_2`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Pauli {
    /// Whether the operator has a `-` sign.
    pub negative: bool,
    /// Sorted by qubit, at most one entry per qubit.
    terms: Vec<(usize, PauliOp)>,
}

impl Pauli {
    /// Parses a [`qodec::PauliString`].
    pub fn from_text(text: &PauliString) -> Result<Self, String> {
        Self::parse(&text.0)
    }

    /// Parses the qodec Pauli grammar: whitespace-separated `B[_index]` tokens
    /// with `B ∈ {I, X, Y, Z}` and a missing index meaning 0, optionally preceded
    /// by a sign (action Paulis may be signed; code Paulis are not).
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut negative = false;
        let mut terms = Vec::new();
        let mut rest = text.trim();
        if let Some(r) = rest.strip_prefix('-') {
            negative = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix('+') {
            rest = r;
        }
        for token in rest.split_whitespace() {
            let (basis, index) = token.split_once('_').unwrap_or((token, "0"));
            let op = match basis {
                "X" => Some(PauliOp::X),
                "Y" => Some(PauliOp::Y),
                "Z" => Some(PauliOp::Z),
                "I" => None,
                _ => return Err(format!("invalid Pauli token `{token}`")),
            };
            let qubit = index
                .parse::<usize>()
                .map_err(|_| format!("invalid Pauli token `{token}`"))?;
            if let Some(op) = op {
                terms.push((qubit, op));
            }
        }
        Self::new(negative, terms)
    }

    /// Builds a Pauli from `(qubit, op)` terms in any order. Fails if a qubit
    /// appears twice.
    pub fn new(negative: bool, mut terms: Vec<(usize, PauliOp)>) -> Result<Self, String> {
        terms.sort_unstable();
        if terms.windows(2).any(|w| w[0].0 == w[1].0) {
            return Err("a Pauli acts on a qubit at most once".to_string());
        }
        Ok(Self { negative, terms })
    }

    /// `op` on `qubit`.
    #[must_use]
    pub fn single(qubit: usize, op: PauliOp) -> Self {
        Self {
            negative: false,
            terms: vec![(qubit, op)],
        }
    }

    /// The non-identity terms, sorted by qubit.
    #[must_use]
    pub fn terms(&self) -> &[(usize, PauliOp)] {
        &self.terms
    }

    /// The Pauli on `qubit`, or `None` for the identity.
    #[must_use]
    pub fn get(&self, qubit: usize) -> Option<PauliOp> {
        self.terms
            .binary_search_by_key(&qubit, |(q, _)| *q)
            .ok()
            .map(|i| self.terms[i].1)
    }

    /// The qubits it acts on non-trivially, in increasing order.
    pub fn support(&self) -> impl Iterator<Item = usize> + '_ {
        self.terms.iter().map(|(q, _)| *q)
    }

    /// The largest qubit it acts on, if any.
    #[must_use]
    pub fn max_qubit(&self) -> Option<usize> {
        self.terms.last().map(|(q, _)| *q)
    }

    /// Whether it is the identity, up to sign.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.terms.is_empty()
    }

    /// The product of two Paulis, ignoring the phase.
    #[must_use]
    pub fn mul_unsigned(&self, other: &Pauli) -> Pauli {
        let mut terms = Vec::with_capacity(self.terms.len() + other.terms.len());
        let (mut i, mut j) = (0, 0);
        while i < self.terms.len() || j < other.terms.len() {
            let a = self.terms.get(i);
            let b = other.terms.get(j);
            match (a, b) {
                (Some(&(qa, pa)), Some(&(qb, pb))) if qa == qb => {
                    if let Some(op) =
                        PauliOp::from_bits(pa.has_x() ^ pb.has_x(), pa.has_z() ^ pb.has_z())
                    {
                        terms.push((qa, op));
                    }
                    i += 1;
                    j += 1;
                }
                (Some(&(qa, pa)), Some(&(qb, _))) if qa < qb => {
                    terms.push((qa, pa));
                    i += 1;
                }
                (Some(&(qa, pa)), None) => {
                    terms.push((qa, pa));
                    i += 1;
                }
                (_, Some(&(qb, pb))) => {
                    terms.push((qb, pb));
                    j += 1;
                }
                (None, None) => unreachable!("loop condition guarantees one side"),
            }
        }
        Pauli {
            negative: false,
            terms,
        }
    }
}

impl fmt::Display for Pauli {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.negative {
            f.write_str("-")?;
        }
        if self.terms.is_empty() {
            return f.write_str("I");
        }
        for (i, (q, op)) in self.terms.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{op:?}_{q}")?;
        }
        Ok(())
    }
}

/// A single-qubit Pauli.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PauliOp {
    /// Pauli X.
    X,
    /// Pauli Y.
    Y,
    /// Pauli Z.
    Z,
}

impl PauliOp {
    /// The lowercase operation name: `x`, `y` or `z`.
    #[must_use]
    pub fn lower(self) -> &'static str {
        match self {
            PauliOp::X => "x",
            PauliOp::Y => "y",
            PauliOp::Z => "z",
        }
    }

    /// Whether it has an X component (X or Y).
    #[must_use]
    pub fn has_x(self) -> bool {
        matches!(self, PauliOp::X | PauliOp::Y)
    }

    /// Whether it has a Z component (Z or Y).
    #[must_use]
    pub fn has_z(self) -> bool {
        matches!(self, PauliOp::Z | PauliOp::Y)
    }

    fn from_bits(x: bool, z: bool) -> Option<PauliOp> {
        match (x, z) {
            (false, false) => None,
            (true, false) => Some(PauliOp::X),
            (true, true) => Some(PauliOp::Y),
            (false, true) => Some(PauliOp::Z),
        }
    }
}
