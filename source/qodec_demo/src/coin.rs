//! Coins: the bottom of the c422 and c4c6 chains, in two forms that read the
//! same bits from the same instructions: the protocol's executor
//! ([`coin_device`], or [`coin_device_on_threads`]) and the Qodec
//! runtime's bottom backend of one shot ([`CoinBackend`]). Each readout is
//! a coin drawn from the shot's seed and the readout's index in the shot,
//! whatever the instruction, so any bottom set runs on it, and the Qodec
//! runtime's outcomes are a function of the coins (its decoder draws
//! nothing). Both count what they ran as a [`Profile`], by opcode in the
//! bottom set's own numbering.
//!
//! [`biased_coin_device`] reads 1 with a chance `p` instead of 1/2, so that
//! a code's checks pass on most shots (at `p` 0, on all), as on a device
//! with little noise; the examples run on it.

use std::sync::Arc;

use executor_protocol::{
    CallInput, Capabilities, ExecutorFn, ExecutorLogic, InstructionCall, InstructionSet, MakeState,
    OnePerShot, OnePerShotExecutor, OnePerShotPlacedExecutor, Operand, Outcome, Outcomes,
    OwnThread, Rejected, ShotContext, executor_fn,
};
use qodec_runtime::{Backend, BasicReadout, BatchResult, OwnedParam, Readout, Status};

use crate::boundary::{Boundary, Ins, opcode};
use crate::profiler::Profile;

/// The `n`-th coin of a shot whose seed is `seed` (one splitmix64 step).
#[must_use]
pub fn coin(seed: u64, n: u64) -> bool {
    let mut z = seed ^ n;
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) & 1 == 1
}

/// One shot's coins: drawn from its seed, in the order it reads, and what
/// it ran.
#[derive(Clone, Debug)]
pub struct Coins {
    seed: u64,
    drawn: u64,
    ran: Profile,
}

impl Coins {
    /// The coins of the shot whose seed is `seed`, none drawn yet.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            drawn: 0,
            ran: Profile::default(),
        }
    }

    /// What the shot ran so far.
    #[must_use]
    pub fn ran(&self) -> &Profile {
        &self.ran
    }

    fn draw(&mut self) -> bool {
        let bit = coin(self.seed, self.drawn);
        self.drawn += 1;
        bit
    }
}

/// The logic of [`coin_device`]: a coin per tag, in call order.
fn toss(
    coins: &mut Coins,
    input: &CallInput<'_, InstructionCall>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    coins
        .ran
        .add(input.instructions().iter().map(|call| call.opcode));
    for call in input.instructions() {
        for operand in call.operands.iter() {
            if let Operand::Tag(tag) = *operand {
                let value = coins.draw().into();
                outcomes.push(Outcome::new(tag, value));
            }
        }
    }
    Ok(())
}

