// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod action;
mod call;
mod renumbered;

use crate::tests::declared;
use crate::{
    Action, Angle, InstructionCall, InstructionMismatch, InstructionSet, Opcode, Operand,
    OperandKind, Pauli, Rejected,
};

use OperandKind::{Block, Integer, Real, Tag};

/// A small device's set: a rotation, a CNOT and a measurement, each
/// declaring what it does.
fn device() -> InstructionSet {
    declared(
        InstructionSet::new("device")
            .with("rz", [Block, Real])
            .with("cx", [Block, Block])
            .with("measure", [Block, Tag]),
    )
}

/// A set fits itself: an adapter may send all that the downstream executor
/// accepts.
#[test]
fn a_set_fits_itself() {
    assert_eq!(device().fits(&device()), Ok(()));
}

/// A part of a set fits the set, whatever their opcodes: an adapter may
/// send any part of what the downstream executor accepts, declared in its own
/// order (here `measure` before `cx`).
#[test]
fn a_part_of_a_set_fits_the_set() {
    let part = declared(
        InstructionSet::new("part")
            .with("measure", [Block, Tag])
            .with("cx", [Block, Block]),
    );
    assert_eq!(part.fits(&device()), Ok(()));
}

/// An instruction the downstream set lacks does not fit.
#[test]
fn an_instruction_missing_downstream_does_not_fit() {
    let sent = declared(
        InstructionSet::new("sent")
            .with("cx", [Block, Block])
            .with("h", [Block]),
    );
    assert_eq!(
        sent.fits(&device()),
        Err(vec![InstructionMismatch::Missing {
            mnemonic: "h".into()
        }])
    );
}

/// An instruction that takes other operands downstream does not fit, even under
/// the same mnemonic: what is sent could not be read there.
#[test]
fn an_instruction_taking_other_operands_downstream_does_not_fit() {
    let sent = InstructionSet::new("sent").with("rz", [Block]);
    assert_eq!(
        sent.fits(&device()),
        Err(vec![InstructionMismatch::DifferentOperands {
            mnemonic: "rz".into()
        }])
    );
}

/// An instruction whose operands are of the same kinds in another order
/// does not fit: operands are read by position.
#[test]
fn an_instruction_taking_its_operands_in_another_order_downstream_does_not_fit() {
    let sent = InstructionSet::new("sent").with("measure", [Tag, Block]);
    assert_eq!(
        sent.fits(&device()),
        Err(vec![InstructionMismatch::DifferentOperands {
            mnemonic: "measure".into()
        }])
    );
}

/// Fitting goes one way: a set does not fit a part of itself, which lacks
/// some of its instructions.
#[test]
fn a_set_does_not_fit_a_part_of_itself() {
    let part = declared(InstructionSet::new("part").with("cx", [Block, Block]));
    assert_eq!(
        device().fits(&part),
        Err(vec![
            InstructionMismatch::Missing {
                mnemonic: "rz".into()
            },
            InstructionMismatch::Missing {
                mnemonic: "measure".into()
            }
        ])
    );
}

/// A set with no instructions sends nothing, so it fits any set, even one
/// with no instructions.
#[test]
fn an_empty_set_fits_any_set() {
    let empty = InstructionSet::new("empty");
    assert_eq!(empty.fits(&device()), Ok(()));
    assert_eq!(empty.fits(&empty), Ok(()));
}

/// An instruction's opcode is its position in the order the set declares
/// it, from 0.
#[test]
fn an_instructions_opcode_is_its_position_in_the_order_declared() {
    let set = device();
    let index = |mnemonic| set.opcode(mnemonic).map(Opcode::index);
    assert_eq!(
        [index("rz"), index("cx"), index("measure"), index("h")],
        [Some(0), Some(1), Some(2), None]
    );
}

/// An opcode built from an index, as a decoder reads one, is the opcode of
/// the instruction at that position.
#[test]
fn an_opcode_built_from_an_index_is_the_instruction_at_that_position() {
    assert_eq!(device().mnemonic(Opcode::new(1)), Some("cx"));
    assert_eq!(device().opcode("cx"), Some(Opcode::new(1)));
}

/// A part of a set numbers its instructions in its own order, so an opcode
/// means something only in its set: a name is resolved again in each.
#[test]
fn a_part_of_a_set_numbers_its_instructions_in_its_own_order() {
    let part = InstructionSet::new("part").with("measure", [Block, Tag]);
    let measure = part.opcode("measure").expect("the part has `measure`");
    assert_eq!(part.mnemonic(measure), Some("measure"));
    assert_eq!(device().mnemonic(measure), Some("rz"));
}

