# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pyqir import FloatConstant, QirModuleVisitor
from math import pi
from ._utils import TOLERANCE


class ValidateAllowedIntrinsics(QirModuleVisitor):
    """
    Ensure that the module only contains allowed intrinsics.
    """

    def _on_call_instr(self, call):
        if not call.callee.name.endswith("_record_output") and call.callee.name not in [
            "__quantum__rt__begin_parallel",
            "__quantum__rt__end_parallel",
            "__quantum__qis__read_result__body",
            "__quantum__qis__move__body",
            "__quantum__qis__cz__body",
            "__quantum__qis__sx__body",
            "__quantum__qis__rz__body",
            "__quantum__qis__mresetz__body",
        ]:
            raise ValueError(f"{call.callee.name} is not a supported intrinsic")


class ValidateBeginEndParallel(QirModuleVisitor):
    """
    Ensure that only one parallel section is active at a time and that they all begin and end in the same block.
    """

    def _on_block(self, block):
        self.parallel = False
        super()._on_block(block)
        if self.parallel:
            raise ValueError("Unmatched __quantum__rt__begin_parallel at end of block")

    def _on_call_instr(self, call):
        if call.callee.name == "__quantum__rt__begin_parallel":
            if self.parallel:
                raise ValueError(
                    "Nested __quantum__rt__begin_parallel in parallel section"
                )
            self.parallel = True
        elif call.callee.name == "__quantum__rt__end_parallel":
            if not self.parallel:
                raise ValueError("Unmatched __quantum__rt__end_parallel")
            self.parallel = False


class ValidateSingleBlock(QirModuleVisitor):
    """
    Ensure that the entry function(s) only contains one block.
    """

    def _on_function(self, function):
        if len(function.basic_blocks) > 1:
            raise ValueError(
                f"Function {function.name} contains multiple blocks. Only one block is allowed."
            )


class ValidateCliffordRzAngles(QirModuleVisitor):
    """
    Ensure that the module only contains Clifford rotation angles.
    """

    def _on_qis_rz(self, call, angle, target):
        if not isinstance(angle, FloatConstant):
            raise ValueError("Angle used in RZ must be a constant")
        angle = angle.value
        if not (
            abs(angle) < TOLERANCE
            or abs(abs(angle) - 2 * pi) < TOLERANCE
            or abs(abs(angle) - pi) < TOLERANCE
            or abs(abs(angle) - pi / 2) < TOLERANCE
        ):
            raise ValueError(
                f"Angle {angle} used in RZ is not a Clifford compatible rotation angle"
            )
