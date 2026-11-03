// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The check of the rule of a described instruction set: an executor
//! refuses a call outside it, at the instruction that leaves it.

use super::readouts::outcomes_of_call;
use crate::{
    Executor, InstructionBatch, InstructionBatchRef, InstructionCall, InstructionSet, Opcode,
    Operand, OperandKind, Rejected, ShotSet,
};

/// Calls `executor`, on `shots`, with batches of `accepted` and a call
/// outside `set`. First `[accepted, accepted, accepted]`, which must be
/// accepted and its reply end well, so that a refusal of a later batch of
/// three calls or fewer can only be the call outside's. Then, for each call
/// outside, `[accepted, outside]`, `[accepted, outside, accepted]` and
/// `[outside, accepted]`: each must be rejected with
/// [`Rejected::Instruction`] at the index of the call outside (1, 1 and
/// 0), whether it is last, in the middle or first, so that a caller learns
/// which call to change. Last, `[outside, accepted, outside]`, for every
/// pair of calls outside, in each order and one call twice included, must
/// be rejected at the first of the two, 0, as [`Rejected::Instruction`]
/// promises: an executor that looks for each reason in its own pass over
/// the batch, and so refuses at a later call when the earlier one is
/// outside for another reason, would show.
///
/// The calls outside are made from `accepted`, one for each way a call can
/// differ from every call of the set, by the definition of
/// [`InstructionSet::check`]: an opcode the set lacks, with `accepted`'s
/// operands; and, at `accepted`'s opcode, operands whose kinds are not
/// those it takes, as a sequence: one operand too many; one too few (the
/// last left out), when it takes one; and, at each of its operands in
/// turn, an operand of each other kind ([`OperandKind`]), the others
/// unchanged. The opcode lacked is the count of `set`'s mnemonics, since an
/// opcode is its instruction's position in the set, so opcodes have no
/// gaps. The rule also asks that nothing of a refused batch runs, which
/// this check cannot see. `set` is the set the executor describes (or its
/// maker does, in [`Capabilities::instruction_set`](crate::Capabilities)),
/// `accepted` a call of it that the executor runs, three times in one
/// batch (one that makes no readout, since a batch repeats no tag), and
/// `shots` shots it runs. It waits for the first batch's reply.
///
/// # Panics
///
/// When a batch with a call outside is accepted, or rejected for another
/// reason or at another index (`[outside, accepted, outside]`, for any
/// pair, at another than 0); when `accepted` is not a call of `set`, or
/// `[accepted, accepted, accepted]` is refused, or its reply fails.
pub fn refuses_calls_outside_its_set<E: Executor<InstructionBatch<InstructionCall>>>(
    executor: &mut E,
    shots: &ShotSet,
    set: &InstructionSet,
    accepted: &InstructionCall,
) {
    assert!(
        set.check(accepted).is_ok(),
        "`accepted` must be a call of the set"
    );
    if let Err(rejected) = outcomes_of_call(
        executor,
        shots,
        &[accepted.clone(), accepted.clone(), accepted.clone()],
    ) {
        panic!(
            "`accepted` must be a call that the executor runs, three times in one batch, so \
             that a refusal of a batch that holds it can only be the call outside's, but \
             `[accepted, accepted, accepted]` was rejected with `{rejected:?}`"
        );
    }
    let outside_calls = calls_outside(set, accepted);
    for (outside, how) in &outside_calls {
        for (instructions, at, batch) in [
            (
                vec![accepted.clone(), outside.clone()],
                1,
                "[accepted, outside]",
            ),
            (
                vec![accepted.clone(), outside.clone(), accepted.clone()],
                1,
                "[accepted, outside, accepted]",
            ),
            (
                vec![outside.clone(), accepted.clone()],
                0,
                "[outside, accepted]",
            ),
        ] {
            let answer = executor.call(InstructionBatchRef {
                shots,
                instructions: &instructions,
            });
            expect_refused_at(
                answer,
                at,
                "a call outside the set",
                &format!("`{batch}`, the call outside {how}"),
            );
        }
    }
    for (first, first_how) in &outside_calls {
        for (second, second_how) in &outside_calls {
            let answer = executor.call(InstructionBatchRef {
                shots,
                instructions: &[first.clone(), accepted.clone(), second.clone()],
            });
            expect_refused_at(
                answer,
                0,
                "the first of two calls outside the set",
                &format!(
                    "`[outside, accepted, outside]`, the first call outside {first_how}, the \
                     second {second_how}"
                ),
            );
        }
    }
}

