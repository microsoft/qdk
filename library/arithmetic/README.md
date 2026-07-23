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
  * `Modular/ModAdd.qs` - quantum-quantum in-place addition modulo classical constant.
  * `Modular/ModDiv.qs` - division of two quantum numbers modulo classical constant.
    This is based on the extended Euclidean algorithm, so some restrictions apply (in
    particular, the divisor must be mutually prime with the modulus). It can also be used
    for modular multiplication and modular inversion.
  * `Modular/ModMul.qs` - modular multiplication and square.
  * `Modular/ModNegate.qs` - modular negation.
  * `Modular/WindowModExp.qs` - modular exponentiation (computes `t:=(t*b^x)%m` where
    `t`, `x` are quantum and `b`, `m` are classical).

### Qubit-optimized and gate-optimized variants

Some algorithms (addition, constant addition, comparison) are implemented with two
different circuit variants: gate-optimized and qubit-optimized. They are functionally
equivalent, but have different resource usage.

To specify which version to use, use Q# configuration `minimize_qubits`.
For example, when creating a QDK Context using the Python API:
 `qdk.Context(..., qsharp_config={"minimize_qubits": True})`.

By default, qubit-optimized versions are used.

