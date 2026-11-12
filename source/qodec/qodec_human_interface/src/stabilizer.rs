// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Stabilizer states with symbolic signs.
//!
//! A [`Tableau`] is a list of commuting Pauli generators, each with a sign
//! that is an affine [`Form`] over named variables: incoming frame signs,
//! random measurement outcomes, bit parameters. Every operation keeps the
//! signs symbolic, so a measurement's outcome comes out as a formula ("this is
//! the sign of input stabilizer 1", "this is random") instead of a sample.
//!
//! A Pauli is stored as X and Z bits per qubit, in Hermitian form: a qubit
//! with both bits set is `Y = i·X·Z`. Its sign is kept apart.

use crate::gf2::{Bits, Form};
use qodec_runtime_recipes::pauli::{Clifford, DensePauli, Pauli as SparsePauli};

/// A Hermitian Pauli operator on a fixed number of qubits, without its sign.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Pauli {
    /// X bits.
    pub x: Bits,
    /// Z bits.
    pub z: Bits,
}

impl Pauli {
    /// The identity.
    #[must_use]
    pub fn identity() -> Self {
        Self {
            x: Bits::default(),
            z: Bits::default(),
        }
    }

    /// `op` (`X`, `Y` or `Z` as x/z bits) on qubit `q`.
    #[must_use]
    pub fn single(q: usize, x: bool, z: bool) -> Self {
        let mut p = Self::identity();
        p.x.set(q, x);
        p.z.set(q, z);
        p
    }

    /// `p` with its qubit `i` placed at `place(i)`, and whether it is negative.
    pub fn place(
        p: &SparsePauli,
        place: impl Fn(usize) -> Option<usize>,
    ) -> Result<(Self, bool), String> {
        let mut out = Self::identity();
        for &(q, op) in p.terms() {
            let at = place(q).ok_or_else(|| format!("`{p}` acts outside its qubits"))?;
            out.x.set(at, op.has_x());
            out.z.set(at, op.has_z());
        }
        Ok((out, p.negative))
    }

    /// Whether it commutes with `other`.
    #[must_use]
    pub fn commutes(&self, other: &Pauli) -> bool {
        (self.x.and_count(&other.z) + self.z.and_count(&other.x)).is_multiple_of(2)
    }

    /// Whether it is the identity.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.x.is_zero() && self.z.is_zero()
    }

    /// Whether it acts on qubit `q`.
    #[must_use]
    pub fn touches(&self, q: usize) -> bool {
        self.x.get(q) || self.z.get(q)
    }

    /// The single-qubit Pauli on `q`: `None`, `X`, `Y` or `Z`.
    #[must_use]
    pub fn letter(&self, q: usize) -> Option<char> {
        match (self.x.get(q), self.z.get(q)) {
            (false, false) => None,
            (true, false) => Some('X'),
            (true, true) => Some('Y'),
            (false, true) => Some('Z'),
        }
    }

    /// `self · other` for commuting operators: the product, and whether it
    /// picks up a `-1`.
    #[must_use]
    pub fn mul(&self, other: &Pauli) -> (Pauli, bool) {
        let mut x = self.x.clone();
        x.xor(&other.x);
        let mut z = self.z.clone();
        z.xor(&other.z);
        // In `i^phase X^x Z^z` form, a Hermitian Pauli has phase |x&z|, and
        // moving Z's past X's costs 2 per overlap.
        let phase = self.x.and_count(&self.z)
            + other.x.and_count(&other.z)
            + 2 * self.z.and_count(&other.x);
        let hermitian = x.and_count(&z);
        let excess = (phase + 4 * (hermitian / 4 + 1) - hermitian) % 4;
        debug_assert!(excess.is_multiple_of(2), "multiplied anticommuting Paulis");
        (Pauli { x, z }, excess == 2)
    }
}

/// A Pauli with a symbolic sign: `(-1)^sign · pauli`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Signed {
    /// The operator.
    pub pauli: Pauli,
    /// Its sign.
    pub sign: Form,
}

impl Signed {
    /// `self · other`, for commuting operators.
    #[must_use]
    pub fn mul(&self, other: &Signed) -> Signed {
        let (pauli, minus) = self.pauli.mul(&other.pauli);
        let mut sign = self.sign.plus(&other.sign);
        if minus {
            sign.flip();
        }
        Signed { pauli, sign }
    }
}

/// A stabilizer state: commuting generators with symbolic signs.
#[derive(Clone, Debug, Default)]
pub struct Tableau {
    /// The generators.
    pub gens: Vec<Signed>,
}

/// What a measurement found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// The outcome was determined by the state: this formula.
    Determined(Form),
    /// The outcome was random: a fresh variable.
    Random(Form),
}

impl Found {
    /// The outcome's formula.
    #[must_use]
    pub fn form(&self) -> &Form {
        match self {
            Found::Determined(f) | Found::Random(f) => f,
        }
    }
}

impl Tableau {
    /// Adds a generator, which must commute with the others.
    pub fn add(&mut self, pauli: Pauli, sign: Form) {
        if !pauli.is_identity() {
            self.gens.push(Signed { pauli, sign });
        }
    }

