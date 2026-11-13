// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): the reads, as `qdk_cpu_mps`'s
//! `engine/reads.rs`, on host copies of the sites they need (their pending
//! gates applied): a site's probabilities and its projection at the
//! centre, and expectations.
//!
//! An expectation contracts every term between the centre and the term's
//! sites, as the CPU engine's, divided by the centre's weight. It shares
//! the work between terms, because the polyene's readings hold hundreds of
//! two-site terms and one contraction per term would cost minutes at large
//! bonds:
//! - the environments of the plain chain (left of a term's first site,
//!   right of its last) are computed once;
//! - the terms are walked as a trie of their sorted factors, so terms with
//!   the same first factors share their walk;
//! - a term's last factor is closed by a right environment with its Pauli,
//!   computed once per site and Pauli.

use super::device_chain::{DeviceChain, HostSite};
use super::{CudaMpsEngine, CudaMpsError, TensorCalls};
use num_complex::Complex64;
use std::collections::BTreeMap;
use std::time::Instant;
use t4a_faer::{Accum, MatMut, MatRef, Par, linalg::matmul::matmul};
use tensornet::{Pauli, PauliSum, Refusal};

impl<T: TensorCalls> CudaMpsEngine<T> {
    /// P(Zero) and P(One) of `site`, from its weight at the centre.
    pub(super) fn probabilities(
        &self,
        chain: &mut DeviceChain<T::Buffer>,
        site: usize,
    ) -> Result<[f64; 2], CudaMpsError> {
        self.centre_on(chain, site)?;
        let started = Instant::now();
        let centre = self.host_site(chain, site)?;
        let [zero, one] = [centre.weight_of(0), centre.weight_of(1)];
        self.count_read(started);
        let total = zero + one;
        Ok([zero / total, one / total])
    }

    /// Projects `site` on `one` and normalises the state, as
    /// `qdk_cpu_mps`'s `project_on`: the probability of the outcome, or
    /// [`Refusal::ZeroProbabilityOutcome`].
    pub(super) fn project_on(
        &self,
        chain: &mut DeviceChain<T::Buffer>,
        site: usize,
        one: bool,
    ) -> Result<f64, CudaMpsError> {
        let level = usize::from(one);
        self.centre_on(chain, site)?;
        let started = Instant::now();
        let mut centre = self.host_site(chain, site)?;
        let [zero, one] = [centre.weight_of(0), centre.weight_of(1)];
        let p = [zero, one][level] / (zero + one);
        if p == 0.0 || p.is_nan() {
            return Err(Refusal::ZeroProbabilityOutcome.into());
        }
        centre.project(level);
        chain.sites[site].buffer = self.tensors.upload(&centre.elements)?;
        chain.pending[site] = None;
        self.count_read(started);
        Ok(p)
    }

    /// `⟨ψ|O|ψ⟩ / ⟨ψ|ψ⟩`, as `qdk_cpu_mps`'s `expectation_of`.
    pub(super) fn expectation_of(
        &self,
        chain: &mut DeviceChain<T::Buffer>,
        observable: &PauliSum,
    ) -> Result<Complex64, CudaMpsError> {
        let centre = chain.centre;
        self.split_pair_toward(chain, centre)?;
        let mut value = observable.identity_coefficient();
        if observable.terms().is_empty() {
            return Ok(value);
        }
        let started = Instant::now();
        let centre = chain.centre;
        let terms: Vec<(Complex64, Vec<(usize, Pauli)>)> = observable
            .terms()
            .iter()
            .map(|term| {
                let mut factors = term.factors().to_vec();
                factors.sort_by_key(|&(site, _)| site);
                (term.coefficient(), factors)
            })
            .collect();
        let sites = terms
            .iter()
            .flat_map(|(_, factors)| factors.iter().map(|&(s, _)| s));
        let low = sites.clone().fold(centre, usize::min);
        let high = sites.fold(centre, usize::max);
        let hosts = (low..=high)
            .map(|at| self.host_site(chain, at))
            .collect::<Result<Vec<_>, _>>()?;
        let walk = Walk::new(&hosts, low, centre, &terms);
        let norm = walk.site(centre).weight();
        let items: Vec<Term<'_>> = terms
            .iter()
            .map(|(coefficient, factors)| (*coefficient, factors.as_slice()))
            .collect();
        let mut by_first: BTreeMap<usize, Vec<Term<'_>>> = BTreeMap::new();
        for item in items {
            by_first.entry(item.1[0].0).or_default().push(item);
        }
        for (first, group) in by_first {
            let start = walk.left_environment(first);
            value += walk.descend(start, first, &group) / norm;
        }
        self.count_read(started);
        Ok(value)
    }

