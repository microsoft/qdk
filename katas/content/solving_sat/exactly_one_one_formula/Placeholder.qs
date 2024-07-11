namespace Kata {
    open Microsoft.Quantum.Arrays;

    operation Oracle_Exactly13SATFormula(x : Qubit[], y : Qubit, formula : (Int, Bool)[][]) : Unit is Adj + Ctl {
        // Implement your solution here...

    }

    // You might want to implement this helper operation that evaluates a single clause and use it in your solution.
    operation Oracle_Exactly13SATClause(x : Qubit[], y : Qubit, clause : (Int, Bool)[]) : Unit is Adj + Ctl {
        // Implement your solution here...

    }

    // You might find these helper operations from earlier tasks useful.
    operation Oracle_Exactly1One(x : Qubit[], y : Qubit) : Unit is Adj + Ctl {
        for i in 0 .. Length(x) - 1 {
            ApplyControlledOnInt(2 ^ i, X, x, y);
        }
    }        

    operation Oracle_And(x : Qubit[], y : Qubit) : Unit is Adj + Ctl {
        Controlled X(x, y);
    }        
}
