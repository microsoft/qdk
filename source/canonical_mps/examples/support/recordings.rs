// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Circuits and distributions read from files: a gate list, a recorded
//! shot, and the exact readout distribution of that shot.
//! The `accuracy_and_cost` example's header gives the format of a gate
//! list, and the `h2_shots` example's those of a recorded shot and of its
//! readout distribution, and where they come from.

use num_complex::Complex64;

use super::{
    ExampleResult, c,
    circuit::{Circuit, Op},
    gates::{H, controlled, pauli_x, pauli_y, pauli_z, phase, rx, ry, rz},
};
use std::{f64::consts::PI, fs};

/// A unitary gate list (a `--gates` file), such as a chemistry program's
/// phase estimation.
pub fn gate_list(path: &str) -> ExampleResult<Circuit> {
    let text = fs::read_to_string(path)?;
    let mut ops = Vec::new();
    let mut qubits = 0;
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let (on, matrix) = at_line(path, index, line, gate_line(line))?;
        qubits = on.iter().fold(qubits, |n, &q| n.max(q + 1));
        ops.push(Op::Gate { qubits: on, matrix });
    }
    Ok(Circuit {
        qubits,
        ops,
        readout: Vec::new(),
    })
}

/// One line of a gate list: the gate's qubits and its matrix.
fn gate_line(line: &str) -> ExampleResult<(Vec<usize>, Vec<Complex64>)> {
    let w: Vec<&str> = line.split_whitespace().collect();
    let q = |i: usize| -> ExampleResult<usize> { Ok(w.get(i).ok_or("short line")?.parse()?) };
    Ok(match w[0] {
        "cx" => (vec![q(1)?, q(2)?], controlled(pauli_x()).to_vec()),
        "h" => (vec![q(1)?], H.to_vec()),
        "s" => (vec![q(1)?], phase(PI / 2.0).to_vec()),
        "sadj" => (vec![q(1)?], phase(-PI / 2.0).to_vec()),
        "x" => (vec![q(1)?], pauli_x().to_vec()),
        "rz" => {
            let t: f64 = w.get(1).ok_or("short line")?.parse()?;
            (vec![q(2)?], rz(t).to_vec())
        }
        "ry" => {
            let t: f64 = w.get(1).ok_or("short line")?.parse()?;
            (vec![q(2)?], ry(t).to_vec())
        }
        other => return Err(format!("unknown gate {other}").into()),
    })
}

fn named_one(name: &str, angle: Option<f64>) -> ExampleResult<[Complex64; 4]> {
    let angle = || angle.ok_or("missing angle");
    let quarter = Complex64::from_polar(1.0, PI / 4.0);
    Ok(match name {
        "X" => pauli_x(),
        "Y" => pauli_y(),
        "Z" => pauli_z(),
        "H" => H,
        "S" => phase(PI / 2.0),
        "SAdj" => phase(-PI / 2.0),
        "T" => [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), quarter],
        "TAdj" => [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), quarter.conj()],
        "Rx" => rx(angle()?),
        "Ry" => ry(angle()?),
        "Rz" => rz(angle()?),
        _ => return Err(format!("unknown one-qubit gate {name}").into()),
    })
}

/// A recorded shot (a `SHOT.replay` file): its gates, its projections on
/// the outcomes the shot drew, and its readout qubits.
pub fn replay(path: &str) -> ExampleResult<Circuit> {
    let text = fs::read_to_string(path)?;
    let mut circuit = Circuit::new(0);
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        at_line(path, index, line, replay_line(&mut circuit, line))?;
    }
    circuit.qubits = circuit
        .ops
        .iter()
        .flat_map(|op| match op {
            Op::Gate { qubits, .. } => qubits.clone(),
            Op::Project { qubit, .. } => vec![*qubit],
        })
        .chain(circuit.readout.iter().copied())
        .max()
        .map_or(0, |q| q + 1);
    Ok(circuit)
}

/// One line of a recorded shot, added to `circuit`.
fn replay_line(circuit: &mut Circuit, line: &str) -> ExampleResult<()> {
    let w: Vec<&str> = line.split_whitespace().collect();
    let q = |i: usize| -> ExampleResult<usize> { Ok(w.get(i).ok_or("short line")?.parse()?) };
    match w[0] {
        "READOUT" => {
            circuit.readout = (1..w.len()).map(q).collect::<ExampleResult<_>>()?;
        }
        "P" => circuit.ops.push(Op::Project {
            qubit: q(1)?,
            one: match w.get(2) {
                Some(&"0") => false,
                Some(&"1") => true,
                _ => return Err("an outcome other than 0 or 1".into()),
            },
            probability: w.get(3).ok_or("short line")?.parse()?,
        }),
        "Cx" => circuit.two(q(1)?, q(2)?, controlled(pauli_x())),
        "Cy" => circuit.two(q(1)?, q(2)?, controlled(pauli_y())),
        "Cz" => circuit.two(q(1)?, q(2)?, controlled(pauli_z())),
        name => {
            let angle = w.get(2).map(|x| x.parse()).transpose()?;
            circuit.one(q(1)?, named_one(name, angle)?);
        }
    }
    Ok(())
}

/// The exact readout distribution of a recorded shot (a `SHOT.readout`
/// file), keyed by the outcome in record order (the first readout qubit
/// first).
pub fn readout_file(path: &str) -> ExampleResult<Vec<(String, f64)>> {
    let text = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        out.push(at_line(path, index, line, readout_line(line))?);
    }
    Ok(out)
}

/// One line of a readout distribution: the outcome and its probability.
fn readout_line(line: &str) -> ExampleResult<(String, f64)> {
    let w: Vec<&str> = line.split_whitespace().collect();
    Ok((w[0].to_string(), w.get(2).ok_or("short line")?.parse()?))
}

/// `result`, its error prefixed with the file, the line's number (from 1)
/// and the line itself.
fn at_line<T>(path: &str, index: usize, line: &str, result: ExampleResult<T>) -> ExampleResult<T> {
    result.map_err(|error| format!("{path}:{}: {error}: {line}", index + 1).into())
}
