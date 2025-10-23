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

    @classmethod
    def create_target(
        cls,
        num_qubits: Union[int, None] = 0,
        target_profile=TargetProfile.Base,
        supports_barrier=False,
        supports_delay=False,
    ) -> Target:
        target = Target(num_qubits=num_qubits)

        # Preserve ``None`` for ``num_qubits`` to avoid downstream checks that
        # assume a concrete register size. Qiskit >= 1.3 defaults to ``0`` when
        # the attribute is unset, so we explicitly store the override when
        # callers request an unspecified qubit count.
        if num_qubits is None:
            try:
                target._num_qubits = None  # type: ignore[attr-defined]
            except AttributeError:
                pass

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