    fn anticommuting(&self, p: &Pauli) -> Vec<usize> {
        (0..self.gens.len())
            .filter(|&i| !self.gens[i].pauli.commutes(p))
            .collect()
    }

    /// Generator `dst` becomes `dst · src`.
    fn multiply(&mut self, dst: usize, src: usize) {
        let product = self.gens[dst].mul(&self.gens[src]);
        self.gens[dst] = product;
    }

    /// Measures `(-1)^negative · p`. A random outcome is the variable
    /// `fresh()` gives.
    pub fn measure(&mut self, p: &Pauli, negative: bool, fresh: impl FnOnce() -> usize) -> Found {
        let anti = self.anticommuting(p);
        if let Some((&a, rest)) = anti.split_first() {
            for &b in rest {
                self.multiply(b, a);
            }
            let outcome = Form::var(fresh());
            let mut sign = outcome.clone();
            if negative {
                sign.flip();
            }
            self.gens[a] = Signed {
                pauli: p.clone(),
                sign,
            };
            return Found::Random(outcome);
        }
        if let Some(mut s) = self.sign_of(p) {
            if negative {
                s.flip();
            }
            return Found::Determined(s);
        }
        // A mixed state: the outcome is random, and the state now knows it.
        let outcome = Form::var(fresh());
        let mut sign = outcome.clone();
        if negative {
            sign.flip();
        }
        self.add(p.clone(), sign);
        Found::Random(outcome)
    }

    /// Prepares the `+1` eigenstate of `(-1)^negative · p`. Fails if the
    /// state already holds `p` with a sign that is not known to be right: that
    /// needs a correction that depends on the state.
    pub fn stabilize(&mut self, p: &Pauli, negative: bool) -> Result<(), String> {
        let anti = self.anticommuting(p);
        if let Some((&a, rest)) = anti.split_first() {
            for &b in rest {
                self.multiply(b, a);
            }
            self.gens[a] = Signed {
                pauli: p.clone(),
                sign: Form::constant(negative),
            };
            return Ok(());
        }
        if let Some(mut s) = self.sign_of(p) {
            if negative {
                s.flip();
            }
            if s.is_zero() {
                Ok(())
            } else {
                Err("it stabilizes an operator the state already holds, with a sign that may be wrong".to_string())
            }
        } else {
            self.add(p.clone(), Form::constant(negative));
            Ok(())
        }
    }

    /// Applies the Pauli `p` when `condition` is 1: it flips the sign of every
    /// generator it anticommutes with by `condition`.
    pub fn apply_pauli(&mut self, p: &Pauli, condition: &Form) {
        for g in &mut self.gens {
            if !g.pauli.commutes(p) {
                g.sign.xor(condition);
            }
        }
    }

    /// Applies Clifford `c`, whose qubit `i` is `positions[i]`.
    pub fn conjugate(&mut self, positions: &[usize], c: &Clifford) {
        let k = positions.len();
        for g in &mut self.gens {
            if !positions.iter().any(|&q| g.pauli.touches(q)) {
                continue;
            }
            let mut local = DensePauli::identity(k);
            for (i, &q) in positions.iter().enumerate() {
                local.x[i] = g.pauli.x.get(q);
                local.z[i] = g.pauli.z.get(q);
            }
            local.phase = u8::try_from(hermitian_phase(&local)).expect("phase is below 4");
            let image = c.conjugate(&local);
            let excess = (usize::from(image.phase) + 4 - hermitian_phase(&image)) % 4;
            for (i, &q) in positions.iter().enumerate() {
                g.pauli.x.set(q, image.x[i]);
                g.pauli.z.set(q, image.z[i]);
            }
            if excess == 2 {
                g.sign.flip();
            }
        }
    }

    /// Removes qubits `positions` from the state: the generators that act on
    /// them are combined until all but the ones that do not act on them are
    /// gone.
    pub fn trace_out(&mut self, positions: &[usize]) {
        let mut pivots: Vec<usize> = Vec::new();
        for &q in positions {
            for column in [true, false] {
                let has = |g: &Signed| {
                    if column {
                        g.pauli.x.get(q)
                    } else {
                        g.pauli.z.get(q)
                    }
                };
                let Some(p) =
                    (0..self.gens.len()).find(|&i| !pivots.contains(&i) && has(&self.gens[i]))
                else {
                    continue;
                };
                for i in 0..self.gens.len() {
                    if i != p && has(&self.gens[i]) {
                        self.multiply(i, p);
                    }
                }
                pivots.push(p);
            }
        }
        pivots.sort_unstable();
        for p in pivots.into_iter().rev() {
            self.gens.remove(p);
        }
    }

    /// Keeps only the part of the state that commutes with `p`, as a rotation
    /// about `p` does. Returns how many generators were lost.
    pub fn keep_commuting(&mut self, p: &Pauli) -> usize {
        let anti = self.anticommuting(p);
        let Some((&a, rest)) = anti.split_first() else {
            return 0;
        };
        for &b in rest {
            self.multiply(b, a);
        }
        self.gens.remove(a);
        1
    }

