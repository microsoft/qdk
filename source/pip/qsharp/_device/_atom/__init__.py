# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from .._device import Device, Zone, ZoneType
from ..._simulation import NoiseConfig, run_qir_clifford, run_qir_cpu, run_qir_gpu
from ..._native import try_create_gpu_adapter
from ..._qsharp import QirInputData
from ... import telemetry_events

from typing import List, Literal, Optional
import time


class NeutralAtomDevice(Device):
    """
    Representation of a neutral atom device quantum computer.
    """

    def __init__(
        self,
        column_count: int = 40,
        register_zone_row_count: int = 25,
        interaction_zone_row_count: int = 2,
        measurement_zone_row_count: int = 2,
    ):
        default_layout = (
            column_count == 40
            and register_zone_row_count == 25
            and interaction_zone_row_count == 2
            and measurement_zone_row_count == 2
        )
        telemetry_events.on_neutral_atom_init(default_layout)

        super().__init__(
            column_count,
            [
                Zone("Register 1", register_zone_row_count, ZoneType.REG),
                Zone("Interaction Zone", interaction_zone_row_count, ZoneType.INTER),
                Zone("Measurement Zone", measurement_zone_row_count, ZoneType.MEAS),
            ],
        )

    def _init_home_locs(self):
        # Set up the home locations for qubits in the NeutralAtomDevice layout.
        assert len(self.zones) == 3
        assert (
            self.zones[0].type == ZoneType.REG
            and self.zones[1].type == ZoneType.INTER
            and self.zones[2].type == ZoneType.MEAS
        )
        rz1_rows = range(self.zones[0].row_count - 1, -1, -1)
        self.home_locs = []
        for row in range(self.zones[0].row_count):
            for col in range(self.column_count):
                self.home_locs.append((rz1_rows[row], col))

    def compile(
        self,
        program: str | QirInputData,
        verbose: bool = False,
    ) -> QirInputData:
        """
        Compile a QIR program for the NeutralAtomDevice device. This includes decomposing gates to the native gate set,
        optimizing sequences of single qubit gates, pruning unused functions, and reordering instructions to
        enable better scheduling during execution.

        :param program: The QIR program to compile, either as a string or as QirInputData.
        :param verbose: If true, print detailed information about each compilation step.
        :returns QirInputData: The compiled QIR program.
        """

        from ._optimize import (
            OptimizeSingleQubitGates,
            PruneUnusedFunctions,
        )
        from ._decomp import (
            DecomposeMultiQubitToCZ,
            DecomposeSingleRotationToRz,
            DecomposeSingleQubitToRzSX,
            ReplaceResetWithMResetZ,
        )
        from ._reorder import Reorder
        from pyqir import Module, Context

        start_time = time.monotonic()
        all_start_time = start_time
        telemetry_events.on_neutral_atom_compile()

        name = ""
        if isinstance(program, QirInputData):
            name = program._name

        if verbose:
            print(f"Compiling program {name} for NeutralAtomDevice device...")

        module = Module.from_ir(Context(), str(program))
        if verbose:
            end_time = time.monotonic()
            print(f"  Loaded module in {end_time - start_time:.2f} seconds")
            start_time = end_time

        OptimizeSingleQubitGates().run(module)
        if verbose:
            end_time = time.monotonic()
            print(
                f"  Optimized single qubit gates in {end_time - start_time:.2f} seconds"
            )
            start_time = end_time

        DecomposeMultiQubitToCZ().run(module)
        if verbose:
            end_time = time.monotonic()
            print(
                f"  Decomposed multi-qubit gates to CZ in {end_time - start_time:.2f} seconds"
            )
            start_time = end_time

        OptimizeSingleQubitGates().run(module)
        if verbose:
            end_time = time.monotonic()
            print(
                f"  Optimized single qubit gates in {end_time - start_time:.2f} seconds"
            )
            start_time = end_time

        DecomposeSingleRotationToRz().run(module)
        if verbose:
            end_time = time.monotonic()
            print(
                f"  Decomposed single rotations to Rz in {end_time - start_time:.2f} seconds"
            )
            start_time = end_time

        OptimizeSingleQubitGates().run(module)
        if verbose:
            end_time = time.monotonic()
            print(
                f"  Optimized single qubit gates in {end_time - start_time:.2f} seconds"
            )
            start_time = end_time

        DecomposeSingleQubitToRzSX().run(module)
        if verbose:
            end_time = time.monotonic()
            print(
                f"  Decomposed single qubit gates to Rz and SX in {end_time - start_time:.2f} seconds"
            )
            start_time = end_time

        OptimizeSingleQubitGates().run(module)
        if verbose:
            end_time = time.monotonic()
            print(
                f"  Optimized single qubit gates in {end_time - start_time:.2f} seconds"
            )
            start_time = end_time

        ReplaceResetWithMResetZ().run(module)
        if verbose:
            end_time = time.monotonic()
            print(
                f"  Replaced resets with mresetz in {end_time - start_time:.2f} seconds"
            )
            start_time = end_time

        PruneUnusedFunctions().run(module)
        if verbose:
            end_time = time.monotonic()
            print(f"  Pruned unused functions in {end_time - start_time:.2f} seconds")
            start_time = end_time

        Reorder(self).run(module)
        if verbose:
            end_time = time.monotonic()
            print(f"  Reordered instructions in {end_time - start_time:.2f} seconds")
            start_time = end_time

        end_time = time.monotonic()
        telemetry_events.on_neutral_atom_compile_end((end_time - all_start_time) * 1000)
        if verbose:
            print(
                f"Finished compiling program {name} in {end_time - all_start_time:.2f} seconds"
            )

        return QirInputData(name, str(module))

    def show_trace(self, qir: str | QirInputData):
        """
        Visualize the execution trace of a QIR program on the NeutralAtomDevice device using the Atoms widget.
        This includes approximate layout and scheduling of the program to show the parallelism of gates and
        movement of qubits during execution.

        :param qir: The QIR program to visualize, either as a string or as QirInputData.
        """

        try:
            from qsharp_widgets import Atoms
        except ImportError:
            raise ImportError(
                "The qsharp-widgets package is required for showing atom trace visualization. "
                "Please install it via 'pip install \"qdk[jupyter]\"' or 'pip install qsharp-widgets'."
            )
        from ._trace import Trace
        from ._validate import ValidateNoConditionalBranches
        from ._scheduler import Schedule
        from pyqir import Module, Context
        from IPython.display import display

        start_time = time.monotonic()
        telemetry_events.on_neutral_atom_trace()

        # Compile and visualize the trace in one step.
        compiled = self.compile(qir)
        module = Module.from_ir(Context(), str(compiled))
        ValidateNoConditionalBranches().run(module)
        Schedule(self).run(module)
        tracer = Trace(self)
        tracer.run(module)
        display(Atoms(machine_layout=self.get_layout(), trace_data=tracer.trace))

        end_time = time.monotonic()
        telemetry_events.on_neutral_atom_trace_end((end_time - start_time) * 1000)

    def simulate(
        self,
        qir: str | QirInputData,
        shots=1,
        noise: NoiseConfig | None = None,
        type: Optional[Literal["clifford", "cpu", "gpu"]] = None,
    ) -> List:
        """
        Simulate a QIR program on the NeutralAtomDevice device. This includes approximate layout and scheduling of the program
        to model the parallelism of gates and movement of qubits during execution. The simulation can optionally
        include noise based on a provided noise configuration.

        :param qir: The QIR program to simulate, either as a string or as QirInputData.
        :param shots: The number of shots to simulate. Defaults to 1.
        :param noise: An optional NoiseConfig to include noise in the simulation.
        :param type: The type of simulator to use:
            Use `"clifford"` if your QIR only contains Clifford gates and measurements.
            Use `"gpu"` if you have a GPU available in your system.
            Use `"cpu"` as a fallback option if you don't have a GPU in your system.
            If `None` (default), the GPU simulator will be tried first, falling back to
            CPU if a suitable GPU device could not be located.
        :returns: The results of each shot of the simulation as a list.
        """

        from ._validate import ValidateNoConditionalBranches
        from ._scheduler import Schedule
        from ._decomp import DecomposeRzAnglesToCliffordGates
        from pyqir import Module, Context

        start_time = time.monotonic()

        using_noise = noise is not None
        if noise is None:
            noise = NoiseConfig()

        compiled = self.compile(qir)
        module = Module.from_ir(Context(), str(compiled))
        ValidateNoConditionalBranches().run(module)
        Schedule(self).run(module)

        if type is None:
            try:
                try_create_gpu_adapter()
                type = "gpu"
            except OSError:
                telemetry_events.on_neutral_atom_cpu_fallback()
                type = "cpu"

        telemetry_events.on_neutral_atom_simulate(shots, using_noise, type)

        match type:
            case "clifford":
                DecomposeRzAnglesToCliffordGates().run(module)
                result = run_qir_clifford(
                    str(module),
                    shots,
                    noise,
                )
            case "cpu":
                result = run_qir_cpu(str(module), shots, noise)
            case "gpu":
                result = run_qir_gpu(str(module), shots, noise)
            case _:
                raise ValueError(f"Simulation type {type} is not supported")

        end_time = time.monotonic()
        telemetry_events.on_neutral_atom_simulate_end(
            (end_time - start_time) * 1000, shots, using_noise, type
        )
        return result


__all__ = ["NeutralAtomDevice"]
