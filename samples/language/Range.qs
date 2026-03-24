// # Sample
// Range
//
// # Description
// Value literals for the Range type are expressions of the form `start..step..stop`, where `start`, `step`,
// and `end` are expressions of type `Int`. If the step size is one, it may be omitted. For example,
// `start..stop` is a valid `Range` literal and the same as `start..1..stop`. Closed ranges specify the
//  `start` and `end` are provided, but open ranges may be expressed by omitting either or both of these
// using the `...` syntax in one of the following forms: `start...`, `...end`, `...`. Open ranges may only
// be used in array slices.

function Main() : Range {

    // Ranges can be captured as local variables.

    // The range 1, 2, 3.
    let range = 1..3;
    Message($"Range: {range}");

    // The range 2, 4.
    let range = 2..2..5;
    Message($"Range: {range}");

    // The range 2, 4, 6.
    let range = 2..2..6;
    Message($"Range: {range}");

    // The range 6, 4, 2.
    let range = 6..-2..2;
    Message($"Range: {range}");

    // The range 2.
    let range = 2..-2..2;
    Message($"Range: {range}");

    // The empty range.
    let range = 2..1;
    Message($"Range: {range}");

    // Ranges are used in for-loop expressions. They must be closed ranges.

    // The array [0, 1, 4, 9, 16, 25, 36, 49, 64, 81, 100].
    mutable array = [];
    for i in 0..10 {
        array += [i^2];
    }
    Message($"Array: {array}");

    // Ranges can be used to create array slices.

    // The array [0, 4, 16, 36, 64, 100].
    let newArray = array[0..2..10];
    Message($"Array slice [0..2..10]: {newArray}");

    // Open ranges can also be used to create array slices.

    // The array [0, 1, 4, 9, 16].
    let newArray = array[...4];
    Message($"Array slice [...4]: {newArray}");

    // The array [25, 36, 49, 64, 81, 100].
    let newArray = array[5...];
    Message($"Array slice [5...]: {newArray}");

    // The array [4, 25, 64].
    let newArray = array[2..3...];
    Message($"Array slice [2..3...]: {newArray}");

    // The array [0, 9, 36].
    let newArray = array[...3..7];
    Message($"Array slice [...3..7]: {newArray}");

    // The array [0, 1, 4, 9, 16, 25, 36, 49, 64, 81, 100].
    let newArray = array[...];
    Message($"Array slice [...]: {newArray}");

    // The array [100, 49, 16, 1].
    let newArray = array[...-3...];
    Message($"Array slice [...-3...]: {newArray}");

    return range;
}
