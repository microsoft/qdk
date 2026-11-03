//! A call counter: an adapter with one state, which counts the calls that
//! reach it and sends each down as it is. With one state it sees each call
//! whole, on all its shots, so it counts the calls the executor below
//! receives: how a chain groups shots into device calls. A counter with a
//! state per shot, as [`profiler`](crate::profiler::profiler) is, sees
//! each shot's part of a call alone, so it cannot see that grouping.

use executor_protocol::{
    AdapterError, AdapterFn, AdapterLogic, Below, CallInput, Fold, InstructionCall, MakeState,
    NoCheck, Outcomes, adapter_fn,
};

/// The calls counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Calls(pub u64);

impl Fold for Calls {
    fn fold(&mut self, other: Self) {
        self.0 += other.0;
    }
}

/// The counter's logic: counts the call, then sends it down as it is.
fn count(
    calls: &mut Calls,
    input: &CallInput<'_, InstructionCall>,
    below: &mut Below<'_, InstructionCall>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    calls.0 += 1;
    below.send(input.instructions())
}

/// The call counter, one state, reporting the [`Calls`] beside the report
/// below. It states no set: it passes on the one below.
#[expect(
    clippy::type_complexity,
    reason = "the facility's type, as `>>` composes it"
)]
pub fn call_counter() -> AdapterFn<
    InstructionCall,
    InstructionCall,
    Calls,
    impl MakeState<Calls>,
    impl AdapterLogic<InstructionCall, InstructionCall, Calls>,
    NoCheck<InstructionCall>,
    impl FnMut(&Calls) -> Calls,
> {
    adapter_fn(|_| Ok(Calls::default()), count).state_report(|calls: &Calls| *calls)
}
