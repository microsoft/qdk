// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! H₂ QPE on CPU through the protocol: the walk over the sparse state
//! simulator as the L3 terminal, in per-shot sessions, against the legacy
//! runtime on the same seeds.
//!
//! The program is a user's input and stays out of the repository, so the
//! test is ignored and reads it from the environment:
//!
//! - `QDK_H2_BYTECODE`: the program's Adaptive bytecode, as the JSON of
//!   `AdaptiveProgram.as_dict()` that qdk's CPU path builds from the `.ll`
//!   (`DecomposeCcxPass`, then `AdaptiveProfilePass(Bytecode.Bit64)`; see
//!   `run_qir_cpu` in `qdk/simulation/_simulation.py`).
//! - `QDK_H2_SHOTS`: the number of shots, 1 by default.
//! - `QDK_H2_SEED`: the run's seed, 0 by default. Shot `j` (from 1) runs in
//!   its own session, opened on shot `j` with this seed (see
//!   `SubmissionWrapper`), so it gives the outcomes it gives in a session on
//!   all the shots.
//!
//! Each shot's output rows must be the same on both paths. The test prints
//! the time per shot of each path and the ten most frequent rows. H₂'s rows
//! are the measurements of qubits 0–4, in the order the program records
//! them: qubit 0 first.
//!
//! `h2_qpe_measurement_trace` runs the walk alone on the same sessions and
//! writes each shot's measurements to `QDK_H2_TRACE`, for the analysis of
//! shot branching. The tests of [`shot_state`] read a shot's state at chosen
//! measurements, to check whether every shot reaches the readout in the same
//! state.

use std::{
    collections::BTreeMap,
    env, fs,
    io::{BufWriter, Write},
    thread,
    time::{Duration, Instant},
};

use executor_protocol::{
    Batch, Executor, Immediate, Outcome, PerShot, PerShotSession, Rejected, ShotId, ShotSet,
    SubmissionWrapper, Value, open, shot_seed, wait,
};
use qdk_simulators::{
    MeasurementResult, OutputRecord, QubitID, ResultID, Simulator, SparseStateSim,
    bytecode::runtime::run_shot,
    cpu_full_state_simulator::FullStateSimulator,
    execution::{
        PreparedAdaptiveProgram, RegionInstruction, RuntimeAdapter, SimulatorShots,
        SimulatorTerminal,
    },
};
use rand::{RngExt as _, SeedableRng, rngs::StdRng};

#[path = "common/adaptive_program_json.rs"]
mod adaptive_program_json;

use adaptive_program_json::adaptive_program_from_json;

#[path = "h2_qpe/shot_state.rs"]
mod shot_state;

/// The legacy [`Simulator`] over [`SparseStateSim`], which does not
/// implement it: qubits `0..n` allocated up front, no noise and no loss.
///
/// With `READS_P_ONE`, each measurement first reads the probability of One
/// it collapses with, for [`MeasurementTrace`]
/// ([`SparseSimulator::last_p_one`]). Reading it applies the queued gates
/// that the measurement would apply first, in the same order, and draws
/// nothing, so the measurement then computes the same probability, over the
/// state in the same order, and draws the same outcome. It costs a pass over
/// the state per measurement, which the default, `false`, does not make.
struct SparseSimulator<const READS_P_ONE: bool = false> {
    sim: SparseStateSim,
    measurements: Vec<MeasurementResult>,
    last_p_one: Option<f64>,
}

impl<const READS_P_ONE: bool> SparseSimulator<READS_P_ONE> {
    fn measure(&mut self, target: QubitID) -> bool {
        if READS_P_ONE {
            self.last_p_one = Some(self.sim.joint_probability(&[target]));
        }
        self.sim.measure(target)
    }

    /// The probability of One of the last measurement, resets included,
    /// before it collapsed the state; `None` before the first one, or without
    /// `READS_P_ONE`.
    fn last_p_one(&self) -> Option<f64> {
        self.last_p_one
    }

