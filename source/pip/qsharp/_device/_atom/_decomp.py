# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pyqir import (
    FloatConstant,
    const,
    Function,
    FunctionType,
    Type,
    qubit_type,
    result_type,
    result,
    Context,
    Linkage,
    QirModuleVisitor,
    required_num_results,
)
from math import pi
from ._utils import TOLERANCE


class DecomposeMultiQubitToCZ(QirModuleVisitor):
    """
    Decomposes all multi-qubit gates to CZ gates and single qubit gates.
    """

    h_func: Function
    s_func: Function
    sadj_func: Function
    t_func: Function
    tadj_func: Function
    rz_func: Function
    cz_func: Function

    def _on_module(self, module):
        void = Type.void(module.context)
        qubit_ty = qubit_type(module.context)
        self.double_ty = Type.double(module.context)
        # Find or create all the needed functions.
        for func in module.functions:
            match func.name:
                case "__quantum__qis__h__body":
                    self.h_func = func
                case "__quantum__qis__s__body":
                    self.s_func = func
                case "__quantum__qis__s__adj":
                    self.sadj_func = func
                case "__quantum__qis__t__body":
                    self.t_func = func
                case "__quantum__qis__t__adj":
                    self.tadj_func = func
                case "__quantum__qis__rz__body":
                    self.rz_func = func
                case "__quantum__qis__cz__body":
                    self.cz_func = func
        if not hasattr(self, "h_func"):
            self.h_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__h__body",
                module,
            )
        if not hasattr(self, "s_func"):
            self.s_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__s__body",
                module,
            )
        if not hasattr(self, "sadj_func"):
            self.sadj_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__s__adj",
                module,
            )
        if not hasattr(self, "t_func"):
            self.t_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__t__body",
                module,
            )
        if not hasattr(self, "tadj_func"):
            self.tadj_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__t__adj",
                module,
            )
        if not hasattr(self, "rz_func"):
            self.rz_func = Function(
                FunctionType(void, [self.double_ty, qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__rz__body",
                module,
            )
        if not hasattr(self, "cz_func"):
            self.cz_func = Function(
                FunctionType(void, [qubit_ty, qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__cz__body",
                module,
            )
        super()._on_module(module)

    def _on_qis_ccx(self, call, ctrl1, ctrl2, target):
        self.builder.insert_before(call)
        self.builder.call(self.h_func, [target])
        self.builder.call(self.tadj_func, [ctrl1])
        self.builder.call(self.tadj_func, [ctrl2])
        self.builder.call(self.h_func, [ctrl1])
        self.builder.call(self.cz_func, [target, ctrl1])
        self.builder.call(self.h_func, [ctrl1])
        self.builder.call(self.t_func, [ctrl1])
        self.builder.call(self.h_func, [target])
        self.builder.call(self.cz_func, [ctrl2, target])
        self.builder.call(self.h_func, [target])
        self.builder.call(self.h_func, [ctrl1])
        self.builder.call(self.cz_func, [ctrl2, ctrl1])
        self.builder.call(self.h_func, [ctrl1])
        self.builder.call(self.t_func, [target])
        self.builder.call(self.tadj_func, [ctrl1])
        self.builder.call(self.h_func, [target])
        self.builder.call(self.cz_func, [ctrl2, target])
        self.builder.call(self.h_func, [target])
        self.builder.call(self.h_func, [ctrl1])
        self.builder.call(self.cz_func, [target, ctrl1])
        self.builder.call(self.h_func, [ctrl1])
        self.builder.call(self.tadj_func, [target])
        self.builder.call(self.t_func, [ctrl1])
        self.builder.call(self.h_func, [ctrl1])
        self.builder.call(self.cz_func, [ctrl2, ctrl1])
        self.builder.call(self.h_func, [ctrl1])
        self.builder.call(self.h_func, [target])
        call.erase()

    def _on_qis_cx(self, call, ctrl, target):
        self.builder.insert_before(call)
        self.builder.call(self.h_func, [target])
        self.builder.call(self.cz_func, [ctrl, target])
        self.builder.call(self.h_func, [target])
        call.erase()

    def _on_qis_cy(self, call, ctrl, target):
        self.builder.insert_before(call)
        self.builder.call(self.sadj_func, [target])
        self.builder.call(self.h_func, [target])
        self.builder.call(self.cz_func, [ctrl, target])
        self.builder.call(self.h_func, [target])
        self.builder.call(self.s_func, [target])
        call.erase()

    def _on_qis_rxx(self, call, angle, target1, target2):
        self.builder.insert_before(call)
        self.builder.call(self.h_func, [target2])
        self.builder.call(self.cz_func, [target2, target1])
        self.builder.call(self.h_func, [target1])
        self.builder.call(self.rz_func, [angle, target1])
        self.builder.call(self.h_func, [target1])
        self.builder.call(self.cz_func, [target2, target1])
        self.builder.call(self.h_func, [target2])
        call.erase()

    def _on_qis_ryy(self, call, angle, target1, target2):
        self.builder.insert_before(call)
        self.builder.call(self.sadj_func, [target1])
        self.builder.call(self.sadj_func, [target2])
        self.builder.call(self.h_func, [target2])
        self.builder.call(self.cz_func, [target2, target1])
        self.builder.call(self.h_func, [target1])
        self.builder.call(self.rz_func, [angle, target1])
        self.builder.call(self.h_func, [target1])
        self.builder.call(self.cz_func, [target2, target1])
        self.builder.call(self.h_func, [target2])
        self.builder.call(self.s_func, [target2])
        self.builder.call(self.s_func, [target1])
        call.erase()

    def _on_qis_rzz(self, call, angle, target1, target2):
        self.builder.insert_before(call)
        self.builder.call(self.h_func, [target1])
        self.builder.call(self.cz_func, [target2, target1])
        self.builder.call(self.h_func, [target1])
        self.builder.call(self.rz_func, [angle, target1])
        self.builder.call(self.h_func, [target1])
        self.builder.call(self.cz_func, [target2, target1])
        self.builder.call(self.h_func, [target1])
        call.erase()

    def _on_qis_swap(self, call, target1, target2):
        self.builder.insert_before(call)
        self.builder.call(self.h_func, [target2])
        self.builder.call(self.cz_func, [target1, target2])
        self.builder.call(self.h_func, [target2])
        self.builder.call(self.h_func, [target1])
        self.builder.call(self.cz_func, [target2, target1])
        self.builder.call(self.h_func, [target1])
        self.builder.call(self.h_func, [target2])
        self.builder.call(self.cz_func, [target1, target2])
        self.builder.call(self.h_func, [target2])
        call.erase()


class DecomposeSingleRotationToRz(QirModuleVisitor):
    """
    Decomposes all single qubit rotations to Rz gates.
    """

    h_func: Function
    s_func: Function
    sadj_func: Function
    rz_func: Function

    def _on_module(self, module):
        void = Type.void(module.context)
        qubit_ty = qubit_type(module.context)
        self.double_ty = Type.double(module.context)
        # Find or create all the needed functions.
        for func in module.functions:
            match func.name:
                case "__quantum__qis__h__body":
                    self.h_func = func
                case "__quantum__qis__s__body":
                    self.s_func = func
                case "__quantum__qis__s__adj":
                    self.sadj_func = func
                case "__quantum__qis__rz__body":
                    self.rz_func = func
        if not hasattr(self, "h_func"):
            self.h_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__h__body",
                module,
            )
        if not hasattr(self, "s_func"):
            self.s_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__s__body",
                module,
            )
        if not hasattr(self, "sadj_func"):
            self.sadj_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__s__adj",
                module,
            )
        if not hasattr(self, "rz_func"):
            self.rz_func = Function(
                FunctionType(void, [self.double_ty, qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__rz__body",
                module,
            )
        super()._on_module(module)

    def _on_qis_rx(self, call, angle, target):
        self.builder.insert_before(call)
        self.builder.call(self.h_func, [target])
        self.builder.call(
            self.rz_func,
            [angle, target],
        )
        self.builder.call(self.h_func, [target])
        call.erase()

    def _on_qis_ry(self, call, angle, target):
        self.builder.insert_before(call)
        self.builder.call(self.sadj_func, [target])
        self.builder.call(self.h_func, [target])
        self.builder.call(
            self.rz_func,
            [angle, target],
        )
        self.builder.call(self.h_func, [target])
        self.builder.call(self.s_func, [target])
        call.erase()


class DecomposeSingleQubitToRzSX(QirModuleVisitor):
    """
    Decomposes all single qubit gates to Rz and Sx gates.
    """

    sx_func: Function
    rz_func: Function

    def _on_module(self, module):
        void = Type.void(module.context)
        qubit_ty = qubit_type(module.context)
        self.double_ty = Type.double(module.context)
        # Find or create all the needed functions.
        for func in module.functions:
            match func.name:
                case "__quantum__qis__sx__body":
                    self.sx_func = func
                case "__quantum__qis__rz__body":
                    self.rz_func = func
        if not hasattr(self, "sx_func"):
            self.sx_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__sx__body",
                module,
            )
        if not hasattr(self, "rz_func"):
            self.rz_func = Function(
                FunctionType(void, [self.double_ty, qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__rz__body",
                module,
            )
        super()._on_module(module)

    def _on_qis_h(self, call, target):
        self.builder.insert_before(call)
        self.builder.call(
            self.rz_func,
            [const(self.double_ty, pi / 2), target],
        )
        self.builder.call(self.sx_func, [target])
        self.builder.call(
            self.rz_func,
            [const(self.double_ty, pi / 2), target],
        )
        call.erase()

    def _on_qis_s(self, call, target):
        self.builder.insert_before(call)
        self.builder.call(
            self.rz_func,
            [const(self.double_ty, pi / 2), target],
        )
        call.erase()

    def _on_qis_s_adj(self, call, target):
        self.builder.insert_before(call)
        self.builder.call(
            self.rz_func,
            [const(self.double_ty, -pi / 2), target],
        )
        call.erase()

    def _on_qis_t(self, call, target):
        self.builder.insert_before(call)
        self.builder.call(
            self.rz_func,
            [const(self.double_ty, pi / 4), target],
        )
        call.erase()

    def _on_qis_t_adj(self, call, target):
        self.builder.insert_before(call)
        self.builder.call(
            self.rz_func,
            [const(self.double_ty, -pi / 4), target],
        )
        call.erase()

    def _on_qis_x(self, call, target):
        self.builder.insert_before(call)
        self.builder.call(self.sx_func, [target])
        self.builder.call(self.sx_func, [target])
        call.erase()

    def _on_qis_y(self, call, target):
        self.builder.insert_before(call)
        self.builder.call(self.sx_func, [target])
        self.builder.call(self.sx_func, [target])
        self.builder.call(
            self.rz_func,
            [const(self.double_ty, pi), target],
        )
        call.erase()

    def _on_qis_z(self, call, target):
        self.builder.insert_before(call)
        self.builder.call(
            self.rz_func,
            [const(self.double_ty, pi), target],
        )
        call.erase()


class DecomposeRzAnglesToCliffordGates(QirModuleVisitor):
    """
    Ensure that the module only contains Clifford gates instead of rotation angles.
    """

    THREE_PI_OVER_2 = 3 * pi / 2
    PI_OVER_2 = pi / 2
    TWO_PI = 2 * pi

    z_func: Function
    s_func: Function
    sadj_func: Function

    def _on_module(self, module):
        void = Type.void(module.context)
        qubit_ty = qubit_type(module.context)
        self.double_ty = Type.double(module.context)
        # Find or create all the needed functions.
        for func in module.functions:
            match func.name:
                case "__quantum__qis__s__body":
                    self.s_func = func
                case "__quantum__qis__s__adj":
                    self.sadj_func = func
                case "__quantum__qis__z__body":
                    self.z_func = func

        if not hasattr(self, "s_func"):
            self.s_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__s__body",
                module,
            )
        if not hasattr(self, "sadj_func"):
            self.sadj_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__s__adj",
                module,
            )
        if not hasattr(self, "z_func"):
            self.z_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__z__body",
                module,
            )

        super()._on_module(module)

    def _on_qis_rz(self, call, angle, target):
        if not isinstance(angle, FloatConstant):
            raise ValueError("Angle used in RZ must be a constant")
        angle = angle.value

        self.builder.insert_before(call)

        if (
            abs(angle - self.THREE_PI_OVER_2) < TOLERANCE
            or abs(angle + self.PI_OVER_2) < TOLERANCE
        ):
            self.builder.call(self.sadj_func, [target])
        elif abs(angle - pi) < TOLERANCE or abs(angle + pi) < TOLERANCE:
            self.builder.call(self.z_func, [target])
        elif (
            abs(angle - self.PI_OVER_2) < TOLERANCE
            or abs(angle + self.THREE_PI_OVER_2) < TOLERANCE
        ):
            self.builder.call(self.s_func, [target])
        elif (
            angle < TOLERANCE
            or abs(angle - self.TWO_PI) < TOLERANCE
            or abs(angle + self.TWO_PI) < TOLERANCE
        ):
            # I, drop it
            pass
        else:
            raise ValueError(
                f"Angle {angle} used in RZ is not a Clifford compatible rotation angle"
            )

        call.erase()


class ReplaceResetWithMResetZ(QirModuleVisitor):
    """
    Replaces all reset operations with a call to mresetz using a new, ignored result identifier.
    """

    context: Context
    mresetz_func: Function
    next_result_id: int

    def _on_module(self, module):
        self.context = module.context
        void = Type.void(self.context)
        qubit_ty = qubit_type(self.context)
        result_ty = result_type(self.context)
        # Find or create the intrinsic mresetz function
        for func in module.functions:
            match func.name:
                case "__quantum__qis__mresetz__body":
                    self.mresetz_func = func
        if not hasattr(self, "mresetz_func"):
            self.mresetz_func = Function(
                FunctionType(void, [qubit_ty, result_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__mresetz__body",
                module,
            )
        super()._on_module(module)

    def _on_function(self, function):
        self.next_result_id = required_num_results(function) or 0
        super()._on_function(function)

    def _on_qis_reset(self, call, target):
        self.builder.insert_before(call)
        # Create a new result identifier to ignore the measurement result
        result_id = result(self.context, self.next_result_id)
        self.next_result_id += 1
        self.builder.call(self.mresetz_func, [target, result_id])
        call.erase()
