# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Internal types that appear in the ``qdk.qre`` public API surface.

.. warning::
    The types re-exported here are **not** part of the supported public API
    and may change in any release without notice.  They are made reachable
    from this module solely so that:

    1. Documentation generators (py2docfx, Sphinx) can emit working
       cross-reference links for return types and parameter types.
    2. Type checkers (pyright, mypy) do not flag references as
       private-module accesses when users annotate variables that hold
       values returned by public functions.
    3. Users who follow a type annotation can land on a clearly-labeled
       page rather than a ``ModuleNotFoundError``.

    Do **not** depend on the presence or shape of any symbol in this
    module.  If you need to construct or configure one of these types
    directly, use the corresponding public API instead.
"""

from ._application import _Context as ApplicationContext
from ._instruction import (
    _InstructionSourceNode as InstructionSourceNode,
    _InstructionSourceNodeReference as InstructionSourceNodeReference,
)
from ._isa_enumeration import (
    _BindingNode as BindingNode,
    _ProductNode as ISAProductNode,
    _SumNode as ISASumNode,
)
from ._qre import Instruction
from ._trace import _Node as TraceNode

try:
    from .interop._cirq import (
        _CirqTraceBuilder as CirqTraceBuilder,
        _QidToTraceId as QidToTraceId,
    )
except ImportError:
    # cirq is an optional dependency; these re-exports are only available
    # when the cirq extra is installed.
    pass

__all__ = [
    "ApplicationContext",
    "BindingNode",
    "CirqTraceBuilder",
    "ISAProductNode",
    "ISASumNode",
    "Instruction",
    "InstructionSourceNode",
    "InstructionSourceNodeReference",
    "QidToTraceId",
    "TraceNode",
]
