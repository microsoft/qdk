// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The per-shot MPS terminal: one matrix product state, the shot's, run as
//! an L3 executor of the region ISA.

use std::rc::Rc;

use executor_protocol::{
    Batch, Capabilities, Close, ComponentId, Describe, Executor, Fold, Immediate, Lifecycle,
    MakeShot, Outcome, Rejected, ShotContext, Value,
};
use qdk_simulators::{
    QubitID,
    execution::{MeasurementKind, RegionInstruction},
};
use rand::{SeedableRng as _, rngs::StdRng};
use tensornet::{MpsEngine, MpsShape, MpsShapeError};

use crate::{
    GateFusion, MeasurementRun, ReadoutDistribution,
    capacity::mps_capacity,
    lowering,
    readout::{MeasurementRuns, draw},
};

/// One shot's matrix product state on an [`MpsEngine`], as an L3 executor of
/// the region ISA ([`RegionInstruction`]): each region's gates are applied as
/// the operators its [`GateFusion`] makes of them (one per gate by default,
/// by `qdk_simulators`' gate tables), and each measurement or reset draws its
/// value from the shot's own random numbers, with P(One) read from the
/// state, then projects the state on it. With a [`ReadoutDistribution`],
/// it also reads the exact distribution of each run of consecutive
/// measurements ([`measurement_runs`](Self::measurement_runs)).
///
/// It is the terminal `mps-a` runs Adaptive programs with, a shot at a time:
/// a shot's measurements steer its own state only, so it offers feedback and
/// resets, and its shots are replayable. Per-shot sessions of it are opened
/// by [`PerShot`](executor_protocol::PerShot) over [`TensorNetShots`]. M1's
/// stack samples Base programs with the batch strategy
/// ([`MpsBatchSimulator`](crate::MpsBatchSimulator)) instead, as M0 did.
///
/// It checks a whole batch before it applies any of it (qubits, operations,
/// tags), so that a rejected batch leaves the state as it was. The engine
/// is shared ([`Rc`]): cuTensorNet's engine owns one device context, which
/// every shot's state uses, and is not `Send` (E20).
pub struct TensorNetTerminal<E: MpsEngine> {
    engine: Rc<E>,
    /// The shot's state, or why the engine could not prepare it: a maker
    /// cannot fail ([`MakeShot::make`]), so each call prepares it again
    /// until the engine can, and reports it until then.
    state: Result<E::State, String>,
    capacity: MpsShape,
    fusion: GateFusion,
    rng: StdRng,
    log_p: f64,
    runs: MeasurementRuns,
    report: TensorNetReport,
    lifecycle: Lifecycle,
}

impl<E: MpsEngine> TensorNetTerminal<E> {
    /// A terminal on |0…0⟩, prepared by `engine` within `capacity` (one site
    /// per qubit), drawing from `seed`, grouping gates by the default
    /// [`GateFusion::Pairs`] (see [`with_gate_fusion`](Self::with_gate_fusion)).
    ///
    /// # Errors
    ///
    /// The engine's, when it cannot hold such a state.
    pub fn new(engine: Rc<E>, capacity: MpsShape, seed: u64) -> Result<Self, E::Error> {
        let state = engine.prepare(&capacity)?;
        Ok(Self::with_state(engine, Ok(state), capacity, seed))
    }

    /// The terminal applying each region's gates as `fusion` groups them.
    #[must_use]
    pub fn with_gate_fusion(mut self, fusion: GateFusion) -> Self {
        self.fusion = fusion;
        self
    }

    /// How it groups each region's gates into the engine's operators.
    #[must_use]
    pub fn gate_fusion(&self) -> GateFusion {
        self.fusion
    }

    /// The terminal reading the readout distribution as `distribution`
    /// says (by default [`ReadoutDistribution::Off`]). Set it before the
    /// first batch: it drops the runs read so far.
    #[must_use]
    pub fn with_readout_distribution(mut self, distribution: ReadoutDistribution) -> Self {
        self.runs = MeasurementRuns::new(distribution);
        self
    }

