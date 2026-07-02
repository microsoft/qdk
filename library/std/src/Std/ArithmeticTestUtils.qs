// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.


/// # Summary
/// Encodes a non-negative integer into a qubit register using a
/// little-endian binary representation.
///
/// # Description
/// Interprets `val` as an unsigned little-endian integer and flips the
/// qubits of `reg` whose corresponding bit is set, so that the register
/// ends up in the computational basis state |`val`⟩. The register is
/// assumed to start in the all-zeros state |0⟩; applying the operation to
/// a register that is not in the all-zeros state instead performs a
/// bitwise XOR of `val` into the register.
///
/// In the little-endian convention `reg[0]` holds the least significant
/// bit. Only the low `Length(reg)` bits of `val` are written; any higher
/// bits are silently ignored.
///
/// # Input
/// ## val
/// The non-negative integer to encode.
/// ## reg
/// The little-endian qubit register to write into. Expected to be in the
/// |0⟩ state.
operation ApplyBigInt(val : BigInt, reg : Qubit[]) : Unit is Adj + Ctl {
    let bits = Std.Convert.BigIntAsBoolArray(val, Length(reg));
    ApplyPauliFromBitString(PauliX, true, bits, reg);
}

/// # Summary
/// Measures a qubit register in the computational basis and returns its
/// content as an unsigned little-endian integer.
///
/// # Description
/// Measures every qubit of `reg` in the Pauli Z basis and interprets the
/// results as an unsigned little-endian integer, where `reg[0]` holds the
/// least significant bit. Each qubit is reset to the |0⟩ state after the
/// measurement, so the register can be safely released or reused.
///
/// # Input
/// ## reg
/// The little-endian qubit register to measure. Reset to the |0⟩ state on
/// return.
///
/// # Output
/// The integer value encoded in `reg`, in the range `0 .. 2^Length(reg) - 1`.
operation MeasureBigInt(reg : Qubit[]) : BigInt {
    let result = Std.Measurement.MResetEachZ(reg);
    Std.Convert.BoolArrayAsBigInt(Std.Convert.ResultArrayAsBoolArray(result))
}

/// # Summary
/// Runs an arithmetic operation on classical inputs and returns the
/// resulting classical outputs.
///
/// # Description
/// Evaluates an arithmetic `op` that acts on a collection of qubit
/// registers holding unsigned little-endian integers. The operation:
/// 1. Allocates one register for each entry of `sizes`, where `sizes[i]`
///    is the number of qubits in the i-th register.
/// 2. Initializes the i-th register to the value `vals[i]` using
///    `ApplyBigInt`.
/// 3. Applies `op` to the array of registers.
/// 4. Measures every register with `MeasureBigInt` and returns the values.
///
/// This is a convenience harness for testing in-place arithmetic
/// operations against their expected classical behavior without manually
/// allocating, initializing, and measuring registers.
///
/// # Input
/// ## op
/// The arithmetic operation under test. It receives the registers as a
/// jagged array, in the same order as `sizes` and `vals`, and is expected
/// to transform their contents in place.
/// ## sizes
/// The number of qubits to allocate for each register.
/// ## vals
/// The initial integer value for each register. Must have the same length
/// as `sizes`, and `vals[i]` must fit in `sizes[i]` qubits.
///
/// # Output
/// The integer values held by the registers after `op` has been applied,
/// in the same order as the inputs.
operation TestArithmeticOp(
    op : (Qubit[][]) => Unit,
    sizes : Int[],
    vals : BigInt[]
) : BigInt[] {
    Std.Diagnostics.Fact(Length(sizes) == Length(vals), "sizes and vals must have the same length.");
    let n = Length(sizes);
    mutable total = 0;
    for sz in sizes {
        set total += sz;
    }
    use allQubits = Qubit[total];
    mutable regs : Qubit[][] = [];
    mutable offset = 0;
    for sz in sizes {
        set regs += [allQubits[offset..offset + sz - 1]];
        set offset += sz;
    }
    Std.Arrays.ForEach(Std.ArithmeticTestUtils.ApplyBigInt, Std.Arrays.Zipped(vals, regs));

    op(regs);

    Std.Arrays.ForEach(Std.ArithmeticTestUtils.MeasureBigInt, regs)
}


operation ApplyClassicalFunctionInternal(f : (BigInt) -> BigInt, target : Qubit[]) : Unit {
    body intrinsic;
}

