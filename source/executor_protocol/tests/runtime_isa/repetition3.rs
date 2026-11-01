// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The repetition code of the Python runtime's fixtures
//! (`repetition3.qodec.yaml`, in `qdk_package`'s
//! `tests/ec_tests/runtime/fixtures`), in part, transcribed as the data a
//! Qodec loader would give: three of its logical instructions (`prepare_z`,
//! `x` and `m`), the instructions of its gadgets' set (`stim+rz`) that
//! their gadgets call, and their gadgets, the fixture's Stim circuits
//! written as calls.
//!
//! What the data drops: the fixture's other instructions (`idle`, whose
//! checks use ancillas, `mresetz`, `rz`, which takes a parameter, `y` and
//! `z`); the Pauli frame terms of the readouts (`in[0].z[0]`), which count
//! as 0 here, since nothing updates the frame; the code itself
//! (`repetition3.code.yaml`), whose stabilizers the frames name, and whose
//! logical operators the actions name by index only; the sets' `blocks`,
//! the code or qubit an operand holds; and the instructions' descriptions.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use executor_protocol::{Action, InstructionSet, OperandKind, Pauli};

use crate::code::{Code, Gadget, GadgetCall, Slot};

use OperandKind::{Block, Tag};

/// The code, as a loader gives it.
pub fn repetition3() -> Code {
    let accepts = InstructionSet::new("repetition3")
        .with_action("prepare_z", [Block], reset())
        .with_action("__quantum__qis__x__body", [Block], pauli(Pauli::x(0)))
        .with_action("__quantum__qis__m__body", [Block, Tag], measure());
    // The fixture's circuits, as `R 0 1 2`: one call per qubit of the block,
    // each `M` making the next circuit readout.
    let gadget = |mnemonic: &str, readouts| Gadget {
        calls: (0..3)
            .map(|q| {
                let readout = (mnemonic == "M").then_some(Slot::Readout(q));
                GadgetCall {
                    mnemonic: mnemonic.to_owned(),
                    operands: [Slot::Qubit(q)].into_iter().chain(readout).collect(),
                }
            })
            .collect(),
        readouts,
    };
    let gadgets = [
        ("prepare_z", gadget("R", Vec::new())),
        ("__quantum__qis__x__body", gadget("X", Vec::new())),
        // The logical readout is the first qubit's, as the fixture reads it.
        ("__quantum__qis__m__body", gadget("M", vec![vec![0]])),
    ];
    Code {
        accepts: Arc::new(accepts),
        sends: stim(),
        block_size: NonZeroU32::new(3).expect("3 is not 0"),
        gadgets: gadgets
            .into_iter()
            .map(|(mnemonic, gadget)| (mnemonic.to_owned(), gadget))
            .collect::<BTreeMap<_, _>>(),
    }
}

/// The instructions of the fixture's `stim+rz` set that the gadgets call,
/// as the fixture declares them.
pub fn stim() -> InstructionSet {
    InstructionSet::new("stim+rz")
        .with_action("R", [Block], reset())
        .with_action("M", [Block, Tag], measure())
        .with_action("X", [Block], pauli(Pauli::x(0)))
}

/// Qodec's `{out: [qubit], action: [stabilize: Z_0]}`.
pub fn reset() -> Action {
    Action::new([], [1]).stabilize([Pauli::z(0)])
}

/// Qodec's `{in: [qubit], action: [observe: Z_0]}`.
pub fn measure() -> Action {
    Action::new([1], []).observe([Pauli::z(0)])
}

/// Qodec's `{in: [qubit], out: [qubit], action: [pauli: …]}`.
pub fn pauli(pauli: Pauli) -> Action {
    Action::new([1], [1]).pauli(pauli)
}
