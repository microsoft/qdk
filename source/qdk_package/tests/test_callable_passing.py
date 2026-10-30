# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pathlib import Path

import pytest
import qdk
from qdk import qsharp
from qdk._native import QSharpError
from qdk.simulation import run_qir
from expecttest import assert_expected_inline


def _write_entry_project(root: Path, source: str) -> None:
    (root / "src").mkdir()
    (root / "qsharp.json").write_text("{}", encoding="utf-8")
    (root / "src" / "Entry.qs").write_text(
        f"namespace Test {{ {source} }}", encoding="utf-8"
    )


@pytest.mark.parametrize(
    "expected,argument,compatible",
    [
        ("Int -> Int", "Good", True),
        ("Int -> Int", "WrongOutput", False),
        ("Int -> Int", "WrongInput", False),
        ("Int -> Int", "WrongKind", False),
        ("Int -> Int", "Save(2)", True),
        ("Int -> Double", "Save(2)", False),
        ("A -> A", "IdA", True),
        ("A -> A", "IdB", False),
        ("Int -> A", "MakeB", False),
        ("Int -> Int", "Identity", None),
        ("(Int -> Int)[]", ["Good", "Good"], True),
        ("(Int -> Int)[]", ["Good", "WrongOutput"], False),
        ("(Int, Int -> Int)", (7, "WrongOutput"), False),
        ("Config", {"Callback": "WrongOutput"}, False),
        ("Qubit => Unit is Adj", "Strong", True),
        ("Qubit => Unit is Adj", "Plain", False),
        ("Qubit => Unit is Adj", "Adjoint Strong", True),
        ("(Qubit[], Qubit) => Unit is Adj", "Controlled Strong", True),
        ("Qubit => Unit", "Controlled Strong", False),
        ("(Qubit[], (Qubit[], Qubit)) => Unit", "Controlled Controlled Strong", True),
        ("(Qubit => Unit is Adj + Ctl) => Unit", "ReadAdjoint", True),
        ("(Qubit => Unit) => Unit", "ReadAdjoint", False),
        ("Unit -> (Qubit => Unit is Adj)", "Factory", True),
        ("Unit -> (Qubit => Unit is Adj + Ctl)", "WeakFactory", False),
    ],
)
def test_callable_argument_signature_compatibility(
    expected: str, argument: object, compatible: bool | None
) -> None:
    ctx = qdk.Context(target_profile=qsharp.TargetProfile.Base)
    ctx.eval("""
        function Good(n : Int) : Int { n + 1 }
        function WrongOutput(n : Int) : Double { 0.5 }
        function WrongInput(n : Double) : Int { 0 }
        operation WrongKind(n : Int) : Int { n }
        function Save(n : Int) : Int -> Int { x -> x + n }
        struct A { Data : Int }
        struct B { Data : Int }
        function IdA(a : A) : A { a }
        function IdB(b : B) : B { b }
        function MakeB(n : Int) : B { new B { Data = n } }
        function Identity<'T>(value : 'T) : 'T { value }
        struct Config { Callback : Int -> Int }
        operation Strong(q : Qubit) : Unit is Adj + Ctl { X(q); }
        operation Plain(q : Qubit) : Unit { X(q); }
        operation ReadAdjoint(op : Qubit => Unit is Adj) : Unit {
            use q = Qubit(); Adjoint op(q); Reset(q);
        }
        function Factory() : Qubit => Unit is Adj + Ctl { Strong }
        function WeakFactory() : Qubit => Unit { Plain }
    """)
    # An unused argument still needs validation, before any target effects.
    if isinstance(argument, list):
        argument = [ctx.eval(name) for name in argument]
    elif isinstance(argument, tuple):
        argument = (argument[0], ctx.eval(argument[1]))
    elif isinstance(argument, dict):
        argument = {field: ctx.eval(name) for field, name in argument.items()}
    elif compatible is None:
        assert isinstance(argument, str)
        argument = getattr(ctx.code, argument)
    else:
        assert isinstance(argument, str)
        argument = ctx.eval(argument)
    ctx.eval(f"""
        operation Probe(callback : {expected}) : Result {{
            use q = Qubit(); X(q); MResetZ(q)
        }}
    """)
    if compatible:
        assert ctx.code.Probe(argument) == qsharp.Result.One
        assert run_qir(
            str(ctx.compile(ctx.code.Probe, argument)), shots=1, type="cpu"
        ) == [qsharp.Result.One]
    else:
        for invoke in [
            lambda: ctx.code.Probe(argument),
            lambda: ctx.compile(ctx.code.Probe, argument),
        ]:
            if compatible is None:
                with pytest.raises(QSharpError, match="unsupported input type: `Param"):
                    invoke()
            else:
                with pytest.raises(TypeError, match="incompatible callable type: expected .*found"):
                    invoke()


