# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations
from abc import ABC, abstractmethod
from dataclasses import dataclass, KW_ONLY, field
from itertools import product
from types import NoneType
from typing import Any, Optional, Generator, Type, TYPE_CHECKING

if TYPE_CHECKING:
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
        default=20, metadata={"domain": list(range(5, 21))}
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


class TraceQuery(_Node):
    # This is a sequence of trace transforms together with possible kwargs to
    # override their default domains.  The first element might be
    sequence: list[tuple[Type, dict[str, Any]]]

    def __init__(self, t: Type, **kwargs):
        self.sequence = [(t, kwargs)]

    def enumerate(
        self, ctx: _Context, track_parameters: bool = False
    ) -> Generator[Trace | tuple[Any, Trace], None, None]:
        sequence = self.sequence
        kwargs = {}
        if len(sequence) > 0 and sequence[0][0] is NoneType:
            kwargs = sequence[0][1]
            sequence = sequence[1:]

        if track_parameters:
            source = ctx.application.enumerate_traces_with_parameters(**kwargs)
        else:
            source = ((None, t) for t in ctx.application.enumerate_traces(**kwargs))

        for params, trace in source:
            if not sequence:
                yield (params, trace) if track_parameters else trace
                continue

            transformer_instances = []

            for t, transformer_kwargs in sequence:
                instances = _enumerate_instances(t, **transformer_kwargs)
                transformer_instances.append(instances)

            # TODO: make parallel
            for combination in product(*transformer_instances):
                transformed = trace
                for transformer in combination:
                    transformed = transformer.transform(transformed)
                yield (params, transformed) if track_parameters else transformed

    def __mul__(self, other: TraceQuery) -> TraceQuery:
        new_query = TraceQuery.__new__(TraceQuery)

        if len(other.sequence) > 0 and other.sequence[0][0] is NoneType:
            raise ValueError(
                "Cannot multiply with a TraceQuery that has a None transform at the beginning of its sequence."
            )

        new_query.sequence = self.sequence + other.sequence
        return new_query
