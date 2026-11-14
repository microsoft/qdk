// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): the input files of `mps_prep_check`,
//! written by the generator of the MPS state-preparation study (outside this
//! repository). All qubits are program qubits; `main.rs` places them.
//!
//! - `circuit`: `qubits Q`, `state S`, `register r…` (the bond register),
//!   then one operation per line: `cx c t`, `cz a b`, `ry q θ`, `rz q θ`,
//!   `u1 q` and 4 complex entries (row-major, re im), `u2 a b` and 16
//!   (`a` the most significant), or `mark text` (a progress point).
//! - `ham`: `const c` lines (summed, the core energy included) and
//!   `term c P q P q …` lines, a Pauli string as `X3 Z7`.
//! - `checks`: `energy E`, `p1 q P` (P(One) of qubit `q`), and
//!   `expect v X3 Z7` (a Pauli string's expectation).
//! - `target-<placement>.bin`: a real MPS in engine site order, `u64`
//!   little-endian site count, then per site `u64` left, `u64` right and
//!   `left·2·right` `f64` in `[left][s][right]` order.

use num_complex::Complex64;
use std::error::Error;
use tensornet::Pauli;

pub type Res<T> = Result<T, Box<dyn Error>>;

/// One line of a circuit: an operation on program qubits, or a mark.
pub enum Op {
    Gate {
        qubits: Vec<usize>,
        matrix: Vec<Complex64>,
        #[allow(
            dead_code,
            reason = "read by the sparse simulator's example, which includes this file"
        )]
        kind: Kind,
    },
    Mark(String),
}

/// What a gate line named, for simulators with their own gates.
#[allow(
    dead_code,
    reason = "read by the sparse simulator's example, which includes this file"
)]
#[derive(Clone, Copy)]
pub enum Kind {
    Cx,
    Cz,
    Ry(f64),
    Rz(f64),
    /// `u1` or `u2`: only the matrix.
    Dense,
}

pub struct Circuit {
    pub qubits: usize,
    pub state_qubits: usize,
    pub register: Vec<usize>,
    pub ops: Vec<Op>,
}

fn real(x: f64) -> Complex64 {
    Complex64::new(x, 0.0)
}

fn numbers(words: &[&str]) -> Res<Vec<f64>> {
    words
        .iter()
        .map(|word| {
            word.parse::<f64>()
                .map_err(|error| format!("{word}: {error}").into())
        })
        .collect()
}

fn complex_entries(words: &[&str], count: usize) -> Res<Vec<Complex64>> {
    let values = numbers(words)?;
    if values.len() != 2 * count {
        return Err(format!("{} numbers, {} expected", values.len(), 2 * count).into());
    }
    Ok(values
        .chunks(2)
        .map(|pair| Complex64::new(pair[0], pair[1]))
        .collect())
}

fn qubit(word: &str) -> Res<usize> {
    word.parse()
        .map_err(|error| format!("qubit {word}: {error}").into())
}

fn cx() -> Vec<Complex64> {
    let mut m = vec![real(0.0); 16];
    for (row, col) in [(0, 0), (1, 1), (2, 3), (3, 2)] {
        m[4 * row + col] = real(1.0);
    }
    m
}

fn cz() -> Vec<Complex64> {
    let mut m = vec![real(0.0); 16];
    for (at, value) in [(0, 1.0), (5, 1.0), (10, 1.0), (15, -1.0)] {
        m[at] = real(value);
    }
    m
}

fn rotation(kind: &str, theta: f64) -> Vec<Complex64> {
    let (c, s) = ((theta / 2.0).cos(), (theta / 2.0).sin());
    if kind == "ry" {
        vec![real(c), real(-s), real(s), real(c)]
    } else {
        vec![
            Complex64::new(c, -s),
            real(0.0),
            real(0.0),
            Complex64::new(c, s),
        ]
    }
}

