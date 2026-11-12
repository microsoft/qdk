// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Bits over GF(2): packed bit vectors, and affine forms, the parities that
//! signs and measurement outcomes are made of.

/// A packed vector of bits that grows as needed. Trailing zero bits don't
/// count: two vectors with the same 1 bits are equal.
#[derive(Clone, Debug, Default)]
pub struct Bits(Vec<u64>);

impl Bits {
    /// The words up to the last non-zero one.
    fn significant(&self) -> &[u64] {
        let n = self.0.iter().rposition(|&w| w != 0).map_or(0, |i| i + 1);
        &self.0[..n]
    }
}

impl PartialEq for Bits {
    fn eq(&self, other: &Self) -> bool {
        self.significant() == other.significant()
    }
}

impl Eq for Bits {}

impl std::hash::Hash for Bits {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.significant().hash(state);
    }
}

impl Bits {
    /// Bit `i`.
    #[must_use]
    pub fn get(&self, i: usize) -> bool {
        self.0.get(i / 64).is_some_and(|w| w >> (i % 64) & 1 == 1)
    }

    /// Sets bit `i` to `value`.
    pub fn set(&mut self, i: usize, value: bool) {
        if self.0.len() <= i / 64 {
            if !value {
                return;
            }
            self.0.resize(i / 64 + 1, 0);
        }
        if value {
            self.0[i / 64] |= 1 << (i % 64);
        } else {
            self.0[i / 64] &= !(1 << (i % 64));
        }
    }

    /// Flips bit `i`.
    pub fn flip(&mut self, i: usize) {
        let v = self.get(i);
        self.set(i, !v);
    }

    /// `self ^= other`.
    pub fn xor(&mut self, other: &Bits) {
        if self.0.len() < other.0.len() {
            self.0.resize(other.0.len(), 0);
        }
        for (a, b) in self.0.iter_mut().zip(&other.0) {
            *a ^= b;
        }
    }

    /// Whether every bit is 0.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|&w| w == 0)
    }

    /// The number of positions where both are 1.
    #[must_use]
    pub fn and_count(&self, other: &Bits) -> usize {
        self.0
            .iter()
            .zip(&other.0)
            .map(|(a, b)| (a & b).count_ones() as usize)
            .sum()
    }

    /// The indices of the 1 bits, in increasing order.
    pub fn ones(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().enumerate().flat_map(|(k, &w)| {
            let mut w = w;
            std::iter::from_fn(move || {
                (w != 0).then(|| {
                    let b = w.trailing_zeros() as usize;
                    w &= w - 1;
                    k * 64 + b
                })
            })
        })
    }

    /// The lowest 1 bit, if any.
    #[must_use]
    pub fn first(&self) -> Option<usize> {
        self.ones().next()
    }
}

/// An affine form over GF(2): a constant XOR some variables. Variables are
/// numbered; what they stand for is up to the user of the form.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Form {
    /// The constant term.
    pub constant: bool,
    /// The variables, as a bit set.
    pub vars: Bits,
}

impl Form {
    /// The constant `value`.
    #[must_use]
    pub fn constant(value: bool) -> Self {
        Self {
            constant: value,
            vars: Bits::default(),
        }
    }

    /// Variable `i`.
    #[must_use]
    pub fn var(i: usize) -> Self {
        let mut vars = Bits::default();
        vars.set(i, true);
        Self {
            constant: false,
            vars,
        }
    }

    /// `self ^= other`.
    pub fn xor(&mut self, other: &Form) {
        self.constant ^= other.constant;
        self.vars.xor(&other.vars);
    }

    /// `self ^ other`.
    #[must_use]
    pub fn plus(&self, other: &Form) -> Form {
        let mut f = self.clone();
        f.xor(other);
        f
    }

    /// Flips the constant term.
    pub fn flip(&mut self) {
        self.constant = !self.constant;
    }

    /// Whether it is the constant 0.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        !self.constant && self.vars.is_zero()
    }

    /// Whether it has no variables.
    #[must_use]
    pub fn is_constant(&self) -> bool {
        self.vars.is_zero()
    }
}

/// An incremental basis of forms in reduced row echelon form over their
/// variables, each row remembering which inputs it combines. It answers
/// whether a form is a combination of the inputs (ignoring constants), and
/// which.
#[derive(Clone, Debug, Default)]
pub struct Basis {
    /// (pivot variable, row, which inputs XOR to the row).
    rows: Vec<(usize, Form, Bits)>,
    inputs: usize,
}

impl Basis {
    /// Adds input `form`, which becomes input number `self.len()`. Returns
    /// the combination of earlier inputs it equals over its variables, if it
    /// is dependent on them.
    pub fn add(&mut self, form: &Form) -> Option<(Bits, bool)> {
        let index = self.inputs;
        self.inputs += 1;
        let mut combo = Bits::default();
        combo.set(index, true);
        let (row, combo) = self.reduce(form.clone(), combo);
        if let Some(pivot) = row.vars.first() {
            for (_, other, other_combo) in &mut self.rows {
                if other.vars.get(pivot) {
                    other.xor(&row);
                    other_combo.xor(&combo);
                }
            }
            self.rows.push((pivot, row, combo));
            None
        } else {
            let mut earlier = combo;
            earlier.set(index, false);
            Some((earlier, row.constant))
        }
    }

    fn reduce(&self, mut form: Form, mut combo: Bits) -> (Form, Bits) {
        for (pivot, row, row_combo) in &self.rows {
            if form.vars.get(*pivot) {
                form.xor(row);
                combo.xor(row_combo);
            }
        }
        (form, combo)
    }

    /// Writes `form` as a combination of the inputs plus a remainder: the
    /// inputs used, and what is left. The remainder has no variable that is a
    /// pivot of the basis.
    #[must_use]
    pub fn express(&self, form: &Form) -> (Bits, Form) {
        let (rest, combo) = self.reduce(form.clone(), Bits::default());
        (combo, rest)
    }
}

#[cfg(test)]
mod tests {
    use super::{Basis, Bits, Form};

    #[test]
    fn a_basis_finds_dependent_forms_and_expresses_others() {
        let mut basis = Basis::default();
        let a = Form::var(0).plus(&Form::var(1));
        let b = Form::var(1).plus(&Form::constant(true));
        assert_eq!(basis.add(&a), None);
        assert_eq!(basis.add(&b), None);
        let c = Form::var(0);
        // var 0 = a + b over variables, with constant 1 left over.
        let (combo, rest) = basis.express(&c);
        assert_eq!(combo.ones().collect::<Vec<_>>(), [0, 1]);
        assert!(rest.is_constant() && rest.constant);
        let dependent = basis.add(&c).expect("var 0 depends on a and b");
        assert_eq!(dependent.0.ones().collect::<Vec<_>>(), [0, 1]);
        let mut bits = Bits::default();
        bits.set(129, true);
        bits.set(3, true);
        assert_eq!(bits.ones().collect::<Vec<_>>(), [3, 129]);
    }
}
