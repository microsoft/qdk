/// # Sample
/// Comparison Operators
///
/// # Description
/// Comparison operators in Q# are used to compare one value relative to
/// another value of the same type, producing an output Boolean value.
/// The comparison operators in Q# are `==`, `!=`, `<`, `<=`, `>`, and `>=`.
namespace MyQuantumApp {

    @EntryPoint()
    operation Main() : Unit {

        // `==` tests if the first value is equivalent to the second value.

        // The Boolean value `true`.
        let boolean = 4 == 4;
        Message($"Equality comparison: {boolean}");

        // The Boolean value `false`.
        let boolean = 4 == 6;
        Message($"Equality comparison: {boolean}");

        // `!=` tests if the first value is not equivalent to the second value.
        // It is the opposite of `==`.

        // The Boolean value `false`.
        let boolean = 4 != 4;
        Message($"Inequality comparison: {boolean}");

        // The Boolean value `true`.
        let boolean = 4 != 6;
        Message($"Inequality comparison: {boolean}");

        // `<` tests if the first value is strictly less than the second value.

        // The Boolean value `false`.
        let boolean = 4 < 4;
        Message($"Less than comparison: {boolean}");

        // The Boolean value `true`.
        let boolean = 4 < 6;
        Message($"Less than comparison: {boolean}");

        // The Boolean value `false`.
        let boolean = 6 < 4;
        Message($"Less than comparison: {boolean}");

        // `<=` tests if the first value is less than or equivalent to
        // the second value.

        // The Boolean value `true`.
        let boolean = 4 <= 4;
        Message($"Less than or equal comparison: {boolean}");

        // The Boolean value `true`.
        let boolean = 4 <= 6;
        Message($"Less than or equal comparison: {boolean}");

        // The Boolean value `false`.
        let boolean = 6 <= 4;
        Message($"Less than or equal comparison: {boolean}");

        // `>` tests if the first value is strictly greater than the second value.

        // The Boolean value `false`.
        let boolean = 4 > 4;
        Message($"Greater than comparison: {boolean}");

        // The Boolean value `false`.
        let boolean = 4 > 6;
        Message($"Greater than comparison: {boolean}");

        // The Boolean value `true`.
        let boolean = 6 > 4;
        Message($"Greater than comparison: {boolean}");

        // `>=` tests if the first value is greater than or equivalent to
        // the second value.

        // The Boolean value `true`.
        let boolean = 4 >= 4;
        Message($"Greater than or equal comparison: {boolean}");

        // The Boolean value `false`.
        let boolean = 4 >= 6;
        Message($"Greater than or equal comparison: {boolean}");

        // The Boolean value `true`.
        let boolean = 6 >= 4;
        Message($"Greater than or equal comparison: {boolean}");
    }
}
