// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Defines the logical instruction set that can appear in QIR emitted from Q#.

use crate::interface::{
    ClassicalParameter::{self, ResultId},
    NamedQuantumInstruction,
};
use smallvec::smallvec;

/// A quantum instruction emitted by the QIR runner.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(non_camel_case_types)]
pub enum QirInstruction {
    ccx(usize, usize, usize), // control_qubit_id_1, control_qubit_id_2, target_qubit_id
    cx(usize, usize),         // control_qubit_id, target_qubit_id
    cy(usize, usize),         // control_qubit_id, target_qubit_id
    cz(usize, usize),         // control_qubit_id, target_qubit_id
    rx(f64, usize),           // angle, qubit_id
    rxx(f64, usize, usize),   // angle, qubit_id_1, qubit_id_2
    ry(f64, usize),           // angle, qubit_id
    ryy(f64, usize, usize),   // angle, qubit_id_1, qubit_id_2
    rz(f64, usize),           // angle, qubit_id
    rzz(f64, usize, usize),   // angle, qubit_id_1, qubit_id_2
    h(usize),                 // qubit_id
    s(usize),                 // qubit_id
    sadj(usize),              // qubit_id
    sx(usize),                // qubit_id
    t(usize),                 // qubit_id
    tadj(usize),              // qubit_id
    x(usize),                 // qubit_id
    y(usize),                 // qubit_id
    z(usize),                 // qubit_id
    swap(usize, usize),       // qubit_id_1, qubit_id_2
    reset(usize),             // qubit_id
    mz(usize, usize),         // qubit_id, result_id
    mresetz(usize, usize),    // qubit_id, result_id
    read_result(usize),       // result_id
}

impl From<NamedQuantumInstruction> for QirInstruction {
    /// Decodes canonical operands, panicking on unknown names or invalid parameter counts.
    fn from(instr: NamedQuantumInstruction) -> Self {
        use ClassicalParameter::Float;

        match (
            instr.name,
            instr.targets.as_slice(),
            instr.classical_parameters.as_slice(),
        ) {
            ("ccx", &[a, b, c], []) => Self::ccx(a, b, c),
            ("cx", &[a, b], []) => Self::cx(a, b),
            ("cy", &[a, b], []) => Self::cy(a, b),
            ("cz", &[a, b], []) => Self::cz(a, b),
            ("rx", &[q], &[Float(angle)]) => Self::rx(angle, q),
            ("rxx", &[a, b], &[Float(angle)]) => Self::rxx(angle, a, b),
            ("ry", &[q], &[Float(angle)]) => Self::ry(angle, q),
            ("ryy", &[a, b], &[Float(angle)]) => Self::ryy(angle, a, b),
            ("rz", &[q], &[Float(angle)]) => Self::rz(angle, q),
            ("rzz", &[a, b], &[Float(angle)]) => Self::rzz(angle, a, b),
            ("h", &[q], []) => Self::h(q),
            ("s", &[q], []) => Self::s(q),
            ("sadj", &[q], []) => Self::sadj(q),
            ("sx", &[q], []) => Self::sx(q),
            ("t", &[q], []) => Self::t(q),
            ("tadj", &[q], []) => Self::tadj(q),
            ("x", &[q], []) => Self::x(q),
            ("y", &[q], []) => Self::y(q),
            ("z", &[q], []) => Self::z(q),
            ("swap", &[a, b], []) => Self::swap(a, b),
            ("reset", &[q], []) => Self::reset(q),
            ("mz", &[q], &[ClassicalParameter::ResultId(r)]) => Self::mz(q, r),
            ("mresetz", &[q], &[ClassicalParameter::ResultId(r)]) => Self::mresetz(q, r),
            ("read_result", [], &[ClassicalParameter::ResultId(r)]) => Self::read_result(r),
            (name, _, _) if !instruction_names().contains(&name) => {
                panic!("Instruction `{name}` is not supported by QIR")
            }
            _ => panic!(
                "Invalid operands for QIR instruction `{}`: targets {:?}, classical parameters {:?}",
                instr.name, instr.targets, instr.classical_parameters
            ),
        }
    }
}

