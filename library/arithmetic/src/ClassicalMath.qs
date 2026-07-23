// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Classical mathematical functions, used in arithmetic algorithms.

import Std.Math.Log;
import Std.Math.LogOf2;

/// Computes x%mod with result in [0, mod).
function SafeMod(x : BigInt, mod : BigInt) : BigInt {
    let remainder = x % mod;
    return remainder < 0L ? remainder + mod | remainder;
}

/// Computes ⌈a/b⌉.
function DivCeil(a : Int, b : Int) : Int {
    return ((a + b - 1) / b);
}

/// Computes base-2 logarithm of x.
function Log2(x : Double) : Double {
    return Log(x) / LogOf2();
}

/// Computes [(a^(2^i))%N for i in 0..n-1].
function ComputeSequentialSquares(a : BigInt, N : BigInt, n : Int) : BigInt[] {
    mutable ans : BigInt[] = [((a % N) + N) % N];
    for i in 1..n-1 {
        set ans += [(ans[i-1] * ans[i-1]) % N];
    }
    return ans;
}

export SafeMod, DivCeil, Log2, ComputeSequentialSquares;
