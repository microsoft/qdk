# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from .._device import Device, Zone, ZoneType
from ..._simulation import clifford_simulation, NoiseConfig, run_qir_gpu
from ..._qsharp import QirInputData

from typing import List, Literal


class AC1000(Device):
    """
    Representation of the Atom Computing AC1000 quantum computer.
    """

    def __init__(self):
        super().__init__(
            36,
            [
                Zone("Register 1", 17, ZoneType.REG),
                Zone("Interaction Zone", 4, ZoneType.INTER),
                Zone("Register 2", 17, ZoneType.REG),
                Zone("Measurement Zone", 4, ZoneType.MEAS),
            ],
        )

    def _init_home_locs(self):
        # Set up the home locations for qubits in the AC1000 layout.
        assert len(self.zones) == 4
        assert (
            self.zones[0].type == ZoneType.REG
            and self.zones[1].type == ZoneType.INTER
            and self.zones[2].type == ZoneType.REG
        )
        assert self.zones[0].row_count == self.zones[2].row_count
        rz1_rows = range(self.zones[0].row_count - 1, -1, -1)
        rz2_rows = range(
            self.zones[0].row_count + self.zones[1].row_count,
            self.zones[0].row_count + self.zones[1].row_count + self.zones[2].row_count,
        )
        self.home_locs = []
        for row in range(self.zones[2].row_count):
            for col in range(self.column_count):
                self.home_locs.append((rz2_rows[row], col))
        for row in range(self.zones[0].row_count):
            for col in range(self.column_count):
                self.home_locs.append((rz1_rows[row], col))

    def compile(
        self,
        program: str | QirInputData,
        verbose: bool = False,
        schedule: bool = False,
    ) -> QirInputData:
        """
        Compile a QIR program for the AC1000 device. This includes decomposing gates to the native gate set,
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
        )
        from ._reorder import Reorder
        from pyqir import Module, Context

        name = ""
        if isinstance(program, QirInputData):
            name = program._name

        if verbose:
            import time

            start_time = time.time()
            all_start_time = start_time
            print(f"Compiling program {name} for AC1000 device...")

        module = Module.from_ir(Context(), str(program))
        if verbose:
            end_time = time.time()
            print(f"  Loaded module in {end_time - start_time:.2f} seconds")
            start_time = end_time

        OptimizeSingleQubitGates().run(module)
        if verbose:
            end_time = time.time()
            print(
                f"  Optimized single qubit gates in {end_time - start_time:.2f} seconds"
            )
            start_time = end_time

        DecomposeMultiQubitToCZ().run(module)
        if verbose:
            end_time = time.time()
            print(
                f"  Decomposed multi-qubit gates to CZ in {end_time - start_time:.2f} seconds"
            )
            start_time = end_time

        OptimizeSingleQubitGates().run(module)
        if verbose:
            end_time = time.time()
            print(
                f"  Optimized single qubit gates in {end_time - start_time:.2f} seconds"
            )
            start_time = end_time

        DecomposeSingleRotationToRz().run(module)
        if verbose:
            end_time = time.time()
            print(
                f"  Decomposed single rotations to Rz in {end_time - start_time:.2f} seconds"
            )
            start_time = end_time

        OptimizeSingleQubitGates().run(module)
        if verbose:
            end_time = time.time()
            print(
                f"  Optimized single qubit gates in {end_time - start_time:.2f} seconds"
            )
            start_time = end_time

        DecomposeSingleQubitToRzSX().run(module)
        if verbose:
            end_time = time.time()
            print(
                f"  Decomposed single qubit gates to Rz and SX in {end_time - start_time:.2f} seconds"
            )
            start_time = end_time

        OptimizeSingleQubitGates().run(module)
        if verbose:
            end_time = time.time()
            print(
                f"  Optimized single qubit gates in {end_time - start_time:.2f} seconds"
            )
            start_time = end_time

        PruneUnusedFunctions().run(module)
        if verbose:
            end_time = time.time()
            print(f"  Pruned unused functions in {end_time - start_time:.2f} seconds")
            start_time = end_time

        Reorder(self).run(module)
        if verbose:
            end_time = time.time()
            print(f"  Reordered instructions in {end_time - start_time:.2f} seconds")
            start_time = end_time

            end_time = time.time()
            print(
                f"Finished compiling program {name} in {end_time - all_start_time:.2f} seconds"
            )

        if schedule:
            from ._validate import ValidateSingleBlock
            from ._scheduler import Schedule

            ValidateSingleBlock().run(module)
            Schedule(self).run(module)

        return QirInputData(name, str(module))

    def trace(self, qir: str | QirInputData):
        """
        Visualize the execution trace of a QIR program on the AC1000 device using the Atoms widget.
        This includes approximate layout and scheduling of the program to show the parallelism of gates and
        movement of qubits during execution.

        :param qir: The QIR program to visualize, either as a string or as QirInputData.
        """

        from qsharp_widgets import Atoms
        from ._trace import Trace
        from ._validate import ValidateSingleBlock
        from ._scheduler import Schedule
        from pyqir import Module, Context
        from IPython.display import display

        # Compile and visualize the trace in one step.
        compiled = self.compile(qir)
        module = Module.from_ir(Context(), str(compiled))
        ValidateSingleBlock().run(module)
        Schedule(self).run(module)
        tracer = Trace(self)
        tracer.run(module)
        display(Atoms(machine_layout=self.get_layout(), trace_data=tracer.trace))

    def simulate(
        self,
        qir: str | QirInputData,
        shots=1,
        noise: NoiseConfig | None = None,
        type: Literal["clifford", "gpu"] = "clifford",
    ) -> List:
        """
        Simulate a QIR program on the AC1000 device. This includes approximate layout and scheduling of the program
        to model the parallelism of gates and movement of qubits during execution. The simulation can optionally
        include noise based on a provided noise configuration.

        :param qir: The QIR program to simulate, either as a string or as QirInputData.
        :param shots: The number of shots to simulate. Defaults to 1.
        :param noise: An optional NoiseConfig to include noise in the simulation.
        :param type: The type of simulation to perform. Currently, only "clifford" is supported.
        :returns: The results of each shot of the simulation as a list.
        """

        from ._validate import ValidateSingleBlock
        from ._scheduler import Schedule
        from ._decomp import DecomposeRzAnglesToCliffordGates
        from pyqir import Module, Context

        if noise is None:
            noise = NoiseConfig()

        compiled = self.compile(qir)
        module = Module.from_ir(Context(), str(compiled))
        ValidateSingleBlock().run(module)
        Schedule(self).run(module)

        if type == "clifford":
            DecomposeRzAnglesToCliffordGates().run(module)
            return clifford_simulation(
                str(module),
                shots,
                noise,
            )

        if type == "gpu":
            return run_qir_gpu(str(module), shots, noise)

        raise ValueError(f"Simulation type {type} is not supported")


__all__ = ["AC1000"]
