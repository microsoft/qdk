// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The mix of gates `chains`, `checks` and `worker` send, and its programs, drawn from a fixed
//! seed and built by name.

use executor_protocol::{Argument, Instruction, InstructionSet, Opcode};

use crate::parts::MixGates;

/// The mix `chains`, `checks` and `worker` draw.
#[derive(Clone, Copy)]
pub(crate) enum Mix {
    /// The gates of QIR programs, with no argument but `rz`'s angle.
    Common,
    /// The common mix and, for one instruction in five, an `x_if` on the
    /// parity of two of the shot's readouts, as a code's rounds send when
    /// they correct their data qubits by feed-forward. A repetition code's
    /// round on `d` data qubits sends about `4d` instructions (each of
    /// `d - 1` syndrome qubits reset, two `cx`, read) and up to `d`
    /// corrections, each the parity of the two syndromes beside a data
    /// qubit.
    ParityHeavy,
}

impl Mix {
    /// The weight in this mix of `MIX`'s gate at `place`.
    fn weight(self, place: usize) -> u64 {
        let (opcode, _, weight) = MIX[place];
        match self {
            Mix::Common if opcode == MixGates::X_IF => 0,
            _ => weight,
        }
    }
}

/// The gates of the mixes: each gate of `MixGates`, its qubits and its
/// weight in the parity-heavy mix. Each gate but `x_if` weighs as in QIR
/// programs (two-qubit gates and single-qubit rotations most); `x_if`
/// makes one instruction in five, and none in the common mix. A gate's
/// place here is its place in a [`Profile`](crate::parts::Profile)'s counts.
pub(crate) const MIX: [(Opcode, usize, u64); 12] = [
    (MixGates::CX, 2, 30),
    (MixGates::H, 1, 15),
    (MixGates::RZ, 1, 15),
    (MixGates::T, 1, 10),
    (MixGates::T_ADJ, 1, 5),
    (MixGates::S, 1, 5),
    (MixGates::X, 1, 5),
    (MixGates::CZ, 2, 5),
    (MixGates::CCX, 3, 3),
    (MixGates::RESET, 1, 2),
    (MixGates::SWAP, 2, 5),
    (MixGates::X_IF, 1, 25),
];

/// The qubits the mix draws from.
pub(crate) const QUBITS: u32 = 100;

/// The seed of the mix's draws, and of every run, whose executors draw
/// nothing.
pub(crate) const SEED: u64 = 1;

/// `n` draws of `mix`, from a fixed seed.
pub(crate) fn draws(n: usize, mix: Mix) -> Vec<Draw> {
    let total: u64 = (0..MIX.len()).map(|place| mix.weight(place)).sum();
    let mut random = RandomGenerator(SEED);
    (0..n)
        .map(|_| {
            let mut pick = random.next() % total;
            let gate = (0..MIX.len())
                .position(|place| {
                    let weight = mix.weight(place);
                    let here = pick < weight;
                    pick = pick.wrapping_sub(weight);
                    here
                })
                .expect("a pick below the total weight");
            let mut qubits = [0; 3];
            for k in 0..MIX[gate].1 {
                // A block once per instruction: the protocol refuses a repeat.
                qubits[k] = loop {
                    let qubit = random.below(QUBITS);
                    if !qubits[..k].contains(&qubit) {
                        break qubit;
                    }
                };
            }
            let theta = f64::from(random.below(1 << 30)) / f64::from(1u32 << 30);
            // Drawn for an `x_if` alone, so that the common mix's draws
            // stay those of the bench before the parity-heavy mix.
            let back = (MIX[gate].0 == MixGates::X_IF).then(|| {
                let first = 1 + random.below(LOOKBACK - 1);
                [first, first + 1]
            });
            Draw {
                gate,
                qubits,
                theta,
                back,
            }
        })
        .collect()
}

/// The program of `draws`, each instruction built and checked by name.
pub(crate) fn build(draws: &[Draw]) -> Vec<Instruction> {
    let set = MixGates::set();
    draws.iter().map(|draw| build_one(&set, draw)).collect()
}

/// The instruction of `draw`, built and checked by name in `set`,
/// `MixGates`.
pub(crate) fn build_one(set: &InstructionSet, draw: &Draw) -> Instruction {
    let (opcode, arity, _) = MIX[draw.gate];
    let mnemonic = set.mnemonic(opcode).expect("a gate of MixGates");
    let theta = [("theta", Argument::from(draw.theta))];
    let parity = draw.back.map(|back| [("c", Argument::parity(back, false))]);
    let arguments: &[(&str, Argument)] = match (opcode, &parity) {
        (MixGates::RZ, _) => &theta,
        (MixGates::X_IF, Some(parity)) => parity,
        _ => &[],
    };
    set.instruction(mnemonic, draw.qubits[..arity].iter().copied(), arguments)
        .expect("an instruction of MixGates")
}

/// One instruction of a mix, drawn: its gate in `MIX`, its qubits (the
/// first ones its gate takes, all different), its angle, and, for an
/// `x_if`, the readouts of its parity, each counted back from the shot's
/// latest.
pub(crate) struct Draw {
    gate: usize,
    qubits: [u32; 3],
    theta: f64,
    back: Option<[u32; 2]>,
}

/// How far back a parity's readouts are drawn from: a round's syndromes.
pub(crate) const LOOKBACK: u32 = 16;

/// A random-number generator, `SplitMix64`: the mix's draws, without a
/// crate beyond the protocol.
pub(crate) struct RandomGenerator(u64);

impl RandomGenerator {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A draw below `n`.
    fn below(&mut self, n: u32) -> u32 {
        u32::try_from(self.next() % u64::from(n)).expect("a draw below a u32")
    }
}
