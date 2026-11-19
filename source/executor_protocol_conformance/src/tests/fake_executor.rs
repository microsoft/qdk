// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`FakeExecutor`]: a toy executor on classical bits, which keeps every rule
//! of an executor, or breaks one ([`Breaks`]).

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use executor_protocol::{
    Batch, BatchReadouts, Execute, ExecuteError, ExecuteRefusal, ExecutorSetup, InstructionSet,
    Needs, NotReady, Offer, Opcode, Seed, ShotConditions, ShotId, ShotSet, instruction_set,
};

use super::{CoinGates, QUBITS};
use crate::splitmix64::{SplitMix64, mix};
use crate::trials::detection_rates;

/// The rule a [`FakeExecutor`] breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Breaks {
    /// It leaves the last readout slot of shot 1's rows unwritten.
    LeavesASlotUnwritten,
    /// It leaves the last readout slot unwritten on the rows of shots
    /// above 3, as a part fitted to runs on shots 0 to 3 alone would, its
    /// values kept in a table of four.
    LeavesSlotsOfShotsAboveThreeUnwritten,
    /// It leaves unwritten each readout slot at or beyond the readout
    /// slots of the `execute`'s first batch, as a part sizing its rows
    /// once per `execute`, by its first batch, would.
    SizesItsRowsByTheFirstBatch,
    /// It answers the first batch of each `execute` with readouts of one
    /// more slot.
    ReplacesItsReadouts,
    /// It answers the first batch of each `execute` with readouts of one
    /// more row.
    AddsARowToItsReadouts,
    /// Its shots toss their bits from one stream per `execute`, shared, so
    /// a shot's draws depend on the shots before it.
    SharesItsDraws,
    /// It makes each shot again at each `execute`.
    RestartsItsShots,
    /// Each `execute` flips the second bit of every shot it holds, not
    /// named, whose id lies between two the `execute` names, as one run
    /// over a range of ids would.
    RunsHeldShotsBetweenItsOwn,
    /// Each `execute` flips the second bit of every shot it holds, not
    /// named, whose id lies above every id the `execute` names, as a run
    /// over a range of ids one past its end would.
    RunsHeldShotsAboveItsOwn,
    /// Each `execute` flips the second bit of every shot it holds, not
    /// named, made before the last shot it names, as a run over the slots
    /// of a device, filled in the order the shots came, up to the last
    /// slot named would.
    RunsHeldShotsMadeBeforeItsOwn,
    /// Each `execute` flips the second bit of every shot it holds, not
    /// named, made after the first shot it names, as a run over the slots
    /// of a device, filled in the order the shots came, from the first
    /// slot named on would.
    RunsHeldShotsMadeAfterItsOwn,
    /// It makes a shot's stream, at its first batch, from the seed, the
    /// shot and the batch's place in its `execute`, as a device seeding
    /// each of its queues apart would.
    SeedsNewShotsByBatch,
    /// It keeps each shot's state at the shot's row in its batch, not at
    /// its id, as a list indexed by row would.
    KeysShotsByRow,
    /// It does an `execute`'s work when called, before its future is
    /// polled.
    WorksWhenCalled,
    /// It does an `execute`'s work when called, and if that call's future
    /// is dropped unpolled, answers the next call with nothing written,
    /// taking the dropped call's work for it.
    WorksWhenCalledAndReuses,
    /// It offers fewer qubits once ready.
    ChangesItsOffer,
    /// It accepts [`CoinGatesAndReset`] once ready.
    ChangesWhatItAccepts,
    /// Once it ended a shot, it refuses every later `execute`.
    RefusesAfterEndingShots,
    /// It ignores `end_shots`, keeping each shot's state past its end.
    KeepsEndedShots,
    /// It answers row `i` of a batch with the state of the batch's `i`-th
    /// smallest shot id, as a device iterating its shots in a sorted map
    /// would.
    SortsItsRows,
    /// It raises `REJECTED` on every row of a batch of three shots or
    /// more, as a device rejecting a batch as a whole would.
    RejectsBatchesOfMany,
    /// It raises `REJECTED` on each row of a shot it held before the
    /// `execute`, as one distrusting a kept state would.
    FlagsContinuedShots,
    /// It makes each shot with an id of `n` or above again at each
    /// `execute`, as a part keeping its states in a table of `n` would: of
    /// 4, fitted to runs on shots 0 to 3 alone; of 64, a `u64` bit mask; of
    /// 4,096, a table sized for the ids most callers give.
    RestartsShotsFrom(u32),
    /// It keys each shot's state by the low 16 bits of its id, as a part
    /// storing ids as `u16` would: shots `k` and `k + 2¹⁶` share one state.
    KeysShotsByTheirLow16Bits,
    /// The shots of every batch after an `execute`'s first toss their bits
    /// from one stream, shared, as a part handling a lone batch apart
    /// would.
    SharesItsDrawsBeyondTheFirstBatch,
    /// An `execute` on one shot drops every other shot it holds, as a part
    /// whose path for a lone shot sets its device up again would.
    DropsItsShotsOnAnExecuteOfOneShot,
    /// At `end_shots`, it keeps each shot that ran in two `execute`s or
    /// more, as a part keeping continued shots in a table of their own,
    /// which `end_shots` misses, would.
    KeepsContinuedShotsPastTheirEnd,
    /// It panics in `end_shots` on a shot it does not hold, as a part
    /// taking each ended shot out of a table with `expect` would, having
    /// ended the shots before it.
    PanicsEndingShotsItDoesNotHold,
    /// It panics in `end_shots` on an id above every shot it made, as a
    /// part keeping each shot's state at its id in a vector, grown to the
    /// largest it made, would, having ended the shots before it.
    PanicsEndingShotsBeyondItsTable,
}