/// Two sets that declare the same instructions in another order fit each
/// other, though their opcodes differ: each accepts what the other sends.
#[test]
fn sets_declaring_the_same_instructions_in_another_order_fit_each_other() {
    let reversed = declared(
        InstructionSet::new("reversed")
            .with("measure", [Block, Tag])
            .with("cx", [Block, Block])
            .with("rz", [Block, Real]),
    );
    assert_eq!(reversed.fits(&device()), Ok(()));
    assert_eq!(device().fits(&reversed), Ok(()));
}

/// A set lists its mnemonics in the order of their opcodes.
#[test]
fn a_set_lists_its_mnemonics_in_the_order_declared() {
    assert_eq!(
        device().mnemonics().collect::<Vec<_>>(),
        ["rz", "cx", "measure"]
    );
}

/// An instruction added under a mnemonic the set has replaces it: a set has
/// one instruction per mnemonic.
#[test]
fn an_instruction_replaces_the_one_of_its_mnemonic() {
    let set = device().with("rz", [Block]);
    assert_eq!(set.operands("rz"), Some([Block].as_slice()));
    assert_eq!(set.mnemonics().count(), 3);
}

/// An instruction that replaces another keeps its opcode, so calls built
/// for the others stay valid.
#[test]
fn a_replaced_instruction_keeps_its_opcode() {
    let set = device().with("rz", [Block]);
    assert_eq!(set.opcode("rz"), device().opcode("rz"));
    assert_eq!(set.opcode("measure"), device().opcode("measure"));
}

/// A call built by a mnemonic the set lacks is missing.
#[test]
fn a_call_by_a_mnemonic_the_set_lacks_is_missing() {
    assert_eq!(
        device().call("h", [Operand::Block(0)]),
        Err(InstructionMismatch::Missing {
            mnemonic: "h".into()
        })
    );
}

/// A call's operands must be of the kinds its instruction takes, in order:
/// the same operands swapped differ.
#[test]
fn a_call_with_other_operands_differs() {
    let rz = |operands: [Operand; 2]| device().call("rz", operands).map(|_| ());
    assert_eq!(rz([Operand::Block(0), Operand::Real(0.5)]), Ok(()));
    assert_eq!(
        rz([Operand::Real(0.5), Operand::Block(0)]),
        Err(InstructionMismatch::DifferentOperands {
            mnemonic: "rz".into()
        })
    );
}

/// A call must have as many operands as its instruction takes: one with
/// fewer or more differs, even if those it has are of the right kinds.
#[test]
fn a_call_with_another_number_of_operands_differs() {
    let rz = |operands: &[Operand]| {
        device().check(&InstructionCall::new(Opcode::new(0), operands.to_vec()))
    };
    let differs = Err(InstructionMismatch::DifferentOperands {
        mnemonic: "rz".into(),
    });
    assert_eq!(rz(&[Operand::Block(0)]), differs);
    assert_eq!(
        rz(&[Operand::Block(0), Operand::Real(0.5), Operand::Real(0.5)]),
        differs
    );
}

/// A call at an opcode the set lacks (here 3, past its last instruction) is
/// refused by its opcode: the set has no name for it.
#[test]
fn a_call_at_an_opcode_the_set_lacks_is_unknown() {
    let unknown = InstructionCall::new(Opcode::new(3), [Operand::Block(0)]);
    assert_eq!(
        device().check(&unknown),
        Err(InstructionMismatch::UnknownOpcode {
            opcode: Opcode::new(3)
        })
    );
}

/// A batch is refused at its first call outside the set, by its index, with
/// the mismatch as the reason; a later call outside it is not reported.
#[test]
fn a_batch_is_refused_at_its_first_call_outside_the_set() {
    let cx = device()
        .call("cx", [Operand::Block(0), Operand::Block(1)])
        .expect("`cx` takes two blocks");
    let unknown = InstructionCall::new(Opcode::new(3), [Operand::Block(0)]);
    let rz = InstructionCall::new(Opcode::new(0), [Operand::Block(0)]);
    assert_eq!(device().check_batch(&[cx.clone(), cx.clone()]), Ok(()));
    assert_eq!(
        device().check_batch(&[cx, unknown, rz]),
        Err(Rejected::Instruction {
            at: 1,
            reason: InstructionMismatch::UnknownOpcode {
                opcode: Opcode::new(3)
            }
            .to_string(),
        })
    );
}

