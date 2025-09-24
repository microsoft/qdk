// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.


import Std.Convert.*;
import Std.Diagnostics.*;

//
// Constants PI, E, LogOf2.
//

/// # Summary
/// Returns a double-precision approximation of the
/// mathematical constant 𝝅 ≈ 3.14159265358979323846
///
/// # Remarks
/// Mathematical constant 𝝅 represents the ratio of the circumference
/// of a circle to its diameter. It is useful in many applications
/// such as rotations and complex arithmetic.
///
/// # References
/// [Wikipedia article - Pi](https://en.wikipedia.org/wiki/Pi)
///
/// # See Also
/// - [Std.Math.E](xref:Qdk.Std.Math.E)
function PI() : Double {
    3.14159265358979323846
}

/// # Summary
/// Returns a double-precision approximation of the
/// mathematical constant 𝒆 ≈ 2.7182818284590452354
///
/// # Remarks
/// Mathematical constant 𝒆 is the base of the natural logarithm
/// also known as the Euler's number
///
/// # References
/// [Wikipedia article - e](https://en.wikipedia.org/wiki/E_(mathematical_constant))
///
/// # See Also
/// - [Std.Math.PI](xref:Qdk.Std.Math.PI)
function E() : Double {
    2.7182818284590452354
}

/// # Summary
/// Returns a double-precision approximation of the constant
/// ㏑2 ≈ 0.6931471805599453
///
/// # Remarks
/// ㏑2 is the natural logarithm of 2, or the logarithm of 2 base 𝒆.
///
/// # References
/// [Wikipedia article - Natural logarithm](https://en.wikipedia.org/wiki/Natural_logarithm)
function LogOf2() : Double {
    0.6931471805599453
}

//
// Special numbers in IEEE floating-point representation
//

/// # Summary
/// Returns whether a given floating-point value is not a number (i.e. is
/// NaN).
///
/// # Input
/// ## d
/// A floating-point value to be checked.
///
/// # Output
/// `true` if and only if `d` is not a number.
function IsNaN(d : Double) : Bool {
    return d != d;
}

/// # Summary
/// Returns whether a given floating-point value is either positive or
/// negative infinity.
///
/// # Input
/// ## d
/// The floating-point value to be checked.
///
/// # Output
/// `true` if and only if `d` is either positive or negative infinity.
///
/// # Remarks
/// `NaN` is not a number, and is thus neither a finite number nor
/// is it infinite. As such, `IsInfinite(0.0 / 0.0)` returns `false`.
/// To check if a value is `NaN`, use `IsNaN(d)`.
///
/// Note that even though this function returns `true` for both
/// positive and negative infinities, these values can still be
/// discriminated by checking `d > 0.0` and `d < 0.0`.
///
/// # Example
/// ```qsharp
/// Message($"{IsInfinite(42.0)}"); // false
/// Message($"{IsInfinite(0.0 / 0.0)}"); // false
/// Message($"{IsInfinite(-1.0 / 0.0}"); // true
/// ```
///
/// # See Also
/// - [Std.Math.IsNaN](xref:Qdk.Std.Math.IsNaN)
function IsInfinite(d : Double) : Bool {
    return d == 1.0 / 0.0 or d == -1.0 / 0.0;
}

//
// Sign, Abs, Min, Max, etc.
//

/// # Summary
/// Returns -1, 0 or +1 that indicates the sign of a number.
function SignI(a : Int) : Int {
    if (a < 0) {
            -1
    } elif (a > 0) {
        + 1
    } else {
        0
    }
}

/// # Summary
/// Returns -1, 0 or +1 that indicates the sign of a number.
function SignD(a : Double) : Int {
    if (a < 0.0) {
            -1
    } elif (a > 0.0) {
        + 1
    } else {
        0
    }
}

/// # Summary
/// Returns -1, 0 or +1 that indicates the sign of a number.
function SignL(a : BigInt) : Int {
    if (a < 0L) {
            -1
    } elif (a > 0L) {
        + 1
    } else {
        0
    }
}

/// # Summary
/// Returns the absolute value of an integer.
function AbsI(a : Int) : Int {
    a < 0 ? -a | a
}

/// # Summary
/// Returns the absolute value of a double-precision floating-point number.
function AbsD(a : Double) : Double {
    a < 0.0 ? -a | a
}

/// # Summary
/// Returns the absolute value of a big integer.
function AbsL(a : BigInt) : BigInt {
    a < 0L ? -a | a
}

/// # Summary
/// Returns the larger of two specified numbers.
function MaxI(a : Int, b : Int) : Int {
    a > b ? a | b
}

/// # Summary
/// Returns the larger of two specified numbers.
function MaxD(a : Double, b : Double) : Double {
    a > b ? a | b
}

/// # Summary
/// Returns the larger of two specified numbers.
function MaxL(a : BigInt, b : BigInt) : BigInt {
    a > b ? a | b
}

/// # Summary
/// Returns the smaller of two specified numbers.
function MinI(a : Int, b : Int) : Int {
    a < b ? a | b
}

/// # Summary
/// Returns the smaller of two specified numbers.
function MinD(a : Double, b : Double) : Double {
    a < b ? a | b
}

/// # Summary
/// Returns the smaller of two specified numbers.
function MinL(a : BigInt, b : BigInt) : BigInt {
    a < b ? a | b
}

