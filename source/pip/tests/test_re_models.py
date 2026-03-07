# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

from qsharp.estimator import PSSPCEstimator, SurfaceCode, RoundBasedFactory, QubitParams


@pytest.fixture
def qsharp():
    import qsharp
    import qsharp._fs

    qsharp._fs.read_file = read_file_memfs
    qsharp._fs.list_directory = list_directory_memfs
    qsharp._fs.exists = exists_memfs
    qsharp._fs.join = join_memfs
    qsharp._fs.resolve = resolve_memfs

    return qsharp


def test_estimation_from_project(qsharp):
    layout = qsharp.estimator.PSSPCEstimator("/project", "Test.Test()")

    assert layout.logical_qubits() == 15


def test_estimation_from_single_file(qsharp):
    layout = qsharp.estimator.PSSPCEstimator(["/SingleFile.qs"], "Test()")

    assert layout.logical_qubits() == 42


def test_estimation_comparison(qsharp):
    qsharp.init()

    source_file = "/SingleFile.qs"
    qsharp.eval(qsharp._fs.read_file(source_file)[1])

    for qubit_name in [
        QubitParams.GATE_US_E3,
        QubitParams.GATE_US_E4,
        QubitParams.GATE_NS_E3,
        QubitParams.GATE_NS_E4,
    ]:
        estimates = qsharp.estimate("Test()", {"qubitParams": {"name": qubit_name}})

        qubit = estimates["jobParams"]["qubitParams"]

        # Remove 'ns' suffix from time metrics
        for key, value in qubit.items():
            if isinstance(value, str) and value.endswith("ns"):
                value = int(value[:-3])
            qubit[key] = value

        estimates2 = qsharp.estimate_custom(
            PSSPCEstimator([source_file], "Test()"),
            qubit,
            SurfaceCode(
                one_qubit_gate_time="oneQubitGateTime",
                two_qubit_gate_time="twoQubitGateTime",
                measurement_time="oneQubitMeasurementTime",
                two_qubit_gate_error_rate="twoQubitGateErrorRate",
                logical_cycle_time_formula="(2 * measurement_time + 4 * two_qubit_gate_time) * distance",
            ),
            [
                RoundBasedFactory(
                    gate_error="tGateErrorRate",
                    gate_time="tGateTime",
                    clifford_error="twoQubitGateErrorRate",
                    use_max_qubits_per_round=True,
                )
            ],
            error_budget=0.001,
        )

        assert (
            estimates["physicalCounts"]["physicalQubits"]
            == estimates2["physicalQubits"]
        )
        assert estimates["physicalCounts"]["runtime"] == estimates2["runtime"]


memfs = {
    "": {
        "project": {
            "src": {
                "Test.qs": "operation Test() : Unit { use qs = Qubit[4]; ApplyToEach(T, qs); ResetAll(qs); }",
            },
            "qsharp.json": "{}",
        },
        "SingleFile.qs": "import Std.TableLookup.*; operation Test() : Unit { use address = Qubit[6]; use target = Qubit[5]; let data = [[true, size = 5], size = 32]; Select(data, address, target); ResetAll(address + target); }",
    }
}


def read_file_memfs(path):
    global memfs
    item = memfs
    for part in path.split("/"):
        if part in item:
            if isinstance(item[part], OSError):
                raise item[part]
            else:
                item = item[part]
        else:
            raise Exception("File not found: " + path)

    return (path, item)


def list_directory_memfs(dir_path):
    global memfs
    item = memfs
    for part in dir_path.split("/"):
        if part in item:
            item = item[part]
        else:
            raise Exception("Directory not found: " + dir_path)

    contents = list(
        map(
            lambda x: {
                "path": join_memfs(dir_path, x[0]),
                "entry_name": x[0],
                "type": "folder" if isinstance(x[1], dict) else "file",
            },
            item.items(),
        )
    )

    return contents


def exists_memfs(path):
    global memfs
    parts = path.split("/")
    item = memfs
    for part in parts:
        if part in item:
            item = item[part]
        else:
            return False

    return True


# The below functions force the use of `/` separators in the unit tests
# so that they function on Windows consistently with other platforms.
def join_memfs(path, *paths):
    return "/".join([path, *paths])


def resolve_memfs(base, path):
    parts = f"{base}/{path}".split("/")
    new_parts = []
    for part in parts:
        if part == ".":
            continue
        if part == "..":
            new_parts.pop()
            continue
        new_parts.append(part)
    return "/".join(new_parts)
