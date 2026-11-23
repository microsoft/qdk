# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Gate lists and random Stim programs for the qdk-stim differential tests."""

import random
from typing import Optional

# fmt: off
SINGLE_QUBIT_GATES = [
    "I", "X", "Y", "Z",
    "H", "H_XZ", "H_XY", "H_YZ", "H_NXY", "H_NXZ", "H_NYZ",
    "S", "SQRT_Z", "S_DAG", "SQRT_Z_DAG",
    "SQRT_X", "SQRT_X_DAG", "SQRT_Y", "SQRT_Y_DAG",
    "C_XYZ", "C_ZYX", "C_NXYZ", "C_XNYZ", "C_XYNZ", "C_NZYX", "C_ZNYX", "C_ZYNX",
]

TWO_QUBIT_GATES = [
    "II",
    "CX", "CNOT", "ZCX", "CY", "ZCY", "CZ", "ZCZ",
    "XCX", "XCY", "XCZ", "YCX", "YCY", "YCZ",
    "SWAP", "ISWAP", "ISWAP_DAG", "CXSWAP", "SWAPCX", "CZSWAP", "SWAPCZ",
    "SQRT_XX", "SQRT_XX_DAG", "SQRT_YY", "SQRT_YY_DAG", "SQRT_ZZ", "SQRT_ZZ_DAG",
]

SINGLE_QUBIT_MEASUREMENTS = ["M", "MZ", "MX", "MY", "MR", "MRZ", "MRX", "MRY"]
PAIR_MEASUREMENTS = ["MXX", "MYY", "MZZ"]

# Gates that accept a measurement record as their control ({rec}) or target.
FEEDBACK = [
    "CX {rec} {q}", "CNOT {rec} {q}", "ZCX {rec} {q}", "CY {rec} {q}", "ZCY {rec} {q}",
    "CZ {rec} {q}", "ZCZ {rec} {q}", "CZ {q} {rec}", "XCZ {q} {rec}", "YCZ {q} {rec}",
]
# fmt: on


def random_program(
    qubits: int = 5,
    length: int = 60,
    noise: Optional[float] = None,
    measure_at_end: bool = False,
) -> str:
    """A random Stim program of gates, measurements, classical feedback and noise.

    `noise=None` only adds noise with probability 0 or 1, which the tableau oracle can
    compare exactly; a probability `noise` adds real noise and readout noise, for the
    statistical tests. Programs first use qubits as 0, 1, 2, ... and only reset qubits
    by measuring them
    """
    rng = random.Random()
    lines = ["R " + " ".join(map(str, range(qubits)))]
    records = 0

    def qubit() -> str:
        return str(rng.randrange(qubits))

    def pair() -> tuple[str, str]:
        a, b = rng.sample(range(qubits), 2)
        return str(a), str(b)

    def target(q: str) -> str:
        return f"!{q}" if rng.random() < 0.25 else q

    def readout() -> str:
        return f"({noise})" if noise and rng.random() < 0.5 else ""

    def pauli_product() -> str:
        size = rng.randint(1, min(qubits, 4))
        factors = [f"{rng.choice('XYZ')}{q}" for q in rng.sample(range(qubits), size)]
        return target("*".join(factors))

    for _ in range(length):
        kind = rng.choices(
            ["gate1", "gate2", "measure1", "measure2", "mpp", "feedback", "noise"],
            weights=[8, 8, 2, 1, 1, 2, 2],
        )[0]
        if kind == "gate1":
            lines.append(f"{rng.choice(SINGLE_QUBIT_GATES)} {qubit()}")
        elif kind == "gate2":
            lines.append(f"{rng.choice(TWO_QUBIT_GATES)} {' '.join(pair())}")
        elif kind == "measure1":
            gate = rng.choice(SINGLE_QUBIT_MEASUREMENTS)
            lines.append(f"{gate}{readout()} {target(qubit())}")
            records += 1
        elif kind == "measure2":
            a, b = pair()
            lines.append(f"{rng.choice(PAIR_MEASUREMENTS)}{readout()} {target(a)} {b}")
            records += 1
        elif kind == "mpp":
            products = [pauli_product() for _ in range(rng.randint(1, 2))]
            lines.append(f"MPP{readout()} {' '.join(products)}")
            records += len(products)
        elif kind == "feedback" and records:
            rec = f"rec[-{rng.randint(1, records)}]"
            lines.append(rng.choice(FEEDBACK).format(rec=rec, q=qubit()))
        elif kind == "noise":
            lines.append(_noise(rng, noise, qubit(), pair()))

    if measure_at_end:
        lines.append("M " + " ".join(map(str, range(qubits))))
    return "\n".join(lines)


def _noise(
    rng: random.Random, p: Optional[float], q: str, pair: tuple[str, str]
) -> str:
    a, b = pair

    def correlated() -> str:
        return f"{rng.choice('XYZ')}{a} {rng.choice('XYZ')}{b}"

    # Pauli channel arguments with `probability` on one random Pauli and 0 on the rest.
    def one_pauli(paulis: int, probability: object) -> str:
        return ", ".join(rng.sample([str(probability)] + ["0"] * (paulis - 1), paulis))

    if p is None:
        first, second = rng.choice("01"), rng.choice("01")
        return rng.choice(
            [
                f"X_ERROR({first}) {q}",
                f"Y_ERROR({first}) {q}",
                f"Z_ERROR({first}) {q}",
                f"PAULI_CHANNEL_1({one_pauli(3, 1)}) {q}",
                f"PAULI_CHANNEL_2({one_pauli(15, 1)}) {a} {b}",
                f"E({first}) {correlated()}",
                f"E({first}) {correlated()}\n"
                f"ELSE_CORRELATED_ERROR({second}) {correlated()}",
            ]
        )
    return rng.choice(
        [
            f"X_ERROR({p}) {q}",
            f"Y_ERROR({p}) {q}",
            f"Z_ERROR({p}) {q}",
            f"DEPOLARIZE1({p}) {q}",
            f"DEPOLARIZE2({p}) {a} {b}",
            f"PAULI_CHANNEL_1({one_pauli(3, p)}) {q}",
            f"PAULI_CHANNEL_2({one_pauli(15, p)}) {a} {b}",
            f"E({p}) {correlated()}",
            f"E({p}) {correlated()}\nELSE_CORRELATED_ERROR({p}) {correlated()}",
        ]
    )
