# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Helpers to compare the final tableau states that Stim and qdk-stim reach for a Stim program."""

from dataclasses import dataclass
from typing import Iterable, Iterator, List, Tuple, cast

import stim

from qdk import Result
from qdk.simulation._simulation import run_qir_clifford_and_get_tableau
from qdk.stim import compile as compile_stim


@dataclass
class RunResult:
    """The final tableau of one run, plus its measurement record."""

    simulator: str
    destabilizers: List[str]
    stabilizers: List[str]
    record: str

    def canonical_stabilizers(self) -> Tuple[str, ...]:
        """Stim's canonical form of the stabilizers: equal if and only if the states are
        equal (up to global phase)."""
        tableau = stim.Tableau.from_stabilizers(
            [stim.PauliString(p) for p in self.stabilizers]
        )
        return tuple(_paulis(tableau.to_stabilizers(canonicalize=True)))

    def outcome(self) -> Tuple[str, Tuple[str, ...]]:
        """The outcome that two simulators must agree on: the measurement record and the canonical stabilizers."""
        return self.record, self.canonical_stabilizers()

    def __str__(self) -> str:
        width = len(self.stabilizers) + 1  # Pauli string plus an optional sign
        lines = [f"{self.simulator}:\n measurements {self.record or '(none)'}"]
        lines += [
            f"  {k}: destabilizer {x:>{width}}  stabilizer {z:>{width}}"
            for k, (x, z) in enumerate(zip(self.destabilizers, self.stabilizers))
        ]
        return "\n".join(lines)


# ---------------------------------------------------------------------------
# Stim (the oracle)
# ---------------------------------------------------------------------------


def run_stim(src: str) -> RunResult:
    """Runs a Stim program on Stim's tableau simulator."""
    sim = stim.TableauSimulator()
    sim.do_circuit(stim.Circuit(src))
    return _result_of(sim)


# The Pauli each measurement gate measures on its targets (MPP targets carry their own).
_MEASURED_PAULI = {
    "M": "Z",
    "MR": "Z",
    "MX": "X",
    "MRX": "X",
    "MY": "Y",
    "MRY": "Y",
    "MXX": "X",
    "MYY": "Y",
    "MZZ": "Z",
}


def replay_in_stim(src: str, record: str) -> RunResult:
    """Runs a program on Stim, forcing each measurement to its outcome in `record`.

    Raises `AssertionError` if an outcome is impossible, and `NotImplementedError` for
    random noise and noisy measurements, whose randomness isn't in the record. For the
    same reason, programs must not reset qubits that are entangled with others.
    """
    sim = stim.TableauSimulator()
    outcomes = iter(record)
    for inst in _flat_instructions(src):
        gate = stim.gate_data(inst.name)
        args = inst.gate_args_copy()
        if gate.name not in _MEASURED_PAULI and gate.name != "MPP":
            if gate.is_noisy_gate and any(0 < p < 1 for p in args):
                raise NotImplementedError(f"random noise is not supported: {inst}")
            sim.do(inst)
            continue

        if any(args):
            raise NotImplementedError(f"noisy measurements are not supported: {inst}")
        for group in inst.target_groups():
            outcome = next(outcomes, None)
            try:
                sim.postselect_observable(
                    _observable(gate.name, group), desired_value=outcome == "1"
                )
            except ValueError as e:
                raise AssertionError(
                    f"recorded {outcome} is impossible: {inst}\n"
                    f"record: {record}\nprogram:\n{src}"
                ) from e
            sim.do(_measurement(gate.name, group))
    return _result_of(sim)


def _observable(gate: str, group: List[stim.GateTarget]) -> stim.PauliString:
    observable = stim.PauliString(0)
    for t in group:
        pauli = t.pauli_type if gate == "MPP" else _MEASURED_PAULI[gate]
        factor = stim.PauliString({t.value: pauli})
        if t.is_inverted_result_target:
            factor *= -1
        observable *= factor
    return observable


def _measurement(gate: str, group: List[stim.GateTarget]) -> stim.CircuitInstruction:
    if gate == "MPP":
        group = cast(List[stim.GateTarget], stim.target_combined_paulis(group))
    return stim.CircuitInstruction(gate, group)


def _flat_instructions(src: str) -> Iterator[stim.CircuitInstruction]:
    """The instructions of a Stim program, with REPEAT blocks unrolled."""
    for inst in stim.Circuit(src).flattened():
        assert isinstance(inst, stim.CircuitInstruction)
        yield inst


def _result_of(sim: stim.TableauSimulator) -> RunResult:
    tableau = sim.current_inverse_tableau().inverse()  # Stim stores the inverse.
    n = len(tableau)
    return RunResult(
        simulator="stim",
        destabilizers=_paulis(tableau.x_output(k) for k in range(n)),
        stabilizers=_paulis(tableau.z_output(k) for k in range(n)),
        record="".join("1" if m else "0" for m in sim.current_measurement_record()),
    )


def _paulis(paulis: Iterable[stim.PauliString]) -> List[str]:
    """Stim Pauli strings as text, with `I` instead of Stim's `_` for identity."""
    return [str(p).replace("_", "I") for p in paulis]


# ---------------------------------------------------------------------------
# qdk-stim (under test)
# ---------------------------------------------------------------------------


class QdkStimProgram:
    """A Stim program compiled once by qdk-stim, run on the QDK stabilizer simulator.

    Each run is unseeded, so runs are independent and random.
    """

    def __init__(self, src: str):
        self._src = src
        self._qir, self._noise = compile_stim(src)

    def run(self) -> RunResult:
        """Runs one shot."""
        state = run_qir_clifford_and_get_tableau(self._qir, self._noise)
        if state is None:
            raise ValueError(
                f"final qdk-stim state is not a stabilizer state\nprogram:\n{self._src}"
            )
        destabilizers, stabilizers, measurements = state
        record = "".join(
            "0" if m == Result.Zero else "1" if m == Result.One else "L"
            for m in measurements
        )
        return RunResult("qdk-stim", destabilizers, stabilizers, record)


def run_qdk_stim(src: str) -> RunResult:
    """Runs a Stim program once on qdk-stim."""
    return QdkStimProgram(src).run()
