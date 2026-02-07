# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations
from abc import ABC, abstractmethod
from dataclasses import dataclass, KW_ONLY, field
from itertools import product
from typing import Any, Optional, Generator, Type
from ._application import _Context
from ._enumeration import _enumerate_instances
from ._qre import PSSPC as _PSSPC, LatticeSurgery as _LatticeSurgery, Trace


class TraceTransform(ABC):
    @abstractmethod
    def transform(self, trace: Trace) -> Optional[Trace]: ...

    @classmethod
    def q(cls, **kwargs) -> TraceQuery:
        return TraceQuery(cls, **kwargs)


@dataclass
class PSSPC(TraceTransform):
    _: KW_ONLY
    num_ts_per_rotation: int = field(
        default=20, metadata={"domain": list(range(1, 21))}
    )
    ccx_magic_states: bool = field(default=False)

    def __post_init__(self):
        self._psspc = _PSSPC(self.num_ts_per_rotation, self.ccx_magic_states)

    def transform(self, trace: Trace) -> Optional[Trace]:
        return self._psspc.transform(trace)


@dataclass
class LatticeSurgery(TraceTransform):
    _: KW_ONLY
    slow_down_factor: float = field(default=1.0, metadata={"domain": [1.0]})

    def __post_init__(self):
        self._lattice_surgery = _LatticeSurgery(self.slow_down_factor)

    def transform(self, trace: Trace) -> Optional[Trace]:
        return self._lattice_surgery.transform(trace)


class _Node(ABC):
    @abstractmethod
    def enumerate(self, ctx: _Context) -> Generator[Trace, None, None]: ...


class RootNode(_Node):
    # NOTE: this might be redundant with TransformationNode with an empty sequence
    def enumerate(self, ctx: _Context) -> Generator[Trace, None, None]:
        yield from ctx.application.enumerate_traces(**ctx.kwargs)


class TraceQuery(_Node):
    sequence: list[tuple[Type, dict[str, Any]]]

    def __init__(self, t: Type, **kwargs):
        self.sequence = [(t, kwargs)]

    def enumerate(self, ctx: _Context) -> Generator[Trace, None, None]:
        for trace in ctx.application.enumerate_traces(**ctx.kwargs):
            if not self.sequence:
                yield trace
                continue

            transformer_instances = []

            for t, transformer_kwargs in self.sequence:
                instances = _enumerate_instances(t, **transformer_kwargs)
                transformer_instances.append(instances)

            # TODO: make parallel
            for sequence in product(*transformer_instances):
                transformed = trace
                for transformer in sequence:
                    transformed = transformer.transform(transformed)
                yield transformed

    def __mul__(self, other: TraceQuery) -> TraceQuery:
        new_query = TraceQuery.__new__(TraceQuery)
        new_query.sequence = self.sequence + other.sequence
        return new_query