/// # Summary
/// Given an array of integers, returns the largest element.
///
/// # Input
/// ## values
/// An array to take the maximum of.
///
/// # Output
/// The largest element of `values`.
function Max(values : Int[]) : Int {
    Fact(Length(values) > 0, "Array must contain at least one element.");
    mutable max = values[0];
    for element in values[1...] {
        if element > max {
            set max = element;
        }
    }

    max
}

/// # Summary
/// Given an array of integers, returns the smallest element.
///
/// # Input
/// ## values
/// An array to take the minimum of.
///
/// # Output
/// The smallest element of `values`.
function Min(values : Int[]) : Int {
    Fact(Length(values) > 0, "Array must contain at least one element.");
    mutable min = values[0];
    for element in values[1...] {
        if element < min {
            set min = element;
        }
    }

    min
}

//
// Trigonometric functions
//

/// # Summary
/// Returns the angle whose cosine is the specified number.
function ArcCos(x : Double) : Double {
    body intrinsic;
}

/// # Summary
/// Returns the angle whose sine is the specified number.
function ArcSin(y : Double) : Double {
    body intrinsic;
}

/// # Summary
/// Returns the angle whose tangent is the specified number.
function ArcTan(d : Double) : Double {
    body intrinsic;
}

/// # Summary
/// Returns the angle whose tangent is the quotient of two specified numbers.
function ArcTan2(y : Double, x : Double) : Double {
    body intrinsic;
}

/// # Summary
/// Returns the cosine of the specified angle.
function Cos(theta : Double) : Double {
    body intrinsic;
}

/// # Summary
/// Returns the hyperbolic cosine of the specified angle.
function Cosh(d : Double) : Double {
    body intrinsic;
}

/// # Summary
/// Returns the sine of the specified angle.
function Sin(theta : Double) : Double {
    body intrinsic;
}

/// # Summary
/// Returns the hyperbolic sine of the specified angle.
function Sinh(d : Double) : Double {
    body intrinsic;
}

/// # Summary
/// Returns the tangent of the specified angle.
function Tan(d : Double) : Double {
    body intrinsic;
}

/// # Summary
/// Returns the hyperbolic tangent of the specified angle.
function Tanh(d : Double) : Double {
    body intrinsic;
}

/// # Summary
/// Computes the inverse hyperbolic cosine of a number.
function ArcCosh(x : Double) : Double {
    Log(x + Sqrt(x * x - 1.0))
}

/// # Summary
/// Computes the inverse hyperbolic sine of a number.
function ArcSinh(x : Double) : Double {
    Log(x + Sqrt(x * x + 1.0))
}


/// # Summary
/// Computes the inverse hyperbolic tangent of a number.
function ArcTanh(x : Double) : Double {
    Log((1.0 + x) / (1.0 - x)) * 0.5
}

//
// Sqrt, Log, exp, etc.
//

/// # Summary
/// Returns the square root of a specified number.
function Sqrt(d : Double) : Double {
    body intrinsic;
}

/// # Summary
/// Returns the natural (base _e_) logarithm of a specified number.
function Log(input : Double) : Double {
    body intrinsic;
}

/// # Summary
/// Returns the base-10 logarithm of a specified number.
function Log10(input : Double) : Double {
    Log(input) / Log(10.0)
}

/// # Summary
/// Computes the base-2 logarithm of a number.
function Lg(input : Double) : Double {
    Log(input) / Log(2.0)
}

//
// Truncation and Rounding
//

/// # Summary
/// Returns the integral part of a number.
/// For example: Truncate(3.7) = 3; Truncate(-3.7) = -3
function Truncate(value : Double) : Int {
    body intrinsic;
}

internal function ExtendedTruncation(value : Double) : (Int, Double, Bool) {
    let truncated = Truncate(value);
    (truncated, IntAsDouble(truncated) - value, value >= 0.0)
}

/// # Summary
/// Returns the smallest integer greater than or equal to the specified number.
/// For example: Ceiling(3.1) = 4; Ceiling(-3.7) = -3
function Ceiling(value : Double) : Int {
    let (truncated, remainder, isPositive) = ExtendedTruncation(value);
    if AbsD(remainder) <= 1e-15 {
        truncated
    } else {
        isPositive ? truncated + 1 | truncated
    }
}

/// # Summary
/// Returns the largest integer less than or equal to the specified number.
/// For example: Floor(3.7) = 3; Floor(-3.1) = -4
function Floor(value : Double) : Int {
    let (truncated, remainder, isPositive) = ExtendedTruncation(value);
    if AbsD(remainder) <= 1e-15 {
        truncated
    } else {
        isPositive ? truncated | truncated - 1
    }
}

/// # Summary
/// Returns the nearest integer to the specified number. Half is rounded towards zero.
/// For example: Round(3.7) = 4; Round(-3.7) = -4; Round(3.5) = 3;
///
/// # References
/// [Wikipedia article - Rounding](https://en.wikipedia.org/wiki/Rounding#Rounding_half_toward_zero)
function Round(value : Double) : Int {
    let (truncated, remainder, isPositive) = ExtendedTruncation(value);
    let abs = AbsD(remainder);
    truncated + (abs <= 0.5 ? 0 | (isPositive ? 1 | -1))
}