@pytest.mark.parametrize(
    "target_profile",
    [
        qsharp.TargetProfile.Base,
        qsharp.TargetProfile.Adaptive_RI,
        qsharp.TargetProfile.Adaptive_RIF,
        qsharp.TargetProfile.Adaptive,
    ],
)
@pytest.mark.parametrize(
    "entry_path",
    [
        "context-run", "context-invoke", "module-run", "module-invoke",
        "context-compile", "module-compile", "circuit-static", "circuit-simulate",
        "logical-counts", "estimate", "trace",
    ],
)
def test_callable_mismatch_is_rejected_before_effects(
    target_profile: qsharp.TargetProfile,
    entry_path: str,
    capsys: pytest.CaptureFixture[str],
) -> None:
    qsharp.init(target_profile=target_profile)
    qsharp.eval("""
        function Bad(n : Int) : Double { Message("callback executed"); 0.5 }
        function Good(n : Int) : Int { n }
        function Coerce(data : (Double, Double[]), read : Int -> Int) : Double {
            let (first, rest) = data;
            first + rest[0] + Std.Convert.IntAsDouble(read(3))
        }
        operation Probe(callback : Int -> Int) : Result {
            Message("target executed");
            let n = callback(0);
            use q = Qubit();
            if n == 0 { X(q); }
            MResetZ(q)
        }
    """)
    from qdk.code import Bad, Coerce, Good, Probe
    from qdk._interpreter import _get_context_or_default
    from qdk.qre.interop._qsharp import trace_from_entry_expr

    ctx = _get_context_or_default(Probe)
    assert Coerce((1, [2]), Good) == 6.0
    assert ctx.code.Probe(Good) == qsharp.Result.One
    assert "target executed" in capsys.readouterr().out
    paths = {
        "context-run": lambda: ctx.run(Probe, 1, Bad),
        "context-invoke": lambda: ctx.code.Probe(Bad),
        "module-run": lambda: qsharp.run(Probe, 1, Bad),
        "module-invoke": lambda: Probe(Bad),
        "context-compile": lambda: ctx.compile(Probe, Bad),
        "module-compile": lambda: qsharp.compile(Probe, Bad),
        "circuit-static": lambda: ctx.circuit(
            Probe, Bad, generation_method=qsharp.CircuitGenerationMethod.Static
        ),
        "circuit-simulate": lambda: ctx.circuit(
            Probe, Bad, generation_method=qsharp.CircuitGenerationMethod.Simulate
        ),
        "logical-counts": lambda: ctx.logical_counts(Probe, Bad),
        "estimate": lambda: qsharp.estimate(Probe, None, Bad),
        "trace": lambda: trace_from_entry_expr(Probe, use_trace_backend=True, args=(Bad,)),
    }
    with pytest.raises(TypeError, match="expected .*Int -> Int.*found .*Int -> Double"):
        paths[entry_path]()
    assert capsys.readouterr().out == ""


@pytest.mark.parametrize(
    "target_profile",
    [
        qsharp.TargetProfile.Base,
        qsharp.TargetProfile.Adaptive_RI,
        qsharp.TargetProfile.Adaptive_RIF,
        qsharp.TargetProfile.Adaptive,
    ],
)
def test_saved_generic_closure_preserves_capture_types(
    target_profile: qsharp.TargetProfile,
) -> None:
    qsharp.init(target_profile=target_profile)
    qsharp.eval("""
        operation Phase(n : Int, q : Qubit) : Unit is Adj + Ctl {
            for _ in 1..n { S(q); }
        }
        newtype Count = Int;
        operation PhaseCount(n : Count, q : Qubit) : Unit is Adj + Ctl { Phase(n!, q); }
        operation PhaseArray(ns : Int[], q : Qubit) : Unit is Adj + Ctl {
            for n in ns { Phase(n, q); }
        }
        function Save<'T>(read : ('T, Qubit) => Unit is Adj + Ctl, value : 'T)
            : Qubit => Unit is Adj + Ctl { read(value, _) }
        function SaveInt(read : (Int, Qubit) => Unit is Adj + Ctl, value : Int)
            : Qubit => Unit is Adj + Ctl { read(value, _) }
        function SaveSecond<'Unused, 'T>(
            unused : 'Unused, read : ('T, Qubit) => Unit is Adj + Ctl, value : 'T
        ) : Qubit => Unit is Adj + Ctl { read(value, _) }
        function Missing<'T>(unused : 'T) : Qubit => Unit is Adj + Ctl {
            q => { let values : 'T[] = []; if Length(values) == 0 { S(q); } }
        }
        operation Run(f : Qubit => Unit is Adj + Ctl) : Result {
            use q = Qubit();
            H(q);
            S(q);
            Adjoint f(q);
            H(q);
            MResetZ(q)
        }
        operation RunPair(
            first : Qubit => Unit is Adj + Ctl, second : Qubit => Unit is Adj + Ctl
        ) : Result[] { [Run(first), Run(second)] }
    """)
    from qdk.code import Run, RunPair

    expected = [[qsharp.Result.Zero, qsharp.Result.One]] * 3
    for first, second in [
        ("SaveInt(Phase, 1)", "SaveInt(Phase, 3)"),
        ("Save(Phase, 1)", "Save(Phase, 3)"),
        ("SaveSecond((), Phase, 1)", "SaveSecond((), Phase, 3)"),
        ("Save(Phase, 1)", "Save(PhaseArray, [3])"),
        ("Save(PhaseCount, Count(1))", "Save(Phase, 3)"),
    ]:
        saved = (qsharp.eval(first), qsharp.eval(second))
        assert qsharp.run(RunPair, 3, *saved) == expected, (first, second)
        for program in [
            qsharp.compile(f"RunPair({first}, {second})"),
            qsharp.compile(RunPair, *saved),
        ]:
            assert run_qir(str(program), shots=3, type="cpu") == expected, (first, second)

    missing = qsharp.eval("Missing(1)")
    assert qsharp.run(Run, 3, missing) == [qsharp.Result.Zero] * 3
    assert run_qir(
        str(qsharp.compile("Run(Missing(1))")), shots=3, type="cpu"
    ) == [qsharp.Result.Zero] * 3
    with pytest.raises(QSharpError, match=r"Qdk\.Qsc\.EntryPoint\.MissingTypeArguments"):
        qsharp.compile(Run, missing)


