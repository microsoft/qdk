// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Defines the logical instruction set that can appear in QIR emitted from Q#.

use crate::interface::{
    Parameter::{Float, QubitId, ResultId},
    QuantumBackendInstruction,
};

pub type InstructionId = u16;

/// Defines canonical IDs, names, and operand shapes.
macro_rules! define_instructions {
    ($(($id:ident, $value:literal, $name:literal, [$($parameter:ident(_)),*])),* $(,)?) => {
        $(
            #[doc = concat!("Instruction ID for `", $name, "`.")]
            pub const $id: InstructionId = $value;
        )*

        /// The number of supported QIR instruction kinds.
        pub const OPERATION_COUNT: InstructionId = 0 $(+ { let _ = $name; 1 })*;

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
        pub fn validate_instruction(instruction: &QuantumBackendInstruction) {
            assert!(
                instruction_is_valid(instruction),
                "Unsupported or malformed QIR instruction: {instruction:?}"
            );
        }

        /// Checks IDs and operand shapes without panicking on untrusted input.
        pub(crate) fn instruction_is_valid(instruction: &QuantumBackendInstruction) -> bool {
            match instruction.instruction_id {
                $($id => matches!(
                    instruction.parameters.as_slice(),
                    [$($parameter(_)),*]
                ),)*
                _ => false,
            }
        }

        /// Parses wire operands using the canonical instruction's parameter types.
        pub(crate) fn parse_instruction<'a>(
            name: &str,
            mut arguments: impl Iterator<Item = &'a str>,
        ) -> Result<QuantumBackendInstruction, String> {
            let id = instruction_name_to_id(name)
                .ok_or_else(|| format!("Unknown instruction: {name}"))?;
            let instruction = match id {
                $($id => QuantumBackendInstruction {
                    instruction_id: $id,
                    parameters: smallvec::smallvec![$(
                        $parameter(
                            arguments.next()
                                .ok_or_else(|| format!("Missing argument for {name}"))?
                                .parse()
                                .map_err(|error| format!("Invalid argument for {name}: {error}"))?
                        )
                    ),*],
                },)*
                _ => unreachable!("instruction name maps to a canonical ID"),
            };
            if arguments.next().is_some() {
                return Err(format!("Too many arguments for {name}"));
            }
            Ok(instruction)
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

    // New quantum instructions beyond what we currently support in QIR.

    // Adjoint AND from https://arxiv.org/abs/1709.06648.
    // Equivalent to CCNOT, assuming after application target qubit is 0.
    (AND_ADJ, 26, "and_adj", [QubitId(_), QubitId(_), QubitId(_)]),
}

/// Identifies instructions that produce a result.
pub fn produces_result(instruction: &QuantumBackendInstruction) -> bool {
    matches!(instruction.instruction_id, READ_RESULT | MZ | MZ_RESET)
}

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::smallvec;

    #[test]
    /// Check that for every instruction its canonical name maps back to its ID.
    fn names_round_trip() {
        for instruction_id in 0..OPERATION_COUNT {
            let name = instruction_id_to_name(instruction_id);
            assert_eq!(instruction_name_to_id(name), Some(instruction_id), "{name}");
        }
    }

    #[test]
    /// Uses the ID to distinguish gates, immediate results, and typed result-register operands.
    fn validates_and_classifies_instruction_ids() {
        let mut instruction = QuantumBackendInstruction {
            instruction_id: H,
            parameters: smallvec![QubitId(7)],
        };
        assert!(instruction_is_valid(&instruction));
        assert!(!produces_result(&instruction));

        for id in [MZ, MZ_RESET] {
            instruction.instruction_id = id;
            assert!(instruction_is_valid(&instruction));
            assert!(produces_result(&instruction));
        }

        instruction.instruction_id = READ_RESULT;
        assert!(!instruction_is_valid(&instruction));
        instruction.parameters[0] = ResultId(7);
        assert!(instruction_is_valid(&instruction));
        assert!(produces_result(&instruction));

        for id in [OPERATION_COUNT, InstructionId::MAX] {
            instruction.instruction_id = id;
            assert!(!instruction_is_valid(&instruction));
            assert!(!produces_result(&instruction));
            assert!(format!("{instruction:?}").contains(&id.to_string()));
        }
    }
}