/// # Summary
/// Returns the nearest integer to the specified number. Half is rounded away from zero.
/// For example: RoundHalfAwayFromZero(-3.7) = -4, RoundHalfAwayFromZero(3.5) = 4;
///
/// # References
/// [Wikipedia article - Rounding](https://en.wikipedia.org/wiki/Rounding#Rounding_half_away_from_zero)
function RoundHalfAwayFromZero(value : Double) : Int {
    let (truncated, remainder, isPositive) = ExtendedTruncation(value);
    let abs = AbsD(remainder);
    truncated + (abs < 0.5 ? 0 | (isPositive ? 1 | -1))
}

//
// Modular arithmetic
//

/// # Summary
/// Divides one Integer value by another, returns the result and the remainder as a tuple.
function DivRemI(dividend : Int, divisor : Int) : (Int, Int) {
    (dividend / divisor, dividend % divisor)
}

/// # Summary
/// Divides one BigInteger value by another, returns the result and the remainder as a tuple.
function DivRemL(dividend : BigInt, divisor : BigInt) : (BigInt, BigInt) {
    (dividend / divisor, dividend % divisor)
}

/// # Summary
/// Computes the canonical residue of `value` modulo `modulus`.
/// The result is always in the range 0..modulus-1 even for negative numbers.
function ModulusI(value : Int, modulus : Int) : Int {
    Fact(modulus > 0, "`modulus` must be positive");
    let r = value % modulus;
    (r < 0) ? (r + modulus) | r
}

/// # Summary
/// Computes the canonical residue of `value` modulo `modulus`.
/// The result is always in the range 0..modulus-1 even for negative numbers.
function ModulusL(value : BigInt, modulus : BigInt) : BigInt {
    Fact(modulus > 0L, "`modulus` must be positive");
    let r = value % modulus;
    (r < 0L) ? (r + modulus) | r
}

/// # Summary
/// Returns an integer raised to a given power, with respect to a given
/// modulus. I.e. (expBase^power) % modulus.
function ExpModI(expBase : Int, power : Int, modulus : Int) : Int {
    Fact(power >= 0, "`power` must be non-negative");
    Fact(modulus > 0, "`modulus` must be positive");
    Fact(expBase > 0, "`expBase` must be positive");

    // shortcut when modulus is 1
    if modulus == 1 {
        return 0;
    }

    mutable res = 1;
    mutable expPow2mod = expBase % modulus;
    mutable powerBits = power;

    while powerBits > 0 {
        if (powerBits &&& 1) != 0 {
            // if bit pₖ is 1, multiply res by expBase^(2^k) (mod `modulus`)
            set res = (res * expPow2mod) % modulus;
        }

        // update value of expBase^(2^k) (mod `modulus`)
        set expPow2mod = (expPow2mod * expPow2mod) % modulus;
        set powerBits >>>= 1;
    }

    res
}

/// # Summary
/// Returns an integer raised to a given power, with respect to a given
/// modulus. I.e. (expBase^power) % modulus.
function ExpModL(expBase : BigInt, power : BigInt, modulus : BigInt) : BigInt {
    Fact(power >= 0L, "`power` must be non-negative");
    Fact(modulus > 0L, "`modulus` must be positive");
    Fact(expBase > 0L, "`expBase` must be positive");

    // shortcut when modulus is 1
    if modulus == 1L {
        return 0L;
    }

    mutable res = 1L;
    mutable expPow2mod = expBase % modulus;
    mutable powerBits = power;

    while powerBits > 0L {
        if (powerBits &&& 1L) != 0L {
            // if bit pₖ is 1, multiply res by expBase^(2ᵏ) (mod `modulus`)
            set res = (res * expPow2mod) % modulus;
        }

        // update value of expBase^(2ᵏ) (mod `modulus`)
        set expPow2mod = (expPow2mod * expPow2mod) % modulus;
        set powerBits >>>= 1;
    }

    res
}

/// # Summary
/// Returns the multiplicative inverse of a modular integer.
///
/// # Description
/// This will calculate the multiplicative inverse of a
/// modular integer `b` such that `a • b = 1 (mod modulus)`.
function InverseModI(a : Int, modulus : Int) : Int {
    let (u, v) = ExtendedGreatestCommonDivisorI(a, modulus);
    let gcd = u * a + v * modulus;
    Fact(gcd == 1, "`a` and `modulus` must be co-prime");
    ModulusI(u, modulus)
}

/// # Summary
/// Returns the multiplicative inverse of a modular integer.
///
/// # Description
/// This will calculate the multiplicative inverse of a
/// modular integer `b` such that `a • b = 1 (mod modulus)`.
function InverseModL(a : BigInt, modulus : BigInt) : BigInt {
    let (u, v) = ExtendedGreatestCommonDivisorL(a, modulus);
    let gcd = u * a + v * modulus;
    Fact(gcd == 1L, "`a` and `modulus` must be co-prime");
    ModulusL(u, modulus)
}

//
// GCD, etc.
//

/// # Summary
/// Computes the greatest common divisor of two integers.
/// Note: GCD is always positive except that GCD(0,0)=0.
function GreatestCommonDivisorI(a : Int, b : Int) : Int {
    mutable aa = AbsI(a);
    mutable bb = AbsI(b);
    while bb != 0 {
        let cc = aa % bb;
        set aa = bb;
        set bb = cc;
    }
    aa
}

