//! A recorder: an adapter that keeps each shot's calls, in the order it
//! sends them below, and sends each down as it is. Below a Qodec layer, it
//! shows the physical batches a shot sent, operand for operand, so a chain
//! compares with the Qodec runtime's own stack on what it sent and in what
//! order, not only on how much ([`Profile`](crate::profiler::Profile)).
//!
//! It keeps each call it sends down, in the shot's state. Once the shot
//! has a state, a refusal below does not undo the record: with a state per
//! shot, the logic's send returns `Ok` and the call goes below after it
//! ran. So its record holds a later call the executor below refuses
//! (`once_a_shot_has_a_state_the_recorder_keeps_a_call_refused_below`),
//! but not a shot's first call refused below, whose state the adapter
//! facility does not keep
//! (`the_recorder_keeps_no_shots_first_call_refused_below`). In a run that
//! succeeds, it equals what ran below.

use std::collections::BTreeMap;

use executor_protocol::{
    AdapterError, AdapterLogic, Below, CallInput, Fold, InstructionCall, MakeState, NoCheck,
    OnePerShot, OnePerShotAdapter, Outcomes, ShotContext, ShotId, adapter_fn,
};

/// Each shot's batches, in the order they were sent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sent(pub BTreeMap<ShotId, Vec<Vec<InstructionCall>>>);

impl Fold for Sent {
    /// Each shot's batches are its state's alone, so the shots of two
    /// reports differ.
    fn fold(&mut self, other: Self) {
        self.0.extend(other.0);
    }
}

/// One shot's record.
pub struct Record {
    shot: ShotId,
    batches: Vec<Vec<InstructionCall>>,
}

/// The recorder's logic: keeps one shot's call, then sends it down as it
/// is; the record keeps it if the executor below refuses the call, unless
/// it is the shot's first. What the call reads below passes up as it
/// comes.
fn record(
    record: &mut Record,
    input: &CallInput<'_, InstructionCall>,
    below: &mut Below<'_, InstructionCall>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    record.batches.push(input.instructions().to_vec());
    below.send(input.instructions())
}

/// The recorder, a state per shot, reporting each shot's batches beside
/// the report below. It states no set: it passes on the one below.
#[expect(
    clippy::type_complexity,
    reason = "the facility's type, as `>>` composes it"
)]
pub fn recorder() -> OnePerShotAdapter<
    InstructionCall,
    InstructionCall,
    Record,
    impl MakeState<Record>,
    impl AdapterLogic<InstructionCall, InstructionCall, Record>,
    NoCheck<InstructionCall>,
    impl FnMut(&Record) -> Sent,
> {
    let make = |shot: ShotContext| {
        Ok(Record {
            shot: shot.shot(),
            batches: Vec::new(),
        })
    };
    adapter_fn(make, record)
        .state_report(|record: &Record| {
            Sent(BTreeMap::from([(record.shot, record.batches.clone())]))
        })
        .shots(OnePerShot)
}
