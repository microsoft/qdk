
## v1.31.0


### Classical arithmetic functions

When working on arithmetic algorithms, it can be useful to define an operation
applying a function on a quantum register without implementing it.

Now you can do this in Q# using `Std.ArithmeticTestUtils.ApplyClassicalFunction`.
It takes `n`-qubit quantum register and a Q# function `f : (BigInt) -> BigInt` 
that represents bijection on `0..2^n-1`. Effect of this operation is equivalent to 
applying a unitary operation that maps `|x>` to `|f(x)>`.

We also support multi-register version(`Std.ArithmeticTestUtils.ApplyClassicalFunctionN`).

For example, you can represent in-place addition (equivalent to `Std.Arithmetic.IncByLE`)
as follows:

```
import Std.ArithmeticTestUtils.ApplyClassicalFunctionN;
operation IncByLE(xs : Qubit[], ys : Qubit[]) : Unit is Ctl {
    let mod = 1L << Length(ys);
    ApplyClassicalFunctionN(a -> [a[0], (a[0]+a[1])%mod], [xs, ys]);
}
```

This feature is intended to be used for development and testing of quantum algorithms
and currently is supported only in sparse simulator.


### Arithemtic test helpers

When writing tests for arithmetic operations, we typically allocate registers, write 
inputs there, apply operation and read outputs. We added a helper 
`Std.ArithmeticTestUtils.TestArithmeticOp` to do all that, which can be used in Q# unit 
tests.

We also added a convenient Python wrapper around `TestArithmeticOp`, called 
`ArithmeticOpTester` that allows you to write unit tests for arithmetic operations in 
Python. For example, this is how to write a test for `Std.Arithmetic.IncByLE` that 
checks this operation on 5 random inputs:

```
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
`qdk.Context` constructor) and access it in Q# code using `Std.Core.ConfigValue`.
The calls to `Std.Core.ConfigValue` will be replaced with provided values at 
compilation time.

Example:

```
import qdk
context = qdk.Context(qsharp_config={"shots": 1000})
assert context.eval('Std.Core.ConfigValue("shots", 100)') == 1000
```

See more details [here](https://github.com/microsoft/qdk/tree/main/source/qdk_package#configuration-map).


### Std.ResourceEstimation.IsResourceEstimating

You can now know within Q# code whether your code is being executed for resource 
estimation. This can be useful if you want to have different behavior for resource 
estimation vs. running code on simulator or quantum hardware. 

For example, if you have a loop, you can use `Std.ResourceEstimation.RepeatEstimates`
in resource estimation mode, and a `for` loop otherwise.


### Running Q# tests using Python API.

Q# supports writing unit tests (as operations annotated with `@Test`). Now you can use
Python API to run all unit tests in your Q# package.

Example:

```
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