/// # Summary
/// Computes the greatest common divisor of two integers.
/// Note: GCD is always positive except that GCD(0,0)=0.
function GreatestCommonDivisorL(a : BigInt, b : BigInt) : BigInt {
    mutable aa = AbsL(a);
    mutable bb = AbsL(b);
    while bb != 0L {
        let cc = aa % bb;
        set aa = bb;
        set bb = cc;
    }
    aa
}

/// # Summary
/// Returns a tuple (u,v) such that u*a+v*b=GCD(a,b)
/// Note: GCD is always positive except that GCD(0,0)=0.
function ExtendedGreatestCommonDivisorI(a : Int, b : Int) : (Int, Int) {
    let signA = SignI(a);
    let signB = SignI(b);
    mutable (s1, s2) = (1, 0);
    mutable (t1, t2) = (0, 1);
    mutable (r1, r2) = (a * signA, b * signB);

    while r2 != 0 {
        let quotient = r1 / r2;
        set (r1, r2) = (r2, r1 - quotient * r2);
        set (s1, s2) = (s2, s1 - quotient * s2);
        set (t1, t2) = (t2, t1 - quotient * t2);
    }

    (s1 * signA, t1 * signB)
}

/// # Summary
/// Returns a tuple (u,v) such that u*a+v*b=GCD(a,b)
/// Note: GCD is always positive except that GCD(0,0)=0.
function ExtendedGreatestCommonDivisorL(a : BigInt, b : BigInt) : (BigInt, BigInt) {
    let signA = IntAsBigInt(SignL(a));
    let signB = IntAsBigInt(SignL(b));
    mutable (s1, s2) = (1L, 0L);
    mutable (t1, t2) = (0L, 1L);
    mutable (r1, r2) = (a * signA, b * signB);

    while r2 != 0L {
        let quotient = r1 / r2;
        set (r1, r2) = (r2, r1 - quotient * r2);
        set (s1, s2) = (s2, s1 - quotient * s2);
        set (t1, t2) = (t2, t1 - quotient * t2);
    }

    (s1 * signA, t1 * signB)
}

/// # Summary
/// Returns if two integers are co-prime.
///
/// # Description
/// Returns true if a and b are co-prime and false otherwise.
///
/// # Input
/// ## a
/// the first number of which co-primality is being tested
/// ## b
/// the second number of which co-primality is being tested
///
/// # Output
/// True, if a and b are co-prime (e.g. their greatest common divisor is 1),
/// and false otherwise
function IsCoprimeI(a : Int, b : Int) : Bool {
    GreatestCommonDivisorI(a, b) == 1
}

/// # Summary
/// Returns if two integers are co-prime.
///
/// # Description
/// Returns true if a and b are co-prime and false otherwise.
///
/// # Input
/// ## a
/// the first number of which co-primality is being tested
/// ## b
/// the second number of which co-primality is being tested
///
/// # Output
/// True, if a and b are co-prime (e.g. their greatest common divisor is 1),
/// and false otherwise
function IsCoprimeL(a : BigInt, b : BigInt) : Bool {
    GreatestCommonDivisorL(a, b) == 1L
}

/// # Summary
/// Finds the continued fraction convergent closest to `fraction`
/// with the denominator less or equal to `denominatorBound`
/// Using process similar to this: https://nrich.maths.org/1397
function ContinuedFractionConvergentI(
    fraction : (Int, Int),
    denominatorBound : Int
) : (Int, Int) {
    Fact(denominatorBound > 0, "Denominator bound must be positive");

    let (a, b) = fraction;
    let signA = SignI(a);
    let signB = SignI(b);
    mutable (s1, s2) = (1, 0);
    mutable (t1, t2) = (0, 1);
    mutable (r1, r2) = (a * signA, b * signB);

    while r2 != 0 and AbsI(s2) <= denominatorBound {
        let quotient = r1 / r2;
        set (r1, r2) = (r2, r1 - quotient * r2);
        set (s1, s2) = (s2, s1 - quotient * s2);
        set (t1, t2) = (t2, t1 - quotient * t2);
    }

    if r2 == 0 and AbsI(s2) <= denominatorBound {
        (-t2 * signB, s2 * signA)
    } else {
        (-t1 * signB, s1 * signA)
    }
}

/// # Summary
/// Finds the continued fraction convergent closest to `fraction`
/// with the denominator less or equal to `denominatorBound`
/// Using process similar to this: https://nrich.maths.org/1397
function ContinuedFractionConvergentL(
    fraction : (BigInt, BigInt),
    denominatorBound : BigInt
) : (BigInt, BigInt) {
    Fact(denominatorBound > 0L, "Denominator bound must be positive");

    let (a, b) = fraction;
    let signA = IntAsBigInt(SignL(a));
    let signB = IntAsBigInt(SignL(b));
    mutable (s1, s2) = (1L, 0L);
    mutable (t1, t2) = (0L, 1L);
    mutable (r1, r2) = (a * signA, b * signB);

    while r2 != 0L and AbsL(s2) <= denominatorBound {
        let quotient = r1 / r2;
        set (r1, r2) = (r2, r1 - quotient * r2);
        set (s1, s2) = (s2, s1 - quotient * s2);
        set (t1, t2) = (t2, t1 - quotient * t2);
    }

    if r2 == 0L and AbsL(s2) <= denominatorBound {
        (-t2 * signB, s2 * signA)
    } else {
        (-t1 * signB, s1 * signA)
    }
}

