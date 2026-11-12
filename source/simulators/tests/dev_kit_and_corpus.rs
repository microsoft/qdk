// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development branch only (not for review): `MpsResumer` through P2's
//! resumer kit and the `run_shot` corpus, on the exact reference and on the
//! CPU engine.

use std::collections::BTreeMap;

use qdk_cpu_mps::CpuMpsEngine;
use qdk_simulators::mps_simulator::{MpsResumer, MpsResumerError};
use qdk_simulators::qir::{
    QirInstruction, QirMachine, QirProgram, QirResumeError, QirResumer, QirShotId, conformance,
    expect_ready,
};
use qdk_simulators::{MeasurementResult, OutputRecord};
use serde::Deserialize;
use tensornet::MpsEngine;
use tensornet::reference::ExactReference;

#[path = "run_shot_compatibility/bytecode_json.rs"]
mod bytecode_json;

use bytecode_json::Bytecode;

#[test]
fn the_kit_passes_on_the_reference() {
    conformance::check(|| MpsResumer::new(ExactReference::new(), 7));
}

#[test]
fn the_kit_passes_on_the_cpu_engine() {
    conformance::check(|| MpsResumer::new(CpuMpsEngine::default(), 7));
}

#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    shots: u32,
    noise: Option<serde_json::Value>,
    expect: Expect,
    bytecode: Bytecode,
}

#[derive(Deserialize)]
struct Expect {
    every_shot: Option<String>,
    every_shot_records: Option<Vec<Record>>,
    distribution: Option<Distribution>,
    differential_only: Option<String>,
}

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

#[derive(Deserialize)]
struct Distribution {
    support: Vec<String>,
    greater_than: BTreeMap<String, usize>,
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

impl Case {
    /// How the shots fail the test's expectation; empty when they meet it.
    fn misses(&self, engine: &str, shots: &[Vec<OutputRecord>]) -> Vec<String> {
        let miss =
            |shot: usize, what: String| format!("{} on {engine}, shot {shot}: {what}", self.name);
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
                if format!("{records:?}") != format!("{expected:?}") {
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
                        "{} on {engine}: {results} in {count} shots",
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

    /// The misses of the case on `engine`; a noisy case must be refused as
    /// noise (planned).
    #[allow(clippy::match_same_arms)] // Each arm says why the case passes.
    fn run<E: MpsEngine>(&self, engine: E, name: &str) -> Vec<String>
    where
        E::Error: std::fmt::Debug,
    {
        let program = QirProgram::new(self.bytecode.program()).expect("the corpus builds");
        let mut resumer = MpsResumer::new(engine, 0x5eed);
        let run =
            expect_ready(resumer.run(&QirMachine::new(&program), &QirShotId::first(self.shots)));
        match (run, &self.noise) {
            // A noise model is set on the simulator, not in the program:
            // noise on matrix product states is planned, so the case is out
            // of scope here.
            (_, Some(_)) => Vec::new(),
            (Ok(shots), None) => self.misses(name, &shots),
            // A program calling noise or loss checks itself: refused, as
            // planned.
            (
                Err(QirResumeError::Resumer {
                    error:
                        MpsResumerError::Unsupported {
                            instruction:
                                QirInstruction::CorrelatedNoise
                                | QirInstruction::ReadoutNoise
                                | QirInstruction::PeekLoss,
                        },
                    ..
                }),
                None,
            ) => Vec::new(),
            (Err(error), None) => vec![format!("{} on {name}: {error:?}", self.name)],
        }
    }
}

fn cases() -> Vec<Case> {
    let corpus: Corpus =
        serde_json::from_str(include_str!("run_shot_compatibility/run_shot_cases.json"))
            .expect("the corpus is valid JSON");
    corpus.cases
}

#[test]
fn every_shot_of_the_corpus_meets_its_python_test_on_the_reference() {
    let misses: Vec<String> = cases()
        .iter()
        .flat_map(|case| case.run(ExactReference::new(), "reference"))
        .collect();
    assert!(misses.is_empty(), "{misses:#?}");
}

#[test]
fn every_shot_of_the_corpus_meets_its_python_test_on_the_cpu_engine() {
    let misses: Vec<String> = cases()
        .iter()
        .flat_map(|case| case.run(CpuMpsEngine::default(), "cpu"))
        .collect();
    assert!(misses.is_empty(), "{misses:#?}");
}
