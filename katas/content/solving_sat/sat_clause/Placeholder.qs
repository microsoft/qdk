namespace Kata {
    import Std.Arrays.*;

    operation Oracle_SATClause(x : Qubit[], y : Qubit, clause : (Int, Bool)[]) : Unit is Adj + Ctl {
        // Implement your solution here...

    }

    // You might find this helper operation from an earlier task useful.
    operation Oracle_Or(x : Qubit[], y : Qubit) : Unit is Adj + Ctl {
        ApplyControlledOnInt(0, X, x, y);
        X(y);
    }        
}
