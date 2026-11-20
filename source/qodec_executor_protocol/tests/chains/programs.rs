// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Programs of a qodec's top ISA, in Qodec's REPL syntax, and the readouts
//! a shot gives for one.

use executor_protocol::ShotConditions;

/// A program: `execute`s separated by `|`, each of instructions separated
/// by `;`, each a mnemonic, its blocks, and its arguments as `name=value`,
/// a number, or 0 or 1 for a `bit` parameter:
/// `prepare_z 0; idle 0 | rotate_z 0 theta=0.5; measure_z 0`.
pub struct Program {
    /// Each `execute`'s instructions, in order.
    pub executes: Vec<Vec<Step>>,
}

/// One instruction of a program.
pub struct Step {
    pub mnemonic: String,
    /// Each operand's block, among the shot's blocks of its type.
    pub blocks: Vec<u32>,
    /// Its arguments' values, by parameter name.
    pub numbers: Vec<(String, f64)>,
}

impl Program {
    /// The program `text` writes.
    pub fn parse(text: &str) -> Self {
        let step = |text: &str| {
            let mut words = text.split_whitespace();
            let mnemonic = words.next().expect("a step has a mnemonic").to_owned();
            let mut blocks = Vec::new();
            let mut numbers = Vec::new();
            for word in words {
                match word.split_once('=') {
                    Some((name, value)) => numbers.push((
                        name.to_owned(),
                        value.parse().expect("an argument's value is a number"),
                    )),
                    None => blocks.push(word.parse().expect("an operand is a block")),
                }
            }
            Step {
                mnemonic,
                blocks,
                numbers,
            }
        };
        Self {
            executes: text
                .split('|')
                .map(|execute| execute.split(';').map(step).collect())
                .collect(),
        }
    }
}

/// What a shot reads in a program: each result's outcomes, then its flags,
/// in the program's order (`None` for a value not read), and the
/// conditions any result raised.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShotReadouts {
    pub values: Vec<Option<bool>>,
    pub conditions: ShotConditions,
}
