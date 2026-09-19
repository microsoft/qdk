# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import cmath
import json
import math
from pathlib import Path
import shutil

import numpy as np
import pytest

from i3a_reference import generate, verify_inputs


FIXTURES = Path(__file__).parent / "fixtures" / "i3a_numerical"


def test_retained_inputs_and_oracles_replay_independently():
    verify_inputs(FIXTURES)


def test_diagnostic_oracle_agrees_with_existing_i2_analytic_expression():
    a, b, phi, c = 0.7, -0.3, 0.41, 0.29

    def rx(angle, out, inp):
        return math.cos(angle / 2) if out == inp else -1j * math.sin(angle / 2)

    def phase(left, right):
        return cmath.exp(-0.5j * phi * (-1) ** (left + right))

    expected = np.zeros(8, dtype=np.complex128)
    for out0 in range(2):
        for out2 in range(2):
            expected[out0 + 4 * out2] = sum(
                rx(a, in0, 0) * rx(a, in2, 0) * phase(in0, in2)
                * rx(b, out0, in0) * phase(out0, in2) * rx(c, out2, in2)
                for in0 in range(2) for in2 in range(2)
            )
    actual = np.load(FIXTURES / "diagnostic.npy", allow_pickle=False)
    np.testing.assert_allclose(actual, expected, atol=1e-12, rtol=0)
    assert not np.allclose(actual[[1, 4]], actual[[4, 1]], atol=1e-12, rtol=0)


def test_generation_preserves_an_existing_directory(tmp_path):
    sentinel = tmp_path / "sentinel"
    sentinel.write_text("preserve")
    with pytest.raises(FileExistsError, match="overwrite"):
        generate(tmp_path)
    assert sentinel.read_text() == "preserve"


def test_changed_fixture_bytes_are_not_accepted(tmp_path):
    copy = tmp_path / "copy"
    shutil.copytree(FIXTURES, copy)
    record_path = copy / "case_a_2x2.json"
    record = json.loads(record_path.read_text())
    record["gates"][0][1] += 1
    record_path.write_text(json.dumps(record))
    with pytest.raises(ValueError, match="Artifact hash mismatch"):
        verify_inputs(copy)