fn parse_op(kind: &str, rest: &[&str], circuit: &mut Circuit) -> Res<()> {
    let arg = |at: usize| -> Res<&str> {
        rest.get(at)
            .copied()
            .ok_or_else(|| format!("{kind}: argument {at} missing").into())
    };
    match kind {
        "qubits" => circuit.qubits = qubit(arg(0)?)?,
        "state" => circuit.state_qubits = qubit(arg(0)?)?,
        "register" => {
            circuit.register = rest.iter().map(|w| qubit(w)).collect::<Res<_>>()?;
        }
        "mark" => circuit.ops.push(Op::Mark(rest.join(" "))),
        "cx" | "cz" => circuit.ops.push(Op::Gate {
            qubits: vec![qubit(arg(0)?)?, qubit(arg(1)?)?],
            matrix: if kind == "cx" { cx() } else { cz() },
            kind: if kind == "cx" { Kind::Cx } else { Kind::Cz },
        }),
        "ry" | "rz" => {
            let theta = numbers(&[arg(1)?])?[0];
            circuit.ops.push(Op::Gate {
                qubits: vec![qubit(arg(0)?)?],
                matrix: rotation(kind, theta),
                kind: if kind == "ry" {
                    Kind::Ry(theta)
                } else {
                    Kind::Rz(theta)
                },
            });
        }
        "u1" => circuit.ops.push(Op::Gate {
            qubits: vec![qubit(arg(0)?)?],
            matrix: complex_entries(&rest[1..], 4)?,
            kind: Kind::Dense,
        }),
        "u2" => circuit.ops.push(Op::Gate {
            qubits: vec![qubit(arg(0)?)?, qubit(arg(1)?)?],
            matrix: complex_entries(&rest[2..], 16)?,
            kind: Kind::Dense,
        }),
        other => return Err(format!("unknown operation {other}").into()),
    }
    Ok(())
}

impl Circuit {
    pub fn read(path: &str) -> Res<Self> {
        let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
        let mut circuit = Self {
            qubits: 0,
            state_qubits: 0,
            register: Vec::new(),
            ops: Vec::new(),
        };
        for (number, line) in text.lines().enumerate() {
            let words: Vec<&str> = line.split_whitespace().collect();
            let Some((&kind, rest)) = words.split_first() else {
                continue;
            };
            if kind.starts_with('#') {
                continue;
            }
            parse_op(kind, rest, &mut circuit)
                .map_err(|error| format!("{path}:{}: {error}", number + 1))?;
        }
        if circuit.qubits == 0 {
            return Err(format!("{path}: no `qubits` line").into());
        }
        Ok(circuit)
    }

    /// The gate count, and how many act on two qubits.
    pub fn gate_counts(&self) -> (usize, usize) {
        let mut gates = 0;
        let mut two = 0;
        for op in &self.ops {
            if let Op::Gate { qubits, .. } = op {
                gates += 1;
                two += usize::from(qubits.len() == 2);
            }
        }
        (gates, two)
    }
}

/// A Pauli string as `X3 Z7`.
pub fn pauli_factors(words: &[&str]) -> Res<Vec<(usize, Pauli)>> {
    words
        .iter()
        .map(|word| {
            let (letter, index) = word.split_at(1);
            let pauli = match letter {
                "X" => Pauli::X,
                "Y" => Pauli::Y,
                "Z" => Pauli::Z,
                other => return Err(format!("Pauli {other}").into()),
            };
            Ok((qubit(index)?, pauli))
        })
        .collect()
}

pub struct Hamiltonian {
    pub constant: f64,
    pub terms: Vec<(f64, Vec<(usize, Pauli)>)>,
}

impl Hamiltonian {
    pub fn read(path: &str) -> Res<Self> {
        let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
        let mut hamiltonian = Self {
            constant: 0.0,
            terms: Vec::new(),
        };
        for line in text.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            match words.as_slice() {
                [] => {}
                [first, ..] if first.starts_with('#') => {}
                ["const", value] => hamiltonian.constant += numbers(&[value])?[0],
                ["term", value, factors @ ..] => hamiltonian
                    .terms
                    .push((numbers(&[value])?[0], pauli_factors(factors)?)),
                _ => return Err(format!("{path}: {line}").into()),
            }
        }
        Ok(hamiltonian)
    }
}

pub struct Checks {
    pub energy: Option<f64>,
    pub probabilities_of_one: Vec<(usize, f64)>,
    pub pauli_strings: Vec<(f64, Vec<(usize, Pauli)>)>,
}

