// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): the polyene case's inputs, read from
//! the text files beside this one. One `pppN` file of each kind per chain
//! length `N`:
//!
//! - [`QpePrefix`] (`pppN.circuit`): the QPE prefix as its trial state and
//!   its one controlled step, which the prefix repeats;
//! - [`Hamiltonian`] (`pppN.ham`): the Pauli terms of H on program qubits,
//!   and which qubits are the ancilla and each spin's modes;
//! - [`Reference`] (`pppN.<source>.reference`): the readings a run is
//!   compared with, from the exact state or another engine's run.
//!
//! In each, one item per line; `#` starts a comment.

use num_complex::Complex64;
use std::{error::Error, f64::consts::FRAC_1_SQRT_2, fs};
use tensornet::{Pauli, PauliSum, PauliTerm};

pub type Res<T> = Result<T, Box<dyn Error>>;

/// One gate of the prefix: its qubits, and its matrix, row-major, the
/// first qubit the most significant bit.
pub struct Gate {
    pub qubits: Vec<usize>,
    pub matrix: Vec<Complex64>,
}

/// The prefix of the first round of iterative QPE: `trial` prepares the
/// trial state |K⟩ on the system qubits, then H on the ancilla,
/// `controlled_step` `steps` times, and H on the ancilla.
///
/// The file has two sections, `trial` and `controlled_step <steps>`, each
/// followed by its gates, one per line: `name [angle] qubit…`, with `name`
/// one of `ry`, `rz`, `h`, `x`, `s`, `sadj`, `cx` (control first).
pub struct QpePrefix {
    pub qubits: usize,
    pub trial: Vec<Gate>,
    pub controlled_step: Vec<Gate>,
    pub steps: usize,
}

impl QpePrefix {
    pub fn read(path: &str) -> Res<Self> {
        let text = fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
        let (mut trial, mut controlled_step) = (Vec::new(), Vec::new());
        let mut steps = None;
        let mut in_step = None;
        for line in text.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            match words.as_slice() {
                [] => {}
                [first, ..] if first.starts_with('#') => {}
                ["trial"] => in_step = Some(false),
                ["controlled_step", count] => {
                    steps = Some(count.parse()?);
                    in_step = Some(true);
                }
                _ => {
                    let gate = parse_gate(&words).map_err(|error| format!("{line:?}: {error}"))?;
                    match in_step {
                        Some(false) => trial.push(gate),
                        Some(true) => controlled_step.push(gate),
                        None => return Err(format!("{path}: a gate before any section").into()),
                    }
                }
            }
        }
        let qubits = trial
            .iter()
            .chain(&controlled_step)
            .flat_map(|gate| gate.qubits.iter().map(|&q| q + 1))
            .max()
            .ok_or("no gates")?;
        Ok(Self {
            qubits,
            trial,
            controlled_step,
            steps: steps.ok_or("no controlled_step section")?,
        })
    }

    /// The gates of the whole prefix, the ancilla's two H included.
    pub fn gate_count(&self) -> usize {
        self.trial.len() + self.steps * self.controlled_step.len() + 2
    }

    /// The two-qubit gates of the whole prefix.
    pub fn two_qubit_count(&self) -> usize {
        let two = |gates: &[Gate]| gates.iter().filter(|gate| gate.qubits.len() == 2).count();
        two(&self.trial) + self.steps * two(&self.controlled_step)
    }
}

/// H on `qubit`.
pub fn hadamard(qubit: usize) -> Gate {
    let qubit = qubit.to_string();
    parse_gate(&["h", &qubit]).expect("h with one qubit is a gate")
}

