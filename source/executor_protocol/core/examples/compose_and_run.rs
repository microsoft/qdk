// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Three toy parts, one of each role, joined with `through` and `on` and
//! run: a driver that tosses a coin and copies it onto a second qubit, an
//! adapter that adds bit-flip noise, and an executor whose qubits are
//! classical bits, all on a small instruction set written here.
//!
//! It prints each shot's two values, runs two of the shots again to show
//! that a shot's values, noise included, are the same each time the chain
//! runs it with the same seed, whatever shots run beside it, since its
//! parts are replayable, runs the shots again with every batch checked to
//! show the checks change no value, and shows a run the chain refuses. The
//! run comes first, in `coin_runner` and `main`; the instruction set and
//! the three parts follow, each in its own section. Run it with
//! `cargo run --example compose_and_run`.

#![expect(
    clippy::unused_async_trait_impl,
    reason = "a part that never waits keeps the trait's `async fn` form, the form the protocol \
              documents; the `core::future::ready` the lint suggests would run the body when \
              the method is called, not when its future is polled"
)]

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::error::Error;
use std::fmt;

use executor_protocol_core::prelude::*;
use executor_protocol_core::{
    Batch, BatchReadouts, Capabilities, DriveError, ExecuteError, Features, NotReady, ReadoutSlots,
    Readouts, Resources, RunRequirement, Runner, Seed, ShotId, ShotSet, StatesLimits,
};

/// The seed of the runs: the same seed gives the same values.
const SEED: u64 = 1;

// ------------------------------------------------------------------ the run

/// The three parts, joined: `coin`, under 20% bit-flip noise, on `bits`
/// ([`Bits`] stating their limits, or a borrow of them, to read what they
/// counted after a run). The coin and the bits state their limits, so each
/// is joined as a [`StatesLimits`] part; the noise states none, and is
/// joined as it is.
///
/// After each run, the protocol ends every shot of the run on every part, so
/// the parts free its state and a rerun with the same seed starts it
/// afresh; the driver need not end them.
fn coin_runner<E: Executor<Accepts = CoinGates>>(
    coin: StatesLimits<CoinCopy>,
    bits: E,
) -> Runner<impl Driver<Sends = CoinGates, Output = Vec<Option<bool>>>, E> {
    // `through` joins the driver and the adapter into one driver, and `on`
    // joins that driver and the executor into the runner. The `on` that
    // makes the runner checks that each part accepts the instructions the
    // part above sends, and that the bits provide what arrives at them: the
    // coin's 2 qubits, passed below by the noise, which states nothing. It
    // gives the runner or every fault of the chain.
    let runner = coin.through(BitFlip::new(0.2)).on(bits);
    runner.expect("the parts compose: the bits provide what the coin requires")
}

fn main() {
    let mut runner = coin_runner(
        CoinCopy::new().stating_limits(),
        Bits::new().stating_limits(),
    );

    let shots = ShotSet::ids(0..12);
    let values = runner.run(&shots, SEED).expect("the chain runs 12 shots");
    println!("shot  coin  copy");
    for (id, shot) in shots.iter().zip(&values) {
        println!("{id:>4}  {:>4}  {:>4}", text(shot[0]), text(shot[1]));
    }
    println!("A copy unlike its coin: the noise flipped it.");

    // The runner ended every shot of the first run, so shots 4 and 5 start
    // afresh, and the seed gives them the same values.
    let again = runner
        .run(&ShotSet::ids([4, 5]), SEED)
        .expect("the chain runs 2 shots");
    let same = again == [values[4].clone(), values[5].clone()];
    println!("\nShots 4 and 5, run again alone: the same values, noise included? {same}");

    // While a part of the chain is developed, the driver may send every
    // batch checked: each boundary checks it before it runs, at a cost.
    let checked = coin_runner(
        CoinCopy::new().checked().stating_limits(),
        Bits::new().stating_limits(),
    )
    .run(&shots, SEED)
    .expect("the chain runs 12 checked shots");
    let same = checked == values;
    println!("\nThe 12 shots again, every batch checked: the same values? {same}");

    let refused = runner
        .run(&ShotSet::ids(0..2000), SEED)
        .expect_err("Bits runs at most 1000 shots");
    println!("\n2000 shots: {refused}");
}

/// A value as the table prints it.
fn text(value: Option<bool>) -> String {
    match value {
        Some(false) => "0".to_owned(),
        Some(true) => "1".to_owned(),
        None => "missing".to_owned(),
    }
}