@pytest.mark.parametrize(
    "target_profile",
    [
        qsharp.TargetProfile.Base,
        qsharp.TargetProfile.Adaptive_RI,
        qsharp.TargetProfile.Adaptive_RIF,
        qsharp.TargetProfile.Adaptive,
    ],
)
@pytest.mark.parametrize(
    "entry_name,attribute", [("Main", ""), ("Run", "@EntryPoint() ")]
)
@pytest.mark.parametrize(
    "parameters,body",
    [
        ("'T", "Int { 42 }"),
        ("'T : Eq + Show", "Int { 42 }"),
        ("'T : Add", "Int { 42 }"),
        ("'T", '\'T { fail "unreachable" }'),
    ],
)
def test_project_generated_generic_entry_reports_missing_type_arguments(
    tmp_path: Path,
    target_profile: qsharp.TargetProfile,
    entry_name: str,
    attribute: str,
    parameters: str,
    body: str,
) -> None:
    _write_entry_project(
        tmp_path, f"{attribute}operation {entry_name}<{parameters}>() : {body}"
    )

    with pytest.raises(
        QSharpError,
        match=(
            r"Qdk\.Qsc\.(?:EntryPoint\.MissingTypeArguments|"
            r"LowerAst\.UnusedGeneric)"
        ),
    ) as error:
        qsharp.init(project_root=str(tmp_path), target_profile=target_profile)

    message = str(error.value)
    if "Qdk.Qsc.EntryPoint.MissingTypeArguments" in message:
        assert f"operation {entry_name}" in message
        assert "concrete type evidence" in message
    else:
        assert "unused generic type parameter" in message


@pytest.mark.parametrize(
    "target_profile",
    [
        qsharp.TargetProfile.Base,
        qsharp.TargetProfile.Adaptive_RI,
        qsharp.TargetProfile.Adaptive_RIF,
        qsharp.TargetProfile.Adaptive,
    ],
)
def test_project_nongeneric_entry_expression_forms_preserve_results(
    tmp_path: Path, target_profile: qsharp.TargetProfile
) -> None:
    _write_entry_project(
        tmp_path, "operation Main() : Result { use q = Qubit(); X(q); MResetZ(q) }"
    )
    qsharp.init(project_root=str(tmp_path), target_profile=target_profile)

    assert qsharp.eval("Test.Main()") == qsharp.Result.One
    for expression in ["Test.Main()", "(Test.Main())", " Test.Main() "]:
        qir = str(qsharp.compile(expression))
        assert run_qir(qir, shots=4, type="cpu") == [qsharp.Result.One] * 4


@pytest.mark.parametrize(
    "target_profile",
    [
        qsharp.TargetProfile.Base,
        qsharp.TargetProfile.Adaptive_RI,
        qsharp.TargetProfile.Adaptive_RIF,
        qsharp.TargetProfile.Adaptive,
    ],
)
@pytest.mark.parametrize(
    "left,right",
    [
        (("X",), ("H", "H")),
        (("H", "H"), ("X",)),
        (("I", "I", "X"), ("I",)),
    ],
    ids=["one_then_two", "two_then_one", "three_then_one"],
)
def test_nested_callable_arrays_preserve_captured_operations(
    target_profile: qsharp.TargetProfile,
    left: tuple[str, ...],
    right: tuple[str, ...],
) -> None:
    qsharp.init(target_profile=target_profile)
    qsharp.eval("""
        operation ApplyAll(ops : (Qubit => Unit)[], q : Qubit) : Unit {
            for op in ops { op(q); }
        }
        function MakeAll(ops : (Qubit => Unit)[]) : Qubit => Unit { ApplyAll(ops, _) }
        operation Run(f : Qubit => Unit) : Result {
            use q = Qubit();
            f(q);
            MResetZ(q)
        }
    """)
    from qdk.code import MakeAll, Run

    argument = MakeAll(
        [
            MakeAll([qsharp.eval(name) for name in left]),
            MakeAll([qsharp.eval(name) for name in right]),
        ]
    )
    expected = [qsharp.Result.One] * 3
    assert qsharp.run(Run, 3, argument) == expected

    expression = (
        f"Run(MakeAll([MakeAll([{', '.join(left)}]), "
        f"MakeAll([{', '.join(right)}])]))"
    )
    for program in [qsharp.compile(expression), qsharp.compile(Run, argument)]:
        assert run_qir(str(program), shots=3, type="cpu") == expected


