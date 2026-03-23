# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Result types and conversion utilities for the Cirq–NeutralAtomDevice integration."""

from __future__ import annotations

import ast
import re
from typing import Any, Dict, List, Optional, Sequence

import cirq
import numpy as np


# ---------------------------------------------------------------------------
# Result type
# ---------------------------------------------------------------------------


class NeutralAtomCirqResult(cirq.ResultDict):
    """A ``cirq.ResultDict`` that also carries raw (loss-inclusive) shot data.

    The inherited ``measurements`` field contains only *accepted* shots - those
    where every measured qubit returned a clean ``{0, 1}`` outcome.  Shots in
    which one or more qubits were lost during the simulation are excluded from
    ``measurements`` but are preserved in ``raw_shots``.

    Attributes:
        raw_shots: The full list of simulation results, one entry per shot,
            in the native simulator output format (tuple, list, or scalar).
            This includes shots that contain qubit-loss markers.

    Methods:
        raw_measurements(): Return the full per-shot data (including loss markers)
            in the same ``{key: 2D-array (shots x bits)}`` format as
            ``measurements``, but with Unicode string dtype so that non-binary
            markers are preserved.
    """

    __slots__ = ("raw_shots", "_measurement_dict_data", "_raw_measurements_cache")

    def __init__(
        self,
        *,
        params: cirq.ParamResolver,
        measurements: Dict[str, np.ndarray],
        raw_shots: List[Any],
        measurement_dict: Dict[str, Sequence[int]],
    ) -> None:
        super().__init__(params=params, measurements=measurements)
        self.raw_shots = raw_shots
        self._measurement_dict_data = measurement_dict
        self._raw_measurements_cache: Optional[Dict[str, Any]] = None

    def raw_measurements(self) -> Dict[str, Any]:
        """Return unfiltered per-shot measurement symbols including loss markers.

        The structure mirrors ``measurements``: ``{key: 2D array (shots x bits)}``,
        but the array dtype is ``"<U1"`` (single Unicode character) so that
        non-binary markers (e.g. ``"-"`` for lost qubits) are preserved.

        The returned arrays should **not** be fed into Cirq tooling that
        assumes ``{0, 1}`` integer bit data.
        """
        if self._raw_measurements_cache is not None:
            return self._raw_measurements_cache

        measurement_dict = self._measurement_dict_data or {"m": []}
        measurement_keys = list(measurement_dict.keys())
        key_lengths = [len(measurement_dict[k]) for k in measurement_keys]

        rows_by_key: Dict[str, List[List[str]]] = {k: [] for k in measurement_keys}

        for shot in self.raw_shots:
            bitstring = _qir_display_to_bitstring(shot)
            registers = _split_registers(bitstring, key_lengths)

            if len(registers) == len(measurement_keys):
                parts = registers
            else:
                flattened = "".join(registers)
                parts = _split_registers(flattened, key_lengths)

            for key_index, key in enumerate(measurement_keys):
                width = key_lengths[key_index]
                if width == 0:
                    rows_by_key[key].append([])
                    continue

                bits = parts[key_index] if key_index < len(parts) else ""
                chars = list(str(bits).strip())
                if len(chars) < width:
                    chars = chars + [""] * (width - len(chars))
                elif len(chars) > width:
                    chars = chars[:width]
                rows_by_key[key].append(chars)

        try:
            raw_meas: Dict[str, Any] = {
                k: np.asarray(v, dtype="<U1") if v else np.zeros((0, 0), dtype="<U1")
                for k, v in rows_by_key.items()
            }
        except Exception:
            raw_meas = rows_by_key  # type: ignore[assignment]

        self._raw_measurements_cache = raw_meas
        return raw_meas


# ---------------------------------------------------------------------------
# Circuit introspection
# ---------------------------------------------------------------------------


def measurement_dict(circuit: cirq.Circuit) -> Dict[str, List[int]]:
    """Extract ``{measurement_key: [global_qubit_indices]}`` from a Cirq circuit.

    Qubit indices are determined by ``sorted(circuit.all_qubits())``, matching
    the ordering that Cirq's ``to_qasm()`` uses when it numbers the qubits.

    Args:
        circuit: The Cirq circuit to introspect.

    Returns:
        An ordered dict mapping each measurement key to the list of global qubit
        indices that key covers, in the order they are measured.
    """
    ordered_qubits = sorted(circuit.all_qubits())
    index_by_qubit = {q: i for i, q in enumerate(ordered_qubits)}

    keys_in_order: List[str] = []
    key_to_qubits: Dict[str, List[int]] = {}

    for op in circuit.all_operations():
        if isinstance(op.gate, cirq.MeasurementGate):
            key = op.gate.key
            if key not in key_to_qubits:
                keys_in_order.append(key)
                key_to_qubits[key] = []
            key_to_qubits[key].extend(index_by_qubit[q] for q in op.qubits)

    return {k: key_to_qubits[k] for k in keys_in_order}


# ---------------------------------------------------------------------------
# Bit-string parsing utilities
# ---------------------------------------------------------------------------


