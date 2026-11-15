// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Three toy parts, one of each role, composed with `>>` and run: a driver
//! that tosses a coin and copies it onto a second qubit, an adapter that
//! adds bit-flip noise, and an executor whose qubits are classical bits.
//!
//! It prints each shot's two values, runs two of the shots again to show
//! that a shot's values, noise included, are the same each time the chain
//! runs it with the same seed, whatever shots run beside it, runs the
//! shots again with every batch checked to show the checks change no
//! value, and shows a run the chain refuses. The run comes first, in
//! `coin_runner` and `main`; the three parts follow, each in its own
//! section. Run it with `cargo run --example compose_and_run`.

use std::collections::BTreeMap;

use executor_protocol::{
    Adapt, Adapter, Batch, BatchReadouts, BlockType, Capabilities, Composed, Drive, DriveError,
    Driver, Execute, ExecuteError, Executor, Instruction, InstructionSet, Needs, OpcodeMap,
    ReadoutValue, Readouts, Runner, Seed, ShotConditions, ShotId, ShotSet, instruction_set,
};

instruction_set! {
    /// QIR's `h`, `x` and `mz`: what every part here sends and accepts.
    pub CoinGates { H = h(qubit), X = x(qubit), MZ = mz(qubit) -> 1 outcome }
}

/// The seed of the runs: the same seed gives the same values.
const SEED: u64 = 1;

// ------------------------------------------------------------------ the run

/// The three parts, composed: `coin`, under 20% bit-flip noise, on bits.
///
/// After each run, the runner ends every shot of the run on every part, so
/// the parts free its state and a rerun with the same seed starts it
/// afresh; the driver need not end them.
fn coin_runner(coin: CoinCopy) -> Runner<Composed<CoinCopy, BitFlip>, Bits> {
    // Each part is wrapped in the role it plays (`Driver`, `Adapter`,
    // `Executor`), which `>>` needs to pick the join. `>>` joins the driver
    // and the adapter into one driver, a `Composed`. Each `>>` checks that
    // what is sent across it fits what is accepted; the last one, onto the
    // executor, also checks that the executor offers what the driver
    // needs, and gives the runner or every fault of the chain.
    let runner = Driver::new(coin) >> Adapter::new(BitFlip::new(200)) >> Executor::new(Bits::new());
    runner.expect("the parts compose: they all send and accept CoinGates")
}