/// What a [`FakeExecutor`] does that breaks no rule, though a check meets
/// it: a run it cannot hold, refuses, or fails. Set by its constructors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Allowed {
    /// It offers runs of at most four shots.
    OffersRunsOfFourShotsAtMost,
    /// It refuses its run of this count, from 1, as busy, in `ready`.
    IsBusyAtRun(u32),
    /// It panics in every `end_shots`.
    PanicsEndingShots,
    /// It refuses every `execute`.
    RefusesEveryExecute,
}

instruction_set! {
    /// [`CoinGates`] and QIR's `reset`: what a [`FakeExecutor`] breaking
    /// [`Breaks::ChangesWhatItAccepts`] accepts once ready.
    pub(crate) CoinGatesAndReset {
        H = h(qubit), X = x(qubit), MZ = mz(qubit) -> 1 outcome, RESET = reset(qubit)
    }
}

/// Each shot's bits and stream, by shot: an executor's own, or a device's
/// that several executors share.
pub(crate) type Device = Rc<RefCell<BTreeMap<ShotId, (Vec<bool>, SplitMix64)>>>;

/// How a fake part draws its shots' random values.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Draws {
    /// Each shot's from its own stream, so that its shots are replayable.
    ShotByShot,
    /// A run's shots' from one stream per `execute`, shared, as a sampler
    /// drawing a run's shots in one call does: the part offers its shots
    /// not replayable, as the protocol then asks.
    Together,
}

/// A toy executor on classical bits, accepting [`CoinGates`]: `h` tosses
/// a bit from the shot's stream, `x` flips it, and `mz` reads it.
pub(crate) struct FakeExecutor {
    set: InstructionSet,
    /// What it accepts once ready, if it breaks
    /// [`Breaks::ChangesWhatItAccepts`].
    wider_set: InstructionSet,
    opcodes: [Opcode; 3],
    breaks: Option<Breaks>,
    shots: Device,
    /// Whether its `ready` answered `Ok` once.
    got_ready: bool,
    ended: bool,
    /// The `execute`s of every executor that shares it, if its readouts
    /// drift with them.
    drift: Option<Rc<Cell<u64>>>,
    /// Whether it raises `REJECTED` on each row whose shot's bit 1 holds 1
    /// once the batch has run.
    flags: bool,
    /// How its shots toss their bits, which sets whether it offers them
    /// replayable.
    draws: Draws,
    /// Whether a call's work is done and its future was dropped unpolled,
    /// for [`Breaks::WorksWhenCalledAndReuses`]: set at the call, cleared
    /// when its future is polled.
    pending: Rc<Cell<bool>>,
    /// The shots it ran in two `execute`s or more, for
    /// [`Breaks::KeepsContinuedShotsPastTheirEnd`]: a table `end_shots`
    /// misses, at every end.
    continued: BTreeSet<ShotId>,
    /// The largest id it made a shot at, for
    /// [`Breaks::PanicsEndingShotsBeyondItsTable`]: its table's last.
    largest_made: Option<ShotId>,
    /// The shots it holds, in the order it made them, for
    /// [`Breaks::RunsHeldShotsMadeBeforeItsOwn`] and
    /// [`Breaks::RunsHeldShotsMadeAfterItsOwn`].
    made: Vec<ShotId>,
    /// How many times it was asked whether it can take a run, for
    /// [`Allowed::IsBusyAtRun`].
    asked_ready: u32,
    /// What it does that breaks no rule, if anything: set by its
    /// constructors.
    allowed: Option<Allowed>,
    /// What panics if it is dropped holding a shot, if it is strict: held
    /// for its `Drop` alone.
    _leak_check: Option<PanicsOnHeldShots>,
}

