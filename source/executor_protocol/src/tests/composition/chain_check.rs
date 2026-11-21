// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The check of a chain by the `on` that makes its runner: every boundary
//! whose sets do not fit, and the fit and the admission reported together.
//! The admission alone is in `chain_check/admission.rs`.

mod admission;

use std::cell::Cell;
use std::rc::Rc;

use crate::prelude::*;
use crate::tests::composition::faults;
use crate::tests::fake_parts::{Bits, CoinCopy, CoinGates, Log, Profiler, in_each_bracketing};
use crate::{BlockType, FitFaults, InstructionDef, InstructionSet, Offer};

/// The instructions of [`CoinGates`] but `missing`, as the set `name`.
fn coin_gates_without(name: &str, missing: &str) -> InstructionSet {
    let defs = [("h", 0), ("x", 0), ("mz", 1)]
        .into_iter()
        .filter(|&(mnemonic, _)| mnemonic != missing)
        .map(|(mnemonic, outcomes)| {
            InstructionDef::new(mnemonic)
                .acts_on([BlockType::qubit()])
                .outcomes(outcomes)
        });
    InstructionSet::new(name, [(BlockType::qubit(), 1)], defs).expect("a valid set")
}

/// [`CoinGates`] without `x`.
pub(super) fn no_x() -> InstructionSet {
    coin_gates_without("NoX", "x")
}

/// [`CoinGates`] without `mz`.
fn no_mz() -> InstructionSet {
    coin_gates_without("NoMz", "mz")
}

/// [`CoinGates`] without `h`.
fn no_h() -> InstructionSet {
    coin_gates_without("NoH", "h")
}

/// The fit faults of `sent` against `accepted`, as the fit finds them.
pub(super) fn fit_faults(sent: &InstructionSet, accepted: &InstructionSet) -> FitFaults {
    FitFaults {
        sent: sent.name().to_owned(),
        accepted: accepted.name().to_owned(),
        mismatches: match sent.fits(accepted) {
            Ok(_) => panic!("a boundary that does not fit"),
            Err(mismatches) => mismatches,
        },
    }
}

// ---------------------------------------------------------------- the fit

/// A code level whose sets do not fit at any of its three boundaries, in
/// each bracketing, so that each join meets a boundary that does not fit
/// (a driver or an adapter through an adapter, either on an executor),
/// and with its joined lower parts lent by `&mut` to the `on` of two
/// chains, as an executor or as an adapter: the runner is never made, and
/// each chain's fault holds each boundary, top to bottom, with both sets
/// named.
#[test]
fn every_boundary_that_does_not_fit_is_a_fault_of_the_chain() {
    let counted = Rc::new(Cell::new(0));
    let expected = || {
        [
            fit_faults(&CoinGates::set(), &no_x()),
            fit_faults(&no_x(), &no_mz()),
            fit_faults(&no_mz(), &no_h()),
        ]
    };
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = CoinCopy::new();
            let a = Profiler::on(no_x(), &counted, &log);
            let b = Profiler::on(no_mz(), &counted, &log);
            let e = Bits::accepting(no_h(), &log);
        },
        |chain, brackets| assert_eq!(faults(chain).fit(), expected(), "{brackets}"),
    );
    let log = Log::default();
    let profiler = |set| Profiler::on(set, &counted, &log);
    let mut lent_executor = profiler(no_x())
        .through(profiler(no_mz()))
        .on(Bits::accepting(no_h(), &log));
    let mut lent_adapter = profiler(no_x()).through(profiler(no_mz()));
    for chain in ["first", "second"] {
        assert_eq!(
            faults(CoinCopy::new().on(&mut lent_executor)).fit(),
            expected(),
            "a lent executor, {chain} chain"
        );
        assert_eq!(
            faults(
                CoinCopy::new()
                    .through(&mut lent_adapter)
                    .on(Bits::accepting(no_h(), &log))
            )
            .fit(),
            expected(),
            "a lent adapter, {chain} chain"
        );
    }
}

// --------------------------------------------------------- the faults together

/// A chain that does not fit and whose needs are unmet: both, at once, in
/// its fields and in its message.
#[test]
fn the_fit_and_the_admission_are_reported_together() {
    let log = Log::default();
    let composed = CoinCopy::new()
        .on(Bits::accepting(no_mz(), &log).offering(Offer::unlimited().max_qubits(1)));
    let faults = faults(composed);
    let fit = fit_faults(&CoinGates::set(), &no_mz());
    assert_eq!(faults.fit(), std::slice::from_ref(&fit));
    let admission = faults.admission().expect("an unmet need");
    let message = faults.to_string();
    assert!(message.contains(&fit.to_string()), "{message}");
    assert!(message.contains(&admission.to_string()), "{message}");
}