fn main() {
    let mut runner = coin_runner(CoinCopy::new());

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
    let checked = coin_runner(CoinCopy::new().checked())
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
/// it, or only reads it, as two batches of a second `execute`, whose
/// readouts come after the coin's in the buffer the driver lends. Each
/// shot's output is its two values: the coin, then the copy. Its batches
/// go unchecked, unless made by [`checked`](Self::checked).
struct CoinCopy {
    set: InstructionSet,
    /// Toss; heads: flip and read qubit 1; tails: read qubit 1.
    sequences: [Vec<Instruction>; 3],
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
        Self {
            set,
            sequences,
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
        // First, every shot tosses its coin. Unchecked by default, as a
        // sender that trusts the chain sends; `.checked()` asks every
        // boundary to check the batch before it runs, at a cost.
        let toss = Batch::new(shots, &self.sequences[0]).checked_if(self.checked);
        let mut readouts = Readouts::new();
        below.execute(&[toss], seed, &mut readouts).await?;
        let coin = &readouts[0];
        let mut outputs: Vec<Vec<ReadoutValue>> =
            (0..shots.len()).map(|i| coin.row(i).to_vec()).collect();

        // Then the heads flip qubit 1 and read it, and the tails only read
        // it: one batch for each side, if some shot took it, since a shot
        // set is never empty.
        let heads = positions_showing(coin, ReadoutValue::One);
        let tails = positions_showing(coin, ReadoutValue::Zero);
        let mut sides = Vec::new();
        for (positions, sequence) in [(heads, &self.sequences[1]), (tails, &self.sequences[2])] {
            if let Some(side) = shots.select(&positions) {
                sides.push((positions, side, sequence));
            }
        }
        let batches: Vec<Batch<'_>> = sides
            .iter()
            .map(|(_, shots, sequence)| Batch::new(shots, sequence).checked_if(self.checked))
            .collect();
        // The same seed: each part keeps a shot's stream from its first
        // batch until the shot ends. The sides' readouts are appended
        // after the coin's.
        below.execute(&batches, seed, &mut readouts).await?;
        for ((positions, _, _), rows) in sides.iter().zip(&readouts[1..]) {
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
/// `per_mille` / 1000, drawn from the adapter's own stream for each shot.
/// Each shot goes below as a batch of its own, since each may flip
/// differently.
struct BitFlip {
    set: InstructionSet,
    per_mille: u64,
    /// The receiver's opcodes, from `bind`.
    below: Option<OpcodeMap>,
    streams: BTreeMap<ShotId, Stream>,
    /// The readouts of the shots sent below one each, kept to reuse their
    /// memory from one `adapt` to the next.
    one_each: Readouts,
}

impl BitFlip {
    fn new(per_mille: u64) -> Self {
        Self {
            set: CoinGates::set(),
            per_mille,
            below: None,
            streams: BTreeMap::new(),
            one_each: Readouts::new(),
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
        readouts: &mut Readouts,
    ) -> Result<(), ExecuteError> {
        let map = self.below.as_ref().expect("`>>` binds before any run");
        // Apart from the executor's streams, which draw from `seed` itself.
        let own = seed.derive("bit_flip");
        let mut sent: Vec<(ShotSet, Vec<Instruction>, bool)> = Vec::new();
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
                    let flips = instruction.opcode == CoinGates::X && stream.chance(self.per_mille);
                    let mut mapped = instruction.clone();
                    mapped.opcode = map.map(instruction.opcode);
                    if flips {
                        instructions.push(mapped.clone());
                    }
                    instructions.push(mapped);
                }
                let shot = batch.shots.select(&[j]).expect("a position of the batch");
                sent.push((shot, instructions, batch.is_checked()));
            }
        }
        let one_each: Vec<Batch<'_>> = sent
            .iter()
            .map(|(shots, instructions, checked)| {
                // Checked if the batch it came from was, as `Adapt`'s
                // guideline says.
                Batch::new(shots, instructions).checked_if(*checked)
            })
            .collect();
        self.one_each.clear();
        below.execute(&one_each, seed, &mut self.one_each).await?;
        // An `x` adds no readout, so each shot's row comes back as it is,
        // joined again into one readouts per batch received, appended to
        // the caller's. The protocol has checked that `one_each` holds one
        // readouts per batch sent, and every batch holds a shot, so
        // `mine[0]` exists.
        let mut rows = self.one_each.iter();
        for batch in batches {
            let mine: Vec<&BatchReadouts> = rows.by_ref().take(batch.shots.len()).collect();
            let joined = readouts.push_with_capacity(mine.len(), mine[0].readouts());
            for one in mine {
                joined
                    .push_row(one.row(0), one.conditions(0))
                    .expect("a row of the batch's width");
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
/// instructions take none.
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
        readouts: &mut Readouts,
    ) -> Result<(), ExecuteError> {
        for batch in batches {
            let reads = batch
                .instructions
                .iter()
                .filter(|instruction| instruction.opcode == CoinGates::MZ)
                .count();
            let width = u32::try_from(reads).expect("a short batch");
            let rows = readouts.push_with_capacity(batch.shots.len(), width);
            for id in batch.shots.iter() {
                let qubits = self.qubits;
                let (bits, stream) = self
                    .shots
                    .entry(id)
                    .or_insert_with(|| (vec![false; qubits], Stream(seed.shot(id))));
                let mut row = Vec::with_capacity(reads);
                for instruction in batch.instructions {
                    // Below the shot's qubits, as the parts above send them;
                    // a checked batch's checks refuse any other.
                    let qubit = instruction.blocks[0] as usize;
                    match instruction.opcode {
                        CoinGates::H => bits[qubit] = stream.chance(500),
                        CoinGates::X => bits[qubit] = !bits[qubit],
                        CoinGates::MZ => row.push(bits[qubit].into()),
                        _ => unreachable!("the parts above send only instructions of the set"),
                    }
                }
                rows.push_row(&row, ShotConditions::NONE)
                    .expect("one value per `mz`");
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
    /// `true` with probability `per_mille` / 1000.
    fn chance(&mut self, per_mille: u64) -> bool {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) % 1000 < per_mille
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use executor_protocol::{RunError, RunRefusal};

    /// A user who runs a few shots of a run again, with its seed, to look
    /// at them: each gets the values it had in the first run, the noise's
    /// draws included.
    #[test]
    fn a_shot_run_again_alone_with_the_seed_gets_its_values() {
        let mut runner = coin_runner(CoinCopy::new());
        let values = runner
            .run(&ShotSet::ids(0..12), SEED)
            .expect("the chain runs 12 shots");
        for shot in [4, 5] {
            assert_ne!(
                values[shot][0], values[shot][1],
                "seed 1's noise flips shot {shot}'s copy, so the rerun checks the noise's draws"
            );
        }
        let again = runner
            .run(&ShotSet::ids([4, 5]), SEED)
            .expect("the chain runs 2 shots");
        assert_eq!(again, [values[4].clone(), values[5].clone()]);
    }

    /// A user who asks for more shots than the executor runs at once: the
    /// run is refused, naming both counts.
    #[test]
    fn a_run_of_more_shots_than_the_executor_runs_is_refused() {
        let refused = coin_runner(CoinCopy::new()).run(&ShotSet::ids(0..2000), SEED);
        assert!(matches!(
            refused,
            Err(RunError::Refused(RunRefusal::TooManyShots {
                asked: 2000,
                most: 1000
            }))
        ));
    }

    /// A developer who checks the coin's batches while a part of the chain
    /// is developed: every boundary checks them, the noise forwarding the
    /// check to each batch it sends, and each shot gets the values it gets
    /// unchecked.
    #[test]
    fn a_run_of_checked_batches_gives_the_values_of_an_unchecked_one() {
        let shots = ShotSet::ids(0..12);
        let unchecked = coin_runner(CoinCopy::new())
            .run(&shots, SEED)
            .expect("the chain runs 12 shots");
        let checked = coin_runner(CoinCopy::new().checked())
            .run(&shots, SEED)
            .expect("the chain runs 12 checked shots");
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
