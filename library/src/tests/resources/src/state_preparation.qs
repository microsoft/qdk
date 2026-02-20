namespace Test {
    import Std.Convert.*;
    import Std.Math.*;
    import Std.Diagnostics.*;
    import Std.Arrays.*;
    import Std.StatePreparation.*;
    import QIR.Intrinsic.*;



    operation TestPlusState() : Unit {
        use q = Qubit();
        PreparePureStateD([Sqrt(0.5), Sqrt(0.5)], [q]);
        DumpMachine();
        // Ucompute plus
        H(q);
    }

    operation TestMinusState() : Unit {
        use q = Qubit();
        PreparePureStateD([Sqrt(0.5), -Sqrt(0.5)], [q]);
        DumpMachine();
        // Ucompute minus
        H(q);
        X(q);
    }

    operation TestBellState() : Unit {
        use q = Qubit[2];
        PreparePureStateD([Sqrt(0.5), 0.0, 0.0, Sqrt(0.5)], q);
        DumpMachine();
        // Ucompute Bell
        CNOT(q[0], q[1]);
        H(q[0]);
    }

    operation TestCat3State() : Unit {
        use q = Qubit[3];
        PreparePureStateD([Sqrt(0.5), 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, Sqrt(0.5)], q);
        DumpMachine();
        // Ucompute Cat
        CNOT(q[0], q[2]);
        CNOT(q[0], q[1]);
        H(q[0]);
    }

    operation PrepareComplex(qs : Qubit[]) : Unit is Adj {
        H(qs[0]);
        S(qs[0]);
        H(qs[1]);
        T(qs[1]);
    }

    operation TestPrepareComplex() : Unit {
        use q = Qubit[2];
        let c00 = ComplexPolar(0.5, 0.0);
        let c01 = ComplexPolar(0.5, PI() / 4.0);
        let c10 = ComplexPolar(0.5, PI() / 2.0);
        let c11 = ComplexPolar(0.5, 3.0 * PI() / 4.0);
        ApproximatelyPreparePureStateCP(0.0, [c00, c01, c10, c11], q);
        DumpMachine();
        Adjoint PrepareComplex(q);
    }

    operation TestPreparationCompletion() : Unit {
        let testCases = [
            // Test positive coefficients
            [0.773761, 0.633478],
            [0.183017, 0.406973, 0.604925, 0.659502],
            [0.0986553, 0.359005, 0.465689, 0.467395, 0.419893, 0.118445, 0.461883, 0.149609],
            [0.271471, 0.0583654, 0.11639, 0.36112, 0.307383, 0.193371, 0.274151, 0.332542, 0.130172, 0.222546, 0.314879, 0.210704, 0.212429, 0.245518, 0.30666, 0.22773],

            // Test negative coefficients; should give same probabilities as positive coefficients
            [-0.773761, 0.633478],
            [0.183017, -0.406973, 0.604925, 0.659502],
            [0.0986553, -0.359005, 0.465689, -0.467395, 0.419893, 0.118445, -0.461883, 0.149609],
            [-0.271471, 0.0583654, 0.11639, 0.36112, -0.307383, 0.193371, -0.274151, 0.332542, 0.130172, 0.222546, 0.314879, -0.210704, 0.212429, 0.245518, -0.30666, -0.22773],

            // Test unnormalized coefficients
            [1.0986553, 0.359005, 0.465689, -0.467395, 0.419893, 0.118445, 0.461883, 0.149609],

            // Test missing coefficients
            [1.0986553, 0.359005, 0.465689, -0.467395, 0.419893, 0.118445]
        ];

        for coefficients in testCases {
            let L = Length(coefficients);
            let N = Ceiling(Log(IntAsDouble(L)) / LogOf2() - 0.001);
            use q = Qubit[N];
            PreparePureStateD(coefficients, q);
            DumpMachine();
            Adjoint PreparePureStateD(coefficients, q);
        }
    }

    operation TestEndianness() : Unit {
        let n = 4;
        use qs = Qubit[n];
        let bitsize = 2^n;
        for i in 0..bitsize - 1 {
            mutable c = Repeated(0.0, bitsize);
            set c w/= i <- 1.0;
            PreparePureStateD(c, qs);
            DumpMachine();
            ResetAll(qs);
        }
    }

    operation TestPrepareUniformSuperposition(nStates : Int) : Unit {
        use qs = Qubit[10];
        PrepareUniformSuperposition(nStates, qs);
        DumpMachine();
        ResetAll(qs);
    }

    operation TestPrepareUniformSuperpositionExhaustive() : Unit {
        let NBits = 4;
        for NStates in 1..2^NBits-1 {
            use qs = Qubit[NBits];
            PrepareUniformSuperposition(NStates, qs);
            let t = Std.Math.Sqrt(1.0 / IntAsDouble(NStates));
            Adjoint Std.StatePreparation.PreparePureStateD(Repeated(t, NStates), Reversed(qs));
            Fact(CheckAllZero(qs), "All qubits must be in zero state.");
        }
    }

    operation TestControlledPreparation() : Unit {
        use qs = Qubit[2];
        let coeffs = [0.5, 0.5, 0.5, 0.5];
        use c = Qubit();
        H(c);
        Controlled Std.StatePreparation.PreparePureStateD([c], (coeffs, qs));
        Std.Diagnostics.DumpMachine();
        ResetAll(qs + [c]);
    }

}
