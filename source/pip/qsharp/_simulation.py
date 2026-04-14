# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import re
import random
from typing import Callable, Literal, List, Optional, Tuple, TypeAlias, Union
from ._native import (
    QirInstructionId,
    QirInstruction,
    run_clifford,
    run_parallel_shots,
    run_adaptive_parallel_shots,
    run_cpu_full_state,
    NoiseConfig,
    GpuContext,
    try_create_gpu_adapter,
    get_qir_profile,
    parse_base_profile_qir,
    compile_adaptive_program,
)
from ._qsharp import QirInputData, Result
from typing import TYPE_CHECKING

if TYPE_CHECKING:  # This is in the pyi file only
    from ._native import GpuShotResults


def _normalize_input(input: Union[QirInputData, str, bytes]) -> str:
    """Normalize QIR input to text IR string."""
    if isinstance(input, QirInputData):
        return str(input)
    elif isinstance(input, str):
        return input
    else:
        raise ValueError(
            "Bitcode input is not supported without PyQIR. " "Provide text IR instead."
        )


def _prepare_params(
    shots: Optional[int] = 1,
    noise: Optional[NoiseConfig] = None,
    seed: Optional[int] = None,
) -> tuple[int, Optional[NoiseConfig], int]:
    if shots is None:
        shots = 1
    if seed is None:
        seed = random.randint(0, 2**32 - 1)
    if isinstance(noise, tuple):
        raise ValueError(
            "Specifying Pauli noise via a tuple is not supported. Use a NoiseConfig instead."
        )
    return (shots, noise, seed)


def _process_output(output_fmt: str, bitstring: str):
    """Evaluate the output format string against a bitstring of results."""
    return eval(
        output_fmt,
        {
            "o": [
                Result.Zero if x == "0" else Result.One if x == "1" else Result.Loss
                for x in bitstring
            ]
        },
    )


def _build_noise_dict(ir: str, noise_config: NoiseConfig) -> dict:
    """Build noise intrinsics dict by scanning IR for function declarations
    and checking them against the noise config intrinsics table."""
    noise_dict = {}
    intrinsics = noise_config.intrinsics
    for match in re.finditer(r"(?:declare|define)\s+\S+\s+@([^\s(]+)", ir):
        name = match.group(1)
        if name in intrinsics:
            noise_dict[name] = intrinsics.get_intrinsic_id(name)
    return noise_dict


def _decompose_ccx_in_gates(gates: list) -> list:
    """Replace CCX gates with the decomposed gate sequence using H, T, TAdj,
    and CZ gates (needed for GPU simulator compatibility)."""
    result = []
    for gate in gates:
        if (
            isinstance(gate, tuple)
            and len(gate) >= 1
            and gate[0] == QirInstructionId.CCX
        ):
            _, ctrl1, ctrl2, target = gate
            result.extend(
                [
                    (QirInstructionId.H, target),
                    (QirInstructionId.TAdj, ctrl1),
                    (QirInstructionId.TAdj, ctrl2),
                    (QirInstructionId.H, ctrl1),
                    (QirInstructionId.CZ, target, ctrl1),
                    (QirInstructionId.H, ctrl1),
                    (QirInstructionId.T, ctrl1),
                    (QirInstructionId.H, target),
                    (QirInstructionId.CZ, ctrl2, target),
                    (QirInstructionId.H, target),
                    (QirInstructionId.H, ctrl1),
                    (QirInstructionId.CZ, ctrl2, ctrl1),
                    (QirInstructionId.H, ctrl1),
                    (QirInstructionId.T, target),
                    (QirInstructionId.TAdj, ctrl1),
                    (QirInstructionId.H, target),
                    (QirInstructionId.CZ, ctrl2, target),
                    (QirInstructionId.H, target),
                    (QirInstructionId.H, ctrl1),
                    (QirInstructionId.CZ, target, ctrl1),
                    (QirInstructionId.H, ctrl1),
                    (QirInstructionId.TAdj, target),
                    (QirInstructionId.T, ctrl1),
                    (QirInstructionId.H, ctrl1),
                    (QirInstructionId.CZ, ctrl2, ctrl1),
                    (QirInstructionId.H, ctrl1),
                    (QirInstructionId.H, target),
                ]
            )
        else:
            result.append(gate)
    return result


Simulator: TypeAlias = Callable[
    [List[QirInstruction], int, int, int, NoiseConfig, int], str
]


