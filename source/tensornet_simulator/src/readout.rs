// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The readout distribution: the exact joint distribution of each run of
//! consecutive measurements, read from the shot's state as the terminal
//! draws the run's values.
//!
//! For a run of k measurements of distinct qubits with no gate or reset
//! between them, the joint distribution of their values is
//! P(r) = ⟨Π(r)⟩ on the state before the run, with Π(r) the product of the k
//! projectors. What the program does after the run cannot change values
//! already read, so the terminal may read this distribution and still draw
//! the run's values from it, one at a time as the walk asks for them, and
//! project: the shot's values and its output keep their distribution. Over
//! shots, the mean of each shot's P(r | its history) estimates the
//! distribution of the run with a variance no larger than counting the
//! rows drawn (Rao–Blackwell), and a program whose shots all reach the run
//! in the same state (H₂ QPE) gives it exactly from one shot.
//!
//! The walk sends each measurement alone, because it needs its value
//! before it goes on, so the terminal does not see a run ahead: it holds
//! the run's projections back, reads each new measurement's P(One) given
//! every row of the run so far ([`MpsEngine::conditional_probability_of_one`],
//! which on cuTensorNet contracts projected sites without copying the
//! state), and projects when the run ends.

use qdk_simulators::QubitID;
use rand::{RngExt as _, rngs::StdRng};
use tensornet::MpsEngine;

use crate::lowering;

/// Whether a terminal reads the exact distribution of each run of
/// consecutive measurements, besides drawing their values.
///
/// Either way the terminal draws each value from the same random numbers
/// with the same probability (to rounding: with the distribution read,
/// P(One) is read on the state before the run, given the run's earlier
/// values, instead of on the state projected on them), so the values of a
/// shot do not depend on it.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ReadoutDistribution {
    /// Each measurement is drawn and projected at once; nothing more is
    /// read. The default.
    #[default]
    Off,
    /// Each run of consecutive measurements is read exactly: the terminal
    /// keeps a [`MeasurementRun`] per run, with the joint distribution of
    /// its values given the shot's history
    /// ([`TensorNetTerminal::measurement_runs`](crate::TensorNetTerminal::measurement_runs)).
    ///
    /// A run is the measurements of distinct qubits that follow one another
    /// with no gate (a region the fusion turns into no operator does not
    /// count) and no reset between them, at most [`MAX_RUN`] of them, and
    /// only while the row drawn so far has a probability of at least
    /// [`SMALLEST_CONDITION`]; the measurement that does not fit starts the
    /// next run. A run of one measurement is its P(One).
    ///
    /// It costs 2^k − 1 reads of P(One) for a run of k measurements instead
    /// of k, and 2^k numbers kept per run, for every run of the shot. The
    /// state is checked within its capacity once a run is projected, so
    /// on the states [`Off`](Self::Off) checks.
    Exact,
}

/// The most measurements of one run: a run of k costs 2^k − 1 reads of
/// P(One). Ten is 1,023 reads, fewer than the 1,404 measurements of one H₂
/// shot, each of which reads P(One) once, so the longest run at most about
/// doubles the reads of such a shot; and H₂'s readout, the run M1's
/// acceptance reads, is five.
pub const MAX_RUN: usize = 10;

/// The smallest probability of a row of a run on which the terminal reads a
/// conditional P(One).
///
/// A conditional probability is a ratio of two reads, ⟨Π·P₁⟩ / ⟨Π⟩, whose
/// relative precision falls as ⟨Π⟩ does: cuTensorNet's engine refuses a
/// condition of 10⁻¹² or less as below that precision. This floor stays
/// two decades above it. A row of the distribution below it is not refined
/// further: its probability is split evenly between its two extensions,
/// which moves the distribution by at most half of that probability per
/// such row (at most 2^(k−1) · 10⁻¹⁰ in total). A run also ends before a
/// measurement when the row drawn so far is below it, so the values drawn
/// never need such a read.
pub const SMALLEST_CONDITION: f64 = 1e-10;

