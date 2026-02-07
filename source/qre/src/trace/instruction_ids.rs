// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// NOTE: Define new instruction ids here.  Then:
// - add them to `add_instruction_ids` in qre.rs
// - add them to instruction_ids.pyi

// Paulis
pub const PAULI_I: u64 = 0x0;
pub const PAULI_X: u64 = 0x1;
pub const PAULI_Y: u64 = 0x2;
pub const PAULI_Z: u64 = 0x3;

// Clifford gates
pub const H: u64 = 0x10;
pub const H_XZ: u64 = 0x10;
pub const H_XY: u64 = 0x11;
pub const H_YZ: u64 = 0x12;
pub const SQRT_X: u64 = 0x13;
pub const SQRT_X_DAG: u64 = 0x14;
pub const SQRT_Y: u64 = 0x15;
pub const SQRT_Y_DAG: u64 = 0x16;
pub const S: u64 = 0x17;
pub const SQRT_Z: u64 = 0x17;
pub const S_DAG: u64 = 0x18;
pub const SQRT_Z_DAG: u64 = 0x18;
pub const CNOT: u64 = 0x19;
pub const CX: u64 = 0x19;
pub const CY: u64 = 0x1A;
pub const CZ: u64 = 0x1B;
pub const SWAP: u64 = 0x1C;

// State preparation
pub const PREP_X: u64 = 0x30;
pub const PREP_Y: u64 = 0x31;
pub const PREP_Z: u64 = 0x32;

// Generic Cliffords
pub const ONE_QUBIT_CLIFFORD: u64 = 0x50;
pub const TWO_QUBIT_CLIFFORD: u64 = 0x51;
pub const N_QUBIT_CLIFFORD: u64 = 0x52;

// Measurements
pub const MEAS_X: u64 = 0x100;
pub const MEAS_Y: u64 = 0x101;
pub const MEAS_Z: u64 = 0x102;
pub const MEAS_RESET_X: u64 = 0x103;
pub const MEAS_RESET_Y: u64 = 0x104;
pub const MEAS_RESET_Z: u64 = 0x105;
pub const MEAS_XX: u64 = 0x106;
pub const MEAS_YY: u64 = 0x107;
pub const MEAS_ZZ: u64 = 0x108;
pub const MEAS_XZ: u64 = 0x109;
pub const MEAS_XY: u64 = 0x10A;
pub const MEAS_YZ: u64 = 0x10B;

// Non-Clifford gates
pub const SQRT_SQRT_X: u64 = 0x400;
pub const SQRT_SQRT_X_DAG: u64 = 0x401;
pub const SQRT_SQRT_Y: u64 = 0x402;
pub const SQRT_SQRT_Y_DAG: u64 = 0x403;
pub const SQRT_SQRT_Z: u64 = 0x404;
pub const T: u64 = 0x404;
pub const SQRT_SQRT_Z_DAG: u64 = 0x405;
pub const T_DAG: u64 = 0x405;
pub const CCX: u64 = 0x406;
pub const CCY: u64 = 0x407;
pub const CCZ: u64 = 0x408;
pub const CSWAP: u64 = 0x409;
pub const AND: u64 = 0x40A;
pub const AND_DAG: u64 = 0x40B;
pub const RX: u64 = 0x40C;
pub const RY: u64 = 0x40D;
pub const RZ: u64 = 0x40E;
pub const CRX: u64 = 0x40F;
pub const CRY: u64 = 0x410;
pub const CRZ: u64 = 0x411;
pub const RXX: u64 = 0x412;
pub const RYY: u64 = 0x413;
pub const RZZ: u64 = 0x414;

// Generic unitaries
pub const ONE_QUBIT_UNITARY: u64 = 0x500;
pub const TWO_QUBIT_UNITARY: u64 = 0x501;

// Multi-qubit Pauli measurement
pub const MULTI_PAULI_MEAS: u64 = 0x1000;

// Some generic logical instructions
pub const LATTICE_SURGERY: u64 = 0x1100;

// Memory/compute operations (used in compute parts of memory-compute layouts)
pub const READ_FROM_MEMORY: u64 = 0x1200;
pub const WRITE_TO_MEMORY: u64 = 0x1201;

// Some special hardware physical instructions
pub const CYCLIC_SHIFT: u64 = 0x1300;

// Generic operation (for unified RE)
pub const GENERIC: u64 = 0xFFFF;

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