/// What a reset does, as Stim's `R` in Qodec: prepares a qubit in `Z_0`'s
/// state.
fn reset() -> Action {
    Action::new([], [1]).stabilize([Pauli::z(0)])
}

/// What an X does.
fn x() -> Action {
    Action::new([1], [1]).pauli(Pauli::x(0))
}

/// What a CNOT does.
fn cx() -> Action {
    Action::new([1, 1], [1, 1]).clifford([
        (Pauli::x(0), Pauli::x(0) * Pauli::x(1)),
        (Pauli::z(1), Pauli::z(0) * Pauli::z(1)),
    ])
}

/// What a measurement does, as Stim's `M` in Qodec: observes `Z_0`, one
/// readout, and leaves no qubit.
fn measure() -> Action {
    Action::new([1], []).observe([Pauli::z(0)])
}

/// A toy device's set, each instruction declaring what it does but its
/// coin, a vendor's instruction.
fn toy() -> InstructionSet {
    InstructionSet::new("toy")
        .with("coin", [Block])
        .with_action("x", [Block], x())
        .with_action("cx", [Block, Block], cx())
        .with_action("measure", [Block, Tag], measure())
        .with_action("reset", [Block], reset())
}

/// A program's set in Stim's mnemonics, declaring what each does.
fn stim() -> InstructionSet {
    InstructionSet::new("stim")
        .with_action("R", [Block], reset())
        .with_action("M", [Block, Tag], measure())
        .with_action("CX", [Block, Block], cx())
        .with_action("X", [Block], x())
}

/// The mnemonic in `accepted` of the instruction `mnemonic` of `sent`, as
/// `sent.opcodes_in(&accepted)` matches it.
fn matched<'a>(sent: &InstructionSet, accepted: &'a InstructionSet, mnemonic: &str) -> &'a str {
    let map = sent
        .opcodes_in(accepted)
        .expect("each instruction is matched");
    let opcode = sent.opcode(mnemonic).expect("`sent` has the mnemonic");
    let theirs = map
        .get(opcode)
        .expect("the map holds each opcode of `sent`");
    accepted
        .mnemonic(theirs)
        .expect("the map gives opcodes of `accepted`")
}

/// An instruction that declares what it does is matched to the one that
/// does the same, whatever its mnemonic: Stim's `R` is the toy's `reset`.
#[test]
fn an_instruction_is_matched_by_action_under_another_mnemonic() {
    let (stim, toy) = (stim(), toy());
    let found = ["R", "M", "CX", "X"].map(|mnemonic| matched(&stim, &toy, mnemonic));
    assert_eq!(found, ["reset", "measure", "cx", "x"]);
}

/// Among instructions that do the same, the one of the same mnemonic is
/// chosen, though another comes first.
#[test]
fn the_same_mnemonic_is_preferred_among_instructions_doing_the_same() {
    let accepted = InstructionSet::new("device")
        .with_action("init", [Block], reset())
        .with_action("reset", [Block], reset());
    let sent = InstructionSet::new("sent").with_action("reset", [Block], reset());
    assert_eq!(matched(&sent, &accepted, "reset"), "reset");
}

/// An instruction whose action either set leaves undeclared is refused,
/// though the other set has its mnemonic with the same operands: a
/// mnemonic alone is not a match, since what it does cannot be checked.
#[test]
fn an_instruction_whose_action_either_set_leaves_undeclared_is_refused() {
    let accepted = InstructionSet::new("device")
        .with("coin", [Block])
        .with("x", [Block])
        .with_action("reset", [Block], reset());
    let sent = InstructionSet::new("sent")
        .with("reset", [Block])
        .with("coin", [Block])
        .with_action("x", [Block], x());
    let undeclared = |mnemonic: &str| InstructionMismatch::UndeclaredAction {
        mnemonic: mnemonic.into(),
    };
    assert_eq!(
        sent.opcodes_in(&accepted),
        Err(vec![
            undeclared("reset"),
            undeclared("coin"),
            undeclared("x")
        ])
    );
}

/// An instruction that declares no action is found only by its mnemonic:
/// one that does what it is named for under another mnemonic is missing.
#[test]
fn an_instruction_without_an_action_is_not_matched_under_another_mnemonic() {
    let sent = InstructionSet::new("sent").with("X", [Block]);
    assert_eq!(
        sent.opcodes_in(&toy()),
        Err(vec![InstructionMismatch::Missing {
            mnemonic: "X".into()
        }])
    );
}

