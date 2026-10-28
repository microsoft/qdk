# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tiny statevector replay of straight-line QIR to catch effects that measured outputs hide.

The kit's oracles only see returned values. A program can return the right answer
while leaving an unmeasured (released) qubit entangled or flipped, e.g. when an
uncompute step applies a body instead of an adjoint. This replays every gate of a
branch-free QIR program, ignoring measurements and resets, and reports per-qubit
P(|1>) plus unmeasured qubits that do not end in |0>. Small programs only (dense
statevector, pure Python).
"""

import cmath
import math
import re

from harness.common import ReviewError

MEASURE = {"m", "mz", "mresetz"}
IGNORED = MEASURE | {"reset", "read_result"}


def gates_of(qir):
    gates = []
    for line in qir.splitlines():
        match = re.search(r"call void @__quantum__qis__(\w+?)__(body|adj|ctl|ctladj)\((.*)\)", line)
        if not match:
            continue
        name, kind, args = match.groups()
        qubits = [
            0 if q == "null" else int(re.search(r"i64 (\d+)", q).group(1))
            for q in re.findall(r"%Qubit\* (null|inttoptr \(i64 \d+ to %Qubit\*\))", args)
        ]
        angles = [float(x) for x in re.findall(r"double ([-0-9.eE+]+)", args)]
        gates.append((name, kind, angles, qubits))
    return gates


def _matrix(name, kind, angles):
    r2 = 1 / math.sqrt(2)
    paulis = {"x": [[0, 1], [1, 0]], "y": [[0, -1j], [1j, 0]], "z": [[1, 0], [0, -1]]}
    fixed = {
        "h": [[r2, r2], [r2, -r2]],
        "s": [[1, 0], [0, 1j]],
        "t": [[1, 0], [0, cmath.exp(1j * math.pi / 4)]],
        **paulis,
    }

    def rotation(pauli, theta):
        size = len(pauli)
        return [
            [(math.cos(theta / 2) if i == j else 0) - 1j * math.sin(theta / 2) * pauli[i][j] for j in range(size)]
            for i in range(size)
        ]

    def controlled(unitary, controls=1):
        size = 2 ** (controls + 1)
        matrix = [[1 if i == j else 0 for j in range(size)] for i in range(size)]
        for i in range(2):
            for j in range(2):
                matrix[size - 2 + i][size - 2 + j] = unitary[i][j]
        return matrix

    def kron(a, b):
        n = len(b)
        return [[a[i // n][j // n] * b[i % n][j % n] for j in range(len(a) * n)] for i in range(len(a) * n)]

    if name in fixed:
        matrix = fixed[name]
    elif name in ("rx", "ry", "rz"):
        matrix = rotation(paulis[name[1]], angles[0])
    elif name in ("cx", "cy", "cz"):
        matrix = controlled(paulis[name[1]])
    elif name == "ccx":
        matrix = controlled(paulis["x"], 2)
    elif name == "swap":
        matrix = [[1, 0, 0, 0], [0, 0, 1, 0], [0, 1, 0, 0], [0, 0, 0, 1]]
    elif name in ("rxx", "ryy", "rzz"):
        pauli = paulis[name[1]]
        matrix = rotation(kron(pauli, pauli), angles[0])
    else:
        raise ReviewError(f"statecheck does not model gate {name}")
    matrix = [[complex(x) for x in row] for row in matrix]
    if kind in ("adj", "ctladj"):
        matrix = [[matrix[j][i].conjugate() for j in range(len(matrix))] for i in range(len(matrix))]
    if kind in ("ctl", "ctladj"):
        raise ReviewError("statecheck does not model __ctl specializations; use decomposed QIR")
    return matrix


def _apply(state, count, matrix, targets):
    """targets[0] is the most significant bit of the matrix index; qubit q is bit q of the state index."""
    width = len(targets)
    others = [q for q in range(count) if q not in targets]
    new = list(state)
    for base in range(2 ** len(others)):
        start = sum(1 << q for j, q in enumerate(others) if (base >> j) & 1)
        indices = [
            start | sum(1 << q for j, q in enumerate(targets) if (m >> (width - 1 - j)) & 1)
            for m in range(2 ** width)
        ]
        vector = [state[i] for i in indices]
        for row, index in enumerate(indices):
            new[index] = sum(matrix[row][col] * vector[col] for col in range(2 ** width))
    return new


def check(qir, max_qubits=16, tolerance=1e-9):
    if re.search(r"\bbr i1\b", qir):
        raise ReviewError("statecheck handles straight-line (Base) QIR only")
    match = re.search(r'"required_num_qubits"="(\d+)"', qir)
    if not match:
        raise ReviewError("QIR lacks required_num_qubits")
    count = int(match.group(1))
    if count > max_qubits:
        raise ReviewError(f"{count} qubits exceeds the statecheck limit of {max_qubits}")
    gates = gates_of(qir)
    state = [0j] * (2**count)
    state[0] = 1
    for name, kind, angles, qubits in gates:
        if name in IGNORED:
            continue
        # QIR passes controls first and the target last, matching _matrix's layout.
        state = _apply(state, count, _matrix(name, kind, angles), qubits)
    probabilities = [abs(x) ** 2 for x in state]
    p1 = [sum(p for i, p in enumerate(probabilities) if (i >> q) & 1) for q in range(count)]
    measured = sorted({qubits[0] for name, _, _, qubits in gates if name in MEASURE and qubits})
    return {
        "qubits": count,
        "gates": len(gates),
        "measured": measured,
        "p1": [round(value, 9) for value in p1],
        "dirty_unmeasured": [q for q in range(count) if q not in measured and p1[q] > tolerance],
    }
