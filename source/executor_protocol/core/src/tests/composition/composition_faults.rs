// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Why the `on` that makes a runner refused its chain: each fault names
//! its parts and says every mismatch.

use crate::prelude::*;
use crate::tests::composition::faults;
use crate::tests::example_sets::{
    QubitsAndResetShort, Resettable, TooFewQubits, UnacceptedNames, qubits_and_reset,
};
use crate::tests::fake_parts::{
    AncillaReuse, Bits, CoinCopy, Log, Program, Renamed, ResettableProgram, Stating, Zeros,
    batches_of, coin_gates,
};
use crate::{BoundaryMismatch, CompositionFaults, Features, PartId, ResourceMismatch};

use super::driver_joins::{NO_X, bits_over, instructions, part, program, short_batches};

/// Faults of each kind, a set's and a resource's at one pair included,
/// and a set's short on two needs at one pair, with parts named by their
/// types, or, as a part built at run time is, by
/// what it was built from: each fault names its parts by those names, in
/// its fields and in its message, beside each mismatch's own message,
/// whose wording is pinned by its own type's tests. A boundary's mismatch
/// alone, as a joined part's `ready` refuses with it, says each of its
/// mismatches, a set's and a resource's both.
#[test]
fn a_composition_fault_names_its_parts_and_every_mismatch() {
    let log = Log::default();
    let named = |position, name| PartId::new(position, name);
    let x = UnacceptedNames(vec!["x"]).to_string();
    let batches_of_nine = |provided| {
        ResourceMismatch::InstructionsPerBatch {
            required: 9,
            provided,
        }
        .to_string()
    };
    let (nine_over_eight, nine_over_four) = (batches_of_nine(8), batches_of_nine(4));
    let short = |qubits, reset| QubitsAndResetShort { qubits, reset }.to_string();
    let ten_over_eight = TooFewQubits {
        required: 10,
        provided: 8,
    };
    let (qubits, reset) = (short(Some(ten_over_eight), false), short(None, true));
    let said = |faults: CompositionFaults| {
        let named: Vec<_> = (faults.faults().iter())
            .map(|fault| (fault.requirer.clone(), fault.provider.clone()))
            .collect();
        (named, faults.to_string())
    };
    let rows = [
        (
            "a set",
            said(faults(CoinCopy::new().on(Bits::accepting(NO_X, &log)))),
            vec![(part::<CoinCopy>(0), part::<Bits>(1))],
            vec![&x],
        ),
        (
            "batches",
            said(faults(
                program(9, Features::nothing_stated()).on(bits_over(8, &log)),
            )),
            vec![(part::<Program>(0), part::<Bits>(1))],
            vec![&nine_over_eight],
        ),
        (
            "a set and batches at one pair",
            said(faults(program(9, Features::nothing_stated()).on(
                Bits::accepting(NO_X, &log).with_resources(batches_of(8)),
            ))),
            vec![(part::<Program>(0), part::<Bits>(1))],
            vec![&x, &nine_over_eight],
        ),
        (
            "a set short on qubits and a reset at one pair",
            said(faults(
                ResettableProgram::new(10, true)
                    .through(AncillaReuse::new())
                    .on(Zeros::providing(
                        Resettable,
                        Some(qubits_and_reset(8, false)),
                    )),
            )),
            vec![(part::<ResettableProgram>(0), part::<Zeros<Resettable>>(2))],
            vec![&qubits, &reset],
        ),
        (
            "parts built at run time",
            said(faults(
                Renamed::new(program(9, Features::nothing_stated()), "h2 program")
                    .through(Renamed::new(Stating::capping(4, &log), "decoder"))
                    .on(Renamed::new(bits_over(8, &log), "device runtime")),
            )),
            vec![
                (named(0, "h2 program"), named(1, "decoder")),
                (named(0, "h2 program"), named(2, "device runtime")),
            ],
            vec![&nine_over_four, &nine_over_eight],
        ),
        (
            "a boundary's set and batches, alone",
            (
                vec![],
                BoundaryMismatch {
                    capabilities: short_batches(9, 8).capabilities,
                    ..instructions(coin_gates(), &NO_X)
                }
                .to_string(),
            ),
            vec![],
            vec![&x, &nine_over_eight],
        ),
    ];
    for (row, (named, message), parts, mismatches) in rows {
        assert_eq!(named, parts, "{row}");
        let parts = parts
            .iter()
            .flat_map(|(requirer, provider)| [requirer, provider]);
        for said in parts
            .map(ToString::to_string)
            .chain(mismatches.into_iter().cloned())
        {
            assert!(message.contains(&said), "{row}: {said} in {message}");
        }
    }
}
