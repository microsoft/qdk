# Writing Q# for the current QDK

Q# changed between the old QDK (`Microsoft.Quantum.*` namespaces, C#-like syntax) and the
current one (`Std.*`). Q# written from memory is often the old dialect and does not compile.
Every statement here was checked against QDK 1.34.

## 1. Syntax that changed

- Loops take no parentheses: `for q in qs { }`, `for i in 0 .. n - 1 { }`,
  `for (a, b) in pairs { }`. Not `for (q in qs) { }`.
- Qubits: `use q = Qubit();` and `use qs = Qubit[n];`. Not `using (q = Qubit()) { }`.
- Logic: `and`, `or`, `not`. Not `&&`, `||`, `!`. There is no `xor`: write `a != b` for
  `Bool` and `a ^^^ b` for `Int`.
- Branches: `if c { } elif d { } else { }`. Not `else if`.
- Arrays: `[0, size = n]` for n zeros, `[]` for an empty one. Not `new Int[n]`.
- Variables: `let x = 1;` cannot change. `mutable x = 1;` can: `x = 2;`, `x += 1;`,
  `xs += [v];`, `xs[i] = v;`.
- Tuples are taken apart with `let (a, b) = pair;`. Not `pair::Item1`.
- Text is built with interpolation: `$"count = {n}"`. `"count = " + n` is a type error.
- There are no list comprehensions: fill an array in a loop, or use `Mapped(x -> x + 1, xs)`.
- A callable that returns a value ends in `return v;`. A body whose last statement is a `for`,
  or an `if` without `else`, returns `Unit`, which is a type error.

```qsharp
function CountAbove(xs : Int[], limit : Int) : Int {
    mutable count = 0;
    for x in xs {
        if x > limit and not (x == 0) {
            count += 1;
        }
    }
    return count;
}
```

## 2. Names: what needs an import, and what does not exist

Open without an import: gates, `M`, `Measure`, `MResetZ`, `Reset`, `Length`, `Repeated`,
`ApplyToEach` and its variants, `ApplyControlledOnInt`, `ApplyControlledOnBitString`
(`Std.Core`, `Std.Canon`, `Std.Intrinsic`, `Std.Measurement`, `Std.Range`). Everything else
needs an import. The `.*` matters:
`import Std.Math;` alone brings in no name. The old form `open Microsoft.Quantum.Math;` is
`import Std.Math.*;` now, and the old namespaces `Arithmetic`, `Preparation`, `Oracles` and
`Characterization` are gone.

Keep the shape of the code you are given. If it has a `namespace` block, keep the block and its
name, and write the imports inside it. If it has none, add none, and write the imports at the
top of the file: a file without a block is a namespace of its own, named after the file, and
other files find its callables under that name. A block you add moves them out of reach.

| Import                      | Gives                                                                                                                                 |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `import Std.Math.*;`        | `PI()`, `E()`, `Sqrt`, `Sin`, `Cos`, `ArcTan2`, `Log`, `AbsD`, `AbsI`, `MaxI`, `MinI`, `Round`, `Truncate`, `Floor`                   |
| `import Std.Convert.*;`     | `IntAsDouble`, `IntAsBoolArray`, `BoolArrayAsInt`, `ResultArrayAsInt`, `ResultAsBool`                                                 |
| `import Std.Arrays.*;`      | `IndexRange`, `Enumerated`, `Zipped`, `Reversed`, `Most`, `Rest`, `Head`, `Tail`, `Mapped`, `Fold`, `All`, `Any`, `Count`, `Filtered` |
| `import Std.Diagnostics.*;` | `Fact`, `DumpMachine`                                                                                                                 |

Names from the old QDK or from other languages that are not found, and what to write:

