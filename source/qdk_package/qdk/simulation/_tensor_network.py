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
from ._simulation import (
    ContractionOptions,
    DecomposeCcxPass,
    MpsOptions,
    preprocess_simulation_input,
)

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

    The result is a float: the single-pass probability P_pass of those outcomes,
    without dividing by the acceptance probability of selection checks. It is
    the program-output probability only when every selection check passes with
    probability one. A record that fails a selection check raises ValueError.

    With ``method="contraction"`` the result is exact. With ``method="mps"`` it
    is the squared norm P̃(χ) of the MPS reached by applying each gate and, for
    each measurement, the operator |r⟩⟨b| (outcome b, leaving r = b, or r = 0
    after a reset), never renormalized. Truncation can move P̃(χ) above or below
    P, so check convergence by increasing ``max_bond_dimension`` until P̃(χ)
    agrees with a known value (for example 2⁻ᵐ) or stops changing. With
    ``method="mps"``, Probability cannot be combined with :class:`Expectation`
    in one call, because they read different states; see :class:`Cost`.
    """


@dataclass(frozen=True)
class Cost:
    """Query the resources of the evaluation.

    The result is a dict whose keys depend on the method. For
    ``method="contraction"`` it describes the network that ``Probability``
    contracts: ``"width"`` (log₂ of the largest intermediate tensor's element
    count), ``"flops"`` and ``"workspace_bytes"`` (minimum device scratch,
    not the allocated amount). Unreported quantities are ``None``. A Cost-only
    call plans and prepares but never contracts; an over-budget plan still
    reports the workspace it would need. Plans are currently unsliced. For
    ``method="mps"``: ``"max_bond_dimension"`` (largest bond reached),
    ``"state_bytes"`` and ``"workspace_bytes"`` of the one MPS the call
    computes: the fixed-outcome state when the call has a :class:`Probability`,
    otherwise the state :class:`Expectation` reads. A call therefore cannot mix
    Probability and Expectation with ``method="mps"``: one Cost could not
    describe both states. Evaluate them in separate calls.
    """


Query = Union[Expectation, Probability, Cost]


def tensornetwork_qir(
    input: Union[QirInputData, str, bytes],
    queries: Sequence[Query],
    *,
    method: Literal["mps", "contraction"],
    options: Optional[Union[MpsOptions, ContractionOptions]] = None,
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
        dimension is capped by ``options.max_bond_dimension``. With ``"mps"``,
        a call evaluates either :class:`Probability` or :class:`Expectation`
        queries (each optionally with :class:`Cost`), not both.
    :param options: :class:`MpsOptions` for ``method="mps"``, or
        :class:`ContractionOptions` for ``method="contraction"``.
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
    expected_options = MpsOptions if method == "mps" else ContractionOptions
    if options is not None and not isinstance(options, expected_options):
        raise TypeError(
            f'options must be a {expected_options.__name__} instance for method="{method}"'
        )
    if options is not None and options.device not in (None, "nvidia"):
        device_kind = "MPS" if method == "mps" else "contraction"
        raise ValueError(
            f"Unsupported {device_kind} device: {options.device!r}. "
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
        mps_options = options if isinstance(options, MpsOptions) else MpsOptions()
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
            program, [encoded[position] for position in network_positions], fixed_outcomes,
            _contraction_options_dict(
                options if isinstance(options, ContractionOptions) else None
            ),
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


def _contraction_options_dict(options: Optional[ContractionOptions]) -> dict[str, int]:
    options = options if options is not None else ContractionOptions()
    # The only place contraction defaults are filled; native code requires every
    # key. Eight trials match the state-query search effort; the tiny
    # qualification's single trial is too weak for large networks. Seed 17 fixes
    # the search's random choices; threads follow the host's cores.
    return {
        "hyper_samples": 8 if options.hyper_samples is None else options.hyper_samples,
        "seed": 17 if options.seed is None else options.seed,
    }


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
