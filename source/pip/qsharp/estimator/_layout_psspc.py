# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
import math
from typing import Union, List, Optional

from ._estimator import LogicalCounts


NUM_MEASUREMENTS_PER_R = 1
NUM_MEASUREMENTS_PER_TOF = 3


class PSSPCEstimator:
    """
    Computes post-layout logical resources based on the Parallel Synthesis
    Sequential Pauli Computation (PSSPC) layout method.
    """

    def __init__(
        self,
        source: Union[List[str], str, LogicalCounts],
        expression: Optional[str] = None,
    ):
        """
        Constructor for PSSPC layout method.  The source can be a list of Q#
        source files (use a list even if there is only one file), a path to a Q#
        project (directory with a qsharp.json file), or a LogicalCounts object.

        :param source: The Q# source files, project path, or LogicalCounts
            object.
        :param expression: An entry point expression that must only be used when
            the source is a list of Q# files or a project path.
        """

        if isinstance(source, LogicalCounts):
            if expression is not None:
                raise ValueError(
                    "Cannot specify entry point expression when source is LogicalCounts"
                )
            self._counts = source
        else:
            if expression is None:
                raise ValueError(
                    "Must specify entry point expression when source is not LogicalCounts"
                )
            self._counts = self._compute_counts(source, expression)

    def logical_qubits(self):
        """
        Calculates the number of logical qubits required for the PSSPC layout
        according to Eq. (D1) in [arXiv:2211.07629](https://arxiv.org/pdf/2211.07629)
        """

        num_qubits = self._counts["numQubits"]

        qubit_padding = math.ceil(math.sqrt(8 * num_qubits)) + 1
        return 2 * num_qubits + qubit_padding

    def logical_depth(self, budget):
        """
        Calculates the number of multi-qubit Pauli measurements executed in
        sequence according to Eq. (D3) in
        [arXiv:2211.07629](https://arxiv.org/pdf/2211.07629)
        """

        budget_rotations = budget["rotations"]
        tof_count = self._counts.get("cczCount", 0) + self._counts.get("ccixCount", 0)
        num_ts_per_rotation = self._num_ts_per_rotation(budget_rotations)

        return (
            (
                self._counts.get("measurementCount", 0)
                + self._counts.get("rotationCount", 0)
                + self._counts.get("tCount", 0)
            )
            * NUM_MEASUREMENTS_PER_R
            + tof_count * NUM_MEASUREMENTS_PER_TOF
            + (
                num_ts_per_rotation
                * self._counts.get("rotationDepth", 0)
                * NUM_MEASUREMENTS_PER_R
            )
        )

    def num_magic_states(self, budget, index):
        """
        Calculates the number of T magic states that are consumbed by
        multi-qubit Pauli measurements executed by PSSPC according to Eq. (D4)
        in [arXiv:2211.07629](https://arxiv.org/pdf/2211.07629)
        """

        # Only works for one kind of magic states, which is assumed to be T
        # magic states
        assert index == 0

        budget_rotations = budget["rotations"]
        tof_count = self._counts.get("cczCount", 0) + self._counts.get("ccixCount", 0)
        num_ts_per_rotation = self._num_ts_per_rotation(budget_rotations)

        return (
            4 * tof_count
            + self._counts.get("tCount", 0)
            + num_ts_per_rotation * self._counts.get("rotationCount", 0)
        )

    def algorithm_overhead(self, budget):
        """
        Returns the pre-layout logical resources as algorithm overhead, which
        can be accessed from the estimation result.
        """
        return self._counts

    def prune_error_budget(self, budget, strategy):
        if self._counts.get("rotationCount", 0) == 0:
            budget_rotations = budget.get("rotations", 0)
            budget["rotations"] = 0
            budget["logical"] += budget_rotations / 2
            budget["magic_states"] += budget_rotations / 2

    def _compute_counts(self, source: Union[List[str], str], expression):
        # NOTE: Importing qsharp here to avoid circular dependency
        import qsharp

        if isinstance(source, list):
            qsharp.init()
            for file in source:
                qsharp.eval(qsharp._fs.read_file(file)[1])
        elif isinstance(source, str):
            qsharp.init(project_root=source)
        else:
            raise ValueError("Invalid source type for PSSPCEstimator")

        return qsharp.logical_counts(expression)

    def _num_ts_per_rotation(self, rotation_budget):
        rotation_count = self._counts.get("rotationCount", 0)

        if rotation_count > 0:
            return math.ceil(0.53 * math.log2(rotation_count / rotation_budget) + 4.86)

        else:
            return 0
