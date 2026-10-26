// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Whether every shot of H₂ reaches the readout in the same state.
//!
//! H₂'s 1,399 mid-circuit measurements are fair coins, and the gates after
//! each depend on its outcome. If those gates undo the measurement's effect
//! (measurement-based uncomputation), every shot reaches the readout in the
//! same state, whatever its outcomes, and the readout's distribution is that
//! state's marginal. The tests here read the sparse simulator's state where
//! the walk would measure, on shots that [`StateProbe`] runs like the walk's
//! terminal, with the same seeds.

use std::{collections::BTreeMap, env, ops::Range, thread};

use executor_protocol::{Batch, Executor, Immediate, Outcome, Rejected, ShotId};
use num_bigint::BigUint;
use num_complex::Complex64;
use qdk_simulators::{
    MeasurementResult, QubitID, Simulator,
    execution::{
        MeasurementKind, PreparedAdaptiveProgram, RegionInstruction, RuntimeAdapter, RuntimeError,
        UnitaryOperation,
    },
};

use super::{SparseSimulator, h2_program, simulator_seed};

/// The qubits H₂'s row reads, qubit 0 first. The traces show that the shot
/// measures them nowhere else, so the first measurement of one of them starts
/// the readout.
const READOUT: Range<QubitID> = 0..5;

/// A shot's state, read where its walk would measure, by qubit: bit `q` of an
/// amplitude's index is qubit `q`, whatever the simulator's internal order.
struct ShotState {
    amplitudes: Vec<(BigUint, Complex64)>,
}

impl ShotState {
    fn read(sim: &mut SparseSimulator) -> Self {
        let (amplitudes, _) = sim.sim.get_state();
        Self { amplitudes }
    }

    fn nonzero(&self) -> usize {
        self.amplitudes
            .iter()
            .filter(|(_, a)| a.norm_sqr() > 0.0)
            .count()
    }

    fn norm_sqr(&self) -> f64 {
        self.amplitudes.iter().map(|(_, a)| a.norm_sqr()).sum()
    }

    /// |⟨self|other⟩|² of the normalized states: 1 when they are equal up to a
    /// global phase.
    fn fidelity(&self, other: &Self) -> f64 {
        let mut overlap = Complex64::default();
        let mut theirs = other.amplitudes.iter().peekable();
        for (index, mine) in &self.amplitudes {
            while theirs.next_if(|(at, _)| at < index).is_some() {}
            if let Some((_, their)) = theirs.next_if(|(at, _)| at == index) {
                overlap += mine.conj() * their;
            }
        }
        overlap.norm_sqr() / (self.norm_sqr() * other.norm_sqr())
    }

    /// The probability of each value of the [`READOUT`] qubits, as the row
    /// reads them (qubit 0 first), on the normalized state.
    fn readout_distribution(&self) -> BTreeMap<String, f64> {
        let mut distribution: BTreeMap<String, f64> = (0..1_u32 << READOUT.len())
            .map(|value| (row(value), 0.0))
            .collect();
        let norm = self.norm_sqr();
        for (index, amplitude) in &self.amplitudes {
            let value = READOUT
                .clone()
                .filter(|&q| index.bit(q as u64))
                .map(|q| 1 << q)
                .sum();
            *distribution
                .get_mut(&row(value))
                .expect("every row is listed") += amplitude.norm_sqr() / norm;
        }
        distribution
    }
}

/// The readout qubits' `value` as the row reads it: bit `q` is its `q`-th
/// character.
fn row(value: u32) -> String {
    READOUT
        .map(|q| if (value >> q) & 1 == 1 { '1' } else { '0' })
        .collect()
}

/// Where a [`StateProbe`] reads the state and stops its shot.
#[derive(Clone, Copy)]
enum ReadAt {
    /// Before the readout's first measurement.
    Readout,
    /// Before the measurement of that index, counted from 1.
    Measurement(u64),
}

