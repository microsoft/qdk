# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Restricted static-resource QIR replay to catch effects that measured outputs hide.

The kit's oracles only see returned values. A program can return the right answer
while leaving an unmeasured (released) qubit entangled or flipped, e.g. when an
uncompute step applies a body instead of an adjoint. Replay one basic block in one
entry function, with constant resource handles and terminal measurements only.
Report the state before those measurements, not a postmeasurement/reset state.
Reject unsupported execution shapes instead of silently approximating them.
"""

import cmath
import math
import re
import struct

from harness.common import ReviewError

MEASURE = {"m", "mz", "mresetz"}
SIGNATURES = {
    **{name: (0, 1) for name in ("h", "s", "t", "x", "y", "z")},
    **{name: (1, 1) for name in ("rx", "ry", "rz")},
    **{name: (0, 2) for name in ("cx", "cy", "cz", "swap")},
    **{name: (1, 2) for name in ("rxx", "ryy", "rzz")},
    "ccx": (0, 3),
}
OUTPUTS = {f"__quantum__rt__{kind}_record_output"
           for kind in ("result", "int", "bool", "double", "array", "tuple")}


def _arguments(text):
    parts, start, depth = [], 0, 0
    for index, character in enumerate(text):
        depth += character == "("
        depth -= character == ")"
        if depth < 0:
            raise ReviewError("statecheck encountered unbalanced call arguments")
        if character == "," and depth == 0:
            parts.append(text[start:index].strip())
            start = index + 1
    if depth:
        raise ReviewError("statecheck encountered unbalanced call arguments")
    return parts + [text[start:].strip()] if text.strip() else []


def _handle(text, kind):
    match = re.fullmatch(rf"%{kind}\*\s+(?:null|inttoptr\s*\(i64\s+(\d+)\s+to\s+%{kind}\*\))", text)
    if match is None:
        raise ReviewError(f"statecheck requires constant {kind} handles: {text}")
    return int(match.group(1) or 0)


def _angle(text):
    match = re.fullmatch(r"double\s+(\S+)", text)
    if match is None:
        raise ReviewError(f"statecheck requires constant double angles: {text}")
    token = match.group(1)
    if re.fullmatch(r"0x[0-9a-fA-F]{16}", token):
        value = struct.unpack(">d", bytes.fromhex(token[2:]))[0]
    elif re.fullmatch(r"[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][-+]?\d+)?", token):
        value = float(token)
    else:
        raise ReviewError(f"statecheck does not support double constant {token}")
    if not math.isfinite(value):
        raise ReviewError("statecheck requires finite rotation angles")
    return value


def gates_of(body, count, return_type):
    gates, measured, label, returned, started = [], False, False, False, False
    for raw in body.splitlines():
        line = raw.strip()
        if not line:
            continue
        if returned:
            raise ReviewError("statecheck does not support instructions after return")
        if re.fullmatch(r"[\w.$-]+:", line):
            if label or started:
                raise ReviewError("statecheck requires a single straight-line block")
            label = True
            continue
        started = True
        if line == ("ret void" if return_type == "void" else "ret i64 0"):
            returned = True
            continue
        match = re.fullmatch(r"(?:tail\s+)?call\s+void\s+@([\w.$]+)\((.*)\)(?:\s+#\d+)?", line)
        if match is None:
            raise ReviewError(f"statecheck requires a single straight-line block; unsupported instruction: {line}")
        function, arguments = match.groups()
        if function in OUTPUTS:
            continue
        if function == "__quantum__rt__initialize" and not gates:
            if arguments.strip() != "i8* null":
                raise ReviewError("statecheck requires default runtime initialization")
            continue
        qis = re.fullmatch(r"__quantum__qis__(\w+?)__(body|adj)", function)
        if qis is None:
            raise ReviewError(f"statecheck does not support call {function}")
        name, kind = qis.groups()
        args = _arguments(arguments)
        if name in MEASURE:
            if kind != "body" or len(args) != 2:
                raise ReviewError("statecheck requires static terminal measurements")
            _handle(args.pop(), "Result")
            angles, qubits = [], [_handle(args[0], "Qubit")]
            measured = True
        else:
            if measured:
                raise ReviewError("statecheck requires terminal measurements; gates/reset after measurement are unsupported")
            if name not in SIGNATURES:
                raise ReviewError(f"statecheck does not model gate {name}")
            angle_count, qubit_count = SIGNATURES[name]
            if len(args) != angle_count + qubit_count:
                raise ReviewError(f"statecheck encountered an unsupported {name} signature")
            angles = [_angle(arg) for arg in args[:angle_count]]
            qubits = [_handle(arg, "Qubit") for arg in args[angle_count:]]
            if len(set(qubits)) != len(qubits):
                raise ReviewError("statecheck requires distinct gate operands")
        if any(qubit >= count for qubit in qubits):
            raise ReviewError("statecheck qubit handle exceeds required_num_qubits")
        gates.append((name, kind, angles, qubits))
    if not returned:
        raise ReviewError("statecheck requires an explicit successful entry return")
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
    qir = "\n".join(line.split(";", 1)[0] for line in qir.splitlines())
    definitions = re.findall(r"(?m)^\s*define\b", qir)
    entry = re.search(r"(?ms)^\s*define\s+(void|i64)\s+@[\w.$]+\(\s*\)\s*([^{]*)\{(.*?)^\s*\}", qir)
    if len(definitions) != 1 or entry is None:
        raise ReviewError("statecheck requires one defined entry function with a straight-line body")
    return_type, header, body = entry.groups()
    attributes = header
    for group in re.findall(r"#(\d+)", header):
        found = re.search(rf"(?m)^\s*attributes\s+#{group}\s*=\s*\{{([^}}]*)\}}", qir)
        if found is None:
            raise ReviewError("statecheck cannot resolve the entry attributes")
        attributes += found.group(1)
    if '"entry_point"' not in attributes:
        raise ReviewError("statecheck requires a marked entry_point function")
    match = re.search(r'"required_num_qubits"\s*=\s*"(\d+)"', attributes)
    if not match:
        raise ReviewError("QIR lacks required_num_qubits")
    count = int(match.group(1))
    if count > max_qubits:
        raise ReviewError(f"{count} qubits exceeds the statecheck limit of {max_qubits}")
    gates = gates_of(body, count, return_type)
    state = [0j] * (2**count)
    state[0] = 1
    for name, kind, angles, qubits in gates:
        if name in MEASURE:
            continue
        # QIR passes controls first and the target last, matching _matrix's layout.
        state = _apply(state, count, _matrix(name, kind, angles), qubits)
    probabilities = [abs(x) ** 2 for x in state]
    p1 = [sum(p for i, p in enumerate(probabilities) if (i >> q) & 1) for q in range(count)]
    measured = sorted({qubits[0] for name, _, _, qubits in gates if name in MEASURE and qubits})
    return {
        "qubits": count,
        "state_point": "before_terminal_measurements" if any(name in MEASURE for name, *_ in gates) else "end_of_unitary",
        "gates": len(gates),
        "measured": measured,
        "p1": [round(value, 9) for value in p1],
        "dirty_unmeasured": [q for q in range(count) if q not in measured and p1[q] > tolerance],
    }