// ------------------------------------------------------- the instruction set

/// `h`, `x` and `mz` on numbered qubits: what every part here sends and
/// accepts. Its parts all know the one encoding, so any two fit and the
/// binding is `()`; its own capabilities are the qubits a shot holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CoinGates;

/// A gate of [`CoinGates`], on its qubit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gate {
    H(u32),
    X(u32),
    Mz(u32),
}

impl Gate {
    fn qubit(self) -> u32 {
        match self {
            Self::H(qubit) | Self::X(qubit) | Self::Mz(qubit) => qubit,
        }
    }
}

impl BoundaryCompatible for CoinGates {
    type Fit = ();
    type Mismatch = Infallible;

    fn fits(&self, _accepted: &Self) -> Result<(), Infallible> {
        Ok(())
    }
}

impl InstructionSet for CoinGates {
    type Instruction = Gate;
    type SetCapabilities = Qubits;
    /// The qubits a shot holds, read once per checked batch.
    type Requirement = u32;
    type Fault = QubitBeyond;
    /// A bit read, or none.
    type Readout = Option<bool>;
    /// Whether a part found the shot suspect: no part here does.
    type ShotConditions = bool;
    const UNWRITTEN_READOUT: Option<bool> = None;

    /// No qubit: what a part that states nothing of the set requires.
    fn needs_nothing() -> Qubits {
        Qubits(0)
    }

    fn readout_slots(&self, gate: &Gate) -> ReadoutSlots {
        u32::from(matches!(gate, Gate::Mz(_)))
    }

    fn requirement(&self, required: &Qubits) -> u32 {
        required.0
    }

    /// A gate on a qubit the shots hold; any other is refused.
    fn check(&self, gate: &Gate, qubits: &u32) -> Result<ReadoutSlots, QubitBeyond> {
        if gate.qubit() < *qubits {
            Ok(self.readout_slots(gate))
        } else {
            Err(QubitBeyond {
                qubit: gate.qubit(),
                qubits: *qubits,
            })
        }
    }
}

/// The qubits each shot holds: what a part requires, or provides, of
/// [`CoinGates`] beside the protocol's resources and features.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Qubits(u32);

impl BoundaryCompatible for Qubits {
    type Fit = ();
    type Mismatch = TooFewQubits;

    fn fits(&self, provided: &Self) -> Result<(), TooFewQubits> {
        if self.0 <= provided.0 {
            Ok(())
        } else {
            Err(TooFewQubits {
                required: self.0,
                provided: provided.0,
            })
        }
    }
}

/// Fewer qubits provided than required.
#[derive(Debug)]
struct TooFewQubits {
    required: u32,
    provided: u32,
}

impl fmt::Display for TooFewQubits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "qubits: {} required, {} provided",
            self.required, self.provided
        )
    }
}

impl Error for TooFewQubits {}

/// A gate on a qubit beyond those the shots hold.
#[derive(Debug)]
struct QubitBeyond {
    qubit: u32,
    qubits: u32,
}

impl fmt::Display for QubitBeyond {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a gate on qubit {} of {} qubits",
            self.qubit, self.qubits
        )
    }
}

impl Error for QubitBeyond {}

// ---------------------------------------------------------------- the driver

/// A coin tossed on qubit 0, copied onto qubit 1: every shot tosses the
/// coin in a first `execute`; then, by its coin, flips qubit 1 and reads
/// it, or only reads it, as two batches of a second `execute`, the first
/// of whose readouts reuses the memory of the coin's. Each shot's output is
/// its two values: the coin, then the copy. Its batches go unchecked,
/// unless made by [`checked`](Self::checked).
struct CoinCopy {
    /// Toss; heads: flip and read qubit 1; tails: read qubit 1.
    sequences: [Vec<Gate>; 3],
    /// Each sequence's readout slots, which a batch of it states.
    slots: [u32; 3],
    /// Whether every batch it sends is checked.
    checked: bool,
}

impl CoinCopy {
    fn new() -> Self {
        let sequences = [
            vec![Gate::H(0), Gate::Mz(0)],
            vec![Gate::X(1), Gate::Mz(1)],
            vec![Gate::Mz(1)],
        ];
        let slots = sequences.each_ref().map(|sequence| {
            sequence
                .iter()
                .map(|gate| CoinGates.readout_slots(gate))
                .sum()
        });
        Self {
            sequences,
            slots,
            checked: false,
        }
    }

