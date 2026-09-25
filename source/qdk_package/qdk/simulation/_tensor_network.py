# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tensor-network queries on QIR programs.

Preview: names, arguments, and result formats may change.
"""

from dataclasses import dataclass
from numbers import Number
from typing import Any, Iterable, List, Literal, Optional, Sequence, Tuple, Union

from .._adaptive_pass import AdaptiveProfilePass, Bytecode
from .._native import Result
from .._types import QirInputData
from ._simulation import DecomposeCcxPass, MpsOptions, preprocess_simulation_input

PauliTerm = Tuple[str, Tuple[int, ...], complex]


@dataclass(frozen=True, init=False)
class Expectation:
    """Query the expectation value ⟨ψ|O|ψ⟩ of O = Σₖ cₖ·Pₖ.

    ψ is the state reached before the program's terminal measurements. Each
    term is ``(paulis, qubits, coefficient)``, in the same order as Qiskit's
    ``SparsePauliOp.from_sparse_list``: ``("ZZ", [0, 1], 0.5)`` is 0.5·Z₀Z₁.
    Qubits are QIR qubit indices. The result is a complex number, real up to
    rounding when O is Hermitian.
    """

    terms: Tuple[PauliTerm, ...]

    def __init__(self, terms: Iterable[Tuple[str, Sequence[int], complex]]) -> None:
        normalized = tuple(_pauli_term(term) for term in terms)
        if not normalized:
            raise ValueError("Expectation requires at least one term")
        object.__setattr__(self, "terms", normalized)


@dataclass(frozen=True)
class Probability:
    """Query the probability of the ``outcomes`` given to ``tensornetwork_qir``.

    The result is a float: the probability that the program produces exactly
    those outcomes for every measurement.
    """


@dataclass(frozen=True)
class Cost:
    """Query the resources of the evaluation.

    The result is a dict whose keys depend on the method. For
    ``method="contraction"`` it describes the network that ``Probability``
    contracts: ``"width"`` (log₂ of the largest intermediate tensor's element
    count), ``"flops"``, ``"slices"`` and ``"workspace_bytes"``. For
    ``method="mps"``: ``"max_bond_dimension"`` (largest bond reached),
    ``"state_bytes"`` and ``"workspace_bytes"``.
    """


Query = Union[Expectation, Probability, Cost]


def tensornetwork_qir(
    input: Union[QirInputData, str, bytes],
    queries: Sequence[Query],
    *,
    method: Literal["mps", "contraction"],
    options: Optional[MpsOptions] = None,
    outcomes: Optional[Sequence[Union[Result, int, bool]]] = None,
) -> List[Any]:
    """Evaluate queries on the tensor network of a QIR program.

    Preview: this API may change.

    ``run_qir`` returns shots. This function instead computes quantities
    directly from the network, without shot noise, and returns one result per
    query, in order: see :class:`Expectation`, :class:`Probability` and
    :class:`Cost`.

    :param input: The QIR program.
    :param queries: The queries to evaluate.
    :param method: ``"contraction"`` evaluates exactly with NVIDIA cuTensorNet.
        ``"mps"`` approximates the state as a matrix product state whose bond
        dimension is capped by ``options.max_bond_dimension``.
    :param options: :class:`MpsOptions`, only for ``method="mps"``.
    :param outcomes: ``outcomes[i]`` fixes the outcome of QIR result ``i``, which
        selects one path through mid-circuit measurements and branches.
        Required by :class:`Probability`, and by :class:`Cost` with
        ``method="contraction"``. A shot from ``run_qir`` can be passed directly
        when the program records every result in index order, as programs from
        ``qdk.stim.compile`` do.
    :return: One result per query.
    """
    if method not in ("mps", "contraction"):
        raise ValueError(
            f'Invalid method: {method!r}. Use "mps" or "contraction".'
        )
    if method == "contraction" and options is not None:
        raise ValueError('options can only be used with method="mps"')
    if options is not None and not isinstance(options, MpsOptions):
        raise TypeError("options must be an MpsOptions instance")
    mps_options = options if options is not None else MpsOptions()
    if mps_options.device not in (None, "nvidia"):
        raise ValueError(
            f"Unsupported MPS device: {mps_options.device!r}. "
            'Only device="nvidia" is accepted.'
        )

    queries = list(queries)
    if not queries:
        raise ValueError("queries must contain at least one query")
    for query in queries:
        if not isinstance(query, (Expectation, Probability, Cost)):
            raise TypeError(
                f"Unsupported query {query!r}; use Expectation, Probability or Cost"
            )
    needs_outcomes = [
        query
        for query in queries
        if isinstance(query, Probability)
        or (method == "contraction" and isinstance(query, Cost))
    ]
    if needs_outcomes and outcomes is None:
        raise ValueError(
            f'{type(needs_outcomes[0]).__name__} with method="{method}" requires outcomes'
        )
    fixed_outcomes = None if outcomes is None else _outcome_bits(outcomes)

    from .. import _native

    mod, _, _, _ = preprocess_simulation_input(input, 1, None, 0)
    DecomposeCcxPass().run(mod)
    program = AdaptiveProfilePass(Bytecode.Bit64).run(mod).as_dict()
    encoded = [_encode_query(query) for query in queries]

    if method == "mps":
        mps = {"max_bond_dimension": mps_options.max_bond_dimension}
        return list(
            _native._tensor_network_state_query(program, encoded, fixed_outcomes, mps)
        )

    # Exact expectation values come from cuTensorNet's state API; probability
    # and cost come from contracting the fixed-outcome network.
    results: List[Any] = [None] * len(queries)
    state_positions = [
        position
        for position, query in enumerate(queries)
        if isinstance(query, Expectation)
    ]
    network_positions = [
        position
        for position, query in enumerate(queries)
        if not isinstance(query, Expectation)
    ]
    if network_positions:
        values = _native._tensor_network_contraction_query(
            program, [encoded[position] for position in network_positions], fixed_outcomes
        )
        for position, value in zip(network_positions, values, strict=True):
            results[position] = value
    if state_positions:
        values = _native._tensor_network_state_query(
            program, [encoded[position] for position in state_positions], fixed_outcomes, None
        )
        for position, value in zip(state_positions, values, strict=True):
            results[position] = value
    return results


def _pauli_term(term: Any) -> PauliTerm:
    try:
        paulis, qubits, coefficient = term
    except (TypeError, ValueError):
        raise TypeError(
            f"Expectation term {term!r} must be (paulis, qubits, coefficient)"
        ) from None
    if not isinstance(paulis, str) or not paulis or set(paulis) - set("IXYZ"):
        raise ValueError(
            f"Expectation term {term!r}: paulis must be a non-empty string of I, X, Y, Z"
        )
    qubits = tuple(qubits)
    if any(isinstance(q, bool) or not isinstance(q, int) or q < 0 for q in qubits):
        raise ValueError(
            f"Expectation term {term!r}: qubits must be non-negative integers"
        )
    if len(qubits) != len(paulis):
        raise ValueError(
            f"Expectation term {term!r}: needs one qubit per Pauli"
        )
    if len(set(qubits)) != len(qubits):
        raise ValueError(f"Expectation term {term!r}: qubits must be distinct")
    if isinstance(coefficient, bool) or not isinstance(coefficient, Number):
        raise TypeError(f"Expectation term {term!r}: coefficient must be a number")
    return (paulis, qubits, complex(coefficient))  # type: ignore[arg-type]


def _outcome_bits(outcomes: Sequence[Union[Result, int, bool]]) -> List[bool]:
    bits = []
    for index, outcome in enumerate(outcomes):
        if outcome == Result.Zero or (isinstance(outcome, int) and outcome == 0):
            bits.append(False)
        elif outcome == Result.One or (isinstance(outcome, int) and outcome == 1):
            bits.append(True)
        else:
            raise ValueError(
                f"outcomes[{index}] is {outcome!r}; use Result.Zero, Result.One, 0 or 1"
            )
    return bits


def _encode_query(query: Query) -> dict:
    if isinstance(query, Expectation):
        return {
            "kind": "expectation",
            "terms": [
                (paulis, list(qubits), coefficient)
                for paulis, qubits, coefficient in query.terms
            ],
        }
    if isinstance(query, Probability):
        return {"kind": "probability"}
    return {"kind": "cost"}