| Not found                                                        | Write                                                              |
| ---------------------------------------------------------------- | ------------------------------------------------------------------ |
| `Exp(x)` or `ExpD(x)` for a power of e                           | `E() ^ x`. `Exp` is a rotation: see section 5                      |
| `PowD(a, b)`, `PowI(a, b)`, `PowL(a, b)`                         | `a ^ b`                                                            |
| `ControlledOnInt(n, op)(cs, t)`                                  | `ApplyControlledOnInt(n, op, cs, t)`: one call with four arguments |
| `ControlledOnBitString(bits, op)(cs, t)`                         | `ApplyControlledOnBitString(bits, op, cs, t)`, likewise            |
| `LittleEndian(qs)`, `BigEndian(qs)`                              | `qs`: registers are plain `Qubit[]`                                |
| `MultiM(qs)`                                                     | `MeasureEachZ(qs)`                                                 |
| `ResultAsInt(r)`                                                 | `ResultAsBool(r)`, or compare with `One`                           |
| `IsResultZero(r)`, `IsResultOne(r)`                              | `r == Zero`, `r == One`                                            |
| `EqualA(EqualI, xs, ys)`, `EqualI(a, b)`                         | `xs == ys`, `a == b`: arrays and tuples compare with `==`          |
| `IntAsString(n)`, `BoolAsString(b)`, `DoubleAsString(x)`         | `$"{n}"`                                                           |
| `ConstantArray(n, v)`, `Zip(xs, ys)`                             | `Repeated(v, n)`, `Zipped(xs, ys)`                                 |
| `AssertQubit`, `AssertMeasurement`, `EqualityFactI`              | `Fact(condition, "message")`                                       |
| `DumpRegister((), qs)`                                           | `DumpRegister(qs)`                                                 |
| `ApplyToFirstQubit`, `OperationPow`, `CControlledCA`, `Fraction` | Gone: write the index, the loop, the `if` or the tuple yourself    |
| `DoubleAsInt(x)`                                                 | `Truncate(x)`, `Round(x)` or `Floor(x)`                            |
| `Ln(x)`, `Pi()`, `Abs(x)`, `RY`                                  | `Log(x)`, `PI()`, `AbsD(x)` or `AbsI(n)`, `Ry`                     |
| `PauliX(q)`                                                      | `X(q)`. `PauliX` is a value of type `Pauli`, not a gate            |
| `CH(c, t)`                                                       | `Controlled H([c], t)`                                             |
| `Std.Arrays.Length`                                              | `Length`                                                           |

Before you finish, check every library name you used against the two tables: a missing
import is the error most often left in otherwise correct code.

```qsharp
namespace Demo {
    import Std.Convert.*;
    import Std.Math.*;

    function PointOnCircle(step : Int, steps : Int) : (Double, Double) {
        let angle = 2.0 * PI() * IntAsDouble(step) / IntAsDouble(steps);
        return (Cos(angle), Sin(angle));
    }
}
```

The same in a file that has no block:

```qsharp
import Std.Math.*;

function HalfTurn() : Double {
    return PI();
}
```

## 3. Numbers and types

- `Int` and `Double` never mix. `1` is an `Int`, `1.0` a `Double`; convert with
  `IntAsDouble(n)`. `2.0 * n` and `1 + 0.5` are type errors, and `1 / 2` is integer division.
- Powers: `2 ^ k` for `Int`, `2.0 ^ x` with a `Double` exponent, `1 <<< k` for a power of two.
- A `Result` is `Zero` or `One`, never a `Bool` or an `Int`: write `M(q) == One`.
- `Complex` has the fields `Real` and `Imag`; `ComplexPolar` has `Magnitude` and `Argument`.
  There is no `Re`, `Im`, `Imaginary` or `Modulus`. Build one with `Complex(1.0, 2.0)` and read
  it with `c.Real`. A struct is not a tuple: `let (a, b) = c;` is an error.

## 4. Adjoint and controlled operations

An operation declared `is Adj` or `is Adj + Ctl` has those versions generated from its body,
which limits the body:

- It may hold `let`, `for` over a range or an array, `if` on classical values, `use`,
  `within { } apply { }`, and calls to operations.
