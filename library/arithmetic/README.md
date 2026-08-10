# Arithmetic

This library contains advanced quantum arithmetic algorithms.

Unless otherwise noted, all quantum inputs are interpreted as unsigned little-endian
integers and represented by `Qubit[]`.

The library contains the following algorithms:
* `Add.qs` - quantum-quantum in-place addition and subtraction (modulo `2^n`).
* `AddConst.qs` - quantum-classical addition (modulo `2^n`).
* `AddLookup.qs` - computes `x += table[i]` where `x`, `i` are quantum registers and
    `table` is a classical table. Supports modular and non-modular addition.
* `Compare.qs` - compares two unsigned quantum integers, writing the result to an output
   qubit. Supports inequality and equality.
* Modular arithmetic:
  * `ModAdd.qs` - quantum-quantum in-place addition modulo classical constant.
  * `ModDiv.qs` - division of two quantum numbers modulo classical constant.
    This is based on the Extended Euclidean Algorithm, so some restrictions apply (in
    particular, the divisor must be mutually prime with the modulus). It can also be
    used for modular multiplication and modular inversion.
  * `ModExp.qs` - modular exponentiation (computes `t:=(t*b^x)%m` where
    `t`, `x` are quantum and `b`, `m` are classical).
  * `ModMul.qs` - modular multiplication and square.
  * `ModNegate.qs` - modular negation.
  
### Space-optimized and time-optimized variants

Some algorithms (addition, constant addition, comparison) are implemented with two
different circuit variants: space-optimized (minimizing the number of qubits used) and
time-optimized (minimizing the number of certain gates). The variants are functionally
equivalent but have different resource usage.

To select a variant, use the Q# configuration `"optimize"` with the value `"space"` or
`"time"`. For example, when creating a QDK Context using the Python API:
`qdk.Context(..., qdk_config={"optimize": "space"})`.

The variant used by default is unspecified (but it is usually the space-optimized one).