/// # Summary
/// Applies an arbitrary classical function to a little-endian register as
/// a unitary, by directly permuting the simulator's state vector.
///
/// # Description
/// Treats the content of `target` as an unsigned little-endian integer `x`
/// and replaces every basis state |`x`⟩ with |`f(x)`⟩. This is implemented
/// by permuting amplitudes inside the simulator and does not correspond to
/// any concrete gate sequence, so it is intended only as a testing and
/// specification aid.
///
/// Because the transformation is realized as a permutation of basis
/// states, it is unitary if and only if `f` is a bijection on
/// `0 .. 2^Length(target) - 1`. It is the caller's responsibility to
/// ensure that `f` is injective on the reachable values. A collision may
/// be detected and reported as an error, but only when two basis states
/// with non-zero amplitude are mapped to the same value; non-bijective
/// functions are otherwise silently mishandled.
///
/// The controlled variant applies `f` only when all control qubits are in
/// the |1⟩ state and acts as the identity otherwise.
///
/// # Input
/// ## f
/// The classical function to apply. Must map every input in
/// `0 .. 2^Length(target) - 1` to an output in the same range, and must be
/// injective over those inputs.
/// ## target
/// The little-endian qubit register to transform.
///
/// # Remarks
/// Supported only on the sparse simulator.
operation ApplyClassicalFunction(f : (BigInt) -> BigInt, target : Qubit[]) : Unit is Ctl {
    body (...) {
        ApplyClassicalFunctionInternal(f, target);
    }
    controlled (ctls, ...) {
        // Apply f(x) only when all control bits are 1.
        // Otherwise, apply identity function.
        let limit = (2L^Length(target)) * (2L^Length(ctls)-1L);
        let f2 : (BigInt) -> BigInt = x -> (if (x < limit) { x } else { f(x - limit) + limit });
        ApplyClassicalFunctionInternal(f2, target + ctls);
    }
}

/// # Summary
/// Applies an arbitrary classical function jointly to two little-endian
/// registers as a unitary.
///
/// # Description
/// Treats the contents of `reg1` and `reg2` as unsigned little-endian
/// integers `x` and `y` and replaces every basis state |`x`⟩|`y`⟩ with
/// |`x'`⟩|`y'`⟩, where `(x', y') = f(x, y)`. The two registers are packed
/// into a single combined register (`reg1` in the low bits, `reg2` in the
/// high bits) and transformed via `ApplyClassicalFunction`.
///
/// # Input
/// ## f
/// The classical function to apply. The pair `(x, y)` must be mapped
/// bijectively to a pair `(x', y')` with `x'` in
/// `0 .. 2^Length(reg1) - 1` and `y'` in `0 .. 2^Length(reg2) - 1`.
/// ## reg1
/// The little-endian register holding the first argument.
/// ## reg2
/// The little-endian register holding the second argument.
///
/// # Remarks
/// Supported only on the sparse simulator. All caveats of
/// `ApplyClassicalFunction` regarding injectivity and output range apply here.
operation ApplyClassicalFunction2(f : (BigInt, BigInt) -> (BigInt, BigInt), reg1 : Qubit[], reg2 : Qubit[]) : Unit is Ctl {
    let n1 = Length(reg1);
    let n2 = Length(reg2);
    let mask1 = (1L <<< n1) - 1L;
    let mask2 = (1L <<< n2) - 1L;
    let f2 : (BigInt) -> BigInt = x -> {
        let (ans1, ans2) = f(x &&& mask1, x >>> n1);
        return (ans1 &&& mask1) + ((ans2 &&& mask2) <<< n1);
    };
    ApplyClassicalFunction(f2, reg1 + reg2);
}

/// # Summary
/// Applies an arbitrary classical function jointly to three little-endian
/// registers as a unitary.
///
/// # Description
/// Treats the contents of `reg1`, `reg2`, and `reg3` as unsigned
/// little-endian integers `x`, `y`, and `z` and replaces every basis state
/// |`x`⟩|`y`⟩|`z`⟩ with |`x'`⟩|`y'`⟩|`z'`⟩, where
/// `(x', y', z') = f(x, y, z)`. The three registers are packed into a
/// single combined register (`reg1` in the lowest bits, then `reg2`, then
/// `reg3` in the highest bits) and transformed via `ApplyClassicalFunction`.
///
/// # Input
/// ## f
/// The classical function to apply. The triple `(x, y, z)` must be mapped
/// bijectively to a triple `(x', y', z')` with each component in the range
/// of its corresponding register.
/// ## reg1
/// The little-endian register holding the first argument.
/// ## reg2
/// The little-endian register holding the second argument.
/// ## reg3
/// The little-endian register holding the third argument.
///
/// # Remarks
/// Supported only on the sparse simulator. All caveats of
/// `ApplyClassicalFunction` regarding injectivity and output range apply here.
operation ApplyClassicalFunction3(f : (BigInt, BigInt, BigInt) -> (BigInt, BigInt, BigInt), reg1 : Qubit[], reg2 : Qubit[], reg3 : Qubit[]) : Unit is Ctl {
    let n1 = Length(reg1);
    let n2 = Length(reg2);
    let n3 = Length(reg3);
    let mask1 = (1L <<< n1) - 1L;
    let mask2 = (1L <<< n2) - 1L;
    let mask3 = (1L <<< n3) - 1L;
    let f3 : (BigInt) -> BigInt = x -> {
        let x1 = x &&& mask1;
        let x2 = (x >>> n1) &&& mask2;
        let x3 = (x >>> (n1 + n2)) &&& mask3;
        let (ans1, ans2, ans3) = f(x1, x2, x3);
        return (ans1 &&& mask1) + ((ans2 &&& mask2) <<< n1) + ((ans3 &&& mask3) <<< (n1 + n2));
    };
    ApplyClassicalFunction(f3, reg1 + reg2 + reg3);
}

export ApplyBigInt, MeasureBigInt, TestArithmeticOp, ApplyClassicalFunction, ApplyClassicalFunction2, ApplyClassicalFunction3;