    /// Whether it reads the readout distribution.
    #[must_use]
    pub fn readout_distribution(&self) -> ReadoutDistribution {
        self.runs.distribution()
    }

    /// The runs of consecutive measurements of the shot so far, in order,
    /// each with the exact distribution of its values given the shot's
    /// history; empty with [`ReadoutDistribution::Off`]. The last run may
    /// still grow with the next batch, if it starts with a measurement.
    #[must_use]
    pub fn measurement_runs(&self) -> &[MeasurementRun] {
        self.runs.runs()
    }

    fn with_state(
        engine: Rc<E>,
        state: Result<E::State, String>,
        capacity: MpsShape,
        seed: u64,
    ) -> Self {
        Self {
            engine,
            state,
            capacity,
            fusion: GateFusion::default(),
            rng: StdRng::seed_from_u64(seed),
            log_p: 0.0,
            runs: MeasurementRuns::default(),
            report: TensorNetReport::default(),
            lifecycle: Lifecycle::default(),
        }
    }

    /// The engine.
    #[must_use]
    pub fn engine(&self) -> &E {
        &self.engine
    }

    /// The shot's state, as the batches so far left it (Q9: a query of a
    /// shot's state mid-session reads it here); `None` when the engine
    /// could not prepare it, or a batch failed.
    ///
    /// While a run of measurements is open
    /// ([`ReadoutDistribution::Exact`]), its projections are still to apply:
    /// this is the state before the run, the state its distribution is
    /// read on. Its retained weight is that of the projected state (a
    /// projection preserves the norm), but its bonds may be larger. The
    /// projections apply before the next gate or reset, before a
    /// measurement that ends the run, and at [`Close::close`].
    #[must_use]
    pub fn state(&self) -> Option<&E::State> {
        self.state.as_ref().ok()
    }

    /// The sum of ln p over the values this shot drew, its measurements' and
    /// its resets'.
    #[must_use]
    pub fn log_p(&self) -> f64 {
        self.log_p
    }

    fn qubits(&self) -> usize {
        self.capacity.site_count()
    }

    fn check(&self, at: usize, instruction: &RegionInstruction) -> Result<(), Rejected> {
        let qubits = self.qubits();
        let reason = match instruction {
            RegionInstruction::ExecuteRegion { region_id, region } => {
                lowering::check_region(region, qubits)
                    .map(|reason| format!("region {}: {reason}", region_id.value()))
            }
            RegionInstruction::Measure(request) => {
                qubit_beyond(request.qubit, qubits).or_else(|| {
                    u32::try_from(request.result_id).is_err().then(|| {
                        format!("result {} does not fit an outcome's tag", request.result_id)
                    })
                })
            }
            RegionInstruction::Reset { qubit } => qubit_beyond(*qubit, qubits),
        };
        match reason {
            Some(reason) => Err(Rejected::Instruction { at, reason }),
            None => Ok(()),
        }
    }