impl From<QirInstruction> for NamedQuantumInstruction {
    /// Encodes operand IDs in order and rotation angles as classical parameters.
    fn from(instruction: QirInstruction) -> Self {
        let targets = match instruction {
            QirInstruction::ccx(a, b, c) => smallvec![a, b, c],
            QirInstruction::cx(a, b)
            | QirInstruction::cy(a, b)
            | QirInstruction::cz(a, b)
            | QirInstruction::rxx(_, a, b)
            | QirInstruction::ryy(_, a, b)
            | QirInstruction::rzz(_, a, b)
            | QirInstruction::swap(a, b) => smallvec![a, b],
            QirInstruction::mz(a, _) | QirInstruction::mresetz(a, _) => smallvec![a],
            QirInstruction::rx(_, q)
            | QirInstruction::ry(_, q)
            | QirInstruction::rz(_, q)
            | QirInstruction::h(q)
            | QirInstruction::s(q)
            | QirInstruction::sadj(q)
            | QirInstruction::sx(q)
            | QirInstruction::t(q)
            | QirInstruction::tadj(q)
            | QirInstruction::x(q)
            | QirInstruction::y(q)
            | QirInstruction::z(q)
            | QirInstruction::reset(q) => smallvec![q],
            QirInstruction::read_result(_) => smallvec![],
        };
        let classical_parameters = match instruction {
            QirInstruction::rx(angle, _)
            | QirInstruction::rxx(angle, _, _)
            | QirInstruction::ry(angle, _)
            | QirInstruction::ryy(angle, _, _)
            | QirInstruction::rz(angle, _)
            | QirInstruction::rzz(angle, _, _) => smallvec![ClassicalParameter::Float(angle)],
            QirInstruction::mz(_, r) | QirInstruction::mresetz(_, r) => {
                smallvec![ClassicalParameter::ResultId(r)]
            }
            QirInstruction::read_result(result_id) => {
                smallvec![ClassicalParameter::ResultId(result_id)]
            }
            _ => smallvec![],
        };
        Self {
            name: instruction_name(&instruction),
            targets,
            classical_parameters,
        }
    }
}

/// The number of QIR instruction kinds accepted by a quantum backend.
pub const OPERATION_COUNT: usize = 24;

const INSTRUCTION_NAMES: [&str; OPERATION_COUNT] = [
    "ccx",
    "cx",
    "cy",
    "cz",
    "rx",
    "rxx",
    "ry",
    "ryy",
    "rz",
    "rzz",
    "h",
    "s",
    "sadj",
    "sx",
    "t",
    "tadj",
    "x",
    "y",
    "z",
    "swap",
    "reset",
    "mz",
    "mresetz",
    "read_result",
];

/// Returns the stable names of all supported QIR instruction kinds.
#[must_use]
pub fn instruction_names() -> &'static [&'static str; OPERATION_COUNT] {
    &INSTRUCTION_NAMES
}

/// Returns the stable operation name for a QIR instruction.
#[must_use]
pub fn instruction_name(instruction: &QirInstruction) -> &'static str {
    INSTRUCTION_NAMES[instruction_index(instruction)]
}

/// Returns the array index for a QIR instruction.
pub(crate) fn instruction_index(instruction: &QirInstruction) -> usize {
    match instruction {
        QirInstruction::ccx(..) => 0,
        QirInstruction::cx(..) => 1,
        QirInstruction::cy(..) => 2,
        QirInstruction::cz(..) => 3,
        QirInstruction::rx(..) => 4,
        QirInstruction::rxx(..) => 5,
        QirInstruction::ry(..) => 6,
        QirInstruction::ryy(..) => 7,
        QirInstruction::rz(..) => 8,
        QirInstruction::rzz(..) => 9,
        QirInstruction::h(..) => 10,
        QirInstruction::s(..) => 11,
        QirInstruction::sadj(..) => 12,
        QirInstruction::sx(..) => 13,
        QirInstruction::t(..) => 14,
        QirInstruction::tadj(..) => 15,
        QirInstruction::x(..) => 16,
        QirInstruction::y(..) => 17,
        QirInstruction::z(..) => 18,
        QirInstruction::swap(..) => 19,
        QirInstruction::reset(..) => 20,
        QirInstruction::mz(..) => 21,
        QirInstruction::mresetz(..) => 22,
        QirInstruction::read_result(..) => 23,
    }
}

#[cfg(test)]
mod tests {
    use super::{QirInstruction, instruction_name, instruction_names};
    use crate::interface::{ClassicalParameter, NamedQuantumInstruction};

