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
//! - `QDK_H2_SEED`: the seed, 0 by default: shot `j` is shot 1 of a
//!   session opened with seed `QDK_H2_SEED + j`.
//!
//! Each shot's output rows must be the same on both paths. The test prints
//! the time per shot of each path and the ten most frequent rows. H₂'s rows
//! are the measurements of qubits 0–4, in the order the program records
//! them: qubit 0 first.

use std::{
    collections::BTreeMap,
    env, fs, thread,
    time::{Duration, Instant},
};

use executor_protocol::{PerShot, ShotSet, SubmissionWrapper, open, shot_seed};
use qdk_simulators::{
    MeasurementResult, OutputRecord, QubitID, ResultID, Simulator, SparseStateSim,
    bytecode::{
        AdaptiveProgram, Block, Function, Instruction, Op, PhiNodeEntry, SwitchCase,
        runtime::run_shot,
    },
    cpu_full_state_simulator::FullStateSimulator,
    execution::{PreparedAdaptiveProgram, RuntimeAdapter, SimulatorShots},
};
use rand::{RngExt as _, SeedableRng, rngs::StdRng};
use serde_json::Value;

/// The legacy [`Simulator`] over [`SparseStateSim`], which does not
/// implement it: qubits `0..n` allocated up front, no noise and no loss.
struct SparseSimulator {
    sim: SparseStateSim,
    measurements: Vec<MeasurementResult>,
}

impl SparseSimulator {
    fn measure(&mut self, target: QubitID) -> bool {
        self.sim.measure(target)
    }

    fn record(&mut self, one: bool, result_id: ResultID) {
        self.measurements[result_id] = if one {
            MeasurementResult::One
        } else {
            MeasurementResult::Zero
        };
    }
}

impl Simulator for SparseSimulator {
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
        let mut sparse = SparseSimulator::new(3, 0, 0, ());
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

/// The rows of table `key`, each of `N` words.
fn rows<const N: usize>(program: &Value, key: &str) -> Vec<[u64; N]> {
    program[key]
        .as_array()
        .unwrap_or_else(|| panic!("the bytecode should have a `{key}` list"))
        .iter()
        .map(|row| {
            let words: Vec<u64> = row
                .as_array()
                .unwrap_or_else(|| panic!("each row of `{key}` should be a list"))
                .iter()
                .map(|word| word.as_u64().expect("each word should be a u64"))
                .collect();
            words
                .try_into()
                .unwrap_or_else(|_| panic!("each row of `{key}` should have {N} words"))
        })
        .collect()
}

/// The words of the flat list `key`.
fn words(program: &Value, key: &str) -> Vec<u64> {
    program[key]
        .as_array()
        .unwrap_or_else(|| panic!("the bytecode should have a `{key}` list"))
        .iter()
        .map(|word| word.as_u64().expect("each word should be a u64"))
        .collect()
}

fn word(program: &Value, key: &str) -> u64 {
    program[key]
        .as_u64()
        .unwrap_or_else(|| panic!("the bytecode should have a `{key}` number"))
}

fn non_empty<T: Default>(mut table: Vec<T>) -> Vec<T> {
    if table.is_empty() {
        table.push(T::default());
    }
    table
}

/// The program from the JSON of `AdaptiveProgram.as_dict()`, built as
/// `adaptive_program_from_pydict` in `qdk_package` builds it.
fn adaptive_program(json: &str) -> AdaptiveProgram<u64> {
    let program: Value = serde_json::from_str(json).expect("the bytecode should be JSON");
    let count = |key| u32::try_from(word(&program, key)).expect("a count should fit u32");
    AdaptiveProgram {
        num_qubits: count("num_qubits"),
        num_results: count("num_results"),
        num_registers: count("num_registers"),
        entry_block: word(&program, "entry_block"),
        instructions: rows::<8>(&program, "instructions")
            .into_iter()
            .map(|[a, b, c, d, e, f, g, h]| Instruction::from_tuple((a, b, c, d, e, f, g, h)))
            .collect(),
        quantum_ops: rows::<5>(&program, "quantum_ops")
            .into_iter()
            .map(|[a, b, c, d, e]| Op::from_tuple((a, b, c, d, e)))
            .collect(),
        block_table: non_empty(
            rows::<3>(&program, "blocks")
                .into_iter()
                .map(|[_, offset, count]| Block::from_tuple((offset, count)))
                .collect(),
        ),
        function_table: non_empty(
            rows::<3>(&program, "functions")
                .into_iter()
                .map(|[a, b, c]| Function::from_tuple((a, b, c)))
                .collect(),
        ),
        phi_entries: non_empty(
            rows::<2>(&program, "phi_entries")
                .into_iter()
                .map(|[a, b]| PhiNodeEntry::from_tuple((a, b)))
                .collect(),
        ),
        switch_cases: non_empty(
            rows::<2>(&program, "switch_cases")
                .into_iter()
                .map(|[a, b]| SwitchCase::from_tuple((a, b)))
                .collect(),
        ),
        call_args: non_empty(words(&program, "call_args")),
        constant_data: non_empty(words(&program, "constant_data")),
    }
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

#[test]
#[ignore = "needs H₂'s bytecode in QDK_H2_BYTECODE; runs for minutes"]
fn h2_qpe_through_the_protocol_matches_the_legacy_runtime() {
    let path = env::var("QDK_H2_BYTECODE").expect(
        "QDK_H2_BYTECODE should name the JSON of H₂'s AdaptiveProgram.as_dict() \
         (DecomposeCcxPass, then AdaptiveProfilePass(Bytecode.Bit64))",
    );
    let shots = u32::try_from(env_or("QDK_H2_SHOTS", 1)).expect("the shots should fit u32");
    let seed = env_or("QDK_H2_SEED", 0);
    let json = fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let program = adaptive_program(&json);
    let qubits = usize::try_from(program.num_qubits).expect("fits usize");
    let results = usize::try_from(program.num_results).expect("fits usize");
    let prepared = PreparedAdaptiveProgram::new(program).expect("H₂ should prepare");
    let per_shot = PerShot(SimulatorShots::<SparseSimulator>::new(qubits, results, ()));

    let (mut walk_time, mut legacy_time) = (Duration::ZERO, Duration::ZERO);
    let mut histogram = BTreeMap::<String, u64>::new();
    for shot in 0..shots {
        // `SubmissionWrapper` runs an L3 driver on shot 1 of a session, so
        // each shot is shot 1 of its own session, whose seed is `seed + shot`.
        let session_seed = seed + u64::from(shot);
        // The two paths run side by side, each on its own thread.
        let ((walked, walk), (legacy, runtime)) = thread::scope(|scope| {
            let legacy = scope.spawn(|| {
                let start = Instant::now();
                let folded = shot_seed(session_seed, 1);
                let folded = u32::try_from((folded ^ (folded >> 32)) & u64::from(u32::MAX))
                    .expect("masked to 32 bits");
                let mut sim = SparseSimulator::new(qubits, results, folded, ());
                let rows = run_shot(prepared.program(), &mut sim);
                (rows, start.elapsed())
            });
            let start = Instant::now();
            let session =
                open(&per_shot, session_seed, &ShotSet::one(1)).expect("the session opens");
            let mut adapter = RuntimeAdapter::new(SubmissionWrapper::new(session));
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