fn parse_gate(words: &[&str]) -> Res<Gate> {
    let qubit = |at: usize| -> Res<usize> { Ok(words.get(at).ok_or("too few words")?.parse()?) };
    let angle = || -> Res<f64> { Ok(words.get(1).ok_or("no angle")?.parse()?) };
    let c = |re: f64, im: f64| Complex64::new(re, im);
    let (qubits, matrix) = match words[0] {
        "cx" => {
            let mut cx = vec![c(0.0, 0.0); 16];
            for (row, col) in [(0, 0), (1, 1), (2, 3), (3, 2)] {
                cx[4 * row + col] = c(1.0, 0.0);
            }
            (vec![qubit(1)?, qubit(2)?], cx)
        }
        "h" => (
            vec![qubit(1)?],
            vec![
                c(FRAC_1_SQRT_2, 0.0),
                c(FRAC_1_SQRT_2, 0.0),
                c(FRAC_1_SQRT_2, 0.0),
                c(-FRAC_1_SQRT_2, 0.0),
            ],
        ),
        "x" => (
            vec![qubit(1)?],
            vec![c(0.0, 0.0), c(1.0, 0.0), c(1.0, 0.0), c(0.0, 0.0)],
        ),
        "s" => (
            vec![qubit(1)?],
            vec![c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(0.0, 1.0)],
        ),
        "sadj" => (
            vec![qubit(1)?],
            vec![c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(0.0, -1.0)],
        ),
        "rz" => {
            let half = angle()? / 2.0;
            (
                vec![qubit(2)?],
                vec![
                    Complex64::from_polar(1.0, -half),
                    c(0.0, 0.0),
                    c(0.0, 0.0),
                    Complex64::from_polar(1.0, half),
                ],
            )
        }
        "ry" => {
            let (sin, cos) = (angle()? / 2.0).sin_cos();
            (
                vec![qubit(2)?],
                vec![c(cos, 0.0), c(-sin, 0.0), c(sin, 0.0), c(cos, 0.0)],
            )
        }
        other => return Err(format!("unknown gate {other}").into()),
    };
    Ok(Gate { qubits, matrix })
}

/// A Hamiltonian on program qubits, with the qubits of the ancilla and of
/// each spin's modes.
///
/// The file: `identity <c>`, `core <c>`, `kekule <E>`, `alpha <q>…`,
/// `beta <q>…`, `ancilla <q>`, and terms `term <c> <P>:<q> …`.
pub struct Hamiltonian {
    pub identity: f64,
    /// The core energy, added to ⟨H⟩.
    pub core: f64,
    /// ⟨K|H|K⟩, core energy included.
    pub kekule: f64,
    pub alpha: Vec<usize>,
    pub beta: Vec<usize>,
    pub ancilla: usize,
    pub terms: Vec<(f64, Vec<(usize, Pauli)>)>,
}

impl Hamiltonian {
    pub fn read(path: &str) -> Res<Self> {
        let text = fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
        let mut hamiltonian = Self {
            identity: 0.0,
            core: 0.0,
            kekule: f64::NAN,
            alpha: Vec::new(),
            beta: Vec::new(),
            ancilla: 0,
            terms: Vec::new(),
        };
        for line in text.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            if words.is_empty() || words[0].starts_with('#') {
                continue;
            }
            let qubits =
                || -> Res<Vec<usize>> { words[1..].iter().map(|word| Ok(word.parse()?)).collect() };
            let value = || -> Res<f64> { Ok(words.get(1).ok_or("no value")?.parse()?) };
            match words[0] {
                "identity" => hamiltonian.identity = value()?,
                "core" => hamiltonian.core = value()?,
                "kekule" => hamiltonian.kekule = value()?,
                "alpha" => hamiltonian.alpha = qubits()?,
                "beta" => hamiltonian.beta = qubits()?,
                "ancilla" => hamiltonian.ancilla = words.get(1).ok_or("no qubit")?.parse()?,
                "term" => hamiltonian
                    .terms
                    .push((value()?, parse_factors(&words[2..])?)),
                other => return Err(format!("{path}: unknown item {other}").into()),
            }
        }
        Ok(hamiltonian)
    }

    /// H as a Pauli sum, identity term included, core energy not.
    pub fn energy_sum(&self) -> Res<PauliSum> {
        let terms = self
            .terms
            .iter()
            .map(|(coefficient, factors)| {
                PauliTerm::new(Complex64::new(*coefficient, 0.0), factors.clone())
            })
            .collect::<Result<_, _>>()?;
        Ok(PauliSum::new(Complex64::new(self.identity, 0.0), terms)?)
    }
}

