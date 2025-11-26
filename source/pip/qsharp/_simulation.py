# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import random
from typing import Callable, List, Optional, Tuple, TypeAlias, Union
import pyqir
from ._native import (
    QirInstructionId,
    QirInstruction,
    run_clifford,
    run_parallel_shots,
    run_cpu_full_state,
    NoiseConfig,
)
from pyqir import (
    Function,
    FunctionType,
    Type,
    qubit_type,
    Linkage,
)
from ._qsharp import QirInputData, Result


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
        # profile_attr = func.attributes.func["qir_profiles"]
        # if profile_attr is None or profile_attr.string_value != "base_profile":
        #     raise ValueError("Only base profile is supported")
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
        elif callee_name == "__quantum__qis__move__body":
            self.gates.append(
                (
                    QirInstructionId.Move,
                    pyqir.qubit_id(call.args[0]),
                )
            )
        elif callee_name == "__quantum__qis__read_result__body":
            self.gates.append(
                (QirInstructionId.ReadResult, pyqir.result_id(call.args[0]))
            )
        elif callee_name == "__quantum__rt__result_record_output":
            tag = self._get_value_as_string(call.args[1])
            self.gates.append(
                (
                    QirInstructionId.ResultRecordOutput,
                    str(pyqir.result_id(call.args[0])),
                    tag,
                )
            )
        elif callee_name == "__quantum__rt__bool_record_output":
            tag = self._get_value_as_string(call.args[1])
            self.gates.append(
                (QirInstructionId.BoolRecordOutput, str(call.args[0].value), tag)
            )
        elif callee_name == "__quantum__rt__int_record_output":
            tag = self._get_value_as_string(call.args[1])
            self.gates.append(
                (QirInstructionId.IntRecordOutput, str(call.args[0].value), tag)
            )
        elif callee_name == "__quantum__rt__double_record_output":
            tag = self._get_value_as_string(call.args[1])
            self.gates.append(
                (QirInstructionId.DoubleRecordOutput, str(call.args[0].value), tag)
            )
        elif callee_name == "__quantum__rt__tuple_record_output":
            tag = self._get_value_as_string(call.args[1])
            self.gates.append(
                (QirInstructionId.TupleRecordOutput, str(call.args[0].value), tag)
            )
        elif callee_name == "__quantum__rt__array_record_output":
            tag = self._get_value_as_string(call.args[1])
            self.gates.append(
                (QirInstructionId.ArrayRecordOutput, str(call.args[0].value), tag)
            )
        else:
            pass


class OutputRecordingPass(pyqir.QirModuleVisitor):
    _output_str = ""
    _closers = []
    _counters = []

    def process_output(self, bitstring: str):
        return eval(
            self._output_str,
            {
                "o": [
                    Result.Zero if x == "0" else Result.One if x == "1" else Result.Loss
                    for x in bitstring
                ]
            },
        )

    def _on_function(self, function):
        if pyqir.is_entry_point(function):
            super()._on_function(function)
            while len(self._closers) > 0:
                self._output_str += self._closers.pop()
                self._counters.pop()

    def _on_rt_result_record_output(self, call, result, target):
        self._output_str += f"o[{pyqir.result_id(result)}]"
        while len(self._counters) > 0:
            self._output_str += ","
            self._counters[-1] -= 1
            if self._counters[-1] == 0:
                self._output_str += self._closers[-1]
                self._closers.pop()
                self._counters.pop()
            else:
                break

    def _on_rt_array_record_output(self, call, value, target):
        self._output_str += "["
        self._closers.append("]")
        # if len(self._counters) > 0:
        #     self._counters[-1] -= 1
        self._counters.append(value.value)

    def _on_rt_tuple_record_output(self, call, value, target):
        self._output_str += "("
        self._closers.append(")")
        # if len(self._counters) > 0:
        #     self._counters[-1] -= 1
        self._counters.append(value.value)