    /// Runs a checked batch on the prepared state: its outcomes, or the
    /// outcomes so far and why it failed.
    fn run(&mut self, batch: &[RegionInstruction]) -> Result<Vec<Outcome>, (Vec<Outcome>, String)> {
        let mut outcomes = Vec::new();
        let Ok(state) = &mut self.state else {
            return Err((outcomes, "the state is not prepared".to_string()));
        };
        let engine = &*self.engine;
        let report = &mut self.report;
        let capacity = &self.capacity;
        // The capacity is checked on the states a terminal that projects at
        // once checks at the end of a batch: an open run's, once projected.
        let mut checked = |state: &E::State, report: &mut TensorNetReport| {
            outgrown(engine, state, capacity).map(|bond| {
                report.max_bond = report.max_bond.max(bond);
            })
        };
        for instruction in batch {
            let step = match instruction {
                RegionInstruction::ExecuteRegion { region, .. } => {
                    report.regions += 1;
                    report.operations += region
                        .operations()
                        .iter()
                        .map(|&operation| u64::from(lowering::is_applied(operation)))
                        .sum::<u64>();
                    let operators = self.fusion.operators(region.operations());
                    // A gate ends the open run; a region of no operator does not.
                    if operators.is_empty() {
                        Ok(())
                    } else {
                        close_run(&mut self.runs, engine, state, report, &mut checked)
                    }
                    .and_then(|()| {
                        operators.iter().try_for_each(|operator| {
                            report.operators += 1;
                            engine
                                .apply(state, operator.sites(), operator.matrix())
                                .map_err(|error| error.to_string())
                        })
                    })
                }
                RegionInstruction::Measure(request) => {
                    let reset = request.kind == MeasurementKind::MeasureResetZ;
                    // The check admitted only tags that fit.
                    let tag = u32::try_from(request.result_id).unwrap_or(u32::MAX);
                    if self.runs.ends_at(request.qubit) {
                        close_run(&mut self.runs, engine, state, report, &mut checked)
                    } else {
                        Ok(())
                    }
                    .and_then(|()| {
                        self.runs
                            .measure(engine, state, &mut self.rng, (request.qubit, tag), reset)
                            .map_err(|error| error.to_string())
                    })
                    .map(|(one, ln_p)| {
                        self.log_p += ln_p;
                        report.measurements += 1;
                        outcomes.push(Outcome {
                            tag,
                            value: Value::from(one),
                        });
                    })
                }
                RegionInstruction::Reset { qubit } => {
                    close_run(&mut self.runs, engine, state, report, &mut checked)
                        .and_then(|()| {
                            draw(engine, state, &mut self.rng, *qubit, true)
                                .map_err(|error| error.to_string())
                        })
                        .map(|(_, ln_p)| {
                            self.log_p += ln_p;
                            report.resets += 1;
                        })
                }
            };
            if let Err(reason) = step {
                return Err((outcomes, reason));
            }
        }
        if self.runs.is_open() {
            return Ok(outcomes);
        }
        match checked(state, report) {
            Ok(()) => Ok(outcomes),
            Err(reason) => Err((outcomes, reason)),
        }
    }
}

/// Ends `runs`' open run, if there is one, and checks the projected state
/// with `checked`.
fn close_run<E: MpsEngine>(
    runs: &mut MeasurementRuns,
    engine: &E,
    state: &mut E::State,
    report: &mut TensorNetReport,
    checked: &mut impl FnMut(&E::State, &mut TensorNetReport) -> Result<(), String>,
) -> Result<(), String> {
    if runs
        .close(engine, state)
        .map_err(|error| error.to_string())?
    {
        checked(state, report)
    } else {
        Ok(())
    }
}

fn qubit_beyond(qubit: QubitID, qubits: usize) -> Option<String> {
    (qubit >= qubits).then(|| format!("qubit {qubit} is beyond the terminal's {qubits} qubits"))
}

/// Why a state outgrew the capacity it was prepared within, if it did. An
/// engine that truncates keeps within it, so for such an engine this is a
/// defect; the exact reference does not truncate, and outgrows a capacity
/// whose bonds are below the state's Schmidt ranks.
pub(crate) fn outgrown<E: MpsEngine>(
    engine: &E,
    state: &E::State,
    capacity: &MpsShape,
) -> Result<usize, String> {
    let shape = engine.shape(state).map_err(|error| error.to_string())?;
    if shape.fits_within(capacity) {
        Ok(shape.max_bond())
    } else {
        Err(format!(
            "the state has bond {}, beyond its capacity's {}: the engine did not keep the \
             state within the capacity it was prepared with",
            shape.max_bond(),
            capacity.max_bond()
        ))
    }
}

impl<E: MpsEngine> Executor<Batch<RegionInstruction>> for TensorNetTerminal<E> {
    type Response = Immediate<Outcome>;

