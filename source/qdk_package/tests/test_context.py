import gc
import weakref

import pytest
import qdk
from qdk import qsharp
from qdk.qsharp import QSharpError


def test_eval() -> None:
    ctx = qdk.Context()
    result = ctx.eval("1 + 2")
    assert result == 3


def test_run() -> None:
    ctx = qdk.Context()
    ctx.eval("operation Main() : Result { use q = Qubit(); X(q); MResetZ(q) }")
    assert ctx.run("Main()", 2) == [qdk.Result.One, qdk.Result.One]
    assert ctx.code.Main() == qdk.Result.One


def test_compile() -> None:
    ctx = qdk.Context(target_profile=qdk.TargetProfile.Base)
    ctx.eval("operation Program() : Result { use q = Qubit(); MResetZ(q) }")
    program = ctx.compile("Program()")
    assert isinstance(program._repr_qir_(), bytes)


def test_circuit() -> None:
    ctx = qdk.Context()
    ctx.eval("operation Program() : Result { use q = Qubit(); H(q); MResetZ(q) }")
    circuit = ctx.circuit("Program()")
    assert "H" in str(circuit)


def test_logical_counts() -> None:
    ctx = qdk.Context(target_profile=qdk.TargetProfile.Base)
    ctx.eval("operation Program() : Result { use q = Qubit(); MResetZ(q) }")
    counts = ctx.logical_counts("Program()")
    assert counts["numQubits"] == 1


def test_seed() -> None:
    ctx1 = qdk.Context()
    ctx2 = qdk.Context()

    # Classical seed.
    ctx1.set_classical_seed(100)
    ctx2.set_classical_seed(100)
    value1 = ctx1.eval("Microsoft.Quantum.Random.DrawRandomInt(0, 100)")
    value2 = ctx2.eval("Microsoft.Quantum.Random.DrawRandomInt(0, 100)")
    assert value1 == value2

    # Quantum seed.
    code = """{
        use qs = Qubit[16];
        for q in qs { H(q); };
        Microsoft.Quantum.Measurement.MResetEachZ(qs)
    }"""
    ctx1.set_quantum_seed(100)
    ctx2.set_quantum_seed(100)
    value1 = ctx1.eval(code)
    value2 = ctx2.eval(code)
    assert value1 == value2


def test_dump_machine() -> None:
    ctx = qdk.Context(target_profile=qdk.TargetProfile.Unrestricted)
    ctx.eval("use q = Qubit(); X(q);")
    state_dump = ctx.dump_machine()
    assert state_dump.qubit_count == 1
    assert state_dump.as_dense_state() == [0, 1]


def test_import_openqasm() -> None:
    """import_openqasm loads and runs an OpenQASM program in this context."""
    ctx = qdk.Context()
    ctx.import_openqasm(
        """
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit q;
        output bit c;
        x q;
        c = measure q;
        reset q;
    """,
        name="Program",
    )

    results = ctx.run("{ use q = Qubit(); Program(q) }", 1)
    assert results == [qdk.Result.One]


def test_context_callable_has_context_ref() -> None:
    """Callables created via eval carry a _qdk_context attribute."""
    ctx = qdk.Context()
    ctx.eval("function Add(a : Int, b : Int) : Int { a + b }")
    add_fn = ctx.code.Add
    assert hasattr(add_fn, "_qdk_context")
    assert add_fn._qdk_context is ctx


def test_stale_callable_after_reinit() -> None:
    """Callables from a prior init() become invalid after re-initialization."""
    qdk.init()
    qsharp.eval("function Stale() : Int { 99 }")
    old_fn = qdk.code.Stale
    # Reinitialize — old callable should now be stale
    qdk.init()
    with pytest.raises(QSharpError, match="disposed"):
        old_fn()


def test_config_property() -> None:
    """Context exposes a .config property with the target profile."""
    ctx = qdk.Context(target_profile=qdk.TargetProfile.Base)
    assert ctx._config.get_target_profile() == "base"
    assert ctx.get_target_profile() == qdk.TargetProfile.Base


def test_context_isolation() -> None:
    ctx1 = qdk.Context()
    ctx2 = qdk.Context()
    ctx1.eval("function Foo() : Int { 42 }")
    assert ctx1.eval("Foo()") == 42
    # s2 should not have Foo defined.
    with pytest.raises(QSharpError):
        ctx2.eval("Foo()")


