// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The batch strategy: one matrix product state for every shot of a
//! session, every shot sampled from it at once, as M0's ④ ran Base
//! programs.

use std::{collections::BTreeMap, rc::Rc};

use executor_protocol::{
    Capabilities, Close, Describe, Executor, Fold, Immediate, Lifecycle, Need, Needs, Rejected,
    SessionReport, ShotId, ShotOutcome, ShotSet, ShotStrategy, Submission, SubmissionRef, Unmet,
    Value,
};
use qdk_simulators::{
    QubitID,
    execution::{MeasurementKind, MeasurementRequest, RegionInstruction},
};
use rand::{RngExt as _, SeedableRng as _, rngs::StdRng};
use tensornet::{MpsEngine, MpsShape, MpsShapeError, SampleSeed, SeedOutOfRange};

use crate::{capacity::mps_capacity, lowering, terminal::outgrown};

/// The seed of a batch session's one [`MpsEngine::sample`] call, from the
/// session's seed, as M0's ④ derived it from the user's seed
/// (`derive_sampler_seed`): a draw in 1..=2³¹−1 of an [`StdRng`] seeded
/// with it. The draw is of an `i32`, as M0's, because the value drawn
/// depends on the integer type drawn; so a session seeded with the user's
/// seed samples with M0's sampler seed. The engine uses it for its
/// contraction path too, with M0's 8 hyper-samples (E24).
///
/// # Errors
///
/// None in fact: the draw is in [`SampleSeed`]'s range. The error is the
/// conversion's, kept so that no panic is written.
pub fn sampler_seed(seed: u64) -> Result<SampleSeed, SeedOutOfRange> {
    let drawn = StdRng::seed_from_u64(seed).random_range(1..=i32::MAX);
    SampleSeed::try_from(u64::from(drawn.unsigned_abs()))
}

/// M0's ④ as a shot strategy (A16): it evolves one state for all of a
/// session's shots J, and draws every shot's terminal measurements in one
/// [`MpsEngine::sample`] call, from one seed, as ④ called cuTensorNet's
/// sampler once for N shots.
///
/// A shot's values then depend on which other shots are drawn with it, so
/// it does not offer `replayable`. It runs Base programs only: each
/// submission on all of J, measurements only at the end of a batch and
/// nothing after them, so it offers neither `feedback` nor `resets`; and it
/// knows no shot's probability. Its capabilities say so, and
/// [`open`](ShotStrategy::open) refuses a caller that needs any of them
/// ([`Rejected::Unmet`]); a submission that would need feedback is refused
/// the same way.
///
/// M1's stack ([`MpsStack`](crate::MpsStack)) runs a Base program through
/// it as ④ did, and replays each shot's row through the program's walk.
pub struct MpsBatchSimulator<E: MpsEngine> {
    engine: Rc<E>,
    capacity: MpsShape,
}

impl<E: MpsEngine> MpsBatchSimulator<E> {
    /// The strategy for `qubits` qubits whose bonds are capped at `max_bond`
    /// (see [`mps_capacity`]), on `engine`.
    ///
    /// # Errors
    ///
    /// [`mps_capacity`]'s, when that is not a chain.
    pub fn new(engine: Rc<E>, qubits: usize, max_bond: usize) -> Result<Self, MpsShapeError> {
        Ok(Self {
            engine,
            capacity: mps_capacity(qubits, max_bond)?,
        })
    }

    /// The capacity a session's state is prepared within.
    #[must_use]
    pub fn capacity(&self) -> &MpsShape {
        &self.capacity
    }

    pub(crate) fn engine(&self) -> &Rc<E> {
        &self.engine
    }
}

impl<E: MpsEngine> Describe for MpsBatchSimulator<E> {
    /// Its qubits; neither feedback, nor resets, nor replayable shots, nor
    /// probabilities. One state for any number of shots, so no limit on
    /// states.
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
            .with_qubits(Some(
                u32::try_from(self.capacity.site_count()).unwrap_or(u32::MAX),
            ))
            .with_feedback(false)
            .with_resets(false)
            .with_replayable(false)
            .with_probabilities(false)
    }
}

impl<E: MpsEngine> ShotStrategy<RegionInstruction> for MpsBatchSimulator<E> {
    type Session = MpsBatchSession<E>;

    /// Checks `needs` against its capabilities, then prepares the one state
    /// of the session.
    ///
    /// # Errors
    ///
    /// [`Rejected::Unmet`] for a need it does not offer, naming this type,
    /// before anything is prepared; [`Rejected::Unavailable`] when the
    /// engine cannot prepare the state.
    fn open(&self, seed: u64, shots: &ShotSet, needs: &Needs) -> Result<Self::Session, Rejected> {
        let capabilities = self.capabilities();
        capabilities.admit(std::any::type_name::<Self>(), shots, needs)?;
        let seed = sampler_seed(seed).map_err(|error| {
            Rejected::Unavailable(format!(
                "unreachable: the sampler seed drawn is out of range ({error})"
            ))
        })?;
        let state = self
            .engine
            .prepare(&self.capacity)
            .map_err(|error| Rejected::Unavailable(error.to_string()))?;
        Ok(MpsBatchSession {
            engine: Rc::clone(&self.engine),
            capacity: self.capacity.clone(),
            capabilities,
            ids: shots.to_vec(),
            index: shots.iter().enumerate().map(|(i, &j)| (j, i)).collect(),
            state: Some(state),
            seed,
            sampled: false,
            report: MpsBatchReport::default(),
            submissions: 0,
            lifecycle: Lifecycle::default(),
        })
    }
}