/// One run of consecutive measurements of a shot, as a terminal that reads
/// the readout distribution ([`ReadoutDistribution::Exact`]) drew it.
///
/// Its measurements are in the order the shot made them; row `r` of
/// [`distribution`](Self::distribution) gives measurement `i` the value of
/// bit `k − 1 − i` of `r`, so `r` written in binary with `k` digits is the
/// row's values in that order (the first measurement first).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeasurementRun {
    tags: Vec<u32>,
    qubits: Vec<QubitID>,
    sites: Vec<usize>,
    values: Vec<bool>,
    probabilities_of_one: Vec<f64>,
    resets: Vec<bool>,
    distribution: Vec<f64>,
}

impl MeasurementRun {
    fn new() -> Self {
        Self {
            distribution: vec![1.0],
            ..Self::default()
        }
    }

    /// The result tag of each measurement (its outcome's tag, the
    /// program's result id).
    #[must_use]
    pub fn tags(&self) -> &[u32] {
        &self.tags
    }

    /// The qubit of each measurement.
    #[must_use]
    pub fn qubits(&self) -> &[QubitID] {
        &self.qubits
    }

    /// The site of each measurement's qubit, where the terminal read and
    /// projected the state: the qubit's site by the terminal's
    /// [`QubitOrder`](crate::QubitOrder), the qubit itself by default.
    #[must_use]
    pub fn sites(&self) -> &[usize] {
        &self.sites
    }

    /// The value drawn for each measurement.
    #[must_use]
    pub fn values(&self) -> &[bool] {
        &self.values
    }

    /// The P(One) each measurement was drawn with: given the shot's history
    /// and the values drawn before it in the run, as a terminal that
    /// projects at once reads it.
    #[must_use]
    pub fn probabilities_of_one(&self) -> &[f64] {
        &self.probabilities_of_one
    }

    /// The joint distribution of the run's values given the shot's history
    /// before the run: 2^k rows, summing to 1 to rounding.
    #[must_use]
    pub fn distribution(&self) -> &[f64] {
        &self.distribution
    }

    /// The row drawn: [`values`](Self::values) as an index of
    /// [`distribution`](Self::distribution).
    #[must_use]
    pub fn row(&self) -> usize {
        self.values
            .iter()
            .fold(0, |row, &one| (row << 1) | usize::from(one))
    }

    /// Its measurements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.qubits.len()
    }

    /// Whether it has no measurement; a terminal never keeps such a run.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.qubits.is_empty()
    }

    /// Whether a measurement of `qubit` continues this run.
    fn admits(&self, qubit: QubitID) -> bool {
        self.len() < MAX_RUN
            && !self.qubits.contains(&qubit)
            && self.distribution[self.row()] >= SMALLEST_CONDITION
    }

    /// The run's distribution extended by a measurement of the qubit on
    /// `site` (the last bit of each row), read on `state`, the state before
    /// the run; and P(One) given the row drawn so far, the one to draw it
    /// with.
    fn extended<E: MpsEngine>(
        &self,
        engine: &E,
        state: &E::State,
        site: usize,
    ) -> Result<(Vec<f64>, f64), E::Error> {
        let drawn = self.row();
        let mut drawn_p1 = 0.0;
        let mut distribution = Vec::with_capacity(2 * self.distribution.len());
        for (row, &p) in self.distribution.iter().enumerate() {
            let p1 = if self.is_empty() {
                engine.probability_of_one(state, site)?
            } else if p >= SMALLEST_CONDITION {
                engine.conditional_probability_of_one(state, site, &self.given(row))?
            } else {
                0.5
            }
            .clamp(0.0, 1.0);
            if row == drawn {
                drawn_p1 = p1;
            }
            distribution.extend([p * (1.0 - p1), p * p1]);
        }
        Ok((distribution, drawn_p1))
    }

    /// The values of row `row` of the run so far, as conditions on their
    /// sites.
    fn given(&self, row: usize) -> Vec<(usize, bool)> {
        let k = self.len();
        self.sites
            .iter()
            .enumerate()
            .map(|(i, &site)| (site, row >> (k - 1 - i) & 1 == 1))
            .collect()
    }
}

/// A terminal's measurement runs: how it reads them, those of the shot so
/// far, and whether the last one's projections are still to apply.
#[derive(Debug, Default)]
pub(crate) struct MeasurementRuns {
    distribution: ReadoutDistribution,
    runs: Vec<MeasurementRun>,
    /// Whether the last run is still open: its measurements are drawn, but
    /// the state is not yet projected on them.
    open: bool,
}

