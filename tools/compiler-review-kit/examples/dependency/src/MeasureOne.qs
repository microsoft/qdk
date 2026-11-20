namespace Library {
    operation MeasureOne() : Result {
        use q = Qubit();
        X(q);
        MResetZ(q)
    }
    export MeasureOne;
}