- It may not change a `mutable` variable (`x += 1;` is the error "cannot generate adjoint with
  this expression"), and may not hold `while`, `repeat`, `return` or a measurement.
- So compute each value with `let` where it is needed, or in a separate `function`, which may
  use `mutable` and `while` freely.
- Every operation it calls must support the same functors. `ApplyToEach` supports none: use
  `ApplyToEachCA` (or `ApplyToEachA`, `ApplyToEachC`). `Reset` and every measurement support
  none either. A helper operation of your own needs
  `is Adj + Ctl` too, and so does the type of an operation passed in:
  `op : Qubit => Unit is Adj + Ctl`.

```qsharp
function Weights(flags : Bool[]) : Double[] {
    mutable weights = [];
    for (index, flag) in Enumerated(flags) {
        weights += [flag ? 1.0 / IntAsDouble(index + 1) | 0.0];
    }
    return weights;
}

operation TiltEach(flags : Bool[], register : Qubit[]) : Unit is Adj + Ctl {
    let weights = Weights(flags);
    for (weight, qubit) in Zipped(weights, register) {
        let angle = weight * PI();
        Ry(angle, qubit);
    }
    ApplyToEachCA(S, register);
}
```

Calling the versions: `Adjoint S(q)`. `Controlled` takes an array of controls, then the
original arguments as one item, a tuple when there are several:

```qsharp
operation Calls(cs : Qubit[], a : Qubit, b : Qubit, theta : Double) : Unit is Adj + Ctl {
    Controlled X(cs, a);
    Controlled X([cs[0]], a);
    Controlled Rz(cs, (theta, a));
    Controlled SWAP(cs, (a, b));
    Controlled Adjoint T(cs, a);
}
```

`Controlled Rz(cs, theta, a)` and `Controlled X(cs[0], a)` are type errors.

## 5. Gates and angles

- One qubit: `H`, `X`, `Y`, `Z`, `S`, `T`, `Rx(theta, q)`, `Ry`, `Rz`, `R1(theta, q)`,
  `R(PauliX, theta, q)`. Two or three: `CNOT`, `CY`, `CZ`, `SWAP`, `CCNOT`.
- `R1(theta, q)` multiplies |1⟩ by e^(i·theta) and leaves |0⟩ alone. `Rz(theta, q)` differs
  from it by a global phase, which becomes a real difference once the gate is controlled.
- `R1Frac(n, k, q)` is `R1(PI() * n / 2^k, q)`: the angle is π·n/2^k, not 2π·n/2^k.
- `Exp(paulis, theta, qubits)` applies e^(i·theta·P): `Exp([PauliZ], theta, [q])` is
  `Rz(-2.0 * theta, q)`.

## 6. Qubits and measurement

- A qubit from `use` starts in |0⟩ and must be back in |0⟩ when its scope ends, or the program
  fails at run time: `Reset(q)`, `ResetAll(qs)`, or measure with `MResetZ(q)`,
  `MResetEachZ(qs)`.
- `M(q)` measures in the Z basis. `Measure([PauliX], [q])` and `MResetX(q)` measure in others.
  `Measure(bases, qubits)` is one joint measurement and returns one `Result`, not an array.
  `MeasureEachZ(qs)` returns a `Result[]`.
- A `function` cannot call an operation or allocate a qubit. Anything that touches a qubit is
  an `operation`.

## 7. Bit order

- Kets and `DumpMachine` list qubit 0 first: after `X(qs[0])` on two qubits the state is |10⟩.
  So when a text writes a register as |j₁j₂…jₙ⟩, or numbers its basis states |k⟩, `qs[0]`
  is the leftmost bit, the most significant one, unless the text says otherwise. Read the
  exercise this way first.
- Only the library callables that relate an integer to an array use the other order,
  little-endian, where index 0 is the least significant bit: `IntAsBoolArray(1, 3)` is `[true, false, false]`,
  and `BoolArrayAsInt`, `ResultArrayAsInt`, `MeasureInteger`, `ApplyXorInPlace` and `ApplyControlledOnInt` agree.
  For a register read the first way, hand them `Reversed(qs)`.