/// An instruction that does something else under the same mnemonic, with
/// none doing the same, differs by action: matching it by its mnemonic would
/// run another operation.
#[test]
fn an_instruction_doing_something_else_under_its_mnemonic_differs() {
    let z = Action::new([1], [1]).pauli(Pauli::z(0));
    let sent = InstructionSet::new("sent").with_action("x", [Block], z);
    assert_eq!(
        sent.opcodes_in(&toy()),
        Err(vec![InstructionMismatch::DifferentAction {
            mnemonic: "x".into()
        }])
    );
}

/// An instruction that does the same under its mnemonic but takes other
/// operands differs by its operands, not its action: what is sent could
/// not be read there.
#[test]
fn an_instruction_doing_the_same_with_other_operands_differs_by_its_operands() {
    let rz = || Action::new([1], [1]).rotate(Pauli::z(0), Angle::Parameter(0));
    let accepted = InstructionSet::new("device").with_action("rz", [Block, Real], rz());
    let sent = InstructionSet::new("sent").with_action("rz", [Block, Integer], rz());
    assert_eq!(
        sent.opcodes_in(&accepted),
        Err(vec![InstructionMismatch::DifferentOperands {
            mnemonic: "rz".into()
        }])
    );
}

/// A program in Stim's mnemonics sent to a device of another name whose
/// own `R`, a vendor's instruction, declares nothing, and whose `rz` takes
/// an integer angle: Stim's `R` and `rz` are matched to the instructions
/// that do the same with the same operands, `init` and `rotate_z`, not
/// refused for the instructions of their mnemonics.
#[test]
fn an_instruction_is_matched_by_action_past_one_of_its_mnemonic_that_does_not_fit() {
    let rz = || Action::new([1], [1]).rotate(Pauli::z(0), Angle::Parameter(0));
    let accepted = InstructionSet::new("device")
        .with("R", [Block])
        .with_action("init", [Block], reset())
        .with_action("rz", [Block, Integer], rz())
        .with_action("rotate_z", [Block, Real], rz());
    let sent = InstructionSet::new("stim")
        .with_action("R", [Block], reset())
        .with_action("rz", [Block, Real], rz());
    let found = ["R", "rz"].map(|mnemonic| matched(&sent, &accepted, mnemonic));
    assert_eq!(found, ["init", "rotate_z"]);
}

/// Every instruction not matched is reported, whatever the reason, in the
/// order the sent set declares them; those matched are not.
#[test]
fn every_instruction_not_matched_is_reported() {
    let z = Action::new([1], [1]).pauli(Pauli::z(0));
    let sent = InstructionSet::new("sent")
        .with("h", [Block])
        .with_action("R", [Block], reset())
        .with_action("x", [Block], z)
        .with("measure", [Block])
        .with("coin", [Block]);
    assert_eq!(
        sent.opcodes_in(&toy()),
        Err(vec![
            InstructionMismatch::Missing {
                mnemonic: "h".into()
            },
            InstructionMismatch::DifferentAction {
                mnemonic: "x".into()
            },
            InstructionMismatch::DifferentOperands {
                mnemonic: "measure".into()
            },
            InstructionMismatch::UndeclaredAction {
                mnemonic: "coin".into()
            },
        ])
    );
}

/// The map gives each instruction's opcode in the other set, by its opcode
/// in its own, and none at an opcode its own set lacks.
#[test]
fn the_map_gives_each_instructions_opcode_in_the_other_set() {
    let reversed = declared(
        InstructionSet::new("reversed")
            .with("measure", [Block, Tag])
            .with("cx", [Block, Block])
            .with("rz", [Block, Real]),
    );
    let map = reversed
        .opcodes_in(&device())
        .expect("the same instructions");
    let opcodes = [0, 1, 2, 3].map(|index| map.get(Opcode::new(index)));
    assert_eq!(
        opcodes,
        [
            Some(Opcode::new(2)),
            Some(Opcode::new(1)),
            Some(Opcode::new(0)),
            None
        ]
    );
}

