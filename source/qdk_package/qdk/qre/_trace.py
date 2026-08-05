# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations
from abc import ABC, abstractmethod
from dataclasses import dataclass, KW_ONLY, field
from enum import IntEnum
from itertools import product
from types import NoneType
from typing import Any, Optional, Generator, Type, TYPE_CHECKING

if TYPE_CHECKING:
    from ._application import ApplicationContext
from ._enumeration import _enumerate_instances
from ._qre import (
    PSSPC as _PSSPC,
    LatticeSurgery as _LatticeSurgery,
    DynamicMemoryCompute as _DynamicMemoryCompute,
    Unmemory as _Unmemory,
    Trace,
)


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
    def q(cls, **kwargs: Any) -> TraceQuery:
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


class EvictionStrategy(IntEnum):
    FIRST_AVAILABLE = 0
    LEAST_RECENTLY_USED = 1
    LEAST_FREQUENTLY_USED = 2


@dataclass
class DynamicMemoryCompute(TraceTransform):
    """Dynamic memory-compute trace transform.

    Splits qubits into a limited compute area and a memory area,
    inserting ``READ_FROM_MEMORY`` and ``WRITE_TO_MEMORY`` operations
    as needed so that at most a fraction of the original qubits reside
    in the compute area at any time.

    Attributes:
        compute_capacity_percentage (float): Fraction (0.0–1.0) of the
            input trace's compute qubits to keep in the compute area.
            Default is 0.5.
    """

    _: KW_ONLY
    compute_capacity_percentage: float = field(
        default=0.5,
        metadata={"domain": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]},
    )
    eviction_strategy: EvictionStrategy = field(
        default=EvictionStrategy.LEAST_RECENTLY_USED,
        metadata={"domain": [EvictionStrategy.LEAST_RECENTLY_USED]},
    )

    def __post_init__(self):
        self._dynamic_memory_compute = _DynamicMemoryCompute(
            self.compute_capacity_percentage, self.eviction_strategy
        )

    def transform(self, trace: Trace) -> Optional[Trace]:
        """Apply the dynamic memory compute transformation to a trace.

        Args:
            trace (Trace): The input trace.

        Returns:
            Optional[Trace]: The transformed trace.
        """
        return self._dynamic_memory_compute.transform(trace)


@dataclass
class Unmemory(TraceTransform):
    """Unmemory trace transform.

    Reverses the effect of ``DynamicMemoryCompute`` by stripping
    ``READ_FROM_MEMORY`` and ``WRITE_TO_MEMORY`` operations and
    remapping compute-slot qubit IDs back to logical qubit IDs.  The
    resulting trace has no memory qubits; all qubits are compute
    qubits.
    """

    def __post_init__(self):
        self._unmemory = _Unmemory()

    def transform(self, trace: Trace) -> Optional[Trace]:
        """Apply the unmemory transformation to a trace.

        Args:
            trace (Trace): The input trace.

        Returns:
            Optional[Trace]: The transformed trace.
        """
        return self._unmemory.transform(trace)


class _Node(ABC):
    """Abstract base class for trace enumeration nodes."""

    @abstractmethod
    def enumerate(
        self, ctx: ApplicationContext, track_parameters: bool = False
    ) -> Generator[Trace | tuple[Any, Trace], None, None]: ...


class TraceQuery(_Node):
    """A query that enumerates transformed traces from an application.

    A trace query chains a sequence of trace transforms, each with optional
    keyword arguments to override their default parameter domains.
    """

    # This is a sequence of trace transforms together with possible kwargs to
    # override their default domains.  The first element might be
    sequence: list[tuple[Type, dict[str, Any]]]

    def __init__(self, t: Type, **kwargs: Any):
        self.sequence = [(t, kwargs)]

    def enumerate(
        self, ctx: ApplicationContext, track_parameters: bool = False
    ) -> Generator[Trace | tuple[Any, Trace], None, None]:
        """Enumerate transformed traces from the application context.

        Args:
            ctx (ApplicationContext): The application enumeration context.
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
