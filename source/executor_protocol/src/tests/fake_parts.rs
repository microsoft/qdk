// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Fake parts for the behaviour tests: a part of each role, and the parts
//! the tests compose between them, on a toy whose shot state is classical
//! bits, written in QIR's `h`, `x` and `mz`.
//!
//! - Drivers: [`CoinCopy`], which branches on a readout, and [`Program`],
//!   one batch for every shot, or one for each half of them.
//! - Executors: [`Bits`], and executors that answer later ([`Later`]),
//!   from a thread ([`FromThread`]), with the wrong shape ([`Misshapen`]),
//!   or by panicking ([`Panics`], also a driver).
//! - Adapters: [`BitFlip`], which draws; [`Profiler`], which draws nothing;
//!   [`Splitter`], which sends each instruction as a batch of its own;
//!   [`NamesBelow`], which logs the name of the part below;
//!   [`Spread`], which changes the needs and the offer; [`Capped`], a
//!   `Spread` with a most of its own; [`Pool`], which draws two block
//!   types from one pool below; [`Queue`], which lowers the most shots;
//!   [`LoweringWithoutParity`], which offers no readout parities;
//!   [`Contrary`], whose offer shrinks as the offer below grows;
//!   [`Workspace`], which takes qubits below it does not ask for; [`Trap`],
//!   which fails a shot; [`FailsOnce`], which fails one `adapt`;
//!   [`PanicsEnding`], which panics in `end_shots`, each time or once;
//!   [`Shifting`], whose offer changes once provisioned; [`Misbehaving`],
//!   which breaks the protocol's rules.
//! - Any role: [`Renamed`], a part with a name other than its type's.
//!
//! Each part writes what it does to a shared [`Log`], which the test reads
//! once the parts are composed.
//!
//! Each role's fakes have a file of their own; this one holds what they
//! share.

use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::OnceLock;
use std::task::{Context, Poll};

use crate::{
    Batch, BatchReadouts, Execute, ExecuteError, Instruction, InstructionSet, OpcodeMap,
    ReadoutValue, Seed, ShotConditions, instruction_set,
};

mod adapters;
mod drivers;
mod executors;
mod renamed;

pub(crate) use adapters::*;
pub(crate) use drivers::*;
pub(crate) use executors::*;
pub(crate) use renamed::Renamed;

instruction_set! {
    /// QIR's `h`, `x` and `mz`: what every fake part sends and accepts,
    /// unless a test gives it another set.
    pub(crate) CoinGates { H = h(qubit), X = x(qubit), MZ = mz(qubit) -> 1 outcome }
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

    /// `true` with probability `per_mille` / 1000.
    fn chance(&mut self, per_mille: u64) -> bool {
        self.next() % 1000 < per_mille
    }
}

/// [`CoinGates`]' set, made once: what most fakes accept and send.
fn coin_gates() -> &'static InstructionSet {
    static SET: OnceLock<InstructionSet> = OnceLock::new();
    SET.get_or_init(CoinGates::set)
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
/// `rewrite` makes of it, in one `execute`, and answers with the readouts
/// below: the run time of an adapter that keeps each batch's shots.
async fn forward(
    batches: &[Batch<'_>],
    seed: Seed,
    below: &mut impl Execute,
    rewrite: impl FnMut(&Batch<'_>) -> Vec<Instruction>,
) -> Result<Vec<BatchReadouts>, ExecuteError> {
    let sent: Vec<Vec<Instruction>> = batches.iter().map(rewrite).collect();
    let below_batches: Vec<Batch<'_>> = batches
        .iter()
        .zip(&sent)
        .map(|(batch, instructions)| Batch {
            shots: batch.shots,
            instructions,
        })
        .collect();
    below.execute(&below_batches, seed).await
}

/// The receiver's opcodes, given by `>>`'s `bind`.
fn bound(map: Option<&OpcodeMap>) -> &OpcodeMap {
    map.expect("`>>` binds before any `execute`")
}

/// The instruction `mnemonic` of [`CoinGates`] on `qubit`.
pub(crate) fn coin_gate(mnemonic: &str, qubit: u32) -> Instruction {
    CoinGates::set()
        .instruction(mnemonic, [qubit], &[])
        .expect("an instruction of CoinGates")
}

/// Each shot's values, in the order it read them.
pub(crate) type Values = Vec<ReadoutValue>;

/// `rows` rows of `width` values `Zero`.
fn zeros(rows: usize, width: usize) -> BatchReadouts {
    let mut readouts =
        BatchReadouts::with_capacity(rows, u32::try_from(width).expect("a small width"));
    for _ in 0..rows {
        readouts
            .push_row(&vec![ReadoutValue::Zero; width], ShotConditions::NONE)
            .expect("`width` values");
    }
    readouts
}

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