    /// The same coin, sending every batch [`checked`](Batch::checked), as
    /// while a part of its chain is developed.
    fn checked(self) -> Self {
        Self {
            checked: true,
            ..self
        }
    }
}

impl DriverRequirements for CoinCopy {
    /// What the run requires, on 2 qubits.
    fn requires(&self, run: &RunRequirement) -> Capabilities<CoinGates> {
        Capabilities::new(run.resources, run.features, Qubits(2))
    }
}

impl Driver for CoinCopy {
    type Sends = CoinGates;

    fn sends(&self) -> &CoinGates {
        &CoinGates
    }

    /// Every part here encodes the gates alike: nothing to rewrite.
    fn bind(&mut self, (): ()) {}

    type Output = Vec<Option<bool>>;

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = CoinGates>,
    ) -> Result<Vec<Vec<Option<bool>>>, DriveError> {
        // First, every shot tosses its coin. Unchecked by default, as a
        // sender that trusts the chain sends; `.checked()` asks every
        // boundary to check the batch before it runs, at a cost.
        // The protocol shapes each batch's readouts from the batch, a row
        // per shot of a value per readout slot the batch states (one per
        // `mz`), and the executor fills them.
        let toss = [Batch::new(shots, &self.sequences[0], self.slots[0]).checked_if(self.checked)];
        let mut kept = Readouts::new();
        let readouts = kept.readouts_for(&toss);
        below.execute(&toss, seed, readouts).await?;
        let coin = &readouts[0];
        let mut outputs: Vec<Vec<Option<bool>>> =
            (0..shots.len()).map(|i| coin.row(i).to_vec()).collect();

        // Then the heads flip qubit 1 and read it, and the tails only read
        // it: one batch for each side, if some shot took it, since a shot
        // set is never empty.
        let heads = positions_showing(coin, Some(true));
        let tails = positions_showing(coin, Some(false));
        let mut sides = Vec::new();
        for (positions, sequence, slots) in [
            (heads, &self.sequences[1], self.slots[1]),
            (tails, &self.sequences[2], self.slots[2]),
        ] {
            if let Some(side) = shots.subset(&positions) {
                sides.push((positions, side, sequence, slots));
            }
        }
        let batches: Vec<Batch<'_, CoinGates>> = sides
            .iter()
            .map(|(_, shots, sequence, slots)| {
                Batch::new(shots, sequence, *slots).checked_if(self.checked)
            })
            .collect();
        // The same seed: each part keeps a shot's stream from its first
        // batch until the shot ends. The first side's readouts reuse the
        // coin's memory, its values copied into the outputs.
        let readouts = kept.readouts_for(&batches);
        below.execute(&batches, seed, readouts).await?;
        for ((positions, _, _, _), rows) in sides.iter().zip(readouts.iter()) {
            for (j, &i) in positions.iter().enumerate() {
                outputs[i].extend_from_slice(rows.row(j));
            }
        }
        Ok(outputs)
    }
}

/// The positions, among a batch's shots, of those whose coin came out
/// `value`.
fn positions_showing(coin: &BatchReadouts<CoinGates>, value: Option<bool>) -> Vec<usize> {
    (0..coin.shots())
        .filter(|&i| coin.row(i) == [value])
        .collect()
}

// --------------------------------------------------------------- the adapter

/// A toy bit-flip noise: after each `x`, another `x` with probability
/// `probability`, drawn from the adapter's own stream for each shot.
/// Each shot goes below as a batch of its own, since each may flip
/// differently. It states only its instructions: by default it takes
/// whatever arrives, and requires it below.
struct BitFlip {
    probability: f64,
    streams: BTreeMap<ShotId, Stream>,
    /// The readouts of the shots sent below one each, kept to reuse their
    /// memory from one `adapt` to the next.
    one_each: Readouts<CoinGates>,
}

impl BitFlip {
    fn new(probability: f64) -> Self {
        Self {
            probability,
            streams: BTreeMap::new(),
            one_each: Readouts::new(),
        }
    }
}

impl Adapter for BitFlip {
    type Set = CoinGates;

    fn accepts(&self) -> &CoinGates {
        &CoinGates
    }

    fn sends(&self) -> &CoinGates {
        &CoinGates
    }