    fn record(&mut self, one: bool, result_id: ResultID) {
        self.measurements[result_id] = if one {
            MeasurementResult::One
        } else {
            MeasurementResult::Zero
        };
    }
}

impl<const READS_P_ONE: bool> Simulator for SparseSimulator<READS_P_ONE> {
    type Noise = ();
    type StateDumpData = ();

    fn new(num_qubits: usize, num_results: usize, seed: u32, (): ()) -> Self {
        let mut sim = SparseStateSim::new(Some(StdRng::seed_from_u64(seed.into())));
        for qubit in 0..num_qubits {
            assert_eq!(sim.allocate(), qubit, "qubits are allocated in order");
        }
        Self {
            sim,
            measurements: vec![MeasurementResult::Zero; num_results],
            last_p_one: None,
        }
    }

    fn x(&mut self, target: QubitID) {
        self.sim.x(target);
    }

    fn y(&mut self, target: QubitID) {
        self.sim.y(target);
    }

    fn z(&mut self, target: QubitID) {
        self.sim.z(target);
    }

    fn h(&mut self, target: QubitID) {
        self.sim.h(target);
    }

    fn s(&mut self, target: QubitID) {
        self.sim.s(target);
    }

    fn s_adj(&mut self, target: QubitID) {
        self.sim.sadj(target);
    }

    fn sx(&mut self, target: QubitID) {
        self.sim.h(target);
        self.sim.s(target);
        self.sim.h(target);
    }

    fn sx_adj(&mut self, target: QubitID) {
        self.sim.h(target);
        self.sim.sadj(target);
        self.sim.h(target);
    }

    fn t(&mut self, target: QubitID) {
        self.sim.t(target);
    }

    fn t_adj(&mut self, target: QubitID) {
        self.sim.tadj(target);
    }

    fn rx(&mut self, angle: f64, target: QubitID) {
        self.sim.rx(angle, target);
    }

    fn ry(&mut self, angle: f64, target: QubitID) {
        self.sim.ry(angle, target);
    }

    fn rz(&mut self, angle: f64, target: QubitID) {
        self.sim.rz(angle, target);
    }

    fn cx(&mut self, control: QubitID, target: QubitID) {
        self.sim.mcx(&[control], target);
    }

    fn cy(&mut self, control: QubitID, target: QubitID) {
        self.sim.mcy(&[control], target);
    }

    fn cz(&mut self, control: QubitID, target: QubitID) {
        self.sim.mcz(&[control], target);
    }

    fn rxx(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.sim.h(q1);
        self.sim.h(q2);
        self.rzz(angle, q1, q2);
        self.sim.h(q2);
        self.sim.h(q1);
    }

    fn ryy(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.sim.sadj(q1);
        self.sim.sadj(q2);
        self.rxx(angle, q1, q2);
        self.sim.s(q2);
        self.sim.s(q1);
    }

    fn rzz(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.sim.mcx(&[q2], q1);
        self.sim.rz(angle, q1);
        self.sim.mcx(&[q2], q1);
    }

    fn swap(&mut self, q1: QubitID, q2: QubitID) {
        self.sim.swap_qubit_ids(q1, q2);
    }

    fn mz(&mut self, target: QubitID, result_id: ResultID) {
        let one = self.measure(target);
        self.record(one, result_id);
    }

    fn mresetz(&mut self, target: QubitID, result_id: ResultID) {
        let one = self.measure(target);
        if one {
            self.sim.x(target);
        }
        self.record(one, result_id);
    }

    fn resetz(&mut self, target: QubitID) {
        if self.measure(target) {
            self.sim.x(target);
        }
    }

    fn mov(&mut self, _target: QubitID) {}

    fn correlated_noise_intrinsic(&mut self, intrinsic_id: u32, _targets: &[usize]) {
        panic!("the sparse simulator here has no noise, so no intrinsic {intrinsic_id}");
    }

    fn measurements(&self) -> &[MeasurementResult] {
        &self.measurements
    }

    fn take_measurements(&mut self) -> Vec<MeasurementResult> {
        std::mem::take(&mut self.measurements)
    }