@pytest.mark.parametrize(
    "target_profile",
    [
        qsharp.TargetProfile.Base,
        qsharp.TargetProfile.Adaptive_RI,
        qsharp.TargetProfile.Adaptive_RIF,
        qsharp.TargetProfile.Adaptive,
    ],
)
def test_nested_adjoint_closures_preserve_within_apply(
    target_profile: qsharp.TargetProfile,
) -> None:
    qsharp.init(target_profile=target_profile)
    qsharp.eval("""
        operation ApplyAll(ops : (Qubit[] => Unit is Adj)[], qs : Qubit[]) : Unit is Adj {
            for op in ops { op(qs); }
        }
        operation Once(op : Qubit[] => Unit is Adj, qs : Qubit[]) : Unit is Adj { op(qs); }
        operation Flip(i : Int, qs : Qubit[]) : Unit is Adj { X(qs[i]); }
        operation FlipFirst(qs : Qubit[]) : Unit is Adj { X(qs[0]); }
        function MakeAll(ops : (Qubit[] => Unit is Adj)[]) : Qubit[] => Unit is Adj {
            ApplyAll(ops, _)
        }
        function MakeOnce(op : Qubit[] => Unit is Adj) : Qubit[] => Unit is Adj { Once(op, _) }
        function MakeFlip(i : Int) : Qubit[] => Unit is Adj { Flip(i, _) }
        operation Run(f : Qubit[] => Unit is Adj) : Result[] {
            use qs = Qubit[2];
            within { Adjoint f(qs); } apply { CNOT(qs[0], qs[1]); }
            MResetEachZ(qs)
        }
    """)
    from qdk.code import FlipFirst, MakeAll, MakeFlip, MakeOnce, Run

    argument = MakeOnce(MakeAll([MakeAll([FlipFirst]), MakeAll([MakeFlip(1)])]))
    expected = [[qsharp.Result.Zero, qsharp.Result.One]] * 3
    assert qsharp.run(Run, 3, argument) == expected

    for program in [
        qsharp.compile(
            "Run(MakeOnce(MakeAll([MakeAll([FlipFirst]), MakeAll([MakeFlip(1)])])))"
        ),
        qsharp.compile(Run, argument),
    ]:
        assert run_qir(str(program), shots=3, type="cpu") == expected


@pytest.mark.parametrize(
    "target_profile",
    [
        qsharp.TargetProfile.Base,
        qsharp.TargetProfile.Adaptive_RI,
        qsharp.TargetProfile.Adaptive_RIF,
        qsharp.TargetProfile.Adaptive,
    ],
)
def test_nested_closures_preserve_callable_and_integer_captures(
    target_profile: qsharp.TargetProfile,
) -> None:
    qsharp.init(target_profile=target_profile)
    qsharp.eval("""
        struct Wrapper { Op : Qubit[] => Unit is Adj }
        operation ApplyAll(ops : (Qubit[] => Unit is Adj)[], qs : Qubit[]) : Unit is Adj {
            for op in ops { op(qs); }
        }
        operation Flip(i : Int, qs : Qubit[]) : Unit is Adj { X(qs[i]); }
        operation FlipSecond(qs : Qubit[]) : Unit is Adj { X(qs[1]); }
        operation Rep(op : Qubit[] => Unit is Adj, count : Int, qs : Qubit[]) : Unit is Adj {
            for _ in 1..count { op(qs); }
        }
        function MakeAll(ops : (Qubit[] => Unit is Adj)[]) : Qubit[] => Unit is Adj {
            ApplyAll(ops, _)
        }
        function MakeFlip(i : Int) : Qubit[] => Unit is Adj { Flip(i, _) }
        function MakeRep(op : Qubit[] => Unit is Adj, count : Int) : Qubit[] => Unit is Adj {
            Rep(op, count, _)
        }
        function Wrap(op : Qubit[] => Unit is Adj) : Wrapper { new Wrapper { Op = op } }
        operation RunWrapped(w : Wrapper) : Result[] {
            let f = w.Op;
            use qs = Qubit[2];
            f(qs);
            MResetEachZ(qs)
        }
    """)
    from qdk.code import FlipSecond, MakeAll, MakeFlip, MakeRep, RunWrapped, Wrap

    argument = Wrap(MakeAll([MakeRep(MakeFlip(0), 1), MakeRep(FlipSecond, 2)]))
    expected = [[qsharp.Result.One, qsharp.Result.Zero]] * 3
    assert qsharp.run(RunWrapped, 3, argument) == expected

    for program in [
        qsharp.compile(
            "RunWrapped(Wrap(MakeAll([MakeRep(MakeFlip(0), 1), MakeRep(FlipSecond, 2)])))"
        ),
        qsharp.compile(RunWrapped, argument),
    ]:
        assert run_qir(str(program), shots=3, type="cpu") == expected


def test_python_callable_passed_to_python_callable() -> None:
    qsharp.init()
    qsharp.eval("""
        function InvokeWithFive(f : Int -> Int) : Int {
            f(5)
        }
        function AddOne(x : Int) : Int {
            x + 1
        }
    """)
    from qdk.code import InvokeWithFive, AddOne

    assert InvokeWithFive(AddOne) == 6


def test_python_callable_passed_to_qsharp_callable() -> None:
    qsharp.init()
    qsharp.eval("""
        function InvokeWithFive(f : Int -> Int) : Int {
            f(5)
        }
        function AddOne(x : Int) : Int {
            x + 1
        }
    """)
    from qdk.code import InvokeWithFive

    f = qsharp.eval("AddOne")
    assert InvokeWithFive(f) == 6


def test_run_qsharp_callable_passed_to_qsharp_callable() -> None:
    qsharp.init()
    qsharp.eval("""
        function InvokeWithFive(f : Int -> Int) : Int {
            f(5)
        }
        function AddOne(x : Int) : Int {
            x + 1
        }
    """)
    invoke_with_five = qsharp.eval("InvokeWithFive")
    add_one = qsharp.eval("AddOne")
    res = qsharp.run(invoke_with_five, 1, add_one)[0]
    assert res == 6


