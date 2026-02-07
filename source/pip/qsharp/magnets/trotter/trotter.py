# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Base Trotter class for first- and second-order Trotter-Suzuki decomposition."""


from typing import Iterator


class TrotterStep:
    """
    Base class for Trotter decompositions. Essentially, this is a wrapper around
    a list of (time, term_index) tuples, which specify which term to apply for
    how long.

    As a default, the base class implements the first-order Trotter-Suzuki formula
    for approximating time evolution under a Hamiltonian represented as a sum of
    terms H = ∑_k H_k by sequentially applying each term for the full time

    e^{-i H t} ≈ ∏_k e^{-i H_k t}.

    Example:

    .. code-block:: python
        >>> trotter = TrotterStep(num_terms=3, time=0.5)
        >>> list(trotter.step())
        [(0.5, 0), (0.5, 1), (0.5, 2)]
    """

    def __init__(self, num_terms: int, time: float):
        """
        Initialize the Trotter decomposition.

        Args:
            num_terms: Number of terms in the Hamiltonian
            time: Total time for the evolution
        """
        self.terms: list[tuple[float, int]] = [
            (time, term_index) for term_index in range(num_terms)
        ]
        self._nterms = num_terms
        self._time_step = time
        self.order = 1

    @property
    def nterms(self) -> int:
        """Get the number of terms in the Hamiltonian."""
        return self._nterms

    @property
    def time_step(self) -> float:
        """Get the time step for each term in the Trotter decomposition."""
        return self._time_step

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
        return f"1st order Trotter expansion: time_step={self.time_step}, num_terms={self.nterms}"

    def __repr__(self) -> str:
        """String representation of the Trotter decomposition."""
        return f"Trotter(time_step={self.time_step}, num_terms={self.nterms})"


class StrangStep(TrotterStep):
    """
    Strang splitting (second-order Trotter-Suzuki decomposition).

    The second-order Trotter formula uses symmetric splitting:
    e^{-i H t} ≈ ∏_{k=1}^{n} e^{-i H_k t/2} ∏_{k=n}^{1} e^{-i H_k t/2}

    This provides second-order accuracy in the time step, compared to
    first-order for the basic Trotter decomposition.

    Example:

    .. code-block:: python
        >>> strang = StrangStep(num_terms=3, time=0.5)
        >>> list(strang.step())
        [(0.25, 0), (0.25, 1), (0.5, 2), (0.25, 1), (0.25, 0)]
    """

    def __init__(self, num_terms: int, time: float):
        """
        Initialize the Strang splitting.

        Args:
            num_terms: Number of terms in the Hamiltonian
            time: Total time for the evolution
        """
        self.terms: list[tuple[float, int]] = []
        for term_index in range(num_terms - 1):
            self.terms.append((time / 2, term_index))
        self.terms.append((time, num_terms - 1))
        for term_index in reversed(range(num_terms - 1)):
            self.terms.append((time / 2, term_index))

        self._nterms = num_terms
        self._time_step = time
        self.order = 2

    def step(self) -> Iterator[tuple[float, int]]:
        """
        Iterate over the Strang splitting as a list of (time, term_index) tuples.

        Returns:
            Iterator of tuples where each tuple contains the time duration and the
            index of the term to be applied. The sequence is symmetric for
            second-order accuracy.
        """
        return iter(self.terms)

    def __str__(self) -> str:
        return f"2nd order Trotter expansion: time_step={self.time_step}, num_terms={self.nterms}"

    def __repr__(self) -> str:
        """String representation of the Strang splitting."""
        return f"Strang(time_step={self.time_step}, num_terms={self.nterms})"


class TrotterExpansion:
    """
    Trotter expansion class for multiple Trotter steps. This class wraps around
    a TrotterStep instance and specifies how many times to repeat this Trotter
    step. The expansion can be used to represent the full time evolution
    as a sequence of Trotter steps

        e^{-i H t} ≈ (∏_k e^{-i H_k t/n})^n.

    where n is the number of Trotter steps.

    Example:

    .. code-block:: python
        >>> n = 4  # Number of Trotter steps
        >>> total_time = 1.0  # Total time
        >>> trotter_expansion = TrotterExpansion(TrotterStep(2, total_time/n), n)
        >>> trotter_expansion.get()
        [([(0.25, 0), (0.25, 1)], 4)]
    """

    def __init__(self, trotter_step: TrotterStep, num_steps: int):
        """
        Initialize the Trotter expansion.

        Args:
            trotter_step: An instance of TrotterStep representing a single Trotter step
            num_steps: Number of Trotter steps
        """
        self._trotter_step = trotter_step
        self._num_steps = num_steps

    def get(self) -> list[tuple[list[tuple[float, int]], int]]:
        """
        Get the Trotter expansion as a list of (terms, step_index) tuples.

        Returns:
            List of tuples where each tuple contains the list of (time, term_index)
            for that step and the number of times that step is executed.
        """
        return [(self._trotter_step.get(), self._num_steps)]
