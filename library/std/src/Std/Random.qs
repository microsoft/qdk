// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.



/// # Summary
/// Draws a random integer from a uniform distribution
/// in a given inclusive range. Fails if `max < min`.
/// This operation is simulator-only and is not supported on hardware.
///
/// # Input
/// ## min
/// The smallest integer to be drawn.
/// ## max
/// The largest integer to be drawn.
///
/// # Output
/// An integer in the inclusive range from `min` to `max` with uniform
/// probability.
///
/// # Example
/// The following Q# snippet randomly rolls a six-sided die:
/// ```qsharp
/// let roll = DrawRandomInt(1, 6);
/// ```
@Config(Unrestricted)
operation DrawRandomInt(min : Int, max : Int) : Int {
    body intrinsic;
}

/// # Summary
/// Draws a random real number from a uniform distribution
/// in a given inclusive interval. Fails if `max < min`.
/// This operation is simulator-only and is not supported on hardware.
///
/// # Input
/// ## min
/// The smallest real number to be drawn.
/// ## max
/// The largest real number to be drawn.
///
/// # Output
/// A random real number in the inclusive interval from `min` to `max` with
/// uniform probability.
///
/// # Example
/// The following Q# snippet randomly draws an angle between 0 and 2π:
/// ```qsharp
/// let angle = DrawRandomDouble(0.0, 2.0 * PI());
/// ```
@Config(Unrestricted)
operation DrawRandomDouble(min : Double, max : Double) : Double {
    body intrinsic;
}

/// # Summary
/// Given a success probability, returns a single Bernoulli trial
/// that is true with the given probability.
/// This operation is simulator-only and is not supported on hardware.
///
/// # Input
/// ## successProbability
/// The probability with which true should be returned.
///
/// # Output
/// `true` with probability `successProbability`
/// and `false` with probability `1.0 - successProbability`.
///
/// # Example
/// The following Q# snippet samples flips from a biased coin:
/// ```qsharp
/// let flips = DrawMany(DrawRandomBool, 10, 0.6);
/// ```
@Config(Unrestricted)
operation DrawRandomBool(successProbability : Double) : Bool {
    body intrinsic;
}

export DrawRandomInt, DrawRandomDouble, DrawRandomBool;