def test_run_qsharp_callable_passed_to_python_callable() -> None:
    qsharp.init()
    qsharp.eval("""
        function InvokeWithFive(f : Int -> Int) : Int {
            f(5)
        }
        function AddOne(x : Int) : Int {
            x + 1
        }
    """)
    from qdk.code import InvokeWithFive

    add_one = qsharp.eval("AddOne")
    res = qsharp.run(InvokeWithFive, 1, add_one)[0]
    assert res == 6


def test_python_callable_with_unsupported_types_passed_to_python_callable() -> None:
    qsharp.init()
    qsharp.eval("""
        function MakeRange() : Range {
            1..10
        }
        function SumRangeFromMaker(maker : Unit -> Range) : Int {
            mutable sum = 0;
            for v in maker() {
                sum += v;
            }
            sum
        }
    """)
    from qdk.code import MakeRange, SumRangeFromMaker

    assert SumRangeFromMaker(MakeRange) == 55


def test_qsharp_closure_from_python_callable_passed_to_python_callable() -> None:
    qsharp.init()
    qsharp.eval("""
        function InvokeWithFive(f : Int -> Int) : Int {
            f(5)
        }
        function MakeAdd(inc : Int) : Int -> Int {
            x -> x + inc
        }
    """)
    from qdk.code import InvokeWithFive, MakeAdd

    assert InvokeWithFive(MakeAdd(1)) == 6


def test_qir_from_python_callable_passed_to_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
            use qs = Qubit[nQubits];
            f(qs)
        }
        operation AllH(qs : Qubit[]) : Unit {
            ApplyToEach(H, qs);
        }
    """)
    from qdk.code import InvokeWithQubits, AllH

    qir = qsharp.compile(InvokeWithQubits, 3, AllH)
    assert_expected_inline(
        str(qir),
        """\
%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="3" "required_num_results"="0" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
""",
    )


def test_qir_from_qsharp_callable_passed_to_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
            use qs = Qubit[nQubits];
            f(qs)
        }
        operation AllH(qs : Qubit[]) : Unit {
            ApplyToEach(H, qs);
        }
    """)
    from qdk.code import InvokeWithQubits

    all_h = qsharp.eval("AllH")
    qir = qsharp.compile(InvokeWithQubits, 3, all_h)
    assert_expected_inline(
        str(qir),
        """\
%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="3" "required_num_results"="0" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
""",
    )


def test_qir_from_qsharp_closure_passed_to_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
            use qs = Qubit[nQubits];
            f(qs)
        }
    """)
    from qdk.code import InvokeWithQubits

    apply_h = qsharp.eval("ApplyToEach(H, _)")
    qir = qsharp.compile(InvokeWithQubits, 3, apply_h)
    assert_expected_inline(
        str(qir),
        """\
%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="3" "required_num_results"="0" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
""",
    )


def test_same_target_multi_closure_args_generate_qir() -> None:
    # Python mirror of the Rust
    # `same_target_multi_closure_args_route_to_synthetic_entry_and_generate_qir`
    # test in source/compiler/qsc/src/codegen/tests.rs.
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeThree(
            first : Qubit => Unit,
            second : Qubit => Unit,
            third : Qubit => Unit
        ) : Unit {
            use q = Qubit();
            first(q);
            second(q);
            third(q);
        }

        function MakeRz(theta : Double) : Qubit => Unit {
            Rz(theta, _)
        }
    """)
    from qdk.code import InvokeThree

    first = qsharp.eval("MakeRz(1.0)")
    second = qsharp.eval("MakeRz(2.0)")
    third = qsharp.eval("MakeRz(3.0)")

    qir = str(qsharp.compile(InvokeThree, first, second, third))
    expected_calls = [
        "call void @__quantum__qis__rz__body(double 1.0,",
        "call void @__quantum__qis__rz__body(double 2.0,",
        "call void @__quantum__qis__rz__body(double 3.0,",
    ]
    assert [qir.count(call) for call in expected_calls] == [1, 1, 1]
    positions = [qir.index(call) for call in expected_calls]
    assert positions == sorted(positions)


def test_nested_closure_arg_generates_inner_effect() -> None:
    # Python mirror of the Rust
    # `nested_closure_arg_routes_to_synthetic_entry_and_generates_inner_effect`
    # test in source/compiler/qsc/src/codegen/tests.rs.
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeOne(op : Qubit => Unit) : Unit {
            use q = Qubit();
            op(q);
        }

        function MakeRz(theta : Double) : Qubit => Unit {
            Rz(theta, _)
        }

        function MakeOuter(inner : Qubit => Unit) : Qubit => Unit {
            inner(_)
        }
    """)
    from qdk.code import InvokeOne

    outer = qsharp.eval("let inner = MakeRz(4.0); MakeOuter(inner)")

    qir = str(qsharp.compile(InvokeOne, outer))
    assert qir.count("call void @__quantum__qis__rz__body(double 4.0,") == 1


def test_qir_from_returned_for_each_callable_capturing_length_alias() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RI)
    qsharp.eval("""
        import Std.Arrays.ForEach;
        function Make() : Int[][] => Int[] {
            let f = Length;
            ForEach(arr => f(arr), _)
        }
        operation Apply(op : Int[][] => Int[], arrs : Int[][]) : Int[] {
            return op(arrs);
        }
    """)

    from qdk import code

    op = code.Make()
    qir = str(qsharp.compile(code.Apply, op, [[1, 2], [1]]))
    assert "i64 2" in qir
    assert "i64 1" in qir


def test_qir_from_returned_for_each_callable_calling_length_directly() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RI)
    qsharp.eval("""
        import Std.Arrays.ForEach;
        function Make() : Int[][] => Int[] {
            ForEach(arr => Length(arr), _)
        }
        operation ToLengths(arrs : Int[][]) : Int[] {
            return Make()(arrs);
        }
        operation Apply(op : Int[][] => Int[], arrs : Int[][]) : Int[] {
            return op(arrs);
        }
    """)

    from qdk import code

    assert qsharp.eval("ToLengths([[1, 2], [1]])") == [2, 1]
    op = code.Make()
    qir = str(qsharp.compile(code.Apply, op, [[1, 2], [1]]))
    assert "__quantum__rt__array_record_output(i64 2" in qir
    assert "__quantum__rt__int_record_output(i64 2" in qir
    assert "__quantum__rt__int_record_output(i64 1" in qir


def test_circuit_from_python_callable_passed_to_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
            use qs = Qubit[nQubits];
            f(qs)
        }
        operation AllH(qs : Qubit[]) : Unit {
            ApplyToEach(H, qs);
        }
    """)
    from qdk.code import InvokeWithQubits, AllH

    circuit = qsharp.circuit(InvokeWithQubits, 3, AllH)
    assert_expected_inline(
        str(circuit),
        """q_0    ── H ──
q_1    ── H ──
q_2    ── H ──
""",
    )


