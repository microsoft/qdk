# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
from typing import Any, Dict
from ._utils import extract_qubit_metric


class RoundBasedFactory:
    """
    Factory for generating magic states using round-based distillation protocols.

    This class implements magic state distillation using round-based protocols.
    It generates distillation units that can operate at both physical and logical levels.
    """

    def __init__(
        self,
        *,
        with_physical: bool = True,
        gate_time: str = "gate_time",
        gate_error: str = "gate_error",
        clifford_error: str = "clifford_error",
        use_max_qubits_per_round: bool = False,
        max_rounds: int = 3,
        max_extra_rounds: int = 5
    ):
        """
        Initialize the round-based magic state factory.

        :param with_physical: Whether to include physical-level distillation units
            (default: True)
        :param gate_time: Key name (or list of key names) for extracting gate time
            from qubit metrics. If a list is provided, the sum of all corresponding
            times is used (default: "gate_time")
        :param gate_error: Key name (or list of key names) for extracting gate
            error rate from qubit metrics. If a list is provided, the maximum of
            all corresponding error rates is used (default: "gate_error")
        :param clifford_error: Key name (or list of key names) for extracting
            Clifford gate error rate from qubit metrics. If a list is provided,
            the maximum of all corresponding error rates is used
            (default: "clifford_error")
        :param use_max_qubits_per_round: Whether to maximize qubits used per round
            (default: False)
        :param max_rounds: Maximum number of distillation rounds (default: 3)
        :param max_extra_rounds: Maximum number of additional rounds beyond
            max_rounds (default: 5)
        """
        self.with_physical = with_physical
        self.gate_time = gate_time
        self.gate_error = gate_error
        self.clifford_error = clifford_error
        self.use_max_qubits_per_round = use_max_qubits_per_round
        self.max_rounds = max_rounds
        self.max_extra_rounds = max_extra_rounds

    def distillation_units(
        self, code: Any, qubit: Dict[str, Any], max_code_parameter: int
    ):
        """
        Generate a list of distillation units for magic state production.

        Creates distillation units using 15-to-1 protocols (RM prep and space
        efficient variants) at both physical level (if enabled) and across
        all valid code parameters up to the maximum.

        :param code: QEC code object that provides code parameters and metrics
        :param qubit: Dictionary containing physical qubit characteristics
        :param max_code_parameter: Maximum code parameter (distance) to consider
        :return: List of distillation unit dictionaries, each containing
            configuration and callable functions for resource calculations
        """
        units = []

        gate_time = extract_qubit_metric(qubit, self.gate_time)
        clifford_error = extract_qubit_metric(qubit, self.clifford_error)

        if self.with_physical:
            units.append(
                _create_unit(
                    "15-to-1 RM prep",
                    1,
                    24,
                    gate_time,
                    1,
                    31,
                    clifford_error,
                )
            )
            units.append(
                _create_unit(
                    "15-to-1 space efficient",
                    1,
                    45,
                    gate_time,
                    1,
                    12,
                    clifford_error,
                )
            )

        for code_parameter in code.code_parameter_range():
            if code.code_parameter_cmp(qubit, code_parameter, max_code_parameter) == 1:
                break

            units.append(
                _create_unit(
                    "15-to-1 RM prep",
                    code_parameter,
                    11,
                    code.logical_cycle_time(qubit, code_parameter),
                    code.physical_qubits(code_parameter),
                    31,
                    code.logical_error_rate(qubit, code_parameter),
                )
            )
            units.append(
                _create_unit(
                    "15-to-1 space efficient",
                    code_parameter,
                    13,
                    code.logical_cycle_time(qubit, code_parameter),
                    code.physical_qubits(code_parameter),
                    20,
                    code.logical_error_rate(qubit, code_parameter),
                )
            )

        return units

    def trivial_distillation_unit(
        self, code: Any, qubit: Dict[str, Any], code_parameter: Any
    ):
        """
        Creates this 1-to-1 distillation unit in the case where the target error
        rate is already met by the physical qubit.

        :param code: QEC code object that provides code parameters and metrics
        :param qubit: Dictionary containing physical qubit characteristics
        :param code_parameter: Code parameter chosen to run the algorithm
        """

        return {
            "name": "trivial 1-to-1",
            "code_parameter": code_parameter,
            "num_input_states": 1,
            "num_output_states": 1,
            "physical_qubits": lambda _: code.physical_qubits(code_parameter),
            "duration": lambda _: code.logical_cycle_time(qubit, code_parameter),
            "output_error_rate": lambda input_error_rate: input_error_rate,
            "failure_probability": lambda _: 0.0,
        }


def _create_unit(
    name: str,
    code_parameter: Any,
    num_cycles: int,
    cycle_time: int,
    physical_qubits_factor: int,
    physical_qubits: int,
    clifford_error_rate: float,
):
    """
    Create a distillation unit configuration dictionary.

    :param name: Name of the distillation protocol
    :param code_parameter: Code parameter (distance) for this unit
    :param num_cycles: Number of cycles required for distillation
    :param cycle_time: Time per cycle
    :param physical_qubits_factor: Multiplier for physical qubit count
    :param physical_qubits: Base number of physical qubits
    :param clifford_error_rate: Error rate for Clifford operations
    :return: Dictionary containing unit configuration and callable functions
        for calculating physical qubits, duration, output error rate, and
        failure probability
    """

    return {
        "name": name,
        "code_parameter": code_parameter,
        "num_input_states": 15,
        "num_output_states": 1,
        "physical_qubits": lambda _: physical_qubits * physical_qubits_factor,
        "duration": lambda _: num_cycles * cycle_time,
        "output_error_rate": lambda input_error_rate: 35 * input_error_rate**3
        + 7.1 * clifford_error_rate,
        "failure_probability": lambda input_error_rate: 15 * input_error_rate
        + 356 * clifford_error_rate,
    }