/// A map is the identity when the set's instructions are the first of the
/// other, in the same order (the empty set's included), and not when they
/// are in another order, or later in the other.
#[test]
fn a_map_is_the_identity_when_the_instructions_come_first_in_the_same_order() {
    let is_identity = |sent: InstructionSet| {
        declared(sent)
            .opcodes_in(&device())
            .expect("the device has each instruction")
            .is_identity()
    };
    let first = InstructionSet::new("first")
        .with("rz", [Block, Real])
        .with("cx", [Block, Block]);
    let other_order = InstructionSet::new("other")
        .with("cx", [Block, Block])
        .with("rz", [Block, Real]);
    let later = InstructionSet::new("later")
        .with("cx", [Block, Block])
        .with("measure", [Block, Tag]);
    assert!(is_identity(first));
    assert!(is_identity(InstructionSet::new("empty")));
    assert!(!is_identity(other_order));
    assert!(!is_identity(later));
}

/// Fitting is matching: a set whose instructions do the same as another's
/// under other mnemonics fits it.
#[test]
fn a_set_doing_the_same_under_other_mnemonics_fits() {
    assert_eq!(stim().fits(&toy()), Ok(()));
}

/// An instruction replaced without an action no longer declares one, so it
/// is refused.
#[test]
fn an_instruction_replaced_without_an_action_declares_none() {
    let sent = InstructionSet::new("sent")
        .with_action("x", [Block], x())
        .with("x", [Block]);
    assert_eq!(
        sent.opcodes_in(&toy()),
        Err(vec![InstructionMismatch::UndeclaredAction {
            mnemonic: "x".into()
        }])
    );
}

/// QEP's set, as its driver and its executor both describe it: by
/// mnemonic, declaring no action.
fn qep() -> InstructionSet {
    InstructionSet::new("qep")
        .with("x", [Block])
        .with("cx", [Block, Block])
        .with("measure", [Block, Tag])
}

/// A set's name is its identity: a set that declares no action is matched
/// by mnemonic in a set of its name, each instruction at its own opcode,
/// so nothing is renumbered (QEP's driver and executor share one set).
#[test]
fn a_set_without_actions_is_matched_by_mnemonic_in_a_set_of_its_name() {
    let map = qep().opcodes_in(&qep()).expect("one set at both ends");
    assert!(map.is_identity());
}

/// A part of a set, declaring no action, is matched by mnemonic in a set
/// of its name, in any order: a driver may send some of its executor's
/// instructions.
#[test]
fn a_part_of_a_set_without_actions_is_matched_by_mnemonic_in_a_set_of_its_name() {
    let part = InstructionSet::new("qep")
        .with("measure", [Block, Tag])
        .with("x", [Block]);
    let qep = qep();
    let found = ["measure", "x"].map(|mnemonic| matched(&part, &qep, mnemonic));
    assert_eq!(found, ["measure", "x"]);
}

/// An instruction that takes other operands in a set of its name is
/// refused: a mnemonic matches only with the same operands, which are read
/// by position.
#[test]
fn an_instruction_taking_other_operands_in_a_set_of_its_name_is_refused() {
    let sent = InstructionSet::new("qep").with("x", [Block, Block]);
    assert_eq!(
        sent.opcodes_in(&qep()),
        Err(vec![InstructionMismatch::DifferentOperands {
            mnemonic: "x".into()
        }])
    );
}

/// In a set of its name, an instruction is matched by mnemonic only: one
/// the set lacks is missing, though another instruction there does the
/// same, since one name means one meaning per mnemonic.
#[test]
fn an_instruction_a_set_of_its_name_lacks_is_missing_though_another_does_the_same() {
    let accepted = InstructionSet::new("qep").with_action("flip", [Block], x());
    let sent = InstructionSet::new("qep").with_action("X", [Block], x());
    assert_eq!(
        sent.opcodes_in(&accepted),
        Err(vec![InstructionMismatch::Missing {
            mnemonic: "X".into()
        }])
    );
}

/// In a set of its name, an instruction taking other operands there is
/// refused, though another instruction does the same with its operands:
/// the search by action is for sets of other names only.
#[test]
fn an_instruction_taking_other_operands_in_a_set_of_its_name_is_refused_though_another_does_the_same()
 {
    let accepted = InstructionSet::new("qep")
        .with("X", [Block, Block])
        .with_action("flip", [Block], x());
    let sent = InstructionSet::new("qep").with_action("X", [Block], x());
    assert_eq!(
        sent.opcodes_in(&accepted),
        Err(vec![InstructionMismatch::DifferentOperands {
            mnemonic: "X".into()
        }])
    );
}

