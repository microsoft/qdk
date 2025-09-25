// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

namespace Std.Core {
    /// # Summary
    /// Returns the number of elements in the input array `a`.
    ///
    /// # Input
    /// ## a
    /// Input array.
    ///
    /// # Output
    /// The total number of elements in the input array `a`.
    ///
    /// # Example
    /// ```qsharp
    /// Message($"{ Length([0, 0, 0]) }"); // Prints 3
    /// ```
    function Length<'T>(a : 'T[]) : Int {
        body intrinsic;
    }

    /// # Summary
    /// Creates an array of given `length` with all elements equal to given
    /// `value`. `length` must be a non-negative integer.
    ///
    /// # Description
    /// Use this function to create an array of length `length` where each
    /// element is equal to `value`. This way of creating an array is preferred
    /// over other methods if all elements of the array must be the same and
    /// the length is known upfront.
    ///
    /// # Input
    /// ## value
    /// The value of each element of the new array.
    /// ## length
    /// Length of the new array.
    ///
    /// # Output
    /// A new array of length `length`, such that every element is `value`.
    ///
    /// # Example
    /// ```qsharp
    /// // Create an array of 3 Boolean values, each equal to `true`
    /// let array = Repeated(true, 3);
    /// ```
    function Repeated<'T>(value : 'T, length : Int) : 'T[] {
        if length < 0 {
            fail "Length must be a non-negative integer";
        }

        mutable output = [];
        for _ in 1..length {
            set output += [value];
        }

        output
    }

    /// # Summary
    /// Represents a complex number by its real and imaginary components.
    /// The real component can be accessed via the `Real` field, and the imaginary
    /// component via the `Imag` field. Complex literals can be written using the
    /// form `a + bi`, where `a` is the Double literal for the real part and
    /// `b` is the Double literal for the imaginary part.
    ///
    /// # Example
    /// The following snippet defines the imaginary unit 𝑖 = 0 + 1𝑖:
    /// ```qsharp
    /// let imagUnit = 1.0i;
    /// ```
    struct Complex { Real : Double, Imag : Double }

    export Length, Repeated, Complex;
}