class DecomposeCcxPass(pyqir.QirModuleVisitor):

    h_func: Function
    t_func: Function
    tadj_func: Function
    cz_func: Function

    def __init__(self):
        super().__init__()

    def _on_module(self, module):
        void = Type.void(module.context)
        qubit_ty = qubit_type(module.context)

        # Find or create all the needed functions.
        for func in module.functions:
            match func.name:
                case "__quantum__qis__h__body":
                    self.h_func = func
                case "__quantum__qis__t__body":
                    self.t_func = func
                case "__quantum__qis__t__adj":
                    self.tadj_func = func
                case "__quantum__qis__cz__body":
                    self.cz_func = func
        if not hasattr(self, "h_func"):
            self.h_func = Function(
                FunctionType(void, [qubit_ty]),
                Linkage.EXTERNAL,
                "__quantum__qis__h__body",
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


Simulator: TypeAlias = Callable[
    [List[QirInstruction], int, int, int, NoiseConfig, int], str
]


def preprocess_simulation_input(
    input: Union[QirInputData, str, bytes],
    shots: Optional[int] = 1,
    noise: Optional[NoiseConfig] = None,
    seed: Optional[int] = None,
) -> tuple[pyqir.Module, int, NoiseConfig, int]:
    if noise is None:
        noise = NoiseConfig()
    if shots is None:
        shots = 1
    # If no seed specified, generate a random u32 to use
    if seed is None:
        seed = random.randint(0, 2**32 - 1)
    if isinstance(noise, tuple):
        raise ValueError(
            "Specifying Pauli noise via a tuple is not supported. Use a NoiseConfig instead."
        )

    context = pyqir.Context()
    if isinstance(input, QirInputData):
        mod = pyqir.Module.from_ir(context, str(input))
    elif isinstance(input, str):
        mod = pyqir.Module.from_ir(context, input)
    else:
        mod = pyqir.Module.from_bitcode(context, input)

    return (mod, shots, noise, seed)


def run_qir_clifford(
    input: Union[QirInputData, str, bytes],
    shots: Optional[int] = 1,
    noise: Optional[NoiseConfig] = None,
    seed: Optional[int] = None,
) -> List:
    (mod, shots, noise, seed) = preprocess_simulation_input(input, shots, noise, seed)
    (gates, required_num_qubits, required_num_results) = AggregateGatesPass().run(mod)
    recorder = OutputRecordingPass()
    recorder.run(mod)

    return list(
        map(
            recorder.process_output,
            run_clifford(
                gates, required_num_qubits, required_num_results, shots, noise, seed
            ),
        )
    )


def run_qir_cpu(
    input: Union[QirInputData, str, bytes],
    shots: Optional[int] = 1,
    noise: Optional[NoiseConfig] = None,
    seed: Optional[int] = None,
) -> List:
    (mod, shots, noise, seed) = preprocess_simulation_input(input, shots, noise, seed)
    (gates, required_num_qubits, required_num_results) = AggregateGatesPass().run(mod)
    recorder = OutputRecordingPass()
    recorder.run(mod)

    return list(
        map(
            recorder.process_output,
            run_cpu_full_state(
                gates, required_num_qubits, required_num_results, shots, noise, seed
            ),
        )
    )


def run_qir_gpu(
    input: Union[QirInputData, str, bytes],
    shots: Optional[int] = 1,
    noise: Optional[NoiseConfig] = None,
    seed: Optional[int] = None,
) -> List[str]:
    (mod, shots, noise, seed) = preprocess_simulation_input(input, shots, noise, seed)
    # Ccx is not support in the GPU simulator, decompose it
    DecomposeCcxPass().run(mod)
    (gates, required_num_qubits, required_num_results) = AggregateGatesPass().run(mod)
    recorder = OutputRecordingPass()
    recorder.run(mod)

    return list(
        map(
            recorder.process_output,
            run_parallel_shots(
                gates, shots, required_num_qubits, required_num_results, noise, seed
            ),
        )
    )
