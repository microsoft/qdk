// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import AddConst.AddConstant;
import Utils.CheckIfAllOnes;
import Utils.CheckIfAllZero;


/// # Summary
/// Reversible, in-place modular negation of an integer modulo a constant
/// integer modulus. Given an $n$-bit integer $x$ encoded in a little-endian
/// register `xs` and a constant integer `modulus`, this operation computes
/// $-x \bmod m$. The result is held in register `xs`.
///
/// # Input
/// ## modulus
/// Constant integer modulus.
/// ## xs
/// Qubit register encoding the integer `x`; replaced with `-x mod modulus`.
///
/// # Reference
/// - Daniel Litinski, "How to compute a 256-bit elliptic curve private key with only
///   50 million Toffoli gates", 2023, https://arxiv.org/pdf/2306.08585, fig. 6b.
operation ModNegate(xs : Qubit[], modulus : BigInt) : Unit is Adj + Ctl {
    body (...) {
        (Controlled ModNegate)([], (xs, modulus));
    }
    controlled (controls, ...) {
        let negModulus = (1L <<< Length(xs)) - modulus - 1L;
        use isAllZeros = Qubit();
        (Controlled CheckIfAllZero)(controls, (xs, isAllZeros));
        (Controlled ApplyXorInPlaceL)([isAllZeros], (modulus, xs));
        (Controlled AddConstant)(controls, (negModulus, xs));
        CheckIfAllOnes(controls + xs, isAllZeros);
        (Controlled ApplyToEachCA)(controls, (X, xs));
    }
}

export ModNegate;
