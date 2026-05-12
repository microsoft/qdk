# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import gc
import sys
import weakref

import qdk
import pytest

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
    """Callables created via eval carry a _qdk_context attribute.

    The attribute is a weakref that resolves to the owning Context. Storing a
    weakref (rather than a direct reference) avoids creating a Python <-> Rust
    reference cycle between the generated Python callable and the native
    Interpreter that owns its underlying Q# callable.
    """
    ctx = qdk.Context()
    ctx.eval("function Add(a : Int, b : Int) : Int { a + b }")
    add_fn = ctx.code.Add
    assert hasattr(add_fn, "_qdk_context")
    assert add_fn._qdk_context() is ctx


def test_stale_callable_after_reinit() -> None:
    """Callables from a prior init() become invalid after re-initialization."""
    qdk.init()
    qsharp.eval("function Stale() : Int { 99 }")
    old_fn = qdk.code.Stale
    # Reinitialize — old callable should now be stale
    qdk.init()
    with pytest.raises(QSharpError, match="disposed"):
        old_fn()


def test_default_context_replaced_on_reinit() -> None:
    """Re-calling qdk.init() releases the prior default context and registers
    the new one's callables, with no leak of the old context object.

    Covers the full handover: prior context dispose, sys.modules cleanup of
    the prior namespace, weak-reference release of the prior context object,
    and registration of the new context's callables under qdk.code.
    """
    from qdk import _interpreter as _qdk_interp

    # Establish a fresh default with a uniquely-named callable.
    qdk.init()
    qsharp.eval("namespace OldNs.Inner { function OldFn() : Int { 1 } }")
    assert qdk.code.OldNs.Inner.OldFn() == 1
    # Default context's _code_prefix is "qdk.code".
    assert "qdk.code.OldNs" in sys.modules
    assert "qdk.code.OldNs.Inner" in sys.modules

    old_ctx_ref = weakref.ref(_qdk_interp._default_context)
    old_fn = qdk.code.OldNs.Inner.OldFn

    # Re-initialize — installs a fresh default context.
    qdk.init()

    # The new default is a distinct object.
    assert _qdk_interp._default_context is not None
    assert old_ctx_ref() is not _qdk_interp._default_context

    # Old namespace attributes are removed from the shared qdk.code module.
    assert not hasattr(qdk.code, "OldNs")
    # And from sys.modules.
    assert "qdk.code.OldNs" not in sys.modules
    assert "qdk.code.OldNs.Inner" not in sys.modules

    # The old callable still raises the disposed-context error.
    with pytest.raises(QSharpError, match="disposed"):
        old_fn()

    # New callables register cleanly under the same qdk.code module.
    qsharp.eval("namespace NewNs { function NewFn() : Int { 2 } }")
    assert qdk.code.NewNs.NewFn() == 2
    assert "qdk.code.NewNs" in sys.modules

    # The old default context is now collectable — no leak.
    del old_fn
    gc.collect()
    assert (
        old_ctx_ref() is None
    ), "previous default Context was not released after qdk.init() replaced it"

    # Cleanup so subsequent tests start from a clean default namespace.
    qdk.init()


def test_config_property() -> None:
    """Context exposes a .config property with the target profile."""
    ctx = qdk.Context(target_profile=qdk.TargetProfile.Base)
    assert ctx._config.get_target_profile() == "base"


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


def test_sys_modules_cleared_after_dispose() -> None:
    """Dropping a Context removes its synthetic modules from sys.modules."""
    ctx = qdk.Context()
    ctx.eval("namespace Foo.Bar { function Hello() : Int { 7 } }")
    prefix = ctx._code_prefix
    del ctx
    gc.collect()
    leaked = [key for key in sys.modules if key.startswith(f"{prefix}.")]
    assert leaked == []


def test_context_manager_returns_self() -> None:
    """Using a Context as a context manager yields the Context itself."""
    ctx = qdk.Context()
    with ctx as got:
        assert got is ctx
    assert ctx._disposed is True


