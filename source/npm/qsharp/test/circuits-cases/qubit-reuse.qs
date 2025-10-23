operation Main() : Unit {
    {
        use q1 = Qubit();
        X(q1);
        MResetZ(q1);
    }
    {
        use q2 = Qubit();
        Y(q2);
        MResetZ(q2);
    }
}