impl MeasurementRuns {
    pub(crate) fn new(distribution: ReadoutDistribution) -> Self {
        Self {
            distribution,
            ..Self::default()
        }
    }

    pub(crate) fn distribution(&self) -> ReadoutDistribution {
        self.distribution
    }

    pub(crate) fn runs(&self) -> &[MeasurementRun] {
        &self.runs
    }

    /// Whether a run is open: the state is not yet projected on its values.
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Whether a measurement of `qubit` ends the open run rather than
    /// continuing it.
    pub(crate) fn ends_at(&self, qubit: QubitID) -> bool {
        self.open && !self.continued_by(qubit)
    }

    fn continued_by(&self, qubit: QubitID) -> bool {
        self.open && self.runs.last().is_some_and(|run| run.admits(qubit))
    }

    /// Draws the value of a measurement of `qubit`, on `site`, into `tag`,
    /// in the open run if it continues it, or in a new one after closing
    /// it, and extends the run's distribution by it. The value and ln p of
    /// it. With [`ReadoutDistribution::Off`], it draws and projects at once.
    pub(crate) fn measure<E: MpsEngine>(
        &mut self,
        engine: &E,
        state: &mut E::State,
        rng: &mut StdRng,
        (qubit, site, tag): (QubitID, usize, u32),
        reset: bool,
    ) -> Result<(bool, f64), E::Error> {
        if self.distribution == ReadoutDistribution::Off {
            return draw(engine, state, rng, site, reset);
        }
        let continues = self.continued_by(qubit);
        let mut run = match self.runs.pop() {
            Some(run) if continues => run,
            last => {
                self.runs.extend(last);
                self.close(engine, state)?;
                MeasurementRun::new()
            }
        };
        match run.extended(engine, state, site) {
            Ok((distribution, p1)) => {
                // `random` is in [0, 1): One only when p1 > 0, Zero only when p1 < 1.
                let one = rng.random::<f64>() < p1;
                let p = if one { p1 } else { 1.0 - p1 };
                run.distribution = distribution;
                run.tags.push(tag);
                run.qubits.push(qubit);
                run.sites.push(site);
                run.values.push(one);
                run.probabilities_of_one.push(p1);
                run.resets.push(reset);
                self.runs.push(run);
                self.open = true;
                Ok((one, p.ln()))
            }
            Err(error) => {
                if !run.is_empty() {
                    self.runs.push(run);
                }
                Err(error)
            }
        }
    }

    /// Ends the open run, if there is one: projects the state on its values,
    /// in order, and flips the qubits its resets measured One back to |0⟩.
    /// Whether there was one. The terminal calls it before it applies a gate
    /// or a reset, before a measurement that ends the run, and at `close`.
    pub(crate) fn close<E: MpsEngine>(
        &mut self,
        engine: &E,
        state: &mut E::State,
    ) -> Result<bool, E::Error> {
        if !std::mem::take(&mut self.open) {
            return Ok(false);
        }
        let Some(run) = self.runs.last() else {
            return Ok(false);
        };
        let measurements = run
            .sites
            .iter()
            .zip(&run.values)
            .zip(&run.probabilities_of_one)
            .zip(&run.resets);
        for (((&site, &one), &p1), &reset) in measurements {
            engine.project(state, site, one, if one { p1 } else { 1.0 - p1 })?;
            if reset && one {
                lowering::flip(engine, state, site)?;
            }
        }
        Ok(true)
    }
}

/// Draws the value of the qubit on `site` from P(One) on `state`, projects
/// on it, and flips it back to |0⟩ when `reset` and it was One. The value,
/// and ln p of the value drawn.
pub(crate) fn draw<E: MpsEngine>(
    engine: &E,
    state: &mut E::State,
    rng: &mut StdRng,
    site: usize,
    reset: bool,
) -> Result<(bool, f64), E::Error> {
    let p1 = engine.probability_of_one(state, site)?.clamp(0.0, 1.0);
    // `random` is in [0, 1): One only when p1 > 0, Zero only when p1 < 1.
    let one = rng.random::<f64>() < p1;
    let p = if one { p1 } else { 1.0 - p1 };
    engine.project(state, site, one, p)?;
    if reset && one {
        lowering::flip(engine, state, site)?;
    }
    Ok((one, p.ln()))
}