    #[test]
    /// Checks canonical operand layouts and both conversions for every instruction.
    fn named_instructions_round_trip() {
        use ClassicalParameter::{Float, ResultId};
        use QirInstruction::{
            ccx, cx, cy, cz, h, mresetz, mz, read_result, reset, rx, rxx, ry, ryy, rz, rzz, s,
            sadj, swap, sx, t, tadj, x, y, z,
        };

        let cases: [(QirInstruction, &[usize], &[ClassicalParameter]); 24] = [
            (ccx(3, 5, 7), &[3, 5, 7], &[]),
            (cx(3, 5), &[3, 5], &[]),
            (cy(3, 5), &[3, 5], &[]),
            (cz(3, 5), &[3, 5], &[]),
            (rx(-0.25, 3), &[3], &[Float(-0.25)]),
            (rxx(0.5, 3, 5), &[3, 5], &[Float(0.5)]),
            (ry(0.75, 3), &[3], &[Float(0.75)]),
            (ryy(-1.0, 3, 5), &[3, 5], &[Float(-1.0)]),
            (rz(1.25, 3), &[3], &[Float(1.25)]),
            (rzz(-1.5, 3, 5), &[3, 5], &[Float(-1.5)]),
            (h(3), &[3], &[]),
            (s(3), &[3], &[]),
            (sadj(3), &[3], &[]),
            (sx(3), &[3], &[]),
            (t(3), &[3], &[]),
            (tadj(3), &[3], &[]),
            (x(3), &[3], &[]),
            (y(3), &[3], &[]),
            (z(3), &[3], &[]),
            (swap(3, 5), &[3, 5], &[]),
            (reset(3), &[3], &[]),
            (mz(3, 11), &[3], &[ResultId(11)]),
            (mresetz(3, 11), &[3], &[ResultId(11)]),
            (read_result(11), &[], &[ResultId(11)]),
        ];
        assert_eq!(cases.len(), instruction_names().len());
        for (instruction, targets, parameters) in cases {
            let named: NamedQuantumInstruction = instruction.into();
            assert!(!named.targets.spilled());
            assert!(!named.classical_parameters.spilled());
            assert_eq!(named.name, instruction_name(&instruction));
            assert_eq!(named.targets.as_slice(), targets);
            match (named.classical_parameters.as_slice(), parameters) {
                ([], []) => {}
                ([Float(actual)], [Float(expected)]) => {
                    assert_eq!(actual, expected);
                }
                ([ResultId(actual)], [ResultId(expected)]) => {
                    assert_eq!(actual, expected);
                }
                _ => panic!("Unexpected classical parameters for {instruction:?}"),
            }
            assert_eq!(QirInstruction::from(named), instruction);

            let named = NamedQuantumInstruction {
                name: instruction_name(&instruction),
                targets: targets.iter().copied().collect(),
                classical_parameters: parameters
                    .iter()
                    .map(|parameter| match parameter {
                        Float(value) => Float(*value),
                        ResultId(id) => ResultId(*id),
                    })
                    .collect(),
            };
            let decoded: QirInstruction = named.into();
            assert_eq!(decoded, instruction);
        }
    }

    #[test]
    /// Rejects unknown names and malformed operand counts even in release builds.
    fn rejects_invalid_named_instructions() {
        for (name, targets, parameters) in [
            ("unknown", vec![0], vec![]),
            ("ccx", vec![0, 1], vec![]),
            ("h", vec![0, 1], vec![]),
            ("h", vec![0], vec![ClassicalParameter::Float(0.5)]),
            ("rx", vec![0], vec![]),
            ("rx", vec![], vec![ClassicalParameter::Float(0.5)]),
            (
                "rxx",
                vec![0, 1],
                vec![
                    ClassicalParameter::Float(0.5),
                    ClassicalParameter::Float(1.0),
                ],
            ),
            ("mz", vec![0], vec![]),
            ("read_result", vec![], vec![]),
        ] {
            let instruction = NamedQuantumInstruction {
                name,
                targets: targets.into_iter().collect(),
                classical_parameters: parameters.into_iter().collect(),
            };
            assert!(
                std::panic::catch_unwind(|| QirInstruction::from(instruction)).is_err(),
                "Malformed {name} should panic"
            );
        }
    }

    #[test]
    /// Verifies that every instruction maps to its canonical public name.
    fn instruction_names_match_all_instruction_variants() {
        let instructions = [
            QirInstruction::ccx(0, 1, 2),
            QirInstruction::cx(0, 1),
            QirInstruction::cy(0, 1),
            QirInstruction::cz(0, 1),
            QirInstruction::rx(0.5, 0),
            QirInstruction::rxx(0.5, 0, 1),
            QirInstruction::ry(0.5, 0),
            QirInstruction::ryy(0.5, 0, 1),
            QirInstruction::rz(0.5, 0),
            QirInstruction::rzz(0.5, 0, 1),
            QirInstruction::h(0),
            QirInstruction::s(0),
            QirInstruction::sadj(0),
            QirInstruction::sx(0),
            QirInstruction::t(0),
            QirInstruction::tadj(0),
            QirInstruction::x(0),
            QirInstruction::y(0),
            QirInstruction::z(0),
            QirInstruction::swap(0, 1),
            QirInstruction::reset(0),
            QirInstruction::mz(0, 0),
            QirInstruction::mresetz(0, 0),
            QirInstruction::read_result(0),
        ];

        assert_eq!(
            instructions
                .iter()
                .map(instruction_name)
                .collect::<Vec<_>>(),
            instruction_names()
        );
    }
}
