// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The environments a term's value is made of: [`LeftEnvironment`], the
//! sites left of a bond; [`RightEnvironment`], the plain sites right of
//! one; and [`ClosingEnvironment`], a right one that holds a term's last
//! factor. The [module doc](super) says what an environment is.
//!
//! An environment grows by one site at a time. The site's ket, with the
//! term's Pauli on its qubit (`P` below; none on a *plain* site), and its
//! bra, the conjugate, are contracted into the environment over the bond
//! they share and over the qubit. What is left is a matrix on the site's
//! other bond:
//!
//! ```text
//!  from the left (E[bra, ket])          from the right (R[ket, bra])
//!
//!  ┌──── A ───      ┌───                ─── A ────┐      ───┐
//!  │     P          │                       P     │         │
//!  E     │     =    E'                      │     R   =     R'
//!  │     │          │                       │     │         │
//!  └──── A* ──      └───                ─── A* ───┘      ───┘
//! ```
//!
//! A term's value joins its left environment and its closing one on the
//! bond between them, the ket index to the ket index and the bra index to
//! the bra index: `tr(E·R)`.
//!
//! An environment is `None` where it is the identity, so no identity
//! matrix is built or multiplied: the plain sites between an end of the
//! chain and the centre contract to it.
//!
//! [`HostSpan`], the sites the expectation reads, builds the environments
//! every term shares: the plain ones from the left and from the right, and
//! one closing environment per last factor.

use super::term::{Factor, Term};
use crate::engine::host_site::HostSite;
use dense_linalg::{Matrix, MatrixRef};
use num_complex::Complex64;
use std::borrow::Cow;
use std::collections::BTreeMap;
use tensornet::Pauli;

/// The sites left of a bond, each with its Pauli if any, contracted with
/// their bra copies: `E[bra, ket]` on the bond, column-major.
#[derive(Clone)]
pub(super) struct LeftEnvironment {
    /// The site right of the bond: the next one the environment grows over.
    next_site: usize,
    /// `E`, or `None` for the identity.
    matrix: Option<Vec<Complex64>>,
}

/// The plain sites right of a bond, contracted with their bra copies:
/// `R[ket, bra]` on the bond, column-major (`E`'s transpose), or `None`
/// for the identity.
#[derive(Clone)]
struct RightEnvironment(Option<Vec<Complex64>>);

/// The environment that closes every term ending with one factor: the
/// plain sites right of the factor's site, and the factor's site with its
/// Pauli, as `R[ket, bra]` on that site's left bond, column-major.
pub(super) struct ClosingEnvironment(Vec<Complex64>);

/// The span's sites, read on the host, from its `lowest_site`: what the
/// environments are built from.
pub(super) struct HostSpan<'h> {
    pub(super) lowest_site: usize,
    pub(super) sites: Vec<HostSite<'h>>,
}

impl LeftEnvironment {
    /// The identity on `next_site`'s left bond: what the sites left of the
    /// centre's left bond contract to, isometries from the left. Also the
    /// placeholder stored where no walk starts.
    pub(super) fn identity_before(next_site: usize) -> Self {
        Self {
            next_site,
            matrix: None,
        }
    }

    /// The site right of the environment's bond.
    pub(super) fn next_site(&self) -> usize {
        self.next_site
    }

    /// The environment grown over its next site, read from `span`, with no
    /// Pauli.
    pub(super) fn grown_over(&self, span: &HostSpan<'_>) -> Self {
        let site = span.site(self.next_site);
        Self {
            next_site: self.next_site + 1,
            matrix: Some(grow_from_the_left(self.matrix.as_deref(), site, site)),
        }
    }

    /// The environment grown over its next site, read from `span`, with
    /// `pauli` on its ket.
    pub(super) fn grown_over_with_pauli(&self, span: &HostSpan<'_>, pauli: Pauli) -> Self {
        let site = span.site(self.next_site);
        let ket = with_pauli(site, pauli);
        Self {
            next_site: self.next_site + 1,
            matrix: Some(grow_from_the_left(self.matrix.as_deref(), &ket, site)),
        }
    }

    /// The value of a term whose factors are this environment's and
    /// `closing`'s, on the same bond: `tr(E·R)`, not yet divided by the
    /// squared norm.
    pub(super) fn closed_by(&self, closing: &ClosingEnvironment) -> Complex64 {
        let closing = &closing.0;
        let bond_size = closing.len().isqrt();
        match &self.matrix {
            Some(environment) => (0..bond_size)
                .flat_map(|i| (0..bond_size).map(move |j| (i, j)))
                .map(|(i, j)| environment[i + bond_size * j] * closing[j + bond_size * i])
                .sum(),
            None => (0..bond_size).map(|i| closing[i + bond_size * i]).sum(),
        }
    }
}

impl RightEnvironment {
    /// The identity: what the sites right of the centre's right bond
    /// contract to, isometries from the right.
    fn identity() -> Self {
        Self(None)
    }

    /// The environment grown over `site`, the site left of its bond, with
    /// no Pauli.
    fn grown_over(&self, site: &HostSite<'_>) -> Self {
        Self(Some(grow_from_the_right(self.0.as_deref(), site, site)))
    }