    fn state_dump(&self) -> &() {
        &()
    }

    fn peek_loss(&mut self, _target: QubitID, result_id: ResultID) {
        self.record(false, result_id);
    }

    fn apply_loss_noise(&mut self, p_loss: f64, _target: QubitID) {
        panic!("the sparse simulator here has no loss, so no loss noise {p_loss}");
    }

    fn apply_readout_noise(&mut self, p_zero_as_one: f64, p_one_as_zero: f64, _: ResultID) {
        panic!("the sparse simulator here has no noise ({p_zero_as_one}, {p_one_as_zero})");
    }

    fn write_result(&mut self, value: bool, result_id: ResultID) {
        self.record(value, result_id);
    }
}

/// One gate of the [`Simulator`] trait on three qubits.
#[derive(Clone, Copy, Debug)]
enum Gate {
    One(u8, QubitID),
    Rotation(u8, f64, QubitID),
    Two(u8, QubitID, QubitID),
    TwoRotation(u8, f64, QubitID, QubitID),
}

impl Gate {
    fn random(rng: &mut StdRng) -> Self {
        let mut qubits = [0, 1, 2];
        let (q1, q2) = (rng.random_range(0..3), rng.random_range(0..2));
        qubits.swap(0, q1);
        let (q1, q2) = (qubits[0], qubits[1 + q2]);
        let angle = rng.random_range(-4.0..4.0);
        match rng.random_range(0..4) {
            0 => Self::One(rng.random_range(0..10), q1),
            1 => Self::Rotation(rng.random_range(0..3), angle, q1),
            2 => Self::Two(rng.random_range(0..4), q1, q2),
            _ => Self::TwoRotation(rng.random_range(0..3), angle, q1, q2),
        }
    }

    fn apply<S: Simulator>(self, sim: &mut S) {
        match self {
            Self::One(0, q) => sim.x(q),
            Self::One(1, q) => sim.y(q),
            Self::One(2, q) => sim.z(q),
            Self::One(3, q) => sim.h(q),
            Self::One(4, q) => sim.s(q),
            Self::One(5, q) => sim.s_adj(q),
            Self::One(6, q) => sim.sx(q),
            Self::One(7, q) => sim.sx_adj(q),
            Self::One(8, q) => sim.t(q),
            Self::One(_, q) => sim.t_adj(q),
            Self::Rotation(0, angle, q) => sim.rx(angle, q),
            Self::Rotation(1, angle, q) => sim.ry(angle, q),
            Self::Rotation(_, angle, q) => sim.rz(angle, q),
            Self::Two(0, c, t) => sim.cx(c, t),
            Self::Two(1, c, t) => sim.cy(c, t),
            Self::Two(2, c, t) => sim.cz(c, t),
            Self::Two(_, a, b) => sim.swap(a, b),
            Self::TwoRotation(0, angle, a, b) => sim.rxx(angle, a, b),
            Self::TwoRotation(1, angle, a, b) => sim.ryy(angle, a, b),
            Self::TwoRotation(_, angle, a, b) => sim.rzz(angle, a, b),
        }
    }
}

/// The probability that the parity of `qubits` is odd, on the dense state.
fn dense_parity(sim: &FullStateSimulator, qubits: &[QubitID]) -> f64 {
    let state = sim.state_dump();
    let mask: usize = qubits.iter().map(|q| 1 << q).sum();
    let norm: f64 = state
        .data()
        .iter()
        .map(num_complex::Complex::norm_sqr)
        .sum();
    let odd: f64 = state
        .data()
        .iter()
        .enumerate()
        .filter(|(index, _)| (index & mask).count_ones() % 2 == 1)
        .map(|(_, a)| a.norm_sqr())
        .sum();
    odd / norm
}

