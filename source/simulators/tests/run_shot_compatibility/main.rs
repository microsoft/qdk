// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The compatibility of the machine with `run_shot`, the bytecode's
//! reference interpreter, which the Python package runs today: the cases
//! of the Python tests of `run_shot` (`qdk_package`'s
//! `tests/test_adaptive_cpu_bytecode.py` and
//! `tests/test_recursive_call_frames.py`), each as the bytecode `run_qir`
//! hands the simulator, the shots, seed and noise it runs them with, and
//! what the test expects of every shot (`run_shot_cases.json`). The
//! machine, resumed by the [`QirSimulatorResumer`] and by the
//! [`QirGroupingResumer`], must meet every expectation and record what
//! `run_shot` records, shot by shot.
//!
//! This test is removed once the simulators run QIR through the machine,
//! as the Python tests then run it. Until then, it guards the two
//! interpreters of one bytecode against drifting apart, on what the
//! Python pass really writes.
//!
//! # Where the machine differs from `run_shot`
//!
//! On the same simulator, seeded alike, every shot records what `run_shot`
//! records, but in the cases below.
//!
//! - A program that records nothing gives every result, as the package's
//!   `run_adaptive` adds them after `run_shot`, which records none; a result
//!   whose readout came back with no value then traps at its return, as its
//!   record would.
//! - A readout with no value, for a reason other than a loss, which
//!   `run_shot`'s simulators never give, leaves its result with none: the
//!   shot traps only where the program uses it (`QirReadout::Missing`).
//! - Where `run_shot` panics on a value that only a run knows, the shot
//!   traps, naming the instruction (`QirTrap`): a division by zero; a load
//!   past the shot's memory; a qubit past the program's, held in a
//!   register, on a quantum call; a result past the program's, held in a
//!   register, that a measurement or a `WRITE_RESULT` writes; a value other
//!   than 0 and 1 that a `WRITE_RESULT` writes; and correlated noise whose
//!   targets reach past the call arguments or name no register. Correlated
//!   noise on a qubit past the program's traps too, where `run_shot` panics
//!   only when the noise draws a fault on it.
//! - A value other than 0 and 1 read as a bit elsewhere (a branch's or a
//!   select's condition, a bool recorded) traps too (`NotABit`), where
//!   `run_shot` reads it as true, as the GPU shader and `_qodec` read a
//!   condition, and `_qodec` a bool recorded. Such a value can only come
//!   from the pass, which does not wrap narrow integer results (`add i1
//!   true, true` leaves 2, where LLVM gives false), so the shot stops to
//!   show the bug. Building refuses a constant other than 0 and 1 where a
//!   bit is read.
//! - A store past the shot's memory traps (`StorePastMemory`), where
//!   `run_shot` grows its memory up to the address: such a store is a
//!   producer's error, and a huge address would allocate without bound.
//! - The `FCMP` predicates that `run_shot` lacks and panics on (all but
//!   `oeq`, `one`, `olt`, `ole`, `ogt` and `oge`) run in the machine.
//! - Readout noise on a result that a `WRITE_RESULT` wrote last: the write
//!   is the machine's and makes no call, so a resumer applies the noise to
//!   its quantum state's earlier value of the result, where `run_shot`
//!   writes the value into its simulator first.
//! - An `ALLOCA` whose end is past the last address panics in the machine,
//!   while `run_shot` runs on in a release build. Otherwise neither caps
//!   the memory nor the calls under way, unlike the GPU shader, which stops
//!   a shot past 8 calls under way or past its fixed memory: a recursion
//!   without end runs until the process's memory runs out.
//! - A loop of quantum calls that never stops adds each call to one batch,
//!   which grows until the process's memory runs out, where `run_shot`
//!   applies each call as it comes.
//! - A result held in a register past the program's that an instruction
//!   reads, checks for loss or records traps (`QirTrap::NoSuchResult`),
//!   while `run_shot` takes it as `Zero`, not lost.
//! - A `PHI` with no entry for a predecessor of its block is refused when
//!   built, also where no run takes that edge: `run_shot`, the GPU shader
//!   and `_qodec` write nothing there, and the register keeps its earlier
//!   value. No valid QIR gives one (`QirError::MissingPhiEntry` quotes
//!   LLVM's rule).
//! - A `PHI` in an entry block (the program's, or that of a function a
//!   `CALL` names) is refused when built: `run_shot`, the GPU shader and
//!   `_qodec` take the entry for the caller's last edge, since a `CALL`
//!   keeps it (at the program's entry, the entry for block 0). No valid QIR
//!   gives one (`QirError::PhiInEntryBlock` quotes LLVM's rule).
//! - A `PHI` after an instruction other than a `PHI` or a `MOV` of an
//!   immediate is refused when built: `run_shot`, the GPU shader and
//!   `_qodec` run it, taking the entry for the last edge, which a `CALL`
//!   before it restores at its return: the edge into the `PHI`'s block.
//!   No valid QIR gives one; a `MOV` of an immediate is accepted there,
//!   unlike in the QIR, as the adaptive pass puts one there for each
//!   constant value of a `phi` (`QirError::PhiAfterInstruction` quotes
//!   LLVM's rule and says why). Such a `MOV` leaves the last edge as it is,
//!   so the `PHI` takes the same entry.
//!   A question for the review: should the pass put these `MOV`s in the
//!   predecessor blocks instead, so that building could follow the QIR's
//!   rule exactly and refuse a `PHI` after any instruction but a `PHI`?
//! - A jump, a branch, or a switch's default or case into an entry block is
//!   refused when built: `run_shot`, the GPU shader and `_qodec` take it as
//!   any other edge. No valid QIR gives one (`QirError::EdgeToEntryBlock`
//!   quotes LLVM's rule).
//! - A few rules of building are stricter than a run needs, each saying so
//!   and why in its `QirError` variant (`src/qir/fault.rs`). Among them,
//!   a `RET` inside a called function is refused
//!   (`ProgramEndInsideFunction`), which `run_shot` takes as the program's
//!   end; and so is a constant result past the program's that an
//!   instruction reads, checks for loss or records (`NoSuchResult`), which
//!   `run_shot` takes as `Zero`.