    /// The sign with which the state holds `p`, or `None` if it does not hold
    /// `p` up to sign.
    #[must_use]
    pub fn sign_of(&self, p: &Pauli) -> Option<Form> {
        if p.is_identity() {
            return Some(Form::default());
        }
        let combo = self.solve(p)?;
        let mut acc = Signed {
            pauli: Pauli::identity(),
            sign: Form::default(),
        };
        for i in combo.ones() {
            acc = acc.mul(&self.gens[i]);
        }
        debug_assert_eq!(acc.pauli, *p, "the generators multiply to `p`");
        Some(acc.sign)
    }

    /// Which generators multiply to `p` up to sign, if any.
    fn solve(&self, p: &Pauli) -> Option<Bits> {
        // Rows are generators' (x | z) bits, offset by `shift` for z.
        let shift = self
            .gens
            .iter()
            .map(|g| {
                g.pauli
                    .x
                    .ones()
                    .chain(g.pauli.z.ones())
                    .max()
                    .map_or(0, |m| m + 1)
            })
            .chain(p.x.ones().chain(p.z.ones()).max().map(|m| m + 1))
            .max()
            .unwrap_or(0);
        let row = |pauli: &Pauli| {
            let mut bits = pauli.x.clone();
            for q in pauli.z.ones() {
                bits.set(shift + q, true);
            }
            bits
        };
        let mut rows: Vec<(usize, Bits, Bits)> = Vec::new();
        for (i, g) in self.gens.iter().enumerate() {
            let mut r = row(&g.pauli);
            let mut combo = Bits::default();
            combo.set(i, true);
            for (pivot, pr, pc) in &rows {
                if r.get(*pivot) {
                    r.xor(pr);
                    combo.xor(pc);
                }
            }
            if let Some(pivot) = r.first() {
                for (_, other, other_combo) in &mut rows {
                    if other.get(pivot) {
                        other.xor(&r);
                        other_combo.xor(&combo);
                    }
                }
                rows.push((pivot, r, combo));
            }
        }
        let mut target = row(p);
        let mut combo = Bits::default();
        for (pivot, r, c) in &rows {
            if target.get(*pivot) {
                target.xor(r);
                combo.xor(c);
            }
        }
        target.is_zero().then_some(combo)
    }
}

/// The phase a Hermitian `X^x Z^z` operator has in `i^phase` form.
fn hermitian_phase(p: &DensePauli) -> usize {
    p.x.iter().zip(&p.z).filter(|(x, z)| **x && **z).count() % 4
}

#[cfg(test)]
mod tests {
    use super::{Found, Pauli, Tableau};
    use crate::gf2::Form;
    use qodec_runtime_recipes::pauli::Clifford;

    fn z(q: usize) -> Pauli {
        Pauli::single(q, false, true)
    }

    fn x(q: usize) -> Pauli {
        Pauli::single(q, true, false)
    }

    #[test]
    fn a_bell_pair_measures_correlated_outcomes() {
        let mut t = Tableau::default();
        t.stabilize(&z(0), false).expect("fresh qubit");
        t.stabilize(&z(1), false).expect("fresh qubit");
        t.conjugate(&[0], &Clifford::named("h", 1).expect("h"));
        t.conjugate(&[0, 1], &Clifford::named("cx", 2).expect("cx"));
        let mut next = 0;
        let first = t.measure(&z(0), false, || {
            next += 1;
            next - 1
        });
        assert_eq!(first, Found::Random(Form::var(0)));
        let second = t.measure(&z(1), false, || unreachable!("determined"));
        assert_eq!(second, Found::Determined(Form::var(0)));
        // X X was a stabilizer; it is gone, and its sign is unknown.
        let (xx, _) = x(0).mul(&x(1));
        assert_eq!(t.sign_of(&xx), None);
    }

    #[test]
    fn signs_follow_conditional_paulis_and_cliffords() {
        let mut t = Tableau::default();
        t.add(z(0), Form::var(3));
        t.apply_pauli(&x(0), &Form::var(5));
        t.conjugate(&[0], &Clifford::named("h", 1).expect("h"));
        assert_eq!(t.sign_of(&x(0)), Some(Form::var(3).plus(&Form::var(5))));
        // H Y H = -Y.
        let mut t = Tableau::default();
        t.add(Pauli::single(0, true, true), Form::default());
        t.conjugate(&[0], &Clifford::named("h", 1).expect("h"));
        assert_eq!(
            t.sign_of(&Pauli::single(0, true, true)),
            Some(Form::constant(true))
        );
    }

    #[test]
    fn tracing_out_keeps_what_does_not_touch_the_qubits() {
        let mut t = Tableau::default();
        let (zz, _) = z(0).mul(&z(1));
        t.add(zz, Form::var(0));
        t.add(z(1), Form::var(1));
        t.trace_out(&[1]);
        assert_eq!(t.sign_of(&z(0)), Some(Form::var(0).plus(&Form::var(1))));
        assert_eq!(t.gens.len(), 1);
    }
}