    /// Runs the batch on the shot's state, in order, then checks that the
    /// state is still within its capacity.
    ///
    /// # Errors
    ///
    /// [`Rejected::Closed`] after [`Close::close`], and
    /// [`Rejected::SessionFailed`] after a failed batch (the state is then
    /// part-way through it); [`Rejected::Unavailable`] when the engine could
    /// not prepare the shot's state; [`Rejected::Instruction`] for the first
    /// instruction on a qubit beyond the terminal's, with an operation that
    /// cannot be applied, or whose result does not fit a tag. Nothing of a
    /// rejected batch runs.
    fn call(&mut self, batch: &[RegionInstruction]) -> Result<Self::Response, Rejected> {
        self.lifecycle.admit()?;
        for (at, instruction) in batch.iter().enumerate() {
            self.check(at, instruction)?;
        }
        if self.state.is_err() {
            // Not prepared yet (a failed batch fails the lifecycle above):
            // nothing ran, so preparing again is what `Later` promises.
            self.state = self
                .engine
                .prepare(&self.capacity)
                .map_err(|error| error.to_string());
        }
        if let Err(reason) = &self.state {
            return Err(Rejected::Unavailable(reason.clone()));
        }
        self.report.batches += 1;
        let run = self.run(batch);
        match run {
            Ok(outcomes) => Ok(Immediate::done(outcomes)),
            Err((outcomes, reason)) => {
                self.lifecycle.fail(&reason);
                self.state = Err(reason.clone());
                Ok(Immediate::failed(outcomes, reason))
            }
        }
    }
}

impl<E: MpsEngine> Describe for TensorNetTerminal<E> {
    /// Its qubits, as many as fit the protocol's count; one state, kept
    /// between calls, so feedback; resets; the probability of its draws.
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
            .with_qubits(Some(u32::try_from(self.qubits()).unwrap_or(u32::MAX)))
            .with_states(Some(1))
            .with_probabilities(true)
    }
}

impl<E: MpsEngine> Close for TensorNetTerminal<E> {
    type Report = TensorNetReport;

    /// Ends the terminal; later calls are [`Rejected::Closed`]. The state is
    /// kept for [`TensorNetTerminal::state`], and dropped with the terminal;
    /// an open run of measurements is projected on first, and the projected
    /// state checked within its capacity, as a batch's end checks it (when
    /// either fails, there is no batch left to fail: the state is dropped
    /// with the reason, and [`TensorNetTerminal::state`] is `None`).
    fn close(&mut self) -> TensorNetReport {
        self.lifecycle.close();
        if let Ok(state) = &mut self.state {
            let engine = &*self.engine;
            let closed = self.runs.close(engine, state).map_err(|e| e.to_string());
            let checked = closed.and_then(|projected| {
                if projected {
                    let bond = outgrown(engine, state, &self.capacity)?;
                    self.report.max_bond = self.report.max_bond.max(bond);
                }
                Ok(())
            });
            if let Err(reason) = checked {
                self.state = Err(reason);
            }
        }
        std::mem::take(&mut self.report)
    }
}

/// The maker of one shot's [`TensorNetTerminal`]: per-shot sessions of the
/// MPS method are opened by [`PerShot`](executor_protocol::PerShot) over it.
///
/// Shot `j` of a session opened with `seed` draws from
/// [`ShotContext::rng_seed`] of [`ComponentId::TERMINAL`], so its outcomes
/// are the same in any session that holds it.
pub struct TensorNetShots<E: MpsEngine> {
    engine: Rc<E>,
    capacity: MpsShape,
    fusion: GateFusion,
    readout: ReadoutDistribution,
}

impl<E: MpsEngine> TensorNetShots<E> {
    /// Makes shots of `qubits` qubits whose bonds are capped at `max_bond`
    /// (see [`mps_capacity`]), on `engine`.
    ///
    /// # Errors
    ///
    /// [`mps_capacity`]'s, when that is not a chain.
    pub fn new(engine: Rc<E>, qubits: usize, max_bond: usize) -> Result<Self, MpsShapeError> {
        Ok(Self {
            engine,
            capacity: mps_capacity(qubits, max_bond)?,
            fusion: GateFusion::default(),
            readout: ReadoutDistribution::default(),
        })
    }

