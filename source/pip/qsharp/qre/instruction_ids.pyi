# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# Paulis
PAULI_I: int
PAULI_X: int
PAULI_Y: int
PAULI_Z: int

# Clifford gates
H: int
H_XZ: int
H_XY: int
H_YZ: int
SQRT_X: int
SQRT_X_DAG: int
SQRT_Y: int
SQRT_Y_DAG: int
S: int
SQRT_Z: int
S_DAG: int
SQRT_Z_DAG: int
CNOT: int
CX: int
CY: int
CZ: int
SWAP: int

# State preparation
PREP_X: int
PREP_Y: int
PREP_Z: int

# Generic Cliffords
ONE_QUBIT_CLIFFORD: int
TWO_QUBIT_CLIFFORD: int
N_QUBIT_CLIFFORD: int

# Measurements
MEAS_X: int
MEAS_Y: int
MEAS_Z: int
MEAS_RESET_X: int
MEAS_RESET_Y: int
MEAS_RESET_Z: int
MEAS_XX: int
MEAS_YY: int
MEAS_ZZ: int
MEAS_XZ: int
MEAS_XY: int
MEAS_YZ: int

# Non-Clifford gates
SQRT_SQRT_X: int
SQRT_SQRT_X_DAG: int
SQRT_SQRT_Y: int
SQRT_SQRT_Y_DAG: int
SQRT_SQRT_Z: int
T: int
SQRT_SQRT_Z_DAG: int
T_DAG: int
CCX: int
CCY: int
CCZ: int
CSWAP: int
AND: int
AND_DAG: int
RX: int
RY: int
RZ: int
CRX: int
CRY: int
CRZ: int
RXX: int
RYY: int
RZZ: int

# Generic unitary gates
ONE_QUBIT_UNITARY: int
TWO_QUBIT_UNITARY: int

# Multi-qubit Pauli measurement
MULTI_PAULI_MEAS: int

# Some generic logical instructions
LATTICE_SURGERY: int

# Memory/compute operations (used in compute parts of memory-compute layouts)
READ_FROM_MEMORY: int
WRITE_TO_MEMORY: int

# Some special hardware physical instructions
CYCLIC_SHIFT: int

# Generic operation (for unified RE)
GENERIC: int
