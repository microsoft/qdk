# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.


###################
# Instruction IDs #
###################

# Paulis
PAULI_I = 0x0
PAULI_X = 0x1
PAULI_Y = 0x2
PAULI_Z = 0x3

# Clifford gates
H = H_XZ = 0x10
H_XY = 0x11
H_YZ = 0x12
SQRT_X = 0x13
SQRT_X_DAG = 0x14
SQRT_Y = 0x15
SQRT_Y_DAG = 0x16
S = SQRT_Z = 0x17
S_DAG = SQRT_Z_DAG = 0x18
CNOT = CX = 0x19
CY = 0x1A
CZ = 0x1B
SWAP = 0x1C

# State preparation
PREP_X = 0x30
PREP_Y = 0x31
PREP_Z = 0x32

# Generic Cliffords
ONE_QUBIT_CLIFFORD = 0x50
TWO_QUBIT_CLIFFORD = 0x51
N_QUBIT_CLIFFORD = 0x52

# Measurements
MEAS_X = 0x100
MEAS_Y = 0x101
MEAS_Z = 0x102
MEAS_XX = 0x103
MEAS_YY = 0x104
MEAS_ZZ = 0x105
MEAS_XZ = 0x106
MEAS_XY = 0x107
MEAS_YZ = 0x108

# Non-Clifford gates
SQRT_SQRT_X = 0x400
SQRT_SQRT_X_DAG = 0x401
SQRT_SQRT_Y = 0x402
SQRT_SQRT_Y_DAG = 0x403
SQRT_SQRT_Z = T = 0x404
SQRT_SQRT_Z_DAG = T_DAG = 0x405
CCX = 0x406
CCY = 0x407
CCZ = 0x408
CSWAP = 0x409
AND = 0x40A
AND_DAG = 0x40B
RX = 0x40C
RY = 0x40D
RZ = 0x40E
CRX = 0x40F
CRY = 0x410
CRZ = 0x411
RXX = 0x412
RYY = 0x413
RZZ = 0x414

# Multi-qubit Pauli measurement
MULTI_PAULI_MEAS = 0x1000

# Some generic logical instructions
LATTICE_SURGERY = 0x1100

# Memory/compute operations (used in compute parts of memory-compute layouts)
READ_FROM_MEMORY = 0x1200
WRITE_TO_MEMORY = 0x1201

# Some special hardware physical instructions
CYCLIC_SHIFT = 0x1300

# Generic operation (for unified RE)
GENERIC = 0xFFFF