use std::collections::BTreeMap;
use std::sync::Arc;

use qdk_simulators::bytecode::AdaptiveProgram;
use qdk_simulators::bytecode::runtime::run_shot;
use qdk_simulators::cpu_full_state_simulator::FullStateSimulator;
use qdk_simulators::noise_config::{CumulativeNoiseConfig, LossPolicy, NoiseConfig, NoiseTable};
use qdk_simulators::qir::{QirMachine, QirProgram, QirResumer, QirShotId, expect_ready};
use qdk_simulators::stabilizer_simulator::StabilizerSimulator;
use qdk_simulators::{MeasurementResult, OutputRecord, Simulator};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use serde::Deserialize;

mod bytecode_json;
/// The bytecode's encoding, which the crate keeps to itself.
#[allow(dead_code)]
#[path = "../../src/bytecode/encoding.rs"]
mod encoding;
// `dispatches()`, which the grouping resumer's tests read, is not used.
#[allow(dead_code)]
#[path = "../../examples/qir_resumers/grouping_resumer.rs"]
mod grouping_resumer;
#[path = "../../examples/qir_resumers/simulator_resumer.rs"]
mod simulator_resumer;

use bytecode_json::Bytecode;
use encoding::OP_RECORD_OUTPUT;
use grouping_resumer::QirGroupingResumer;
use simulator_resumer::QirSimulatorResumer;

/// The corpus file.
#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}

/// One case of a Python test: one program, run for a number of shots.
#[derive(Deserialize)]
struct Case {
    /// The pytest id, without the simulator. The exception: the four
    /// cases of the GPU tests of call depth
    /// (`test_compiled_recursion_{within,exceeding}_gpu_call_depth_…`) are
    /// named by the test and its depth and value (`[6-0]`), since their
    /// ids name the GPU, and their program is run here on `run_shot`.
    name: String,
    /// The simulators the test runs it on: `cpu`, `stabilizer`,
    /// `clifford`. The cases of the GPU tests of call depth list `cpu`:
    /// the test expects the value of Q#'s evaluation, which it checks on
    /// the CPU for depths 8 and 9 (`run_one_shot(qir, "cpu")`), and which
    /// `run_shot` is held to here for every depth.
    sim_types: Vec<String>,
    shots: u32,
    /// The seed `run_adaptive` draws each shot's seed from.
    seed: u64,
    noise: Option<Noise>,
    expect: Expect,
    bytecode: Bytecode,
}

