# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Base Trotter class for first- and second-order Trotter-Suzuki decomposition."""


from typing import Iterator


class TrotterStep:
    """
    Base class for Trotter decompositions. Essentially, this is a wrapper around a
    list of (time, term_index) tuples, which specify which term to apply for how long.

    The TrotterStep class provides a common interface for different Trotter decompositions,
    such as first-order Trotter and Strang splitting. It also serves as the base class for
    higher-order Trotter steps that can be constructed via Suzuki or Yoshida recursion. Each
    Trotter step is defined by the sequence of terms to apply and their corresponding time
    durations, as well as the overall order of the decomposition and the time step for each term.
    """

    def __init__(self):
        """
        Creates an empty Trotter decomposition.

        """
        self.terms: list[tuple[float, int]] = []
        self._nterms = 0
        self._time_step = 0.0
        self._order = 0
        self._repr_string = "TrotterStep()"

    @property
    def order(self) -> int:
        """Get the order of the Trotter decomposition."""
        return self._order

    @property
    def nterms(self) -> int:
        """Get the number of terms in the Hamiltonian."""
        return self._nterms

    @property
    def time_step(self) -> float:
        """Get the time step for each term in the Trotter decomposition."""
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
        """
        Iterate over the Trotter decomposition as a list of (time, term_index) tuples.

        Returns:
            Iterator of tuples where each tuple contains the time duration and the
            index of the term to be applied.
        """
        return iter(self.terms)

    def __str__(self) -> str:
        """String representation of the Trotter decomposition."""
        return f"Trotter expansion of order {self._order}: time_step={self._time_step}, num_terms={self._nterms}"

    def __repr__(self) -> str:
        """String representation of the Trotter decomposition."""
        return self._repr_string


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


def trotter_decomposition(num_terms: int, time: float) -> TrotterStep:
    """
    Factory function for creating a first-order Trotter decomposition.

    The first-order Trotter-Suzuki formula for approximating time evolution
    under a Hamiltonian represented as a sum of terms

    H = ∑_k H_k

    is obtained by sequentially applying each term for the full time

    e^{-i H t} ≈ ∏_k e^{-i H_k t}.

    Example:

    .. code-block:: python
        >>> trotter = first_order_trotter(num_terms=3, time=0.5)
        >>> list(trotter.step())
        [(0.5, 0), (0.5, 1), (0.5, 2)]

    References:
        H. F. Trotter, Proc. Amer. Math. Soc. 10, 545 (1959).
    """
    trotter = TrotterStep()
    trotter.terms = [(time, term_index) for term_index in range(num_terms)]
    trotter._nterms = num_terms
    trotter._time_step = time
    trotter._order = 1
    trotter._repr_string = f"FirstOrderTrotter(time_step={time}, num_terms={num_terms})"
    return trotter


def strang_splitting(num_terms: int, time: float) -> TrotterStep:
    """
    Factory function for creating a Strang splitting (second-order
    Trotter-Suzuki decomposition).

    The second-order Trotter formula uses symmetric splitting:

    e^{-i H t} ≈ ∏_{k=1}^{n} e^{-i H_k t/2} ∏_{k=n}^{1} e^{-i H_k t/2}

    This provides second-order accuracy in the time step, compared to
    first-order for the basic Trotter decomposition.

    Example:

    .. code-block:: python
        >>> strang = strang_splitting(num_terms=3, time=0.5)
        >>> list(strang.step())
        [(0.25, 0), (0.25, 1), (0.5, 2), (0.25, 1), (0.25, 0)]

    References:
        G. Strang, SIAM J. Numer. Anal. 5, 506 (1968).
    """
    strang = TrotterStep()
    strang._nterms = num_terms
    strang._time_step = time
    strang._order = 2
    strang._repr_string = f"StrangSplitting(time_step={time}, num_terms={num_terms})"
    strang.terms = []
    for term_index in range(num_terms - 1):
        strang.terms.append((time / 2, term_index))
    strang.terms.append((time, num_terms - 1))
    for term_index in reversed(range(num_terms - 1)):
        strang.terms.append((time / 2, term_index))
    return strang


