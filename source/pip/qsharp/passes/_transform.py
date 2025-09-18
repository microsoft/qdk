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
from ._optimize import OptimizeSingleQubitGates, PruneUnusedFunctions
from ._reorder import Reorder
from ._device import Device, Zone, ZoneType
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
    check_clifford: bool = False,
    verbose: bool = False,
) -> QirInputData:
    """
    Transform the given QIR module for AC1k by applying a series of smaller transformation, optimization
    and validation passes.

    Args:
        qir (str | QirInputData): The input QIR module as a string or QirInputData object.
        device (Device | None): The target device layout. If None, a default device layout for AC1k is used.
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

    PruneUnusedFunctions().run(module)
    if verbose:
        end_time = time.time()
        print(f"PruneUnusedFunctions: {end_time - start_time:.3f} seconds")
        start_time = end_time

    Reorder().run(module)
    if verbose:
        end_time = time.time()
        print(f"Reorder: {end_time - start_time:.3f} seconds")
        start_time = end_time

    if device is None:
        device = Device.ac1k()
    Schedule(device).run(module)
    if verbose:
        end_time = time.time()
        print(f"Schedule: {end_time - start_time:.3f} seconds")
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
        print(f"Total: {end_time - all_start_time:.3f} seconds")

    return QirInputData(name, str(module))


def transform_to_clifford(input: str | QirInputData) -> QirInputData:
    name = ""
    if isinstance(input, QirInputData):
        name = input._name

    input = transform(input, check_clifford=True)

    module = Module.from_ir(Context(), str(input))

    DecomposeRzAnglesToCliffordGates().run(module)
    data = QirInputData(name, str(module))

    return data