/// The noise of a case: the tables it sets, by gate; the others are
/// noiseless.
#[derive(Deserialize)]
struct Noise {
    tables: BTreeMap<String, Table>,
}

#[derive(Deserialize)]
struct Table {
    qubits: u32,
    on_loss: String,
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    /// The Pauli and loss string, encoded as `NoiseTable` holds it.
    encoded: u64,
    probability: f64,
}

/// What a test expects of every shot; one field is set.
#[derive(Deserialize)]
struct Expect {
    /// The results of every shot ([`results`]).
    every_shot: Option<String>,
    /// The records of every shot.
    every_shot_records: Option<Vec<Record>>,
    distribution: Option<Distribution>,
    /// Why the test checks no outcome: every shot records at least four
    /// results ([`results`]), none of them `Loss`.
    differential_only: Option<String>,
}

/// A record, as the corpus writes it.
#[derive(Deserialize)]
enum Record {
    Result(Outcome),
    Bool(bool),
    Int(i64),
    Double(f64),
}

#[derive(Deserialize)]
enum Outcome {
    Zero,
    One,
    Loss,
}

/// Shots whose results vary: each shot's results ([`results`]) are one of
/// `support`, and each listed one occurs in more than its count of shots.
#[derive(Deserialize)]
struct Distribution {
    support: Vec<String>,
    greater_than: BTreeMap<String, usize>,
}

/// A simulator a case runs on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Sim {
    FullState,
    Stabilizer,
}

/// What runs the shots of a case, each on a simulator seeded as
/// `run_adaptive` seeds it.
#[derive(Clone, Copy, Debug)]
enum Runner {
    /// The machine, resumed by a [`QirSimulatorResumer`].
    PerShot,
    /// The machine, resumed by a [`QirGroupingResumer`].
    Grouped,
    /// `run_shot`, with `run_adaptive`'s records for a program that records
    /// no output: every measurement, in order.
    Reference,
}

impl Case {
    /// The simulators of the case, each once: `stabilizer` and `clifford`
    /// name the same one.
    fn sims(&self) -> Vec<Sim> {
        let mut sims = Vec::new();
        for name in &self.sim_types {
            let sim = match name.as_str() {
                "cpu" => Sim::FullState,
                "stabilizer" | "clifford" => Sim::Stabilizer,
                other => panic!("{}: no simulator is named {other}", self.name),
            };
            if !sims.contains(&sim) {
                sims.push(sim);
            }
        }
        sims
    }

    /// The case's noise, as `run_adaptive` hands it to a simulator.
    fn noise(&self) -> Arc<CumulativeNoiseConfig> {
        let mut config = NoiseConfig::<f64, f64>::NOISELESS;
        for (gate, table) in self.noise.iter().flat_map(|noise| &noise.tables) {
            let on_loss = match table.on_loss.as_str() {
                "Skip" => LossPolicy::Skip,
                "Propagate" => LossPolicy::Propagate,
                "Degrade" => LossPolicy::Degrade,
                "ResidualSDagger" => LossPolicy::ResidualSDagger,
                other => panic!("{}: no loss policy is named {other}", self.name),
            };
            let table = NoiseTable {
                qubits: table.qubits,
                pauli_strings: table.entries.iter().map(|entry| entry.encoded).collect(),
                probabilities: table
                    .entries
                    .iter()
                    .map(|entry| entry.probability)
                    .collect(),
                on_loss,
            };
            match gate.as_str() {
                "s" => config.s = table,
                "z" => config.z = table,
                "mov" => config.mov = table,
                other => panic!("{}: the corpus sets no noise of {other}", self.name),
            }
        }
        Arc::new(config.into())
    }

    /// The seed of each shot, as `run_adaptive` draws them.
    fn shot_seeds(&self) -> Vec<u32> {
        let mut rng = StdRng::seed_from_u64(self.seed);
        (0..self.shots).map(|_| rng.random()).collect()
    }

