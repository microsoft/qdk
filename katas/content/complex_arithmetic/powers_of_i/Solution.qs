namespace Kata {
    function EvenPowerOfI(n : Int) : Int {
        // NOTE: Only even values of n will be passed to this function.
        if n % 4 == 0 {
            // n is divisible by 4. Therefore, i^n = 1.
            return 1;
        } else {
            // n is not divisible by 4. Since n is even, n % 4 must be 2 or -2.
            // Therefore, i^n = -1.
            return -1;
        }
   }
}
