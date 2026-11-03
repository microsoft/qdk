//! A profiler: an adapter that counts what each shot sends below, batches
//! and instructions by opcode, and sends each call down as it is. Below a
//! Qodec layer, it counts the physical instructions a program costs; above
//! it, the logical ones. Its counts are a [`Profile`], the same form the
//! bottoms of this crate report what they ran in, so a profile taken in a
//! chain compares with the Qodec runtime's own stack's.
//!
//! It counts each call it sends down, in the shot's state. Once the shot
//! has a state, a refusal below does not undo the count: with a state per
//! shot, the logic's send returns `Ok` and the call goes below after it
//! ran. So its counts hold a later call the executor below refuses
//! (`once_a_shot_has_a_state_the_profiler_counts_a_call_refused_below`),
//! but not a shot's first call refused below, whose state the adapter
//! facility does not keep
//! (`the_profiler_counts_no_shots_first_call_refused_below`). In a run
//! that succeeds, they equal what ran below.

use std::collections::BTreeMap;

use executor_protocol::{
    AdapterError, AdapterLogic, Below, CallInput, Fold, InstructionCall, MakeState, NoCheck,
    OnePerShot, OnePerShotAdapter, Opcode, Outcomes, adapter_fn,
};

/// Batches and instructions by opcode, summed over the shots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    /// The batches, one per shot each batch was counted on.
    pub batches: u64,
    /// The instructions, by opcode.
    pub counts: BTreeMap<Opcode, u64>,
}

impl Profile {
    /// Counts one batch of `opcodes`.
    pub fn add(&mut self, opcodes: impl IntoIterator<Item = Opcode>) {
        self.batches += 1;
        for opcode in opcodes {
            *self.counts.entry(opcode).or_default() += 1;
        }
    }

    /// The instructions, all opcodes together.
    #[must_use]
    pub fn instructions(&self) -> u64 {
        self.counts.values().sum()
    }
}

impl Fold for Profile {
    fn fold(&mut self, other: Self) {
        self.batches += other.batches;
        self.counts.fold(other.counts);
    }
}

/// The profiler's logic: counts one shot's call, then sends it down as it
/// is; the count stays if the executor below refuses the call, unless it
/// is the shot's first. What the call reads below passes up as it comes.
fn profile(
    profile: &mut Profile,
    input: &CallInput<'_, InstructionCall>,
    below: &mut Below<'_, InstructionCall>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    profile.add(input.instructions().iter().map(|call| call.opcode));
    below.send(input.instructions())
}

/// The profiler, a state per shot, reporting its [`Profile`] folded over
/// the shots, beside the report below. It states no set: it passes on the
/// one below, so the opcodes it counts are those of the executor below.
#[expect(
    clippy::type_complexity,
    reason = "the facility's type, as `>>` composes it"
)]
pub fn profiler() -> OnePerShotAdapter<
    InstructionCall,
    InstructionCall,
    Profile,
    impl MakeState<Profile>,
    impl AdapterLogic<InstructionCall, InstructionCall, Profile>,
    NoCheck<InstructionCall>,
    impl FnMut(&Profile) -> Profile,
> {
    adapter_fn(|_| Ok(Profile::default()), profile)
        .state_report(Profile::clone)
        .shots(OnePerShot)
}