impl Checks {
    pub fn read(path: &str) -> Res<Self> {
        let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
        let mut checks = Self {
            energy: None,
            probabilities_of_one: Vec::new(),
            pauli_strings: Vec::new(),
        };
        for line in text.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            match words.as_slice() {
                [] => {}
                [first, ..] if first.starts_with('#') => {}
                ["energy", value] => checks.energy = Some(numbers(&[value])?[0]),
                ["p1", q, value] => checks
                    .probabilities_of_one
                    .push((qubit(q)?, numbers(&[value])?[0])),
                ["expect", value, factors @ ..] => checks
                    .pauli_strings
                    .push((numbers(&[value])?[0], pauli_factors(factors)?)),
                _ => return Err(format!("{path}: {line}").into()),
            }
        }
        Ok(checks)
    }
}

/// A site tensor in the CPU engine's layout: element `a + left·(s + 2·b)`.
pub struct Tensor {
    pub left: usize,
    pub right: usize,
    pub elements: Vec<Complex64>,
}

/// Reads a `target-<placement>.bin` into the engine's layout.
pub fn read_target(path: &str) -> Res<Vec<Tensor>> {
    let bytes = std::fs::read(path).map_err(|error| format!("{path}: {error}"))?;
    let mut at = 0;
    let mut word = || -> Res<[u8; 8]> {
        let chunk: [u8; 8] = bytes
            .get(at..at + 8)
            .ok_or_else(|| format!("{path}: truncated"))?
            .try_into()?;
        at += 8;
        Ok(chunk)
    };
    let count = usize::try_from(u64::from_le_bytes(word()?))?;
    let mut sites = Vec::with_capacity(count);
    for _ in 0..count {
        let left = usize::try_from(u64::from_le_bytes(word()?))?;
        let right = usize::try_from(u64::from_le_bytes(word()?))?;
        let mut elements = vec![Complex64::new(0.0, 0.0); left * 2 * right];
        for a in 0..left {
            for s in 0..2 {
                for b in 0..right {
                    elements[a + left * (s + 2 * b)] = real(f64::from_le_bytes(word()?));
                }
            }
        }
        sites.push(Tensor {
            left,
            right,
            elements,
        });
    }
    Ok(sites)
}

/// ⟨a|b⟩ of two MPSs with the same number of sites.
pub fn overlap(a: &[Tensor], b: &[Tensor]) -> Complex64 {
    let zero = Complex64::new(0.0, 0.0);
    // env[x + left_a · y] over the bonds (x of a, y of b) at the current cut.
    let mut env = vec![Complex64::new(1.0, 0.0)];
    for (ta, tb) in a.iter().zip(b) {
        let (la, lb, ra, rb) = (ta.left, tb.left, ta.right, tb.right);
        // tmp[x, s, y'] = Σ_y env[x, y] b[y, s, y']
        let mut tmp = vec![zero; la * 2 * rb];
        for yp in 0..rb {
            for s in 0..2 {
                for y in 0..lb {
                    let value = tb.elements[y + lb * (s + 2 * yp)];
                    if value == zero {
                        continue;
                    }
                    for x in 0..la {
                        tmp[x + la * (s + 2 * yp)] += env[x + la * y] * value;
                    }
                }
            }
        }
        // env'[x', y'] = Σ_{x, s} conj(a[x, s, x']) tmp[x, s, y']
        let mut next = vec![zero; ra * rb];
        for yp in 0..rb {
            for xp in 0..ra {
                let mut sum = zero;
                for s in 0..2 {
                    for x in 0..la {
                        sum +=
                            ta.elements[x + la * (s + 2 * xp)].conj() * tmp[x + la * (s + 2 * yp)];
                    }
                }
                next[xp + ra * yp] = sum;
            }
        }
        env = next;
    }
    env[0]
}

/// The amplitudes of an MPS, site 0 the most significant bit.
pub fn amplitudes(sites: &[Tensor]) -> Vec<Complex64> {
    // v[x + count · bond], x the sites so far.
    let mut v = vec![Complex64::new(1.0, 0.0)];
    let mut count = 1;
    for t in sites {
        let mut next = vec![Complex64::new(0.0, 0.0); count * 2 * t.right];
        for b in 0..t.right {
            for a in 0..t.left {
                for s in 0..2 {
                    let value = t.elements[a + t.left * (s + 2 * b)];
                    for x in 0..count {
                        next[(2 * x + s) + 2 * count * b] += v[x + count * a] * value;
                    }
                }
            }
        }
        v = next;
        count *= 2;
    }
    v
}
