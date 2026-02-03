# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from ._application import Application
from ._architecture import Architecture
from ._qre import EstimationCollection, estimate_parallel
from ._trace import TraceQuery
from ._isa_enumeration import ISAQuery


def estimate(
    application: Application,
    architecture: Architecture,
    trace_query: TraceQuery,
    isa_query: ISAQuery,
    *,
    max_error: float = 1.0,
) -> EstimationCollection:
    """
    Estimate the resource requirements for a given application instance and
    architecture.

    The application instance might return multiple traces.  Each of the traces
    is transformed by the trace query, which applies several trace transforms in
    sequence.  Each transform may return multiple traces.  Similarly, the
    architecture's ISA is transformed by the ISA query, which applies several
    ISA transforms in sequence, each of which may return multiple ISAs.  The
    estimation is performed for each combination of transformed trace and ISA.
    The results are collected into an EstimationCollection and returned.

    The collection only contains the results that are optimal with respect to
    the total number of qubits and the total runtime.

    Args:
        application (Application): The quantum application to be estimated.
        architecture (Architecture): The target quantum architecture.
        trace_query (TraceQuery): The trace query to enumerate traces from the
            application.
        isa_query (ISAQuery): The ISA query to enumerate ISAs from the architecture.

    Returns:
        EstimationCollection: A collection of estimation results.
    """

    app_ctx = application.context()
    arch_ctx = architecture.context()

    return estimate_parallel(
        list(trace_query.enumerate(app_ctx)),
        list(isa_query.enumerate(arch_ctx)),
        max_error,
    )
