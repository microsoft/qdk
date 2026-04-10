# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""NeutralAtomSampler — a cirq.Sampler backed by the local NeutralAtomDevice."""

from __future__ import annotations

from typing import List, Optional, TYPE_CHECKING

import cirq

from ._result import NeutralAtomCirqResult, measurement_dict, to_cirq_result

if TYPE_CHECKING:
    from qsharp._simulation import NoiseConfig
    from qsharp._device._atom import NeutralAtomDevice


class NeutralAtomSampler(cirq.Sampler):
    """A ``cirq.Sampler`` that runs Cirq circuits on the local NeutralAtomDevice simulator.

    This sampler integrates with the standard Cirq sampler protocol, so it can
    be used anywhere a ``cirq.Sampler`` is expected.

    Pipeline for each ``run()`` call:

    1. ``cirq.Circuit.to_qasm(version="3.0")`` → OpenQASM 3.0
    2. OpenQASM 3.0 → QIR (base profile, via the Q# compiler)
    3. QIR → ``NeutralAtomDevice.simulate()`` (decompose, schedule, simulate)
    4. Raw shots → :class:`NeutralAtomCirqResult`

    :param noise: Optional :class:`~qsharp._simulation.NoiseConfig` describing
        per-gate noise. The device decomposes gates to the native set
        ``{Rz, SX, CZ, MResetZ}``; configure noise on those native gates.
        For example, a Cirq ``X`` gate arriving via QASM 2.0 is decomposed
        to ``SX·SX``, so ``noise.sx`` is the relevant field. Defaults to
        ``None`` (noiseless).
    :param simulator_type: Force a particular simulator backend.
        ``"clifford"`` — Clifford-only, fast. Requires a Clifford circuit.
        ``"cpu"`` — Full state-vector on CPU.
        ``"gpu"`` — Full state-vector on GPU.
        ``None`` (default) — GPU if available, CPU otherwise.
    :param seed: Optional integer seed for reproducibility. Defaults to ``None``.
    :param device: An existing :class:`~qsharp._device._atom.NeutralAtomDevice`
        instance to reuse across calls. A default-configured device is
        created lazily on the first call when not provided.

    Example::

        import cirq
        from qsharp.interop.cirq import NeutralAtomSampler
        from qsharp._simulation import NoiseConfig

        q0, q1 = cirq.LineQubit.range(2)
        circuit = cirq.Circuit([
            cirq.H(q0),
            cirq.CNOT(q0, q1),
            cirq.measure(q0, q1, key="m"),
        ])

        # Noiseless simulation
        sampler = NeutralAtomSampler(seed=42)
        result = sampler.run(circuit, repetitions=1000)
        print(result.histogram(key="m"))

        # Noisy simulation — 1% loss on Rz (native gate)
        noise = NoiseConfig()
        noise.rz.loss = 0.01
        sampler = NeutralAtomSampler(noise=noise, seed=42)
        result = sampler.run(circuit, repetitions=1000)
        print(f"Accepted: {len(result.measurements['m'])} / {len(result.raw_shots)}")
    """

    def __init__(
        self,
        *,
        noise: Optional["NoiseConfig"] = None,
        simulator_type: Optional[str] = None,
        seed: Optional[int] = None,
        device: Optional["NeutralAtomDevice"] = None,
    ) -> None:
        self._noise = noise
        self._simulator_type = simulator_type
        self._seed = seed
        self._device = device

    def _get_device(self) -> "NeutralAtomDevice":
        """Return the NeutralAtomDevice, creating a default one on first access."""
        if self._device is None:
            from qsharp._device._atom import NeutralAtomDevice

            self._device = NeutralAtomDevice()
        return self._device

    def run_sweep(
        self,
        program: cirq.AbstractCircuit,
        params: cirq.Sweepable,
        repetitions: int = 1,
    ) -> List[NeutralAtomCirqResult]:
        """Run the circuit for each parameter resolver in the sweep.

        :param program: The Cirq circuit to simulate.
        :type program: cirq.AbstractCircuit
        :param params: A :class:`cirq.Sweepable` defining the parameter resolvers
            to sweep over. Each resolver produces one result.
        :type params: cirq.Sweepable
        :param repetitions: Number of shots per parameter resolver.
        :type repetitions: int
        :return: A list of :class:`NeutralAtomCirqResult` objects, one per resolver.
        :rtype: List[NeutralAtomCirqResult]
        """
        resolvers = list(cirq.to_sweep(params)) if params is not None else [cirq.ParamResolver()]
        return [
            self._run_once(program, resolver, repetitions) for resolver in resolvers
        ]

    def _run_once(
        self,
        circuit: cirq.AbstractCircuit,
        param_resolver: cirq.ParamResolver,
        repetitions: int,
    ) -> NeutralAtomCirqResult:
        from qsharp._native import compile_qasm_program_to_qir
        from qsharp._fs import read_file, list_directory, resolve
        from qsharp._http import fetch_github
        from qsharp._qsharp import TargetProfile

        # Resolve parameters
        resolved_circuit = cirq.resolve_parameters(circuit, param_resolver)

        # Step 1: Cirq circuit → QASM 3.0
        try:
            qasm = resolved_circuit.to_qasm(version="3.0")
        except Exception as exc:
            raise ValueError(
                "Failed to convert the Cirq circuit to QASM 3.0. "
                "Ensure every gate in the circuit supports QASM serialization "
                f"(see cirq.Circuit.to_qasm). Original error: {exc}"
            ) from exc

        # Step 2: QASM 3.0 → QIR (base profile)
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

        # Step 3: QIR → NeutralAtomDevice simulation
        device = self._get_device()
        raw_shots = device.simulate(
            qir,
            shots=repetitions,
            noise=self._noise,
            type=self._simulator_type,
            seed=self._seed,
        )

        # Step 4: Build NeutralAtomCirqResult
        meas_dict = measurement_dict(resolved_circuit)
        return to_cirq_result(raw_shots, meas_dict, param_resolver)
