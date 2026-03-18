# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Simulate Cirq circuits on the local NeutralAtomDevice simulator."""

from __future__ import annotations

from typing import Optional, TYPE_CHECKING

import cirq

from ._result import NeutralAtomCirqResult, measurement_dict, to_cirq_result

if TYPE_CHECKING:
    from qsharp._simulation import NoiseConfig
    from qsharp._device._atom import NeutralAtomDevice


def simulate_with_neutral_atom(
    circuit: cirq.Circuit,
    *,
    shots: int = 1000,
    noise: Optional["NoiseConfig"] = None,
    simulator_type: Optional[str] = None,
    seed: Optional[int] = None,
    device: Optional["NeutralAtomDevice"] = None,
    param_resolver: Optional[cirq.ParamResolverOrSimilarType] = None,
) -> NeutralAtomCirqResult:
    """Simulate a Cirq circuit using the local NeutralAtomDevice simulator.

    Pipeline:

    1. ``cirq.Circuit.to_qasm()`` → OpenQASM 2.0
    2. OpenQASM 2.0 → QIR (via the Q# compiler)
    3. QIR → ``NeutralAtomDevice.simulate()`` (compile + schedule + simulate)
    4. Raw results → :class:`NeutralAtomCirqResult`

    Args:
        circuit: The Cirq circuit to simulate.  All gates must support QASM 2.0
            serialization (i.e. the circuit must be serializable via
            ``circuit.to_qasm()``).
        shots: Number of simulation shots. Defaults to 1000.
        noise: Optional :class:`~qsharp._simulation.NoiseConfig` to model
            per-gate noise. ``NeutralAtomDevice.compile()`` decomposes all gates
            to the native set ``{Rz, SX, CZ, MResetZ}``; the exact decomposition
            depends on the gate. For example, a Cirq ``X`` gate (arriving via
            QASM 2.0) is decomposed to ``SX·SX``, not ``Rz+SX``, so
            ``noise.sx`` is the relevant field for X-gate circuits. Configure
            noise on ``noise.rz``, ``noise.sx``, ``noise.cz``, and/or
            ``noise.mresetz`` as appropriate for your circuit.
            Defaults to ``None`` (noiseless).
        simulator_type: Force a particular simulator backend:
            - ``"clifford"`` — Clifford-only, fast. Requires Clifford circuit.
            - ``"cpu"`` — Full state-vector on CPU.
            - ``"gpu"`` — Full state-vector on GPU.
            - ``None`` (default) — GPU if available, CPU as fallback.
        seed: Optional integer seed for reproducibility. Defaults to ``None``.
        device: An existing :class:`~qsharp._device._atom.NeutralAtomDevice`
            instance. A default-configured device (40 columns, 25 register rows)
            is created automatically if ``None``.
        param_resolver: Cirq parameter resolver for the circuit. Defaults to the
            empty resolver ``cirq.ParamResolver({})``.

    Returns:
        A :class:`NeutralAtomCirqResult` (subclass of ``cirq.ResultDict``) with:

        - ``result.measurements``: ``{key: np.ndarray}`` of shape
          ``(accepted_shots, qubits_per_key)`` with ``dtype=np.int8``.
          Loss shots are excluded.
        - ``result.raw_shots``: Full list of raw simulator output for all shots,
          including those with qubit-loss markers.
        - ``result.raw_measurements()``: Same structure as ``measurements`` but
          with ``dtype="<U1"`` so loss markers are preserved.

    Raises:
        ValueError: If ``circuit.to_qasm()`` fails (unsupported gate).

    Example::

        import cirq
        from qsharp.interop.cirq import simulate_with_neutral_atom
        from qsharp._simulation import NoiseConfig

        q0, q1 = cirq.LineQubit.range(2)
        circuit = cirq.Circuit([
            cirq.H(q0),
            cirq.CNOT(q0, q1),
            cirq.measure(q0, q1, key="m"),
        ])

        # Noiseless simulation
        result = simulate_with_neutral_atom(circuit, shots=1000, seed=42)
        print(result.histogram(key="m"))

        # Noisy simulation — apply loss on Rz (native gate)
        noise = NoiseConfig()
        noise.rz.loss = 0.01
        result = simulate_with_neutral_atom(circuit, shots=1000, noise=noise, seed=42)
        print(f"Accepted: {len(result.measurements['m'])} / {len(result.raw_shots)}")
    """
    from qsharp._native import compile_qasm_program_to_qir
    from qsharp._fs import read_file, list_directory, resolve
    from qsharp._http import fetch_github
    from qsharp._qsharp import TargetProfile
    from qsharp._device._atom import NeutralAtomDevice as _NeutralAtomDevice

    if device is None:
        device = _NeutralAtomDevice()

    # Step 1: Cirq circuit → QASM 2.0
    try:
        qasm = circuit.to_qasm()
    except Exception as exc:
        raise ValueError(
            "Failed to convert the Cirq circuit to QASM 2.0. "
            "Ensure every gate in the circuit supports QASM serialization "
            f"(see cirq.Circuit.to_qasm). Original error: {exc}"
        ) from exc

    # Step 2: QASM 2.0 → QIR (base profile)
    qir = compile_qasm_program_to_qir(
        qasm,
        read_file,
        list_directory,
        resolve,
        fetch_github,
        name="cirq_circuit",
        target_profile=TargetProfile.Base,
        search_path=".",
    )

    # Step 3: QIR → simulation (NeutralAtomDevice handles decomposition,
    # scheduling, and simulation internally)
    raw_shots = device.simulate(
        qir,
        shots=shots,
        noise=noise,
        type=simulator_type,
        seed=seed,
    )

    # Step 4: Reconstruct a cirq.ResultDict from the raw shot list
    meas_dict = measurement_dict(circuit)
    return to_cirq_result(raw_shots, meas_dict, param_resolver)
