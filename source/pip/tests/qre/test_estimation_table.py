# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

cirq = pytest.importorskip("cirq")

import pandas as pd

from qdk.qre import (
    PSSPC,
    LatticeSurgery,
    estimate,
)
from qdk.qre.application import QSharpApplication
from qdk.qre.models import SurfaceCode, GateBased
from qdk.qre._estimation import (
    EstimationTable,
    EstimationTableEntry,
)
from qdk.qre._instruction import InstructionSource
from qdk.qre.instruction_ids import LATTICE_SURGERY
from qdk.qre.property_keys import DISTANCE, NUM_TS_PER_ROTATION

from .conftest import ExampleFactory


def _make_entry(qubits, runtime, error, properties=None):
    """Helper to create an EstimationTableEntry with a dummy InstructionSource."""
    return EstimationTableEntry(
        qubits=qubits,
        runtime=runtime,
        error=error,
        source=InstructionSource(),
        properties=properties or {},
    )


def test_estimation_table_default_columns():
    """Test that a new EstimationTable has the three default columns."""
    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01))

    frame = table.as_frame()
    assert list(frame.columns) == ["qubits", "runtime", "error"]
    assert frame["qubits"][0] == 100
    assert frame["runtime"][0] == pd.Timedelta(5000, unit="ns")
    assert frame["error"][0] == 0.01


def test_estimation_table_multiple_rows():
    """Test as_frame with multiple entries."""
    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01))
    table.append(_make_entry(200, 10000, 0.02))
    table.append(_make_entry(300, 15000, 0.03))

    frame = table.as_frame()
    assert len(frame) == 3
    assert list(frame["qubits"]) == [100, 200, 300]
    assert list(frame["error"]) == [0.01, 0.02, 0.03]


def test_estimation_table_empty():
    """Test as_frame with no entries produces an empty DataFrame."""
    table = EstimationTable()
    frame = table.as_frame()
    assert len(frame) == 0


def test_estimation_table_add_column():
    """Test adding a column to the table."""
    VAL = 0

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={VAL: 42}))
    table.append(_make_entry(200, 10000, 0.02, properties={VAL: 84}))

    table.add_column("val", lambda e: e.properties[VAL])

    frame = table.as_frame()
    assert list(frame.columns) == ["qubits", "runtime", "error", "val"]
    assert list(frame["val"]) == [42, 84]


def test_estimation_table_add_column_with_formatter():
    """Test adding a column with a formatter."""
    NS = 0

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={NS: 1000}))

    table.add_column(
        "duration",
        lambda e: e.properties[NS],
        formatter=lambda x: pd.Timedelta(x, unit="ns"),
    )

    frame = table.as_frame()
    assert frame["duration"][0] == pd.Timedelta(1000, unit="ns")


def test_estimation_table_add_multiple_columns():
    """Test adding multiple columns preserves order."""
    A = 0
    B = 1
    C = 2

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={A: 1, B: 2, C: 3}))

    table.add_column("a", lambda e: e.properties[A])
    table.add_column("b", lambda e: e.properties[B])
    table.add_column("c", lambda e: e.properties[C])

    frame = table.as_frame()
    assert list(frame.columns) == ["qubits", "runtime", "error", "a", "b", "c"]
    assert frame["a"][0] == 1
    assert frame["b"][0] == 2
    assert frame["c"][0] == 3


def test_estimation_table_insert_column_at_beginning():
    """Test inserting a column at index 0."""
    NAME = 0

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={NAME: "test"}))

    table.insert_column(0, "name", lambda e: e.properties[NAME])

    frame = table.as_frame()
    assert list(frame.columns) == ["name", "qubits", "runtime", "error"]
    assert frame["name"][0] == "test"


def test_estimation_table_insert_column_in_middle():
    """Test inserting a column between existing default columns."""
    EXTRA = 0

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={EXTRA: 99}))

    # Insert between qubits and runtime (index 1)
    table.insert_column(1, "extra", lambda e: e.properties[EXTRA])

    frame = table.as_frame()
    assert list(frame.columns) == ["qubits", "extra", "runtime", "error"]
    assert frame["extra"][0] == 99


def test_estimation_table_insert_column_at_end():
    """Test inserting a column at the end (same effect as add_column)."""
    LAST = 0

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={LAST: True}))

    # 3 default columns, inserting at index 3 = end
    table.insert_column(3, "last", lambda e: e.properties[LAST])

    frame = table.as_frame()
    assert list(frame.columns) == ["qubits", "runtime", "error", "last"]
    assert frame["last"][0]


def test_estimation_table_insert_column_with_formatter():
    """Test inserting a column with a formatter."""
    NS = 0

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={NS: 2000}))

    table.insert_column(
        0,
        "custom_time",
        lambda e: e.properties[NS],
        formatter=lambda x: pd.Timedelta(x, unit="ns"),
    )

    frame = table.as_frame()
    assert frame["custom_time"][0] == pd.Timedelta(2000, unit="ns")
    assert list(frame.columns)[0] == "custom_time"


def test_estimation_table_insert_and_add_columns():
    """Test combining insert_column and add_column."""
    A = 0
    B = 0

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={A: 1, B: 2}))

    table.add_column("b", lambda e: e.properties[B])
    table.insert_column(0, "a", lambda e: e.properties[A])

    frame = table.as_frame()
    assert list(frame.columns) == ["a", "qubits", "runtime", "error", "b"]


