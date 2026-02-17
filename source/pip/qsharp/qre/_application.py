# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

import types
from abc import ABC, abstractmethod
from concurrent.futures import ThreadPoolExecutor
from typing import (
    Any,
    ClassVar,
    Generic,
    Protocol,
    TypeVar,
    Generator,
    get_type_hints,
    cast,
)

from ._enumeration import _enumerate_instances
from ._qre import Trace


class DataclassProtocol(Protocol):
    __dataclass_fields__: ClassVar[dict]


TraceParameters = TypeVar("TraceParameters", DataclassProtocol, types.NoneType)


class Application(ABC, Generic[TraceParameters]):
    """
    An application defines a class of quantum computation problems along with a
    method to generate traces for specific problem instances.

    We distinguish between application and trace parameters.  The application
    parameters define which particular instance of the application we want to
    consider.  The trace parameters define how to generate a trace.  They change
    the specific way in which we solve the problem, but not the problem itself.

    For example, in quantum cryptanalysis, the application parameters could
    define the key size for an RSA prime product, while the trace parameters
    define which algorithm to use to break the cryptography, as well as
    parameters therein.
    """

    _parallel_traces: bool = True

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

        if self._parallel_traces:
            instances = list(_enumerate_instances(cast(type, param_type), **kwargs))
            with ThreadPoolExecutor() as executor:
                for trace in executor.map(self.get_trace, instances):
                    yield trace
        else:
            for instances in _enumerate_instances(cast(type, param_type), **kwargs):
                yield self.get_trace(instances)

    def disable_parallel_traces(self):
        """Disable parallel trace generation for this application."""
        self._parallel_traces = False


class _Context:
    application: Application
    kwargs: dict[str, Any]

    def __init__(self, application: Application, **kwargs):
        self.application = application
        self.kwargs = kwargs
