import qsharp
import pytest

from qsharp import QSharpError


def test_eval() -> None:
    s = qsharp.Session()
    result = s.eval("1 + 2")
    assert result == 3


def test_session_isolation() -> None:
    s1 = qsharp.Session()
    s2 = qsharp.Session()
    s1.eval("function Foo() : Int { 42 }")
    assert s1.eval("Foo()") == 42
    # s2 should not have Foo defined.
    with pytest.raises(QSharpError):
        s2.eval("Foo()")


def test_run() -> None:
    s = qsharp.Session()
    s.eval('operation Foo() : Result { Message("hi"); Zero }')
    results = s.run("Foo()", 3)
    assert results == [qsharp.Result.Zero, qsharp.Result.Zero, qsharp.Result.Zero]


# TODO: test compile,circuit,logcial_counts,set_quantum_seed,set_classical_seed,
# dump_machine,import_openqasm


def test_module_level_backward_compat() -> None:
    qsharp.init()
    result = qsharp.eval("1 + 1")
    assert result == 2


def test_context_callable_has_session_ref() -> None:
    """Callables created via eval carry a _qdk_get_interpreter attribute."""
    s = qsharp.Session()
    s.eval("function Add(a : Int, b : Int) : Int { a + b }")
    add_fn = s.code.Add
    assert hasattr(add_fn, "_qdk_session")
    assert add_fn._qdk_session is s


def test_import_openqasm() -> None:
    """import_openqasm loads an OpenQASM program into the context."""
    s = qsharp.Session()
    s.import_openqasm("""
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit q;
        h q;
        """)


def test_stale_callable_after_reinit() -> None:
    """Callables from a prior init() become invalid after re-initialization."""
    qsharp.init()
    qsharp.eval("function Stale() : Int { 99 }")
    old_fn = qsharp.code.Stale
    # Reinitialize — old callable should now be stale
    qsharp.init()
    with pytest.raises(QSharpError, match="disposed"):
        old_fn()


def test_config_property() -> None:
    """Session exposes a .config property with the target profile."""
    s = qsharp.Session(target_profile=qsharp.TargetProfile.Base)
    assert s.config.get_target_profile() == "base"


def test_cross_session_callable_passing_raises() -> None:
    session_a = qsharp.Session()
    session_b = qsharp.Session()
    session_a.eval("operation Foo() : Result { use q = Qubit(); M(q) }")
    foo = session_a.code.Foo

    with pytest.raises(QSharpError, match="different Session"):
        session_b.run(foo, 1)

    with pytest.raises(QSharpError, match="different Session"):
        session_b.compile(foo)

    with pytest.raises(QSharpError, match="different Session"):
        session_b.circuit(foo)

    with pytest.raises(Exception, match="different Session"):
        session_b.logical_counts(foo)


def test_cross_session_struct_passing_raises() -> None:
    session_a = qsharp.Session()
    session_b = qsharp.Session()
    # Define struct and function in both contexts with same definitions.
    code = """
    struct Point { a : Int, b : Int }
    function ProcessPoint(p : Point) : Int { p.a + p.b }
    """
    session_a.eval(code)
    session_b.eval(code)

    # Create a Point struct instance in session_a
    point_from_session_a = session_a.code.Point(3, 4)
    assert session_a.code.ProcessPoint(point_from_session_a) == 7

    with pytest.raises(QSharpError, match="different Session"):
        session_b.code.ProcessPoint(point_from_session_a)


def test_cross_session_callable_as_argument_raises() -> None:
    session_a = qsharp.Session()
    session_b = qsharp.Session()

    # Define a higher-order function in both contexts
    code = """
    function InvokeWithFive(f : Int -> Int) : Int { f(5) }
    function AddOne(x : Int) : Int { x + 1 }
    """
    session_a.eval(code)
    session_b.eval(code)
    assert session_a.code.InvokeWithFive(session_a.code.AddOne) == 6

    with pytest.raises(QSharpError, match="different Session"):
        session_b.code.InvokeWithFive(session_a.code.AddOne)


def test_circular_reference_raises():
    qsharp.eval("function First(x : Int[]) : Int { x[0] }")
    assert qsharp.code.First([1, 2]) == 1

    circular_list = []
    circular_list.append(circular_list)

    with pytest.raises(
        QSharpError, match="Cannot send circular objects from Python to Q#"
    ):
        qsharp.code.First(circular_list)
