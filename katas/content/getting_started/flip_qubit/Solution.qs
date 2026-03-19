namespace Kata {
    operation FlipQubit(q : Qubit) : Unit is Adj + Ctl {
        // Perform a "bit flip" on the qubit by applying the X gate.
        X(q);
    }
}
namespace Foo {
    @EntryPoint()
    operation ShowCircuit() : Unit {
        use q = Qubit();
        Kata.FlipQubit(q);
    }
}
