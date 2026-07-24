
## v1.31.0


### Classical arithmetic functions

When working on arithmetic algorithms, it can be useful to define an operation
that applies a function to a quantum register without implementing it.

Now you can do this in Q# using `Std.ArithmeticTestUtils.ApplyClassicalFunction`.
It takes an `n`-qubit quantum register and a Q# function `f : (BigInt) -> BigInt`
that represents a bijection on `0..2^n-1`. The effect of this operation is equivalent to
applying a unitary operation that maps `|x>` to `|f(x)>`.

We also support a multi-register version (`Std.ArithmeticTestUtils.ApplyClassicalFunctionN`).

For example, you can represent in-place addition (equivalent to `Std.Arithmetic.IncByLE`)
as follows:

```qsharp
import Std.ArithmeticTestUtils.ApplyClassicalFunctionN;
operation IncByLE(xs : Qubit[], ys : Qubit[]) : Unit is Ctl {
    let mod = 1L << Length(ys);
    ApplyClassicalFunctionN(a -> [a[0], (a[0]+a[1])%mod], [xs, ys]);
}
```

This feature is intended to be used for development and testing of quantum algorithms
and is currently supported only in the sparse simulator.


### Arithmetic test helpers

When writing tests for arithmetic operations, we typically allocate registers, write
inputs to them, apply an operation, and read the outputs. We added a helper
`Std.ArithmeticTestUtils.TestArithmeticOp` to do all that, which can be used in Q# unit
tests.

We also added a convenient Python wrapper around `TestArithmeticOp`, called
`ArithmeticOpTester`, that allows you to write unit tests for arithmetic operations in
Python. For example, this is how to write a test for `Std.Arithmetic.IncByLE` that
checks this operation on 5 random inputs:

```python
import random
from qdk.test_utils import ArithmeticOpTester
n = 10
tester = ArithmeticOpTester("Std.Arithmetic.IncByLE", [n, n])
for _ in range(5):
    x, y = random.randint(0, 2**n - 1), random.randint(0, 2**n - 1)
    assert tester.run([x, y]) == [x, (x + y) % (2**n)]
```


### Compile-time configuration

You can now specify a compile-time configuration (as a Python dictionary passed to 
`qsharp.init` or `qdk.Context` constructor) and access it in Q# code using 
`Std.Core.ConfigValue`. Calls to `Std.Core.ConfigValue` will be replaced with the 
provided values at compilation time.

Example:

```python
from qdk import qsharp
qsharp.init(qdk_config={"shots": 1000})
assert qsharp.eval('Std.Core.ConfigValue("shots", 100)') == 1000
```

See the [configuration map documentation](https://github.com/microsoft/qdk/tree/main/source/qdk_package#configuration-map) for more details.


### Resource estimation mode detection

You can now determine within Q# whether your code is being executed for resource
estimation. This can be useful if you want different behavior for resource
estimation versus running code on a simulator or quantum hardware.

For example, if you have a loop, you can use `Std.ResourceEstimation.RepeatEstimates`
in resource estimation mode, and a `for` loop otherwise.


### Running Q# tests using the Python API

Q# supports writing unit tests (as operations annotated with `@Test`). Now you can use
the Python API to run all unit tests in your Q# package.

Example:

```python
from qdk import qsharp
from qdk.test_utils import run_tests

qsharp.eval("""
import Std.Diagnostics.Fact;

@Test()
operation MyTest() : Unit {
    Fact(2 + 2 == 4, "assertion failed");
}
""")

run_tests()
```
