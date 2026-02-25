# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.


from __future__ import annotations

from pathlib import Path
from dataclasses import dataclass, field
from typing import Callable

from ...estimator import LogicalCounts
from .._qre import Trace
from .._application import Application
from ..interop import trace_from_entry_expr_cached


@dataclass
class QSharpApplication(Application[None]):
    cache_dir: Path = field(
        default=Path.home() / ".cache" / "re3" / "qsharp", repr=False
    )
    use_cache: bool = field(default=False, repr=False)

    def __init__(self, entry_expr: str | Callable | LogicalCounts):
        self._entry_expr = entry_expr

    def get_trace(self, parameters: None = None) -> Trace:
        # TODO: make caching work for `Callable` as well
        if self.use_cache and isinstance(self._entry_expr, str):
            cache_path = self.cache_dir / f"{self._entry_expr}.json"
        else:
            cache_path = None

        return trace_from_entry_expr_cached(self._entry_expr, cache_path)
