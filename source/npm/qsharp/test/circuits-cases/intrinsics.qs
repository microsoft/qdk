operation Main() : Result[] {
    use q = Qubit();
    CustomIntrinsic(q);
    SimulatableIntrinsic(q);
    [MResetZ(q)]
}

operation CustomIntrinsic(q : Qubit) : Unit  {
    body intrinsic;
}


@SimulatableIntrinsic()
operation SimulatableIntrinsic(q : Qubit) : Unit  {
    H(q);
}