/// Between sets of other names, a mnemonic alone is still not a match: the
/// same instructions under another set's name, declaring no action, are
/// each refused.
#[test]
fn sets_of_other_names_still_need_actions() {
    let vendor = InstructionSet::new("vendor")
        .with("x", [Block])
        .with("cx", [Block, Block])
        .with("measure", [Block, Tag]);
    let undeclared = |mnemonic: &str| InstructionMismatch::UndeclaredAction {
        mnemonic: mnemonic.into(),
    };
    assert_eq!(
        vendor.opcodes_in(&qep()),
        Err(vec![
            undeclared("x"),
            undeclared("cx"),
            undeclared("measure")
        ])
    );
}

/// An instruction that a set of its name declares to do something else is
/// refused: two unrelated sets given one name are caught wherever both
/// declare what an instruction does.
#[test]
fn an_instruction_a_set_of_its_name_declares_doing_something_else_is_refused() {
    let z = Action::new([1], [1]).pauli(Pauli::z(0));
    let accepted = InstructionSet::new("qep").with_action("x", [Block], x());
    let sent = InstructionSet::new("qep").with_action("x", [Block], z);
    assert_eq!(
        sent.opcodes_in(&accepted),
        Err(vec![InstructionMismatch::DifferentAction {
            mnemonic: "x".into()
        }])
    );
}

/// An instruction that both sets declare under one mnemonic, doing
/// different things, is refused whatever the sets' names, though another
/// instruction does the same: one mnemonic meaning two things at a
/// boundary is taken for a collision, not matched past.
#[test]
fn a_mnemonic_both_sets_declare_doing_different_things_is_refused_whatever_their_names() {
    let z = || Action::new([1], [1]).pauli(Pauli::z(0));
    let accepted = InstructionSet::new("device")
        .with_action("x", [Block], x())
        .with_action("z", [Block], z());
    let sent = InstructionSet::new("sent").with_action("x", [Block], z());
    assert_eq!(
        sent.opcodes_in(&accepted),
        Err(vec![InstructionMismatch::DifferentAction {
            mnemonic: "x".into()
        }])
    );
}

/// An instruction that both sets declare under one mnemonic, doing
/// different things, is refused whatever its operands there, though
/// another instruction does the same with its operands: the guard compares
/// what the mnemonic does before the operands it takes.
#[test]
fn a_mnemonic_both_sets_declare_doing_different_things_is_refused_whatever_its_operands() {
    let z = || Action::new([1], [1]).pauli(Pauli::z(0));
    let accepted = InstructionSet::new("toy")
        .with_action("x", [Block, Block], x())
        .with_action("z", [Block], z());
    let sent = InstructionSet::new("stim").with_action("x", [Block], z());
    assert_eq!(
        sent.opcodes_in(&accepted),
        Err(vec![InstructionMismatch::DifferentAction {
            mnemonic: "x".into()
        }])
    );
}

/// A table holds each instruction's entry at its opcode, as the handler
/// gave it for the instruction's mnemonic.
#[test]
fn a_table_holds_each_instructions_entry_at_its_opcode() {
    let set = device();
    let table = set
        .table(|mnemonic| Some(mnemonic.len()))
        .expect("every instruction has an entry");
    let measure = set.opcode("measure").expect("the device has `measure`");
    let rz = set.opcode("rz").expect("the device has `rz`");
    assert_eq!((table[measure], table[rz]), (7, 2));
}

/// A table is refused with every mnemonic the handler has no entry for,
/// in the order of their opcodes, so that all are seen at once.
#[test]
fn a_table_reports_every_instruction_without_an_entry() {
    let table = device().table(|mnemonic| (mnemonic == "cx").then_some(()));
    assert_eq!(table.err(), Some(vec!["rz".into(), "measure".into()]));
}

/// A set whose `x` takes two blocks by its operands, but whose action takes
/// and gives one: the description disagrees with itself, so what `x` does
/// is unknown, and it is matched by nothing, here in a set of another name
/// describing it alike, where matching is by action. Pins the known gap
/// flagged at `InstructionSet::with_action`. In a set of its own name it
/// is matched by mnemonic, unchecked, so the fix belongs at declaration;
/// when it lands there, this test changes shape to check the declaration.
#[test]
#[ignore = "known gap: nothing checks an action's blocks against the Block operands"]
fn an_instruction_whose_action_and_operands_disagree_on_its_blocks_is_not_matched() {
    let x = || Action::new([1], [1]).pauli(Pauli::x(0));
    let sent = InstructionSet::new("disagreeing").with_action("x", [Block, Block], x());
    let accepted = InstructionSet::new("alike").with_action("x", [Block, Block], x());
    assert!(
        sent.opcodes_in(&accepted).is_err(),
        "the action takes one block, the operands address two"
    );
}