    /// The site `at` on the host, its pending gate applied.
    pub(super) fn host_site(
        &self,
        chain: &DeviceChain<T::Buffer>,
        at: usize,
    ) -> Result<HostSite, CudaMpsError> {
        let site = &chain.sites[at];
        let mut host = HostSite {
            left: site.left,
            right: site.right,
            elements: self.tensors.download(&site.buffer, site.len())?,
        };
        if let Some(gate) = chain.pending[at] {
            host.apply(&gate);
        }
        Ok(host)
    }

    fn count_read(&self, started: Instant) {
        let seconds = started.elapsed().as_secs_f64();
        self.count(|counters| counters.read_seconds += seconds);
    }
}

/// An environment on a bond, `E[bra, ket]` column-major, or the identity
/// (`None`), which the canonical form gives left of the centre's left bond
/// and right of its right bond.
type Environment = Option<Vec<Complex64>>;

/// A term's coefficient and its factors, sorted by site.
type Term<'t> = (Complex64, &'t [(usize, Pauli)]);

/// The expectation's walk over the sites `low..=high`, with the plain
/// chain's environments and the closing environments computed once.
struct Walk<'s> {
    sites: &'s [HostSite],
    low: usize,
    centre: usize,
    /// `left[s − low]`: the environment on site `s`'s left bond, from the
    /// left.
    left: Vec<Environment>,
    /// The environment on site `s`'s left bond, from the right, with the
    /// Pauli `P` on `s`, for each last factor `(s, P)`, stored transposed
    /// (`R[ket, bra]`).
    closing: BTreeMap<(usize, u8), Vec<Complex64>>,
}

fn pauli_key(pauli: Pauli) -> u8 {
    match pauli {
        Pauli::X => 0,
        Pauli::Y => 1,
        Pauli::Z => 2,
    }
}

impl<'s> Walk<'s> {
    fn new(
        sites: &'s [HostSite],
        low: usize,
        centre: usize,
        terms: &[(Complex64, Vec<(usize, Pauli)>)],
    ) -> Self {
        let high = low + sites.len() - 1;
        let mut walk = Self {
            sites,
            low,
            centre,
            left: Vec::with_capacity(sites.len()),
            closing: BTreeMap::new(),
        };
        let last_first = terms
            .iter()
            .map(|(_, factors)| factors[0].0)
            .max()
            .expect("a term");
        for at in low..=high {
            let environment = if at <= centre || at > last_first {
                None
            } else {
                let previous = walk.left[at - 1 - low].as_deref();
                let site = walk.site(at - 1);
                Some(transfer_left(previous, site, site))
            };
            walk.left.push(environment);
        }
        // The plain environments from the right, on each site's right
        // bond, then the closing ones.
        let mut right: Vec<Environment> = vec![None; sites.len()];
        for at in (low..centre).rev() {
            let next = right[at + 1 - low].as_deref();
            right[at - low] = Some(transfer_right(next, walk.site(at + 1), walk.site(at + 1)));
        }
        for (_, factors) in terms {
            let &(at, pauli) = factors.last().expect("a factor");
            let key = (at, pauli_key(pauli));
            if walk.closing.contains_key(&key) {
                continue;
            }
            let site = walk.site(at);
            let ket = with_pauli(site, pauli);
            let closing = transfer_right(right[at - low].as_deref(), &ket, site);
            walk.closing.insert(key, closing);
        }
        walk
    }