def test_cross_context_callable_passing_raises() -> None:
    context_a = qdk.Context()
    context_b = qdk.Context()
    context_a.eval("operation Foo() : Result { use q = Qubit(); M(q) }")
    foo = context_a.code.Foo

    with pytest.raises(QSharpError, match="different Context"):
        context_b.run(foo, 1)

    with pytest.raises(QSharpError, match="different Context"):
        context_b.compile(foo)

    with pytest.raises(QSharpError, match="different Context"):
        context_b.circuit(foo)

    with pytest.raises(Exception, match="different Context"):
        context_b.logical_counts(foo)


def test_global_api_uses_context_of_callable() -> None:
    context_a = qdk.Context(target_profile=qdk.TargetProfile.Base)
    context_a.eval("operation Foo() : Result { use q = Qubit(); MResetZ(q) }")
    foo = context_a.code.Foo

    assert qsharp.run(foo, 1) == [qdk.Result.Zero]
    assert isinstance(qsharp.compile(foo)._repr_qir_(), bytes)
    assert str(qsharp.circuit(foo)) != ""
    assert qsharp.estimate(foo).logical_counts["numQubits"] == 1
    assert qsharp.logical_counts(foo)["numQubits"] == 1


def test_cross_context_struct_passing_raises() -> None:
    context_a = qdk.Context()
    context_b = qdk.Context()
    # Define struct and function in both contexts with same definitionctx.
    code = """
    struct Point { a : Int, b : Int }
    function ProcessPoint(p : Point) : Int { p.a + p.b }
    """
    context_a.eval(code)
    context_b.eval(code)

    # Create a Point struct instance in context_a
    point_from_context_a = context_a.code.Point(3, 4)
    assert context_a.code.ProcessPoint(point_from_context_a) == 7

    with pytest.raises(QSharpError, match="different Context"):
        context_b.code.ProcessPoint(point_from_context_a)


def test_cross_context_callable_as_argument_raises() -> None:
    context_a = qdk.Context()
    context_b = qdk.Context()

    # Define a higher-order function in both contexts
    code = """
    function InvokeWithFive(f : Int -> Int) : Int { f(5) }
    function AddOne(x : Int) : Int { x + 1 }
    """
    context_a.eval(code)
    context_b.eval(code)
    assert context_a.code.InvokeWithFive(context_a.code.AddOne) == 6

    with pytest.raises(QSharpError, match="different Context"):
        context_b.code.InvokeWithFive(context_a.code.AddOne)


def test_circular_reference_raises():
    qsharp.eval("function First(x : Int[]) : Int { x[0] }")
    assert qdk.code.First([1, 2]) == 1

    circular_list = []
    circular_list.append(circular_list)

    with pytest.raises(QSharpError, match="Cannot send circular objects"):
        qdk.code.First(circular_list)


def test_context_released_after_drop() -> None:
    """Dropping the last strong reference to a Context releases it via gc."""
    ctx = qdk.Context()
    ctx.eval("function Add(a : Int, b : Int) : Int { a + b }")
    ref = weakref.ref(ctx)
    del ctx
    gc.collect()
    assert ref() is None


def test_qsharp_config(context: qdk.Context) -> None:
    context = qdk.Context(
        qsharp_config={
            "int_config": 123,
            "bool_config": True,
            "string_config": "value",
            "double_config": 124.1,
        }
    )

    assert context.eval("""Std.Core.GetConfig("int_config", 0)""") == 123
    assert context.eval("""Std.Core.GetConfig("bool_config", true)""") is True
    assert context.eval("""Std.Core.GetConfig("string_config", "")""") == "value"
    assert context.eval("""Std.Core.GetConfig("double_config", 0.0)""") == 124.1

    # Default values.
    assert context.eval("""Std.Core.GetConfig("unknown1", "foo")""") == "foo"
    assert context.eval("""Std.Core.GetConfig("unknown2", false)""") is False
    assert context.eval("""Std.Core.GetConfig("unknown3", 12)""") == 12
    assert context.eval("""Std.Core.GetConfig("unknown4", 12.0)""") == 12.0

    # Wrong type.
    with pytest.raises(
        QSharpError,
        match="configuration value type does not match GetConfig default value type",
    ):
        context.eval("""Std.Core.GetConfig("int_config", false)""")


def test_config_invalid_type(context: qdk.Context) -> None:
    with pytest.raises(
        TypeError, match="config value must be bool, int, float, or str"
    ):
        qdk.Context(qsharp_config={"invalid": {"a": 1}})  # type: ignore

    with pytest.raises(TypeError, match="'int' object is not an instance of 'str'"):
        qdk.Context(qsharp_config={1: 1})  # type: ignore