    fn bind(&mut self, (): ()) {}

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, CoinGates>],
        seed: Seed,
        below: &mut impl Executor<Accepts = CoinGates>,
        readouts: &mut [BatchReadouts<CoinGates>],
    ) -> Result<(), ExecuteError> {
        // Apart from the executor's streams, which draw from `seed` itself.
        let own = seed.derive("bit_flip");
        let mut sent: Vec<(ShotSet, Vec<Gate>, u32, bool)> = Vec::new();
        for batch in batches {
            for (j, id) in batch.shots.iter().enumerate() {
                let stream = self
                    .streams
                    .entry(id)
                    .or_insert_with(|| Stream(own.shot(id)));
                let mut gates = Vec::with_capacity(batch.instructions.len() + 1);
                for &gate in batch.instructions {
                    if matches!(gate, Gate::X(_)) && stream.chance(self.probability) {
                        gates.push(gate);
                    }
                    gates.push(gate);
                }
                let shot = batch.shots.subset(&[j]).expect("a position of the batch");
                sent.push((shot, gates, batch.readout_slots(), batch.is_checked()));
            }
        }
        let one_each: Vec<Batch<'_, CoinGates>> = sent
            .iter()
            .map(|(shots, gates, slots, checked)| {
                // Checked if the batch it came from was, as `Adapter`'s rule
                // says; an `x` adds no readout slot.
                Batch::new(shots, gates, *slots).checked_if(*checked)
            })
            .collect();
        let readouts_below = self.one_each.readouts_for(&one_each);
        below.execute(&one_each, seed, readouts_below).await?;
        // An `x` adds no readout, so each shot's row comes back as wide as
        // in the caller's readouts, which the protocol shaped from the batch
        // received: copied into it, with its conditions, in order.
        let mut ones = readouts_below.iter();
        for batch_readouts in readouts {
            for position in 0..batch_readouts.shots() {
                let one = ones
                    .next()
                    .expect("one `BatchReadouts` per shot sent below");
                batch_readouts.row_mut(position).copy_from_slice(one.row(0));
                batch_readouts.set_conditions(position, one.conditions(0));
            }
        }
        Ok(())
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        for id in shots.iter() {
            self.streams.remove(&id);
        }
    }
}

// -------------------------------------------------------------- the executor

/// A toy executor whose qubits are classical bits: `h` tosses a bit from
/// the shot's stream, `x` flips it, and `mz` reads it. It holds 64 qubits
/// and runs at most 1000 shots in one run. It counts the batches it
/// receives, checked and unchecked, which shows whether a sender's checks
/// reach it.
struct Bits {
    /// 64 qubits, 1000 shots a run.
    provides: Capabilities<CoinGates>,
    /// The qubits each shot requires, from `ready`.
    qubits: usize,
    /// Each running shot's bits and stream, until it ends.
    shots: BTreeMap<ShotId, (Vec<bool>, Stream)>,
    /// The batches it received [`checked`](Batch::checked).
    checked_batches: usize,
    /// The batches it received unchecked.
    unchecked_batches: usize,
}

impl Bits {
    fn new() -> Self {
        Self {
            provides: Capabilities::new(
                Resources::nothing_stated().with_shots(1000),
                Features::nothing_stated(),
                Qubits(64),
            ),
            qubits: 0,
            shots: BTreeMap::new(),
            checked_batches: 0,
            unchecked_batches: 0,
        }
    }
}

impl ExecutorLimits for Bits {
    fn provides(&self) -> Option<&Capabilities<CoinGates>> {
        Some(&self.provides)
    }

    /// Holds the qubits the parts above require.
    async fn ready_for(&mut self, above: &Capabilities<CoinGates>) -> Result<(), NotReady> {
        self.qubits = above.set_capabilities().0 as usize;
        Ok(())
    }
}

impl Executor for Bits {
    type Accepts = CoinGates;

