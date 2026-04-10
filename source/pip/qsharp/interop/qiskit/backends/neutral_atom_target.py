# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import logging

from qiskit.circuit import Measure, Parameter, Reset
from qiskit.circuit.library.standard_gates import CZGate, RZGate, SXGate
from qiskit.transpiler.target import Target

logger = logging.getLogger(__name__)


class NeutralAtomTarget:
    """Factory for a Qiskit ``Target`` restricted to the NeutralAtomDevice native gate set.

    The native gate set is ``{rz, sx, cz, measure}`` — the only gates that survive
    ``NeutralAtomDevice.compile()``'s decomposition pipeline. Using this target ensures
    that Qiskit's transpiler decomposes all non-native gates (H, CX, X, etc.) into
    native gates *before* QASM3 export, so the noise model fields that matter
    (``noise.rz``, ``noise.sx``, ``noise.cz``, ``noise.mresetz``) align with the
    gates actually present during simulation.
    """

    @classmethod
    def build_target(
        cls,
        num_qubits: int | None = None,
    ) -> Target:
        """Return a Qiskit ``Target`` with only the NeutralAtomDevice native gates.

        :param num_qubits: Number of qubits. ``None`` means no limit (simulator).
        :return: A ``Target`` containing ``{rz, sx, cz, measure, reset}``.
        :rtype: Target
        """
        target = Target(num_qubits=num_qubits)

        target.add_instruction(RZGate(Parameter("theta")), name="rz")
        target.add_instruction(SXGate, name="sx")
        target.add_instruction(CZGate, name="cz")
        target.add_instruction(Measure, name="measure")
        # Reset is used internally by NeutralAtomDevice (MResetZ), so include it
        # so the transpiler can express mid-circuit resets.
        target.add_instruction(Reset, name="reset")

        return target
