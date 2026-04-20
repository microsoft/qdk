/// Implements arithmetic gates given by classical function.

import Std.Arrays.Reversed;
import Std.Convert.BigIntAsInt;


// Uses little-endian conversion.
operation _ApplyPermutationUnitary(permutation : Int[], qubits : Qubit[], mode: Int) : Unit {
    if (mode == 0) {
        // Explicitly constructs unitary.
        let n = Length(permutation);
        if (n != 1 <<< Length(qubits)) { fail "Size mismatch."; }
        mutable matrix = Repeated(Repeated(0.0 + 0.0i, n), n);
        for i in 0..n-1 {
            let j = permutation[i];
            set matrix w/= j <- (matrix[j] w/ i <- 1.0 + 0.0i);
        }
        ApplyUnitary(matrix, Reversed(qubits));
    } elif (mode == 1) {
        ApplyPermutationUnitary(permutation, qubits);
    } else {
        fail "Unuspported mode."
    }
}


// Generates permutation for a gate applying given arithmetic function on basis states.
// Size of permutation is 2^num_qubits.
// f must be a bijection on [0..2^num_bits-1].
// Permutation indices are big-endian.
function MakeArithmeticPermutation(num_qubits : Int, f : (Int) -> Int) : Int[] {
    let dim = 2^num_qubits;
    mutable perm = Repeated(0, dim);
    for i in 0..dim-1 {
        set perm w/= i <- f(i);
    }
    return perm;
}


function InversePermutation(perm : Int[]) : Int[] {
    let n = Length(perm);
    mutable ans = Repeated(0, n);
    for i in 0..n-1 {
        set ans w/= perm[i] <- i;
    }
    return ans;
}


operation _ArithmeticGate(f : (Int) -> Int, target : Qubit[], is_adjoint : Bool, mode : Int) : Unit is Ctl {
    body (...) {
        mutable perm = MakeArithmeticPermutation(Length(target), f);
        if (is_adjoint) {
            set perm = InversePermutation(perm);
        }
        _ApplyPermutationUnitary(perm, target, mode);
    }
    controlled (ctls, ...) {
        // Apply f(x) only when all control bits are 1.
        // Otherwise, apply identity function.
        let limit = (2^Length(target)) * (2^Length(ctls)-1);
        let f2 : (Int) -> Int = x -> (if (x < limit) { x } else { f(x - limit) + limit });
        _ArithmeticGate(f2, target + ctls, is_adjoint, mode);
    }
}


// Implements a gate that permutes basis states.
// Maps basis state |i> to |f(i)> (little-endian).
operation ArithmeticGate(f : (Int) -> Int, target : Qubit[], mode: Int) : Unit is Ctl + Adj {
    body (...) {
        _ArithmeticGate(f, target, false, mode);
    }
    adjoint (...) {
        _ArithmeticGate(f, target, true, mode);
    }
}


// Applies arithmetic gate on 2 arguments.
operation ArithmeticGate2Args(f: (Int, Int) -> (Int, Int), reg1 : Qubit[], reg2 : Qubit[], mode: Int) : Unit is Ctl + Adj {
    let n1 = Length(reg1);
    let mask1 = (1<<<n1) - 1;
    let f2  : (Int) -> Int  = x -> {
        let (ans1, ans2) = f(x &&& mask1, x >>> n1);
        return ans1 + (ans2 <<< n1);
    };
    ArithmeticGate(f2, reg1 + reg2, mode);
}


// The code below is temporary, for testing only.

// Modular multiplication implemented by explicit unitary:
//   x -> a*x % N, if 0<=x<N.
//   x -> x,       if x>=N.
operation ModMultiply(target : Qubit[], a : BigInt, N : BigInt, mode: Int) : Unit is Adj + Ctl {
    let a = BigIntAsInt(a);
    let N = BigIntAsInt(N);
    let f = x -> if (x < N) { (x * a) % N } else { x };
    ArithmeticGate(f, target, mode);
}


// Temporary, for testing only.
operation Increment(target : Qubit[], mode: Int) : Unit is Adj + Ctl {
     let N = 1<<<Length(target);
     let f = x -> (x+1)%N;
     ArithmeticGate(f, target, mode);
}


/// Computes [(a^(2^i))%N for i in 0..n-1].
function ComputeSequentialSquares(a : BigInt, N : BigInt, n : Int) : BigInt[] {
    mutable ans : BigInt[] = [((a % N) + N) % N];
    for i in 1..n-1 {
        set ans += [(ans[i-1] * ans[i-1]) % N];
    }
    return ans;
}


// Modular exponent:
//   x -> (base^exponent * x) % N, if 0<=x<N.
//   x -> x,                       if x >= N.
// Assumes gcd(base, N)=1, otherwise modular multiplication is not unitary.
operation ModExp(target : Qubit[], exponent : Qubit[], base : BigInt, N : BigInt, mode: Int) : Unit is Adj + Ctl {
    let le = Length(exponent);
    let sqs = ComputeSequentialSquares(base, N, le);
    for i in 0..le-1 {
        Controlled ModMultiply([exponent[i]], (target, sqs[i], N, mode));
    }
}


export ArithmeticGate, ArithmeticGate2Args, ModExp, Increment;
