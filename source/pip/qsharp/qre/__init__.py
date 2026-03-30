# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from ._application import Application
from ._architecture import Architecture
from ._estimation import estimate
from ._instruction import (
    LOGICAL,
    PHYSICAL,
    Encoding,
    ISATransform,
    constraint,
    InstructionSource,
)
from ._isa_enumeration import ISAQuery, ISARefNode, ISA_ROOT
from ._qre import (
    ISA,
    InstructionFrontier,
    Constraint,
    ConstraintBound,
    EstimationResult,
    FactoryResult,
    ISARequirements,
    Block,
    Trace,
    block_linear_function,
    constant_function,
    generic_function,
    linear_function,
    instruction_name,
    property_name,
    property_name_to_key,
)
from ._results import (
    EstimationTable,
    EstimationTableColumn,
    EstimationTableEntry,
    plot_estimates,
)
from ._trace import LatticeSurgery, PSSPC, TraceQuery, TraceTransform

# Extend Rust Python types with additional Python-side functionality
from ._instruction import _isa_as_frame, _requirements_as_frame

ISA.as_frame = _isa_as_frame
ISARequirements.as_frame = _requirements_as_frame

__all__ = [
    "block_linear_function",
    "constant_function",
    "constraint",
    "estimate",
    "linear_function",
    "plot_estimates",
    "Application",
    "Architecture",
    "Block",
    "Constraint",
    "ConstraintBound",
    "Encoding",
    "EstimationResult",
    "EstimationTable",
    "EstimationTableColumn",
    "EstimationTableEntry",
    "FactoryResult",
    "generic_function",
    "instruction_name",
    "InstructionFrontier",
    "InstructionSource",
    "ISA",
    "ISA_ROOT",
    "ISAQuery",
    "ISARefNode",
    "ISARequirements",
    "ISATransform",
    "LatticeSurgery",
    "PSSPC",
    "property_name",
    "property_name_to_key",
    "Trace",
    "TraceQuery",
    "TraceTransform",
    "LOGICAL",
    "PHYSICAL",
]