/// A session of the batch strategy: one state for all of J, sampled once.
pub struct MpsBatchSession<E: MpsEngine> {
    engine: Rc<E>,
    capacity: MpsShape,
    capabilities: Capabilities,
    /// J, in the order the session was opened with: shot `ids[s]` gets row
    /// `s` of the sample, whatever the order of a submission's shots.
    ids: Vec<ShotId>,
    index: BTreeMap<ShotId, usize>,
    /// `None` once closed, or after a failed submission.
    state: Option<E::State>,
    seed: SampleSeed,
    sampled: bool,
    report: MpsBatchReport,
    submissions: u64,
    lifecycle: Lifecycle,
}

impl<E: MpsEngine> MpsBatchSession<E> {
    /// The session's state, as the submissions so far left it; `None` once
    /// closed or failed.
    #[must_use]
    pub fn state(&self) -> Option<&E::State> {
        self.state.as_ref()
    }

    /// The seed its one sample call uses ([`sampler_seed`]).
    #[must_use]
    pub fn sampler_seed(&self) -> SampleSeed {
        self.seed
    }

    /// Rejects, before anything runs, what one state for all of J cannot
    /// run; returns where the batch's terminal measurements start.
    fn check(&self, submission: &SubmissionRef<'_, RegionInstruction>) -> Result<usize, Rejected> {
        for &j in submission.shots {
            if !self.index.contains_key(&j) {
                return Err(Rejected::ShotNotInSession(j));
            }
        }
        // The shots are distinct (a ShotSet) and all of the session's, so
        // the same count is all of J.
        if submission.shots.len() != self.ids.len() || self.sampled {
            return Unmet::check(std::any::type_name::<Self>(), Need::Feedback, 1, 0).map(|()| 0);
        }
        let batch = submission.batch;
        let body = batch.len()
            - batch
                .iter()
                .rev()
                .take_while(|instruction| matches!(instruction, RegionInstruction::Measure(_)))
                .count();
        let qubits = self.capacity.site_count();
        let mut reset_by_measurement = Vec::new();
        for (at, instruction) in batch.iter().enumerate() {
            let reason = match instruction {
                RegionInstruction::ExecuteRegion { region_id, region } => {
                    lowering::check_region(region, qubits)
                        .map(|reason| format!("region {}: {reason}", region_id.value()))
                }
                RegionInstruction::Measure(_) if at < body => Some(
                    "a measurement before the end of the batch: the batch strategy samples \
                     only the measurements that end a Base program"
                        .to_owned(),
                ),
                RegionInstruction::Measure(request) => {
                    check_terminal_measurement(request, qubits, &mut reset_by_measurement)
                }
                RegionInstruction::Reset { qubit } => Some(format!(
                    "reset is not supported by MPS batch sampling (qubit {qubit})"
                )),
            };
            if let Some(reason) = reason {
                return Err(Rejected::Instruction { at, reason });
            }
        }
        Ok(body)
    }

    /// Runs a checked batch: its events, or the events so far and why it
    /// failed.
    fn run(
        &mut self,
        batch: &[RegionInstruction],
        body: usize,
    ) -> Result<Vec<ShotOutcome>, String> {
        let Some(state) = self.state.as_mut() else {
            // `Lifecycle::admit` refused a closed or failed session first.
            return Err("the session holds no state: it was closed or failed".to_owned());
        };
        let engine = &*self.engine;
        let (regions, measures) = batch.split_at(body);
        for instruction in regions {
            if let RegionInstruction::ExecuteRegion { region, .. } = instruction {
                for &operation in region.operations() {
                    lowering::apply_operation(engine, state, operation)
                        .map_err(|error| error.to_string())?;
                    self.report.operations += u64::from(lowering::is_applied(operation));
                }
                self.report.regions += 1;
            }
        }
        let bond = outgrown(engine, state, &self.capacity)?;
        self.report.max_bond = self.report.max_bond.max(bond);
        if measures.is_empty() {
            return Ok(Vec::new());
        }
        // A qubit measured twice is sampled once; both results read its
        // column, as M0's replay read one column per qubit.
        let mut sites: Vec<QubitID> = Vec::new();
        let columns: Vec<(u32, usize)> = measures
            .iter()
            .filter_map(|instruction| match instruction {
                RegionInstruction::Measure(request) => Some(request),
                _ => None,
            })
            .filter_map(|request| {
                let column = sites
                    .iter()
                    .position(|&site| site == request.qubit)
                    .unwrap_or_else(|| {
                        sites.push(request.qubit);
                        sites.len() - 1
                    });
                // The check admitted only tags that fit.
                u32::try_from(request.result_id)
                    .ok()
                    .map(|tag| (tag, column))
            })
            .collect();
        let values = engine
            .sample(state, &sites, self.ids.len(), self.seed)
            .map_err(|error| error.to_string())?;
        self.sampled = true;
        self.report.sampled_shots += self.ids.len() as u64;
        self.report.measurements += (columns.len() * self.ids.len()) as u64;
        let k = sites.len();
        let mut events = Vec::with_capacity(columns.len() * self.ids.len());
        for (s, &shot) in self.ids.iter().enumerate() {
            for &(tag, column) in &columns {
                let Some(&one) = values.get(s * k + column) else {
                    return Err(format!(
                        "the engine sampled {} values, not {} shots of {k} sites",
                        values.len(),
                        self.ids.len()
                    ));
                };
                events.push(ShotOutcome {
                    shot,
                    tag,
                    value: Value::from(one),
                });
            }
        }
        Ok(events)
    }
}

