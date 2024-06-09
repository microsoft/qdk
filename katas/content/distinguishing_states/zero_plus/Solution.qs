namespace Kata {
    open Microsoft.Quantum.Math;
    
    operation IsQubitZeroOrPlus (q : Qubit) : Bool {
        Ry(0.25 * PI(), q);
        return M(q) == Zero;
    }
}
