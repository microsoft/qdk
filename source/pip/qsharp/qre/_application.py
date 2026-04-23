# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

import types
from abc import ABC, abstractmethod
from concurrent.futures import ThreadPoolExecutor
from types import NoneType
from typing import (
    ClassVar,
    Generic,
    Protocol,
    TypeVar,
    Generator,
    get_type_hints,
    cast,
)

from ._enumeration import _enumerate_instances
from ._qre import Trace, EstimationResult
from ._trace import TraceQuery


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
        """Return the trace corresponding to this application and parameters.

        Args:
            parameters (TraceParameters): The trace parameters.

        Returns:
            Trace: The trace for this application instance and parameters.
        """

    @staticmethod
    def q(**kwargs) -> TraceQuery:
        """Create a trace query for this application.

        Args:
            **kwargs: Domain overrides forwarded to trace parameter enumeration.

        Returns:
            TraceQuery: A trace query for this application type.
        """
        return TraceQuery(NoneType, **kwargs)

    def context(self) -> _Context:
        """Create a new enumeration context for this application."""
        return _Context(self)

    def post_process(
        self, parameters: TraceParameters, estimation: EstimationResult
    ) -> EstimationResult:
        """Post-process an estimation result for a given set of trace parameters."""
        return estimation

    def enumerate_traces(
        self,
        **kwargs,
    ) -> Generator[Trace, None, None]:
        """Yield all traces of an application given its dataclass parameters.

        Args:
            **kwargs: Domain overrides forwarded to ``_enumerate_instances``.

        Yields:
            Trace: A trace for each enumerated set of trace parameters.
        """

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

    def enumerate_traces_with_parameters(
        self,
        **kwargs,
    ) -> Generator[tuple[TraceParameters, Trace], None, None]:
        """Yield (parameters, trace) pairs for an application.

        Like ``enumerate_traces``, but each yielded trace is accompanied by the
        trace parameters that were used to generate it.

        Args:
            **kwargs: Domain overrides forwarded to ``_enumerate_instances``.

        Yields:
            tuple[TraceParameters, Trace]: A pair of trace parameters and
                the corresponding trace.
        """

        param_type = get_type_hints(self.__class__.get_trace).get("parameters")
        if param_type is types.NoneType:
            yield None, self.get_trace(None)  # type: ignore
            return

        if isinstance(param_type, TypeVar):
            for c in param_type.__constraints__:
                if c is not types.NoneType:
                    param_type = c
                    break

        if self._parallel_traces:
            instances = list(_enumerate_instances(cast(type, param_type), **kwargs))
            with ThreadPoolExecutor() as executor:
                for instance, trace in zip(
                    instances, executor.map(self.get_trace, instances)
                ):
                    yield instance, trace
        else:
            for instance in _enumerate_instances(cast(type, param_type), **kwargs):
                yield instance, self.get_trace(instance)

    def disable_parallel_traces(self):
        """Disable parallel trace generation for this application."""
        self._parallel_traces = False


class _Context:
    """Enumeration context wrapping an application instance."""

    application: Application

    def __init__(self, application: Application, **kwargs):
        """Initialize the context for the given application.

        Args:
            application (Application): The application instance.
            **kwargs: Additional keyword arguments (reserved for future use).
        """
        self.application = application