/// Why a terminal measurement cannot be sampled: a qubit beyond the state, a
/// result that does not fit a tag, or a measurement of a qubit that a
/// `MeasureResetZ` before it reset (M0's replay refused it too).
fn check_terminal_measurement(
    request: &MeasurementRequest,
    qubits: usize,
    reset_by_measurement: &mut Vec<QubitID>,
) -> Option<String> {
    if request.qubit >= qubits {
        return Some(format!(
            "qubit {} is beyond the session's {qubits} qubits",
            request.qubit
        ));
    }
    if u32::try_from(request.result_id).is_err() {
        return Some(format!(
            "result {} does not fit an outcome's tag",
            request.result_id
        ));
    }
    if reset_by_measurement.contains(&request.qubit) {
        return Some(format!(
            "MPS batch sampling cannot replay a measurement of qubit {} after MeasureResetZ",
            request.qubit
        ));
    }
    if request.kind == MeasurementKind::MeasureResetZ {
        reset_by_measurement.push(request.qubit);
    }
    None
}

impl<E: MpsEngine> Executor<Submission<RegionInstruction>> for MpsBatchSession<E> {
    type Response = Immediate<ShotOutcome>;

    /// Applies the batch's regions to the one state, then samples its
    /// terminal measurements for every shot of J at once. Events come by
    /// shot, in J's order, then in the batch's order of measurements.
    ///
    /// # Errors
    ///
    /// [`Rejected::Closed`] and [`Rejected::SessionFailed`] first;
    /// [`Rejected::ShotNotInSession`]; [`Rejected::Unmet`] (`Feedback`, which
    /// it does not offer) for a submission on part of J or after the shots
    /// were sampled; [`Rejected::Instruction`] for a measurement before the
    /// end of the batch, a reset, a qubit beyond the state, an operation that
    /// cannot be applied, a result that does not fit a tag, or a measurement
    /// after `MeasureResetZ` of the same qubit. Nothing ran in each case. A
    /// failure of the engine fails the submission and the session, whose
    /// state is dropped.
    fn call(
        &mut self,
        submission: SubmissionRef<'_, RegionInstruction>,
    ) -> Result<Self::Response, Rejected> {
        self.lifecycle.admit()?;
        let body = self.check(&submission)?;
        self.submissions += 1;
        match self.run(submission.batch, body) {
            Ok(events) => Ok(Immediate::done(events)),
            Err(reason) => {
                self.lifecycle.fail(&reason);
                self.state = None;
                Ok(Immediate::failed(Vec::new(), reason))
            }
        }
    }
}

impl<E: MpsEngine> Describe for MpsBatchSession<E> {
    /// The strategy's capabilities.
    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }
}

impl<E: MpsEngine> Close for MpsBatchSession<E> {
    type Report = SessionReport<MpsBatchReport>;

    /// Ends the session and drops its state. No shot's probability is
    /// known, so the report's `log_p` is empty.
    fn close(&mut self) -> Self::Report {
        self.lifecycle.close();
        self.state = None;
        SessionReport {
            submissions: std::mem::take(&mut self.submissions),
            shots: std::mem::take(&mut self.report),
            ..SessionReport::default()
        }
    }
}

/// What a batch session ran on its one state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MpsBatchReport {
    /// The regions applied.
    pub regions: u64,
    /// The operations of those regions applied (`I` is not applied).
    pub operations: u64,
    /// The shots sampled.
    pub sampled_shots: u64,
    /// The measurement results sampled, over every shot.
    pub measurements: u64,
    /// The largest bond of the state after a submission's regions.
    pub max_bond: usize,
}

impl Fold for MpsBatchReport {
    fn fold(&mut self, other: Self) {
        self.regions += other.regions;
        self.operations += other.operations;
        self.sampled_shots += other.sampled_shots;
        self.measurements += other.measurements;
        self.max_bond = self.max_bond.max(other.max_bond);
    }
}