def test_circuit_from_qsharp_callable_passed_to_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
            use qs = Qubit[nQubits];
            f(qs)
        }
        operation AllH(qs : Qubit[]) : Unit {
            ApplyToEach(H, qs);
        }
    """)
    from qdk.code import InvokeWithQubits

    all_h = qsharp.eval("AllH")
    circuit = qsharp.circuit(InvokeWithQubits, 3, all_h)
    assert_expected_inline(
        str(circuit),
        """q_0    ── H ──
q_1    ── H ──
q_2    ── H ──
""",
    )


def test_circuit_from_qsharp_closure_passed_to_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
            use qs = Qubit[nQubits];
            f(qs)
        }
    """)
    from qdk.code import InvokeWithQubits

    apply_h = qsharp.eval("ApplyToEach(H, _)")
    circuit = qsharp.circuit(InvokeWithQubits, 3, apply_h)
    assert_expected_inline(
        str(circuit),
        """q_0    ── H ──
q_1    ── H ──
q_2    ── H ──
""",
    )


def test_qir_from_callable_returning_closure_passed_to_qsharp_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    qsharp.eval("""
        function First<'T>(arr : 'T[]) : 'T {
            arr[0]
        }
        function MakeOp(qs : Qubit[]) : Unit => Unit is Adj + Ctl {
            () => H(First(qs))
        }
        operation DoOp(make : Qubit[] -> Unit => Unit is Adj + Ctl) : Unit is Adj + Ctl {
            use qs = Qubit[1];
            let op = make(qs);
            op();
        }
    """)
    from qdk.code import DoOp, MakeOp

    qir = qsharp.compile(DoOp, MakeOp)
    assert "__quantum__qis__h__body" in str(qir)


