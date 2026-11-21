// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Fake parts for the behaviour tests: a part of each role, and the parts
//! the tests compose between them, on a toy whose shot state is classical
//! bits, written in QIR's `h`, `x` and `mz`.
//!
//! - Drivers: [`CoinCopy`], which branches on a readout, and [`Program`],
//!   one batch for every shot, or one for each half of them, in
//!   [`CoinGates`] or another set, such as [`ParityGates`].
//! - Executors: [`Bits`], and executors that answer later ([`Later`]) or
//!   from a thread ([`FromThread`]).
//! - Adapters: [`BitFlip`], which draws; [`Profiler`], which draws nothing;
//!   [`Splitter`], which sends each instruction as a batch of its own;
//!   [`NamesBelow`], which logs the names of the part below and its set;
//!   [`Spread`], an encoding, which may state the qubits it requires and
//!   provides; [`Stating`], a pass-through with capabilities of its own,
//!   one toy per constructor: a part reserving an ancilla, a job queue
//!   providing at most so many shots, a decoder restating its qubits, a
//!   link that cannot be seeded, and a lowering carrying no readout
//!   parities; [`Pool`], which draws two block types from one pool below;
//!   [`Trap`],
//!   which fails a shot; [`FailsOnce`], which fails, or panics in, one
//!   `adapt`;
//!   [`PanicsEnding`], which panics in `end_shots`, each time or once;
//!   [`EndsBelow`], which ends its shots below inside `adapt`;
//!   [`Impatient`], which stops waiting for its first answer from below;
//!   [`Misbehaving`], which breaks the protocol's rules;
//!   [`ForwardsParity`], which passes readout parities down while it
//!   requires none below; [`ReplaysItself`], which requires no replay
//!   below and seeded runs, and provides replay.
//! - Driver and executor: [`Panics`], which panics in `drive` and in
//!   `execute`.
//! - Any role: [`Renamed`], a part with a name other than its type's;
//!   [`PanicsInDrop`], a part whose futures may panic once dropped;
//!   [`SeesSeeds`], a part that logs the seed of each call it gets.
//!
//! Each part writes what it does to a shared [`Log`], which the test reads
//! once the parts are composed.
//!
//! Each role's fakes have a file of their own, and each fake of more than
//! one role too; this one holds what they share.

use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::OnceLock;
use std::task::{Context, Poll};

use crate::prelude::*;
use crate::{
    Batch, BatchReadouts, Capabilities, ExecuteError, Instruction, InstructionSet, OpcodeMap,
    ReadoutValue, Resources, Seed, instruction_set,
};

mod adapters;
mod drivers;
mod executors;
mod panics;
mod panics_in_drop;
mod renamed;
mod sees_seeds;

pub(crate) use adapters::*;
pub(crate) use drivers::*;
pub(crate) use executors::*;
pub(crate) use panics::{Panics, PanicsWhen};
pub(crate) use panics_in_drop::PanicsInDrop;
pub(crate) use renamed::Renamed;
pub(crate) use sees_seeds::SeesSeeds;

instruction_set! {
    /// QIR's `h`, `x` and `mz`: what every fake part requires and provides,
    /// unless a test gives it another set.
    pub(crate) CoinGates { H = h(qubit), X = x(qubit), MZ = mz(qubit) -> 1 outcome }
}

instruction_set! {
    /// `mz`, and an `x` run if a bit is 1, which a readout parity may give:
    /// what the tests of readout parities send.
    pub(crate) ParityGates { MZ = mz(qubit) -> 1 outcome, X_IF = x_if(qubit; c: Bit) }
}

/// What the fake parts did, in order, shared with the test that reads it.
#[derive(Clone, Default)]
pub(crate) struct Log(Rc<RefCell<Vec<String>>>);

impl Log {
    fn push(&self, entry: impl Into<String>) {
        self.0.borrow_mut().push(entry.into());
    }

    /// Every entry, in order.
    pub(crate) fn entries(&self) -> Vec<String> {
        self.0.borrow().clone()
    }

    /// How many entries are `entry`.
    pub(crate) fn count(&self, entry: &str) -> usize {
        self.0.borrow().iter().filter(|e| *e == entry).count()
    }
}

/// A shot's random stream: `SplitMix64`, seeded from the shot's seed.
struct Stream(u64);

impl Stream {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `true` with probability `probability`.
    fn chance(&mut self, probability: f64) -> bool {
        // The top 52 bits as the fraction of a number in [1, 2), minus 1:
        // uniform in [0, 1).
        f64::from_bits(0x3FF0_0000_0000_0000 | (self.next() >> 12)) - 1.0 < probability
    }
}

/// [`CoinGates`]' set, made once: what most fakes accept and send.
pub(crate) fn coin_gates() -> &'static InstructionSet {
    static SET: OnceLock<InstructionSet> = OnceLock::new();
    SET.get_or_init(CoinGates::set)
}

/// [`CoinGates`]' capabilities, stating no resource and no feature: what
/// a part that forwards every resource and feature requires and provides.
pub(crate) fn coin_capabilities() -> &'static Capabilities {
    static CAPABILITIES: OnceLock<Capabilities> = OnceLock::new();
    CAPABILITIES.get_or_init(|| Capabilities::new(CoinGates::set()))
}

/// `n` qubits, and no other resource.
pub(crate) fn qubits(n: u32) -> Resources {
    Resources::new().with_qubits(n)
}

