// # Sample
// Break and Continue
//
// # Description
// The `break` statement immediately exits the innermost enclosing loop.
// The `continue` statement skips the rest of the current iteration and
// proceeds to the next one. Both can be used in the bodies of `for`,
// `while`, and `repeat`-`until` loops. They cannot be used in loop
// conditions or in `repeat`-`until` fixup blocks.

function Main() : (Int, Int, Int) {
    // Use `break` to stop a loop early. Here we find the first integer whose
    // square exceeds 50 and then stop searching.
    mutable firstOverFifty = 0;
    for n in 1..100 {
        if n * n > 50 {
            firstOverFifty = n;
            break;
        }
    }

    // Use `continue` to skip selected iterations. Here we sum the numbers from
    // 1 to 20, skipping every multiple of 3.
    mutable sumWithoutMultiplesOfThree = 0;
    for n in 1..20 {
        if n % 3 == 0 {
            continue;
        }
        sumWithoutMultiplesOfThree += n;
    }

    // `break` and `continue` also work in `while` loops. Here we count how many
    // times 1 can be doubled before the result exceeds 1000.
    mutable doublings = 0;
    mutable value = 1;
    while true {
        value *= 2;
        if value <= 1000 {
            doublings += 1;
            continue;
        }
        break;
    }

    return (firstOverFifty, sumWithoutMultiplesOfThree, doublings);
}
