namespace Kata.Verification {
   
    operation EvenPowersOfI_Reference(n : Int) : Int{
        // If n is divisible by 4
        if n % 4 == 0 { 
            return 1;
        } else {
            return -1;
        }
    }

    @EntryPoint()
    operation CheckSolution() : Bool {
        for n in -20 .. 2 .. 20 {   
            let expected = EvenPowersOfI_Reference(n);
            let actual = Kata.EvenPowersOfI(n);
            if expected != actual {
                Message("Incorrect.");
                Message($"Result of exponentiation doesn't match expected value: expected i^({n}) = {expected}, got {actual}");
                return false; 
            }
        }
        Message("Correct!");
        return true; 
    }
}