    /// The environment grown over `site`, the site left of its bond, with
    /// `pauli` on its ket: it closes the terms ending with `pauli` on
    /// `site`.
    fn closing_over(&self, site: &HostSite<'_>, pauli: Pauli) -> ClosingEnvironment {
        let ket = with_pauli(site, pauli);
        ClosingEnvironment(grow_from_the_right(self.0.as_deref(), &ket, site))
    }
}

impl<'h> HostSpan<'h> {
    /// The site `at`, which the span holds.
    pub(super) fn site(&self, at: usize) -> &HostSite<'h> {
        &self.sites[at - self.lowest_site]
    }

    /// The plain chain's environment from the left on each site's left
    /// bond. It is stored as the identity in two places:
    ///
    /// - up to the centre, where it is the identity: the sites there are
    ///   isometries from the left;
    /// - beyond `last_starting_site`, the last site a term starts on, as a
    ///   placeholder: no walk starts there, so it is never read.
    pub(super) fn plain_environments_from_the_left(
        &self,
        centre: usize,
        last_starting_site: usize,
    ) -> Vec<LeftEnvironment> {
        let mut environments: Vec<LeftEnvironment> = Vec::with_capacity(self.sites.len());
        for at in self.lowest_site..self.lowest_site + self.sites.len() {
            let environment = if at <= centre || at > last_starting_site {
                LeftEnvironment::identity_before(at)
            } else {
                environments[at - 1 - self.lowest_site].grown_over(self)
            };
            environments.push(environment);
        }
        environments
    }

    /// For each last factor of the `terms`, the environment that closes
    /// every term ending with it: the plain environment from the right on
    /// the factor site's right bond, grown over that site with the factor's
    /// Pauli.
    pub(super) fn closing_environments(
        &self,
        centre: usize,
        terms: &[Term],
    ) -> BTreeMap<Factor, ClosingEnvironment> {
        let from_the_right = self.plain_environments_from_the_right(centre);
        let mut closing = BTreeMap::new();
        for term in terms {
            let factor = term.last_factor();
            closing.entry(factor).or_insert_with(|| {
                from_the_right[factor.site - self.lowest_site]
                    .closing_over(self.site(factor.site), factor.pauli)
            });
        }
        closing
    }

    /// The plain chain's environment from the right on each site's right
    /// bond. It is the identity from the centre on, where the sites are
    /// isometries from the right.
    fn plain_environments_from_the_right(&self, centre: usize) -> Vec<RightEnvironment> {
        let mut environments = vec![RightEnvironment::identity(); self.sites.len()];
        for at in (self.lowest_site..centre).rev() {
            environments[at - self.lowest_site] =
                environments[at + 1 - self.lowest_site].grown_over(self.site(at + 1));
        }
        environments
    }
}

/// A copy of `site` with `pauli` applied to its qubit.
fn with_pauli(site: &HostSite<'_>, pauli: Pauli) -> HostSite<'static> {
    let mut ket = HostSite {
        left: site.left,
        right: site.right,
        elements: Cow::Owned(site.elements.to_vec()),
    };
    ket.apply(&pauli.matrix());
    ket
}

/// `E'[b', b] = Σ conj(bra[a', s, b']) · E[a', a] · ket[a, s, b]`, on the
/// sites' right bond: `bra† · (E · ket)`, with `ket` as an `l × 2r`
/// matrix, then both sites as `2l × r` ones (`l` and `r` the sizes of the
/// site's left and right bonds).
fn grow_from_the_left(
    environment: Option<&[Complex64]>,
    ket: &HostSite<'_>,
    bra: &HostSite<'_>,
) -> Vec<Complex64> {
    let (l, r) = (bra.left, bra.right);
    let owned_product;
    let ket_with_environment: &[Complex64] = match environment {
        Some(environment) => {
            owned_product = Matrix::product(
                MatrixRef::new(l, l, environment),
                MatrixRef::new(l, 2 * r, &ket.elements),
            );
            owned_product.elements()
        }
        None => &ket.elements,
    };
    let bra_adjoint = MatrixRef::new(2 * l, r, &bra.elements).adjoint();
    Matrix::product(
        bra_adjoint.view(),
        MatrixRef::new(2 * l, r, ket_with_environment),
    )
    .into_elements()
}

/// `R'[a, a'] = Σ ket[a, s, b] · R[b, b'] · conj(bra[a', s, b'])`, on the
/// sites' left bond: `(ket · R) · bra†`, with `ket` as a `2l × r` matrix,
/// then both sites as `l × 2r` ones (`l` and `r` the sizes of the site's
/// left and right bonds).
fn grow_from_the_right(
    environment: Option<&[Complex64]>,
    ket: &HostSite<'_>,
    bra: &HostSite<'_>,
) -> Vec<Complex64> {
    let (l, r) = (bra.left, bra.right);
    let owned_product;
    let ket_with_environment: &[Complex64] = match environment {
        Some(environment) => {
            owned_product = Matrix::product(
                MatrixRef::new(2 * l, r, &ket.elements),
                MatrixRef::new(r, r, environment),
            );
            owned_product.elements()
        }
        None => &ket.elements,
    };
    let bra_adjoint = MatrixRef::new(l, 2 * r, &bra.elements).adjoint();
    Matrix::product(
        MatrixRef::new(l, 2 * r, ket_with_environment),
        bra_adjoint.view(),
    )
    .into_elements()
}