    /// Shots whose terminals apply each region's gates as `fusion` groups
    /// them (by default [`GateFusion::Pairs`]).
    #[must_use]
    pub fn with_gate_fusion(mut self, fusion: GateFusion) -> Self {
        self.fusion = fusion;
        self
    }

    /// How each shot's terminal groups a region's gates.
    #[must_use]
    pub fn gate_fusion(&self) -> GateFusion {
        self.fusion
    }

    /// Shots whose terminals read the readout distribution as
    /// `distribution` says (by default [`ReadoutDistribution::Off`]).
    #[must_use]
    pub fn with_readout_distribution(mut self, distribution: ReadoutDistribution) -> Self {
        self.readout = distribution;
        self
    }

    /// Whether each shot's terminal reads the readout distribution.
    #[must_use]
    pub fn readout_distribution(&self) -> ReadoutDistribution {
        self.readout
    }

    /// The capacity each shot's state is prepared within.
    #[must_use]
    pub fn capacity(&self) -> &MpsShape {
        &self.capacity
    }

    pub(crate) fn engine(&self) -> &Rc<E> {
        &self.engine
    }
}

impl<E: MpsEngine> Describe for TensorNetShots<E> {
    /// The qubits of each shot; any number of shots, each its own state,
    /// with feedback and resets, replayable, and the probability of each
    /// shot's draws.
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
            .with_qubits(Some(
                u32::try_from(self.capacity.site_count()).unwrap_or(u32::MAX),
            ))
            .with_probabilities(true)
    }
}

impl<E: MpsEngine> MakeShot<RegionInstruction> for TensorNetShots<E> {
    type Shot = TensorNetTerminal<E>;

    /// The shot's terminal. A state the engine cannot prepare is reported by
    /// the shot's calls ([`Rejected::Unavailable`], retried later), since
    /// `make` cannot fail; each call prepares it again until the engine can.
    fn make(&self, ctx: &ShotContext) -> TensorNetTerminal<E> {
        let state = self
            .engine
            .prepare(&self.capacity)
            .map_err(|error| error.to_string());
        TensorNetTerminal::with_state(
            Rc::clone(&self.engine),
            state,
            self.capacity.clone(),
            ctx.rng_seed(ComponentId::TERMINAL),
        )
        .with_gate_fusion(self.fusion)
        .with_readout_distribution(self.readout)
    }

    fn log_p(shot: &TensorNetTerminal<E>) -> Option<f64> {
        Some(shot.log_p)
    }
}

/// What a [`TensorNetTerminal`] ran: its batches and their instructions, and
/// the largest bond its state reached.
///
/// The reports of terminals fold into the report of the run they took part
/// in, by adding their counts and keeping the largest bond.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TensorNetReport {
    /// The batches it ran; rejected batches are not counted.
    pub batches: u64,
    /// The regions it applied.
    pub regions: u64,
    /// The operations of those regions it applied (`I` is not applied).
    pub operations: u64,
    /// The operators it gave the engine for them
    /// ([`MpsEngine::apply`](tensornet::MpsEngine::apply) calls): as many as
    /// the operations unfused, fewer when its [`GateFusion`] groups them.
    /// A reset's flip is not counted.
    pub operators: u64,
    /// The measurements it made.
    pub measurements: u64,
    /// The resets it made.
    pub resets: u64,
    /// The largest bond of the state at the end of a batch (with
    /// [`ReadoutDistribution::Exact`], once an open run's projections apply,
    /// so the same states are checked).
    pub max_bond: usize,
}

impl Fold for TensorNetReport {
    fn fold(&mut self, other: Self) {
        self.batches += other.batches;
        self.regions += other.regions;
        self.operations += other.operations;
        self.operators += other.operators;
        self.measurements += other.measurements;
        self.resets += other.resets;
        self.max_bond = self.max_bond.max(other.max_bond);
    }
}
