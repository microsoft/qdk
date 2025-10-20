# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pyqir import QirModuleVisitor, is_entry_point


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


class ValidateSingleBlock(QirModuleVisitor):
    """
    Ensure that the entry function(s) only contains one block.
    """

    def _on_function(self, function):
        if len(function.basic_blocks) > 1:
            raise ValueError(
                f"Function {function.name} contains multiple blocks. Only one block is allowed."
            )
