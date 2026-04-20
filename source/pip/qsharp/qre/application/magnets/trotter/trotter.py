# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Trotter schedule utilities for magnet models.

This module provides:

- ``TrotterStep``: a schedule of ``(time, term_index)`` entries,
- recursion helpers (Suzuki and Yoshida) that raise the order by 2,
- factory helpers such as Strang splitting, and
- ``TrotterExpansion`` to apply a step repeatedly to a concrete model.
"""

from collections.abc import Callable
from typing import Iterator, Optional
from ..models import Model
from ..utilities import PauliString

import math

try:
    import cirq
except Exception as ex:
    raise ImportError(
        "qsharp.magnets.models requires the cirq extras. Install with 'pip install \"qsharp[cirq]\"'."
    ) from ex


class TrotterStep:
    """Schedule of Hamiltonian-term applications for one Trotter step.

    A ``TrotterStep`` stores an ordered list of ``(time, term_index)`` tuples.
    Each tuple indicates that term group ``term_index`` should be applied for
    evolution time ``time``.

    The constructor builds a first-order step over the provided term indices:

    .. math::

        e^{-i H t} \\approx \\prod_k e^{-i H_k t}, \\quad H = \\sum_k H_k.

    where each supplied term index appears once with duration ``time_step``.
    """

    def __init__(self, terms: list[int] = [], time_step: float = 0.0):
        """Initialize a Trotter step from explicit term indices.

        Args:
            terms: Ordered term indices to include in this step.
            time_step: Duration associated with each listed term.

        Notes:
            If ``terms`` is empty, the step is initialized as order 0.
            Otherwise, it is initialized as order 1.
        """
        self._nterms = len(terms)
        self._time_step = time_step
        self._order = 1 if self._nterms > 0 else 0
        self._repr_string: Optional[str] = None
        self.terms: list[tuple[float, int]] = [(time_step, j) for j in terms]

    @property
    def order(self) -> int:
        """Get the order of the Trotter decomposition."""
        return self._order

    @property
    def nterms(self) -> int:
        """Get the number of term entries used to build this schedule."""
        return self._nterms

    @property
    def time_step(self) -> float:
        """Get the base time step metadata stored on this step."""
        return self._time_step

    def reduce(self) -> None:
        """
        Reduce the Trotter step in place by combining consecutive terms that are the same.

        This can be useful for optimizing the Trotter sequence by merging adjacent
        applications of the same term into a single application with a longer time step.

        Example:
        >>> trotter = TrotterStep()
        >>> trotter.terms = [(0.5, 0), (0.5, 0), (0.5, 1)]
        >>> trotter.reduce()
        >>> list(trotter.step())
        [(1.0, 0), (0.5, 1)]
        """
        if len(self.terms) > 1:
            reduced_terms: list[tuple[float, int]] = []
            current_time, current_term = self.terms[0]

            for time, term in self.terms[1:]:
                if term == current_term:
                    current_time += time
                else:
                    reduced_terms.append((current_time, current_term))
                    current_time, current_term = time, term

            reduced_terms.append((current_time, current_term))
            self.terms = reduced_terms

    def step(self) -> Iterator[tuple[float, int]]:
        """Iterate over ``(time, term_index)`` entries for this step."""
        return iter(self.terms)

    def cirq(self, model: Model) -> cirq.Circuit:
        """Build a Cirq circuit for one application of this Trotter step.

        Args:
            model: Model that maps each term index to grouped Pauli operators.

        Returns:
            A ``cirq.Circuit`` containing ``cirq.PauliStringPhasor`` operations
            in the same order as ``self.step()``.
        """
        _INT_TO_CIRQ = (cirq.I, cirq.X, cirq.Z, cirq.Y)
        circuit = cirq.Circuit()
        for time, term_index in self.step():
            for color in model.colors(term_index):
                for op in model.ops(term_index, color):
                    pauli = cirq.PauliString(
                        {
                            cirq.LineQubit(p.qubit): _INT_TO_CIRQ[p.op]
                            for p in op._paulis
                        },
                    )
                    oper = cirq.PauliStringPhasor(pauli, exponent_neg=time / math.pi)
                    circuit.append(oper)
        return circuit

    def __str__(self) -> str:
        """String representation of the Trotter decomposition."""
        return f"Trotter expansion of order {self._order}: time_step={self._time_step}, num_terms={self._nterms}"

    def __repr__(self) -> str:
        """String representation of the Trotter decomposition."""
        if self._repr_string is not None:
            return self._repr_string
        else:
            return f"TrotterStep(num_terms={self._nterms}, time_step={self._time_step})"


def suzuki_recursion(trotter: TrotterStep) -> TrotterStep:
    """
    Apply one level of Suzuki recursion to double the order of a Trotter step.

    Given a k-th order Trotter step S_k(t), this function constructs a (k+2)-nd order
    step using the Suzuki fractal decomposition:

        S_{k+2}(t) = S_{k}(p t) S_{k}(p t) S_{k}((1 - 4p) t) S_{k}(p t) S_{k}(p t)

    where p = 1 / (4 - 4^{1/(2k+1)}).

    The resulting step has improved accuracy: the error scales as O(t^{k+3}) instead
    of O(t^{k+1}), at the cost of 5x more exponential applications per step.

    Args:
        trotter: A TrotterStep of order k to be promoted to order k+2.

    Returns:
        A new TrotterStep of order k+2 constructed via Suzuki recursion.

    References:
        M. Suzuki, Phys. Lett. A 146, 319 (1990).
    """

    suzuki = TrotterStep()
    suzuki._nterms = trotter._nterms
    suzuki._time_step = trotter._time_step
    suzuki._order = trotter._order + 2
    suzuki._repr_string = f"SuzukiRecursion(order={suzuki._order}, time_step={suzuki._time_step}, num_terms={suzuki._nterms})"

    p = 1 / (4 - 4 ** (1 / (2 * trotter.order + 1)))

    suzuki.terms = [(p * time, term_index) for time, term_index in trotter.step()]
    suzuki.terms += [(p * time, term_index) for time, term_index in trotter.step()]
    suzuki.terms += [
        ((1 - 4 * p) * time, term_index) for time, term_index in trotter.step()
    ]
    suzuki.terms += [(p * time, term_index) for time, term_index in trotter.step()]
    suzuki.terms += [(p * time, term_index) for time, term_index in trotter.step()]
    suzuki.reduce()  # Combine consecutive terms that are the same

    return suzuki


def yoshida_recursion(trotter: TrotterStep) -> TrotterStep:
    """
    Apply one level of Yoshida recursion to increase the order of a Trotter step by 2.

    Given a k-th order Trotter step S_k(t), this function constructs a (k+2)-nd order
    step using Yoshida's symmetric triple-jump composition:

        S_{k+2}(t) = S_{k}(w_1 t) S_{k}(w_0 t) S_{k}(w_1 t)

    where:
        w_1 = 1 / (2 - 2^{1/(2k+1)})
        w_0 = -2^{1/(2k+1)} / (2 - 2^{1/(2k+1)}) = 1 - 2 w_1

    The resulting step has improved accuracy: the error scales as O(t^{k+3}) instead
    of O(t^{k+1}), at the cost of 3x more exponential applications per step.

    Args:
        trotter: A TrotterStep of order k to be promoted to order k+2.

    Returns:
        A new TrotterStep of order k+2 constructed via Yoshida recursion.

    References:
        H. Yoshida, Phys. Lett. A 150, 262 (1990).
    """

    yoshida = TrotterStep()
    yoshida._nterms = trotter._nterms
    yoshida._time_step = trotter._time_step
    yoshida._order = trotter._order + 2
    yoshida._repr_string = f"YoshidaRecursion(order={yoshida._order}, time_step={yoshida._time_step}, num_terms={yoshida._nterms})"

    cube_root_2 = 2 ** (1 / (2 * trotter.order + 1))
    w1 = 1 / (2 - cube_root_2)
    w0 = 1 - 2 * w1  # equivalent to -cube_root_2 / (2 - cube_root_2)

    yoshida.terms = [(w1 * time, term_index) for time, term_index in trotter.step()]
    yoshida.terms += [(w0 * time, term_index) for time, term_index in trotter.step()]
    yoshida.terms += [(w1 * time, term_index) for time, term_index in trotter.step()]
    yoshida.reduce()  # Combine consecutive terms that are the same

    return yoshida


def strang_splitting(terms: list[int], time: float) -> TrotterStep:
    """
    Create a second-order Strang splitting schedule for explicit term indices.

    The second-order Trotter formula uses symmetric splitting:

    e^{-i H t} \\approx \\prod_{k=1}^{n-1} e^{-i H_k t/2} \\, e^{-i H_n t} \\, \\prod_{k=n-1}^{1} e^{-i H_k t/2}

    This provides second-order accuracy in the time step, compared to
    first-order for the basic Trotter decomposition.

    Example:

    .. code-block:: python
        >>> strang = strang_splitting(terms=[0, 1, 2], time=0.5)
        >>> list(strang.step())
        [(0.25, 0), (0.25, 1), (0.5, 2), (0.25, 1), (0.25, 0)]

    Args:
        terms: Ordered term indices for a single symmetric step. Must be non-empty.
        time: Total evolution time assigned to this second-order step.

    Returns:
        A second-order ``TrotterStep``.

    References:
        G. Strang, SIAM J. Numer. Anal. 5, 506 (1968).
    """
    strang = TrotterStep()
    strang._nterms = len(terms)
    strang._time_step = time
    strang._order = 2
    strang._repr_string = f"StrangSplitting(time_step={time}, num_terms={len(terms)})"
    strang.terms = []
    for i in range(len(terms) - 1):
        strang.terms.append((time / 2, terms[i]))
    strang.terms.append((time, terms[-1]))
    for i in reversed(range(len(terms) - 1)):
        strang.terms.append((time / 2, terms[i]))
    return strang


def fourth_order_trotter_suzuki(terms: list[int], time: float) -> TrotterStep:
    """
    Factory function for creating a fourth-order Trotter-Suzuki decomposition
    using Suzuki recursion.

    This is obtained by applying one level of Suzuki recursion to the second-order
    Strang splitting. The resulting fourth-order decomposition has improved accuracy
    compared to the second-order Strang splitting, at the cost of more exponential
    applications per step.

    Example:

    .. code-block:: python
        >>> fourth_order = fourth_order_trotter_suzuki(terms=[0, 1, 2], time=0.5)
        >>> list(fourth_order.step())
        [(0.1767766952966369, 0), (0.1767766952966369, 1), (0.1767766952966369, 2), (0.3535533905932738, 1), (0.3535533905932738, 0), (0.1767766952966369, 1), (0.1767766952966369, 2), (0.1767766952966369, 1), (0.1767766952966369, 0)]
    """
    return suzuki_recursion(strang_splitting(terms, time))


class TrotterExpansion:
    """Repeated application of a Trotter method on a concrete model.

    ``TrotterExpansion`` builds one step with ``trotter_method(model.terms, dt)``
    where ``dt = time / num_steps`` and then repeats it ``num_steps`` times.

    Iteration via :meth:`step` yields ``PauliString`` operators already scaled by
    the per-entry schedule time.
    """

    def __init__(
        self,
        trotter_method: Callable[[list[int], float], TrotterStep],
        model: Model,
        time: float,
        num_steps: int,
    ):
        """Initialize a repeated-step Trotter expansion.

        Args:
            trotter_method: Callable mapping ``(terms, dt)`` to a ``TrotterStep``.
            model: Model that defines term groups and per-term operators.
            time: Total evolution time.
            num_steps: Number of repeated Trotter steps.
        """
        self._model = model
        self._num_steps = num_steps
        self._trotter_step = trotter_method(model.terms, time / num_steps)

    @property
    def order(self) -> int:
        """Get the order of the underlying Trotter step."""
        return self._trotter_step.order

    @property
    def nterms(self) -> int:
        """Get the number of Hamiltonian terms."""
        return self._model.nterms

    @property
    def nsteps(self) -> int:
        """Get the number of Trotter steps."""
        return self._num_steps

    @property
    def total_time(self) -> float:
        """Get the total evolution time (time_step * num_steps)."""
        return self._trotter_step.time_step * self._num_steps

    def step(self) -> Iterator[PauliString]:
        """Iterate over scaled operators for the full expansion.

        Yields:
            ``PauliString`` operators with coefficients scaled by schedule time,
            in execution order across all repeated steps.
        """
        for _ in range(self._num_steps):
            for s, i in self._trotter_step.step():
                for c in self._model.colors(i):
                    for op in self._model.ops(i, c):
                        yield (op * s)

    def cirq(self) -> cirq.CircuitOperation:
        """Get a repeated Cirq circuit operation for this expansion."""
        circuit = self._trotter_step.cirq(self._model).freeze()
        return cirq.CircuitOperation(circuit, repetitions=self._num_steps)

    def __str__(self) -> str:
        """String representation of the Trotter expansion."""
        return (
            f"TrotterExpansion(order={self.order}, num_steps={self._num_steps}, "
            f"total_time={self.total_time}, num_terms={self.nterms})"
        )

    def __repr__(self) -> str:
        """Repr representation of the Trotter expansion."""
        return f"TrotterExpansion({self._trotter_step!r}, num_steps={self._num_steps})"