    /// The records of every shot of the case on `sim`, run by `runner`.
    fn shots(&self, sim: Sim, runner: Runner) -> Vec<Vec<OutputRecord>> {
        match sim {
            Sim::FullState => self.shots_on::<FullStateSimulator>(runner),
            Sim::Stabilizer => self.shots_on::<StabilizerSimulator>(runner),
        }
    }

    fn shots_on<S: Simulator<Noise = Arc<CumulativeNoiseConfig>>>(
        &self,
        runner: Runner,
    ) -> Vec<Vec<OutputRecord>> {
        let bytecode = self.bytecode.program();
        let seeds = self.shot_seeds();
        let noise = self.noise();
        // Shot `i` of the case is the run's shot of id `i`, so that each
        // resumer's factory seeds it as `run_adaptive` seeds shot `i`.
        let simulator = |shot: QirShotId| {
            S::new(
                bytecode.num_qubits as usize,
                bytecode.num_results as usize,
                seeds[shot.0 as usize],
                noise.clone(),
            )
        };
        let make = |_: &QirProgram, shot| simulator(shot);
        // The program takes its bytecode; `run_shot` reads a second
        // decoding of the case's.
        let program =
            QirProgram::new(self.bytecode.program()).expect("every program of the corpus builds");
        let machine = QirMachine::new(&program);
        let shots = QirShotId::first(self.shots);
        let run = match runner {
            Runner::PerShot => expect_ready(QirSimulatorResumer::new(make).run(&machine, &shots)),
            Runner::Grouped => expect_ready(QirGroupingResumer::new(make).run(&machine, &shots)),
            Runner::Reference => {
                let records_output = records_output(&bytecode);
                return shots
                    .into_iter()
                    .map(|shot| {
                        let mut sim = simulator(shot);
                        let records = run_shot(&bytecode, &mut sim);
                        if records_output {
                            records
                        } else {
                            sim.take_measurements()
                                .into_iter()
                                .map(OutputRecord::Result)
                                .collect()
                        }
                    })
                    .collect();
            }
        };
        run.unwrap_or_else(|error| panic!("{}: a run fails: {error}", self.name))
    }

    /// How `shots`, the records of every shot on `sim`, fail what the test
    /// expects; empty when they meet it.
    fn misses(&self, sim: Sim, shots: &[Vec<OutputRecord>]) -> Vec<String> {
        let miss =
            |shot: usize, what: String| format!("{} on {sim:?}, shot {shot}: {what}", self.name);
        let expect = &self.expect;
        let mut misses = Vec::new();
        if let Some(expected) = &expect.every_shot {
            for (shot, records) in shots.iter().enumerate() {
                if results(records) != *expected {
                    misses.push(miss(shot, format!("{records:?}, not {expected}")));
                }
            }
        } else if let Some(expected) = &expect.every_shot_records {
            let expected: Vec<OutputRecord> = expected.iter().map(Record::output).collect();
            for (shot, records) in shots.iter().enumerate() {
                if *records != expected {
                    misses.push(miss(shot, format!("{records:?}, not {expected:?}")));
                }
            }
        } else if let Some(distribution) = &expect.distribution {
            let mut counts = BTreeMap::<String, usize>::new();
            for (shot, records) in shots.iter().enumerate() {
                let results = results(records);
                if distribution.support.contains(&results) {
                    *counts.entry(results).or_default() += 1;
                } else {
                    misses.push(miss(shot, format!("{records:?}, out of the support")));
                }
            }
            for (results, least) in &distribution.greater_than {
                let count = counts.get(results).copied().unwrap_or_default();
                if count <= *least {
                    misses.push(format!(
                        "{} on {sim:?}: {results} in {count} shots",
                        self.name
                    ));
                }
            }
        } else {
            assert!(
                expect.differential_only.is_some(),
                "{}: no expectation",
                self.name
            );
            for (shot, records) in shots.iter().enumerate() {
                let results = results(records);
                if results.len() < 4 || results.contains('L') {
                    misses.push(miss(shot, format!("{records:?}")));
                }
            }
        }
        misses
    }

