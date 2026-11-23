// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Fake parts for the behaviour tests: a part of each role, and the parts
//! the tests compose between them, on a toy whose shot state is classical
//! bits, written in the [`Measuring`] gates `h`, `x` and `mz`.
//!
//! - Drivers: [`CoinCopy`], which branches on a readout; [`Program`],
//!   one batch for every shot, or one for each half of them, in
//!   [`coin_gates`], requiring the run's shots, and seeded runs if its
//!   seed is fixed; [`PassesTheRun`], which keeps the default `requires`.
//! - Executors: [`Bits`]; executors that answer later ([`Later`]), get
//!   ready later ([`ReadiesLater`]) or answer from a thread
//!   ([`FromThread`]); [`StatesNoLimits`], which keeps
//!   the default `provides`; and [`Zeros`], of any set, which reads `Zero`
//!   everywhere.
//! - Adapters: [`BitFlip`], which draws; [`Profiler`], which draws nothing;
//!   [`Splitter`], which sends each instruction as a batch of its own;
//!   [`NamesBelow`], which logs the names of the part below and its set;
//!   [`Spread`], an encoding, which may state the instructions a batch it
//!   takes and requires; [`Stating`], a pass-through with capabilities of
//!   its own, one toy per constructor: an interleaver, a part adding
//!   ancilla resets, a job splitter, a job queue, a cap on instructions a
//!   batch, a replayer, and a sampler seeding its runs itself;
//!   [`Trap`],
//!   which fails a shot; [`FailsOnce`], which fails, or panics in, one
//!   `adapt`;
//!   [`PanicsEnding`], which panics in `end_shots`, each time or once;
//!   [`EndsBelow`], which ends its shots below inside `adapt`;
//!   [`Impatient`], which stops waiting for its first answer from below;
//!   [`Misbehaving`], which breaks the protocol's rules; and the
//!   translating adapter [`GateForGate`], which binds to the receiver's
//!   set.
//! - Driver and executor: [`Panics`], which panics in `drive` and in
//!   `execute`.
//! - Any role: [`Renamed`], a part with a name other than its type's,
//!   which may log the seed of each call it gets; and [`PanicsInDrop`], a
//!   part whose futures may panic once dropped.
//! - On [`QubitLimited`](crate::tests::example_sets::QubitLimited), whose
//!   capabilities are a count of qubits: [`QubitProgram`], a driver;
//!   [`QubitPassThrough`], an adapter keeping the defaults;
//!   [`QubitNarrowing`], an adapter requiring fewer qubits below than it
//!   sends on; and [`QubitRegister`], an executor.
//! - On [`Resettable`](crate::tests::example_sets::Resettable), whose
//!   capabilities are qubits and a reset: [`ResettableProgram`], a driver
//!   needing qubits; [`AncillaReuse`], an adapter needing a reset
//!   below; and [`ResetInsertion`], a translating adapter onto it from
//!   [`QubitLimited`](crate::tests::example_sets::QubitLimited), adding a
//!   need of reset.
//! - Across a translation of
//!   [`WithToffoli`](crate::tests::example_sets::WithToffoli) to
//!   [`WithoutToffoli`](crate::tests::example_sets::WithoutToffoli):
//!   [`ToffoliProgram`], a driver; [`RoundsIntoItsReadouts`], a driver
//!   sending rounds into the readouts it keeps; [`CliffordTRegister`], an
//!   executor; [`CliffordTPassThrough`], an adapter of the set sent below;
//!   and [`CczDecomposition`], the translating adapter between them, which
//!   decomposes `ccz`.
//!
//! Each part writes what it does to a shared [`Log`], which the test reads
//! once the parts are composed.
//!
//! Each role's fakes have a file of their own, and each fake of more than
//! one role too; this one holds what they share.

use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use crate::prelude::*;
use crate::tests::example_sets::BitReadout;
use crate::tests::example_sets::{Gate, Measuring, NoCapabilities};
use crate::{Batch, BatchReadouts, Capabilities, ExecuteError, Features, Resources, Seed};

mod adapters;
mod drivers;
mod executors;
mod panics;
mod panics_in_drop;
mod qubit_limited;
mod renamed;
mod resettable;
mod toffoli;

