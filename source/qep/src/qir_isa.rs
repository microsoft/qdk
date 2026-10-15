// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Defines the logical instruction set that can appear in QIR emitted from Q#.

/// A quantum instruction emitted by the QIR runner.
#[derive(Debug)]
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