def test_chemistry_like_controlled_factory_generates_qir() -> None:
    # Python mirror of the Rust `chemistry_like_controlled_factory_generates_qir`
    # test in source/compiler/qsc/src/codegen/tests.rs.
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation PrepareIdentity(qs : Qubit[]) : Unit is Adj + Ctl {}

        operation SelectIdentity(systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {}

        function MakeControlledPrepSelPrepOp(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            numAncillaQubits : Int,
            power : Int
        ) : (Qubit, Qubit[]) => Unit {
            (control, allQubits) => {
                let systems = allQubits[0..numSystemQubits - 1];
                let ancilla = allQubits[numSystemQubits...];
                for _ in 0..power - 1 {
                    Controlled prepareOp([control], systems);
                    Controlled selectOp([control], (systems, ancilla));
                }
            }
        }

        operation MakeControlledPrepSelPrepCircuit(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            numAncillaQubits : Int,
            power : Int
        ) : Unit {
            use control = Qubit();
            use systems = Qubit[numSystemQubits + numAncillaQubits];
            let op = MakeControlledPrepSelPrepOp(
                prepareOp,
                selectOp,
                numSystemQubits,
                numAncillaQubits,
                power
            );
            op(control, systems);
        }
    """)
    from qdk.code import (
        MakeControlledPrepSelPrepCircuit,
        PrepareIdentity,
        SelectIdentity,
    )

    qir = str(
        qsharp.compile(
            MakeControlledPrepSelPrepCircuit,
            PrepareIdentity,
            SelectIdentity,
            1,
            1,
            1,
        )
    )
    assert "define i64 @ENTRYPOINT__main()" in qir


def test_chemistry_like_state_preparation_closure_with_empty_expansion_ops_generates_qir() -> (
    None
):
    # Python mirror of the Rust
    # `chemistry_like_state_preparation_closure_with_empty_expansion_ops_generates_qir`
    # test in source/compiler/qsc/src/codegen/tests.rs.
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        struct StatePreparationParams {
            rowMap : Int[],
            stateVector : Double[],
            expansionOps : Int[][],
            numQubits : Int
        }

        operation ApplyStatePreparation(params : StatePreparationParams, qs : Qubit[]) : Unit is Adj + Ctl {
            if Length(params.expansionOps) != 0 {
                X(qs[0]);
            }
        }

        operation MakeStatePreparationCircuit(
            rowMap : Int[],
            stateVector : Double[],
            expansionOps : Int[][],
            numQubits : Int
        ) : Unit {
            use qs = Qubit[numQubits];
            ApplyStatePreparation(
                new StatePreparationParams {
                    rowMap = rowMap,
                    stateVector = stateVector,
                    expansionOps = expansionOps,
                    numQubits = numQubits
                },
                qs
            );
        }

        function MakeStatePreparationOp(
            rowMap : Int[],
            stateVector : Double[],
            expansionOps : Int[][],
            numQubits : Int
        ) : Qubit[] => Unit is Adj + Ctl {
            ApplyStatePreparation(
                new StatePreparationParams {
                    rowMap = rowMap,
                    stateVector = stateVector,
                    expansionOps = expansionOps,
                    numQubits = numQubits
                },
                _
            )
        }

        operation SelectIdentity(systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {}

        function MakeControlledPrepSelPrepOp(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            numAncillaQubits : Int,
            power : Int
        ) : (Qubit, Qubit[]) => Unit {
            (control, allQubits) => {
                let systems = allQubits[0..numSystemQubits - 1];
                let ancilla = allQubits[numSystemQubits...];
                for _ in 0..power - 1 {
                    Controlled prepareOp([control], systems);
                    Controlled selectOp([control], (systems, ancilla));
                }
            }
        }

        operation MakeControlledPrepSelPrepCircuit(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            numAncillaQubits : Int,
            power : Int
        ) : Unit {
            use control = Qubit();
            use systems = Qubit[numSystemQubits + numAncillaQubits];
            let op = MakeControlledPrepSelPrepOp(
                prepareOp,
                selectOp,
                numSystemQubits,
                numAncillaQubits,
                power
            );
            op(control, systems);
        }
    """)
    from qdk.code import (
        MakeControlledPrepSelPrepCircuit,
        MakeStatePreparationCircuit,
        SelectIdentity,
    )

    state_prep_args = ([0], [1.0, 0.0], [], 1)

    direct_qir = str(qsharp.compile(MakeStatePreparationCircuit, *state_prep_args))
    assert "define i64 @ENTRYPOINT__main()" in direct_qir

    prepare_op = qsharp.eval("MakeStatePreparationOp([0], [1.0, 0.0], [], 1)")
    nested_qir = str(
        qsharp.compile(
            MakeControlledPrepSelPrepCircuit,
            prepare_op,
            SelectIdentity,
            1,
            1,
            1,
        )
    )
    assert "define i64 @ENTRYPOINT__main()" in nested_qir


def test_chemistry_like_iqpe_params_struct_generates_qir() -> None:
    # Python mirror of the Rust `chemistry_like_iqpe_params_struct_generates_qir`
    # test in source/compiler/qsc/src/codegen/tests.rs.
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        import Std.Arrays.Subarray;

        struct IterativePhaseEstimationParams {
            statePrep : Qubit[] => Unit,
            repControlledUnitary : (Qubit, Qubit[]) => Unit,
            accumulatePhase : Double,
            phaseQubit : Int,
            systems : Int[],
            numAncillaQubits : Int
        }

        operation PrepareSystems(systems : Qubit[]) : Unit {}

        operation RepControlledUnitary(control : Qubit, targets : Qubit[]) : Unit {}

        operation RunIQPE(params : IterativePhaseEstimationParams) : Result[] {
            use qs = Qubit[Length(params.systems) + 1 + params.numAncillaQubits];
            let phaseQubit = qs[params.phaseQubit];
            let systems = Subarray(params.systems, qs);
            let ancillas = if params.numAncillaQubits == 0 {
                []
            } else {
                qs[1 + Length(params.systems)..Length(qs) - 1]
            };
            let allTargets = systems + ancillas;

            params.statePrep(systems);

            within {
                H(phaseQubit);
            } apply {
                Rz(params.accumulatePhase, phaseQubit);
                params.repControlledUnitary(phaseQubit, allTargets);
            }
            ResetAll(allTargets);
            return [MResetZ(phaseQubit)];
        }

        operation MakeIQPECircuit(
            statePrep : Qubit[] => Unit,
            repControlledUnitary : (Qubit, Qubit[]) => Unit,
            accumulatePhase : Double,
            phaseQubit : Int,
            systems : Int[],
            numAncillaQubits : Int
        ) : Result[] {
            return RunIQPE(new IterativePhaseEstimationParams {
                statePrep = statePrep,
                repControlledUnitary = repControlledUnitary,
                accumulatePhase = accumulatePhase,
                phaseQubit = phaseQubit,
                systems = systems,
                numAncillaQubits = numAncillaQubits
            });
        }
    """)
    from qdk.code import MakeIQPECircuit, PrepareSystems, RepControlledUnitary

    qir = str(
        qsharp.compile(
            MakeIQPECircuit,
            PrepareSystems,
            RepControlledUnitary,
            0.25,
            0,
            [1],
            1,
        )
    )
    assert "define i64 @ENTRYPOINT__main()" in qir


def test_chemistry_like_sequential_partial_application_generates_qir() -> None:
    # Python mirror of the Rust
    # `chemistry_like_sequential_partial_application_generates_qir`
    # test in source/compiler/qsc/src/codegen/tests.rs.
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        import Std.Arrays.Subarray;

        operation ApplyFirstStep(systems : Qubit[]) : Unit {
            for q in systems {
                H(q);
            }
        }

        operation ApplySecondStep(systems : Qubit[]) : Unit {
            for q in systems {
                X(q);
            }
        }

        operation ApplyThirdStep(systems : Qubit[]) : Unit {
            for q in systems {
                Z(q);
            }
        }

        operation ApplySequential(
            first : Qubit[] => Unit,
            second : Qubit[] => Unit,
            systems : Qubit[]
        ) : Unit {
            first(systems);
            second(systems);
        }

        function MakeSequentialOp(
            first : Qubit[] => Unit,
            second : Qubit[] => Unit
        ) : Qubit[] => Unit {
            ApplySequential(first, second, _)
        }

        function MaxInt(values : Int[]) : Int {
            mutable max = values[0];
            for idx in 1 .. Length(values) - 1 {
                let value = values[idx];
                if value > max {
                    set max = value;
                }
            }
            return max;
        }

        operation MakeSequentialCircuit(
            first : Qubit[] => Unit,
            second : Qubit[] => Unit,
            targets : Int[]
        ) : Unit {
            if Length(targets) == 0 {
                return ();
            } else {
                let maxTarget = MaxInt(targets);
                use qs = Qubit[1 + maxTarget];
                ApplySequential(first, second, Subarray(targets, qs));
            }
        }
    """)
    from qdk.code import ApplyThirdStep, MakeSequentialCircuit

    sequential = qsharp.eval("MakeSequentialOp(ApplyFirstStep, ApplySecondStep)")
    qir = str(qsharp.compile(MakeSequentialCircuit, sequential, ApplyThirdStep, [0, 1]))
    assert "__quantum__qis__h__body" in qir
    assert "__quantum__qis__x__body" in qir
    assert "__quantum__qis__z__body" in qir


