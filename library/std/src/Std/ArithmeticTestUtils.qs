// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Arrays.*;
import Std.Diagnostics.Fact;
import Std.Measurement.MeasureBigInt;

/// # Summary
/// Runs an arithmetic operation on classical inputs and returns the
/// resulting classical outputs.
///
/// # Description
/// Evaluates an arithmetic `op` that acts on a collection of qubit
/// registers holding unsigned little-endian integers. The operation:
/// 1. Allocates one register for each entry of `widths`, where `widths[i]`
///    is the number of qubits in the i-th register.
/// 2. Initializes the i-th register to the value `vals[i]` using
///    `ApplyXorInPlaceL`.
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
/// jagged array, in the same order as `widths` and `vals`, and is expected
/// to transform their contents in place.
/// ## widths
/// The number of qubits to allocate for each register.
/// ## vals
/// The initial integer value for each register. Must have the same length
/// as `widths`, and `vals[i]` must fit in `widths[i]` qubits.
///
/// # Output
/// The integer values held by the registers after `op` has been applied,
/// in the same order as the inputs.
@Config(Unrestricted)
operation TestArithmeticOp(
    op : (Qubit[][]) => Unit,
    widths : Int[],
    vals : BigInt[]
) : BigInt[] {
    Fact(Length(widths) == Length(vals), "widths and vals must have the same length.");
    let total_size = Fold((acc, sz) -> acc + sz, 0, widths);
    use allQubits = Qubit[total_size];
    let regs = Most(Partitioned(widths, allQubits));
    ForEach(ApplyXorInPlaceL, Zipped(vals, regs));
    op(regs);
    ForEach(MeasureBigInt, regs)
}

/// Returns a number consisting of `n` ones in binary representation.
function MaskNOnes(n : Int) : BigInt {
    (1L <<< n) - 1L
}

// Packs little-endian integers into one BigInt using the provided bit widths.
function PackInts(args : BigInt[], widths : Int[]) : BigInt {
    mutable packed = 0L;
    mutable shift = 0;
    for (arg, width) in Zipped(args, widths) {
        set packed += (arg &&& MaskNOnes(width)) <<< shift;
        set shift += width;
    }
    packed
}

// Unpacks one BigInt into little-endian integers using the provided bit widths.
function UnpackInts(packed_args : BigInt, widths : Int[]) : BigInt[] {
    mutable args = [];
    mutable shift = 0;
    for width in widths {
        set args += [(packed_args >>> shift) &&& MaskNOnes(width)];
        set shift += width;
    }
    args
}

// Uncontrolled version of ApplyClassicalFunction, implemented natively.
@Config(Unrestricted)
operation ApplyClassicalFunctionInternal(f : (BigInt) -> BigInt, target : Qubit[]) : Unit {
    body intrinsic;
}


/// # Summary
/// Applies an arbitrary bijective classical function to a little-endian
/// register as a unitary by directly permuting the simulator's state vector.
/// Supported only on the sparse simulator.
///
/// # Description
/// Treats the contents of `target` as an unsigned little-endian integer `x`
/// and replaces every basis state |`x`⟩ with |`f(x)`⟩. This is implemented
/// by permuting amplitudes inside the simulator and does not correspond to
/// any concrete gate sequence, so it is intended only as a testing and
/// specification aid.
///
/// Because the transformation is realized as a permutation of basis
/// states, it is unitary if and only if `f` is a bijection on
/// `0 .. 2^Length(target) - 1`. It is the caller's responsibility to
/// ensure that `f` is bijective on the reachable values. A collision may
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
/// bijective on those inputs.
/// ## target
/// The little-endian qubit register to transform.
@Config(Unrestricted)
operation ApplyClassicalFunction(f : (BigInt) -> BigInt, target : Qubit[]) : Unit is Ctl {
    body (...) {
        ApplyClassicalFunctionInternal(f, target);
    }
    controlled (ctls, ...) {
        // Apply f(x) only when all control bits are 1.
        // Otherwise, apply identity function.
        let limit = MaskNOnes(Length(ctls)) <<< Length(target);
        let f2 : (BigInt) -> BigInt = x -> (if (x < limit) { x } else { f(x - limit) + limit });
        ApplyClassicalFunctionInternal(f2, target + ctls);
    }
}

/// # Summary
/// Applies a bijective classical function (taking `N` arguments and
/// returning `N` values) to `N` little-endian registers.
///
/// Supported only on the sparse simulator. All caveats of
/// `ApplyClassicalFunction` regarding bijectivity and output range apply here.
///
/// # Description
/// See `ApplyClassicalFunction` for implementation details.
///
/// # Input
/// ## f
/// The classical function to apply to the tuple of register values. The
/// function receives one integer per register and must return one integer
/// per register.
/// ## regs
/// The little-endian registers that hold the input values and are updated
/// in place.
@Config(Unrestricted)
operation ApplyClassicalFunctionN(f : (BigInt[]) -> (BigInt[]), regs : Qubit[][]) : Unit is Ctl {
    let widths = Mapped(Length, regs);
    ApplyClassicalFunction(x -> PackInts(f(UnpackInts(x, widths)), widths), Flattened(regs));
}

export TestArithmeticOp, ApplyClassicalFunction, ApplyClassicalFunctionN;
