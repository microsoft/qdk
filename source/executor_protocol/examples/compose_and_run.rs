// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Three toy parts, one of each role, joined with `through` and `on` and
//! run: a driver that tosses a coin and copies it onto a second qubit, an
//! adapter that adds bit-flip noise, and an executor whose qubits are
//! classical bits.
//!
//! It prints each shot's two values, runs two of the shots again to show
//! that a shot's values, noise included, are the same each time the chain
//! runs it with the same seed, whatever shots run beside it, since its
//! parts are replayable, runs the shots again with every batch checked to
//! show the checks change no value, and shows a run the chain refuses. The
//! run comes first, in `coin_runner` and `main`; the three parts follow,
//! each in its own section. Run it with
//! `cargo run --example compose_and_run`.

#![expect(
    clippy::unused_async_trait_impl,
    reason = "a part that never waits keeps the trait's `async fn` form, the form the protocol \
              documents; the `core::future::ready` the lint suggests would run the body when \
              the method is called, not when its future is polled"
)]

use std::collections::BTreeMap;

use executor_protocol::prelude::*;
use executor_protocol::{
    Batch, BatchReadouts, BlockType, Capabilities, DriveError, ExecuteError, Features, Instruction,
    InstructionSet, NotReady, OpcodeMap, ReadoutValue, Readouts, Resources, Runner, Seed, ShotId,
    ShotSet, instruction_set,
};

instruction_set! {
    /// QIR's `h`, `x` and `mz`: what every part here sends and accepts.
    pub CoinGates { H = h(qubit), X = x(qubit), MZ = mz(qubit) -> 1 outcome }
}

/// The seed of the runs: the same seed gives the same values.
const SEED: u64 = 1;

// ------------------------------------------------------------------ the run

