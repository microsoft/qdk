# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Internal types that appear in the ``qdk.qre`` public API surface.

Warning:
    The types exposed here are **not** part of the supported public API
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

from ._application import ApplicationContext, DataclassProtocol
from ._instruction import (
    _InstructionSourceNodeReference as InstructionSourceNodeReference,
)

__all__ = [
    "ApplicationContext",
    "DataclassProtocol",
    "InstructionSourceNodeReference",
]
