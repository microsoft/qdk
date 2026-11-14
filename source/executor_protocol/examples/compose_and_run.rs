// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Three toy parts, one of each role, composed with `>>` and run: a driver
//! that tosses a coin and copies it onto a second qubit, an adapter that
//! adds bit-flip noise, and an executor whose qubits are classical bits.
//!
//! It prints each shot's two values, runs two of the shots again to show
//! that a shot's values, noise included, depend only on the seed and its
//! id, and shows a run the chain refuses. Run it with
//! `cargo run --example compose_and_run`.

use std::collections::BTreeMap;

use executor_protocol::{
    Adapt, Adapter, Batch, BatchReadouts, BlockType, Capabilities, Composed, Drive, DriveError,
    Driver, Execute, ExecuteError, Executor, Instruction, InstructionSet, Needs, OpcodeMap,
    ReadoutValue, RunError, Runner, Seed, ShotConditions, ShotId, ShotSet, instruction_set,
};

instruction_set! {
    /// QIR's `h`, `x` and `mz`: what every part here sends and accepts.
    pub CoinGates { H = h(qubit), X = x(qubit), MZ = mz(qubit) -> 1 outcome }
}

/// The seed of the runs: the same seed gives the same values.
const SEED: u64 = 1;

// ---------------------------------------------------------------- the driver

/// A coin tossed on qubit 0, copied onto qubit 1: every shot tosses the
/// coin in a first `execute`; then, by its coin, flips qubit 1 and reads
/// it, or only reads it, as two batches of a second `execute`. Each shot's
/// output is its two values: the coin, then the copy.
struct CoinCopy {
    set: InstructionSet,
    /// Toss; heads: flip and read qubit 1; tails: read qubit 1.
    sequences: [Vec<Instruction>; 3],
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
        Self { set, sequences }
    }
}

impl Drive for CoinCopy {
    type Output = Vec<ReadoutValue>;

    fn sends(&self) -> &InstructionSet {
        &self.set
    }

