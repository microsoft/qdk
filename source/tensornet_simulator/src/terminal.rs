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
    execution::{MeasurementKind, RegionInstruction, UnitaryOperation},
};
use rand::{RngExt as _, SeedableRng as _, rngs::StdRng};
use tensornet::{MpsEngine, MpsShape, MpsShapeError};

use crate::{capacity::mps_capacity, lowering};

/// One shot's matrix product state on an [`MpsEngine`], as an L3 executor of
/// the region ISA ([`RegionInstruction`]): regions are applied gate by gate
/// (by `qdk_simulators`' gate tables), and each measurement or reset draws its
/// value from the shot's own random numbers, with P(One) read from the
/// state, then projects the state on it.
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
    rng: StdRng,
    log_p: f64,
    report: TensorNetReport,
    lifecycle: Lifecycle,
}

impl<E: MpsEngine> TensorNetTerminal<E> {
    /// A terminal on |0…0⟩, prepared by `engine` within `capacity` (one site
    /// per qubit), drawing from `seed`.
    ///
    /// # Errors
    ///
    /// The engine's, when it cannot hold such a state.
    pub fn new(engine: Rc<E>, capacity: MpsShape, seed: u64) -> Result<Self, E::Error> {
        let state = engine.prepare(&capacity)?;
        Ok(Self::with_state(engine, Ok(state), capacity, seed))
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
            rng: StdRng::seed_from_u64(seed),
            log_p: 0.0,
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

    /// Runs a checked batch: its outcomes, or the outcomes so far and why it
    /// failed.
    fn run(
        engine: &E,
        state: &mut E::State,
        rng: &mut StdRng,
        log_p: &mut f64,
        report: &mut TensorNetReport,
        batch: &[RegionInstruction],
    ) -> Result<Vec<Outcome>, (Vec<Outcome>, String)> {
        let mut outcomes = Vec::new();
        for instruction in batch {
            let step = match instruction {
                RegionInstruction::ExecuteRegion { region, .. } => {
                    report.regions += 1;
                    region.operations().iter().try_for_each(|&operation| {
                        report.operations += u64::from(lowering::is_applied(operation));
                        lowering::apply_operation(engine, state, operation)
                    })
                }
                RegionInstruction::Measure(request) => {
                    let reset = request.kind == MeasurementKind::MeasureResetZ;
                    draw(engine, state, rng, request.qubit, reset).map(|(one, ln_p)| {
                        *log_p += ln_p;
                        report.measurements += 1;
                        // The check admitted only tags that fit.
                        if let Ok(tag) = u32::try_from(request.result_id) {
                            outcomes.push(Outcome {
                                tag,
                                value: Value::from(one),
                            });
                        }
                    })
                }
                RegionInstruction::Reset { qubit } => {
                    draw(engine, state, rng, *qubit, true).map(|(_, ln_p)| {
                        *log_p += ln_p;
                        report.resets += 1;
                    })
                }
            };
            if let Err(error) = step {
                return Err((outcomes, error.to_string()));
            }
        }
        Ok(outcomes)
    }
}

/// Draws the value of `qubit` from P(One) on `state`, projects on it, and
/// flips it back to |0⟩ when `reset` and it was One. The value, and ln p of
/// the value drawn.
fn draw<E: MpsEngine>(
    engine: &E,
    state: &mut E::State,
    rng: &mut StdRng,
    qubit: QubitID,
    reset: bool,
) -> Result<(bool, f64), E::Error> {
    let p1 = engine.probability_of_one(state, qubit)?.clamp(0.0, 1.0);
    // `random` is in [0, 1): One only when p1 > 0, Zero only when p1 < 1.
    let one = rng.random::<f64>() < p1;
    let p = if one { p1 } else { 1.0 - p1 };
    engine.project(state, qubit, one, p)?;
    if reset && one {
        lowering::apply_operation(engine, state, UnitaryOperation::X { target: qubit })?;
    }
    Ok((one, p.ln()))
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
        let state = match &mut self.state {
            Ok(state) => state,
            Err(reason) => return Err(Rejected::Unavailable(reason.clone())),
        };
        self.report.batches += 1;
        let run = Self::run(
            &self.engine,
            state,
            &mut self.rng,
            &mut self.log_p,
            &mut self.report,
            batch,
        )
        .and_then(
            |outcomes| match outgrown(&*self.engine, state, &self.capacity) {
                Ok(bond) => {
                    self.report.max_bond = self.report.max_bond.max(bond);
                    Ok(outcomes)
                }
                Err(reason) => Err((outcomes, reason)),
            },
        );
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
    /// kept for [`TensorNetTerminal::state`], and dropped with the terminal.
    fn close(&mut self) -> TensorNetReport {
        self.lifecycle.close();
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
        })
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
    /// The measurements it made.
    pub measurements: u64,
    /// The resets it made.
    pub resets: u64,
    /// The largest bond of the state at the end of a batch.
    pub max_bond: usize,
}

impl Fold for TensorNetReport {
    fn fold(&mut self, other: Self) {
        self.batches += other.batches;
        self.regions += other.regions;
        self.operations += other.operations;
        self.measurements += other.measurements;
        self.resets += other.resets;
        self.max_bond = self.max_bond.max(other.max_bond);
    }
}
