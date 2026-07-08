namespace Kata {
    import Std.Arrays.*;

    operation Oracle_WeakColoring_OneVertex(
        V : Int, edges: (Int, Int)[], x : Qubit[], y : Qubit, vertex : Int
    ) : Unit is Adj + Ctl {
        // Implement your solution here...

    }

    // You might find this helper operation from an earlier task useful.
    operation Oracle_ColorEquality(x0 : Qubit[], x1 : Qubit[], y : Qubit) : Unit is Adj + Ctl {
        within {
            for i in 0..Length(x0) - 1 {
                CNOT(x0[i], x1[i]);
            }
        } apply {
            ApplyControlledOnInt(0, X, x1, y);
        }
    }
}