/// The adapter against the dense `Simulator`: after each gate of random
/// circuits on three qubits, the parities of every set of qubits, which fix
/// the distribution of outcomes, have the same probabilities.
#[test]
fn the_sparse_simulator_has_the_dense_simulators_distribution() {
    const SETS: [&[QubitID]; 7] = [&[0], &[1], &[2], &[0, 1], &[0, 2], &[1, 2], &[0, 1, 2]];
    let mut rng = StdRng::seed_from_u64(0);
    let mut probe = FullStateSimulator::new(3, 0, 0, Default::default());
    probe.x(0);
    assert!(
        (dense_parity(&probe, &[0]) - 1.0).abs() < 1e-12,
        "the dense state's qubit 0 is its least significant bit"
    );
    for circuit in 0..100 {
        let mut dense = FullStateSimulator::new(3, 0, 0, Default::default());
        let mut sparse: SparseSimulator = SparseSimulator::new(3, 0, 0, ());
        for step in 0..40 {
            let gate = Gate::random(&mut rng);
            gate.apply(&mut dense);
            gate.apply(&mut sparse);
            for set in SETS {
                let (d, s) = (dense_parity(&dense, set), sparse.sim.joint_probability(set));
                assert!(
                    (d - s).abs() < 1e-9,
                    "circuit {circuit}, step {step}, after {gate:?}: parity of {set:?} \
                     is odd with probability {d} dense, {s} sparse"
                );
            }
        }
    }
}

fn env_or(name: &str, default: u64) -> u64 {
    env::var(name).map_or(default, |value| {
        value
            .parse()
            .unwrap_or_else(|_| panic!("{name}={value} should be a number"))
    })
}

fn bits(row: &[OutputRecord]) -> String {
    row.iter()
        .map(|record| match record {
            OutputRecord::Result(MeasurementResult::Zero) => '0',
            OutputRecord::Result(MeasurementResult::One) => '1',
            other => panic!("H₂'s rows should be results, not {other:?}"),
        })
        .collect()
}

/// H₂'s program, from `QDK_H2_BYTECODE`, with its qubits and results.
fn h2_program() -> (PreparedAdaptiveProgram<u64>, usize, usize) {
    let path = env::var("QDK_H2_BYTECODE").expect(
        "QDK_H2_BYTECODE should name the JSON of H₂'s AdaptiveProgram.as_dict() \
         (DecomposeCcxPass, then AdaptiveProfilePass(Bytecode.Bit64))",
    );
    let json = fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let program = adaptive_program_from_json(&json);
    let qubits = usize::try_from(program.num_qubits).expect("fits usize");
    let results = usize::try_from(program.num_results).expect("fits usize");
    let prepared = PreparedAdaptiveProgram::new(program).expect("H₂ should prepare");
    (prepared, qubits, results)
}

/// The seed of the simulator of shot `shot` of a run seeded `seed`, as
/// [`SimulatorShots`] gives it: the exclusive or of the halves of
/// [`shot_seed`], since [`Simulator::new`] takes 32 bits.
fn simulator_seed(seed: u64, shot: ShotId) -> u32 {
    let folded = shot_seed(seed, shot);
    u32::try_from((folded ^ (folded >> 32)) & u64::from(u32::MAX)).expect("masked to 32 bits")
}

/// `QDK_H2_SHOTS` and `QDK_H2_SEED`.
fn h2_shots_and_seed() -> (u32, u64) {
    let shots = u32::try_from(env_or("QDK_H2_SHOTS", 1)).expect("the shots should fit u32");
    (shots, env_or("QDK_H2_SEED", 0))
}