/// # Summary
/// Computes the modulus between two real numbers.
///
/// # Input
/// ## value
/// A real number x to take the modulus of.
/// ## modulo
/// A real number to take the modulus of x with respect to.
/// ## minValue
/// The smallest value to be returned by this function.
///
/// # Example
/// ```qsharp
///     // Returns 3 π / 2.
///     let y = RealMod(5.5 * PI(), 2.0 * PI(), 0.0);
///     // Returns -1.2, since +3.6 and -1.2 are 4.8 apart on the real line,
///     // which is a multiple of 2.4.
///     let z = RealMod(3.6, 2.4, -1.2);
/// ```
function RealMod(value : Double, modulo : Double, minValue : Double) : Double {
    let timesModuloInSegment = (value - minValue) / modulo;
    let fractionalPart = timesModuloInSegment - IntAsDouble(Truncate(timesModuloInSegment));
    modulo * fractionalPart + minValue
}

//
// Binary, bits, etc.
//

/// # Summary
/// For a non-negative integer `a`, returns the number of bits required to represent `a`.
/// NOTE: This function returns the smallest n such that a < 2^n.
function BitSizeI(a : Int) : Int {
    Fact(a >= 0, "`a` must be non-negative.");
    mutable number = a;
    mutable size = 0;
    while (number != 0) {
        set size = size + 1;
        set number = number >>> 1;
    }

    size
}

/// # Summary
/// For a non-negative integer `a`, returns the number of bits required to represent `a`.
/// NOTE: This function returns the smallest n such that a < 2^n.
function BitSizeL(a : BigInt) : Int {
    Fact(a >= 0L, "`a` must be non-negative.");
    mutable number = a;
    mutable size = 0;
    while (number != 0L) {
        set size = size + 1;
        set number = number >>> 1;
    }

    size
}

/// # Summary
/// For a non-zero integer `a`, returns the number of trailing zero bits
/// in the binary representation of `a`.
function TrailingZeroCountI(a : Int) : Int {
    Fact(a != 0, "TrailingZeroCountI: `a` cannot be 0.");

    mutable count = 0;
    mutable n = a;
    while n &&& 1 == 0 {
        set count += 1;
        set n >>>= 1;
    }

    count
}

/// # Summary
/// For a non-zero integer `a`, returns the number of trailing zero bits
/// in the binary representation of `a`.
function TrailingZeroCountL(a : BigInt) : Int {
    Fact(a != 0L, "TrailingZeroCountL: `a` cannot be 0.");

    mutable count = 0;
    mutable n = a;
    while n &&& 1L == 0L {
        set count += 1;
        set n >>>= 1;
    }

    count
}

/// # Summary
/// Returns the number of 1 bits in the binary representation of integer `n`.
function HammingWeightI(n : Int) : Int {
    let i1 = n - ((n >>> 1) &&& 0x5555555555555555);
    let i2 = (i1 &&& 0x3333333333333333) + ((i1 >>> 2) &&& 0x3333333333333333);
    // Multiplication may overflow. See https://github.com/microsoft/qsharp/issues/828
    (((i2 + (i2 >>> 4)) &&& 0xF0F0F0F0F0F0F0F) * 0x101010101010101) >>> 56
}

//
// Combinatorics
//

/// # Summary
/// Returns the factorial of a given number.
///
/// # Description
/// Returns the factorial of a given nonnegative integer n, where 0 ≤ n ≤ 20.
///
/// # Input
/// ## n
/// The number to take the factorial of.
///
/// # Output
/// The factorial of `n`.
///
/// # Remarks
/// For inputs greater than 20, please use `Std.Math.FactorialL`.
///
/// # See Also
/// - [Std.Math.FactorialL](xref:Qdk.Std.Math.FactorialL)
/// - [Std.Math.ApproximateFactorial](xref:Qdk.Std.Math.ApproximateFactorial)
function FactorialI(n : Int) : Int {
    Fact(n >= 0, "The factorial is not defined for negative inputs.");
    Fact(n <= 20, "The largest factorial that can be stored as an Int is 20!. Use FactorialL or ApproximateFactorial.");

    [
        1,
        1,
        2,
        6,
        24,
        120,
        720,
        5040,
        40320,
        362880,
        3628800,
        39916800,
        479001600,
        6227020800,
        87178291200,
        1307674368000,
        20922789888000,
        355687428096000,
        6402373705728000,
        121645100408832000,
        2432902008176640000
    ][n]
}

/// # Summary
/// Returns the factorial of a given number.
///
/// # Input
/// ## n
/// The number to take the factorial of.
///
/// # Output
/// The factorial of `n`.
///
/// # See Also
/// - [Std.Math.FactorialI](xref:Qdk.Std.Math.FactorialI)
/// - [Std.Math.ApproximateFactorial](xref:Qdk.Std.Math.ApproximateFactorial)
function FactorialL(n : Int) : BigInt {
    Fact(n >= 0, "The factorial is not defined for negative inputs.");

    mutable result = 1L;
    for i in 1..n {
        set result *= IntAsBigInt(i);
    }
    result
}

