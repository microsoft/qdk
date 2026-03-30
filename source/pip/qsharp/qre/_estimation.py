# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

from typing import cast, Optional, Any


from ._application import Application
from ._architecture import Architecture
from ._qre import (
    _estimate_parallel,
    _estimate_with_graph,
    _EstimationCollection,
    Trace,
)
from ._trace import TraceQuery, PSSPC, LatticeSurgery
from ._isa_enumeration import ISAQuery
from ._results import EstimationTable, EstimationTableEntry


def estimate(
    application: Application,
    architecture: Architecture,
    isa_query: ISAQuery,
    trace_query: Optional[TraceQuery] = None,
    *,
    max_error: float = 1.0,
    post_process: bool = False,
    use_graph: bool = True,
    name: Optional[str] = None,
) -> EstimationTable:
    """
    Estimate the resource requirements for a given application instance and
    architecture.

    The application instance might return multiple traces.  Each of the traces
    is transformed by the trace query, which applies several trace transforms in
    sequence.  Each transform may return multiple traces.  Similarly, the
    architecture's ISA is transformed by the ISA query, which applies several
    ISA transforms in sequence, each of which may return multiple ISAs.  The
    estimation is performed for each combination of transformed trace and ISA.
    The results are collected into an EstimationTable and returned.

    The collection only contains the results that are optimal with respect to
    the total number of qubits and the total runtime.

    Note:
        The pruning strategy used when ``use_graph`` is set to True (default)
        filters ISA instructions by comparing their per-instruction space, time,
        and error independently. However, the total qubit count of a result
        depends on the interaction between factory space and runtime:
        ``factory_qubits = copies × factory_space`` where copies are determined
        by ``count.div_ceil(runtime / factory_time)``. Because of this, an ISA
        instruction that is dominated on per-instruction metrics can still
        contribute to a globally Pareto-optimal result (e.g., a factory with
        higher time may need fewer copies, leading to fewer total qubits). As a
        consequence, ``use_graph=True`` may miss some results that
        ``use_graph=False`` would find. Use ``use_graph=False`` when completeness of
        the Pareto frontier is required.

    Args:
        application (Application): The quantum application to be estimated.
        architecture (Architecture): The target quantum architecture.
        isa_query (ISAQuery): The ISA query to enumerate ISAs from the architecture.
        trace_query (TraceQuery): The trace query to enumerate traces from the
            application.
        max_error (float): The maximum allowed error for the estimation results.
        post_process (bool): If True, use the Python-threaded estimation path
            (intended for future post-processing logic).  If False (default),
            use the Rust parallel estimation path.
        use_graph (bool): If True (default), use the Rust estimation path that
            builds a graph of ISAs and prunes suboptimal ISAs during estimation.
            If False, use the Rust estimation path that does not perform any
            pruning and simply enumerates all ISAs for each trace.
        name (Optional[str]): An optional name for the estimation.  If given, this
            will be added as a first column to the results table for all entries.

    Returns:
        EstimationTable: A table containing the optimal estimation results.
    """

    app_ctx = application.context()
    arch_ctx = architecture.context()

    if trace_query is None:
        trace_query = PSSPC.q() * LatticeSurgery.q()

    if post_process:
        # Enumerate traces with their parameters so we can post-process later
        params_and_traces = cast(
            list[tuple[Any, Trace]],
            list(trace_query.enumerate(app_ctx, track_parameters=True)),
        )
        num_traces = len(params_and_traces)

        # Phase 1: Run all estimates in Rust (parallel, fast).
        traces_only = [trace for _, trace in params_and_traces]

        if use_graph:
            isa_query.populate(arch_ctx)
            arch_ctx._provenance.build_pareto_index()

            num_isas = arch_ctx._provenance.total_isa_count()

            collection = _estimate_with_graph(
                cast(list[Trace], traces_only), arch_ctx._provenance, max_error, True
            )
            isas = collection.isas
        else:
            isas = list(isa_query.enumerate(arch_ctx))

            num_isas = len(isas)

            collection = _estimate_parallel(
                cast(list[Trace], traces_only), isas, max_error, True
            )

        total_jobs = collection.total_jobs
        successful = collection.successful_estimates
        summaries = collection.all_summaries  # (trace_idx, isa_idx, qubits, runtime)

        # Phase 2: Learn per-trace runtime multiplier and qubit multiplier from
        # one sample each: if post_process changes runtime or qubit count it
        # will affect the Pareto optimality, but the changes depend only on the
        # trace, not on the ISA.
        trace_multipliers: dict[int, tuple[float, float]] = {}
        trace_sample_isa: dict[int, int] = {}
        for t_idx, isa_idx, _q, r in summaries:
            if t_idx not in trace_sample_isa:
                trace_sample_isa[t_idx] = isa_idx
        for t_idx, isa_idx in trace_sample_isa.items():
            params, trace = params_and_traces[t_idx]
            sample = trace.estimate(isas[isa_idx], max_error)
            if sample is not None:
                pre_q = sample.qubits
                pre_r = sample.runtime
                pp = app_ctx.application.post_process(params, sample)
                if pp is not None and pre_r > 0 and pre_q > 0:
                    trace_multipliers[t_idx] = (pp.qubits / pre_q, pp.runtime / pre_r)

        # Phase 3: Estimate post-pp values and filter to Pareto candidates.
        estimated_pp: list[tuple[int, int, int, int]] = (
            []
        )  # (t_idx, isa_idx, est_q, est_r)
        for t_idx, isa_idx, q, r in summaries:
            mult_q, mult_r = trace_multipliers.get(t_idx, (0.0, 0.0))
            est_q = int(q * mult_q) if mult_q > 0 else q
            est_r = int(r * mult_r) if mult_r > 0 else r
            estimated_pp.append((t_idx, isa_idx, est_q, est_r))

        # Build approximate post-pp Pareto frontier to identify candidates.
        estimated_pp.sort(key=lambda x: (x[2], x[3]))  # sort by qubits, then runtime
        approx_pareto: list[tuple[int, int, int, int]] = []
        min_r = float("inf")
        for item in estimated_pp:
            if item[3] < min_r:
                approx_pareto.append(item)
                min_r = item[3]

        # Phase 4: Re-estimate and post-process only the Pareto candidates.
        pp_collection = _EstimationCollection()
        for t_idx, isa_idx, _q, _r in approx_pareto:
            params, trace = params_and_traces[t_idx]
            result = trace.estimate(isas[isa_idx], max_error)
            if result is not None:
                pp_result = app_ctx.application.post_process(params, result)
                if pp_result is not None:
                    pp_collection.insert(pp_result)
        collection = pp_collection
    else:
        traces = list(trace_query.enumerate(app_ctx))
        num_traces = len(traces)

        if use_graph:
            isa_query.populate(arch_ctx)
            arch_ctx._provenance.build_pareto_index()

            num_isas = arch_ctx._provenance.total_isa_count()

            collection = _estimate_with_graph(
                cast(list[Trace], traces), arch_ctx._provenance, max_error, False
            )
        else:
            isas = list(isa_query.enumerate(arch_ctx))

            num_isas = len(isas)

            # Use the Rust parallel estimation path
            collection = _estimate_parallel(
                cast(list[Trace], traces), isas, max_error, False
            )

        total_jobs = collection.total_jobs
        successful = collection.successful_estimates

    # Post-process the results and add them to a results table
    table = EstimationTable()

    table.name = name

    if name is not None:
        table.insert_column(0, "name", lambda entry: name)

    table.extend(
        EstimationTableEntry.from_result(result, arch_ctx) for result in collection
    )

    # Fill in the stats for this estimation run
    table.stats.num_traces = num_traces
    table.stats.num_isas = num_isas
    table.stats.total_jobs = total_jobs
    table.stats.successful_estimates = successful
    table.stats.pareto_results = len(collection)

    return table
