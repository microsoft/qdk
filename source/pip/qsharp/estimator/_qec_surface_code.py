# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
import math
from typing import Any, Dict, List, Union
from ._utils import extract_qubit_metric


class SurfaceCode:
    """
    Surface code quantum error correction implementation.

    This class implements the surface code QEC scheme, which is a widely-studied
    topological quantum error correction code. It provides methods to calculate
    physical resource requirements, logical error rates, and timing parameters
    based on code distance and physical qubit characteristics.
    """

    def __init__(
        self,
        *,
        crossing_prefactor=0.03,
        error_correction_threshold=0.01,
        one_qubit_gate_time: Union[str, List[str]] = "one_qubit_gate_time",
        measurement_time: Union[str, List[str]] = "measurement_time",
        two_qubit_gate_time: Union[str, List[str]] = "two_qubit_gate_time",
        two_qubit_gate_error_rate: Union[str, List[str]] = "two_qubit_gate_error_rate",
        physical_qubits_formula: str = "2*distance**2",
        logical_cycle_time_formula: str = "(one_qubit_gate_tine + measurement_time + 4 * two_qubit_gate_time) * distance",
        max_distance: int = 149,
    ):
        """
        Initialize the surface code QEC scheme.

        :param crossing_prefactor: Prefactor used in logical error rate calculation
            (default: 0.03)
        :param error_correction_threshold: Error correction threshold below which
            the code can effectively correct errors (default: 0.01)
        :param one_qubit_gate_time: Key name (or list of key names) for extracting
            single-qubit gate time from qubit metrics. If a list is provided, the
            sum of all corresponding times is used (default: "one_qubit_gate_time")
        :param measurement_time: Key name (or list of key names) for extracting
            measurement time from qubit metrics. If a list is provided, the sum
            of all corresponding times is used (default: "measurement_time")
        :param two_qubit_gate_time: Key name (or list of key names) for extracting
            two-qubit gate time from qubit metrics. If a list is provided, the sum
            of all corresponding times is used (default: "two_qubit_gate_time")
        :param two_qubit_gate_error_rate: Key name (or list of key names) for
            extracting two-qubit gate error rate from qubit metrics. If a list is
            provided, the maximum of all corresponding error rates is used
            (default: "two_qubit_gate_error_rate")
        :param physical_qubits_formula: Mathematical formula to calculate the
            number of physical qubits as a function of distance. The formula is
            evaluated with 'distance' and 'math' module available (default:
            "2*distance**2")
        :param logical_cycle_time_formula: Mathematical formula to calculate the
            logical cycle time as a function of distance and gate times (default:
            "(one_qubit_gate_time + measurement_time + 4 * two_qubit_gate_time) * distance")
        :param max_distance: Maximum code distance to consider (default: 149)
        """
        # Logical error rate coefficients
        self._crossing_prefactor = crossing_prefactor
        self._error_correction_threshold = error_correction_threshold

        # Keys to extract physical qubit metrics
        self._one_qubit_gate_time = one_qubit_gate_time
        self._measurement_time = measurement_time
        self._two_qubit_gate_time = two_qubit_gate_time
        self._two_qubit_gate_error_rate = two_qubit_gate_error_rate

        self._physical_qubits_formula = physical_qubits_formula
        self._logical_cycle_time_formula = logical_cycle_time_formula
        self._max_distance = max_distance

    def physical_qubits(self, distance: int):
        """
        Calculate the number of physical qubits required for a given code distance.

        :param distance: The code distance
        :return: Number of physical qubits required
        """
        safe_context = {
            "distance": distance,
            "math": math,
            "__builtins__": {  # Prevent access to built-in functions
                "abs": abs,
                "min": min,
                "max": max,
                "pow": pow,
                "round": round,
            },
        }

        return eval(self._physical_qubits_formula, safe_context)

    def logical_qubits(self, distance: int):
        """
        Calculate the number of logical qubits encoded by this code.

        For surface codes, this is always 1 logical qubit per code block.

        :param distance: The code distance (unused but kept for interface consistency)
        :return: Number of logical qubits (always 1)
        """
        return 1

    def logical_cycle_time(self, qubit, distance):
        """
        Calculate the time required for one logical cycle.

        A logical cycle includes the time for syndrome extraction and correction,
        which depends on the physical gate times and the code distance.

        :param qubit: Dictionary containing physical qubit characteristics
        :param distance: The code distance
        :return: Logical cycle time in the same units as the physical gate times
        """
        one_qubit_gate_time = extract_qubit_metric(qubit, self._one_qubit_gate_time)
        measurement_time = extract_qubit_metric(qubit, self._measurement_time)
        two_qubit_gate_time = extract_qubit_metric(qubit, self._two_qubit_gate_time)

        safe_context = {
            "one_qubit_gate_time": one_qubit_gate_time,
            "measurement_time": measurement_time,
            "two_qubit_gate_time": two_qubit_gate_time,
            "distance": distance,
            "math": math,
            "__builtins__": {  # Prevent access to built-in functions
                "abs": abs,
                "min": min,
                "max": max,
                "pow": pow,
                "round": round,
            },
        }

        return eval(self._logical_cycle_time_formula, safe_context)

    def logical_error_rate(self, qubit: Dict[str, Any], distance: int):
        """
        Calculate the logical error rate for a given code distance.

        The logical error rate is calculated using an exponential suppression
        formula based on the ratio of physical error rate to the error correction
        threshold.

        :param qubit: Dictionary containing physical qubit characteristics
        :param distance: The code distance
        :return: Logical error rate per cycle
        """
        physical_error_rate = extract_qubit_metric(
            qubit, self._two_qubit_gate_error_rate, combine=max
        )

        return self._crossing_prefactor * (
            (physical_error_rate / self._error_correction_threshold)
            ** ((distance + 1) // 2)
        )

    def code_parameter_range(self):
        """
        Get the range of valid code distances for this surface code.

        Returns odd integers from 3 to max_distance (inclusive), as surface
        codes require odd distances.

        :return: List of valid code distances
        """
        return list(range(3, self._max_distance + 1, 2))

    def code_parameter_cmp(self, qubit: Dict[str, Any], p1: int, p2: int):
        """
        Compare two code parameters (distances).

        :param qubit: Dictionary containing physical qubit characteristics (unused)
        :param p1: First code parameter to compare
        :param p2: Second code parameter to compare
        :return: 1 if p1 > p2, -1 if p1 < p2, 0 if p1 == p2
        """
        return (p1 > p2) - (p1 < p2)