/// `instructions`, rewritten in the receiver's opcodes.
fn mapped(map: &OpcodeMap, instructions: &[Instruction]) -> Vec<Instruction> {
    instructions
        .iter()
        .map(|instruction| {
            let mut mapped = instruction.clone();
            mapped.opcode = map.map(instruction.opcode);
            mapped
        })
        .collect()
}

/// Sends `batches` below, each to its own shots with the instructions
/// `rewrite` makes of it, checked if it is, in one `execute`, and answers
/// with the readouts below, filled into `readouts`: the run time of an
/// adapter that keeps each batch's shots and readout slots.
async fn forward(
    batches: &[Batch<'_>],
    seed: Seed,
    below: &mut impl Executor,
    readouts: &mut [BatchReadouts],
    rewrite: impl FnMut(&Batch<'_>) -> Vec<Instruction>,
) -> Result<(), ExecuteError> {
    let sent: Vec<Vec<Instruction>> = batches.iter().map(rewrite).collect();
    let below_batches: Vec<Batch<'_>> = batches
        .iter()
        .zip(&sent)
        .map(|(batch, instructions)| {
            Batch::new(batch.shots, instructions, batch.readout_slots())
                .checked_if(batch.is_checked())
        })
        .collect();
    below.execute(&below_batches, seed, readouts).await
}

/// The receiver's opcodes, given by the join's `bind`.
fn bound(map: Option<&OpcodeMap>) -> &OpcodeMap {
    map.expect("the join binds before any `execute`")
}

/// The instruction `mnemonic` of [`CoinGates`] on `qubit`.
pub(crate) fn coin_gate(mnemonic: &str, qubit: u32) -> Instruction {
    CoinGates::set()
        .instruction(mnemonic, [qubit], &[])
        .expect("an instruction of CoinGates")
}

/// Each shot's values, in the order it read them.
pub(crate) type Values = Vec<ReadoutValue>;

/// Runs `$check` on the chain of the parts `[$d, …, $e]`, top to bottom,
/// in each bracketing `through` and `on` make of them: 2 for three parts,
/// 5 for four and 14 for five. Each bracketing runs `$setup`, a block of
/// statements that binds the parts, afresh, so it gets new parts (and a
/// new log, if `$setup` makes one); `$check` reads the chain the joins
/// make as `$chain`, and the bracketing, written out, as `$brackets`, for
/// its messages.
macro_rules! in_each_bracketing {
    (
        [$d:ident, $a:ident, $e:ident] from $setup:tt,
        |$chain:ident, $brackets:ident| $check:expr $(,)?
    ) => {
        $crate::tests::fake_parts::in_each_bracketing!(
            @each $setup $chain $brackets $check;
            $d.through($a).on($e),
            $d.on($a.on($e)),
        )
    };
    (
        [$d:ident, $a:ident, $b:ident, $e:ident] from $setup:tt,
        |$chain:ident, $brackets:ident| $check:expr $(,)?
    ) => {
        $crate::tests::fake_parts::in_each_bracketing!(
            @each $setup $chain $brackets $check;
            $d.through($a).through($b).on($e),
            $d.through($a).on($b.on($e)),
            $d.on($a.on($b.on($e))),
            $d.on($a.through($b).on($e)),
            $d.through($a.through($b)).on($e),
        )
    };
    (
        [$d:ident, $a:ident, $b:ident, $c:ident, $e:ident] from $setup:tt,
        |$chain:ident, $brackets:ident| $check:expr $(,)?
    ) => {
        $crate::tests::fake_parts::in_each_bracketing!(
            @each $setup $chain $brackets $check;
            $d.on($a.through($b).through($c).on($e)),
            $d.on($a.through($b.through($c)).on($e)),
            $d.on($a.through($b).on($c.on($e))),
            $d.on($a.on($b.through($c).on($e))),
            $d.on($a.on($b.on($c.on($e)))),
            $d.through($a).on($b.through($c).on($e)),
            $d.through($a).on($b.on($c.on($e))),
            $d.through($a).through($b).on($c.on($e)),
            $d.through($a.through($b)).on($c.on($e)),
            $d.through($a).through($b).through($c).on($e),
            $d.through($a.through($b)).through($c).on($e),
            $d.through($a).through($b.through($c)).on($e),
            $d.through($a.through($b).through($c)).on($e),
            $d.through($a.through($b.through($c))).on($e),
        )
    };
    (@each $setup:tt $chain:ident $brackets:ident $check:tt; $bracketing:expr, $($rest:expr,)*) => {
        $crate::tests::fake_parts::in_each_bracketing!(
            @one $setup $chain $brackets $check; $bracketing
        );
        $crate::tests::fake_parts::in_each_bracketing!(
            @each $setup $chain $brackets $check; $($rest,)*
        );
    };
    (@each $setup:tt $chain:ident $brackets:ident $check:tt;) => {};
    (
        @one { $($setup:tt)* } $chain:ident $brackets:ident $check:tt;
        $bracketing:expr
    ) => {{
        $($setup)*
        #[allow(unused_variables)]
        let $brackets: &str = stringify!($bracketing);
        let $chain = $bracketing;
        $check
    }};
}

pub(crate) use in_each_bracketing;

// ---------------------------------------------------------------- futures

/// Gives the other tasks a turn: pending once, and woken at once.
#[derive(Default)]
pub(crate) struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        context.waker().wake_by_ref();
        Poll::Pending
    }
}
