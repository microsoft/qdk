operation Main() : Result[] {
    use q = Qubit();
    CustomIntrinsic(q);
    SimulatableIntrinsic(q);
    let r = CustomMeasurement(q);
    [r]
}

operation CustomIntrinsic(q : Qubit) : Unit  {
    body intrinsic;
}


@SimulatableIntrinsic()
operation SimulatableIntrinsic(q : Qubit) : Unit  {
    H(q);
}


@Measurement()
@SimulatableIntrinsic()
operation CustomMeasurement(q : Qubit) : Result {
    M(q)
}