/// # Summary
/// Returns an approximate factorial of a given number.
///
/// # Description
/// Returns the factorial as `Double`, given an input `n`.
/// The domain of inputs for this function is `n <= 169`.
///
/// # Remarks
/// For n > 10, this function uses the Ramanujan approximation with a
/// relative error of the order of 1 / n⁵.
///
/// # Input
/// ## n
/// The number to take the approximate factorial of. Must not be negative.
///
/// # Output
/// The approximate factorial of `n`.
///
/// # See Also
/// - [Std.Math.FactorialI](xref:Qdk.Std.Math.FactorialI)
/// - [Std.Math.FactorialL](xref:Qdk.Std.Math.FactorialL)
function ApproximateFactorial(n : Int) : Double {
    Fact(n >= 0, "The factorial is not defined for negative inputs.");
    Fact(n <= 169, "The largest approximate factorial that can be stored as a Double is 169!. Use FactorialL.");

    // For small enough n, use the exact factorial instead.
    if n <= 20 {
        return IntAsDouble(FactorialI(n));
    }

    let absN = IntAsDouble(n);
    let a = Sqrt(2.0 * PI() * absN);
    let b = (absN / E())^absN;
    let c = E()^(1.0 / (12.0 * absN) - (1.0 / (360.0 * (absN^3.0))));

    a * b * c
}

/// # Summary
/// Returns the natural logarithm of the gamma function (aka the log-gamma
/// function).
///
/// # Description
/// The gamma function Γ(x) generalizes the factorial function
/// to the positive real numbers and is defined as
/// integral from 0 to ∞ of t¹⁻ˣ⋅e⁻ᵗ𝑑t
///
/// The gamma function has the property that for all positive real numbers
/// x, Γ(x + 1) = x⋅Γ(x), such that the factorial function
/// is a special case of Γ, n! = Γ(n + 1) for all natural numbers n.
///
/// # Input
/// ## x
/// The point x at which the log-gamma function is to be evaluated.
///
/// # Output
/// The value ㏑(Γ(x)).
function LogGammaD(x : Double) : Double {
    // Here, we use the approximation described in Numerical Recipes in C.
    let coefficients = [
        57.1562356658629235,
            -59.5979603554754912,
        14.1360979747417471,
            -0.491913816097620199,
        0.339946499848118887e-4,
        0.465236289270485756e-4,
            -0.983744753048795646e-4,
        0.158088703224912494e-3,
            -0.210264441724104883e-3,
        0.217439618115212643e-3,
            -0.164318106536763890e-3,
        0.844182239838527433e-4,
            -0.261908384015814087e-4,
        0.368991826595316234e-5
    ];

    Fact(x > 0.0, "Γ(x) not defined for x <= 0.");

    mutable y = x;
    let tmp = x + 5.2421875000000000;

    mutable acc = 0.99999999999999709;
    for coeff in coefficients {
        set y += 1.0;
        set acc += coeff / y;
    }

    Log(2.506628274631000 * acc / x) + ((x + 0.5) * Log(tmp) - tmp)
}

/// # Summary
/// Returns the approximate natural logarithm of the factorial of a given
/// integer.
///
/// # Input
/// ## n
/// The number to take the log-factorial of.
///
/// # Output
/// The natural logarithm of the factorial of the provided input.
///
/// # See Also
/// - [Std.Math.ApproximateFactorial](xref:Qdk.Std.Math.ApproximateFactorial)
/// - [Std.Math.FactorialI](xref:Qdk.Std.Math.FactorialI)
/// - [Std.Math.FactorialL](xref:Qdk.Std.Math.FactorialL)
function LogFactorialD(n : Int) : Double {
    LogGammaD(IntAsDouble(n) + 1.0)
}

/// # Summary
/// Returns the approximate binomial coefficient of two integers.
///
/// # Description
/// Given two integers n and k, returns the binomial coefficient
/// binom(n, k), also known as n-choose-k. Computed approximately.
///
/// # Input
/// ## n
/// The first of the two integers to compute the binomial coefficient of.
/// ## k
/// The second of the two integers to compute the binomial coefficient of.
///
/// # Output
/// The binomial coefficient n-choose-k.
function Binom(n : Int, k : Int) : Int {
    // Here, we use the approximation described in Numerical Recipes in C.
    if n < 171 {
        Floor(0.5 + ApproximateFactorial(n) / (ApproximateFactorial(k) * ApproximateFactorial(n - k)))
    } else {
        Floor(0.5 + E()^(LogFactorialD(n) - LogFactorialD(k) - LogFactorialD(n - k)))
    }
}

//
// Norms
//

/// # Summary
/// Returns the squared 2-norm of a vector.
///
/// # Description
/// Returns the squared 2-norm of a vector; that is, given an input
/// x̄, returns ∑xᵢ.
///
/// # Input
/// ## array
/// The vector whose squared 2-norm is to be returned.
///
/// # Output
/// The squared 2-norm of `array`.
function SquaredNorm(array : Double[]) : Double {
    mutable sum = 0.0;
    for element in array {
        set sum += element * element;
    }

    sum
}

/// # Summary
/// Returns the `L(p)` norm of a vector of `Double`s.
///
/// That is, given an array x of type `Double[]`, this returns the p-norm
/// |x̄|ₚ= (∑(xᵢ)ᵖ)¹ᐟᵖ.
///
/// # Input
/// ## p
/// The exponent p in the p-norm.
///
/// # Output
/// The p-norm |x̄|ₚ.
function PNorm(p : Double, array : Double[]) : Double {
    if p < 1.0 {
        fail "p must be >= 1.0";
    }

    mutable sum = 0.0;
    for element in array {
        set sum += AbsD(element)^p;
    }

    sum^(1.0 / p)
}

