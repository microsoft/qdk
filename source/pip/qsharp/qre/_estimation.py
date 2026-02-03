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
) -> EstimationCollection:
    app_ctx = application.context()
    arch_ctx = architecture.context()

    return estimate_parallel(
        list(trace_query.enumerate(app_ctx)),
        list(isa_query.enumerate(arch_ctx)),
    )