def is_adaptive(ir: str) -> bool:
    """Check if the QIR uses the Adaptive Profile."""
    return get_qir_profile(ir) == "adaptive_profile"


def run_qir_clifford(
    input: Union[QirInputData, str, bytes],
    shots: Optional[int] = 1,
    noise: Optional[NoiseConfig] = None,
    seed: Optional[int] = None,
) -> List:
    ir = _normalize_input(input)
    (shots, noise, seed) = _prepare_params(shots, noise, seed)

    noise_intrinsics = _build_noise_dict(ir, noise) if noise else None
    (gates, num_qubits, num_results, output_fmt) = parse_base_profile_qir(
        ir, noise_intrinsics
    )

    return list(
        map(
            lambda bs: _process_output(output_fmt, bs),
            run_clifford(gates, num_qubits, num_results, shots, noise, seed),
        )
    )


def run_qir_cpu(
    input: Union[QirInputData, str, bytes],
    shots: Optional[int] = 1,
    noise: Optional[NoiseConfig] = None,
    seed: Optional[int] = None,
) -> List:
    ir = _normalize_input(input)
    (shots, noise, seed) = _prepare_params(shots, noise, seed)

    noise_intrinsics = _build_noise_dict(ir, noise) if noise else None
    (gates, num_qubits, num_results, output_fmt) = parse_base_profile_qir(
        ir, noise_intrinsics
    )

    return list(
        map(
            lambda bs: _process_output(output_fmt, bs),
            run_cpu_full_state(gates, num_qubits, num_results, shots, noise, seed),
        )
    )


def str_to_result(result: str):
    match result:
        case "0":
            return Result.Zero
        case "1":
            return Result.One
        case "L":
            return Result.Loss
        case _:
            raise ValueError(f"Invalid result {result}")


def run_qir_gpu(
    input: Union[QirInputData, str, bytes],
    shots: Optional[int] = 1,
    noise: Optional[NoiseConfig] = None,
    seed: Optional[int] = None,
) -> List:
    ir = _normalize_input(input)
    (shots, noise, seed) = _prepare_params(shots, noise, seed)

    if is_adaptive(ir):
        noise_intrinsics = _build_noise_dict(ir, noise) if noise else None
        program = compile_adaptive_program(ir, noise_intrinsics)
        results = run_adaptive_parallel_shots(program, shots, noise, seed)

        # Extract recorded output result indices from the bytecode.
        # OP_RECORD_OUTPUT (0x14) with aux1=0 is result_record_output where
        # src0 is the result index in the results buffer.
        recorded_result_indices = []
        for ins in program["instructions"]:
            if (ins[0] & 0xFF) == 0x14 and ins[5] == 0:
                recorded_result_indices.append(ins[2])
        # Filter shot_results to only include recorded output indices
        filtered = []
        for s in results:
            filtered.append([str_to_result(s[i]) for i in recorded_result_indices])
        return filtered
    else:
        noise_intrinsics = _build_noise_dict(ir, noise) if noise else None
        (gates, num_qubits, num_results, output_fmt) = parse_base_profile_qir(
            ir, noise_intrinsics
        )
        gates = _decompose_ccx_in_gates(gates)
        return list(
            map(
                lambda bs: _process_output(output_fmt, bs),
                run_parallel_shots(gates, shots, num_qubits, num_results, noise, seed),
            )
        )


def prepare_qir_with_correlated_noise(
    input: Union[QirInputData, str, bytes],
    noise_tables: List[Tuple[int, str, int]],
) -> Tuple[List[QirInstruction], int, int]:
    ir = _normalize_input(input)
    noise_dict = {name: table_id for table_id, name, _count in noise_tables}
    (gates, num_qubits, num_results, _) = parse_base_profile_qir(
        ir, noise_dict if noise_dict else None
    )
    gates = _decompose_ccx_in_gates(gates)
    return (gates, num_qubits, num_results)


