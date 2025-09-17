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
from ._validate import (
    ValidateAllowedIntrinsics,
    ValidateBeginEndParallel,
    ValidateSingleBlock,
    ValidateCliffordRzAngles,
)


def transform(qir: str | QirInputData, check_clifford: bool = False) -> QirInputData:
    """
    Transform the given QIR module for AC1k by applying a series of smaller transformation, optimization
    and validation passes.

    Args:
        qir (str | QirInputData): The input QIR module as a string or QirInputData object.
        check_clifford (bool): If True, validate that all Rz angles are multiples of π/2.

    Returns:
        QirInputData: The transformed and validated QIR module.
    """
    name = ""
    if isinstance(qir, QirInputData):
        name = qir._name
    module = Module.from_ir(Context(), str(qir))

    OptimizeSingleQubitGates().run(module)
    DecomposeMultiQubitToCZ().run(module)
    OptimizeSingleQubitGates().run(module)
    DecomposeSingleRotationToRz().run(module)
    OptimizeSingleQubitGates().run(module)
    DecomposeSingleQubitToRzSX().run(module)
    OptimizeSingleQubitGates().run(module)
    PruneUnusedFunctions().run(module)
    Reorder().run(module)

    ValidateAllowedIntrinsics().run(module)
    ValidateBeginEndParallel().run(module)
    if check_clifford:
        ValidateSingleBlock().run(module)
        ValidateCliffordRzAngles().run(module)

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