/// Every kind of operand, for [`calls_outside`] to try each. A kind added
/// to [`OperandKind`] fails to compile in [`an_operand_of`]: add it here
/// too.
const OPERAND_KINDS: [OperandKind; 4] = [
    OperandKind::Block,
    OperandKind::Real,
    OperandKind::Integer,
    OperandKind::Tag,
];

/// An operand of `kind`.
fn an_operand_of(kind: OperandKind) -> Operand {
    match kind {
        OperandKind::Block => Operand::Block(0),
        OperandKind::Real => Operand::Real(0.0),
        OperandKind::Integer => Operand::Integer(0),
        OperandKind::Tag => Operand::Tag(0),
    }
}

/// The calls outside `set` that [`refuses_calls_outside_its_set`] sends,
/// made from `accepted`, a call of `set`, each with how it is outside: one
/// for each way [`InstructionSet::check`] refuses a call.
fn calls_outside(
    set: &InstructionSet,
    accepted: &InstructionCall,
) -> Vec<(InstructionCall, String)> {
    let beyond = Opcode::new(u32::try_from(set.mnemonics().count()).unwrap_or(u32::MAX));
    let operands = || accepted.operands.iter().copied();
    let at_its_opcode = |operands: Vec<Operand>| InstructionCall::new(accepted.opcode, operands);
    let mut outside = vec![
        (
            InstructionCall::new(beyond, operands()),
            "at an opcode the set lacks".to_owned(),
        ),
        (
            at_its_opcode(operands().chain([Operand::Integer(0)]).collect()),
            "with an operand too many".to_owned(),
        ),
    ];
    if let Some(fewer) = accepted.operands.len().checked_sub(1) {
        outside.push((
            at_its_opcode(operands().take(fewer).collect()),
            "with an operand too few".to_owned(),
        ));
    }
    for (at, operand) in accepted.operands.iter().enumerate() {
        for kind in OPERAND_KINDS
            .into_iter()
            .filter(|&kind| kind != operand.kind())
        {
            let mut operands: Vec<Operand> = operands().collect();
            operands[at] = an_operand_of(kind);
            outside.push((
                at_its_opcode(operands),
                format!(
                    "with its operand {at} of kind `{kind:?}`, not `{:?}`",
                    operand.kind()
                ),
            ));
        }
    }
    outside
}

/// The rule of an executor that refuses a batch for a fault of one of its
/// instructions: `answer`, the answer to `batch`, must be a rejection with
/// [`Rejected::Instruction`] at `at`, the index of `what`, the instruction
/// at fault, so that a caller learns which instruction to change.
///
/// # Panics
///
/// When `answer` is an accepted call, or a rejection for another reason or
/// at another index; the panic message starts with `what` and ends with
/// `batch`.
pub(super) fn expect_refused_at<R>(
    answer: Result<R, Rejected>,
    at: usize,
    what: &str,
    batch: &str,
) {
    match answer {
        Err(Rejected::Instruction { at: refused, .. }) if refused == at => {}
        Err(Rejected::Instruction { at: refused, .. }) => {
            panic!("{what} must be rejected at its index, {at}, not {refused} ({batch})")
        }
        Err(other) => {
            panic!("{what} must be rejected with `Instruction`, not `{other:?}` ({batch})")
        }
        Ok(_) => {
            panic!("{what} must be rejected with `Instruction`, but it was accepted ({batch})")
        }
    }
}