class GpuSimulator:
    """
    Represents a GPU-based QIR simulator. This is a 'full state' simulator that can simulate
    quantum programs, including non-Clifford gates, up to a limit of 27 qubits.
    """

    def __init__(self):
        self.gpu_context = GpuContext()
        self._is_adaptive = False
        self._recorded_result_indices = []
        self.tables = None

    def load_noise_tables(
        self,
        noise_dir: str,
    ):
        """
        Loads noise tables from the specified directory path. For each .csv file found in the directory,
        the noise table is loaded and associated with a unique identifier. The name of the file (without the .csv extension)
        is used as the label for the noise table, which should match the QIR instruction that will apply noise using this table.

        If testing various noise models, you may load new noise models at any time by calling this method again
        with a different directory path. Previously loaded noise tables will be replaced. The program currently loaded
        into the simulator (if any) will remain loaded, but any subsequent calls to `run_shots` will use the newly loaded noise tables.

        Each line of the table should be of the format: "IXYZ,1.345e-4" where IXYZ is a string of Pauli operators
        representing the error on each qubit (Z applying to the first qubit argument, Y to the second, etc.), and the second value
        is the corresponding error probability for that specific Pauli string.

        Blank lines, lines starting with #, or lines that start with the string "pauli" (i.e., a column header) are ignored.
        """
        self.tables = self.gpu_context.load_noise_tables(noise_dir)

    def set_program(self, input: Union[QirInputData, str, bytes]):
        """
        Load the QIR program into the GPU simulator, preparing it for execution. You may load and run
        multiple programs sequentially by calling this method multiple times before calling `run_shots`
        without needing to create a new simulator instance or reloading noise tables.
        """
        ir = _normalize_input(input)
        if is_adaptive(ir):
            self._is_adaptive = True

            # Build noise_intrinsics dict from loaded noise tables (if any)
            noise_intrinsics = None
            if self.tables is not None:
                noise_intrinsics = {name: table_id for table_id, name, _ in self.tables}
            program = compile_adaptive_program(ir, noise_intrinsics)
            self.gpu_context.set_adaptive_program(program)

            # Extract recorded output result indices from the bytecode.
            # OP_RECORD_OUTPUT (0x14) with aux1=0 is result_record_output where
            # src0 is the result index in the results buffer.
            self._recorded_result_indices = []
            for instr in program["instructions"]:
                if instr[0] & 0xFF == 0x14 and instr[5] == 0:
                    self._recorded_result_indices.append(instr[2])
        else:
            (self.gates, self.required_num_qubits, self.required_num_results) = (
                prepare_qir_with_correlated_noise(
                    input, self.tables if self.tables is not None else []
                )
            )
            self.gpu_context.set_program(
                self.gates, self.required_num_qubits, self.required_num_results
            )

    def run_shots(self, shots: int, seed: Optional[int] = None) -> "GpuShotResults":
        """
        Run the loaded QIR program for the specified number of shots, using an optional seed for reproducibility.
        If noise is to be applied, ensure that noise has been loaded prior to running shots.
        """
        seed = seed if seed is not None else random.randint(0, 2**32 - 1)
        if self._is_adaptive:
            results = self.gpu_context.run_adaptive_shots(shots, seed=seed)
            # Filter shot_results to only include recorded output indices
            if self._recorded_result_indices:
                indices = self._recorded_result_indices
                filtered = []
                for s in results["shot_results"]:
                    filtered.append("".join(s[i] for i in indices))
                results["shot_results"] = filtered
            return results
        return self.gpu_context.run_shots(shots, seed=seed)


def run_qir(
    input: Union[QirInputData, str, bytes],
    shots: Optional[int] = 1,
    noise: Optional[NoiseConfig] = None,
    seed: Optional[int] = None,
    type: Optional[Literal["clifford", "cpu", "gpu"]] = None,
) -> List:
    """
    Simulate the given QIR source.

    Args:
        input: The QIR source to simulate.
        type: The type of simulator to use.
            Use `"clifford"` if your QIR only contains Clifford gates and measurements.
            Use `"gpu"` if you have a GPU available in your system.
            Use `"cpu"` as a fallback option if you don't have a GPU in your system.
            If `None` (default), the GPU simulator will be tried first, falling back to
            CPU if a suitable GPU device could not be located.
        shots: The number of shots to run.
        noise: A noise model to use in the simulation.
        seed: A seed for reproducibility.

    Returns:
        A list of measurement results, in the order they happened during the simulation.
    """
    if type is None:
        try:
            try_create_gpu_adapter()
            type = "gpu"
        except OSError:
            type = "cpu"

    match type:
        case "clifford":
            return run_qir_clifford(input, shots, noise, seed)
        case "cpu":
            return run_qir_cpu(input, shots, noise, seed)
        case "gpu":
            return run_qir_gpu(input, shots, noise, seed)
        case _:
            raise ValueError(f"Invalid simulator type: {type}")
