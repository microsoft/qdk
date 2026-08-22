"""Equivalence predicates: does one artifact do the same thing as another?

These are the "test" half of develop/test/deploy — the questions an author asks
when refactoring a gadget, swapping in a cheaper circuit, or checking a draft
against a reference implementation.

The predicates come in two strengths:

* :func:`codes_equivalent` / :func:`gadgets_equivalent` compare whole artifacts,
  with :func:`why_not_equivalent` explaining a negative gadget answer.
* :func:`actions_equivalent_mod_pauli` / :func:`actions_outcome_equivalent`
  compare two already-computed
  :class:`~qdk.ec.action.CircuitAction` objects, ignoring Pauli frames
  and comparing only measurement outcomes respectively.
"""

from ._analysis.circuit_action import (
    are_equivalent_mod_paulis as actions_equivalent_mod_pauli,
    are_outcome_equivalent as actions_outcome_equivalent,
)
from ._analysis.equivalence import gadgets_equivalent, why_not_equivalent
from .code import codes_equivalent

__all__ = [
    "actions_equivalent_mod_pauli",
    "actions_outcome_equivalent",
    "codes_equivalent",
    "gadgets_equivalent",
    "why_not_equivalent",
]
