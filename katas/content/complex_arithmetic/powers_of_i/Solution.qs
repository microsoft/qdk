namespace Kata {
    function EvenPowersOfI(n : Int) : Int {
        // Check remainder when n is divided by 4
        let remainder = Std.Math.ModulusI(n, 4);
        if remainder == 0 {
            return 1;
        } elif remainder == 2 {
            return -1;
        } else {
            fail "n must be even.";
        }
    }
}
