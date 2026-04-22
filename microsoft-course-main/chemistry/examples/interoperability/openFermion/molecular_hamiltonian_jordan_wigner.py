# --------------------------------------------------------------------------------------------
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License. See LICENSE.txt in the project root for license information.
# --------------------------------------------------------------------------------------------

"""qdk-chemistry ↔ OpenFermion bidirectional interop example.

Demonstrates the round-trip workflow:

1. QDK/Chemistry → OpenFermion: build a Hamiltonian with QDK, map it to qubits
   via the OpenFermion plugin, then convert to an OpenFermion ``QubitOperator``
   for exact diagonalisation.
2. OpenFermion → QDK/Chemistry: take that ``QubitOperator`` and convert it back
   to a QDK ``QubitHamiltonian``.

This example is adapted from the introduction to OpenFermion tutorial:
https://quantumai.google/openfermion/tutorials/intro_to_openfermion
"""

import numpy as np
from qdk_chemistry.algorithms import create
from qdk_chemistry.constants import ANGSTROM_TO_BOHR
from qdk_chemistry.data import Structure
from qdk_chemistry.utils import Logger

# OpenFermion must be installed to run this example.
try:
    import openfermion as of
    from qdk_chemistry.plugins.openfermion.conversion import (
        qubit_hamiltonian_to_qubit_operator,
        qubit_operator_to_qubit_hamiltonian,
    )
except ImportError as e:
    raise ImportError(
        "OpenFermion is not installed. Please install it to run this example: "
        "pip install qdk-chemistry[openfermion-extras]"
    ) from e

Logger.set_global_level("info")

ACTIVE_ELECTRONS = 2
ACTIVE_ORBITALS = 2

########################################################################################
# 1. QDK/Chemistry SCF calculation for LiH (1.45 Å, STO-3G)
########################################################################################
structure = Structure(
    np.array([[0.0, 0.0, 0.0], [0.0, 0.0, 1.45 * ANGSTROM_TO_BOHR]], dtype=float),
    ["Li", "H"],
)

scf_solver = create("scf_solver")
scf_energy, scf_wavefunction = scf_solver.run(
    structure, charge=0, spin_multiplicity=1, basis_or_guess="sto-3g"
)

########################################################################################
# 2. Active-space Hamiltonian
########################################################################################
selector = create(
    "active_space_selector",
    "qdk_valence",
    num_active_electrons=ACTIVE_ELECTRONS,
    num_active_orbitals=ACTIVE_ORBITALS,
)
active_orbitals = selector.run(scf_wavefunction).get_orbitals()

constructor = create("hamiltonian_constructor")
active_hamiltonian = constructor.run(active_orbitals)

Logger.info("=== LiH (1.45 Å, STO-3G) ===")
Logger.info(f"  SCF total energy: {scf_energy: .8f} Hartree")

########################################################################################
# 3. QDK → OpenFermion: map to qubits via the plugin, then exact-diagonalise
########################################################################################
mapper = create("qubit_mapper", "openfermion", encoding="jordan-wigner")
qdk_qubit_ham = mapper.run(active_hamiltonian)

Logger.info("=== QDK → OpenFermion (Jordan-Wigner) ===")
Logger.info(f"  Pauli terms: {len(qdk_qubit_ham.pauli_strings)}")

# Convert QDK QubitHamiltonian → OpenFermion QubitOperator and diagonalise.
qop = qubit_hamiltonian_to_qubit_operator(qdk_qubit_ham)
sparse = of.linalg.get_sparse_operator(qop)
energy, _ = of.linalg.get_ground_state(sparse)
Logger.info(f"  Ground-state energy: {energy: .15f} Hartree")

########################################################################################
# 4. OpenFermion → QDK: convert the QubitOperator back to a QDK QubitHamiltonian
########################################################################################
qdk_qubit_ham_rt = qubit_operator_to_qubit_hamiltonian(qop, encoding="jordan-wigner")

Logger.info("=== OpenFermion → QDK (round-trip check) ===")
assert qdk_qubit_ham_rt.equiv(qdk_qubit_ham), "Round-trip mismatch!"
Logger.info(
    f"  Round-trip check passed ({len(qdk_qubit_ham_rt.pauli_strings)} Pauli terms)"
)
