namespace Kata {
    operation ArbitraryBitPattern_Oracle(x : Qubit[], y : Qubit, pattern : Bool[])
    : Unit  is Adj + Ctl {
        ApplyControlledOnBitString(pattern, X, x, y);
    }
}
