# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from typing import cast, Optional, Callable, Any

import pandas as pd

from ._application import Application
from ._architecture import Architecture
from ._qre import (
    _estimate_parallel,
    _EstimationCollection,
    Trace,
    FactoryResult,
    instruction_name,
)
from ._trace import TraceQuery, PSSPC, LatticeSurgery
from ._instruction import InstructionSource
from ._isa_enumeration import ISAQuery


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
        params_and_traces = list(trace_query.enumerate(app_ctx, track_parameters=True))
        isas = list(isa_query.enumerate(arch_ctx))

        # Estimate all trace × ISA combinations using Python threads
        collection = _EstimationCollection()

        def _estimate_one(params, trace, isa):
            result = trace.estimate(isa, max_error)
            if result is not None:
                result = app_ctx.application.post_process(params, result)
            return result

        with ThreadPoolExecutor() as executor:
            futures = [
                executor.submit(_estimate_one, params, trace, isa)
                for params, trace in cast(list[tuple[Any, Trace]], params_and_traces)
                for isa in isas
            ]
            for future in futures:
                result = future.result()
                if result is not None:
                    collection.insert(result)
    else:
        traces = list(trace_query.enumerate(app_ctx))
        isas = list(isa_query.enumerate(arch_ctx))

        # Use the Rust parallel estimation path
        collection = _estimate_parallel(cast(list[Trace], traces), isas, max_error)

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
    properties: dict[str, int | float | bool | str] = field(default_factory=dict)
