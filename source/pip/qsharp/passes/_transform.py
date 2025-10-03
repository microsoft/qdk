# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pyqir import Module, Context
from .._qsharp import QirInputData
from ._decomp import (
    DecomposeMultiQubitToCZ,
    DecomposeRzAnglesToCliffordGates,
    DecomposeSingleRotationToRz,
    DecomposeSingleQubitToRzSX,
)
from ._optimize import (
    OptimizeSingleQubitGates,
    PruneUnusedFunctions,
    PruneInitializeCalls,
)
from ._reorder import Reorder, PerQubitOrdering
from ._device import Device, AC1000
from ._scheduler import Schedule
from ._validate import (
    ValidateAllowedIntrinsics,
    ValidateBeginEndParallel,
    ValidateSingleBlock,
    ValidateCliffordRzAngles,
)


def transform(
    qir: str | QirInputData,
    device: Device | None = None,
    skip_scheduling: bool = False,
    check_clifford: bool = False,
    verbose: bool = False,
) -> QirInputData:
    """
    Transform the given QIR module for AC1k by applying a series of smaller transformation, optimization
    and validation passes.

    Args:
        qir (str | QirInputData): The input QIR module as a string or QirInputData object.
        device (Device | None): The target device layout. If None, a default device layout for AC1k is used.
        skip_scheduling (bool): If True, skip the scheduling pass.
        check_clifford (bool): If True, validate that all Rz angles are multiples of π/2.
        verbose (bool): If True, print information about each pass duration.

    Returns:
        QirInputData: The transformed and validated QIR module.
    """
    name = ""
    if isinstance(qir, QirInputData):
        name = qir._name

    start_time = None
    all_start_time = None
    if verbose:
        import time

        start_time = time.time()
        all_start_time = start_time

    module = Module.from_ir(Context(), str(qir))
    if verbose:
        end_time = time.time()
        print(f"Initial parse: {end_time - start_time:.3f} seconds")
        start_time = end_time

    OptimizeSingleQubitGates().run(module)
    if verbose:
        end_time = time.time()
        print(f"OptimizeSingleQubitGates: {end_time - start_time:.3f} seconds")
        start_time = end_time

    DecomposeMultiQubitToCZ().run(module)
    if verbose:
        end_time = time.time()
        print(f"DecomposeMultiQubitToCZ: {end_time - start_time:.3f} seconds")
        start_time = end_time

    OptimizeSingleQubitGates().run(module)
    if verbose:
        end_time = time.time()
        print(f"OptimizeSingleQubitGates: {end_time - start_time:.3f} seconds")
        start_time = end_time

    DecomposeSingleRotationToRz().run(module)
    if verbose:
        end_time = time.time()
        print(f"DecomposeSingleRotationToRz: {end_time - start_time:.3f} seconds")
        start_time = end_time

    OptimizeSingleQubitGates().run(module)
    if verbose:
        end_time = time.time()
        print(f"OptimizeSingleQubitGates: {end_time - start_time:.3f} seconds")
        start_time = end_time

    DecomposeSingleQubitToRzSX().run(module)
    if verbose:
        end_time = time.time()
        print(f"DecomposeSingleQubitToRzSX: {end_time - start_time:.3f} seconds")
        start_time = end_time

    OptimizeSingleQubitGates().run(module)
    if verbose:
        end_time = time.time()
        print(f"OptimizeSingleQubitGates: {end_time - start_time:.3f} seconds")
        start_time = end_time

    PruneInitializeCalls().run(module)
    PruneUnusedFunctions().run(module)
    if verbose:
        end_time = time.time()
        print(f"PruneUnusedFunctions: {end_time - start_time:.3f} seconds")
        start_time = end_time

    ValidateAllowedIntrinsics().run(module)
    # ValidateBeginEndParallel().run(module)
    if check_clifford:
        ValidateSingleBlock().run(module)
        ValidateCliffordRzAngles().run(module)
    if verbose:
        end_time = time.time()
        print(f"Validation: {end_time - start_time:.3f} seconds")
        start_time = end_time

    # before = PerQubitOrdering()
    # before.run(module)

    Reorder().run(module)
    if verbose:
        end_time = time.time()
        print(f"Reorder: {end_time - start_time:.3f} seconds")
        start_time = end_time

    # after = PerQubitOrdering()
    # after.run(module)

    # for q, (after_instrs, before_instrs) in enumerate(
    #     zip(after.qubit_instructions, before.qubit_instructions)
    # ):
    #     if before_instrs != after_instrs:
    #         print("Reordering changed the per-qubit instruction order:")
    #         print(f"Qubit {q}:")
    #         print("  Before:")
    #         for instr in before_instrs:
    #             print(f"    {instr}")
    #         print("  After:")
    #         for instr in after_instrs:
    #             print(f"    {instr}")
    #         raise RuntimeError("Reordering changed the per-qubit instruction order")

    if not skip_scheduling:
        if device is None:
            device = AC1000()
        Schedule(device).run(module)

        # scheduled_ops = PerQubitOrdering()
        # scheduled_ops.run(module)
        # for q, (after_instrs, before_instrs) in enumerate(
        #     zip(scheduled_ops.qubit_instructions, after.qubit_instructions)
        # ):
        #     if before_instrs != after_instrs:
        #         print("Scheduling changed the per-qubit instruction order:")
        #         print(f"Qubit {q}:")
        #         print("  Before:")
        #         for instr in before_instrs:
        #             print(f"    {instr}")
        #         print("  After:")
        #         for instr in after_instrs:
        #             print(f"    {instr}")
        #         raise RuntimeError("Scheduling changed the per-qubit instruction order")

        if verbose:
            end_time = time.time()
            print(f"Schedule: {end_time - start_time:.3f} seconds")
            start_time = end_time
            print(f"Total: {end_time - all_start_time:.3f} seconds")

    return QirInputData(name, str(module))


def transform_to_clifford(
    input: str | QirInputData, skip_scheduling: bool = False
) -> QirInputData:
    name = ""
    if isinstance(input, QirInputData):
        name = input._name

    input = transform(input, check_clifford=True, skip_scheduling=skip_scheduling)

    module = Module.from_ir(Context(), str(input))

    DecomposeRzAnglesToCliffordGates().run(module)
    PruneUnusedFunctions().run(module)
    data = QirInputData(name, str(module))

    return data
