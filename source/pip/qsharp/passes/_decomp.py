# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pyqir import (
    const,
    Function,
    FunctionType,
    Type,
    qubit_type,
    Linkage,
    QirModuleVisitor,
)
from math import pi


class DecomposeMultiQubitToCZ(QirModuleVisitor):
    """
    Decomposes all multi-qubit gates to CZ gates and single qubit gates.
    """

    def _on_module(self, module):
        void = Type.void(module.context)
        qubit_ty = qubit_type(module.context)
        self.double_ty = Type.double(module.context)
        # Find or create all the needed functions.
        self.h_func = None
        self.s_func = None
        self.sadj_func = None
        self.t_func = None
        self.tadj_func = None
        self.rz_func = None
        self.cz_func = None
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
        if not self.h_func:
            self.h_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__h__body",
                module,
            )
        if not self.s_func:
            self.s_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__s__body",
                module,
            )
        if not self.sadj_func:
            self.sadj_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__s__adj",
                module,
            )
        if not self.t_func:
            self.t_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__t__body",
                module,
            )
        if not self.tadj_func:
            self.tadj_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__t__adj",
                module,
            )
        if not self.rz_func:
            self.rz_func = Function(
                FunctionType(void, [self.double_ty, qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__rz__body",
                module,
            )
        if not self.cz_func:
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

    def _on_module(self, module):
        void = Type.void(module.context)
        qubit_ty = qubit_type(module.context)
        self.double_ty = Type.double(module.context)
        # Find or create all the needed functions.
        self.h_func = None
        self.s_func = None
        self.sadj_func = None
        self.rz_func = None
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
        if not self.h_func:
            self.h_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__h__body",
                module,
            )
        if not self.s_func:
            self.s_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__s__body",
                module,
            )
        if not self.sadj_func:
            self.sadj_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__s__adj",
                module,
            )
        if not self.rz_func:
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

    def _on_module(self, module):
        void = Type.void(module.context)
        qubit_ty = qubit_type(module.context)
        self.double_ty = Type.double(module.context)
        # Find or create all the needed functions.
        self.sx_func = None
        self.rz_func = None
        for func in module.functions:
            match func.name:
                case "__quantum__qis__sx__body":
                    self.sx_func = func
                case "__quantum__qis__rz__body":
                    self.rz_func = func
        if not self.sx_func:
            self.sx_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__sx__body",
                module,
            )
        if not self.rz_func:
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
