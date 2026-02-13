// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// NOTE: To add a new instruction ID:
// 1. Add it to the `define_instructions!` macro below (primary or alias section)
// 2. Add it to `add_instruction_ids` in qre.rs
// 3. Add it to instruction_ids.pyi
//
// The `instruction_name` function is auto-generated from the primary entries.

#[cfg(test)]
mod tests;

/// Macro that defines instruction ID constants and generates the `instruction_name` function.
/// Primary entries are the canonical names returned by `instruction_name`.
/// Aliases are alternative names for the same value.
macro_rules! define_instructions {
    (
        primary: [ $( ($name:ident, $value:expr) ),* $(,)? ],
        aliases: [ $( ($alias:ident, $avalue:expr) ),* $(,)? ]
    ) => {
        // Define primary constants
        $(
            pub const $name: u64 = $value;
        )*

        // Define alias constants
        $(
            pub const $alias: u64 = $avalue;
        )*

        /// Returns the canonical name for an instruction ID.
        /// For IDs with aliases, returns the primary name.
        #[must_use]
        pub fn instruction_name(id: u64) -> Option<&'static str> {
            match id {
                $(
                    $name => Some(stringify!($name)),
                )*
                _ => None,
            }
        }
    };
}

define_instructions! {
    primary: [
        // Paulis
        (PAULI_I, 0x0),
        (PAULI_X, 0x1),
        (PAULI_Y, 0x2),
        (PAULI_Z, 0x3),

        // Clifford gates
        (H, 0x10),
        (H_XY, 0x11),
        (H_YZ, 0x12),
        (SQRT_X, 0x13),
        (SQRT_X_DAG, 0x14),
        (SQRT_Y, 0x15),
        (SQRT_Y_DAG, 0x16),
        (S, 0x17),
        (S_DAG, 0x18),
        (CNOT, 0x19),
        (CY, 0x1A),
        (CZ, 0x1B),
        (SWAP, 0x1C),

        // State preparation
        (PREP_X, 0x30),
        (PREP_Y, 0x31),
        (PREP_Z, 0x32),

        // Generic Cliffords
        (ONE_QUBIT_CLIFFORD, 0x50),
        (TWO_QUBIT_CLIFFORD, 0x51),
        (N_QUBIT_CLIFFORD, 0x52),

        // Measurements
        (MEAS_X, 0x100),
        (MEAS_Y, 0x101),
        (MEAS_Z, 0x102),
        (MEAS_RESET_X, 0x103),
        (MEAS_RESET_Y, 0x104),
        (MEAS_RESET_Z, 0x105),
        (MEAS_XX, 0x106),
        (MEAS_YY, 0x107),
        (MEAS_ZZ, 0x108),
        (MEAS_XZ, 0x109),
        (MEAS_XY, 0x10A),
        (MEAS_YZ, 0x10B),

        // Non-Clifford gates
        (SQRT_SQRT_X, 0x400),
        (SQRT_SQRT_X_DAG, 0x401),
        (SQRT_SQRT_Y, 0x402),
        (SQRT_SQRT_Y_DAG, 0x403),
        (T, 0x404),
        (T_DAG, 0x405),
        (CCX, 0x406),
        (CCY, 0x407),
        (CCZ, 0x408),
        (CSWAP, 0x409),
        (AND, 0x40A),
        (AND_DAG, 0x40B),
        (RX, 0x40C),
        (RY, 0x40D),
        (RZ, 0x40E),
        (CRX, 0x40F),
        (CRY, 0x410),
        (CRZ, 0x411),
        (RXX, 0x412),
        (RYY, 0x413),
        (RZZ, 0x414),

        // Generic unitaries
        (ONE_QUBIT_UNITARY, 0x500),
        (TWO_QUBIT_UNITARY, 0x501),

        // Multi-qubit Pauli measurement
        (MULTI_PAULI_MEAS, 0x1000),

        // Some generic logical instructions
        (LATTICE_SURGERY, 0x1100),

        // Memory/compute operations (used in compute parts of memory-compute layouts)
        (READ_FROM_MEMORY, 0x1200),
        (WRITE_TO_MEMORY, 0x1201),
        (MEMORY, 0x1210),

        // Some special hardware physical instructions
        (CYCLIC_SHIFT, 0x1300),

        // Generic operation (for unified RE)
        (GENERIC, 0xFFFF),
    ],
    aliases: [
        // Clifford gate aliases
        (H_XZ, 0x10),       // alias for H
        (SQRT_Z, 0x17),     // alias for S
        (SQRT_Z_DAG, 0x18), // alias for S_DAG
        (CX, 0x19),         // alias for CNOT

        // Non-Clifford aliases
        (SQRT_SQRT_Z, 0x404),     // alias for T
        (SQRT_SQRT_Z_DAG, 0x405), // alias for T_DAG
    ]
}

#[must_use]
pub fn is_pauli_measurement(id: u64) -> bool {
    matches!(
        id,
        MEAS_X
            | MEAS_Y
            | MEAS_Z
            | MEAS_XX
            | MEAS_YY
            | MEAS_ZZ
            | MEAS_XZ
            | MEAS_XY
            | MEAS_YZ
            | MULTI_PAULI_MEAS
    )
}

#[must_use]
pub fn is_t_like(id: u64) -> bool {
    matches!(
        id,
        SQRT_SQRT_X
            | SQRT_SQRT_X_DAG
            | SQRT_SQRT_Y
            | SQRT_SQRT_Y_DAG
            | SQRT_SQRT_Z
            | SQRT_SQRT_Z_DAG
    )
}

#[must_use]
pub fn is_ccx_like(id: u64) -> bool {
    matches!(id, CCX | CCY | CCZ | CSWAP | AND | AND_DAG)
}

#[must_use]
pub fn is_rotation_like(id: u64) -> bool {
    matches!(id, RX | RY | RZ | RXX | RYY | RZZ)
}

#[must_use]
pub fn is_clifford(id: u64) -> bool {
    matches!(
        id,
        PAULI_I
            | PAULI_X
            | PAULI_Y
            | PAULI_Z
            | H_XZ
            | H_XY
            | H_YZ
            | SQRT_X
            | SQRT_X_DAG
            | SQRT_Y
            | SQRT_Y_DAG
            | SQRT_Z
            | SQRT_Z_DAG
            | CX
            | CY
            | CZ
            | SWAP
            | PREP_X
            | PREP_Y
            | PREP_Z
            | ONE_QUBIT_CLIFFORD
            | TWO_QUBIT_CLIFFORD
            | N_QUBIT_CLIFFORD
    )
}