#[test]
#[ignore = "needs H₂'s bytecode in QDK_H2_BYTECODE; runs for minutes"]
fn h2_qpe_through_the_protocol_matches_the_legacy_runtime() {
    let (shots, seed) = h2_shots_and_seed();
    let (prepared, qubits, results) = h2_program();
    let per_shot = PerShot(SimulatorShots::<SparseSimulator>::new(qubits, results, ()));

    let (mut walk_time, mut legacy_time) = (Duration::ZERO, Duration::ZERO);
    let mut histogram = BTreeMap::<String, u64>::new();
    for shot in 1..=shots {
        // The walk drives one shot, so each shot has its own session, which
        // holds that shot's state only, opened with the run's seed.
        // The two paths run side by side, each on its own thread.
        let ((walked, walk), (legacy, runtime)) = thread::scope(|scope| {
            let legacy = scope.spawn(|| {
                let start = Instant::now();
                let mut sim: SparseSimulator =
                    SparseSimulator::new(qubits, results, simulator_seed(seed, shot), ());
                let rows = run_shot(prepared.program(), &mut sim);
                (rows, start.elapsed())
            });
            let start = Instant::now();
            let session = open(&per_shot, seed, &ShotSet::one(shot)).expect("the session opens");
            let mut adapter = RuntimeAdapter::new(SubmissionWrapper::new(session, shot));
            let mut rows = Vec::new();
            adapter
                .run(&prepared, &mut rows)
                .unwrap_or_else(|e| panic!("shot {shot}: the walk fails: {e}"));
            let walked = (rows, start.elapsed());
            (walked, legacy.join().expect("the legacy runtime runs"))
        });
        walk_time += walk;
        legacy_time += runtime;
        assert_eq!(
            walked, legacy,
            "shot {shot}: the walk and the runtime differ"
        );
        let row = bits(&walked);
        println!("shot {shot}: {row} (walk {walk:?}, legacy runtime {runtime:?})");
        *histogram.entry(row).or_default() += 1;
    }

    let per = |total: Duration| total / shots.max(1);
    println!(
        "{shots} shots, seed {seed}: walk {:?} per shot, legacy runtime {:?} per shot",
        per(walk_time),
        per(legacy_time)
    );
    let mut ranked: Vec<_> = histogram.into_iter().collect();
    ranked.sort_by(|(a, m), (b, n)| n.cmp(m).then(a.cmp(b)));
    for (row, count) in ranked.iter().take(10) {
        println!("{row} {count}");
    }
}

/// One shot of the walk on the sparse simulator that reads P(One): the
/// session of the shot, as the walk calls it.
type TracedShot = SubmissionWrapper<PerShotSession<SimulatorTerminal<SparseSimulator<true>>>>;

/// Writes a line per measurement of one shot to `out`, as the walk sends the
/// shot's batches down to `inner`: which qubit, its outcome, the gates since
/// the measurement before it, and its probability of One before the
/// collapse. It passes the batches on unchanged and replies with the
/// outcomes of `inner`.
struct MeasurementTrace<'a, W> {
    inner: TracedShot,
    out: &'a mut W,
    seed: u64,
    shot: ShotId,
    measurements: u64,
    gates_since_measurement: u64,
    gates: u64,
    resets: u64,
}

impl<'a, W: Write> MeasurementTrace<'a, W> {
    fn new(inner: TracedShot, out: &'a mut W, seed: u64, shot: ShotId) -> Self {
        Self {
            inner,
            out,
            seed,
            shot,
            measurements: 0,
            gates_since_measurement: 0,
            gates: 0,
            resets: 0,
        }
    }

    /// The probability of One of the shot's last measurement, before it
    /// collapsed the state.
    fn last_p_one(&self) -> f64 {
        self.inner
            .inner()
            .shot(self.shot)
            .and_then(|terminal| terminal.simulator().last_p_one())
            .expect("the shot's simulator reads P(One) at each measurement")
    }
}

