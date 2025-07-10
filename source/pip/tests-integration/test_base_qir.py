# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

import qsharp

try:
    from pyqir import (
        Call,
        Context,
        Module,
        Opcode,
        qubit_id,
        result_id,
        required_num_qubits,
        required_num_results,
    )

    PYQIR_AVAILABLE = True
except ImportError:
    PYQIR_AVAILABLE = False

SKIP_REASON = "PyQIR is not available"


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_compile_qir_input_data() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("operation Program() : Result { use q = Qubit(); return M(q) }")
    operation = qsharp.compile("Program()")
    qir = operation._repr_qir_()
    assert isinstance(qir, bytes)
    module = Module.from_ir(Context(), qir.decode(), "module")
    assert len(module.functions) == 4
    assert module.functions[0].name == "ENTRYPOINT__main"
    func = module.functions[0]
    assert len(func.basic_blocks) == 1
    assert len(func.basic_blocks[0].instructions) == 4
    call_m = func.basic_blocks[0].instructions[1]
    assert isinstance(call_m, Call)
    assert call_m.callee.name == "__quantum__qis__m__body"
    assert len(call_m.args) == 2
    assert qubit_id(call_m.args[0]) == 0
    assert result_id(call_m.args[1]) == 0
    record_res = func.basic_blocks[0].instructions[2]
    assert isinstance(record_res, Call)
    assert len(record_res.args) == 2
    assert record_res.callee.name == "__quantum__rt__result_record_output"
    assert result_id(record_res.args[0]) == 0
    assert func.basic_blocks[0].instructions[3].opcode == Opcode.RET


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_compile_qir_all_gates() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    operation = qsharp.compile(
        "{\
        use (q1, q2, q3) = (Qubit(), Qubit(), Qubit());\
        CCNOT(q1, q2, q3);\
        CX(q1, q2);\
        CY(q1, q2);\
        CZ(q1, q2);\
        Rx(0.0, q1);\
        Rxx(0.0, q1, q2);\
        Ry(0.0, q1);\
        Ryy(0.0, q1, q2);\
        Rz(0.0, q1);\
        Rzz(0.0, q1, q2);\
        H(q1);\
        S(q1);\
        Adjoint S(q1);\
        SX(q1); \
        T(q1);\
        Adjoint T(q1);\
        X(q1);\
        Y(q1);\
        Z(q1);\
        SWAP(q1, q2);\
        Reset(q1);\
        (M(q1),\
        Microsoft.Quantum.Measurement.MResetZ(q1))\
        }"
    )
    qir = operation._repr_qir_()
    assert isinstance(qir, bytes)
    module = Module.from_ir(Context(), qir.decode(), "module")
    assert len(module.functions) == 25
    assert module.functions[0].name == "ENTRYPOINT__main"
    func = module.functions[0]
    assert len(func.basic_blocks) == 1
    assert len(func.basic_blocks[0].instructions) == 28

    def check_call(i: int, name: str, num_args: int) -> None:
        call = func.basic_blocks[0].instructions[i + 1]
        assert isinstance(call, Call)
        assert call.callee.name == name
        assert len(call.args) == num_args

    check_call(0, "__quantum__qis__ccx__body", 3)
    check_call(1, "__quantum__qis__cx__body", 2)
    check_call(2, "__quantum__qis__cy__body", 2)
    check_call(3, "__quantum__qis__cz__body", 2)
    check_call(4, "__quantum__qis__rx__body", 2)
    check_call(5, "__quantum__qis__rxx__body", 3)
    check_call(6, "__quantum__qis__ry__body", 2)
    check_call(7, "__quantum__qis__ryy__body", 3)
    check_call(8, "__quantum__qis__rz__body", 2)
    check_call(9, "__quantum__qis__rzz__body", 3)
    check_call(10, "__quantum__qis__h__body", 1)
    check_call(11, "__quantum__qis__s__body", 1)
    check_call(12, "__quantum__qis__s__adj", 1)
    check_call(13, "__quantum__qis__sx__body", 1)
    check_call(14, "__quantum__qis__t__body", 1)
    check_call(15, "__quantum__qis__t__adj", 1)
    check_call(16, "__quantum__qis__x__body", 1)
    check_call(17, "__quantum__qis__y__body", 1)
    check_call(18, "__quantum__qis__z__body", 1)
    check_call(19, "__quantum__qis__swap__body", 2)
    check_call(20, "__quantum__qis__cx__body", 2)
    check_call(21, "__quantum__qis__m__body", 2)
    check_call(22, "__quantum__qis__m__body", 2)
    check_call(23, "__quantum__rt__tuple_record_output", 2)
    check_call(24, "__quantum__rt__result_record_output", 2)
    check_call(25, "__quantum__rt__result_record_output", 2)

    assert required_num_qubits(module.functions[0]) == 5
    assert required_num_results(module.functions[0]) == 2