/// A terminal for one shot on the sparse simulator, which runs the walk's
/// batches as [`SimulatorTerminal`](qdk_simulators::execution::SimulatorTerminal)
/// does, with the same seed, so the same outcomes; but which can force one
/// measurement's outcome, and those after it to 0, and which reads the state
/// before the measurement of [`ReadAt`] and fails the shot there.
struct StateProbe {
    sim: SparseSimulator,
    read_at: ReadAt,
    /// The index of the measurement to force, counted from 1, and its
    /// outcome.
    forced: Option<(u64, bool)>,
    /// Whether the measurements after the forced one give 0.
    zeros_after_forced: bool,
    /// The forced measurement's qubit and kind, and its outcome's probability
    /// before the collapse, once forced.
    forced_measurement: Option<(QubitID, MeasurementKind, f64)>,
    measurements: u64,
    /// The outcomes so far, `0` or `1` each, in order.
    history: String,
    /// The unitary operations since the forced measurement, or else since the
    /// last measurement or the start.
    segment: Vec<UnitaryOperation>,
    /// The length of `segment` at each measurement after the forced one.
    segment_measurements: Vec<usize>,
    state: Option<ShotState>,
}

impl StateProbe {
    fn new(qubits: usize, results: usize, seed: u64, shot: ShotId, read_at: ReadAt) -> Self {
        Self {
            sim: SparseSimulator::new(qubits, results, simulator_seed(seed, shot), ()),
            read_at,
            forced: None,
            zeros_after_forced: false,
            forced_measurement: None,
            measurements: 0,
            history: String::new(),
            segment: Vec::new(),
            segment_measurements: Vec::new(),
            state: None,
        }
    }

    fn forcing(self, index: u64, one: bool, zeros_after: bool) -> Self {
        Self {
            forced: Some((index, one)),
            zeros_after_forced: zeros_after,
            ..self
        }
    }

    /// Whether the measurement of `index` comes after the forced one.
    fn after_forced(&self, index: u64) -> bool {
        self.forced.is_some_and(|(forced, _)| index > forced)
    }

    fn reads_before(&self, qubit: QubitID) -> bool {
        match self.read_at {
            ReadAt::Readout => READOUT.contains(&qubit),
            ReadAt::Measurement(index) => self.measurements + 1 == index,
        }
    }

    /// Measures `qubit`, as forced or by the simulator, and resets it to |0⟩
    /// after with `MeasureResetZ`.
    fn measure(&mut self, kind: MeasurementKind, qubit: QubitID) -> bool {
        let index = self.measurements + 1;
        let one = match self.forced {
            Some((forced, one)) if forced == index => {
                let p = self.sim.sim.force_collapse(one, qubit);
                assert!(p > 1e-9, "measurement {index} cannot give {one}: p = {p}");
                self.forced_measurement = Some((qubit, kind, p));
                one
            }
            _ if self.zeros_after_forced && self.after_forced(index) => {
                let p = self.sim.sim.force_collapse(false, qubit);
                assert!(p > 1e-9, "measurement {index} cannot give 0: p = {p}");
                false
            }
            _ => self.sim.sim.measure(qubit),
        };
        if one && kind == MeasurementKind::MeasureResetZ {
            self.sim.sim.x(qubit);
        }
        one
    }
}

