//! Coins ([`coin_device`]) as the protocol's executor, called directly,
//! with no chain above them; here over `stim+rz`, as any bottom set runs on
//! them.

mod common;

use std::sync::Arc;

use executor_protocol::{
    Executor, InstructionBatchRef, InstructionCall, Opcode, Operand, Rejected, ShotSet,
};
use qodec_demo::coin::coin_device;

use common::{SEED, stim_rz};

/// A caller sends the coins of `stim+rz` a batch that reads under tag 0,
/// then calls an opcode beyond the set under tag 1. The coins read a coin
/// per tag whatever the instruction, yet the batch is refused at its call,
/// naming its second call.
#[test]
fn a_call_outside_the_set_is_refused() {
    let set = stim_rz().set;
    let read = set
        .call("M", [Operand::Block(0), Operand::Tag(0)])
        .expect("precondition: stim+rz reads a block under a tag");
    let beyond_the_set = u32::try_from(set.mnemonics().count()).expect("a few instructions");
    let beyond = InstructionCall::new(
        Opcode::new(beyond_the_set),
        [Operand::Block(0), Operand::Tag(1)],
    );
    let mut coins = coin_device(Arc::clone(&set), SEED);
    let shots = ShotSet::new(vec![0]).expect("one shot");

    let called = coins.call(InstructionBatchRef {
        shots: &shots,
        instructions: &[read, beyond],
    });
    assert!(
        matches!(called, Err(Rejected::Instruction { at: 1, .. })),
        "{:?}",
        called.err()
    );
}
