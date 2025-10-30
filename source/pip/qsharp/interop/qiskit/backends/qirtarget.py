# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import logging
from typing import Union

from qiskit.circuit import (
    Barrier,
    Delay,
    Measure,
    Parameter,
    Reset,
    Store,
)
from qiskit.circuit.controlflow import (
    ControlFlowOp,
    ForLoopOp,
    IfElseOp,
    SwitchCaseOp,
    WhileLoopOp,
)
from qiskit.circuit.library.standard_gates import (
    CHGate,
    CCXGate,
    CXGate,
    CYGate,
    CZGate,
    CRXGate,
    CRYGate,
    CRZGate,
    RXGate,
    RXXGate,
    RYGate,
    RYYGate,
    RZGate,
    RZZGate,
    HGate,
    SGate,
    SdgGate,
    SXGate,
    SwapGate,
    TGate,
    TdgGate,
    XGate,
    YGate,
    ZGate,
    IGate,
)

from qiskit.transpiler.target import Target
from .... import TargetProfile

logger = logging.getLogger(__name__)


class QirTarget:
    """Factory for QIR-compatible Qiskit ``Target`` instances."""

    def __init__(
        self,
        num_qubits=None,
        target_profile=TargetProfile.Base,
        supports_barrier=False,
        supports_delay=False,
    ) -> None:
        logger.warning(
            "QirTarget should not be instantiated directly. Use the 'build_target' class method"
            + " instead. This will be enforced in a future release. You can replace"
            + " 'QirTarget(...)' with 'QirTarget.build_target(...)'."
        )
        self._target = self.build_target(
            num_qubits=num_qubits,
            target_profile=target_profile,
            supports_barrier=supports_barrier,
            supports_delay=supports_delay,
        )

    def __getattr__(self, item):
        """
        Delegate attribute access to the underlying _target object.

        This method is called when an attribute is not found in the current instance.
        It forwards the attribute lookup to the internal _target object, effectively
        making this class act as a proxy or wrapper around the target.

        Args:
            item (str): The name of the attribute being accessed.

        Returns:
            Any: The value of the requested attribute from the _target object.

        Raises:
            AttributeError: If the requested item is "_target" or if the attribute
                           does not exist on the _target object.

        Note:
            The special handling of "_target" prevents infinite recursion and
            maintains proper encapsulation of the internal target object.
        """
        if item == "_target":
            raise AttributeError(item)
        return getattr(self._target, item)

    def to_target(self) -> Target:
        """Return the underlying Qiskit Target instance."""
        return self._target

    @classmethod
    def build_target(
        cls,
        num_qubits: Union[int, None] = None,
        target_profile=TargetProfile.Base,
        supports_barrier=False,
        supports_delay=False,
    ) -> Target:
        """
        Create a Qiskit Target object with quantum gates and operations for QIR compilation.

        This class method creates a Target instance that defines the available quantum
        operations and gates that can be used when compiling Q#/OpenQASM code to QIR (Quantum
        Intermediate Representation) format.

        Args:
            num_qubits (Union[int, None], optional): The number of qubits for the target.
                If None, the target will support any number of qubits. Defaults to None.
            target_profile (TargetProfile, optional): The target profile that determines
                which control flow operations are supported. If not TargetProfile.Base,
                adds control flow operations like if_else, switch_case, and while_loop.
                Defaults to TargetProfile.Base.
            supports_barrier (bool, optional): Whether to include barrier operations
                in the target. Defaults to False.
            supports_delay (bool, optional): Whether to include delay operations
                in the target. Defaults to False.

        Returns:
            Target: A Qiskit Target object configured with quantum gates and operations
                including:
                - Basic single-qubit gates (X, Y, Z, H, S, T, SX, I)
                - Rotation gates (RX, RY, RZ) with parameters
                - Two-qubit gates (CX, CY, CZ, SWAP, controlled rotations)
                - Three-qubit gates (CCX)
                - Multi-qubit rotation gates (RXX, RYY, RZZ)
                - Measurement and reset operations
                - Control flow operations (when target_profile != Base)
                - Optional barrier and delay operations

        Note:
            The target includes reset operations even for base profile since the
            compiler can implement workarounds using decompositions.
        """

        target = Target(num_qubits=num_qubits)

        if target_profile != TargetProfile.Base:
            target.add_instruction(ControlFlowOp, name="control_flow")
            target.add_instruction(IfElseOp, name="if_else")
            target.add_instruction(SwitchCaseOp, name="switch_case")
            target.add_instruction(WhileLoopOp, name="while_loop")

            # We don't currently support break or continue statements in Q#,
            # so we don't include them yet.
            # target.add_instruction(BreakLoopOp, name="break")
            # target.add_instruction(ContinueLoopOp, name="continue")

        target.add_instruction(Store, name="store")

        if supports_barrier:
            target.add_instruction(Barrier, name="barrier")
        if supports_delay:
            target.add_instruction(Delay, name="delay")

        # For loops should be fully deterministic in Qiskit/QASM.
        target.add_instruction(ForLoopOp, name="for_loop")
        target.add_instruction(Measure, name="measure")

        # While reset is technically not supported in base profile, the
        # compiler can use decompositions to implement workarounds.
        target.add_instruction(Reset, name="reset")

        target.add_instruction(CCXGate, name="ccx")
        target.add_instruction(CXGate, name="cx")
        target.add_instruction(CYGate, name="cy")
        target.add_instruction(CZGate, name="cz")

        target.add_instruction(RXGate(Parameter("theta")), name="rx")
        target.add_instruction(RXXGate(Parameter("theta")), name="rxx")
        target.add_instruction(CRXGate(Parameter("theta")), name="crx")

        target.add_instruction(RYGate(Parameter("theta")), name="ry")
        target.add_instruction(RYYGate(Parameter("theta")), name="ryy")
        target.add_instruction(CRYGate(Parameter("theta")), name="cry")

        target.add_instruction(RZGate(Parameter("theta")), name="rz")
        target.add_instruction(RZZGate(Parameter("theta")), name="rzz")
        target.add_instruction(CRZGate(Parameter("theta")), name="crz")

        target.add_instruction(HGate, name="h")

        target.add_instruction(SGate, name="s")
        target.add_instruction(SdgGate, name="sdg")

        target.add_instruction(SXGate, name="sx")

        target.add_instruction(SwapGate, name="swap")

        target.add_instruction(TGate, name="t")
        target.add_instruction(TdgGate, name="tdg")

        target.add_instruction(XGate, name="x")
        target.add_instruction(YGate, name="y")
        target.add_instruction(ZGate, name="z")

        target.add_instruction(IGate, name="id")

        target.add_instruction(CHGate, name="ch")

        return target