/// The walk's `apply_unitary_immediately`, which is the crate's own.
fn apply(sim: &mut SparseSimulator, operation: UnitaryOperation) {
    match operation {
        UnitaryOperation::I { .. } => {}
        UnitaryOperation::X { target } => sim.x(target),
        UnitaryOperation::Y { target } => sim.y(target),
        UnitaryOperation::Z { target } => sim.z(target),
        UnitaryOperation::H { target } => sim.h(target),
        UnitaryOperation::S { target } => sim.s(target),
        UnitaryOperation::SAdj { target } => sim.s_adj(target),
        UnitaryOperation::Sx { target } => sim.sx(target),
        UnitaryOperation::SxAdj { target } => sim.sx_adj(target),
        UnitaryOperation::T { target } => sim.t(target),
        UnitaryOperation::TAdj { target } => sim.t_adj(target),
        UnitaryOperation::Rx { angle, target } => sim.rx(angle, target),
        UnitaryOperation::Ry { angle, target } => sim.ry(angle, target),
        UnitaryOperation::Rz { angle, target } => sim.rz(angle, target),
        UnitaryOperation::Cx { control, target } => sim.cx(control, target),
        UnitaryOperation::Cy { control, target } => sim.cy(control, target),
        UnitaryOperation::Cz { control, target } => sim.cz(control, target),
        UnitaryOperation::Rxx { angle, q1, q2 } => sim.rxx(angle, q1, q2),
        UnitaryOperation::Ryy { angle, q1, q2 } => sim.ryy(angle, q1, q2),
        UnitaryOperation::Rzz { angle, q1, q2 } => sim.rzz(angle, q1, q2),
        UnitaryOperation::Swap { q1, q2 } => sim.swap(q1, q2),
    }
}

impl Executor<Batch<RegionInstruction>> for StateProbe {
    type Response = Immediate<Outcome>;

    fn call(&mut self, batch: &[RegionInstruction]) -> Result<Immediate<Outcome>, Rejected> {
        let mut outcomes = Vec::new();
        for instruction in batch {
            match instruction {
                RegionInstruction::ExecuteRegion { region, .. } => {
                    for &operation in region.operations() {
                        apply(&mut self.sim, operation);
                        self.segment.push(operation);
                    }
                }
                RegionInstruction::Measure(request) => {
                    if self.reads_before(request.qubit) {
                        self.state = Some(ShotState::read(&mut self.sim));
                        // The shot ran, so it fails rather than being rejected.
                        return Ok(Immediate::failed(
                            outcomes,
                            "the probe read the state".to_owned(),
                        ));
                    }
                    let one = self.measure(request.kind, request.qubit);
                    self.measurements += 1;
                    self.history.push(if one { '1' } else { '0' });
                    if self.after_forced(self.measurements) {
                        self.segment_measurements.push(self.segment.len());
                    } else {
                        self.segment.clear();
                    }
                    outcomes.push(Outcome {
                        tag: u32::try_from(request.result_id).expect("H₂'s results fit u32"),
                        value: if one {
                            MeasurementResult::One
                        } else {
                            MeasurementResult::Zero
                        }
                        .into(),
                    });
                }
                RegionInstruction::Reset { qubit } => self.sim.resetz(*qubit),
            }
        }
        Ok(Immediate::done(outcomes))
    }
}

/// Runs `unread`'s shot through the walk until it reads the state.
fn read_state(program: &PreparedAdaptiveProgram<u64>, unread: StateProbe) -> StateProbe {
    let mut adapter = RuntimeAdapter::new(unread);
    let mut rows = Vec::new();
    match adapter.run(program, &mut rows) {
        Err(RuntimeError::Failed(_)) => {}
        other => panic!("the probe should stop the shot where it reads: {other:?}"),
    }
    let probe = adapter.into_inner();
    assert!(probe.state.is_some(), "the probe read the state");
    probe
}

/// `seed:shot` pairs, comma-separated, from `name`.
fn shots(name: &str, default: &str) -> Vec<(u64, ShotId)> {
    env::var(name)
        .unwrap_or_else(|_| default.to_owned())
        .split(',')
        .map(|pair| {
            let (seed, shot) = pair.split_once(':').expect("seed:shot");
            (
                seed.trim().parse().expect("a seed"),
                shot.trim().parse().expect("a shot"),
            )
        })
        .collect()
}

