# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.


from __future__ import annotations

from dataclasses import dataclass
from typing import Callable

from ...estimator import LogicalCounts
from .._qre import Trace
from .._application import Application
from ..interop import trace_from_entry_expr


@dataclass
class QSharpApplication(Application[None]):
    def __init__(self, entry_expr: str | Callable | LogicalCounts):
        self._entry_expr = entry_expr

    def get_trace(self, parameters: None = None) -> Trace:
        return trace_from_entry_expr(self._entry_expr)
