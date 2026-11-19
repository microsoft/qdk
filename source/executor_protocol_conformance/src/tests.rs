// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The kit's tests: each check passes a part that keeps its rule, and
//! reports a part that breaks it, with the rule and every place where it
//! broke it. The parts are fakes on a toy whose shot state is classical
//! bits, written in QIR's `h`, `x` and `mz`: an executor
//! ([`FakeExecutor`](fake_executor::FakeExecutor)), an adapter
//! ([`FakeAdapter`](fake_adapter::FakeAdapter)) and a driver
//! ([`FakeDriver`](fake_driver::FakeDriver)), each able to break one
//! rule at a time.

mod adapter;
mod driver;
mod executor;
mod fake_adapter;
mod fake_driver;
mod fake_executor;
mod nonconformance;
mod script;
mod trials;

use executor_protocol::{Instruction, Needs, ShotSet, instruction_set};

use crate::{Nonconformance, Place, Rule, Script, Trials};

instruction_set! {
    /// QIR's `h`, `x` and `mz`: what every fake part sends and accepts.
    pub(crate) CoinGates { H = h(qubit), X = x(qubit), MZ = mz(qubit) -> 1 outcome }
}

/// The qubits each fake shot holds.
const QUBITS: u32 = 4;

/// The instruction `mnemonic` of [`CoinGates`] on `qubit`.
fn gate(mnemonic: &str, qubit: u32) -> Instruction {
    CoinGates::set()
        .instruction(mnemonic, [qubit], &[])
        .expect("an instruction of CoinGates")
}

/// The instructions `gates`, each a mnemonic and a qubit.
fn gates(gates: &[(&str, u32)]) -> Vec<Instruction> {
    gates
        .iter()
        .map(|&(mnemonic, qubit)| gate(mnemonic, qubit))
        .collect()
}

/// The shots `ids`, in that order.
fn shots(ids: impl IntoIterator<Item = u32>) -> ShotSet {
    ShotSet::ids(ids)
}

/// What the kit's driver needs of the fakes: their qubits.
fn needs() -> Needs {
    Needs::qubits(QUBITS)
}

/// A script of two `execute`s on four shots: the first tosses a bit and
/// flips another on every shot, and reads both; the second flips the
/// first bit of shots 0 and 1, and tosses the second of shots 2 and 3, in
/// two batches, and reads it. So the shots read differently, and each
/// `execute` changes what it reads when it runs again.
fn script() -> Script {
    Script::new()
        .execute([(
            shots(0..4),
            gates(&[("h", 0), ("x", 1), ("mz", 0), ("mz", 1)]),
        )])
        .execute([
            (shots([0, 1]), gates(&[("x", 0), ("mz", 0)])),
            (shots([2, 3]), gates(&[("h", 1), ("mz", 1)])),
        ])
}

/// `first`, for the checks of a shot's state: it tosses a bit and flips
/// another.
fn first() -> Vec<Instruction> {
    gates(&[("h", 0), ("x", 1)])
}

/// `second`, for the checks of a shot's state: it reads both bits `first`
/// changes.
fn second() -> Vec<Instruction> {
    gates(&[("mz", 0), ("mz", 1)])
}

/// The trials every test runs: the default ones, as an implementer's
/// tests run them.
fn trials() -> Trials {
    Trials::default()
}

/// The places of `rule`'s faults in `result`, which reports only `rule`
/// broken.
fn faults_of(rule: Rule, result: Result<(), Nonconformance>) -> Vec<Place> {
    faults_found(rule, result)
        .into_iter()
        .map(|(place, _)| place)
        .collect()
}

/// `rule`'s faults in `result`, which reports only `rule` broken, each its
/// place and what was found there, in the order the check found them;
/// panics if the check passed or could not judge the part.
fn faults_found(rule: Rule, result: Result<(), Nonconformance>) -> Vec<(Place, String)> {
    match result {
        Err(Nonconformance::Broken { rules, .. }) => {
            let broken: Vec<Rule> = rules.iter().map(|broken| broken.rule).collect();
            assert_eq!(broken, [rule], "the rules broken: {rules:?}");
            rules
                .into_iter()
                .flat_map(|broken| broken.faults)
                .map(|fault| (fault.place, fault.found))
                .collect()
        }
        other => panic!("expected broken rules, got {other:?}"),
    }
}

/// Asserts that `result` reports only `rule` broken, with, for each
/// `(found_in, expected_in)` of `comparisons`, a fault found by comparing
/// the records `found_in` with those `expected_in`: so each comparison of
/// the check shows the fault. A label is matched whole, up to the comma
/// that ends it, so that one is not taken for a longer one it begins.
fn shows_each(rule: Rule, result: Result<(), Nonconformance>, comparisons: &[(&str, &str)]) {
    let found: Vec<String> = faults_found(rule, result)
        .into_iter()
        .map(|(_, found)| found)
        .collect();
    for (found_in, expected_in) in comparisons {
        assert!(
            found
                .iter()
                .any(|found| found.contains(&format!(" {found_in}, "))
                    && found.ends_with(&format!(" {expected_in}"))),
            "no fault {found_in}, against {expected_in}: {found:#?}"
        );
    }
}

/// Why `result` could not judge the part; panics if it judged it.
fn why_cannot_judge(result: Result<(), Nonconformance>) -> String {
    match result {
        Err(Nonconformance::CannotJudge { why, .. }) => why,
        other => panic!("expected the check not to judge the part, got {other:?}"),
    }
}

/// The rules the checks of an executor's rules, run in their order, write
/// they did not judge on a part whose offer is not replayable: the rules
/// whose every comparison is of runs sending the shots otherwise, then
/// those whose guard compares runs sent alike, which the check could not
/// run.
const NOT_JUDGED_IF_NOT_REPLAYABLE: [Rule; 5] = [
    Rule::GivesEachShotWhatItGivesAlone,
    Rule::ContinuesEachShotAcrossExecutes,
    Rule::ChangesNoShotOutsideItsBatches,
    Rule::StartsEachShotAfresh,
    Rule::DoesNothingUntilPolled,
];

/// The rule each of `notes`, written on stderr, names as not judged
/// because the part is not replayable, in their order: each note that says
/// the part is not replayable names one of
/// [`NOT_JUDGED_IF_NOT_REPLAYABLE`].
fn not_judged_as_not_replayable(notes: &[String]) -> Vec<Rule> {
    notes
        .iter()
        .filter(|note| note.contains("not replayable"))
        .map(|note| {
            NOT_JUDGED_IF_NOT_REPLAYABLE
                .into_iter()
                .find(|rule| note.contains(&format!("{rule:?}")))
                .unwrap_or_else(|| panic!("a note naming no rule it did not judge: {note}"))
        })
        .collect()
}
