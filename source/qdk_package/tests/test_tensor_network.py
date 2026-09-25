# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from typing import Any, Literal, cast

import pytest

import qdk.simulation
from qdk._native import Result
from qdk.simulation import (
    Cost,
    Expectation,
    MpsOptions,
    Probability,
    tensornetwork_qir,
)

# Arguments are validated before the program is parsed, so these tests need no
# valid program.
QIR = "not parsed"


def test_queries_are_publicly_exported():
    for name in ("tensornetwork_qir", "Expectation", "Probability", "Cost"):
        assert name in qdk.simulation.__all__


def test_expectation_normalizes_terms():
    query = Expectation([("ZZ", [0, 1], 0.5), ("X", (2,), 1)])
    assert query.terms == (("ZZ", (0, 1), 0.5 + 0j), ("X", (2,), 1 + 0j))


@pytest.mark.parametrize(
    "term",
    [
        ("ZA", [0, 1], 1.0),
        ("", [], 1.0),
        ("ZZ", [0], 1.0),
        ("ZZ", [1, 1], 1.0),
        ("Z", [-1], 1.0),
        ("Z", [True], 1.0),
    ],
)
def test_expectation_rejects_invalid_terms(term: Any):
    with pytest.raises(ValueError):
        Expectation([term])


@pytest.mark.parametrize("term", [("Z", [0]), ("Z", [0], "1"), ("Z", [0], True)])
def test_expectation_rejects_malformed_terms(term: Any):
    with pytest.raises(TypeError):
        Expectation([term])


def test_expectation_requires_a_term():
    with pytest.raises(ValueError, match="at least one term"):
        Expectation([])


def test_rejects_unknown_method():
    with pytest.raises(ValueError, match="Invalid method"):
        tensornetwork_qir(QIR, [Cost()], method=cast(Any, "dense"))


def test_rejects_options_with_contraction():
    with pytest.raises(ValueError, match='only be used with method="mps"'):
        tensornetwork_qir(
            QIR, [Cost()], method="contraction", options=MpsOptions(), outcomes=[0]
        )


def test_rejects_options_of_another_type():
    with pytest.raises(TypeError, match="MpsOptions"):
        tensornetwork_qir(QIR, [Cost()], method="mps", options=cast(Any, {}))


def test_rejects_unsupported_device():
    with pytest.raises(ValueError, match="Unsupported MPS device"):
        tensornetwork_qir(
            QIR, [Cost()], method="mps", options=MpsOptions(device=cast(Any, "cpu"))
        )


def test_requires_a_query():
    with pytest.raises(ValueError, match="at least one query"):
        tensornetwork_qir(QIR, [], method="mps")


def test_rejects_unknown_queries():
    with pytest.raises(TypeError, match="Unsupported query"):
        tensornetwork_qir(QIR, [cast(Any, "cost")], method="mps")


@pytest.mark.parametrize("method", ["mps", "contraction"])
def test_probability_requires_outcomes(method: Literal["mps", "contraction"]):
    with pytest.raises(ValueError, match="Probability .* requires outcomes"):
        tensornetwork_qir(QIR, [Probability()], method=method)


def test_contraction_cost_requires_outcomes():
    with pytest.raises(ValueError, match="Cost .* requires outcomes"):
        tensornetwork_qir(QIR, [Cost()], method="contraction")


@pytest.mark.parametrize("outcome", [Result.Loss, 2, "0"])
def test_rejects_invalid_outcomes(outcome: Any):
    with pytest.raises(ValueError, match=r"outcomes\[1\]"):
        tensornetwork_qir(
            QIR, [Probability()], method="mps", outcomes=[0, cast(Any, outcome)]
        )