/// Panics when dropped while its device holds a shot, as a device handle
/// that checks for leaks would.
struct PanicsOnHeldShots(Device);

impl Drop for PanicsOnHeldShots {
    fn drop(&mut self) {
        let held: Vec<ShotId> = self.0.borrow().keys().copied().collect();
        assert!(
            held.is_empty() || std::thread::panicking(),
            "dropped with shots never ended: {held:?}"
        );
    }
}

impl FakeExecutor {
    /// An executor that keeps every rule.
    pub(crate) fn new() -> Self {
        let set = CoinGates::set();
        let opcode = |mnemonic| set.opcode(mnemonic).expect("a mnemonic of CoinGates");
        let made = Self {
            opcodes: [opcode("h"), opcode("x"), opcode("mz")],
            set,
            wider_set: CoinGatesAndReset::set(),
            breaks: None,
            shots: Device::default(),
            got_ready: false,
            ended: false,
            drift: None,
            flags: false,
            draws: Draws::ShotByShot,
            pending: Rc::default(),
            continued: BTreeSet::new(),
            largest_made: None,
            made: Vec::new(),
            asked_ready: 0,
            allowed: None,
            _leak_check: None,
        };
        detection_rates::name_the_part("FakeExecutor::new");
        made
    }

    /// An executor that keeps every rule, and panics when dropped holding a
    /// shot, as a device handle that checks for leaks would: the protocol's
    /// runner ends every shot it runs, so this keeps the protocol.
    pub(crate) fn strict() -> Self {
        let executor = Self::new();
        let made = Self {
            _leak_check: Some(PanicsOnHeldShots(executor.shots.clone())),
            ..executor
        };
        detection_rates::name_the_part("FakeExecutor::strict");
        made
    }

    /// An executor that breaks `breaks`.
    pub(crate) fn breaking(breaks: Breaks) -> Self {
        let made = Self {
            breaks: Some(breaks),
            ..Self::new()
        };
        detection_rates::name_the_part(format!("FakeExecutor::breaking({breaks:?})"));
        made
    }

    /// An executor holding its shots on `device`, which keeps a shot no
    /// executor ended: a device reset only at some point.
    pub(crate) fn on_device(device: &Device) -> Self {
        let made = Self {
            shots: device.clone(),
            ..Self::new()
        };
        detection_rates::name_the_part("FakeExecutor::on_device");
        made
    }

    /// An executor whose every readout value flips with the parity of the
    /// `execute`s counted in `executes`, shared by the executors of one
    /// test: a part whose values change from run to run, as a GPU's sums
    /// in another order do, and that writes every value.
    pub(crate) fn drifting(executes: &Rc<Cell<u64>>) -> Self {
        let made = Self {
            drift: Some(executes.clone()),
            ..Self::new()
        };
        detection_rates::name_the_part("FakeExecutor::drifting");
        made
    }

    /// An executor that keeps every rule and raises `REJECTED` on each row
    /// whose shot's bit 1 holds 1 once the batch has run, as a device
    /// flagging a shot by its own state would: a shot's conditions then
    /// depend on its state alone, as its values do.
    pub(crate) fn flagging() -> Self {
        let made = Self {
            flags: true,
            ..Self::new()
        };
        detection_rates::name_the_part("FakeExecutor::flagging");
        made
    }