def test_context_manager_disposes_on_exit() -> None:
    """Exiting the with-block disposes the context and allows gc."""
    with qdk.Context() as ctx:
        ctx.eval("function Foo() : Int { 42 }")
        ref = weakref.ref(ctx)
        assert ctx.code.Foo() == 42
    # Outside the with-block, dispose() ran.
    assert ctx._disposed is True
    del ctx
    gc.collect()
    assert ref() is None


def test_context_manager_disposes_on_exception() -> None:
    """An exception inside the with-block still triggers dispose()."""
    with pytest.raises(RuntimeError, match="boom"):
        with qdk.Context() as ctx:
            ctx.eval("function Bar() : Int { 1 }")
            raise RuntimeError("boom")
    # ctx is still in scope; dispose() ran during __exit__.
    assert ctx._disposed is True


def _build_interpreter_cycle() -> "weakref.ref":
    """Construct an Interpreter whose make_callable closes over a holder.

    The closure forms a cycle:
    holder -> Interpreter (Rust) -> make_callable (Py<PyAny>) -> closure ->
    holder. Returning only a `weakref.ref` lets the strong references go out
    of scope so the cycle becomes unreachable.
    """
    from qdk._native import Interpreter, TargetProfile

    class Holder:
        pass

    holder = Holder()

    def make_callable(*_args):
        # Capture `holder` so the cycle exists at construction time.
        _ = holder

    holder.interp = Interpreter(  # type: ignore[attr-defined]
        TargetProfile.Unrestricted,
        None,
        None,
        None,
        None,
        None,
        None,
        make_callable,
        None,
        None,
    )
    return weakref.ref(holder)


def test_interpreter_gc_breaks_cycle() -> None:
    """Python's cyclic GC can collect Interpreter cycles via __traverse__/__clear__."""
    ref = _build_interpreter_cycle()
    gc.collect()
    assert (
        ref() is None
    ), "Interpreter cycle should be collectable via __traverse__/__clear__"


def _rss_kib() -> int:
    import resource

    raw = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    # macOS ru_maxrss is in bytes; Linux is in kilobytes (see getrusage(2)).
    return raw // 1024 if sys.platform == "darwin" else raw


@pytest.mark.slow
def test_context_soak_no_growth() -> None:
    """1000 dropped Contexts must release with bounded RSS growth."""
    # Warm up: prime caches once so first-context allocation cost is excluded.
    warm = qdk.Context()
    warm.eval("function Warm() : Int { 1 }")
    del warm
    gc.collect()

    baseline_kib = _rss_kib()
    refs = []
    for _ in range(1000):
        ctx = qdk.Context()
        ctx.eval("function Foo() : Int { 42 }")
        refs.append(weakref.ref(ctx))
        del ctx
        gc.collect()

    alive = sum(1 for r in refs if r() is not None)
    assert alive == 0, f"{alive}/1000 contexts still alive"

    growth_mib = (_rss_kib() - baseline_kib) / 1024
    assert growth_mib < 50, f"RSS grew by {growth_mib:.1f} MiB over 1000 contexts"


def test_non_default_context_no_sys_modules() -> None:
    """Non-default Contexts never write to sys.modules.

    Using `from qsharp.code.<ns> import ...` is only
    supported for the default context. Non-default contexts expose Q# code via
    `ctx.code.<ns>.<Name>` attribute access without registering anything in
    `sys.modules`.
    """
    ctx = qdk.Context()
    ctx.eval("namespace Foo.Bar { function Hello() : Int { 7 } }")
    prefix = ctx._code_prefix

    leaked = [
        key for key in sys.modules if key == prefix or key.startswith(f"{prefix}.")
    ]
    assert leaked == [], f"non-default context leaked sys.modules entries: {leaked}"

    # Attribute access still works.
    assert ctx.code.Foo.Bar.Hello() == 7


def test_default_context_supports_import_path() -> None:
    """Default context's namespaces are registered in sys.modules so
    `from qdk.code.<ns> import <name>` works."""
    qdk.init()
    qdk.qsharp.eval("namespace Demo.Sub { function Answer() : Int { 42 } }")

    # Import path must work for the default context.
    from qdk.code.Demo.Sub import Answer  # type: ignore[import-not-found]

    assert Answer() == 42