    /// Rewrites the sequences in the receiver's opcodes, once.
    fn bind(&mut self, below: OpcodeMap) {
        for instruction in self.sequences.iter_mut().flatten() {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    fn needs(&self) -> Needs {
        Needs::qubits(2)
    }

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<Vec<ReadoutValue>>, DriveError> {
        let toss = Batch {
            shots,
            instructions: &self.sequences[0],
        };
        let tossed = below.execute(&[toss], seed).await?;
        let coin = &tossed[0];
        let mut outputs: Vec<Vec<ReadoutValue>> =
            (0..shots.len()).map(|i| coin.row(i).to_vec()).collect();

        let at = |value: ReadoutValue| -> Vec<usize> {
            (0..shots.len())
                .filter(|&i| coin.row(i) == [value])
                .collect()
        };
        let (heads, tails) = (at(ReadoutValue::One), at(ReadoutValue::Zero));
        // A side no shot took has no batch: a shot set is never empty.
        let sides: Vec<(&[usize], ShotSet, &[Instruction])> =
            [(&heads, &self.sequences[1]), (&tails, &self.sequences[2])]
                .into_iter()
                .filter_map(|(positions, sequence)| {
                    Some((
                        positions.as_slice(),
                        shots.select(positions)?,
                        &sequence[..],
                    ))
                })
                .collect();
        let batches: Vec<Batch<'_>> = sides
            .iter()
            .map(|(_, shots, instructions)| Batch {
                shots,
                instructions,
            })
            .collect();
        let read = below.execute(&batches, seed).await?;
        for ((positions, _, _), rows) in sides.iter().zip(&read) {
            for (j, &i) in positions.iter().enumerate() {
                outputs[i].extend_from_slice(rows.row(j));
            }
        }
        // Every shot is done: the parts below free its state.
        below.end_shots(shots);
        Ok(outputs)
    }
}

// --------------------------------------------------------------- the adapter

/// A toy bit-flip noise: after each `x`, another `x` with probability
/// `per_mille` / 1000, drawn from the adapter's own stream for each shot.
/// Each shot goes below as a batch of its own, since each may flip
/// differently.
struct BitFlip {
    set: InstructionSet,
    per_mille: u64,
    /// The receiver's opcodes, from `bind`.
    below: Option<OpcodeMap>,
    streams: BTreeMap<ShotId, Stream>,
}

impl BitFlip {
    fn new(per_mille: u64) -> Self {
        Self {
            set: CoinGates::set(),
            per_mille,
            below: None,
            streams: BTreeMap::new(),
        }
    }
}

impl Adapt for BitFlip {
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
        below: &mut impl Execute,
    ) -> Result<Vec<BatchReadouts>, ExecuteError> {
        let map = self.below.as_ref().expect("`>>` binds before any run");
        // Apart from the executor's streams, which draw from `seed` itself.
        let own = seed.derive("bit_flip");
        let mut sent: Vec<(ShotSet, Vec<Instruction>)> = Vec::new();
        for batch in batches {
            for (j, id) in batch.shots.iter().enumerate() {
                let stream = self
                    .streams
                    .entry(id)
                    .or_insert_with(|| Stream(own.shot(id)));
                let mut instructions = Vec::with_capacity(batch.instructions.len() + 1);
                for instruction in batch.instructions {
                    // It came in this adapter's opcodes: the sender's `bind` mapped it.
                    let flips = instruction.opcode == CoinGates::X && stream.chance(self.per_mille);
                    let mut mapped = instruction.clone();
                    mapped.opcode = map.map(instruction.opcode);
                    if flips {
                        instructions.push(mapped.clone());
                    }
                    instructions.push(mapped);
                }
                let shot = batch.shots.select(&[j]).expect("a position of the batch");
                sent.push((shot, instructions));
            }
        }
        let one_each: Vec<Batch<'_>> = sent
            .iter()
            .map(|(shots, instructions)| Batch {
                shots,
                instructions,
            })
            .collect();
        let answer = below.execute(&one_each, seed).await?;
        // An `x` adds no readout, so each shot's row comes back as it is,
        // joined again into one readouts per batch received.
        let mut rows = answer.iter();
        let mut readouts = Vec::with_capacity(batches.len());
        for batch in batches {
            let mine: Vec<&BatchReadouts> = rows.by_ref().take(batch.shots.len()).collect();
            let mut joined = BatchReadouts::with_capacity(mine.len(), mine[0].readouts());
            for one in mine {
                joined
                    .push_row(one.row(0), one.conditions(0))
                    .expect("a row of the batch's width");
            }
            readouts.push(joined);
        }
        Ok(readouts)
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
/// 1000 shots at once.
struct Bits {
    set: InstructionSet,
    /// The qubits each shot needs, from `provision`.
    qubits: usize,
    /// Each running shot's bits and stream, until it ends.
    shots: BTreeMap<ShotId, (Vec<bool>, Stream)>,
}

impl Bits {
    fn new() -> Self {
        Self {
            set: CoinGates::set(),
            qubits: 0,
            shots: BTreeMap::new(),
        }
    }
}

impl Execute for Bits {
    fn accepts(&self) -> &InstructionSet {
        &self.set
    }

    fn offers(&self) -> Capabilities {
        Capabilities::unlimited()
            .max_qubits(64)
            .max_shots(1000)
            .without_readout_parity()
    }

    fn provision(&mut self, needs: Needs) {
        self.qubits = needs.blocks_of(&BlockType::qubit()) as usize;
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
    ) -> Result<Vec<BatchReadouts>, ExecuteError> {
        let mut answer = Vec::with_capacity(batches.len());
        for batch in batches {
            let reads = batch
                .instructions
                .iter()
                .filter(|instruction| instruction.opcode == CoinGates::MZ)
                .count();
            let width = u32::try_from(reads).expect("a short batch");
            let mut rows = BatchReadouts::with_capacity(batch.shots.len(), width);
            for id in batch.shots.iter() {
                let qubits = self.qubits;
                let (bits, stream) = self
                    .shots
                    .entry(id)
                    .or_insert_with(|| (vec![false; qubits], Stream(seed.shot(id))));
                let mut row = Vec::with_capacity(reads);
                for instruction in batch.instructions {
                    // Below the shot's qubits: the protocol checked it.
                    let qubit = instruction.blocks[0] as usize;
                    match instruction.opcode {
                        CoinGates::H => bits[qubit] = stream.chance(500),
                        CoinGates::X => bits[qubit] = !bits[qubit],
                        CoinGates::MZ => row.push(if bits[qubit] {
                            ReadoutValue::One
                        } else {
                            ReadoutValue::Zero
                        }),
                        _ => unreachable!("the protocol checked: an instruction of the set"),
                    }
                }
                rows.push_row(&row, ShotConditions::NONE)
                    .expect("one value per `mz`");
            }
            answer.push(rows);
        }
        Ok(answer)
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
    /// `true` with probability `per_mille` / 1000.
    fn chance(&mut self, per_mille: u64) -> bool {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) % 1000 < per_mille
    }
}

// ------------------------------------------------------------------ the run

/// The three parts, composed: the coin, under 20% bit-flip noise, on bits.
fn coin_runner() -> Runner<Composed<CoinCopy, BitFlip>, Bits> {
    let chain = Driver::new(CoinCopy::new())
        >> Adapter::new(BitFlip::new(200))
        >> Executor::new(Bits::new());
    chain.expect("the parts compose: they all send and accept CoinGates")
}

fn main() {
    let mut runner = coin_runner();

    let shots = ShotSet::ids(0..12);
    let values = runner.run(&shots, SEED).expect("the chain runs 12 shots");
    println!("shot  coin  copy");
    for (id, shot) in shots.iter().zip(&values) {
        println!(
            "{:>4}  {:>4}  {:>4}",
            id.get(),
            text(shot[0]),
            text(shot[1])
        );
    }
    println!("A copy unlike its coin: the noise flipped it.");

    let again = runner
        .run(&ShotSet::ids([4, 5]), SEED)
        .expect("the chain runs 2 shots");
    let same = again == [values[4].clone(), values[5].clone()];
    println!("\nShots 4 and 5, run again alone: the same values, noise included? {same}");

    let Err(RunError::Refused(refusal)) = runner.run(&ShotSet::ids(0..2000), SEED) else {
        unreachable!("Bits runs at most 1000 shots");
    };
    println!("\n2000 shots: refused: {refusal}");
}

/// A value as the table prints it.
fn text(value: ReadoutValue) -> &'static str {
    match value {
        ReadoutValue::Zero => "0",
        ReadoutValue::One => "1",
        _ => "missing",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use executor_protocol::RunRefusal;

    /// A user who runs a few shots of a run again, with its seed, to look
    /// at them: each gets the values it had in the first run, the noise's
    /// draws included.
    #[test]
    fn a_shot_run_again_alone_with_the_seed_gets_its_values() {
        let mut runner = coin_runner();
        let values = runner
            .run(&ShotSet::ids(0..12), SEED)
            .expect("the chain runs 12 shots");
        let again = runner
            .run(&ShotSet::ids([4, 5]), SEED)
            .expect("the chain runs 2 shots");
        assert_eq!(again, [values[4].clone(), values[5].clone()]);
    }

    /// A user who asks for more shots than the executor runs at once: the
    /// run is refused, naming both counts.
    #[test]
    fn a_run_of_more_shots_than_the_executor_runs_is_refused() {
        let refused = coin_runner().run(&ShotSet::ids(0..2000), SEED);
        assert!(matches!(
            refused,
            Err(RunError::Refused(RunRefusal::TooManyShots {
                asked: 2000,
                most: 1000
            }))
        ));
    }

    /// The example's `main`, as `cargo run` runs it, ends without a panic.
    #[test]
    fn main_runs() {
        main();
    }
}