    /// An executor whose shots toss their bits from one stream per
    /// `execute`, shared, as those breaking [`Breaks::SharesItsDraws`] do,
    /// and that offers shots that are not replayable, as a sampler drawing a
    /// run's shots in one call does: a shot's records depend on the shots
    /// run beside it, which the protocol allows such a part.
    pub(crate) fn drawing_shots_together() -> Self {
        let made = Self {
            draws: Draws::Together,
            ..Self::new()
        };
        detection_rates::name_the_part("FakeExecutor::drawing_shots_together");
        made
    }

    /// An executor that keeps every rule and offers runs of at most four
    /// shots, as a device with room for four shots' states would: no room
    /// for a shot beyond a script's four.
    pub(crate) fn offering_four_shots_at_most() -> Self {
        let made = Self {
            allowed: Some(Allowed::OffersRunsOfFourShotsAtMost),
            ..Self::new()
        };
        detection_rates::name_the_part("FakeExecutor::offering_four_shots_at_most");
        made
    }

    /// An executor that keeps every rule and refuses its `run`-th run as
    /// busy, in `ready`, taking the others: at its first, as a device
    /// still busy with another user's run would; at its second, as one
    /// another user took between the two would.
    pub(crate) fn busy_at_run(run: u32) -> Self {
        let made = Self {
            allowed: Some(Allowed::IsBusyAtRun(run)),
            ..Self::new()
        };
        detection_rates::name_the_part(format!("FakeExecutor::busy_at_run({run})"));
        made
    }

    /// An executor that panics in every `end_shots`, as a part whose
    /// release of a shot's state fails would: a fault of its own, not of
    /// the ends it is sent, which fails the run and breaks no rule the
    /// kit checks.
    pub(crate) fn panicking_ending_shots() -> Self {
        let made = Self {
            allowed: Some(Allowed::PanicsEndingShots),
            ..Self::new()
        };
        detection_rates::name_the_part("FakeExecutor::panicking_ending_shots");
        made
    }

    /// An executor that keeps every rule and refuses every `execute`, so
    /// that every run on it fails.
    pub(crate) fn refusing_every_execute() -> Self {
        let made = Self {
            allowed: Some(Allowed::RefusesEveryExecute),
            ..Self::new()
        };
        detection_rates::name_the_part("FakeExecutor::refusing_every_execute");
        made
    }

    /// An executor holding its shots on `device`, that breaks `breaks`.
    pub(crate) fn on_device_breaking(device: &Device, breaks: Breaks) -> Self {
        let made = Self {
            breaks: Some(breaks),
            ..Self::on_device(device)
        };
        detection_rates::name_the_part(format!("FakeExecutor::on_device_breaking({breaks:?})"));
        made
    }

    fn breaks(&self, breaks: Breaks) -> bool {
        self.breaks == Some(breaks)
    }

    /// What it keeps `shot`'s state under, at row `position` of a batch:
    /// its id, unless it breaks a rule by keying it otherwise.
    fn key(&self, shot: ShotId, position: usize) -> ShotId {
        if self.breaks(Breaks::KeysShotsByRow) {
            ShotId::new(u32::try_from(position).expect("a row fits a shot id"))
        } else if self.breaks(Breaks::KeysShotsByTheirLow16Bits) {
            low_16_bits(shot)
        } else {
            shot
        }
    }