/// `N = Σ (1 − Z_q) / 2` over `modes`, and `N²`, as Pauli sums.
pub fn number_sums(modes: &[usize]) -> Res<(PauliSum, PauliSum)> {
    let c = |re: f64| Complex64::new(re, 0.0);
    #[expect(clippy::cast_precision_loss, reason = "a mode count")]
    let a = modes.len() as f64 / 2.0;
    let number = PauliSum::new(
        c(a),
        modes
            .iter()
            .map(|&q| PauliTerm::new(c(-0.5), vec![(q, Pauli::Z)]))
            .collect::<Result<_, _>>()?,
    )?;
    // N² = a² + n/4 − a Σ Z_i + ½ Σ_{i<j} Z_i Z_j, with n = 2a modes.
    let mut terms = Vec::new();
    for &i in modes {
        terms.push(PauliTerm::new(c(-a), vec![(i, Pauli::Z)])?);
    }
    for (k, &i) in modes.iter().enumerate() {
        for &j in &modes[k + 1..] {
            terms.push(PauliTerm::new(c(0.5), vec![(i, Pauli::Z), (j, Pauli::Z)])?);
        }
    }
    let square = PauliSum::new(c(a * a + a / 2.0), terms)?;
    Ok((number, square))
}

/// Factors `P:q`, with `P` one of `X`, `Y`, `Z`.
fn parse_factors(words: &[&str]) -> Res<Vec<(usize, Pauli)>> {
    words
        .iter()
        .map(|word| {
            let (pauli, qubit) = word.split_once(':').ok_or("a factor P:q")?;
            let pauli = match pauli {
                "X" => Pauli::X,
                "Y" => Pauli::Y,
                "Z" => Pauli::Z,
                other => return Err(format!("unknown Pauli {other}").into()),
            };
            Ok((qubit.parse()?, pauli))
        })
        .collect()
}

/// Where a reference's readings come from, which sets how closely a run
/// must agree with them (`main.rs`, `check`).
#[derive(Clone, Copy, Debug)]
pub enum ReferenceKind {
    /// The exact state, from a state-vector simulator.
    Exact,
    /// Another engine's run at bond `chi`, which kept the weight
    /// `1 − one_minus_w`.
    Truncated { chi: usize, one_minus_w: f64 },
}

/// The readings at the end of the prefix that a run is compared with.
///
/// The file: `kind exact` or `kind truncated chi <χ> one_minus_w <1 − W>`;
/// `energy <⟨H⟩>` (core energy included), `n_alpha <⟨N_α⟩>`,
/// `n_beta <⟨N_β⟩>`; `one <q> <P(One)>` for each qubit; and
/// `pauli <⟨P⟩> <P>:<q> …` for each Pauli string.
pub struct Reference {
    pub kind: ReferenceKind,
    pub energy: f64,
    pub n_alpha: f64,
    pub n_beta: f64,
    pub probabilities_of_one: Vec<(usize, f64)>,
    pub pauli_strings: Vec<(f64, Vec<(usize, Pauli)>)>,
}

impl Reference {
    pub fn read(path: &str) -> Res<Self> {
        let text = fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
        let mut kind = None;
        let (mut energy, mut n_alpha, mut n_beta) = (None, None, None);
        let (mut probabilities_of_one, mut pauli_strings) = (Vec::new(), Vec::new());
        for line in text.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            let value = |at: usize| -> Res<f64> { Ok(words.get(at).ok_or("no value")?.parse()?) };
            match words.as_slice() {
                [] => {}
                [first, ..] if first.starts_with('#') => {}
                ["kind", "exact"] => kind = Some(ReferenceKind::Exact),
                ["kind", "truncated", "chi", chi, "one_minus_w", weight] => {
                    kind = Some(ReferenceKind::Truncated {
                        chi: chi.parse()?,
                        one_minus_w: weight.parse()?,
                    });
                }
                ["energy", _] => energy = Some(value(1)?),
                ["n_alpha", _] => n_alpha = Some(value(1)?),
                ["n_beta", _] => n_beta = Some(value(1)?),
                ["one", qubit, _] => probabilities_of_one.push((qubit.parse()?, value(2)?)),
                ["pauli", _, factors @ ..] => {
                    pauli_strings.push((value(1)?, parse_factors(factors)?));
                }
                _ => return Err(format!("{path}: unknown line {line:?}").into()),
            }
        }
        let missing = |what: &str| format!("{path}: no {what}");
        Ok(Self {
            kind: kind.ok_or_else(|| missing("kind"))?,
            energy: energy.ok_or_else(|| missing("energy"))?,
            n_alpha: n_alpha.ok_or_else(|| missing("n_alpha"))?,
            n_beta: n_beta.ok_or_else(|| missing("n_beta"))?,
            probabilities_of_one,
            pauli_strings,
        })
    }
}
