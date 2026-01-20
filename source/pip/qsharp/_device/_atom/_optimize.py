# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pyqir import (
    Type,
    Function,
    FunctionType,
    FloatConstant,
    Linkage,
    const,
    qubit_type,
    qubit_id,
    result_type,
    is_entry_point,
    QirModuleVisitor,
)
from math import pi

from ._utils import TOLERANCE


class OptimizeSingleQubitGates(QirModuleVisitor):
    """
    Optimizes single qubit gates by looking for sequences of a gate and its adjoint on a given qubit.
    Will also try to replace certain patterns with simpler gates.
    """

    sx_func: Function
    mresetz_func: Function

    def _on_module(self, module):
        void = Type.void(module.context)
        qubit_ty = qubit_type(module.context)
        result_ty = result_type(module.context)
        self.double_ty = Type.double(module.context)
        self.used_qubits = set()
        # Find or create the intrinsic gate functions
        for func in module.functions:
            match func.name:
                case "__quantum__qis__mresetz__body":
                    self.mresetz_func = func
                case "__quantum__qis__sx__body":
                    self.sx_func = func
        if not hasattr(self, "sx_fun"):
            self.sx_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__sx__body",
                module,
            )
        if not hasattr(self, "mresetz_func"):
            self.mresetz_func = Function(
                FunctionType(void, [qubit_ty, result_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__mresetz__body",
                module,
            )
        super()._on_module(module)

    def _drop_ops(self, qubits):
        # Since instructions are only removed when they are canceled out by their adjoint or folded with another
        # instruction, we can just pop the entries for these qubits so they start fresh with the next gates.
        for qubit in qubits:
            q = qubit_id(qubit)
            self.qubit_ops.pop(q, None)
            self.last_meas.pop(q, None)
            self.used_qubits.add(q)

    def _schedule_gate(self, instr, key, name, adj):
        if key in self.qubit_ops:
            # There are previous operations on this qubit, so check if the last one was the adjoint of this one.
            if self.qubit_ops[key][-1][1] == adj:
                (other_instr, _) = self.qubit_ops[key].pop()
                # Erase the adjoint instruction and the current instruction since they cancel out.
                other_instr.erase()
                instr.erase()
            elif (
                len(self.qubit_ops[key]) > 1
                and name == "h"
                and self.qubit_ops[key][-1][1] == "s"
                and self.qubit_ops[key][-2][1] == "h"
            ):
                # We have a sequence of h s h, which can be replaced with a single sx.
                self.builder.insert_before(instr)
                self.builder.call(self.sx_func, [instr.args[0]])
                instr.erase()
                (other_instr, _) = self.qubit_ops[key].pop()
                other_instr.erase()
                (other_instr, _) = self.qubit_ops[key].pop()
                other_instr.erase()
            else:
                # The last operation was not the adjoint of this one, so add this instruction to the list.
                self.qubit_ops[key].append((instr, name))
                self.used_qubits.add(key)
                self.last_meas.pop(key, None)

            if len(self.qubit_ops[key]) == 0:
                # There are no more operations on this qubit, so pop it's entry to avoid having empty lists in the dict.
                self.qubit_ops.pop(key)

        else:
            # No previous operations on this qubit, so create a new list from this instruction.
            self.qubit_ops[key] = [(instr, name)]
            self.used_qubits.add(key)
            self.last_meas.pop(key, None)

    def _schedule_rotation(self, instr, key, name):
        if isinstance(instr.args[0], FloatConstant):
            # The angle is constant, so we can try to fold this rotation with other instances of the same rotation
            # tht are constant.
            if key in self.qubit_ops:
                if self.qubit_ops[key][-1][1] == name and isinstance(
                    self.qubit_ops[key][-1][0].args[0], FloatConstant
                ):
                    # The last operation on this qubit was also a rotation of the same type by a constant angle.
                    (other_instr, _) = self.qubit_ops[key].pop()
                    new_angle = instr.args[0].value + other_instr.args[0].value
                    sign = -1 if new_angle < 0 else 1
                    abs_new_angle = abs(new_angle)
                    # Normalize the angle to be within 0 to 2*pi
                    while abs_new_angle > 2 * pi:
                        abs_new_angle -= 2 * pi
                    new_angle = sign * abs_new_angle
                    if (
                        abs(new_angle) > TOLERANCE
                        and abs(abs(new_angle) - (2 * pi)) > TOLERANCE
                    ):
                        # Create a new rotation instruction with the sum of the angles,
                        # and insert it, but only if the angle is above our threshold.
                        self.builder.insert_before(instr)
                        new_instr = self.builder.call(
                            instr.callee,
                            [const(self.double_ty, new_angle), instr.args[1]],
                        )
                        self.qubit_ops[key].append((new_instr, name))
                        self.used_qubits.add(key)
                        self.last_meas.pop(key, None)
                    # Erase the old instructions the new rotation replaces.
                    other_instr.erase()
                    instr.erase()
                else:
                    # Can't fold this rotation with the previous one, so just add it to the list.
                    self.qubit_ops[key].append((instr, name))
                    self.used_qubits.add(key)
                    self.last_meas.pop(key, None)

                if len(self.qubit_ops[key]) == 0:
                    # There are no more operations on this qubit, so pop it's entry to avoid having empty lists in the dict.
                    self.qubit_ops.pop(key)

            else:
                # No previous operations on this qubit, so create a new list from this instruction.
                self.qubit_ops[key] = [(instr, name)]
                self.used_qubits.add(key)
                self.last_meas.pop(key, None)
        else:
            # This angle is not constant, so append it to the list of operations on this qubit.
            if key in self.qubit_ops:
                self.qubit_ops[key].append((instr, name))
            else:
                self.qubit_ops[key] = [(instr, name)]
            self.used_qubits.add(key)
            self.last_meas.pop(key, None)

    def _on_function(self, function):
        self.last_meas = {}
        self.qubit_ops = {}
        super()._on_function(function)
        # At the end of a function, if there are any remaining entries in self.last_meas, it means
        # that there were measurements on qubits that were never reset. Convert those into mresetz.
        for key, (instr, target, result) in self.last_meas.items():
            self.builder.insert_before(instr)
            self.builder.call(
                self.mresetz_func,
                [target, result],
            )
            instr.erase()
        for key in self.qubit_ops:
            if self.qubit_ops[key][-1][1] == "reset":
                # The last operation on this qubit was a reset, so we can drop it.
                (instr, _) = self.qubit_ops[key].pop()
                instr.erase()

    def _on_block(self, block):
        # Each block is independent, so start from an empty list of operations per qubit.
        self.qubit_ops = {}
        self.last_meas = {}
        super()._on_block(block)

    def _on_call_instr(self, call):
        if call.callee.name == "__quantum__qis__sx__body":
            self._drop_ops([call.args[0]])
        elif call.callee.name == "__quantum__qis__move__body":
            self._drop_ops([call.args[0]])
        elif call.callee.name == "__quantum__qis__barrier__body":
            # Don't optimize across barrier calls. Treat this as a drop of all tracked gates,
            # which effectively flushes all scheduled operations.
            self.qubit_ops = {}
            self.last_meas = {}
        else:
            super()._on_call_instr(call)

    def _on_qis_h(self, call, target):
        self._schedule_gate(call, qubit_id(target), "h", "h")

    def _on_qis_s(self, call, target):
        self._schedule_gate(call, qubit_id(target), "s", "s_adj")

    def _on_qis_s_adj(self, call, target):
        self._schedule_gate(call, qubit_id(target), "s_adj", "s")

    def _on_qis_t(self, call, target):
        self._schedule_gate(call, qubit_id(target), "t", "t_adj")

    def _on_qis_t_adj(self, call, target):
        self._schedule_gate(call, qubit_id(target), "t_adj", "t")

    def _on_qis_x(self, call, target):
        self._schedule_gate(call, qubit_id(target), "x", "x")

    def _on_qis_y(self, call, target):
        self._schedule_gate(call, qubit_id(target), "y", "y")

    def _on_qis_z(self, call, target):
        self._schedule_gate(call, qubit_id(target), "z", "z")

    def _on_qis_rx(self, call, angle, target):
        self._schedule_rotation(call, qubit_id(target), "rx")

    def _on_qis_rxx(self, call, angle, target1, target2):
        self._drop_ops([target1, target2])

    def _on_qis_ry(self, call, angle, target):
        self._schedule_rotation(call, qubit_id(target), "ry")

    def _on_qis_ryy(self, call, angle, target1, target2):
        self._drop_ops([target1, target2])

    def _on_qis_rz(self, call, angle, target):
        self._schedule_rotation(call, qubit_id(target), "rz")

    def _on_qis_rzz(self, call, angle, target1, target2):
        self._drop_ops([target1, target2])

    def _on_qis_ccx(self, call, ctrl1, ctrl2, target):
        self._drop_ops([ctrl1, ctrl2, target])

    def _on_qis_cx(self, call, target1, target2):
        self._drop_ops([target1, target2])

    def _on_qis_cy(self, call, target1, target2):
        self._drop_ops([target1, target2])

    def _on_qis_cz(self, call, target1, target2):
        self._drop_ops([target1, target2])

    def _on_qis_swap(self, call, target1, target2):
        self._drop_ops([target1, target2])

    def _on_qis_m(self, call, target, result):
        self._drop_ops([target])
        self.last_meas[qubit_id(target)] = (call, target, result)

    def _on_qis_mz(self, call, target, result):
        self._on_qis_m(call, target, result)

    def _on_qis_mresetz(self, call, target, result):
        self._on_qis_m(call, target, result)

    def _on_qis_reset(self, call, target):
        id = qubit_id(target)
        if id in self.last_meas:
            # Since the last operation on this qubit was a measurement,
            # we can combine that measurement with the reset.
            (instr, target, result) = self.last_meas.pop(id)
            instr.erase()
            self.builder.insert_before(call)
            new_call = self.builder.call(
                self.mresetz_func,
                [target, result],
            )
            call.erase()
            self.last_meas[qubit_id(target)] = (new_call, target, result)
        elif not id in self.used_qubits:
            # This qubit was never used, so we can just erase the reset instruction.
            call.erase()
        elif id in self.qubit_ops and self.qubit_ops[id][-1][1] == "reset":
            # The last operation on this qubit was also a reset, so we drop the current,
            # extra one.
            call.erase()
        else:
            self._drop_ops([target])
            self._schedule_gate(call, id, "reset", "")


class PruneUnusedFunctions(QirModuleVisitor):
    def _on_module(self, module):
        # Assume every non-entry point function is unused.
        self.funcs_to_drop = [f for f in module.functions if not is_entry_point(f)]
        super()._on_module(module)
        # Delete all unused functions.
        for func in self.funcs_to_drop:
            func.delete()

    def _on_call_instr(self, call):
        # Remove calls to initialization and barrier functions, since they aren't handled
        # by most of the stack.
        if call.callee.name == "__quantum__rt__initialize":
            call.erase()
        elif call.callee.name == "__quantum__qis__barrier__body":
            call.erase()
        elif call.callee in self.funcs_to_drop:
            # This function is used in a call, so remove it from the list of
            # functions to drop.
            assert isinstance(call.callee, Function)
            self.funcs_to_drop.remove(call.callee)
