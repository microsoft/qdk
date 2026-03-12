# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from concurrent.futures import Executor
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

logger = logging.getLogger(__name__)


def _result_to_bitstring(value) -> str:
    """Convert a simulation result value to a bitstring.

    Handles the nested structure produced by OutputRecordingPass:
    - tuples (multiple classical registers) -> space-joined parts
    - lists (single register or flat results) -> concatenated bit characters
    - Result.One/Zero -> "1"/"0"
    - Loss -> "-"
    """
    if isinstance(value, tuple):
        return " ".join(_result_to_bitstring(part) for part in value)
    elif isinstance(value, list):
        chars = []
        for v in value:
            if v == Result.One:
                chars.append("1")
            elif v == Result.Zero:
                chars.append("0")
            else:
                chars.append("-")
        return "".join(chars)
    else:
        return str(value)


def _bitstring_has_qubit_loss(bitstring: str) -> bool:
    """Return True if the bitstring contains a qubit-loss marker.

    Lost qubits may be represented using non-binary markers (e.g. '-', '2').
    We treat any shot containing those markers as lost-qubit affected.
    """
    return "-" in bitstring or "2" in bitstring


class NeutralAtomBackend(BackendBase):
    """A Qiskit backend that simulates circuits using the NeutralAtomDevice pipeline.

    Circuits are transpiled to OpenQASM 3, compiled to QIR via the Q# compiler,
    then run through the NeutralAtomDevice compilation and simulation pipeline.
    The device handles gate decomposition to native gate sets (Rz, SX, CZ),
    single-qubit gate optimization, and qubit movement scheduling. An optional
    noise model can be applied to model realistic device behavior.

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
        **fields,
    ):
        """
        Parameters:
            device (NeutralAtomDevice, optional): The NeutralAtomDevice instance to use
                for compilation and simulation. A default-configured device is created
                automatically if not provided. Pass a custom device to control the
                qubit layout (column count, zone dimensions, etc.).
            target (Target): Qiskit transpiler target. Derived from ``target_profile``
                if not provided.
            qiskit_pass_options (Dict): Options forwarded to Qiskit pre-transpilation
                passes (e.g. barrier/delay removal).
            transpile_options (Dict): Options forwarded to ``qiskit.transpile()``.
            qasm_export_options (Dict): Options forwarded to the Qiskit QASM3 exporter.
            skip_transpilation (bool): Skip Qiskit transpilation. Useful when the
                circuit is already expressed in terms of the target gate set.
            **fields: Additional backend options. Common options:

                - ``shots`` (int): Number of shots. Defaults to 1024.
                - ``seed`` (int): Random seed for reproducibility. Defaults to None.
                - ``noise`` (NoiseConfig): Optional per-gate noise model. Defaults to
                  None (noiseless).
                - ``simulator_type`` (str): Simulator to use — ``"clifford"`` (Clifford
                  only), ``"cpu"`` (CPU full-state), ``"gpu"`` (GPU full-state), or
                  None to auto-select (GPU if available, CPU otherwise).
                - ``target_profile`` (TargetProfile): Must be ``Base``. Defaults to
                  ``Base``.
                - ``output_semantics`` (OutputSemantics): QIR output encoding. Defaults
                  to ``OpenQasm``.
                - ``executor``: Executor for async job submission.
        """
        self._device = device
        super().__init__(
            target,
            qiskit_pass_options,
            transpile_options,
            qasm_export_options,
            skip_transpilation,
            **fields,
        )

    def _get_device(self):
        """Return the NeutralAtomDevice, creating a default one on first access."""
        if self._device is None:
            from qsharp._device._atom import NeutralAtomDevice

            self._device = NeutralAtomDevice()
        return self._device

    @classmethod
    def _default_options(cls):
        return Options(
            name="program",
            search_path=".",
            shots=1024,
            seed=None,
            noise=None,
            simulator_type=None,
            target_profile=TargetProfile.Base,
            output_semantics=OutputSemantics.OpenQasm,
            executor=DetaultExecutor(),
        )

    def run(
        self,
        run_input: Union[QuantumCircuit, List[QuantumCircuit]],
        **options,
    ) -> Union[QsSimJob, QsJobSet]:
        """Simulate the given circuit(s) using the NeutralAtomDevice pipeline.

        Args:
            run_input: A single ``QuantumCircuit`` or a list of them.
            **options: Per-call option overrides (``shots``, ``seed``, ``noise``,
                ``simulator_type``, ``target_profile``, etc.). See class docstring
                for the full list.

        Returns:
            QsSimJob: A job object whose ``.result()`` returns a Qiskit ``Result``.

        Raises:
            ValueError: If ``run_input`` is not a ``QuantumCircuit`` or list thereof,
                or if ``target_profile`` is set to ``Unrestricted``.
        """
        if not isinstance(run_input, list):
            run_input = [run_input]
        for circuit in run_input:
            if not isinstance(circuit, QuantumCircuit):
                raise ValueError(str(Errors.INPUT_MUST_BE_QC))
        return self._run(run_input, **options)

    def _execute(self, programs: List[Compilation], **input_params) -> Dict[str, Any]:
        device = self._get_device()

        shots = input_params.get("shots", 1024)
        if shots is None:
            raise ValueError(str(Errors.MISSING_NUMBER_OF_SHOTS))

        noise = input_params.get("noise", None)
        simulator_type: Optional[Literal["clifford", "cpu", "gpu"]] = input_params.get(
            "simulator_type", None
        )
        seed: Optional[int] = input_params.get("seed", None)
        search_path: str = input_params.get("search_path", ".")
        target_profile = input_params.get("target_profile", TargetProfile.Base)
        output_semantics = input_params.get(
            "output_semantics", OutputSemantics.OpenQasm
        )

        if target_profile == TargetProfile.Unrestricted:
            raise ValueError(str(Errors.UNRESTRICTED_INVALID_QIR_TARGET))

        job_results = []
        for program in programs:
            name = input_params.get("name", program.circuit.name)

            # Compile QASM3 → QIR (base profile).
            qir = self._qasm_to_qir(
                program.qasm,
                name=name,
                target_profile=target_profile,
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

            raw_memory = [_result_to_bitstring(shot) for shot in sim_results]

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

        return {
            "results": job_results,
            "qobj_id": str(uuid4()),
            "success": True,
        }

    def _create_results(self, output: Dict[str, Any]) -> Any:
        from qiskit.result import Result

        return Result.from_dict(output)

    def _submit_job(
        self, run_input: List[QuantumCircuit], **options
    ) -> Union[QsSimJob, QsJobSet]:
        job_id = str(uuid4())
        executor: Executor = options.pop("executor", DetaultExecutor())
        if len(run_input) == 1:
            job = QsSimJob(self, job_id, self.run_job, run_input, options, executor)
        else:
            job = QsJobSet(self, job_id, self.run_job, run_input, options, executor)
        job.submit()
        return job
