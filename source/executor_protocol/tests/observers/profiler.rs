// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A profiler: an adapter that counts the instructions each shot sends
//! below, and sends each call down as it is. It is written with
//! `adapter_fn`, a state per shot (`OnePerShot`), so it holds only its
//! counting: the facility keeps each shot's counts apart, folds them over
//! the shots when the chain closes (`state_report`), and gives them beside
//! the report below.

use std::collections::BTreeMap;

use executor_protocol::{
    AdapterError, AdapterLogic, Below, CallInput, InstructionCall, MakeState, NoCheck, OnePerShot,
    OnePerShotAdapter, Opcode, Outcomes, adapter_fn,
};

/// A profile: how many calls of each instruction the shots sent below, by
/// opcode. It folds as maps of integers do, adding count by count, so it
/// needs no fold of its own.
pub type Counts = BTreeMap<Opcode, u64>;

/// The profiler's logic: counts each instruction of one shot's call, then
/// sends the call down as it is. What the call reads below passes up as
/// it comes.
fn profile(
    counts: &mut Counts,
    input: &CallInput<'_, InstructionCall>,
    below: &mut Below<'_, InstructionCall>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    for call in input.instructions() {
        *counts.entry(call.opcode).or_default() += 1;
    }
    below.send(input.instructions())
}

/// The profiler: counts per shot what it sends below, and reports its
/// counts, folded over the shots.
/// The counts are exact for a run whose calls all succeed below
/// (`the_profilers_report_counts_each_instruction_over_the_shots`). For
/// the shots of a call refused or failed below, they are unspecified
/// until it is decided whether a profile counts what was sent or what
/// ran: today, whether such a call is counted depends on its place in the
/// shot, since the facility drops the states it made for such a call.
/// It states no instruction set: it passes on the one below, whatever it
/// is; over an executor that describes none, the chain is refused when it
/// is built (`NoInstructionSetDownstream`), since what it sends could not
/// be checked. Its type is the one its last option gives, since
/// `>>` takes an adapter by its type: its state and logic closures named
/// by their role, the check it was given none of (`NoCheck`), and its
/// report closure by the bound `state_report` puts on it, `FnMut`.
#[expect(
    clippy::type_complexity,
    reason = "the facility's type, as `>>` composes it; no role trait names a report"
)]
pub fn profiler() -> OnePerShotAdapter<
    InstructionCall,
    InstructionCall,
    Counts,
    impl MakeState<Counts>,
    impl AdapterLogic<InstructionCall, InstructionCall, Counts>,
    NoCheck<InstructionCall>,
    impl FnMut(&Counts) -> Counts,
> {
    adapter_fn(|_| Ok(Counts::default()), profile)
        .state_report(|counts: &Counts| counts.clone())
        .shots(OnePerShot)
}
