// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Defines the logical instruction set that can appear in QIR emitted from Q#.

use crate::interface::{
    ClassicalParameter::{Float, ResultId},
    InstructionId, QuantumInstruction,
};

/// Defines canonical IDs, names, and operand shapes without aliases.
macro_rules! define_instructions {
    ($(($id:ident, $value:literal, $name:literal, $targets:pat, $parameters:pat)),* $(,)?) => {
        $(
            #[doc = concat!("Instruction ID for `", $name, "`.")]
            pub const $id: InstructionId = $value;
        )*

        /// The number of supported QIR instruction kinds.
        pub const OPERATION_COUNT: usize = [$($name),*].len();

        /// Returns the canonical name, panicking for an unknown instruction ID.
        #[must_use]
        pub fn instruction_id_to_name(id: InstructionId) -> &'static str {
            match id {
                $($id => $name,)*
                _ => panic!("Unknown QIR instruction ID {id}"),
            }
        }

        /// Returns the canonical ID, or `None` for an unknown name.
        #[must_use]
        pub fn instruction_name_to_id(name: &str) -> Option<InstructionId> {
            match name {
                $($name => Some($id),)*
                _ => None,
            }
        }

        /// Rejects unknown instruction IDs and malformed operand shapes.
        pub fn validate_instruction(instruction: &QuantumInstruction) {
            let named = instruction;
            let valid = match named.instruction_id {
                $($id => matches!(
                    (named.targets.as_slice(), named.classical_parameters.as_slice()),
                    ($targets, $parameters)
                ),)*
                _ => false,
            };
            assert!(valid, "Unsupported or malformed QIR instruction: {instruction:?}");
        }
    };
}

define_instructions! {
    (CCX, 0, "ccx", [_, _, _], []),
    (CX, 1, "cx", [_, _], []),
    (CY, 2, "cy", [_, _], []),
    (CZ, 3, "cz", [_, _], []),
    (RX, 4, "rx", [_], [Float(_)]),
    (RXX, 5, "rxx", [_, _], [Float(_)]),
    (RY, 6, "ry", [_], [Float(_)]),
    (RYY, 7, "ryy", [_, _], [Float(_)]),
    (RZ, 8, "rz", [_], [Float(_)]),
    (RZZ, 9, "rzz", [_, _], [Float(_)]),
    (H, 10, "h", [_], []),
    (S, 11, "s", [_], []),
    (SADJ, 12, "sadj", [_], []),
    (SX, 13, "sx", [_], []),
    (T, 14, "t", [_], []),
    (TADJ, 15, "tadj", [_], []),
    (X, 16, "x", [_], []),
    (Y, 17, "y", [_], []),
    (Z, 18, "z", [_], []),
    (SWAP, 19, "swap", [_, _], []),
    (RESET, 20, "reset", [_], []),
    (MZ, 21, "mz", [_], [ResultId(_)]),
    (MRESETZ, 22, "mresetz", [_], [ResultId(_)]),
    (READ_RESULT, 23, "read_result", [], [ResultId(_)]),
}