def test_estimation_table_factory_summary_no_factories():
    """Test factory summary column when entries have no factories."""
    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01))

    table.add_factory_summary_column()

    frame = table.as_frame()
    assert "factories" in frame.columns
    assert frame["factories"][0] == "None"


def test_estimation_table_factory_summary_with_estimation():
    """Test factory summary column with real estimation results."""
    code = """
    {
        use (a, b, c) = (Qubit(), Qubit(), Qubit());
        T(a);
        CCNOT(a, b, c);
        Rz(1.2345, a);
    }
    """
    app = QSharpApplication(code)
    arch = GateBased(gate_time=50, measurement_time=100)
    results = estimate(
        app,
        arch,
        SurfaceCode.q() * ExampleFactory.q(),
        PSSPC.q() * LatticeSurgery.q(),
        max_error=0.5,
    )

    assert len(results) >= 1

    results.add_factory_summary_column()
    frame = results.as_frame()

    assert "factories" in frame.columns
    # Each result should mention T in the factory summary
    for val in frame["factories"]:
        assert "T" in val


def test_estimation_table_add_column_from_source():
    """Test adding a column that accesses the InstructionSource (like distance)."""
    code = """
    {
        use (a, b, c) = (Qubit(), Qubit(), Qubit());
        T(a);
        CCNOT(a, b, c);
        Rz(1.2345, a);
    }
    """
    app = QSharpApplication(code)
    arch = GateBased(gate_time=50, measurement_time=100)
    results = estimate(
        app,
        arch,
        SurfaceCode.q() * ExampleFactory.q(),
        PSSPC.q() * LatticeSurgery.q(),
        max_error=0.5,
    )

    assert len(results) >= 1

    results.add_column(
        "compute_distance",
        lambda entry: entry.source[LATTICE_SURGERY].instruction[DISTANCE],
    )

    frame = results.as_frame()
    assert "compute_distance" in frame.columns
    for d in frame["compute_distance"]:
        assert isinstance(d, int)
        assert d >= 3


def test_estimation_table_add_column_from_properties():
    """Test adding columns that access trace properties from estimation."""
    code = """
    {
        use (a, b, c) = (Qubit(), Qubit(), Qubit());
        T(a);
        CCNOT(a, b, c);
        Rz(1.2345, a);
    }
    """
    app = QSharpApplication(code)
    arch = GateBased(gate_time=50, measurement_time=100)
    results = estimate(
        app,
        arch,
        SurfaceCode.q() * ExampleFactory.q(),
        PSSPC.q() * LatticeSurgery.q(),
        max_error=0.5,
    )

    assert len(results) >= 1

    results.add_column(
        "num_ts_per_rotation",
        lambda entry: entry.properties[NUM_TS_PER_ROTATION],
    )

    frame = results.as_frame()
    assert "num_ts_per_rotation" in frame.columns
    for val in frame["num_ts_per_rotation"]:
        assert isinstance(val, int)
        assert val >= 1


def test_estimation_table_insert_column_before_defaults():
    """Test inserting a name column before all default columns, similar to the factoring notebook."""
    code = """
    {
        use (a, b, c) = (Qubit(), Qubit(), Qubit());
        T(a);
        CCNOT(a, b, c);
        Rz(1.2345, a);
    }
    """
    app = QSharpApplication(code)
    arch = GateBased(gate_time=50, measurement_time=100)
    results = estimate(
        app,
        arch,
        SurfaceCode.q() * ExampleFactory.q(),
        PSSPC.q() * LatticeSurgery.q(),
        max_error=0.5,
        name="test_experiment",
    )

    assert len(results) >= 1

    # Add a factory summary at the end
    results.add_factory_summary_column()

    frame = results.as_frame()
    assert frame.columns[0] == "name"
    assert frame.columns[-1] == "factories"
    # Default columns should still be in order
    assert list(frame.columns[1:4]) == ["qubits", "runtime", "error"]


def test_estimation_table_as_frame_sortable():
    """Test that the DataFrame from as_frame can be sorted, as done in the factoring tests."""
    table = EstimationTable()
    table.append(_make_entry(300, 15000, 0.03))
    table.append(_make_entry(100, 5000, 0.01))
    table.append(_make_entry(200, 10000, 0.02))

    frame = table.as_frame()
    sorted_frame = frame.sort_values(by=["qubits", "runtime"]).reset_index(drop=True)

    assert list(sorted_frame["qubits"]) == [100, 200, 300]
    assert list(sorted_frame["error"]) == [0.01, 0.02, 0.03]


def test_estimation_table_computed_column():
    """Test adding a column that computes a derived value from the entry."""
    table = EstimationTable()
    table.append(_make_entry(100, 5_000_000, 0.01))
    table.append(_make_entry(200, 10_000_000, 0.02))

    # Compute qubits * error as a derived metric
    table.add_column("qubit_error_product", lambda e: e.qubits * e.error)

    frame = table.as_frame()
    assert frame["qubit_error_product"][0] == pytest.approx(1.0)
    assert frame["qubit_error_product"][1] == pytest.approx(4.0)
