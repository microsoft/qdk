// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An executor modelled on the per-shot MPS executor, built with
//! [`executor_fn`]: only its logic is written here, a state
//! ([`BondLimitedState`]), the logic that runs a batch on it
//! ([`run_bond_limited`]), and how to read its log probability. Each
//! shot's state is made at its first call, on a device that can be busy;
//! it draws from the last executor's seed in the shot, keeps the log
//! probability of its coins, and fails a batch that outgrows its capacity.
//! The toy's bits stand for the MPS.

use executor_protocol::{
    CallInput, Capabilities, ExecutorFn, Outcomes, Rejected, ShotContext, ShotId, executor_fn,
};

use crate::toy::{Op, QUBITS, ToyState, check};

/// The largest bond a shot's state may reach: each `Cx` adds one.
pub const MAX_BOND: u32 = 2;

/// The most shots the device holds at once, a state each.
pub const MAX_SHOTS: u64 = 8;

/// One shot's state: the toy's bits, the bond its `Cx`s reached, and the
/// log probability of its coins.
pub struct BondLimitedState {
    bits: ToyState,
    bond: u32,
    log_p: f64,
}

impl BondLimitedState {
    /// A state whose coins come from `rng_seed`, with no bond yet.
    fn new(rng_seed: u64) -> Self {
        Self {
            bits: ToyState::new(rng_seed),
            bond: 0,
            log_p: 0.0,
        }
    }
}

/// The logic, run on each shot's state: it runs the toy's instructions,
/// then fails the batch when the bond outgrew [`MAX_BOND`], as the MPS
/// executor checks its state's capacity at a batch's end; the outcomes
/// read before stay in the reply.
///
/// # Errors
///
/// The bond and the capacity, when the bond outgrew it.
pub fn run_bond_limited(
    state: &mut BondLimitedState,
    input: &CallInput<'_, Op>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    state.bits.run(input, outcomes)?;
    for op in input.instructions() {
        match op {
            Op::Coin(_) => state.log_p += 0.5_f64.ln(),
            Op::Cx(..) => state.bond += 1,
            _ => {}
        }
    }
    if state.bond > MAX_BOND {
        return Err(format!(
            "bond {} outgrew the capacity {MAX_BOND}",
            state.bond
        ));
    }
    Ok(())
}

/// The executor, whose device is busy when shot `busy_at` first starts,
/// and free afterwards: [`QUBITS`] qubits, at most [`MAX_SHOTS`] shots at
/// once, resumable shots and replayable shots, the toy's [`check`], and the
/// log probability of each state's coins. Each shot's state draws from the
/// last executor's seed in the shot ([`ShotContext::seed`]), so a run gives it
/// the run's seed and a state per shot:
/// `bond_limited_executor(None).seed(seed).shots(OnePerShot)`. Its logic and
/// log probability's reader are `Clone`, so that each shot's state may also run
/// on a thread of its own: `.run_on(OwnThread).shots(OnePerShot)`, which gives
/// what the states give on the caller's thread
/// (`a_run_gives_the_same_outcomes_and_report_wherever_the_shots_states_run`).
#[expect(
    clippy::type_complexity,
    reason = "the closures' types are named only as `impl FnMut`, so that the \
              body is what an implementer writes, with no casts"
)]
pub fn bond_limited_executor(
    busy_at: Option<ShotId>,
) -> ExecutorFn<
    Op,
    BondLimitedState,
    impl FnMut(ShotContext) -> Result<BondLimitedState, Rejected>,
    impl FnMut(&mut BondLimitedState, &CallInput<'_, Op>, &mut Outcomes<'_>) -> Result<(), String>
    + Clone,
    impl FnMut(&CallInput<'_, Op>) -> Result<(), Rejected>,
    impl FnMut(&BondLimitedState) -> f64 + Clone,
> {
    let mut busy = busy_at;
    let promise = Capabilities::default()
        .with_qubits(QUBITS)
        .with_resumable_shots(true)
        .with_replayable(true)
        .with_max_shots(MAX_SHOTS);
    executor_fn(
        promise,
        move |context: ShotContext| {
            if busy.take_if(|&mut j| j == context.shot()).is_some() {
                return Err(Rejected::Unavailable("the device is busy".into()));
            }
            Ok(BondLimitedState::new(context.seed()))
        },
        run_bond_limited,
    )
    .check(check)
    .log_probability(|state| state.log_p)
}