def _qir_display_to_bitstring(obj: Any) -> str:
    """Convert a raw QIR simulation result value to a flat bitstring.

    Handles the various formats the NeutralAtomDevice simulator may emit:
    - ``qsharp.Result`` enum values (``Result.One`` -> ``"1"``, ``Result.Zero`` -> ``"0"``)
    - ``tuple`` - multiple classical registers, joined with spaces
    - ``list``  - single register bits, each element processed recursively
    - ``str``   - already a representation, parsed with ``ast.literal_eval`` if needed
    - other     - converted to string with ``str()``
    """
    # Handle qsharp.Result enum values produced by the local simulator.
    try:
        from qsharp import Result as _Result

        if obj == _Result.One:
            return "1"
        if obj == _Result.Zero:
            return "0"
        if obj == _Result.Loss:
            return "-"
    except ImportError:
        pass

    if isinstance(obj, str) and not re.match(r"[\d\s\-]+$", obj):
        try:
            obj = ast.literal_eval(obj)
        except Exception:
            return str(obj)

    if isinstance(obj, tuple):
        return " ".join(_qir_display_to_bitstring(t) for t in obj)
    if isinstance(obj, list):
        # Recurse per element so Result.One/Zero inside lists are handled correctly.
        return "".join(_qir_display_to_bitstring(bit) for bit in obj)
    return str(obj)


def _split_registers(bitstring: str, key_lengths: List[int]) -> List[str]:
    """Split a flat or space-delimited bitstring into per-register chunks.

    Args:
        bitstring: The raw bitstring, possibly containing spaces between registers.
        key_lengths: The expected width of each register, in order.

    Returns:
        A list of register strings, one per key.
    """
    raw = str(bitstring).strip()

    if " " in raw:
        return raw.split(" ")

    if not key_lengths:
        return [raw]

    total_len = sum(key_lengths)
    if total_len == len(raw):
        regs: List[str] = []
        start = 0
        for length in key_lengths:
            regs.append(raw[start : start + length])
            start += length
        return regs

    return [raw]


# ---------------------------------------------------------------------------
# Loss-filtering shot conversion
# ---------------------------------------------------------------------------


def _shots_to_rows(
    shots: Sequence[Any],
    measurement_dict_data: Optional[Dict[str, Sequence[int]]] = None,
) -> Dict[str, List[List[int]]]:
    """Convert raw simulation shots to ``{key: [[bit_per_shot]]}`` filtering loss.

    Shots where any qubit returned a non-binary value (loss marker) are silently
    dropped. Only ``{0, 1}`` shots contribute to the returned arrays.

    Args:
        shots: Raw simulation output, one entry per shot.
        measurement_dict_data: ``{key: [qubit_indices]}`` - the measurement
            register layout. Defaults to a single key ``"m"`` with no qubits.

    Returns:
        ``{key: list_of_rows}`` where each row is a list of 0/1 integers.
    """
    if measurement_dict_data is None:
        measurement_dict_data = {"m": []}

    measurement_keys = list(measurement_dict_data.keys())
    key_lengths = [len(measurement_dict_data[k]) for k in measurement_keys]

    shots_by_key: Dict[str, List[List[int]]] = {k: [] for k in measurement_keys}

    for shot in shots:
        bitstring = _qir_display_to_bitstring(shot)
        registers = _split_registers(bitstring, key_lengths)

        if len(registers) == len(measurement_keys):
            parts = registers
        else:
            flattened = "".join(registers)
            parts = _split_registers(flattened, key_lengths)

        per_key_rows: Dict[str, List[int]] = {}
        is_valid_shot = True

        for key, bits in zip(measurement_keys, parts):
            bit_chars = list(str(bits).strip())
            if not all(ch in "01" for ch in bit_chars):
                is_valid_shot = False
                break
            per_key_rows[key] = [1 if ch == "1" else 0 for ch in bit_chars]

        if not is_valid_shot:
            continue

        for key in measurement_keys:
            shots_by_key[key].append(per_key_rows.get(key, []))

    return shots_by_key


# ---------------------------------------------------------------------------
# Result construction
# ---------------------------------------------------------------------------


def to_cirq_result(
    raw_shots: List[Any],
    meas_dict: Dict[str, List[int]],
    param_resolver: Optional[cirq.ParamResolverOrSimilarType] = None,
) -> NeutralAtomCirqResult:
    """Build a :class:`NeutralAtomCirqResult` from raw simulation output.

    Args:
        raw_shots: The raw per-shot results from ``NeutralAtomDevice.simulate()``.
        meas_dict: ``{key: [qubit_indices]}`` as returned by :func:`measurement_dict`.
        param_resolver: Cirq parameter resolver for the circuit. Defaults to the
            empty resolver.

    Returns:
        A ``NeutralAtomCirqResult`` whose ``measurements`` field contains only
        loss-free shots, and whose ``raw_shots`` / ``raw_measurements()`` retain
        all shots including those with loss markers.
    """
    if param_resolver is None:
        param_resolver = cirq.ParamResolver({})

    normalized = meas_dict or {"m": []}
    shots_by_key = _shots_to_rows(raw_shots, normalized)
    measurement_keys = list(normalized.keys())

    measurements: Dict[str, np.ndarray] = {}
    for key in measurement_keys:
        rows = shots_by_key.get(key, [])
        if not rows:
            measurements[key] = np.zeros((0, 0), dtype=np.int8)
        else:
            measurements[key] = np.asarray(rows, dtype=np.int8)

    return NeutralAtomCirqResult(
        params=param_resolver,
        measurements=measurements,
        raw_shots=raw_shots,
        measurement_dict=normalized,
    )
