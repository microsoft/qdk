# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import logging
from typing import Any, Dict, List, Literal, Optional, Union
from uuid import uuid4

from qiskit import QuantumCircuit
from qiskit.providers import Options
from qiskit.transpiler.target import Target

from .... import Result, TargetProfile
from .. import OutputSemantics
from ..execution import DetaultExecutor
from ..jobs import QsSimJob, QsJobSet
from .backend_base import BackendBase
from .compilation import Compilation
from .errors import Errors
from .neutral_atom_target import NeutralAtomTarget

logger = logging.getLogger(__name__)


def _bitstring_has_qubit_loss(bitstring: str) -> bool:
    """Return True if the bitstring contains a qubit-loss marker.

    Lost qubits may be represented using non-binary markers (e.g. '-', '2').
    We treat any shot containing those markers as lost-qubit affected.
    """
    return "-" in bitstring or "2" in bitstring


class NeutralAtomBackend(BackendBase):
    """A Qiskit backend that simulates circuits using the NeutralAtomDevice pipeline.

    Circuits are transpiled to OpenQASM 3 using the device's native gate set
    (Rz, SX, CZ), compiled to QIR via the Q# compiler, then run through the
    NeutralAtomDevice compilation and simulation pipeline.
    The device handles single-qubit gate optimization and qubit movement scheduling.
    An optional noise model can be applied to model realistic device behavior.

    The native gate set target ensures Qiskit's transpiler decomposes all non-native
    gates before simulation, so noise configured on native gates (``noise.rz``,
    ``noise.sx``, ``noise.cz``, ``noise.mresetz``) behaves as expected.

    The simulator backend (Clifford, CPU full-state, or GPU full-state) is
    selected automatically unless overridden via the ``simulator_type`` option.

    Example::

        from qiskit import QuantumCircuit
        from qsharp.interop.qiskit import NeutralAtomBackend
        from qsharp._simulation import NoiseConfig

        qc = QuantumCircuit(2)
        qc.h(0)
        qc.cx(0, 1)
        qc.measure_all()

        # Noiseless simulation
        backend = NeutralAtomBackend()
        job = backend.run(qc, shots=1000)
        print(job.result().get_counts())

        # Noisy simulation
        noise = NoiseConfig()
        noise.cz.set_depolarizing(1e-3)
        noise.mresetz.set_bitflip(1e-3)

        job = backend.run(qc, shots=1000, noise=noise, seed=42)
        print(job.result().get_counts())
    """

    def __init__(
        self,
        device=None,
        target: Optional[Target] = None,
        qiskit_pass_options: Optional[Dict[str, Any]] = None,
        transpile_options: Optional[Dict[str, Any]] = None,
        qasm_export_options: Optional[Dict[str, Any]] = None,
        skip_transpilation: bool = False,
        **options,
    ):
        """
        :param device: The NeutralAtomDevice instance to use for compilation and simulation.
            A default-configured device is created automatically if not provided.
            Pass a custom device to control the qubit layout (column count, zone dimensions, etc.).
        :type device: NeutralAtomDevice
        :param target: Qiskit transpiler target. Defaults to the NeutralAtomDevice native
            gate set ``{rz, sx, cz, measure, reset}``. Override only if you need a custom
            decomposition strategy.
        :param qiskit_pass_options: Options forwarded to Qiskit pre-transpilation passes.
        :type qiskit_pass_options: Dict
        :param transpile_options: Options forwarded to ``qiskit.transpile()``.
        :type transpile_options: Dict
        :param qasm_export_options: Options forwarded to the Qiskit QASM3 exporter.
        :type qasm_export_options: Dict
        :param skip_transpilation: Skip Qiskit transpilation. Useful when the circuit is
            already expressed in terms of the target gate set.
        :type skip_transpilation: bool
        :param **options: Default option overrides. These can also be overridden per-call via
            :meth:`run`. Common options:

            - ``name`` (str): Backend name for job metadata. Defaults to the circuit name.
            - ``shots`` (int): Number of shots. Defaults to ``1024``.
            - ``seed`` (int): Random seed for reproducibility. Defaults to ``None``.
            - ``noise`` (NoiseConfig): Optional per-gate noise model. Defaults to ``None`` (noiseless).
            - ``simulator_type`` (str): Simulator to use — ``"clifford"`` (Clifford only),
              ``"cpu"`` (CPU full-state), ``"gpu"`` (GPU full-state), or ``None`` to
              auto-select (GPU if available, CPU otherwise).
            - ``output_semantics`` (OutputSemantics): QIR output encoding. Defaults to ``OutputSemantics.Qiskit``.
            - ``executor``: Executor for async job submission.
        """
        self._device = device
        super().__init__(
            target,
            qiskit_pass_options,
            transpile_options,
            qasm_export_options,
            skip_transpilation,
            **options,
        )

    def _get_device(self):
        """Return the NeutralAtomDevice, creating a default one on first access."""
        if self._device is None:
            from qsharp._device._atom import NeutralAtomDevice

            self._device = NeutralAtomDevice()
        return self._device

    def _build_target(self) -> Target:
        """Return a target restricted to the NeutralAtomDevice native gate set.

        Limiting the target to ``{rz, sx, cz, measure, reset}`` ensures Qiskit's
        transpiler decomposes all non-native gates before QASM3 export, so the
        circuit that reaches the simulator already uses only native gates.
        """
        return NeutralAtomTarget.build_target(num_qubits=None)

    @classmethod
    def _default_options(cls):
        return Options(
            search_path=".",
            shots=1024,
            seed=None,
            noise=None,
            simulator_type=None,
            output_semantics=OutputSemantics.Qiskit,
            executor=DetaultExecutor(),
        )

    def run(
        self,
        run_input: Union[QuantumCircuit, List[QuantumCircuit]],
        **options,
    ) -> Union[QsSimJob, QsJobSet]:
        """Simulate the given circuit(s) using the NeutralAtomDevice pipeline.

        :param run_input: A single ``QuantumCircuit`` or a list of them.
        :param **options: Per-call option overrides. Common options:

            - ``name`` (str): Backend name for job metadata. Defaults to the circuit name.
            - ``shots`` (int): Number of shots. Defaults to ``1024``.
            - ``seed`` (int): Random seed for reproducibility. Defaults to ``None``.
            - ``noise`` (NoiseConfig): Optional per-gate noise model. Defaults to ``None`` (noiseless).
            - ``simulator_type`` (str): Simulator to use — ``"clifford"`` (Clifford only),
              ``"cpu"`` (CPU full-state), ``"gpu"`` (GPU full-state), or ``None`` to
              auto-select (GPU if available, CPU otherwise).
            - ``output_semantics`` (OutputSemantics): QIR output encoding. Defaults to ``OutputSemantics.Qiskit``.
            - ``executor``: Executor for async job submission.
        :return: A job object whose ``.result()`` returns a Qiskit ``Result``.
        :rtype: QsSimJob
        :raises ValueError: If ``run_input`` is not a ``QuantumCircuit`` or list thereof,
            or if a ``target_profile`` other than ``TargetProfile.Base`` is provided.
        """
        run_input = self._validate_quantum_circuits(run_input)
        return self._run(run_input, **options)

    def _map_result_bit(self, v) -> str:
        """Override: unknown values are qubit-loss markers (``"-"``)."""
        if v == Result.One:
            return "1"
        if v == Result.Zero:
            return "0"
        return "-"

    def _execute(self, programs: List[Compilation], **input_params) -> Dict[str, Any]:
        device = self._get_device()

        shots = input_params.get("shots")
        if shots is None:
            raise ValueError(str(Errors.MISSING_NUMBER_OF_SHOTS))

        noise = input_params.get("noise")
        simulator_type: Optional[Literal["clifford", "cpu", "gpu"]] = input_params.get(
            "simulator_type"
        )
        seed: Optional[int] = input_params.get("seed")
        search_path: str = input_params.get("search_path", ".")
        output_semantics = input_params.get("output_semantics")

        # NeutralAtomDevice always requires base-profile QIR — the device's
        # compilation pipeline validates that no conditional branches exist.
        # Raise explicitly if the caller passed a non-Base profile so the
        # error is immediate and clear rather than silently ignored.
        target_profile = input_params.get("target_profile")
        if target_profile is not None and target_profile != TargetProfile.Base:
            raise ValueError(
                "NeutralAtomBackend only supports TargetProfile.Base. "
                "The NeutralAtomDevice compilation pipeline does not support "
                f"conditional branches produced by {target_profile}."
            )

        job_results = []
        for program in programs:
            name = input_params.get("name", program.circuit.name)

            # Compile QASM3 → QIR (base profile).
            qir = self._qasm_to_qir(
                program.qasm,
                name=name,
                target_profile=TargetProfile.Base,
                output_semantics=output_semantics,
                search_path=search_path,
            )

            # Run through NeutralAtomDevice compilation + simulation pipeline.
            sim_results = device.simulate(
                qir,
                shots=shots,
                noise=noise,
                type=simulator_type,
                seed=seed,
            )

            raw_memory = [self._shot_to_bitstring(shot) for shot in sim_results]

            # Separate accepted shots (no loss markers) from raw shots.
            # Qiskit-compatible fields (counts, memory, probabilities)
            # contain only clean {0,1} outcomes; raw_* fields retain the
            # full picture including loss.
            memory = [s for s in raw_memory if not _bitstring_has_qubit_loss(s)]
            accepted_total_count = len(memory)
            raw_total_count = len(raw_memory)

            raw_counts: Dict[str, int] = {}
            counts: Dict[str, int] = {}
            for bs in raw_memory:
                raw_counts[bs] = raw_counts.get(bs, 0) + 1
                if not _bitstring_has_qubit_loss(bs):
                    counts[bs] = counts.get(bs, 0) + 1

            raw_probabilities = (
                {}
                if raw_total_count == 0
                else {bs: c / raw_total_count for bs, c in raw_counts.items()}
            )
            probabilities = (
                {}
                if accepted_total_count == 0
                else {bs: c / accepted_total_count for bs, c in counts.items()}
            )

            job_results.append(
                {
                    "data": {
                        # Qiskit-compatible fields: loss shots excluded.
                        "counts": counts,
                        "probabilities": probabilities,
                        "memory": memory,
                        # Raw fields: all shots, including loss markers.
                        "raw_counts": raw_counts,
                        "raw_probabilities": raw_probabilities,
                        "raw_memory": raw_memory,
                    },
                    "success": True,
                    "header": {
                        "metadata": {"qasm": program.qasm},
                        "name": program.circuit.name,
                        "compilation_time_taken": program.time_taken,
                    },
                    # shots reflects accepted (non-loss) count.
                    "shots": accepted_total_count,
                }
            )

        return {"results": job_results, "qobj_id": str(uuid4()), "success": True}
