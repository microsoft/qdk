// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`PauliSum`]: the observable an [`MpsEngine`](crate::MpsEngine) takes an
//! expectation of, `O = c_I·I + Σₖ cₖ·Pₖ`, on sites rather than qubits.
//!
//! The crate owns this type because the engine sits below every simulator:
//! it cannot borrow a simulator's Pauli sum without depending on it. A
//! caller converts its own observable into this one at the boundary.

use num_complex::Complex64;

/// A single-site Pauli operator other than the identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pauli {
    /// σx.
    X,
    /// σy.
    Y,
    /// σz.
    Z,
}

impl Pauli {
    /// The operator as a row-major 2×2 matrix, `M[out][in]`, the layout
    /// [`MpsEngine::apply`](crate::MpsEngine::apply) takes.
    #[must_use]
    pub fn matrix(self) -> [Complex64; 4] {
        let (o, l, i) = (
            Complex64::new(0.0, 0.0),
            Complex64::new(1.0, 0.0),
            Complex64::new(0.0, 1.0),
        );
        match self {
            Self::X => [o, l, l, o],
            Self::Y => [o, -i, i, o],
            Self::Z => [l, o, o, -l],
        }
    }
}

/// Why a term or a sum was not built.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum PauliError {
    /// A term with no factor. An identity term is the sum's identity
    /// coefficient instead, so that no engine needs an identity operator.
    #[error("a Pauli term needs at least one non-identity factor")]
    NoFactors,
    /// A site named twice in one term; the product would not be a Pauli
    /// string on distinct sites.
    #[error("site {0} is named twice in one Pauli term")]
    RepeatedSite(usize),
    /// A coefficient whose real or imaginary part is NaN or infinite.
    #[error("the coefficient {0} is not finite")]
    NonFinite(Complex64),
}

/// One product `c·P_{s₁}⊗P_{s₂}⊗…` with at least one factor, on distinct
/// sites.
#[derive(Clone, Debug, PartialEq)]
pub struct PauliTerm {
    coefficient: Complex64,
    factors: Vec<(usize, Pauli)>,
}

impl PauliTerm {
    /// `coefficient·Π P` over `factors`, each a (site, Pauli) pair.
    pub fn new(coefficient: Complex64, factors: Vec<(usize, Pauli)>) -> Result<Self, PauliError> {
        if !(coefficient.re.is_finite() && coefficient.im.is_finite()) {
            return Err(PauliError::NonFinite(coefficient));
        }
        if factors.is_empty() {
            return Err(PauliError::NoFactors);
        }
        if let Some(&(site, _)) = factors
            .iter()
            .enumerate()
            .find_map(|(k, f)| factors[..k].iter().any(|e| e.0 == f.0).then_some(f))
        {
            return Err(PauliError::RepeatedSite(site));
        }
        Ok(Self {
            coefficient,
            factors,
        })
    }

    /// The term's coefficient.
    #[must_use]
    pub fn coefficient(&self) -> Complex64 {
        self.coefficient
    }

    /// The factors, as (site, Pauli), in the order they were given.
    #[must_use]
    pub fn factors(&self) -> &[(usize, Pauli)] {
        &self.factors
    }
}

/// A validated Pauli sum `O = c_I·I + Σₖ cₖ·Pₖ`. The coefficients are
/// complex, so a non-Hermitian `O` (σ⁺ = (X + iY)/2) has a complex ⟨O⟩;
/// each Pauli string is Hermitian, so `⟨O⟩ = c_I + Σₖ cₖ·⟨Pₖ⟩` with every
/// ⟨Pₖ⟩ real.
#[derive(Clone, Debug, PartialEq)]
pub struct PauliSum {
    identity: Complex64,
    terms: Vec<PauliTerm>,
}

impl PauliSum {
    /// `identity·I + Σ terms`.
    pub fn new(identity: Complex64, terms: Vec<PauliTerm>) -> Result<Self, PauliError> {
        if !(identity.re.is_finite() && identity.im.is_finite()) {
            return Err(PauliError::NonFinite(identity));
        }
        Ok(Self { identity, terms })
    }

    /// The identity's coefficient, `c_I`.
    #[must_use]
    pub fn identity(&self) -> Complex64 {
        self.identity
    }

    /// The terms with at least one factor, in the order they were given.
    #[must_use]
    pub fn terms(&self) -> &[PauliTerm] {
        &self.terms
    }

    /// The largest site any factor acts on; `None` for `c_I·I` alone.
    #[must_use]
    pub fn max_site(&self) -> Option<usize> {
        self.terms
            .iter()
            .flat_map(|t| t.factors.iter().map(|&(site, _)| site))
            .max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_term_is_a_product_on_distinct_sites_with_a_finite_coefficient() {
        let one = Complex64::new(1.0, 0.0);
        assert_eq!(PauliTerm::new(one, vec![]), Err(PauliError::NoFactors));
        assert_eq!(
            PauliTerm::new(one, vec![(0, Pauli::X), (2, Pauli::Z), (0, Pauli::Y)]),
            Err(PauliError::RepeatedSite(0))
        );
        let nan = Complex64::new(f64::NAN, 0.0);
        assert!(matches!(
            PauliTerm::new(nan, vec![(0, Pauli::X)]),
            Err(PauliError::NonFinite(_))
        ));
        assert!(matches!(
            PauliSum::new(Complex64::new(0.0, f64::INFINITY), vec![]),
            Err(PauliError::NonFinite(_))
        ));
        let term = PauliTerm::new(one, vec![(3, Pauli::Y), (1, Pauli::X)]);
        let sum = PauliSum::new(one, vec![term.clone().unwrap_or_else(|e| panic!("{e}"))]);
        assert_eq!(sum.map(|s| s.max_site()), Ok(Some(3)));
        assert_eq!(PauliSum::new(one, vec![]).map(|s| s.max_site()), Ok(None));
    }
}