/// The protocol's executor of `set`, a state per shot: shot `j` draws from
/// its seed in a run of seed `seed` (`shot_seed(seed, j)`), so it reads
/// alike whatever the other shots. It refuses a call outside `set`, and
/// reports what it ran, summed over the shots. It answers within its call.
#[expect(
    clippy::type_complexity,
    reason = "the facility's type, as `>>` composes it"
)]
pub fn coin_device(
    set: Arc<InstructionSet>,
    seed: u64,
) -> OnePerShotExecutor<
    InstructionCall,
    Coins,
    impl MakeState<Coins>,
    impl ExecutorLogic<InstructionCall, Coins>,
    impl FnMut(&CallInput<'_, InstructionCall>) -> Result<(), Rejected>,
    fn(&Coins) -> f64,
    impl FnMut(&Coins) -> Profile,
> {
    coins(set, seed).shots(OnePerShot)
}

/// [`coin_device`], each shot on a thread of its own: it answers later,
/// when the shot's thread has run the call.
#[expect(
    clippy::type_complexity,
    reason = "the facility's type, as `>>` composes it"
)]
pub fn coin_device_on_threads(
    set: Arc<InstructionSet>,
    seed: u64,
) -> OnePerShotPlacedExecutor<
    InstructionCall,
    Coins,
    impl MakeState<Coins> + Clone + Send,
    impl ExecutorLogic<InstructionCall, Coins> + Clone + Send,
    OwnThread,
    impl FnMut(&CallInput<'_, InstructionCall>) -> Result<(), Rejected> + Clone + Send,
    fn(&Coins) -> f64,
    impl FnMut(&Coins) -> Profile + Clone + Send,
> {
    coins(set, seed).run_on(OwnThread).shots(OnePerShot)
}

/// The executor of both forms, before its shots are given a state each.
#[expect(
    clippy::type_complexity,
    reason = "the facility's type, as `>>` composes it"
)]
fn coins(
    set: Arc<InstructionSet>,
    seed: u64,
) -> ExecutorFn<
    InstructionCall,
    Coins,
    impl MakeState<Coins> + Clone + Send,
    impl ExecutorLogic<InstructionCall, Coins> + Clone + Send,
    impl FnMut(&CallInput<'_, InstructionCall>) -> Result<(), Rejected> + Clone + Send,
    fn(&Coins) -> f64,
    impl FnMut(&Coins) -> Profile + Clone + Send,
> {
    let promise = Capabilities::default()
        .with_resumable_shots(true)
        .with_replayable(true)
        .with_instruction_set(Some(Arc::clone(&set)));
    let check = move |input: &CallInput<'_, InstructionCall>| set.check_batch(input.instructions());
    executor_fn(
        promise,
        |shot: ShotContext| Ok(Coins::new(shot.seed())),
        toss,
    )
    .check(check)
    .state_report(|coins: &Coins| coins.ran.clone())
    .seed(seed)
}

/// The same coins, as the Qodec runtime's bottom backend of one shot.
pub struct CoinBackend {
    coins: Coins,
    bottom: Arc<Boundary>,
    /// The batches sent, when kept ([`CoinBackend::keeping_sent`]).
    sent: Option<Vec<Vec<InstructionCall>>>,
}

impl CoinBackend {
    /// The coins of the shot whose seed is `seed` (`shot_seed(seed, j)`
    /// for [`coin_device`]'s shot `j`), each instruction's readouts read
    /// from `bottom`.
    #[must_use]
    pub fn new(seed: u64, bottom: Arc<Boundary>) -> Self {
        Self {
            coins: Coins::new(seed),
            bottom,
            sent: None,
        }
    }

    /// These coins, keeping the batches sent ([`CoinBackend::sent`]), at a
    /// cost: each batch is converted to the protocol's calls as it comes.
    #[must_use]
    pub fn keeping_sent(mut self) -> Self {
        self.sent = Some(Vec::new());
        self
    }

    /// What the shot ran so far.
    #[must_use]
    pub fn ran(&self) -> &Profile {
        &self.coins.ran
    }

    /// The batches the shot ran so far, in order, as the protocol's calls
    /// of the bottom set, each batch's tags numbered from 0, as a Qodec
    /// layer of a chain sends them (`Lower`).
    ///
    /// # Panics
    ///
    /// When the coins do not keep them ([`CoinBackend::keeping_sent`]).
    #[must_use]
    pub fn sent(&self) -> &[Vec<InstructionCall>] {
        self.sent
            .as_deref()
            .expect("coins keeping the batches sent")
    }
}

impl Backend<usize, OwnedParam, BasicReadout> for CoinBackend {
    type Error = std::convert::Infallible;

    fn execute_batch(
        &mut self,
        instructions: &[Ins],
    ) -> Result<BatchResult<BasicReadout>, Self::Error> {
        let opcodes = instructions.iter().map(|ins| opcode(ins.instruction_id));
        self.coins.ran.add(opcodes);
        if let Some(sent) = &mut self.sent {
            let mut next = 0;
            let calls = instructions.iter().map(|ins| {
                let opcode = opcode(ins.instruction_id);
                self.bottom.call(ins, opcode, &mut next)
            });
            sent.push(calls.collect());
        }
        let mut results = smallvec::SmallVec::new();
        for ins in instructions {
            let layout = &self.bottom.layouts[ins.instruction_id];
            if layout.readouts == 0 {
                continue;
            }
            let bits: Vec<_> = (0..layout.readouts)
                .map(|_| Some(self.coins.draw()))
                .collect();
            let (outs, flags) = bits.split_at(layout.observes);
            results.push(BasicReadout::new(outs, flags, Status::default()));
        }
        Ok(BatchResult { results })
    }
}

/// One shot's biased coins: its seed, the readouts drawn, and the chance
/// that a readout is 1.
#[derive(Clone, Debug)]
pub struct BiasedCoins {
    seed: u64,
    drawn: u64,
    p: f64,
}

/// The `n`-th draw of the shot whose seed is `seed`, uniform in `[0, 1)`:
/// one splitmix64 step, its top 53 bits.
fn uniform(seed: u64, n: u64) -> f64 {
    let mut z = (seed ^ n).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    #[expect(clippy::cast_precision_loss, reason = "53 bits: exact in an f64")]
    let top = (z >> 11) as f64;
    top / (1u64 << 53) as f64
}

/// A biased coin per tag, in call order.
fn toss_biased(
    coins: &mut BiasedCoins,
    input: &CallInput<'_, InstructionCall>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    for call in input.instructions() {
        for operand in call.operands.iter() {
            if let Operand::Tag(tag) = *operand {
                let one = uniform(coins.seed, coins.drawn) < coins.p;
                coins.drawn += 1;
                outcomes.push(Outcome::new(tag, one.into()));
            }
        }
    }
    Ok(())
}

/// Coins of `set` that read 1 with chance `p`, a state per shot, each shot
/// drawing from its seed in a run of seed `seed`. At `p` 0 every readout
/// is 0, as on a noiseless device where the code's checks all pass; above
/// it, readouts flip, as noise would. Unlike [`coin_device`], it neither
/// checks its calls against `set` nor reports what it ran.
pub fn biased_coin_device(
    set: Arc<InstructionSet>,
    seed: u64,
    p: f64,
) -> OnePerShotExecutor<
    InstructionCall,
    BiasedCoins,
    impl MakeState<BiasedCoins>,
    impl ExecutorLogic<InstructionCall, BiasedCoins>,
> {
    let promise = Capabilities::default()
        .with_resumable_shots(true)
        .with_instruction_set(Some(set));
    let make = move |shot: ShotContext| {
        Ok(BiasedCoins {
            seed: shot.seed(),
            drawn: 0,
            p,
        })
    };
    executor_fn(promise, make, toss_biased)
        .seed(seed)
        .shots(OnePerShot)
}