    fn accepts(&self) -> &CoinGates {
        &CoinGates
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_, CoinGates>],
        seed: Seed,
        readouts: &mut [BatchReadouts<CoinGates>],
    ) -> Result<(), ExecuteError> {
        // The protocol shaped each batch's readouts from the batch: a row
        // per shot, of a value per `mz`, in order.
        for (batch, batch_readouts) in batches.iter().zip(readouts) {
            if batch.is_checked() {
                self.checked_batches += 1;
            } else {
                self.unchecked_batches += 1;
            }
            for (position, id) in batch.shots.iter().enumerate() {
                let qubits = self.qubits;
                let (bits, stream) = self
                    .shots
                    .entry(id)
                    .or_insert_with(|| (vec![false; qubits], Stream(seed.shot(id))));
                let mut row = batch_readouts.row_mut(position).iter_mut();
                for &gate in batch.instructions {
                    // Below the shot's qubits, as the parts above send them;
                    // a checked batch's checks refuse any other.
                    let qubit = gate.qubit() as usize;
                    match gate {
                        Gate::H(_) => bits[qubit] = stream.chance(0.5),
                        Gate::X(_) => bits[qubit] = !bits[qubit],
                        Gate::Mz(_) => {
                            *row.next().expect("a readout slot per `mz`") = Some(bits[qubit]);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        for id in shots.iter() {
            self.shots.remove(&id);
        }
    }
}

/// A shot's random stream, `SplitMix64`, seeded from the shot's seed: a
/// few lines, so that the example needs no crate beyond the protocol.
struct Stream(u64);

impl Stream {
    /// `true` with probability `probability`.
    fn chance(&mut self, probability: f64) -> bool {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // The top 52 bits as the fraction of a number in [1, 2), minus 1:
        // uniform in [0, 1).
        f64::from_bits(0x3FF0_0000_0000_0000 | (z >> 12)) - 1.0 < probability
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use executor_protocol_core::{ResourceMismatch, RunError, RunRefusal};

    /// A user who runs a few shots of a run again, with its seed, to look
    /// at them: each gets the values it had in the first run, the noise's
    /// draws included.
    #[test]
    fn a_shot_run_again_alone_with_the_seed_gets_its_values() {
        let mut runner = coin_runner(
            CoinCopy::new().stating_limits(),
            Bits::new().stating_limits(),
        );
        let values = runner
            .run(&ShotSet::ids(0..12), SEED)
            .expect("the chain runs 12 shots");
        assert_ne!(
            values[4][0], values[4][1],
            "seed 1's noise flips shot 4's copy, so the rerun checks the noise's draws"
        );
        let again = runner
            .run(&ShotSet::ids([4, 5]), SEED)
            .expect("the chain runs 2 shots");
        assert_eq!(again, [values[4].clone(), values[5].clone()]);
    }

    /// A user who asks for more shots than the executor runs at once: the
    /// run is refused, naming both counts.
    #[test]
    fn a_run_of_more_shots_than_the_executor_runs_is_refused() {
        let refused = coin_runner(
            CoinCopy::new().stating_limits(),
            Bits::new().stating_limits(),
        )
        .run(&ShotSet::ids(0..2000), SEED);
        let Err(RunError::Refused(RunRefusal::NotProvided(faults))) = refused else {
            panic!("a refused run, not {refused:?}");
        };
        let [fault] = faults.as_slice() else {
            panic!("one fault, not {faults:?}");
        };
        assert_eq!(
            fault.mismatch.capabilities.resources,
            [ResourceMismatch::Shots {
                required: 2000,
                provided: 1000
            }]
        );
    }

    /// A developer who checks the coin's batches while a part of the chain
    /// is developed: the executor receives checked batches only, the noise
    /// forwarding the check to each batch it sends, and each shot gets the
    /// values it gets unchecked, whose batches all reach it unchecked.
    #[test]
    fn checked_batches_reach_the_executor_checked_and_change_no_value() {
        let shots = ShotSet::ids(0..12);
        let run = |coin| {
            let mut bits = Bits::new().stating_limits();
            let values = coin_runner(coin, &mut bits)
                .run(&shots, SEED)
                .expect("the chain runs 12 shots");
            (
                values,
                bits.part.checked_batches,
                bits.part.unchecked_batches,
            )
        };
        let (unchecked, checked_got, unchecked_got) = run(CoinCopy::new().stating_limits());
        assert_eq!(checked_got, 0, "an unchecked run sends no checked batch");
        assert!(unchecked_got > 0, "the executor received the batches");
        let (checked, checked_got, unchecked_got) = run(CoinCopy::new().checked().stating_limits());
        assert_eq!(
            unchecked_got, 0,
            "the noise forwards the check to each batch it sends"
        );
        assert!(checked_got > 0, "the executor received the batches");
        assert_eq!(checked, unchecked);
    }

    /// The example's `main`, as `cargo run` runs it, ends without a panic.
    /// Kept beside the tests above: only it runs the printed path, the
    /// table with `text` and the refusal's message.
    #[test]
    fn main_runs() {
        main();
    }
}