pub(crate) use adapters::*;
pub(crate) use drivers::*;
pub(crate) use executors::*;
pub(crate) use panics::{Panics, PanicsWhen};
pub(crate) use panics_in_drop::PanicsInDrop;
pub(crate) use qubit_limited::{QubitNarrowing, QubitPassThrough, QubitProgram, QubitRegister};
pub(crate) use renamed::Renamed;
pub(crate) use resettable::{AncillaReuse, ResetInsertion, ResettableProgram};
pub(crate) use toffoli::{
    CczDecomposition, CliffordTPassThrough, CliffordTRegister, RoundsIntoItsReadouts,
    ToffoliProgram,
};

/// `h`, `x` and `mz`: what every fake part accepts and sends, unless a
/// test gives it another set.
static COIN_GATES: Measuring = Measuring::new(&["h", "x", "mz"]);

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

    /// The entries containing `part`, in order.
    pub(crate) fn entries_containing(&self, part: &str) -> Vec<String> {
        self.entries_where(|entry| entry.contains(part))
    }

    /// The entries starting with `prefix`, in order.
    pub(crate) fn entries_starting_with(&self, prefix: &str) -> Vec<String> {
        self.entries_where(|entry| entry.starts_with(prefix))
    }

    fn entries_where(&self, keep: impl Fn(&str) -> bool) -> Vec<String> {
        let entries = self.0.borrow();
        entries.iter().filter(|e| keep(e)).cloned().collect()
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

/// `h`, `x` and `mz`: what most fakes accept and send.
pub(crate) fn coin_gates() -> &'static Measuring {
    &COIN_GATES
}

/// A gate at position 99, beyond [`coin_gates`] and every other set the
/// tests give a part, as a faulty sender writes it.
pub(crate) const BEYOND_THE_SET: Gate = Gate { at: 99, qubit: 0 };

/// The gate `name` of [`coin_gates`] on `qubit`.
pub(crate) fn coin_gate(name: &str, qubit: u32) -> Gate {
    COIN_GATES.gate(name, qubit)
}

/// The capabilities `resources` and `features` of a [`Measuring`] set,
/// which has none of its own.
pub(crate) fn capabilities(resources: Resources, features: Features) -> Capabilities<Measuring> {
    Capabilities::new(resources, features, NoCapabilities)
}

/// Batches of at most `n` instructions, and no other resource.
pub(crate) fn batches_of(n: u32) -> Resources {
    Resources::nothing_stated().with_instructions_per_batch(n)
}

/// What `part` logs once ready for `above`: the instructions a batch it
/// gets may hold, 0 if unstated.
pub(crate) fn ready_for(part: &str, above: &Capabilities<Measuring>) -> String {
    let n = above.resources().instructions_per_batch.unwrap_or(0);
    format!("{part} ready for batches of {n}")
}

/// `gates`, re-encoded with `binding`, the receiver's.
fn mapped(binding: &[u32], gates: &[Gate]) -> Vec<Gate> {
    gates
        .iter()
        .map(|&gate| Measuring::bound(binding, gate))
        .collect()
}

/// Sends `batches` below, each to its own shots with the instructions
/// `rewrite` makes of it, checked if it is, in one `execute`, and answers
/// with the readouts below, filled into `readouts`: the run time of an
/// adapter that keeps each batch's shots and readout slots.
async fn forward(
    batches: &[Batch<'_, Measuring>],
    seed: Seed,
    below: &mut impl Executor<Accepts = Measuring>,
    readouts: &mut [BatchReadouts<Measuring>],
    rewrite: impl FnMut(&Batch<'_, Measuring>) -> Vec<Gate>,
) -> Result<(), ExecuteError> {
    let sent: Vec<Vec<Gate>> = batches.iter().map(rewrite).collect();
    let below_batches: Vec<Batch<'_, Measuring>> = batches
        .iter()
        .zip(&sent)
        .map(|(batch, instructions)| {
            Batch::new(batch.shots, instructions, batch.readout_slots())
                .checked_if(batch.is_checked())
        })
        .collect();
    below.execute(&below_batches, seed, readouts).await
}

/// The receiver's encoding, given by the join's `bind`.
fn bound(binding: Option<&Vec<u32>>) -> &[u32] {
    binding.expect("the join binds before any `execute`")
}

/// Each shot's values, in the order it read them.
pub(crate) type Values = Vec<BitReadout>;

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
