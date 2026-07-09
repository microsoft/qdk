# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pyqir import QirModuleVisitor, is_entry_point, Opcode


class ValidateAllowedIntrinsics(QirModuleVisitor):
    """
    Ensure that the module only contains allowed intrinsics.
    """

    def _on_function(self, function):
        name = function.name
        if (
            not is_entry_point(function)
            and not name.endswith("_record_output")
            and name
            not in [
                "__quantum__rt__begin_parallel",
                "__quantum__rt__end_parallel",
                "__quantum__qis__read_result__body",
                "__quantum__rt__read_result",
                "__quantum__qis__move__body",
                "__quantum__qis__cz__body",
                "__quantum__qis__sx__body",
                "__quantum__qis__rz__body",
                "__quantum__qis__mresetz__body",
            ]
        ):
            raise ValueError(f"{name} is not a supported intrinsic")


class ValidateNoConditionalBranches(QirModuleVisitor):
    """
    Ensure that the function(s) only use unconditional branches.
    """

    def _on_block(self, block):
        if (
            block.terminator
            and block.terminator.opcode == Opcode.BR
            and len(block.terminator.operands) > 1
        ):
            raise ValueError("programs with branching control flow are not supported")
        super()._on_block(block)
