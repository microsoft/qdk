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
    """Abstract base class for trace transformations."""

    @abstractmethod
    def transform(self, trace: Trace) -> Optional[Trace]:
        """Apply this transformation to a trace.

        Args:
            trace (Trace): The input trace.

        Returns:
            Optional[Trace]: The transformed trace, or None if the
                transformation is not applicable.
        """
        ...

    @classmethod
    def q(cls, **kwargs) -> TraceQuery:
        """Create a trace query for this transform type.

        Args:
            **kwargs: Domain overrides for parameter enumeration.

        Returns:
            TraceQuery: A trace query wrapping this transform type.
        """
        return TraceQuery(cls, **kwargs)


@dataclass
class PSSPC(TraceTransform):
    """Pauli-based computation trace transform (PSSPC).

    Converts rotation gates and optionally CCX gates into T-state-based
    operations suitable for lattice surgery resource estimation.

    Attributes:
        num_ts_per_rotation (int): Number of T states used per rotation
            gate. Default is 20.
        ccx_magic_states (bool): If True, CCX gates are treated as magic
            states rather than being decomposed into T gates. Default is
            False.
    """

    _: KW_ONLY
    num_ts_per_rotation: int = field(
        default=20, metadata={"domain": list(range(5, 21))}
    )
    ccx_magic_states: bool = field(default=False)

    def __post_init__(self):
        self._psspc = _PSSPC(self.num_ts_per_rotation, self.ccx_magic_states)

    def transform(self, trace: Trace) -> Optional[Trace]:
        """Apply the PSSPC transformation to a trace.

        Args:
            trace (Trace): The input trace.

        Returns:
            Optional[Trace]: The transformed trace.
        """
        return self._psspc.transform(trace)


@dataclass
class LatticeSurgery(TraceTransform):
    """Lattice surgery trace transform.

    Converts a trace into a form suitable for lattice-surgery-based
    resource estimation.

    Attributes:
        slow_down_factor (float): Multiplicative factor applied to the
            trace depth. Default is 1.0.
    """

    _: KW_ONLY
    slow_down_factor: float = field(default=1.0, metadata={"domain": [1.0]})

    def __post_init__(self):
        self._lattice_surgery = _LatticeSurgery(self.slow_down_factor)

    def transform(self, trace: Trace) -> Optional[Trace]:
        """Apply the lattice surgery transformation to a trace.

        Args:
            trace (Trace): The input trace.

        Returns:
            Optional[Trace]: The transformed trace.
        """
        return self._lattice_surgery.transform(trace)


class _Node(ABC):
    """Abstract base class for trace enumeration nodes."""

    @abstractmethod
    def enumerate(self, ctx: _Context) -> Generator[Trace, None, None]: ...


class TraceQuery(_Node):
    """A query that enumerates transformed traces from an application.

    A trace query chains a sequence of trace transforms, each with optional
    keyword arguments to override their default parameter domains.
    """

    # This is a sequence of trace transforms together with possible kwargs to
    # override their default domains.  The first element might be
    sequence: list[tuple[Type, dict[str, Any]]]

    def __init__(self, t: Type, **kwargs):
        self.sequence = [(t, kwargs)]

    def enumerate(
        self, ctx: _Context, track_parameters: bool = False
    ) -> Generator[Trace | tuple[Any, Trace], None, None]:
        """Enumerate transformed traces from the application context.

        Args:
            ctx (_Context): The application enumeration context.
            track_parameters (bool): If True, yield ``(parameters, trace)``
                tuples instead of plain traces. Default is False.

        Yields:
            Trace | tuple[Any, Trace]: A transformed trace, or a
                ``(parameters, trace)`` tuple when *track_parameters* is True.
        """
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

            for combination in product(*transformer_instances):
                transformed = trace
                for transformer in combination:
                    transformed = transformer.transform(transformed)
                yield (params, transformed) if track_parameters else transformed

    def __mul__(self, other: TraceQuery) -> TraceQuery:
        """Chain another trace query onto this one.

        Args:
            other (TraceQuery): The trace query to append.

        Returns:
            TraceQuery: A new query with the combined transform sequence.

        Raises:
            ValueError: If *other* begins with a None transform.
        """
        new_query = TraceQuery.__new__(TraceQuery)

        if len(other.sequence) > 0 and other.sequence[0][0] is NoneType:
            raise ValueError(
                "Cannot multiply with a TraceQuery that has a None transform at the beginning of its sequence."
            )

        new_query.sequence = self.sequence + other.sequence
        return new_query