/// # Summary
/// Normalizes a vector of `Double`s in the `L(p)` norm.
///
/// That is, given an array x of type `Double[]`, this returns an array where
/// all elements are divided by the p-norm |x̄|ₚ.
/// Function leaves array with norm 0 unchanged.
///
/// # Input
/// ## p
/// The exponent p in the p-norm.
///
/// # Output
/// The array x normalized by the p-norm |x̄|ₚ.
///
/// # See Also
/// - PNorm
function PNormalized(p : Double, array : Double[]) : Double[] {
    let norm = PNorm(p, array);
    if (norm == 0.0) {
        return array;
    }

    mutable result = [];
    for element in array {
        set result += [element / norm];
    }

    result
}

//
// Complex numbers
//

/// # Summary
/// Represents a complex number in polar form.
/// The polar representation of a complex number is c = r⋅𝑒^(t𝑖).
///
/// # Named Items
/// ## Magnitude
/// The absolute value r>0 of c.
/// ## Argument
/// The phase t ∈ ℝ of c.
struct ComplexPolar { Magnitude : Double, Argument : Double }

/// # Summary
/// Returns the squared absolute value of a complex number of type
/// `Complex`.
///
/// # Input
/// ## input
/// Complex number c = x + y𝑖.
///
/// # Output
/// Squared absolute value |c|² = x² + y².
function AbsSquaredComplex(input : Complex) : Double {
    input.Real * input.Real + input.Imag * input.Imag
}

/// # Summary
/// Returns the absolute value of a complex number of type
/// `Complex`.
///
/// # Input
/// ## input
/// Complex number c = x + y𝑖.
///
/// # Output
/// Absolute value |c| = √(x² + y²).
function AbsComplex(input : Complex) : Double {
    Sqrt(AbsSquaredComplex(input))
}

/// # Summary
/// Returns the phase of a complex number of type
/// `Complex`.
///
/// # Input
/// ## input
/// Complex number c = x + y𝑖.
///
/// # Output
/// Phase Arg(c) = ArcTan(y,x) ∈ (-𝜋,𝜋].
function ArgComplex(input : Complex) : Double {
    ArcTan2(input.Imag, input.Real)
}

/// # Summary
/// Returns the squared absolute value of a complex number of type
/// `ComplexPolar`.
///
/// # Input
/// ## input
/// Complex number c = r⋅𝑒^(t𝑖).
///
/// # Output
/// Squared absolute value |c|² = r².
function AbsSquaredComplexPolar(input : ComplexPolar) : Double {
    input.Magnitude * input.Magnitude
}

/// # Summary
/// Returns the absolute value of a complex number of type
/// `ComplexPolar`.
///
/// # Input
/// ## input
/// Complex number c = r⋅𝑒^(t𝑖).
///
/// # Output
/// Absolute value |c| = r.
function AbsComplexPolar(input : ComplexPolar) : Double { input.Magnitude }

/// # Summary
/// Returns the phase of a complex number of type `ComplexPolar`.
///
/// # Input
/// ## input
/// Complex number c = r⋅𝑒^(t𝑖).
///
/// # Output
/// Phase Arg(c) = t.
function ArgComplexPolar(input : ComplexPolar) : Double { input.Argument }

/// # Summary
/// Returns the unary negation of an input of type `Complex`.
///
/// # Input
/// ## input
/// A value whose negation is to be returned.
///
/// # Output
/// The unary negation of `input`.
function NegationC(input : Complex) : Complex {
    -input
}

/// # Summary
/// Returns the unary negation of an input of type `ComplexPolar`
///
/// # Input
/// ## input
/// A value whose negation is to be returned.
///
/// # Output
/// The unary negation of `input`.
function NegationCP(input : ComplexPolar) : ComplexPolar {
    ComplexPolar(input.Magnitude, input.Argument + PI())
}

/// # Summary
/// Returns the sum of two inputs of type `Complex`.
///
/// # Input
/// ## a
/// The first input a to be summed.
/// ## b
/// The second input b to be summed.
///
/// # Output
/// The sum a + b.
function PlusC(a : Complex, b : Complex) : Complex {
    a + b
}

/// # Summary
/// Returns the sum of two inputs of type `ComplexPolar`.
///
/// # Input
/// ## a
/// The first input a to be summed.
/// ## b
/// The second input b to be summed.
///
/// # Output
/// The sum a + b.
function PlusCP(a : ComplexPolar, b : ComplexPolar) : ComplexPolar {
    ComplexAsComplexPolar(ComplexPolarAsComplex(a) + ComplexPolarAsComplex(b))
}

/// # Summary
/// Returns the difference between two inputs of type `Complex`.
///
/// # Input
/// ## a
/// The first input a to be subtracted.
/// ## b
/// The second input b to be subtracted.
///
/// # Output
/// The difference a - b.
function MinusC(a : Complex, b : Complex) : Complex {
    a - b
}

/// # Summary
/// Returns the difference between two inputs of type `ComplexPolar`.
///
/// # Input
/// ## a
/// The first input a to be subtracted.
/// ## b
/// The second input b to be subtracted.
///
/// # Output
/// The difference a - b.
function MinusCP(a : ComplexPolar, b : ComplexPolar) : ComplexPolar {
    PlusCP(a, NegationCP(b))
}

