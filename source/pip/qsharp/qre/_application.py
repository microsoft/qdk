# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

import types
from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import (
    Any,
    Callable,
    ClassVar,
    Generic,
    Protocol,
    TypeVar,
    Generator,
    get_type_hints,
    cast,
)

from .._qsharp import logical_counts
from ..estimator import LogicalCounts
from ._enumeration import _enumerate_instances
from ._qre import Trace
from .instruction_ids import CCX, MEAS_Z, RZ, T


class DataclassProtocol(Protocol):
    __dataclass_fields__: ClassVar[dict]


TraceParameters = TypeVar("TraceParameters", DataclassProtocol, types.NoneType)


class Application(ABC, Generic[TraceParameters]):
    """
    An application defines a class of quantum computation problems along with a
    method to generate traces for specific problem instances.

    We distinguish between application and trace parameters.  The application
    parameters define which particular instance of the application we want to
    consider.  The trace parameters define how to generate a trace.  They
    change the specific way in which we solve the problem, but not the problem
    itself.

    For example, in quantum cryptography, the application parameters could
    define the key size for an RSA prime product, while the trace parameters
    define which algorithm to use to break the cryptography, as well as
    parameters therein.
    """

    @abstractmethod
    def get_trace(self, parameters: TraceParameters) -> Trace:
        """Return the trace corresponding to this application."""

    def context(self, **kwargs) -> _Context:
        """Create a new enumeration context for this application."""
        return _Context(self, **kwargs)

    def enumerate_traces(
        self,
        **kwargs,
    ) -> Generator[Trace, None, None]:
        """Yields all traces of an application given its dataclass parameters."""

        param_type = get_type_hints(self.__class__.get_trace).get("parameters")
        if param_type is types.NoneType:
            yield self.get_trace(None)  # type: ignore
            return

        if isinstance(param_type, TypeVar):
            for c in param_type.__constraints__:
                if c is not types.NoneType:
                    param_type = c
                    break
        for parameters in _enumerate_instances(cast(type, param_type), **kwargs):
            yield self.get_trace(parameters)


class _Context:
    application: Application
    kwargs: dict[str, Any]

    def __init__(self, application: Application, **kwargs):
        self.application = application
        self.kwargs = kwargs


@dataclass
class QSharpApplication(Application[None]):
    def __init__(self, entry_expr: str | Callable | LogicalCounts):
        self._entry_expr = entry_expr

    def get_trace(self, parameters: None = None) -> Trace:
        if not isinstance(self._entry_expr, LogicalCounts):
            self._counts = logical_counts(self._entry_expr)
        else:
            self._counts = self._entry_expr
        return self._trace_from_logical_counts(self._counts)

    def _trace_from_logical_counts(self, counts: LogicalCounts) -> Trace:
        ccx_count = counts.get("cczCount", 0) + counts.get("ccixCount", 0)

        trace = Trace(counts.get("numQubits", 0))

        rotation_count = counts.get("rotationCount", 0)
        rotation_depth = counts.get("rotationDepth", rotation_count)

        if rotation_count != 0:
            if rotation_depth > 1:
                rotations_per_layer = rotation_count // (rotation_depth - 1)
            else:
                rotations_per_layer = 0

            last_layer = rotation_count - (rotations_per_layer * (rotation_depth - 1))

            if rotations_per_layer != 0:
                block = trace.add_block(repetitions=rotation_depth - 1)
                for i in range(rotations_per_layer):
                    block.add_operation(RZ, [i])
            block = trace.add_block()
            for i in range(last_layer):
                block.add_operation(RZ, [i])

        if t_count := counts.get("tCount", 0):
            block = trace.add_block(repetitions=t_count)
            block.add_operation(T, [0])

        if ccx_count:
            block = trace.add_block(repetitions=ccx_count)
            block.add_operation(CCX, [0, 1, 2])

        if meas_count := counts.get("measurementCount", 0):
            block = trace.add_block(repetitions=meas_count)
            block.add_operation(MEAS_Z, [0])

        # TODO: handle memory qubits

        return trace
