// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Defines the logical instruction set that can appear in QIR emitted from Q#.

use crate::interface::{
    Parameter::{FactoryId, Float, QubitId, ResultId},
    QuantumBackendInstruction,
};

pub type InstructionId = usize;

/// Defines canonical IDs, names, and operand shapes.
macro_rules! define_instructions {
    ($(($id:ident, $value:literal, $name:literal, $parameters:pat)),* $(,)?) => {
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

        /// Rejects unknown instruction names and malformed operand shapes.
        pub fn validate_instruction(instruction: &QuantumBackendInstruction) {
            let valid = match instruction.instruction_name {
                $($name => matches!(
                    instruction.parameters.as_slice(),
                    $parameters
                ),)*
                _ => false,
            };
            assert!(valid, "Unsupported or malformed QIR instruction: {instruction:?}");
        }
    };
}

define_instructions! {
    (CCX, 0, "ccx", [QubitId(_), QubitId(_), QubitId(_)]),
    (CX, 1, "cx", [QubitId(_), QubitId(_)]),
    (CY, 2, "cy", [QubitId(_), QubitId(_)]),
    (CZ, 3, "cz", [QubitId(_), QubitId(_)]),
    (RX, 4, "rx", [QubitId(_), Float(_)]),
    (RXX, 5, "rxx", [QubitId(_), QubitId(_), Float(_)]),
    (RY, 6, "ry", [QubitId(_), Float(_)]),
    (RYY, 7, "ryy", [QubitId(_), QubitId(_), Float(_)]),
    (RZ, 8, "rz", [QubitId(_), Float(_)]),
    (RZZ, 9, "rzz", [QubitId(_), QubitId(_), Float(_)]),
    (H, 10, "h", [QubitId(_)]),
    (S, 11, "s", [QubitId(_)]),
    (SADJ, 12, "sadj", [QubitId(_)]),
    (SX, 13, "sx", [QubitId(_)]),
    (T, 14, "t", [QubitId(_)]),
    (TADJ, 15, "tadj", [QubitId(_)]),
    (X, 16, "x", [QubitId(_)]),
    (Y, 17, "y", [QubitId(_)]),
    (Z, 18, "z", [QubitId(_)]),
    (SWAP, 19, "swap", [QubitId(_), QubitId(_)]),
    (RESET, 20, "reset", [QubitId(_)]),
    (MZ_STORE, 21, "mz_store", [QubitId(_), ResultId(_)]),
    (MZ_STORE_RESET, 22, "mz_store_reset", [QubitId(_), ResultId(_)]),
    (READ_RESULT, 23, "read_result", [ResultId(_)]),  // returns Result.
    (MZ, 24, "mz", [QubitId(_)]),  // returns Result.
    (MZ_RESET, 25, "mz_reset", [QubitId(_)]),  // returns Result.

    // These instructions are used internally by the profilers.
    // We really only need them to put inside events and detailed traces, so maybe they should not
    // part of the public ISA.
    (INJECT_T, 26, "inject_t", [FactoryId(_), QubitId(_)]),
    (INJECT_TADJ, 27, "inject_tadj", [FactoryId(_), QubitId(_)]),
    (ROUND, 28, "round", [FactoryId(_)]),

}

#[cfg(test)]
mod tests {
    use super::{
        MZ_STORE, MZ_STORE_RESET, instruction_id_to_name, instruction_name_to_id,
        validate_instruction,
    };
    use crate::{Parameter::ResultId, QuantumBackendInstruction};
    use smallvec::smallvec;

    #[test]
    /// Preserves measurement IDs and validates their canonical names and operand shapes.
    fn measurement_store_instructions_use_canonical_names() {
        for (id, expected_id, name) in [
            (MZ_STORE, 21, "mz_store"),
            (MZ_STORE_RESET, 22, "mz_store_reset"),
        ] {
            assert_eq!(id, expected_id);
            assert_eq!(instruction_id_to_name(id), name);
            assert_eq!(instruction_name_to_id(name), Some(id));
            validate_instruction(&QuantumBackendInstruction {
                instruction_name: name,
                parameters: smallvec![crate::Parameter::QubitId(0), ResultId(1)],
            });
        }
        assert_eq!(instruction_name_to_id("mresetz"), None);
    }

    #[test]
    /// Validates immediate measurement IDs, names, and result-slot-free operands.
    fn immediate_measurements_use_canonical_names() {
        for (id, expected_id, name) in [(super::MZ, 24, "mz"), (super::MZ_RESET, 25, "mz_reset")] {
            assert_eq!(id, expected_id);
            assert_eq!(instruction_id_to_name(id), name);
            assert_eq!(instruction_name_to_id(name), Some(id));
            validate_instruction(&QuantumBackendInstruction {
                instruction_name: name,
                parameters: smallvec![crate::Parameter::QubitId(0)],
            });
        }
    }
}
