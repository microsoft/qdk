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
    """Application that produces a resource estimation trace from Q# code.

    Accepts a Q# entry expression string, a callable, or pre-computed
    ``LogicalCounts``.

    Attributes:
        cache_dir (Path): Directory for caching compiled traces.
        use_cache (bool): Whether to use the trace cache. Default is False.
    """

    cache_dir: Path = field(
        default=Path.home() / ".cache" / "re3" / "qsharp", repr=False
    )
    use_cache: bool = field(default=False, repr=False)

    def __init__(self, entry_expr: str | Callable | LogicalCounts):
        """Initialize the Q# application.

        Args:
            entry_expr (str | Callable | LogicalCounts): The Q# entry
                expression, a callable returning logical counts, or
                pre-computed logical counts.
        """
        self._entry_expr = entry_expr

    def get_trace(self, parameters: None = None) -> Trace:
        """Return the resource estimation trace for the Q# program.

        Args:
            parameters (None): Unused. Defaults to None.

        Returns:
            Trace: The resource estimation trace.
        """
        # TODO: make caching work for `Callable` as well
        if self.use_cache and isinstance(self._entry_expr, str):
            cache_path = self.cache_dir / f"{self._entry_expr}.json"
        else:
            cache_path = None

        return trace_from_entry_expr_cached(self._entry_expr, cache_path)