    /// Changes the shots it holds before an `execute` of `batches` runs,
    /// if it breaks a rule by doing so: restarts them, drops them, or runs
    /// some of those the `execute` does not name.
    fn change_held_shots(&mut self, batches: &[Batch<'_>]) {
        let mut shots = self.shots.borrow_mut();
        let named: BTreeSet<ShotId> = batches
            .iter()
            .flat_map(|batch| batch.shots.iter())
            .collect();
        self.made.retain(|shot| shots.contains_key(shot));
        for shot in batches.iter().flat_map(|batch| batch.shots.iter()) {
            if !self.made.contains(&shot) {
                self.made.push(shot);
            }
        }
        let before = self.breaks(Breaks::RunsHeldShotsMadeBeforeItsOwn);
        if before || self.breaks(Breaks::RunsHeldShotsMadeAfterItsOwn) {
            let places: Vec<usize> = (0..self.made.len())
                .filter(|&place| named.contains(&self.made[place]))
                .collect();
            let (lowest, highest) = (places[0], places[places.len() - 1]);
            let runs = if before {
                &self.made[..=highest]
            } else {
                &self.made[lowest..]
            };
            for shot in runs.iter().filter(|shot| !named.contains(shot)) {
                if let Some((bits, _)) = shots.get_mut(shot) {
                    bits[1] = !bits[1];
                }
            }
        }
        if self.breaks(Breaks::RestartsItsShots) {
            shots.clear();
        }
        if named.len() == 1 && self.breaks(Breaks::DropsItsShotsOnAnExecuteOfOneShot) {
            shots.retain(|shot, _| named.contains(shot));
        }
        let above = self.breaks(Breaks::RunsHeldShotsAboveItsOwn);
        if above || self.breaks(Breaks::RunsHeldShotsBetweenItsOwn) {
            let (first, last) = (named.first().copied(), named.last().copied());
            for (shot, (bits, _)) in shots.iter_mut() {
                let runs = if above {
                    Some(*shot) > last
                } else {
                    Some(*shot) > first && Some(*shot) < last
                };
                if !named.contains(shot) && runs {
                    bits[1] = !bits[1];
                }
            }
        }
    }

    /// The readout slots it writes in each row of an `execute`: as many as
    /// the first batch's when it sizes its rows by the first batch, else
    /// every slot.
    fn row_slots(&self, batches: &[Batch<'_>]) -> usize {
        if self.breaks(Breaks::SizesItsRowsByTheFirstBatch) {
            batches[0].readout_slots() as usize
        } else {
            usize::MAX
        }
    }

    /// Runs `batches`, writing their readouts.
    fn run(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        if self.allowed == Some(Allowed::RefusesEveryExecute)
            || self.ended && self.breaks(Breaks::RefusesAfterEndingShots)
        {
            let shot = batches[0]
                .shots
                .iter()
                .next()
                .expect("a batch holds a shot");
            return Err(ExecuteError::Refused(ExecuteRefusal::ShotNotHeld(shot)));
        }
        let drift = self.drift.as_ref().is_some_and(|executes| {
            executes.set(executes.get() + 1);
            mix(executes.get()) & 1 == 1
        });
        self.change_held_shots(batches);
        let mut shots = self.shots.borrow_mut();
        let mut shared = SplitMix64::new(seed.shot(ShotId::new(0)));
        let row_slots = self.row_slots(batches);
        let [h, x, mz] = self.opcodes;
        for (index, (batch, readouts)) in batches.iter().zip(readouts).enumerate() {
            if index == 0 && self.breaks(Breaks::ReplacesItsReadouts) {
                *readouts = BatchReadouts::new(batch.shots.len(), batch.readout_slots() + 1);
                continue;
            }
            if index == 0 && self.breaks(Breaks::AddsARowToItsReadouts) {
                *readouts = BatchReadouts::new(batch.shots.len() + 1, batch.readout_slots());
                continue;
            }
            let mut sorted: Vec<ShotId> = batch.shots.iter().collect();
            sorted.sort_unstable();
            for (position, named) in batch.shots.iter().enumerate() {
                let shot = if self.breaks(Breaks::SortsItsRows) {
                    sorted[position]
                } else {
                    named
                };
                let key = self.key(shot, position);
                if let Some(Breaks::RestartsShotsFrom(n)) = self.breaks
                    && key >= ShotId::new(n)
                {
                    shots.remove(&key);
                }
                let held = shots.contains_key(&key);
                self.largest_made = self.largest_made.max(Some(key));
                if held {
                    self.continued.insert(key);
                }
                let by_batch = if self.breaks(Breaks::SeedsNewShotsByBatch) {
                    u64::try_from(index).expect("a batch's place fits in u64")
                } else {
                    0
                };
                let (bits, stream) = shots.entry(key).or_insert_with(|| {
                    (
                        vec![false; QUBITS as usize],
                        SplitMix64::new(seed.shot(shot) ^ by_batch),
                    )
                });
                let row = readouts.row_mut(position);
                let unwritten = (self.breaks(Breaks::LeavesASlotUnwritten)
                    && shot == ShotId::new(1)
                    || self.breaks(Breaks::LeavesSlotsOfShotsAboveThreeUnwritten)
                        && shot > ShotId::new(3))
                .then(|| row.len() - 1);
                let mut next_slot = 0;
                for instruction in batch.instructions {
                    let qubit = instruction.blocks[0] as usize;
                    if instruction.opcode == h {
                        let one_stream = self.breaks(Breaks::SharesItsDraws)
                            || self.draws == Draws::Together
                            || index > 0 && self.breaks(Breaks::SharesItsDrawsBeyondTheFirstBatch);
                        let stream = if one_stream {
                            &mut shared
                        } else {
                            &mut *stream
                        };
                        bits[qubit] = stream.next_u64() & 1 == 1;
                    } else if instruction.opcode == x {
                        bits[qubit] = !bits[qubit];
                    } else if instruction.opcode == mz {
                        if unwritten != Some(next_slot) && next_slot < row_slots {
                            row[next_slot] = (bits[qubit] ^ drift).into();
                        }
                        next_slot += 1;
                    }
                }
                let rejected = self.flags && bits[1]
                    || self.breaks(Breaks::RejectsBatchesOfMany) && batch.shots.len() > 2
                    || self.breaks(Breaks::FlagsContinuedShots) && held;
                if rejected {
                    readouts.set_conditions(position, ShotConditions::REJECTED);
                }
            }
        }
        Ok(())
    }
}

impl ExecutorSetup for FakeExecutor {
    fn accepts(&self) -> &InstructionSet {
        if self.got_ready && self.breaks(Breaks::ChangesWhatItAccepts) {
            &self.wider_set
        } else {
            &self.set
        }
    }

    fn offers(&self) -> Offer {
        let qubits = if self.got_ready && self.breaks(Breaks::ChangesItsOffer) {
            QUBITS
        } else {
            64
        };
        let offer = Offer::unlimited().max_qubits(qubits);
        let offer = if self.draws == Draws::Together {
            offer.not_replayable()
        } else {
            offer
        };
        if self.allowed == Some(Allowed::OffersRunsOfFourShotsAtMost) {
            offer.max_shots(4)
        } else {
            offer
        }
    }
}

impl Execute for FakeExecutor {
    fn ready(&mut self, needs: &Needs) -> impl Future<Output = Result<(), NotReady>> {
        let _ = needs;
        self.asked_ready += 1;
        let busy = self.allowed == Some(Allowed::IsBusyAtRun(self.asked_ready));
        self.got_ready |= !busy;
        async move {
            if busy {
                Err(NotReady::new("busy"))
            } else {
                Ok(())
            }
        }
    }

    fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        let pending = self.pending.clone();
        let done = if pending.replace(false) {
            Some(Ok(()))
        } else if self.breaks(Breaks::WorksWhenCalled) {
            Some(self.run(batches, seed, readouts))
        } else if self.breaks(Breaks::WorksWhenCalledAndReuses) {
            pending.set(true);
            Some(self.run(batches, seed, readouts))
        } else {
            None
        };
        async move {
            pending.set(false);
            match done {
                Some(done) => done,
                None => self.run(batches, seed, readouts),
            }
        }
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        if self.breaks(Breaks::KeepsEndedShots) {
            return;
        }
        let keeps_continued = self.breaks(Breaks::KeepsContinuedShotsPastTheirEnd);
        assert!(
            self.allowed != Some(Allowed::PanicsEndingShots),
            "the shots' release failed"
        );
        let mut held = self.shots.borrow_mut();
        for shot in shots.iter() {
            assert!(
                !self.breaks(Breaks::PanicsEndingShotsItDoesNotHold) || held.contains_key(&shot),
                "shot {shot} is not held"
            );
            assert!(
                !self.breaks(Breaks::PanicsEndingShotsBeyondItsTable)
                    || self.largest_made.is_some_and(|largest| shot <= largest),
                "shot {shot} is beyond its table"
            );
            if keeps_continued && self.continued.contains(&shot) {
                continue;
            }
            if self.breaks(Breaks::KeysShotsByTheirLow16Bits) {
                held.remove(&low_16_bits(shot));
            } else {
                held.remove(&shot);
            }
        }
        self.ended = true;
    }
}

/// `shot`'s id cut to its low 16 bits, as a `u16` would store it.
fn low_16_bits(shot: ShotId) -> ShotId {
    ShotId::new(shot.get() & u32::from(u16::MAX))
}
