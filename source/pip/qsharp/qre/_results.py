# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional, Callable, Any, Iterable

import pandas as pd

from ._architecture import ISAContext
from ._qre import (
    FactoryResult,
    instruction_name,
    EstimationResult,
)
from ._instruction import InstructionSource
from .property_keys import (
    PHYSICAL_COMPUTE_QUBITS,
    PHYSICAL_MEMORY_QUBITS,
    PHYSICAL_FACTORY_QUBITS,
)


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

        self.name: Optional[str] = None
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

    def plot(self, **kwargs):
        """Plot this table's results.

        Convenience wrapper around :func:`plot_estimates`.  All keyword
        arguments are forwarded.

        Returns:
            matplotlib.figure.Figure: The figure containing the plot.
        """
        return plot_estimates(self, **kwargs)


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

    @classmethod
    def from_result(
        cls, result: EstimationResult, ctx: ISAContext
    ) -> EstimationTableEntry:
        return cls(
            qubits=result.qubits,
            runtime=result.runtime,
            error=result.error,
            source=InstructionSource.from_isa(ctx, result.isa),
            factories=result.factories.copy(),
            properties=result.properties.copy(),
        )


@dataclass(slots=True)
class EstimationTableStats:
    num_traces: int = 0
    num_isas: int = 0
    total_jobs: int = 0
    successful_estimates: int = 0
    pareto_results: int = 0


# Mapping from runtime unit name to its value in nanoseconds.
_TIME_UNITS: dict[str, float] = {
    "ns": 1,
    "µs": 1e3,
    "us": 1e3,
    "ms": 1e6,
    "s": 1e9,
    "min": 60e9,
    "hours": 3600e9,
    "days": 86_400e9,
    "weeks": 604_800e9,
    "months": 31 * 86_400e9,
    "years": 365 * 86_400e9,
    "decades": 10 * 365 * 86_400e9,
    "centuries": 100 * 365 * 86_400e9,
}

# Ordered subset of _TIME_UNITS used for default x-axis tick labels.
_TICK_UNITS: list[tuple[str, float]] = [
    ("1 ns", _TIME_UNITS["ns"]),
    ("1 µs", _TIME_UNITS["µs"]),
    ("1 ms", _TIME_UNITS["ms"]),
    ("1 s", _TIME_UNITS["s"]),
    ("1 min", _TIME_UNITS["min"]),
    ("1 hour", _TIME_UNITS["hours"]),
    ("1 day", _TIME_UNITS["days"]),
    ("1 week", _TIME_UNITS["weeks"]),
    ("1 month", _TIME_UNITS["months"]),
    ("1 year", _TIME_UNITS["years"]),
    ("1 decade", _TIME_UNITS["decades"]),
    ("1 century", _TIME_UNITS["centuries"]),
]


def plot_estimates(
    data: EstimationTable | Iterable[EstimationTable],
    *,
    runtime_unit: Optional[str] = None,
    figsize: tuple[float, float] = (15, 8),
    scatter_args: dict[str, Any] = {"marker": "x"},
):
    """Returns a plot of the estimates displaying qubits vs runtime.

    Creates a log-log scatter plot where the x-axis shows the total runtime and
    the y-axis shows the total number of physical qubits.

    *data* may be a single `EstimationTable` or an iterable of tables.  When
    multiple tables are provided, each is plotted as a separate series.  If a
    table has a `EstimationTable.name` (set via the *name* parameter of
    `estimate`), it is used as the legend label for that series.

    When *runtime_unit* is ``None`` (the default), the x-axis uses
    human-readable time-unit tick labels spanning nanoseconds to centuries.
    When a unit string is given (e.g. ``"hours"``), all runtimes are scaled to
    that unit and the x-axis label includes the unit while the ticks are plain
    numbers.

    Supported *runtime_unit* values: ``"ns"``, ``"µs"`` (or ``"us"``), ``"ms"``,
    ``"s"``, ``"min"``, ``"hours"``, ``"days"``, ``"weeks"``, ``"months"``,
    ``"years"``.

    Args:
        data: A single EstimationTable or an iterable of
            EstimationTable objects to plot.
        runtime_unit: Optional time unit to scale the x-axis to.
        figsize: Figure dimensions in inches as ``(width, height)``.
        scatter_args: Additional keyword arguments to pass to
            ``matplotlib.axes.Axes.scatter`` when plotting the points.

    Returns:
        matplotlib.figure.Figure: The figure containing the plot.

    Raises:
        ImportError: If matplotlib is not installed.
        ValueError: If all tables are empty or *runtime_unit* is not
            recognised.
    """
    try:
        import matplotlib.pyplot as plt
    except ImportError:
        raise ImportError(
            "Missing optional 'matplotlib' dependency. To install run: "
            "pip install matplotlib"
        )

    # Normalize to a list of tables
    if isinstance(data, EstimationTable):
        tables = [data]
    else:
        tables = list(data)

    if not tables or all(len(t) == 0 for t in tables):
        raise ValueError("Cannot plot an empty EstimationTable.")

    if runtime_unit is not None and runtime_unit not in _TIME_UNITS:
        raise ValueError(
            f"Unknown runtime_unit {runtime_unit!r}. "
            f"Supported units: {', '.join(_TIME_UNITS)}"
        )

    fig, ax = plt.subplots(figsize=figsize)
    ax.set_ylabel("Physical qubits")
    ax.set_xscale("log")
    ax.set_yscale("log")

    all_xs: list[float] = []
    has_labels = False

    for table in tables:
        if len(table) == 0:
            continue

        ys = [entry.qubits for entry in table]

        if runtime_unit is not None:
            scale = _TIME_UNITS[runtime_unit]
            xs = [entry.runtime / scale for entry in table]
        else:
            xs = [float(entry.runtime) for entry in table]

        all_xs.extend(xs)

        label = table.name
        if label is not None:
            has_labels = True

        ax.scatter(x=xs, y=ys, label=label, **scatter_args)

    if runtime_unit is not None:
        ax.set_xlabel(f"Runtime ({runtime_unit})")
    else:
        ax.set_xlabel("Runtime")

        time_labels, time_units = zip(*_TICK_UNITS)

        cutoff = (
            next(
                (i for i, x in enumerate(time_units) if x > max(all_xs)),
                len(time_units) - 1,
            )
            + 1
        )

        ax.set_xticks(time_units[:cutoff])
        ax.set_xticklabels(time_labels[:cutoff], rotation=90)

    if has_labels:
        ax.legend()

    plt.close(fig)

    return fig