impl<W: Write> Executor<Batch<RegionInstruction>> for MeasurementTrace<'_, W> {
    type Response = Immediate<Outcome>;

    fn call(&mut self, batch: &[RegionInstruction]) -> Result<Immediate<Outcome>, Rejected> {
        let reply = self.inner.call(batch)?;
        let mut outcomes = Vec::new();
        if let Err(reason) = wait(reply, &mut outcomes) {
            return Ok(Immediate::failed(outcomes, reason));
        }
        for (at, instruction) in batch.iter().enumerate() {
            match instruction {
                RegionInstruction::ExecuteRegion { region, .. } => {
                    let gates = region.operations().len() as u64;
                    self.gates_since_measurement += gates;
                    self.gates += gates;
                }
                RegionInstruction::Reset { .. } => self.resets += 1,
                RegionInstruction::Measure(request) => {
                    // P(One) is the last measurement's, resets included, so
                    // it is this one's only if nothing measures after it.
                    assert_eq!(at + 1, batch.len(), "a batch ends at its measurement");
                    let outcome = outcomes
                        .iter()
                        .find(|outcome| u32::try_from(request.result_id) == Ok(outcome.tag))
                        .map(|outcome| match outcome.value {
                            Value::Zero => '0',
                            Value::One => '1',
                            Value::Error(kind) => panic!("H₂'s measurements give 0 or 1: {kind:?}"),
                        })
                        .expect("the reply has the measurement's outcome");
                    self.measurements += 1;
                    writeln!(
                        self.out,
                        "{} {} {} {} {outcome} {} {:e}",
                        self.seed,
                        self.shot,
                        self.measurements,
                        request.qubit,
                        self.gates_since_measurement,
                        self.last_p_one()
                    )
                    .expect("the trace is written");
                    self.gates_since_measurement = 0;
                }
            }
        }
        Ok(Immediate::done(outcomes))
    }
}

/// H₂'s mid-circuit measurements, shot by shot, for the analysis of shot
/// branching: carrying several shots on one state, which forks only where
/// their outcomes differ, costs the gates of the distinct outcome prefixes
/// instead of those of every shot. This trace gives the outcomes, the gates
/// between measurements and how random each outcome was.
///
/// It runs only the walk, through the per-shot sessions of
/// `h2_qpe_through_the_protocol_matches_the_legacy_runtime`, on one core,
/// with `QDK_H2_BYTECODE`, `QDK_H2_SHOTS` and `QDK_H2_SEED` as that test.
/// `QDK_H2_TRACE` names the file it writes, with a line per measurement:
///
/// `seed shot index qubit outcome gates_since_previous_measurement p_one`
///
/// where `index` counts the shot's measurements from 1, the gates are the
/// unitary operations of the regions since the measurement before (or the
/// start), and `p_one` is the probability of One before the collapse; and a
/// line per shot as it ends:
///
/// `end seed shot row measurements gates gates_after_last_measurement resets`
///
/// with its row as the existing test prints it (qubit 0 first).
///
/// Reading P(One) changes no outcome ([`SparseSimulator`]): the rows of
/// shots 1 and 2 of seed 1 are those of the existing test.
#[test]
#[ignore = "needs H₂'s bytecode in QDK_H2_BYTECODE and a file in QDK_H2_TRACE; runs for minutes"]
fn h2_qpe_measurement_trace() {
    let path = env::var("QDK_H2_TRACE").expect("QDK_H2_TRACE should name the file to write");
    let (shots, seed) = h2_shots_and_seed();
    let (prepared, qubits, results) = h2_program();
    let per_shot = PerShot(SimulatorShots::<SparseSimulator<true>>::new(
        qubits,
        results,
        (),
    ));
    let file = fs::File::create(&path).unwrap_or_else(|e| panic!("cannot write {path}: {e}"));
    let mut out = BufWriter::new(file);
    for shot in 1..=shots {
        let start = Instant::now();
        let session = open(&per_shot, seed, &ShotSet::one(shot)).expect("the session opens");
        let trace =
            MeasurementTrace::new(SubmissionWrapper::new(session, shot), &mut out, seed, shot);
        let mut adapter = RuntimeAdapter::new(trace);
        let mut rows = Vec::new();
        adapter
            .run(&prepared, &mut rows)
            .unwrap_or_else(|e| panic!("shot {shot}: the walk fails: {e}"));
        let trace = adapter.into_inner();
        let row = bits(&rows);
        writeln!(
            trace.out,
            "end {seed} {shot} {row} {} {} {} {}",
            trace.measurements, trace.gates, trace.gates_since_measurement, trace.resets
        )
        .and_then(|()| trace.out.flush())
        .expect("the trace is written");
        println!(
            "shot {shot}: {row}, {} measurements, {} gates, {} resets ({:?})",
            trace.measurements,
            trace.gates,
            trace.resets,
            start.elapsed()
        );
    }
}
