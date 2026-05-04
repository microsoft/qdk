# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Core type definitions for the qdk package.

This module contains the pure-Python types that are used across the qdk
package.  They have no dependency on the interpreter lifecycle and can be
imported freely by any submodule.

Types defined here:

- :class:`PauliNoise`, :class:`DepolarizingNoise`, :class:`BitFlipNoise`,
  :class:`PhaseFlipNoise` — noise models for simulation.
- :class:`StateDump` — sparse state-vector snapshot.
- :class:`ShotResult` — per-shot output container.
- :class:`Config` — interpreter configuration / language-service hint.
- :class:`QirInputData` — compiled QIR wrapper for azure-quantum submission.
"""

import os
from pathlib import Path
from typing import (
    Any,
    Dict,
    List,
    Optional,
    TypedDict,
    Union,
)

from ._native import (  # type: ignore
    Output,
    StateDumpData,
    TargetProfile,
)

# ---------------------------------------------------------------------------
# Noise models
# ---------------------------------------------------------------------------


class PauliNoise(tuple):
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


# ---------------------------------------------------------------------------
# State dump
# ---------------------------------------------------------------------------


class StateDump:
    """
    A state dump returned from the Q# interpreter.
    """

    """
    The number of allocated qubits at the time of the dump.
    """
    qubit_count: int

    __inner: dict
    __data: StateDumpData

    def __init__(self, data: StateDumpData):
        self.__data = data
        self.__inner = data.get_dict()
        self.qubit_count = data.qubit_count

    def __getitem__(self, index: int) -> complex:
        return self.__inner.__getitem__(index)

    def __iter__(self):
        return self.__inner.__iter__()

    def __len__(self) -> int:
        return len(self.__inner)

    def __repr__(self) -> str:
        return self.__data.__repr__()

    def __str__(self) -> str:
        return self.__data.__str__()

    def _repr_markdown_(self) -> str:
        return self.__data._repr_markdown_()

    def check_eq(
        self, state: Union[Dict[int, complex], List[complex]], tolerance: float = 1e-10
    ) -> bool:
        """
        Checks if the state dump is equal to the given state. This is not mathematical equality,
        as the check ignores global phase.

        :param state: The state to check against, provided either as a dictionary of state indices to complex amplitudes,
            or as a list of real amplitudes.
        :param tolerance: The tolerance for the check. Defaults to 1e-10.
        :return: ``True`` if the state dump is equal to the given state within the given tolerance, ignoring global phase.
        :rtype: bool
        """
        phase = None
        # Convert a dense list of real amplitudes to a dictionary of state indices to complex amplitudes
        if isinstance(state, list):
            state = {i: val for i, val in enumerate(state)}
        # Filter out zero states from the state dump and the given state based on tolerance
        state = {k: v for k, v in state.items() if abs(v) > tolerance}
        inner_state = {k: v for k, v in self.__inner.items() if abs(v) > tolerance}
        if len(state) != len(inner_state):
            return False
        for key in state:
            if key not in inner_state:
                return False
            if phase is None:
                # Calculate the phase based on the first state pair encountered.
                # Every pair of states after this must have the same phase for the states to be equivalent.
                phase = inner_state[key] / state[key]
            elif abs(phase - inner_state[key] / state[key]) > tolerance:
                # This pair of states does not have the same phase,
                # within tolerance, so the equivalence check fails.
                return False
        return True

    def as_dense_state(self) -> List[complex]:
        """
        Returns the state dump as a dense list of complex amplitudes. This will include zero amplitudes.

        :return: A dense list of complex amplitudes, one per computational basis state.
        :rtype: List[complex]
        """
        return [self.__inner.get(i, complex(0)) for i in range(2**self.qubit_count)]


# ---------------------------------------------------------------------------
# Shot result
# ---------------------------------------------------------------------------


class ShotResult(TypedDict):
    """
    A single result of a shot.
    """

    events: List[Output | StateDump | str]
    result: Any
    messages: List[str]
    matrices: List[Output]
    dumps: List[StateDump]


# ---------------------------------------------------------------------------
# Interpreter configuration
# ---------------------------------------------------------------------------


class Config:
    """
    Configuration hints for the language service.
    """

    _config: Dict[str, Any]

    def __init__(
        self,
        target_profile: TargetProfile,
        language_features: Optional[List[str]],
        manifest: Optional[str],
        project_root: Optional[str],
    ):
        if target_profile == TargetProfile.Adaptive_RI:
            self._config = {"targetProfile": "adaptive_ri"}
        elif target_profile == TargetProfile.Adaptive_RIF:
            self._config = {"targetProfile": "adaptive_rif"}
        elif target_profile == TargetProfile.Adaptive_RIFLA:
            self._config = {"targetProfile": "adaptive_rifla"}
        elif target_profile == TargetProfile.Base:
            self._config = {"targetProfile": "base"}
        elif target_profile == TargetProfile.Unrestricted:
            self._config = {"targetProfile": "unrestricted"}

        if language_features is not None:
            self._config["languageFeatures"] = language_features
        if manifest is not None:
            self._config["manifest"] = manifest
        if project_root:
            # For now, we only support local project roots, so use a file schema in the URI.
            # In the future, we may support other schemes, such as github, if/when
            # we have VS Code Web + Jupyter support.
            self._config["projectRoot"] = Path(os.getcwd(), project_root).as_uri()

    def __repr__(self) -> str:
        return "Q# initialized with configuration: " + str(self._config)

    # See https://ipython.readthedocs.io/en/stable/config/integrating.html#rich-display
    # See https://ipython.org/ipython-doc/3/notebook/nbformat.html#display-data
    # This returns a custom MIME-type representation of the Q# configuration.
    # This data will be available in the cell output, but will not be displayed
    # to the user, as frontends would not know how to render the custom MIME type.
    # Editor services that interact with the notebook frontend
    # (i.e. the language service) can read and interpret the data.
    def _repr_mimebundle_(
        self, include: Union[Any, None] = None, exclude: Union[Any, None] = None
    ) -> Dict[str, Dict[str, Any]]:
        return {"application/x.qsharp-config": self._config}

    def get_target_profile(self) -> str:
        """
        Returns the target profile as a string, or "unspecified" if not set.
        """
        return self._config.get("targetProfile", "unspecified")


# ---------------------------------------------------------------------------
# QIR input data
# ---------------------------------------------------------------------------


# Class that wraps generated QIR, which can be used by
# azure-quantum as input data.
#
# This class must implement the QirRepresentable protocol
# that is defined by the azure-quantum package.
# See: https://github.com/microsoft/qdk-python/blob/fcd63c04aa871e49206703bbaa792329ffed13c4/azure-quantum/azure/quantum/target/target.py#L21
class QirInputData:
    # The name of this variable is defined
    # by the protocol and must remain unchanged.
    _name: str

    def __init__(self, name: str, ll_str: str):
        self._name = name
        self._ll_str = ll_str

    # The name of this method is defined
    # by the protocol and must remain unchanged.
    def _repr_qir_(self, **kwargs) -> bytes:
        return self._ll_str.encode("utf-8")

    def __str__(self) -> str:
        return self._ll_str


# ---------------------------------------------------------------------------
# __all__
# ---------------------------------------------------------------------------

__all__ = [
    "PauliNoise",
    "DepolarizingNoise",
    "BitFlipNoise",
    "PhaseFlipNoise",
    "StateDump",
    "ShotResult",
    "Config",
    "QirInputData",
]
