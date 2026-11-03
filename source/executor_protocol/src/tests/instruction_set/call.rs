// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::device;
use crate::{InstructionCall, InstructionSet, Opcode, Operand, OperandKind, Operands};

use OperandKind::{Block, Integer, Real, Tag};

/// A call keeps all its operands, in order, however many it takes: none,
/// as many as are held in the call ([`Operands::INLINE`]), or more (a
/// logical instruction on many blocks).
#[test]
fn a_call_keeps_any_number_of_operands() {
    let inline = u32::try_from(Operands::INLINE).expect("`INLINE` is a few operands");
    for count in [0, inline, inline + 1, 7] {
        let blocks: Vec<Operand> = (0..count).map(Operand::Block).collect();
        let call = InstructionCall::new(Opcode::new(0), blocks.iter().copied());
        assert_eq!(*call.operands, blocks[..], "{count} operands");
    }
}

/// A call of up to [`Operands::INLINE`] operands is built and copied, into
/// a new call or one already held, without allocating memory.
#[test]
fn a_call_of_up_to_inline_operands_allocates_nothing() {
    let inline = u32::try_from(Operands::INLINE).expect("`INLINE` is a few operands");
    for count in 0..=inline {
        let mut kept = InstructionCall::new(Opcode::new(1), [Operand::Block(9)]);
        let made = crate::tests::allocations(|| {
            let call = InstructionCall::new(Opcode::new(0), (0..count).map(Operand::Block));
            kept.clone_from(&call);
            std::hint::black_box((call.clone(), &kept));
        });
        assert_eq!(made, 0, "{count} operands");
    }
}

/// Many operands copied into a call already holding as many on the heap, as
/// an adapter copies calls into a buffer it keeps across batches, reuse
/// that call's memory.
#[test]
fn operands_copied_into_as_many_on_the_heap_reuse_their_memory() {
    let many = || (0..7).map(Operand::Block).collect::<Operands>();
    let mut kept = many();
    let operands = many();
    let made = crate::tests::allocations(|| kept.clone_from(&operands));
    assert_eq!(made, 0);
}

/// Operands copied into others, as an adapter copies calls into a buffer
/// it keeps, equal their source, whichever of the two holds more than fit
/// in the call.
#[test]
fn operands_copied_into_others_equal_them() {
    let operands = |count: u32| -> Operands { (0..count).map(Operand::Tag).collect() };
    let counts = [1, 3, 4, 6];
    for from in counts {
        for into in counts {
            let mut kept = operands(into);
            kept.clone_from(&operands(from));
            assert_eq!(kept, operands(from), "{from} operands into {into}");
        }
    }
}

/// A call is written with the mnemonic of its opcode in the set, and each
/// operand's kind and value (one of each kind here), for a log or a
/// debugger.
#[test]
fn a_call_is_displayed_with_its_mnemonic() {
    let set = InstructionSet::new("pulses").with("pulse", [Block, Real, Integer, Tag]);
    let operands = [
        Operand::Block(2),
        Operand::Real(0.5),
        Operand::Integer(3),
        Operand::Tag(5),
    ];
    let pulse = set
        .call("pulse", operands)
        .expect("`pulse` takes one operand of each kind");
    assert_eq!(
        set.display(&pulse).to_string(),
        "pulse(block 2, real 0.5, integer 3, tag 5)"
    );
}

/// A call at an opcode the set lacks has no mnemonic there, so it is
/// written with its opcode.
#[test]
fn a_call_at_an_unknown_opcode_is_displayed_with_its_opcode() {
    let unknown = InstructionCall::new(Opcode::new(7), [Operand::Tag(1)]);
    assert_eq!(device().display(&unknown).to_string(), "opcode 7(tag 1)");
}
