# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from typing import List, Optional, Tuple, Union
import pyqir
from ._native import QirInstructionId, QirInstruction, run_clifford, NoiseConfig


class AggregateGatesPass(pyqir.QirModuleVisitor):
    def __init__(self):
        super().__init__()
        self.gates: List[QirInstruction] = []
        self.required_num_qubits = None
        self.required_num_results = None

    def _get_value_as_string(self, value: pyqir.Value) -> str:
        value = pyqir.extract_byte_string(value)
        if value is None:
            return ""
        value = value.decode("utf-8")
        return value

    def run(self, mod: pyqir.Module) -> Tuple[List[QirInstruction], int, int]:
        errors = mod.verify()
        if errors is not None:
            raise ValueError(f"Module verification failed: {errors}")

        # if len(mod.functions) != 1:
        #    raise ValueError("Only single function modules are supported")

        # verify that the module is base profile
        func = next(filter(pyqir.is_entry_point, mod.functions))
        profile_attr = func.attributes.func["qir_profiles"]
        if profile_attr is None or profile_attr.string_value != "base_profile":
            raise ValueError("Only base profile is supported")
        self.required_num_qubits = pyqir.required_num_qubits(func)
        self.required_num_results = pyqir.required_num_results(func)

        super().run(mod)
        return (self.gates, self.required_num_qubits, self.required_num_results)

    def _on_call_instr(self, call: pyqir.Call) -> None:
        callee_name = call.callee.name
        if callee_name == "__quantum__qis__ccx__body":
            self.gates.append(
                (
                    QirInstructionId.CCX,
                    pyqir.qubit_id(call.args[0]),
                    pyqir.qubit_id(call.args[1]),
                    pyqir.qubit_id(call.args[2]),
                )
            )
        elif callee_name == "__quantum__qis__cx__body":
            self.gates.append(
                (
                    QirInstructionId.CX,
                    pyqir.qubit_id(call.args[0]),
                    pyqir.qubit_id(call.args[1]),
                )
            )
        elif callee_name == "__quantum__qis__cy__body":
            self.gates.append(
                (
                    QirInstructionId.CY,
                    pyqir.qubit_id(call.args[0]),
                    pyqir.qubit_id(call.args[1]),
                )
            )
        elif callee_name == "__quantum__qis__cz__body":
            self.gates.append(
                (
                    QirInstructionId.CZ,
                    pyqir.qubit_id(call.args[0]),
                    pyqir.qubit_id(call.args[1]),
                )
            )
        elif callee_name == "__quantum__qis__swap__body":
            self.gates.append(
                (
                    QirInstructionId.SWAP,
                    pyqir.qubit_id(call.args[0]),
                    pyqir.qubit_id(call.args[1]),
                )
            )
        elif callee_name == "__quantum__qis__rx__body":
            self.gates.append(
                (
                    QirInstructionId.RX,
                    call.args[0].value,
                    pyqir.qubit_id(call.args[1]),
                )
            )
        elif callee_name == "__quantum__qis__rxx__body":
            self.gates.append(
                (
                    QirInstructionId.RXX,
                    call.args[0].value,
                    pyqir.qubit_id(call.args[1]),
                    pyqir.qubit_id(call.args[2]),
                )
            )
        elif callee_name == "__quantum__qis__ry__body":
            self.gates.append(
                (
                    QirInstructionId.RY,
                    call.args[0].value,
                    pyqir.qubit_id(call.args[1]),
                )
            )
        elif callee_name == "__quantum__qis__ryy__body":
            self.gates.append(
                (
                    QirInstructionId.RYY,
                    call.args[0].value,
                    pyqir.qubit_id(call.args[1]),
                    pyqir.qubit_id(call.args[2]),
                )
            )
        elif callee_name == "__quantum__qis__rz__body":
            self.gates.append(
                (
                    QirInstructionId.RZ,
                    call.args[0].value,
                    pyqir.qubit_id(call.args[1]),
                )
            )
        elif callee_name == "__quantum__qis__rzz__body":
            self.gates.append(
                (
                    QirInstructionId.RZZ,
                    call.args[0].value,
                    pyqir.qubit_id(call.args[1]),
                    pyqir.qubit_id(call.args[2]),
                )
            )
        elif callee_name == "__quantum__qis__h__body":
            self.gates.append((QirInstructionId.H, pyqir.qubit_id(call.args[0])))
        elif callee_name == "__quantum__qis__s__body":
            self.gates.append((QirInstructionId.S, pyqir.qubit_id(call.args[0])))
        elif callee_name == "__quantum__qis__s__adj":
            self.gates.append((QirInstructionId.SAdj, pyqir.qubit_id(call.args[0])))
        elif callee_name == "__quantum__qis__sx__body":
            self.gates.append((QirInstructionId.SX, pyqir.qubit_id(call.args[0])))
        elif callee_name == "__quantum__qis__t__body":
            self.gates.append((QirInstructionId.T, pyqir.qubit_id(call.args[0])))
        elif callee_name == "__quantum__qis__t__adj":
            self.gates.append((QirInstructionId.TAdj, pyqir.qubit_id(call.args[0])))
        elif callee_name == "__quantum__qis__x__body":
            self.gates.append((QirInstructionId.X, pyqir.qubit_id(call.args[0])))
        elif callee_name == "__quantum__qis__y__body":
            self.gates.append((QirInstructionId.Y, pyqir.qubit_id(call.args[0])))
        elif callee_name == "__quantum__qis__z__body":
            self.gates.append((QirInstructionId.Z, pyqir.qubit_id(call.args[0])))
        elif callee_name == "__quantum__qis__m__body":
            self.gates.append(
                (
                    QirInstructionId.M,
                    pyqir.qubit_id(call.args[0]),
                    pyqir.result_id(call.args[1]),
                )
            )
        elif callee_name == "__quantum__qis__mz__body":
            self.gates.append(
                (
                    QirInstructionId.MZ,
                    pyqir.qubit_id(call.args[0]),
                    pyqir.result_id(call.args[1]),
                )
            )
        elif callee_name == "__quantum__qis__mresetz__body":
            self.gates.append(
                (
                    QirInstructionId.MResetZ,
                    pyqir.qubit_id(call.args[0]),
                    pyqir.result_id(call.args[1]),
                )
            )
        elif callee_name == "__quantum__qis__reset__body":
            self.gates.append((QirInstructionId.RESET, pyqir.qubit_id(call.args[0])))
        elif callee_name == "__quantum__qis__read_result__body":
            self.gates.append(
                (QirInstructionId.ReadResult, pyqir.result_id(call.args[0]))
            )
        elif callee_name == "__quantum__rt__result_record_output":
            tag = self._get_value_as_string(call.args[1])
            self.gates.append(
                (
                    QirInstructionId.RESULT_RECORD_OUTPUT,
                    str(pyqir.result_id(call.args[0])),
                    tag,
                )
            )
        elif callee_name == "__quantum__rt__bool_record_output":
            tag = self._get_value_as_string(call.args[1])
            self.gates.append(
                (QirInstructionId.BOOL_RECORD_OUTPUT, str(call.args[0].value), tag)
            )
        elif callee_name == "__quantum__rt__int_record_output":
            tag = self._get_value_as_string(call.args[1])
            self.gates.append(
                (QirInstructionId.INT_RECORD_OUTPUT, str(call.args[0].value), tag)
            )
        elif callee_name == "__quantum__rt__double_record_output":
            tag = self._get_value_as_string(call.args[1])
            self.gates.append(
                (QirInstructionId.DOUBLE_RECORD_OUTPUT, str(call.args[0].value), tag)
            )
        elif callee_name == "__quantum__rt__tuple_record_output":
            tag = self._get_value_as_string(call.args[1])
            self.gates.append(
                (QirInstructionId.TUPLE_RECORD_OUTPUT, str(call.args[0].value), tag)
            )
        elif callee_name == "__quantum__rt__array_record_output":
            tag = self._get_value_as_string(call.args[1])
            self.gates.append(
                (QirInstructionId.ARRAY_RECORD_OUTPUT, str(call.args[0].value), tag)
            )
        else:
            pass


def run_qir(
    input: Union[str, bytes],
    shots: Optional[int] = 1,
    noise: Optional[NoiseConfig] = None,
) -> str:
    context = pyqir.Context()
    if isinstance(input, str):
        mod = pyqir.Module.from_ir(context, input)
    else:
        mod = pyqir.Module.from_bitcode(context, input)

    passtoRun = AggregateGatesPass()
    (gates, required_num_qubits, _) = passtoRun.run(mod)

    if noise is None:
        noise = NoiseConfig()
    if shots is None:
        shots = 1

    return run_clifford(gates, required_num_qubits, shots, noise)