/// The three parts, joined: `coin`, under 20% bit-flip noise, on `bits`
/// (a [`Bits`], or a borrow of one, to read what it counted after a run).
///
/// After each run, the protocol ends every shot of the run on every part, so
/// the parts free its state and a rerun with the same seed starts it
/// afresh; the driver need not end them.
fn coin_runner<E: Executor>(
    coin: CoinCopy,
    bits: E,
) -> Runner<impl Driver<Output = Vec<ReadoutValue>>, E> {
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
    let mut runner = coin_runner(CoinCopy::new(), Bits::new());

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
    let checked = coin_runner(CoinCopy::new().checked(), Bits::new())
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
fn text(value: ReadoutValue) -> String {
    match value {
        ReadoutValue::Zero => "0".to_owned(),
        ReadoutValue::One => "1".to_owned(),
        ReadoutValue::Missing(_) => "missing".to_owned(),
        // `ReadoutValue` may gain variants: shown as they are.
        other => format!("{other:?}"),
    }
}

// ---------------------------------------------------------------- the driver

/// A coin tossed on qubit 0, copied onto qubit 1: every shot tosses the
/// coin in a first `execute`; then, by its coin, flips qubit 1 and reads
/// it, or only reads it, as two batches of a second `execute`, the first
/// of whose readouts reuses the memory of the coin's. Each shot's output is
/// its two values: the coin, then the copy. Its batches go unchecked,
/// unless made by [`checked`](Self::checked).
struct CoinCopy {
    /// `CoinGates`, the instructions it sends.
    sends: InstructionSet,
    /// Toss; heads: flip and read qubit 1; tails: read qubit 1.
    sequences: [Vec<Instruction>; 3],
    /// Each sequence's readout slots, which a batch of it states.
    slots: [u32; 3],
    /// Whether every batch it sends is checked.
    checked: bool,
}

impl CoinCopy {
    fn new() -> Self {
        let set = CoinGates::set();
        let gate = |mnemonic: &str, qubit: u32| {
            set.instruction(mnemonic, [qubit], &[])
                .expect("an instruction of CoinGates")
        };
        let sequences = [
            vec![gate("h", 0), gate("mz", 0)],
            vec![gate("x", 1), gate("mz", 1)],
            vec![gate("mz", 1)],
        ];
        let slots = sequences.each_ref().map(|sequence| {
            set.readout_slots_of(sequence)
                .expect("instructions of CoinGates")
        });
        Self {
            sends: set,
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

impl Driver for CoinCopy {
    fn sends(&self) -> &InstructionSet {
        &self.sends
    }

    /// What the run requires, on 2 qubits.
    fn requires(&self, run: &Capabilities) -> Capabilities {
        run.clone()
            .map_resources(|resources| resources.with_qubits(2))
    }

    /// Rewrites the sequences in the receiver's opcodes, once.
    fn bind(&mut self, below: OpcodeMap) {
        for instruction in self.sequences.iter_mut().flatten() {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    type Output = Vec<ReadoutValue>;

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor,
    ) -> Result<Vec<Vec<ReadoutValue>>, DriveError> {
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
        let mut outputs: Vec<Vec<ReadoutValue>> =
            (0..shots.len()).map(|i| coin.row(i).to_vec()).collect();

        // Then the heads flip qubit 1 and read it, and the tails only read
        // it: one batch for each side, if some shot took it, since a shot
        // set is never empty.
        let heads = positions_showing(coin, ReadoutValue::One);
        let tails = positions_showing(coin, ReadoutValue::Zero);
        let mut sides = Vec::new();
        for (positions, sequence, slots) in [
            (heads, &self.sequences[1], self.slots[1]),
            (tails, &self.sequences[2], self.slots[2]),
        ] {
            if let Some(side) = shots.select(&positions) {
                sides.push((positions, side, sequence, slots));
            }
        }
        let batches: Vec<Batch<'_>> = sides
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
fn positions_showing(coin: &BatchReadouts, value: ReadoutValue) -> Vec<usize> {
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
    /// `CoinGates`, the instructions it accepts and sends.
    set: InstructionSet,
    probability: f64,
    /// The receiver's opcodes, from `bind`.
    below: Option<OpcodeMap>,
    streams: BTreeMap<ShotId, Stream>,
    /// The readouts of the shots sent below one each, kept to reuse their
    /// memory from one `adapt` to the next.
    one_each: Readouts,
}

impl BitFlip {
    fn new(probability: f64) -> Self {
        Self {
            set: CoinGates::set(),
            probability,
            below: None,
            streams: BTreeMap::new(),
            one_each: Readouts::new(),
        }
    }
}

impl Adapter for BitFlip {
    fn accepts(&self) -> &InstructionSet {
        &self.set
    }

    fn sends(&self) -> &InstructionSet {
        &self.set
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let map = self.below.as_ref().expect("the join binds before any run");
        // Apart from the executor's streams, which draw from `seed` itself.
        let own = seed.derive("bit_flip");
        let mut sent: Vec<(ShotSet, Vec<Instruction>, u32, bool)> = Vec::new();
        for batch in batches {
            for (j, id) in batch.shots.iter().enumerate() {
                let stream = self
                    .streams
                    .entry(id)
                    .or_insert_with(|| Stream(own.shot(id)));
                let mut instructions = Vec::with_capacity(batch.instructions.len() + 1);
                for instruction in batch.instructions {
                    // It came in this adapter's opcodes: the sender's
                    // `bind` mapped it.
                    let flips =
                        instruction.opcode == CoinGates::X && stream.chance(self.probability);
                    let mut mapped = instruction.clone();
                    mapped.opcode = map.map(instruction.opcode);
                    if flips {
                        instructions.push(mapped.clone());
                    }
                    instructions.push(mapped);
                }
                let shot = batch.shots.select(&[j]).expect("a position of the batch");
                sent.push((
                    shot,
                    instructions,
                    batch.readout_slots(),
                    batch.is_checked(),
                ));
            }
        }
        let one_each: Vec<Batch<'_>> = sent
            .iter()
            .map(|(shots, instructions, slots, checked)| {
                // Checked if the batch it came from was, as `Adapter`'s rule
                // says; an `x` adds no readout slot.
                Batch::new(shots, instructions, *slots).checked_if(*checked)
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
/// the shot's stream, `x` flips it, and `mz` reads it. It runs at most
/// 1000 shots in one run, and resolves no readout parity (an argument
/// that is the parity of earlier readouts, `Bit::Parity`): the coin's
/// instructions take none. It counts the batches it receives, checked and
/// unchecked, which shows whether a sender's checks reach it.
struct Bits {
    /// `CoinGates`, the instructions it runs.
    accepts: InstructionSet,
    /// 64 qubits, 1000 shots a run, no readout parity.
    provides: Capabilities,
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
        let resources = Resources::unconstrained().with_qubits(64).with_shots(1000);
        let features = Features::unconstrained().with_readout_parity(false);
        Self {
            accepts: CoinGates::set(),
            provides: Capabilities::new(resources, features),
            qubits: 0,
            shots: BTreeMap::new(),
            checked_batches: 0,
            unchecked_batches: 0,
        }
    }
}

impl Executor for Bits {
    fn accepts(&self) -> &InstructionSet {
        &self.accepts
    }

    fn provides(&self) -> &Capabilities {
        &self.provides
    }

    /// Holds the qubits the parts above require, or, if none states them,
    /// every qubit it provides.
    async fn ready(&mut self, above: &Capabilities) -> Result<(), NotReady> {
        let qubit = BlockType::qubit();
        let stated = above.resources().blocks(&qubit);
        self.qubits = stated
            .or(self.provides.resources().blocks(&qubit))
            .unwrap_or(0) as usize;
        Ok(())
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
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
                for instruction in batch.instructions {
                    // Below the shot's qubits, as the parts above send them;
                    // a checked batch's checks refuse any other.
                    let qubit = instruction.blocks[0] as usize;
                    match instruction.opcode {
                        CoinGates::H => bits[qubit] = stream.chance(0.5),
                        CoinGates::X => bits[qubit] = !bits[qubit],
                        CoinGates::MZ => {
                            *row.next().expect("a readout slot per `mz`") = bits[qubit].into();
                        }
                        _ => unreachable!("the parts above send only instructions of the set"),
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
    use executor_protocol::{Resource, ResourceMismatch, RunError, RunRefusal};

    /// A user who runs a few shots of a run again, with its seed, to look
    /// at them: each gets the values it had in the first run, the noise's
    /// draws included.
    #[test]
    fn a_shot_run_again_alone_with_the_seed_gets_its_values() {
        let mut runner = coin_runner(CoinCopy::new(), Bits::new());
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
        let refused = coin_runner(CoinCopy::new(), Bits::new()).run(&ShotSet::ids(0..2000), SEED);
        let Err(RunError::Refused(RunRefusal::NotProvided(faults))) = refused else {
            panic!("a refused run, not {refused:?}");
        };
        let [fault] = faults.as_slice() else {
            panic!("one fault, not {faults:?}");
        };
        assert_eq!(
            fault.mismatch.capabilities.resources,
            [ResourceMismatch {
                key: Resource::Shots,
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
            let mut bits = Bits::new();
            let values = coin_runner(coin, &mut bits)
                .run(&shots, SEED)
                .expect("the chain runs 12 shots");
            (values, bits.checked_batches, bits.unchecked_batches)
        };
        let (unchecked, checked_got, unchecked_got) = run(CoinCopy::new());
        assert_eq!(checked_got, 0, "an unchecked run sends no checked batch");
        assert!(unchecked_got > 0, "the executor received the batches");
        let (checked, checked_got, unchecked_got) = run(CoinCopy::new().checked());
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