/// # Summary
/// Returns the product of two inputs of type `Complex`.
///
/// # Input
/// ## a
/// The first input a to be multiplied.
/// ## b
/// The second input b to be multiplied.
///
/// # Output
/// The product a⋅b.
function TimesC(a : Complex, b : Complex) : Complex {
    a * b
}

/// # Summary
/// Returns the product of two inputs of type `ComplexPolar`.
///
/// # Input
/// ## a
/// The first input a to be multiplied.
/// ## b
/// The second input b to be multiplied.
///
/// # Output
/// The product a⋅b.
function TimesCP(a : ComplexPolar, b : ComplexPolar) : ComplexPolar {
    ComplexPolar(
        a.Magnitude * b.Magnitude,
        a.Argument + b.Argument
    )
}

/// # Summary
/// Returns a number raised to a given power of type `Complex`.
/// Note that this is a multi-valued function, but only one value is returned.
///
/// # Input
/// ## a
/// The number a that is to be raised.
/// ## power
/// The power b to which a should be raised.
///
/// # Output
/// The power a^b
function PowC(a : Complex, power : Complex) : Complex {
    a^power
}

/// # Summary
/// Returns a number raised to a given power of type `ComplexPolar`.
/// Note that this is a multi-valued function, but only one value is returned.
///
/// # Input
/// ## a
/// The number a that is to be raised.
/// ## power
/// The power b to which a should be raised.
///
/// # Output
/// The power a^b
function PowCP(a : ComplexPolar, power : ComplexPolar) : ComplexPolar {
    ComplexAsComplexPolar(ComplexPolarAsComplex(a)^ComplexPolarAsComplex(power))
}

/// # Summary
/// Returns the quotient of two inputs of type `Complex`.
///
/// # Input
/// ## a
/// The first input a to be divided.
/// ## b
/// The second input b to be divided.
///
/// # Output
/// The quotient a / b.
function DividedByC(a : Complex, b : Complex) : Complex {
    a / b
}

/// # Summary
/// Returns the quotient of two inputs of type `ComplexPolar`.
///
/// # Input
/// ## a
/// The first input a to be divided.
/// ## b
/// The second input b to be divided.
///
/// # Output
/// The quotient a / b.
function DividedByCP(a : ComplexPolar, b : ComplexPolar) : ComplexPolar {
    ComplexPolar(a.Magnitude / b.Magnitude, a.Argument - b.Argument)
}

//
// Fixed point
//

/// # Summary
/// Returns the smallest representable number for specific fixed point dimensions.
///
/// # Input
/// ## integerBits
/// Number of integer bits (including the sign bit).
/// ## fractionalBits
/// Number of fractional bits.
///
/// # Remark
/// The value can be computed as -2^(p-1), where p is the number of integer bits.
function SmallestFixedPoint(integerBits : Int, fractionalBits : Int) : Double {
        -(2.0^IntAsDouble(integerBits - 1))
}

/// # Summary
/// Returns the largest representable number for specific fixed point dimensions.
///
/// # Input
/// ## integerBits
/// Number of integer bits (including the sign bit).
/// ## fractionalBits
/// Number of fractional bits.
///
/// # Remark
/// The value can be computed as 2^(p-1) - 2^(-q), where p
/// is the number of integer bits and q is the number of fractional bits.
function LargestFixedPoint(integerBits : Int, fractionalBits : Int) : Double {
    2.0^IntAsDouble(integerBits - 1) - 2.0^(-IntAsDouble(fractionalBits))
}

export
    PI,
    E,
    LogOf2,
    IsNaN,
    IsInfinite,
    SignI,
    SignD,
    SignL,
    AbsI,
    AbsD,
    AbsL,
    MaxI,
    MaxD,
    MaxL,
    MinI,
    MinD,
    MinL,
    Max,
    Min,
    ArcCos,
    ArcSin,
    ArcTan,
    ArcTan2,
    Cos,
    Cosh,
    Sin,
    Sinh,
    Tan,
    Tanh,
    ArcCosh,
    ArcSinh,
    ArcTanh,
    Sqrt,
    Log,
    Log10,
    Lg,
    Truncate,
    Ceiling,
    Floor,
    Round,
    RoundHalfAwayFromZero,
    DivRemI,
    DivRemL,
    ModulusI,
    ModulusL,
    ExpModI,
    ExpModL,
    InverseModI,
    InverseModL,
    GreatestCommonDivisorI,
    GreatestCommonDivisorL,
    ExtendedGreatestCommonDivisorI,
    ExtendedGreatestCommonDivisorL,
    IsCoprimeI,
    IsCoprimeL,
    ContinuedFractionConvergentI,
    ContinuedFractionConvergentL,
    RealMod,
    BitSizeI,
    BitSizeL,
    TrailingZeroCountI,
    TrailingZeroCountL,
    HammingWeightI,
    FactorialI,
    FactorialL,
    ApproximateFactorial,
    LogGammaD,
    LogFactorialD,
    Binom,
    SquaredNorm,
    PNorm,
    PNormalized,
    Complex,
    ComplexPolar,
    AbsSquaredComplex,
    AbsComplex,
    ArgComplex,
    AbsSquaredComplexPolar,
    AbsComplexPolar,
    ArgComplexPolar,
    NegationC,
    NegationCP,
    PlusC,
    PlusCP,
    MinusC,
    MinusCP,
    TimesC,
    TimesCP,
    PowC,
    PowCP,
    DividedByC,
    DividedByCP,
    SmallestFixedPoint,
    LargestFixedPoint;