@pytest.mark.xfail(
    raises=QSharpError,
    strict=True,
    reason=(
        "End-to-end gap: the codegen unlock (return_unify on a pinned "
        "ReinvokeOriginal target body) runs inside qir() codegen, but the public Python "
        "eval/init path eagerly runs RCA capability checking on every defined callable "
        "(source/compiler/qsc/src/interpret.rs `with_compiler` + `run_fir_passes`). That "
        "gate rejects the `ReturnWithinDynamicScope` target body at definition time, before "
        "any codegen routing, so neither the SyntheticEntry nor the ReinvokeOriginal variant "
        "is reachable from `qsharp.eval` under an Adaptive profile. The unlock itself is "
        "covered at the codegen layer by the Rust regression "
        "`early_return_in_dynamic_branch_synthetic_and_reinvoke_both_compile_parity` in "
        "source/compiler/qsc/src/codegen/tests.rs. This strict xfail flips (and fails) once "
        "the eager interpreter RCA gains a pinned-body carve-out, at which point the body "
        "below becomes a real passing parity assertion."
    ),
)
def test_qir_early_return_in_dynamic_branch_synthetic_and_reinvoke_parity() -> None:
    # End-to-end parity: a target operation whose body early-returns inside a
    # measurement-dependent branch should compile to QIR under an Adaptive profile for
    # BOTH closure-arg routes. A classical capture is FIR-lowerable and flows through the
    # self-contained synthetic entry; a captured allocated qubit is a runtime identity
    # that forces the pin-based ReinvokeOriginal route, where the body-only
    # signature-preserving sub-pipeline return-unifies the pinned body so the early
    # return becomes flag-guarded forward control flow. Mirrors the Rust regression
    # `early_return_in_dynamic_branch_synthetic_and_reinvoke_both_compile_parity` in
    # source/compiler/qsc/src/codegen/tests.rs.
    #
    # Today this raises QSharpError at the `qsharp.eval` below: the interpreter's eager
    # RCA capability check rejects the `ReturnWithinDynamicScope` body of `RunOp` before
    # codegen routing is ever reached (see the xfail reason above).
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    qsharp.eval("""
        import Std.Measurement.*;

        operation RunOp(op : (Qubit => Unit)) : Int {
            let r = {
                use q = Qubit();
                op(q);
                MResetZ(q)
            };
            if r == One {
                return 1;
            }
            return 2;
        }

        operation Rotate(reps : Int, target : Qubit) : Unit {
            for _ in 1..reps {
                X(target);
            }
        }
        operation MakeRotation(reps : Int) : (Qubit => Unit) {
            return target => Rotate(reps, target);
        }

        operation Entangle(control : Qubit, target : Qubit) : Unit is Adj + Ctl {
            CNOT(control, target);
        }
        operation MakeEntangler(control : Qubit) : (Qubit => Unit) {
            return target => Entangle(control, target);
        }
    """)
    from qdk.code import RunOp

    # SyntheticEntry variant: the closure captures a classical Int, which is
    # FIR-lowerable, so the target routes through the synthetic entry.
    rotation = qsharp.eval("MakeRotation(1)")
    synthetic_qir = str(qsharp.compile(RunOp, rotation))
    assert "__quantum__qis__x__body" in synthetic_qir

    # ReinvokeOriginal variant: the closure captures an allocated qubit (a runtime
    # identity that is NOT FIR-lowerable), forcing the pin-based ReinvokeOriginal
    # route. The allocation is a top-level statement so the captured qubit stays
    # alive for the duration of the closure value.
    entangler = qsharp.eval("use control = Qubit(); MakeEntangler(control)")
    reinvoke_qir = str(qsharp.compile(RunOp, entangler))
    assert (
        "__quantum__qis__cnot__body" in reinvoke_qir
        or "__quantum__qis__cx__body" in reinvoke_qir
    )
