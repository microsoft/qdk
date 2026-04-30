# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Noise Models for Quantum Simulation"""

from typing import Tuple


class PauliNoise(Tuple[float, float, float]):
    """
    The Pauli noise to use in simulation represented
    as probabilities of Pauli-X, Pauli-Y, and Pauli-Z errors
    """

    def __new__(cls, x: float, y: float, z: float):
        """
        Creates a new :class:`PauliNoise` instance with the given error probabilities.

        :param x: Probability of a Pauli-X (bit flip) error. Must be non-negative.
        :type x: float
        :param y: Probability of a Pauli-Y error. Must be non-negative.
        :type y: float
        :param z: Probability of a Pauli-Z (phase flip) error. Must be non-negative.
        :type z: float
        :return: A new :class:`PauliNoise` tuple ``(x, y, z)``.
        :rtype: PauliNoise
        :raises ValueError: If any probability is negative or if ``x + y + z > 1``.
        """
        if x < 0 or y < 0 or z < 0:
            raise ValueError("Pauli noise probabilities must be non-negative.")
        if x + y + z > 1:
            raise ValueError("The sum of Pauli noise probabilities must be at most 1.")
        return super().__new__(cls, (x, y, z))


class DepolarizingNoise(PauliNoise):
    """
    The depolarizing noise to use in simulation.
    """

    def __new__(cls, p: float):
        """
        Creates a new :class:`DepolarizingNoise` instance.

        The depolarizing channel applies Pauli-X, Pauli-Y, or Pauli-Z errors each with
        probability ``p / 3``.

        :param p: Total depolarizing error probability. Must satisfy ``0 ≤ p ≤ 1``.
        :type p: float
        :return: A new :class:`DepolarizingNoise` with equal X, Y, and Z error probabilities.
        :rtype: DepolarizingNoise
        :raises ValueError: If ``p`` is negative or ``p > 1``.
        """
        return super().__new__(cls, p / 3, p / 3, p / 3)


class BitFlipNoise(PauliNoise):
    """
    The bit flip noise to use in simulation.
    """

    def __new__(cls, p: float):
        """
        Creates a new :class:`BitFlipNoise` instance.

        The bit flip channel applies a Pauli-X error with probability ``p``.

        :param p: Probability of a bit flip (Pauli-X) error. Must satisfy ``0 ≤ p ≤ 1``.
        :type p: float
        :return: A new :class:`BitFlipNoise` with X error probability ``p``.
        :rtype: BitFlipNoise
        :raises ValueError: If ``p`` is negative or ``p > 1``.
        """
        return super().__new__(cls, p, 0, 0)


class PhaseFlipNoise(PauliNoise):
    """
    The phase flip noise to use in simulation.
    """

    def __new__(cls, p: float):
        """
        Creates a new :class:`PhaseFlipNoise` instance.

        The phase flip channel applies a Pauli-Z error with probability ``p``.

        :param p: Probability of a phase flip (Pauli-Z) error. Must satisfy ``0 ≤ p ≤ 1``.
        :type p: float
        :return: A new :class:`PhaseFlipNoise` with Z error probability ``p``.
        :rtype: PhaseFlipNoise
        :raises ValueError: If ``p`` is negative or ``p > 1``.
        """
        return super().__new__(cls, 0, 0, p)
