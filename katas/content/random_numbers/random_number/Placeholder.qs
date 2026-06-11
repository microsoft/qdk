namespace Kata {
    import Std.Math.*;

    operation RandomNumberInRange(min : Int, max : Int) : Int {
        // Implement your solution here...

        return -1;
    }

    // You can use the operations defined in the earlier exercises to implement your solution.
    operation RandomBit() : Int {
        use q = Qubit();
        H(q);
        return MResetZ(q) == Zero ? 0 | 1;
    }

    operation RandomNBits(N : Int) : Int {
        mutable result = 0;
        for i in 0 .. N - 1 {
            set result = result * 2 + RandomBit();
        }
        return result;
    }
}