/// `get_state`'s index has qubit `q` at bit `q`, after a swap too, which
/// relabels the simulator's qubits without moving amplitudes.
#[test]
fn the_state_is_read_by_qubit() {
    let mut sim: SparseSimulator = SparseSimulator::new(3, 0, 0, ());
    sim.x(0);
    sim.swap(0, 2);
    sim.h(1);
    let state = ShotState::read(&mut sim);
    let indices: Vec<_> = state
        .amplitudes
        .iter()
        .map(|(index, _)| index.clone())
        .collect();
    assert_eq!(
        indices,
        [BigUint::from(0b100_u32), BigUint::from(0b110_u32)]
    );
    assert!((state.norm_sqr() - 1.0).abs() < 1e-12);
    assert!((state.fidelity(&state) - 1.0).abs() < 1e-12);
    let distribution = state.readout_distribution();
    assert!((distribution["00100"] - 0.5).abs() < 1e-12);
    assert!((distribution["01100"] - 0.5).abs() < 1e-12);
}

/// The global check: the states of several shots just before the readout,
/// with their outcomes so far, the fidelity of each pair, and the readout's
/// exact distribution from the first.
///
/// `QDK_H2_BYTECODE` as for `h2_qpe_measurement_trace`; `QDK_H2_PROBE_SHOTS`
/// lists the shots as `seed:shot` pairs (default `1:1,1:2,2:1,2:2`), each run
/// on its own thread. It prints a line per shot,
///
/// `shot seed shot measurements_before_readout nonzero norm_sqr history`,
///
/// a line per pair, `fidelity seed:shot seed:shot f`, and a line per row,
/// `row record_order chemistry_order p`.
#[test]
#[ignore = "needs H₂'s bytecode in QDK_H2_BYTECODE; runs for minutes"]
fn h2_states_before_the_readout() {
    let (prepared, qubits, results) = h2_program();
    let shots = shots("QDK_H2_PROBE_SHOTS", "1:1,1:2,2:1,2:2");
    let probes: Vec<StateProbe> = thread::scope(|scope| {
        let handles: Vec<_> = shots
            .iter()
            .map(|&(seed, shot)| {
                let unread = StateProbe::new(qubits, results, seed, shot, ReadAt::Readout);
                let prepared = &prepared;
                scope.spawn(move || read_state(prepared, unread))
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("the probe's thread"))
            .collect()
    });
    let states: Vec<&ShotState> = probes
        .iter()
        .map(|probe| probe.state.as_ref().expect("read"))
        .collect();
    for ((seed, shot), (probe, state)) in shots.iter().zip(probes.iter().zip(&states)) {
        println!(
            "shot {seed} {shot} {} {} {:.17e} {}",
            probe.measurements,
            state.nonzero(),
            state.norm_sqr(),
            probe.history
        );
    }
    for a in 0..states.len() {
        for b in a + 1..states.len() {
            let ((sa, ta), (sb, tb)) = (shots[a], shots[b]);
            let fidelity = states[a].fidelity(states[b]);
            println!("fidelity {sa}:{ta} {sb}:{tb} {fidelity:.17e}");
        }
    }
    for (record, p) in states[0].readout_distribution() {
        let chemistry: String = record.chars().rev().collect();
        println!("row {record} {chemistry} {p:.17e}");
    }
}

/// Operations, each with its position in its segment.
type Placed = Vec<(usize, UnitaryOperation)>;

/// The operations only `a` has and those only `b` has, after a longest
/// common subsequence of the two.
fn only_in(a: &[UnitaryOperation], b: &[UnitaryOperation]) -> (Placed, Placed) {
    // common[i][j]: the longest common subsequence of a[i..] and b[j..].
    let mut common = vec![vec![0_u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            common[i][j] = if a[i] == b[j] {
                common[i + 1][j + 1] + 1
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let (mut only_a, mut only_b) = (Vec::new(), Vec::new());
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            (i, j) = (i + 1, j + 1);
        } else if j == b.len() || (i < a.len() && common[i + 1][j] >= common[i][j + 1]) {
            only_a.push((i, a[i]));
            i += 1;
        } else {
            only_b.push((j, b[j]));
            j += 1;
        }
    }
    (only_a, only_b)
}

/// The local check: at chosen mid-circuit measurements of one shot, forces
/// each outcome, runs what the walk runs for it, and compares the two states
/// where it stops.
///
/// `QDK_H2_BYTECODE` as for `h2_qpe_measurement_trace`; `QDK_H2_FORCED_SHOT`
/// is the shot as `seed:shot` (default `1:1`) and `QDK_H2_FORCED_AT` the
/// measurements, comma-separated (default `10,693,1388,1388-1400`): `d` forces
/// the `d`-th measurement and reads before the next; `d-e` forces the `d`-th,
/// forces those after it to 0 in both runs, so that only the `d`-th differs,
/// and reads before the `e`-th. Each forced run replays the shot from the
/// start, on its own thread. It prints a line per entry with the forced
/// measurement's qubit and kind, the probabilities of 0 and 1, the nonzero
/// amplitudes and the operations of the two segments (from the forced
/// measurement to the read), and the fidelity of the two states; then the
/// operations that only one outcome's segment has, after a longest common
/// subsequence, with their positions in the segment, and the segment's
/// lengths at the measurements in between.
#[test]
#[ignore = "needs H₂'s bytecode in QDK_H2_BYTECODE; runs for minutes"]
fn h2_forced_outcomes() {
    let (prepared, qubits, results) = h2_program();
    let (seed, shot) = shots("QDK_H2_FORCED_SHOT", "1:1")[0];
    let entries: Vec<(u64, u64)> = env::var("QDK_H2_FORCED_AT")
        .unwrap_or_else(|_| "10,693,1388,1388-1400".to_owned())
        .split(',')
        .map(|entry| {
            let (forced, read) = entry.split_once('-').unwrap_or((entry, ""));
            let forced: u64 = forced.trim().parse().expect("a measurement index");
            let read = if read.is_empty() {
                forced + 1
            } else {
                read.trim().parse().expect("a measurement index")
            };
            assert!(
                read > forced,
                "{entry}: it reads after the forced measurement"
            );
            (forced, read)
        })
        .collect();
    let probes: Vec<[StateProbe; 2]> = thread::scope(|scope| {
        let handles: Vec<_> = entries
            .iter()
            .map(|&(forced, read)| {
                [false, true].map(|one| {
                    let unread =
                        StateProbe::new(qubits, results, seed, shot, ReadAt::Measurement(read))
                            .forcing(forced, one, read > forced + 1);
                    let prepared = &prepared;
                    scope.spawn(move || read_state(prepared, unread))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|pair| pair.map(|handle| handle.join().expect("the probe's thread")))
            .collect()
    });
    for ((forced, read), [zero, one]) in entries.iter().zip(&probes) {
        let at = usize::try_from(*forced).expect("fits usize") - 1;
        assert_eq!(
            zero.history[..at],
            one.history[..at],
            "the outcomes before the forced one are the shot's"
        );
        assert_eq!(&zero.history[at..=at], "0");
        assert_eq!(&one.history[at..=at], "1");
        assert_eq!(
            zero.history[at + 1..],
            one.history[at + 1..],
            "only the forced outcome differs"
        );
        let (a, b) = (
            zero.state.as_ref().expect("read"),
            one.state.as_ref().expect("read"),
        );
        let (qubit, kind, p_zero) = zero.forced_measurement.expect("forced");
        let (_, _, p_one) = one.forced_measurement.expect("forced");
        println!(
            "forced {forced} read {read} qubit {qubit} {kind:?} p0 {p_zero:.17e} \
             p1 {p_one:.17e} nonzero {} {} ops {} {} fidelity {:.17e}",
            a.nonzero(),
            b.nonzero(),
            zero.segment.len(),
            one.segment.len(),
            a.fidelity(b)
        );
        let (only_zero, only_one) = only_in(&zero.segment, &one.segment);
        println!("  only after 0: {only_zero:?}");
        println!("  only after 1: {only_one:?}");
        println!(
            "  measurements at {:?} after 0, {:?} after 1",
            zero.segment_measurements, one.segment_measurements
        );
    }
}
