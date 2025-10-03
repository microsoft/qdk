# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import logging

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


class QirTarget(Target):
    def __init__(
        self,
        num_qubits=None,
        target_profile=TargetProfile.Base,
        supports_barrier=False,
        supports_delay=False,
    ):
        super().__init__(num_qubits=num_qubits)

        self._num_qubits = num_qubits

        if target_profile != TargetProfile.Base:
            self.add_instruction(ControlFlowOp, name="control_flow")
            self.add_instruction(IfElseOp, name="if_else")
            self.add_instruction(SwitchCaseOp, name="switch_case")
            self.add_instruction(WhileLoopOp, name="while_loop")

            # We don't currently support break or continue statements
            # in Q#, so we don't include them yet.
            # self.add_instruction(BreakLoopOp, name="break")
            # self.add_instruction(ContinueLoopOp, name="continue")

        self.add_instruction(Store, name="store")

        if supports_barrier:
            self.add_instruction(Barrier, name="barrier")
        if supports_delay:
            self.add_instruction(Delay, name="delay")

        # For loops should be fully deterministic in Qiskit/QASM
        self.add_instruction(ForLoopOp, name="for_loop")
        self.add_instruction(Measure, name="measure")

        # While reset is technically not supported in base profile,
        # the compiler can use decompositions to implement workarounds
        self.add_instruction(Reset, name="reset")

        self.add_instruction(CCXGate, name="ccx")
        self.add_instruction(CXGate, name="cx")
        self.add_instruction(CYGate, name="cy")
        self.add_instruction(CZGate, name="cz")

        self.add_instruction(RXGate(Parameter("theta")), name="rx")
        self.add_instruction(RXXGate(Parameter("theta")), name="rxx")
        self.add_instruction(CRXGate(Parameter("theta")), name="crx")

        self.add_instruction(RYGate(Parameter("theta")), name="ry")
        self.add_instruction(RYYGate(Parameter("theta")), name="ryy")
        self.add_instruction(CRYGate(Parameter("theta")), name="cry")

        self.add_instruction(RZGate(Parameter("theta")), name="rz")
        self.add_instruction(RZZGate(Parameter("theta")), name="rzz")
        self.add_instruction(CRZGate(Parameter("theta")), name="crz")

        self.add_instruction(HGate, name="h")

        self.add_instruction(SGate, name="s")
        self.add_instruction(SdgGate, name="sdg")

        self.add_instruction(SXGate, name="sx")

        self.add_instruction(SwapGate, name="swap")

        self.add_instruction(TGate, name="t")
        self.add_instruction(TdgGate, name="tdg")

        self.add_instruction(XGate, name="x")
        self.add_instruction(YGate, name="y")
        self.add_instruction(ZGate, name="z")

        self.add_instruction(IGate, name="id")

        self.add_instruction(CHGate, name="ch")

    # NOTE: The follow property intentionally shadows the property on the parent class to allow it to return `None`
    # when the value is not set, which allows bypassing transpilation checks for number of qubits. Without this,
    # versions of Qiskit 1.3.0 and higher default to `0` which will fail later checks.
    @property
    def num_qubits(self):
        return self._num_qubits

    # NOTE: The follow property setter intentionally shadows the property on the parent class to allow it to be set, which
    # maintains compatibility with Qiskit versions before 1.3.0 where the property was settable.
    @num_qubits.setter
    def num_qubits(self, value):
        self._num_qubits = value
