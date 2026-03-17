# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

from dataclasses import dataclass, field
from typing import cast, Optional, Callable, Any

import pandas as pd

from ._application import Application
from ._architecture import Architecture
from ._qre import (
    _estimate_parallel,
    _estimate_with_graph,
    _EstimationCollection,
    Trace,
    FactoryResult,
    instruction_name,
)
from ._trace import TraceQuery, PSSPC, LatticeSurgery
from ._instruction import InstructionSource
from ._isa_enumeration import ISAQuery
from .property_keys import (
    PHYSICAL_COMPUTE_QUBITS,
    PHYSICAL_MEMORY_QUBITS,
    PHYSICAL_FACTORY_QUBITS,
)


def estimate(
    application: Application,
    architecture: Architecture,
    isa_query: ISAQuery,
    trace_query: Optional[TraceQuery] = None,
    *,
    max_error: float = 1.0,
    post_process: bool = False,
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
        name (Optional[str]): An optional name for the estimation.  If give, this
            will be added as a first column to the results table for all entries.

    Returns:
        EstimationTable: A table containing the optimal estimation results
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
        isas = list(isa_query.enumerate(arch_ctx))

        num_traces = len(params_and_traces)
        num_isas = len(isas)

        # Phase 1: Run all estimates in Rust (parallel, fast).
        traces_only = [trace for _, trace in params_and_traces]
        collection = _estimate_parallel(cast(list[Trace], traces_only), isas, max_error)
        successful = collection.successful_estimates
        summaries = collection.all_summaries  # (trace_idx, isa_idx, qubits, runtime)

        # Phase 2: Learn per-trace runtime multiplier and qubit multiplier from
        # one sample each: if post_process changes runtime or qubit count it
        # will affect the Pareto optimality, but the changes depend only on the
        # trace, not on the ISA.
        trace_multipliers: dict[int, tuple[float, float]] = {}
        trace_sample_isa: dict[int, int] = {}
        for t_idx, i_idx, _q, r in summaries:
            if t_idx not in trace_sample_isa:
                trace_sample_isa[t_idx] = i_idx
        for t_idx, i_idx in trace_sample_isa.items():
            params, trace = params_and_traces[t_idx]
            sample = trace.estimate(isas[i_idx], max_error)
            if sample is not None:
                pre_q = sample.qubits
                pre_r = sample.runtime
                pp = app_ctx.application.post_process(params, sample)
                if pp is not None and pre_r > 0 and pre_q > 0:
                    trace_multipliers[t_idx] = (pp.qubits / pre_q, pp.runtime / pre_r)

        # Phase 3: Estimate post-pp values and filter to Pareto candidates.
        estimated_pp: list[tuple[int, int, int, int]] = []  # (t, i, q, est_r)
        for t_idx, i_idx, q, r in summaries:
            mult_q, mult_r = trace_multipliers.get(t_idx, (0.0, 0.0))
            est_q = int(q * mult_q) if mult_q > 0 else q
            est_r = int(r * mult_r) if mult_r > 0 else r
            estimated_pp.append((t_idx, i_idx, est_q, est_r))

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
        for t_idx, i_idx, _q, _r in approx_pareto:
            params, trace = params_and_traces[t_idx]
            result = trace.estimate(isas[i_idx], max_error)
            if result is not None:
                pp_result = app_ctx.application.post_process(params, result)
                if pp_result is not None:
                    pp_collection.insert(pp_result)
        collection = pp_collection
    else:
        traces = list(trace_query.enumerate(app_ctx))
        isas = list(isa_query.enumerate(arch_ctx))

        num_traces = len(traces)
        num_isas = len(isas)

        # Use the Rust parallel estimation path
        collection = _estimate_parallel(cast(list[Trace], traces), isas, max_error)
        successful = collection.successful_estimates

    # Post-process the results and add them to a results table
    table = EstimationTable()

    if name is not None:
        table.insert_column(0, "name", lambda entry: name)

    for result in collection:
        entry = EstimationTableEntry(
            qubits=result.qubits,
            runtime=result.runtime,
            error=result.error,
            source=InstructionSource.from_isa(arch_ctx, result.isa),
            factories=result.factories.copy(),
            properties=result.properties.copy(),
        )

        table.append(entry)

    # Fill in the stats for this estimation run
    table.stats.num_traces = num_traces
    table.stats.num_isas = num_isas
    table.stats.total_jobs = num_traces * num_isas
    table.stats.successful_estimates = successful
    table.stats.pareto_results = len(collection)

    return table


def estimate_with_graph(
    application: Application,
    architecture: Architecture,
    isa_query: ISAQuery,
    trace_query: Optional[TraceQuery] = None,
    *,
    max_error: float = 1.0,
    post_process: bool = False,
    name: Optional[str] = None,
) -> EstimationTable:
    """
    Estimate the resource requirements for a given application instance and
    architecture using a graph-based exploration of ISA combinations.

    Unlike `estimate`, which enumerates all ISAs upfront and evaluates every
    trace × ISA combination independently, this function populates a provenance
    graph from the ISA query and builds a Pareto index over it. The graph-based
    approach can prune dominated ISA combinations early, reducing the number of
    estimations that need to be performed.

    The application instance might return multiple traces.  Each of the traces
    is transformed by the trace query, which applies several trace transforms in
    sequence.  Each transform may return multiple traces.  The collection only
    contains the results that are optimal with respect to the total number of
    qubits and the total runtime.

    Note:
        The ``post_process`` parameter is accepted for API compatibility with
        `estimate` but must be ``False`` for now; passing ``True`` will raise an
        ``AssertionError``.

    Args:
        application (Application): The quantum application to be estimated.
        architecture (Architecture): The target quantum architecture.
        isa_query (ISAQuery): The ISA query used to populate the provenance
            graph from the architecture.
        trace_query (TraceQuery): The trace query to enumerate traces from the
            application.
        max_error (float): The maximum allowed error for the estimation
            results.
        post_process (bool): Must be False.  Post-processing is not supported
            in the graph-based estimation path yet.
        name (Optional[str]): An optional name for the estimation.  If given,
            this will be added as a first column to the results table for all
            entries.

    Returns:
        EstimationTable: A table containing the optimal estimation results.
    """

    app_ctx = application.context()
    arch_ctx = architecture.context()

    if trace_query is None:
        trace_query = PSSPC.q() * LatticeSurgery.q()

    assert not post_process

    isa_query.populate(arch_ctx)
    arch_ctx._provenance.build_pareto_index()

    traces = list(trace_query.enumerate(app_ctx))

    collection = _estimate_with_graph(
        cast(list[Trace], traces), arch_ctx._provenance, max_error
    )

    # Post-process the results and add them to a results table
    table = EstimationTable()

    if name is not None:
        table.insert_column(0, "name", lambda entry: name)

    for result in collection:
        entry = EstimationTableEntry(
            qubits=result.qubits,
            runtime=result.runtime,
            error=result.error,
            source=InstructionSource.from_isa(arch_ctx, result.isa),
            factories=result.factories.copy(),
            properties=result.properties.copy(),
        )

        table.append(entry)

    # Fill in the stats for this estimation run
    table.stats.num_traces = len(traces)
    table.stats.num_isas = arch_ctx._provenance.total_isa_count()
    table.stats.total_jobs = collection.total_jobs
    table.stats.successful_estimates = collection.successful_estimates
    table.stats.pareto_results = len(collection)

    return table


class EstimationTable(list["EstimationTableEntry"]):
    """A table of quantum resource estimation results.

    Extends ``list[EstimationTableEntry]`` and provides configurable columns for
    displaying estimation data.  By default the table includes *qubits*,
    *runtime* (displayed as a ``pandas.Timedelta``), and *error* columns.
    Additional columns can be added or inserted with :meth:`add_column` and
    :meth:`insert_column`.
    """

    def __init__(self):
        """Initialize an empty estimation table with default columns."""
        super().__init__()

        self.stats = EstimationTableStats()

        self._columns: list[tuple[str, EstimationTableColumn]] = [
            ("qubits", EstimationTableColumn(lambda entry: entry.qubits)),
            (
                "runtime",
                EstimationTableColumn(
                    lambda entry: entry.runtime,
                    formatter=lambda x: pd.Timedelta(x, unit="ns"),
                ),
            ),
            ("error", EstimationTableColumn(lambda entry: entry.error)),
        ]

    def add_column(
        self,
        name: str,
        function: Callable[[EstimationTableEntry], Any],
        formatter: Optional[Callable[[Any], Any]] = None,
    ) -> None:
        """Adds a column to the estimation table.

        Args:
            name (str): The name of the column.
            function (Callable[[EstimationTableEntry], Any]): A function that
                takes an EstimationTableEntry and returns the value for this
                column.
            formatter (Optional[Callable[[Any], Any]]): An optional function
                that formats the output of `function` for display purposes.
        """
        self._columns.append((name, EstimationTableColumn(function, formatter)))

    def insert_column(
        self,
        index: int,
        name: str,
        function: Callable[[EstimationTableEntry], Any],
        formatter: Optional[Callable[[Any], Any]] = None,
    ) -> None:
        """Inserts a column at the specified index in the estimation table.

        Args:
            index (int): The index at which to insert the column.
            name (str): The name of the column.
            function (Callable[[EstimationTableEntry], Any]): A function that
                takes an EstimationTableEntry and returns the value for this
                column.
            formatter (Optional[Callable[[Any], Any]]): An optional function
                that formats the output of `function` for display purposes.
        """
        self._columns.insert(index, (name, EstimationTableColumn(function, formatter)))

    def add_qubit_partition_column(self) -> None:
        self.add_column(
            "physical_compute_qubits",
            lambda entry: entry.properties.get(PHYSICAL_COMPUTE_QUBITS, 0),
        )
        self.add_column(
            "physical_factory_qubits",
            lambda entry: entry.properties.get(PHYSICAL_FACTORY_QUBITS, 0),
        )
        self.add_column(
            "physical_memory_qubits",
            lambda entry: entry.properties.get(PHYSICAL_MEMORY_QUBITS, 0),
        )

    def add_factory_summary_column(self) -> None:
        """Adds a column to the estimation table that summarizes the factories used in the estimation."""

        def summarize_factories(entry: EstimationTableEntry) -> str:
            if not entry.factories:
                return "None"
            return ", ".join(
                f"{factory_result.copies}×{instruction_name(id)}"
                for id, factory_result in entry.factories.items()
            )

        self.add_column("factories", summarize_factories)

    def as_frame(self):
        """Convert the estimation table to a :class:`pandas.DataFrame`.

        Each row corresponds to an :class:`EstimationTableEntry` and each
        column is determined by the columns registered on this table.  Column
        formatters, when present, are applied to the values before they are
        placed in the frame.

        Returns:
            pandas.DataFrame: A DataFrame representation of the estimation
                results.
        """
        return pd.DataFrame(
            [
                {
                    column_name: (
                        column.formatter(column.function(entry))
                        if column.formatter is not None
                        else column.function(entry)
                    )
                    for column_name, column in self._columns
                }
                for entry in self
            ]
        )


@dataclass(frozen=True, slots=True)
class EstimationTableColumn:
    """Definition of a single column in an :class:`EstimationTable`.

    Attributes:
        function: A callable that extracts the raw column value from an
            :class:`EstimationTableEntry`.
        formatter: An optional callable that transforms the raw value for
            display purposes (e.g. converting nanoseconds to a
            ``pandas.Timedelta``).
    """

    function: Callable[[EstimationTableEntry], Any]
    formatter: Optional[Callable[[Any], Any]] = None


@dataclass(frozen=True, slots=True)
class EstimationTableEntry:
    """A single row in an :class:`EstimationTable`.

    Each entry represents one Pareto-optimal estimation result for a
    particular combination of application trace and architecture ISA.

    Attributes:
        qubits: Total number of physical qubits required.
        runtime: Total runtime of the algorithm in nanoseconds.
        error: Total estimated error probability.
        source: The instruction source derived from the architecture ISA used
            for this estimation.
        factories: A mapping from instruction id to the
            :class:`FactoryResult` describing the magic-state factory used
            and the number of copies required.
        properties: Additional key-value properties attached to the
            estimation result.
    """

    qubits: int
    runtime: int
    error: float
    source: InstructionSource
    factories: dict[int, FactoryResult] = field(default_factory=dict)
    properties: dict[int, int | float | bool | str] = field(default_factory=dict)


@dataclass(slots=True)
class EstimationTableStats:
    num_traces: int = 0
    num_isas: int = 0
    total_jobs: int = 0
    successful_estimates: int = 0
    pareto_results: int = 0
