import Std.ArithmeticUtils.ApplyBigInt;
import Std.ArithmeticUtils.MResetL;


operation TestUnaryOp(op : (Qubit[]) => Unit, n : Int, x_val : BigInt) : BigInt {
    use x = Qubit[n];
    ApplyBigInt(x_val, x);
    op(x);
    return MResetL(x);
}


operation TestBinaryOp(op : (Qubit[], Qubit[]) => Unit, n1 : Int, n2: Int, x_val : BigInt, y_val : BigInt) : (BigInt, BigInt) {
    use x = Qubit[n1];
    use y = Qubit[n2];
    ApplyBigInt(x_val, x);
    ApplyBigInt(y_val, y);
    op(x, y);
    return (MResetL(x), MResetL(y));
}

export TestUnaryOp, TestBinaryOp;