    fn site(&self, at: usize) -> &'s HostSite {
        &self.sites[at - self.low]
    }

    fn left_environment(&self, at: usize) -> Environment {
        self.left[at - self.low].clone()
    }

    /// The sum of `coefficient · ⟨P…⟩` (unnormalised) over `items`, whose
    /// factors all lie at or right of `at`, `environment` on `at`'s left
    /// bond.
    fn descend(&self, environment: Environment, at: usize, items: &[Term<'_>]) -> Complex64 {
        let mut groups: BTreeMap<(usize, u8), (Pauli, Vec<Term<'_>>)> = BTreeMap::new();
        for &(coefficient, factors) in items {
            let (site, pauli) = factors[0];
            groups
                .entry((site, pauli_key(pauli)))
                .or_insert_with(|| (pauli, Vec::new()))
                .1
                .push((coefficient, factors));
        }
        let mut sum = Complex64::new(0.0, 0.0);
        let mut environment = environment;
        let mut position = at;
        for ((site, key), (pauli, group)) in groups {
            while position < site {
                // Left of the centre the sites are left-canonical: the
                // identity stays the identity.
                if environment.is_some() || position >= self.centre {
                    let plain = self.site(position);
                    environment = Some(transfer_left(environment.as_deref(), plain, plain));
                }
                position += 1;
            }
            let mut continuing = Vec::new();
            for (coefficient, factors) in group {
                if factors.len() == 1 {
                    let closing = &self.closing[&(site, key)];
                    sum += coefficient * close(environment.as_deref(), closing);
                } else {
                    continuing.push((coefficient, &factors[1..]));
                }
            }
            if !continuing.is_empty() {
                let plain = self.site(site);
                let ket = with_pauli(plain, pauli);
                let next = Some(transfer_left(environment.as_deref(), &ket, plain));
                sum += self.descend(next, site + 1, &continuing);
            }
        }
        sum
    }
}

fn with_pauli(site: &HostSite, pauli: Pauli) -> HostSite {
    let mut ket = HostSite {
        left: site.left,
        right: site.right,
        elements: site.elements.clone(),
    };
    ket.apply(&pauli.matrix());
    ket
}

/// `E'[b', b] = Σ conj(bra[a', s, b']) · E[a', a] · ket[a, s, b]`, on the
/// sites' right bond.
fn transfer_left(
    environment: Option<&[Complex64]>,
    ket: &HostSite,
    bra: &HostSite,
) -> Vec<Complex64> {
    let (l, r) = (bra.left, bra.right);
    let x = match environment {
        Some(environment) => product(
            MatRef::from_column_major_slice(environment, l, l),
            MatRef::from_column_major_slice(&ket.elements, l, 2 * r),
        ),
        None => ket.elements.clone(),
    };
    let bra_adjoint = adjoint(&bra.elements, 2 * l, r);
    product(
        MatRef::from_column_major_slice(&bra_adjoint, r, 2 * l),
        MatRef::from_column_major_slice(&x, 2 * l, r),
    )
}

/// The transposed right environment `R'[a, a'] = Σ ket[a, s, b] ·
/// R[b, b'] · conj(bra[a', s, b'])`, on the sites' left bond.
fn transfer_right(
    environment: Option<&[Complex64]>,
    ket: &HostSite,
    bra: &HostSite,
) -> Vec<Complex64> {
    let (l, r) = (bra.left, bra.right);
    let y = match environment {
        Some(environment) => product(
            MatRef::from_column_major_slice(&ket.elements, 2 * l, r),
            MatRef::from_column_major_slice(environment, r, r),
        ),
        None => ket.elements.clone(),
    };
    let bra_adjoint = adjoint(&bra.elements, l, 2 * r);
    product(
        MatRef::from_column_major_slice(&y, l, 2 * r),
        MatRef::from_column_major_slice(&bra_adjoint, 2 * r, l),
    )
}

/// `tr(E · R)` on one bond.
fn close(environment: Option<&[Complex64]>, closing: &[Complex64]) -> Complex64 {
    let n = closing.len().isqrt();
    match environment {
        Some(environment) => (0..n)
            .flat_map(|i| (0..n).map(move |j| (i, j)))
            .map(|(i, j)| environment[i + n * j] * closing[j + n * i])
            .sum(),
        None => (0..n).map(|i| closing[i + n * i]).sum(),
    }
}

/// The conjugate transpose of a column-major `rows × cols` matrix.
fn adjoint(elements: &[Complex64], rows: usize, cols: usize) -> Vec<Complex64> {
    let mut transposed = Vec::with_capacity(elements.len());
    for row in 0..rows {
        transposed.extend((0..cols).map(|col| elements[row + rows * col].conj()));
    }
    transposed
}

fn product(a: MatRef<'_, Complex64>, b: MatRef<'_, Complex64>) -> Vec<Complex64> {
    let (rows, cols) = (a.nrows(), b.ncols());
    let mut elements = vec![Complex64::new(0.0, 0.0); rows * cols];
    matmul(
        MatMut::from_column_major_slice_mut(&mut elements, rows, cols),
        Accum::Replace,
        a,
        b,
        Complex64::new(1.0, 0.0),
        Par::Seq,
    );
    elements
}
