import Std.Measurement.*;

operation Main() : Result[] {
    use q0 = Qubit();
    use q1 = Qubit();
    H(q0);
    let r = M(q0);
    if r == One {
        X(q1);
    } else {
        Y(q1);
    }
    let r1 = M(q1);
    [r, r1]
}