    /// Where the records of every shot on `sim`, run by `runner`, differ
    /// from `run_shot`'s, shot by shot; empty when they do not.
    fn differences(&self, sim: Sim, runner: Runner) -> Vec<String> {
        let resumed = self.shots(sim, runner);
        let reference = self.shots(sim, Runner::Reference);
        // Through `Debug`, so that two NaN records are the same.
        resumed
            .into_iter()
            .zip(reference)
            .enumerate()
            .filter(|(_, (resumed, reference))| format!("{resumed:?}") != format!("{reference:?}"))
            .map(|(shot, (resumed, reference))| {
                format!(
                    "{} on {sim:?}, {runner:?}, shot {shot}: {resumed:?}, not {reference:?}",
                    self.name
                )
            })
            .collect()
    }
}

impl Record {
    fn output(&self) -> OutputRecord {
        match self {
            Record::Result(Outcome::Zero) => OutputRecord::Result(MeasurementResult::Zero),
            Record::Result(Outcome::One) => OutputRecord::Result(MeasurementResult::One),
            Record::Result(Outcome::Loss) => OutputRecord::Result(MeasurementResult::Loss),
            Record::Bool(value) => OutputRecord::Bool(*value),
            Record::Int(value) => OutputRecord::Int(*value),
            Record::Double(value) => OutputRecord::Double(*value),
        }
    }
}

/// The results `records` holds, as the Python tests write them
/// (`map_result_list_to_str`): `0` for `Zero`, `1` for `One`, `L` for
/// `Loss`, and nothing for the other records.
fn results(records: &[OutputRecord]) -> String {
    records
        .iter()
        .filter_map(|record| match record {
            OutputRecord::Result(MeasurementResult::Zero) => Some('0'),
            OutputRecord::Result(MeasurementResult::One) => Some('1'),
            OutputRecord::Result(MeasurementResult::Loss) => Some('L'),
            _ => None,
        })
        .collect()
}

/// Whether `bytecode` records output, as `run_adaptive` decides it.
fn records_output(bytecode: &AdaptiveProgram<u64>) -> bool {
    bytecode
        .instructions
        .iter()
        .any(|instruction| instruction.primary_opcode() == OP_RECORD_OUTPUT)
}

/// Every case of the corpus.
fn cases() -> Vec<Case> {
    let corpus: Corpus = serde_json::from_str(include_str!("run_shot_cases.json"))
        .expect("the corpus is valid JSON of its format");
    corpus.cases
}

/// Every program `run_qir` hands `run_shot` in the Python tests.
#[test]
fn every_program_of_the_corpus_builds() {
    let refused: Vec<String> = cases()
        .iter()
        .filter_map(|case| {
            QirProgram::new(case.bytecode.program())
                .err()
                .map(|faults| format!("{}: {faults}", case.name))
        })
        .collect();
    assert!(refused.is_empty(), "{refused:#?}");
}

/// Each Python test's program, run by the machine and a simulator
/// resumer on each simulator the test runs it on, with the test's shots,
/// seed and noise.
#[test]
fn every_shot_meets_what_its_python_test_expects() {
    let misses: Vec<String> = cases()
        .iter()
        .flat_map(|case| {
            case.sims()
                .into_iter()
                .flat_map(|sim| case.misses(sim, &case.shots(sim, Runner::PerShot)))
        })
        .collect();
    assert!(misses.is_empty(), "{misses:#?}");
}

/// Each Python test's program, run by the machine and each resumer, and by
/// `run_shot`, on simulators seeded alike, shot by shot: their simulator
/// operations draw the same randomness only when they are the same operations.
/// The name states the rule at the level the `qir` module promises it: the
/// machine records what `run_shot` records, on every program that builds and
/// that `run_shot` runs, but for readout noise on a result that a
/// `WRITE_RESULT` wrote last, as the module's doc states.
#[test]
fn every_shot_records_what_run_shot_records() {
    // COMPATIBILITY NOTE: every program here ran on `run_shot`
    // (bytecode/runtime.rs) in the Python tests, hence the machine and a
    // resumer give the same records, so that they may replace it.
    let differ: Vec<String> = cases()
        .iter()
        .flat_map(|case| {
            case.sims().into_iter().flat_map(move |sim| {
                [Runner::PerShot, Runner::Grouped]
                    .into_iter()
                    .flat_map(move |runner| case.differences(sim, runner))
            })
        })
        .collect();
    assert!(differ.is_empty(), "{differ:#?}");
}