def fourth_order_trotter_suzuki(num_terms: int, time: float) -> TrotterStep:
    """
    Factory function for creating a fourth-order Trotter-Suzuki decomposition
    using Suzuki recursion.

    This is obtained by applying one level of Suzuki recursion to the second-order
    Strang splitting. The resulting fourth-order decomposition has improved accuracy
    compared to the second-order Strang splitting, at the cost of more exponential
    applications per step.

    Example:

    .. code-block:: python
        >>> fourth_order = fourth_order_trotter_suzuki(num_terms=3, time=0.5)
        >>> list(fourth_order.step())
        [(0.1767766952966369, 0), (0.1767766952966369, 1), (0.1767766952966369, 2), (0.3535533905932738, 1), (0.3535533905932738, 0), (0.1767766952966369, 1), (0.1767766952966369, 2), (0.1767766952966369, 1), (0.1767766952966369, 0)]
    """
    return suzuki_recursion(strang_splitting(num_terms, time))


class TrotterExpansion:
    """
    Trotter expansion for repeated application of a Trotter step.

    This class wraps a TrotterStep instance and specifies how many times to repeat
    the step. The expansion represents full time evolution as a sequence of
    Trotter steps:

        e^{-i H T} ≈ (S(T/n))^n

    where S is the Trotter step formula, T is the total time, and n is the number
    of steps.

    Example:

    .. code-block:: python
        >>> n = 4  # Number of Trotter steps
        >>> total_time = 1.0  # Total time
        >>> step = trotter_decomposition(num_terms=2, time=total_time/n)
        >>> expansion = TrotterExpansion(step, n)
        >>> expansion.order
        1
        >>> expansion.total_time
        1.0
        >>> list(expansion.step())[:4]
        [(0.25, 0), (0.25, 1), (0.25, 0), (0.25, 1)]
    """

    def __init__(self, trotter_step: TrotterStep, num_steps: int):
        """
        Initialize the Trotter expansion.

        Args:
            trotter_step: An instance of TrotterStep representing a single Trotter step.
            num_steps: Number of times to repeat the Trotter step.
        """
        self._trotter_step = trotter_step
        self._num_steps = num_steps

    @property
    def order(self) -> int:
        """Get the order of the underlying Trotter step."""
        return self._trotter_step.order

    @property
    def nterms(self) -> int:
        """Get the number of Hamiltonian terms."""
        return self._trotter_step.nterms

    @property
    def num_steps(self) -> int:
        """Get the number of Trotter steps."""
        return self._num_steps

    @property
    def total_time(self) -> float:
        """Get the total evolution time (time_step * num_steps)."""
        return self._trotter_step.time_step * self._num_steps

    def step(self) -> Iterator[tuple[float, int]]:
        """
        Iterate over the full Trotter expansion.

        Yields all (time, term_index) tuples for the complete expansion,
        repeating the Trotter step sequence num_steps times.

        Returns:
            Iterator of (time, term_index) tuples for the full evolution.
        """
        for _ in range(self._num_steps):
            yield from self._trotter_step.step()

    def get(self) -> list[tuple[list[tuple[float, int]], int]]:
        """
        Get the Trotter expansion as a compact representation.

        Returns:
            List containing a single tuple of (terms, num_steps) where terms
            is the list of (time, term_index) for one step.
        """
        return [(list(self._trotter_step.step()), self._num_steps)]

    def __str__(self) -> str:
        """String representation of the Trotter expansion."""
        return (
            f"TrotterExpansion(order={self.order}, num_steps={self._num_steps}, "
            f"total_time={self.total_time}, num_terms={self.nterms})"
        )

    def __repr__(self) -> str:
        """Repr representation of the Trotter expansion."""
        return f"TrotterExpansion({self._trotter_step!r}, num_steps={self._num_steps})"
