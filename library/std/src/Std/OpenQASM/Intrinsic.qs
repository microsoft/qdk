// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// This file defines the standard gates for OpenQASM and Qiskit.
/// It is an internal implementation detail for OpenQASM compilation
/// and is not intended for use outside of this context.

// OpenQASM 3.0 intrinsics
export gphase, U;

// OpenQASM 2.0 intrinsics
export CX; // `U` exported above.

// stdgates.inc <https://github.com/openqasm/openqasm/blob/spec/v3.1.0/examples/stdgates.inc>
// main gate definitions
export p, x, y, z, h, s, sdg, t, tdg, sx, rx, ry, rz, cx, cy, cz, cp, crx, cry, crz, ch, swap, ccx, cswap, cu;

// stdgates.inc OpenQASM 2.0 backwards compatibility gates
// `CX` is already exported above, so we don't need to export it again.
export phase, cphase, id, u1, u2, u3;

// qelib1.inc <https://github.com/openqasm/openqasm/blob/2.0/examples/qelib1.inc>
// QE Hardware primitives are defined above as the OpenQASM 2.0 compatibility gates.
// QE Standard Gates are defined above as the stdgates.inc gates.
// Standard rotations are defined above as the stdgates.inc gates.

// Most QE Standard User-Defined Gates are defined above as the stdgates.inc gates.
// Remaining QE Standard User-Defined Gates:
export cu1, cu3;

// gates that qiskit won't emit qasm defs for that are NOT part of stgates.inc
// but are have the _standard_gate property in Qiskit:

// QIR intrinsics missing from qasm std library, that Qiskit won't emit qasm defs for
export rxx, ryy, rzz;

// Remaining gates that are not in the qasm std library, but are standard gates in Qiskit
// that Qiskit wont emit correctly.
export dcx, ecr, r, rzx, cs, csdg, sxdg, csx, rccx, c3sqrtx, c3x, rc3x, xx_minus_yy, xx_plus_yy, ccz;

export mresetz_checked;

export __quantum__qis__barrier__body;

import Std.OpenQASM.Angle.Angle;
import Std.OpenQASM.Angle.AngleAsDouble;
import Std.OpenQASM.Angle.DoubleAsAngle;
import Std.OpenQASM.Angle.AddAngles;
import Std.OpenQASM.Angle.SubtractAngles;
import Std.OpenQASM.Angle.DivideAngleByInt;
import Std.OpenQASM.Angle.NegAngle;

function ZERO_ANGLE() : Angle {
    return DoubleAsAngle(0., 1);
}

function PI_OVER_2() : Angle {
    return DoubleAsAngle(Std.Math.PI() / 2., 53);
}

function PI_OVER_4() : Angle {
    return DoubleAsAngle(Std.Math.PI() / 4., 53);
}

function PI_OVER_8() : Angle {
    return DoubleAsAngle(Std.Math.PI() / 8., 53);
}

function PI_ANGLE() : Angle {
    return DoubleAsAngle(Std.Math.PI(), 53);
}

function NEG_PI_OVER_2() : Angle {
    return DoubleAsAngle(-Std.Math.PI() / 2., 53);
}

function NEG_PI_OVER_4() : Angle {
    return DoubleAsAngle(-Std.Math.PI() / 4., 53);
}

function NEG_PI_OVER_8() : Angle {
    return DoubleAsAngle(-Std.Math.PI() / 8., 53);
}

operation gphase(theta : Angle) : Unit is Adj + Ctl {
    body ... {
        Exp([], AngleAsDouble(theta), [])
    }
    adjoint auto;
    controlled auto;
    controlled adjoint auto;
}

operation U(theta : Angle, phi : Angle, lambda : Angle, qubit : Qubit) : Unit is Adj + Ctl {
    body ... {
        let theta = AngleAsDouble(theta);
        let phi = AngleAsDouble(phi);
        let lambda = AngleAsDouble(lambda);

        Std.Intrinsic.Rz(lambda, qubit);
        Std.Intrinsic.Ry(theta, qubit);
        Std.Intrinsic.Rz(phi, qubit);
        Std.Intrinsic.R(PauliI, -lambda - phi - theta, qubit);
    }
    adjoint auto;
    controlled auto;
    controlled adjoint auto;
}
operation CX(ctrl : Qubit, qubit : Qubit) : Unit is Adj + Ctl {
    Std.Canon.CX(ctrl, qubit);
}

operation p(lambda : Angle, qubit : Qubit) : Unit is Adj + Ctl {
    Controlled gphase([qubit], lambda);
}

operation x(qubit : Qubit) : Unit is Adj + Ctl {
    Std.Intrinsic.X(qubit);
}

operation y(qubit : Qubit) : Unit is Adj + Ctl {
    Std.Intrinsic.Y(qubit);
}

operation z(qubit : Qubit) : Unit is Adj + Ctl {
    Std.Intrinsic.Z(qubit);
}

operation h(qubit : Qubit) : Unit is Adj + Ctl {
    Std.Intrinsic.H(qubit);
}

operation s(qubit : Qubit) : Unit is Adj + Ctl {
    Std.Intrinsic.S(qubit);
}

operation sdg(qubit : Qubit) : Unit is Adj + Ctl {
    Adjoint Std.Intrinsic.S(qubit);
}

operation t(qubit : Qubit) : Unit is Adj + Ctl {
    Std.Intrinsic.T(qubit);
}

operation tdg(qubit : Qubit) : Unit is Adj + Ctl {
    Adjoint Std.Intrinsic.T(qubit);
}

operation sx(qubit : Qubit) : Unit is Adj + Ctl {
    Std.Intrinsic.SX(qubit);
}

operation rx(theta : Angle, qubit : Qubit) : Unit is Adj + Ctl {
    let theta = AngleAsDouble(theta);
    Std.Intrinsic.Rx(theta, qubit);
}

operation ry(theta : Angle, qubit : Qubit) : Unit is Adj + Ctl {
    let theta = AngleAsDouble(theta);
    Std.Intrinsic.Ry(theta, qubit);
}

operation rz(theta : Angle, qubit : Qubit) : Unit is Adj + Ctl {
    let theta = AngleAsDouble(theta);
    Std.Intrinsic.Rz(theta, qubit);
}

operation cx(ctrl : Qubit, qubit : Qubit) : Unit is Adj + Ctl {
    Std.Canon.CX(ctrl, qubit);
}

operation cy(ctrl : Qubit, qubit : Qubit) : Unit is Adj + Ctl {
    Std.Canon.CY(ctrl, qubit);
}

operation cz(ctrl : Qubit, qubit : Qubit) : Unit is Adj + Ctl {
    Std.Canon.CZ(ctrl, qubit);
}

operation cp(lambda : Angle, ctrl : Qubit, qubit : Qubit) : Unit is Adj + Ctl {
    Controlled p([ctrl], (lambda, qubit));
}

operation crx(theta : Angle, ctrl : Qubit, qubit : Qubit) : Unit is Adj + Ctl {
    let theta = AngleAsDouble(theta);
    Controlled Std.Intrinsic.Rx([ctrl], (theta, qubit));
}

operation cry(theta : Angle, ctrl : Qubit, qubit : Qubit) : Unit is Adj + Ctl {
    let theta = AngleAsDouble(theta);
    Controlled Std.Intrinsic.Ry([ctrl], (theta, qubit));
}

operation crz(theta : Angle, ctrl : Qubit, qubit : Qubit) : Unit is Adj + Ctl {
    let theta = AngleAsDouble(theta);
    Controlled Std.Intrinsic.Rz([ctrl], (theta, qubit));
}

operation ch(ctrl : Qubit, qubit : Qubit) : Unit is Adj + Ctl {
    Controlled Std.Intrinsic.H([ctrl], qubit);
}

operation swap(qubit1 : Qubit, qubit2 : Qubit) : Unit is Adj + Ctl {
    Std.Intrinsic.SWAP(qubit1, qubit2);
}

operation ccx(ctrl1 : Qubit, ctrl2 : Qubit, target : Qubit) : Unit is Adj + Ctl {
    Std.Intrinsic.CCNOT(ctrl1, ctrl2, target);
}

operation cswap(ctrl : Qubit, qubit1 : Qubit, qubit2 : Qubit) : Unit is Adj + Ctl {
    Controlled Std.Intrinsic.SWAP([ctrl], (qubit1, qubit2));
}

operation cu(theta : Angle, phi : Angle, lambda : Angle, gamma : Angle, qubit1 : Qubit, qubit2 : Qubit) : Unit is Adj + Ctl {
    p(SubtractAngles(gamma, DivideAngleByInt(theta, 2)), qubit1);
    Controlled U([qubit1], (theta, phi, lambda, qubit2));
}

// Gates for OpenQASM 2 backwards compatibility
operation phase(lambda : Angle, qubit : Qubit) : Unit is Adj + Ctl {
    U(ZERO_ANGLE(), ZERO_ANGLE(), lambda, qubit);
}

operation cphase(ctrl : Qubit, lambda : Angle, qubit : Qubit) : Unit is Adj + Ctl {
    Controlled phase([ctrl], (lambda, qubit));
}

operation id(qubit : Qubit) : Unit is Adj + Ctl {
    Std.Intrinsic.I(qubit)
}

operation u1(lambda : Angle, qubit : Qubit) : Unit is Adj + Ctl {
    U(ZERO_ANGLE(), ZERO_ANGLE(), lambda, qubit);
}

operation u2(phi : Angle, lambda : Angle, qubit : Qubit) : Unit is Adj + Ctl {
    gphase(NegAngle(DivideAngleByInt(AddAngles(
        phi,
        AddAngles(
            lambda,
            PI_OVER_2()
        )
    ), 2)));

    U(PI_OVER_2(), phi, lambda, qubit);
}

operation u3(theta : Angle, phi : Angle, lambda : Angle, qubit : Qubit) : Unit is Adj + Ctl {
    gphase(NegAngle(DivideAngleByInt(AddAngles(
        phi,
        AddAngles(
            lambda,
            theta
        )
    ), 2)));

    U(theta, phi, lambda, qubit);
}

/// Controlled-U1 gate.
/// `ctrl @ u1(lambda) a, b` or:
/// ```
/// gate cu1(lambda) a,b {
///     u1(lambda/2) a;
///     cx a,b;
///     u1(-lambda/2) b;
///     cx a,b;
///     u1(lambda/2) b;
/// }
/// ```
operation cu1(lambda : Angle, ctrl : Qubit, target : Qubit) : Unit is Adj + Ctl {
    Controlled u1([ctrl], (lambda, target));
}

/// Controlled-U3 gate (3-parameter two-qubit gate).
/// `ctrl @ u3(theta, phi, lambda) a, b` or:
/// ```
/// gate cu3(theta,phi,lambda) c, t {
///     u1((lambda+phi)/2) c;
///     u1((lambda-phi)/2) t;
///     cx c,t;
///     u3(-theta/2,0,-(phi+lambda)/2) t;
///     cx c,t;
///     u3(theta/2,phi,0) t;
/// }
/// ```
operation cu3(theta : Angle, phi : Angle, lambda : Angle, ctrl : Qubit, target : Qubit) : Unit is Adj + Ctl {
    Controlled u3([ctrl], (theta, phi, lambda, target));
}

/// rxx: gate rxx(theta) a, b { h a; h b; cx a, b; rz(theta) b; cx a, b; h b; h a; }
operation rxx(theta : Angle, qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
    Std.Intrinsic.Rxx(AngleAsDouble(theta), qubit0, qubit1);
}

/// ryy: gate ryy(theta) a, b { rx(pi/2) a; rx(pi/2) b; cx a, b; rz(theta) b; cx a, b; rx(-pi/2) a; rx(-pi/2) b; }
operation ryy(theta : Angle, qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
    Std.Intrinsic.Ryy(AngleAsDouble(theta), qubit0, qubit1);
}

/// rzz: gate rzz(theta) a, b { cx a, b; u1(theta) b; cx a, b; }
operation rzz(theta : Angle, qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
    Std.Intrinsic.Rzz(AngleAsDouble(theta), qubit0, qubit1);
}

/// Double-CNOT gate.
/// ```
/// gate dcx a, b {
///     cx a, b;
///     cx b, a;
/// }
/// ```
operation dcx(qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
    cx(qubit0, qubit1);
    cx(qubit1, qubit0);
}

/// An echoed cross-resonance gate.
/// `gate ecr a, b { rzx(pi/4) a, b; x a; rzx(-pi/4) a, b; }`
operation ecr(qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
    rzx(PI_OVER_4(), qubit0, qubit1);
    x(qubit0);
    rzx(NEG_PI_OVER_4(), qubit0, qubit1);
}

/// Rotation θ around the cos(φ)x + sin(φ)y axis.
/// `gate r(θ, φ) a {u3(θ, φ - π/2, -φ + π/2) a;}`
operation r(theta : Angle, phi : Angle, qubit : Qubit) : Unit is Adj + Ctl {
    u3(theta, AddAngles(
        phi,
        NEG_PI_OVER_2()
    ), SubtractAngles(PI_OVER_2(), phi), qubit);
}

/// A parametric 2-qubit `Z ⊗ X` interaction (rotation about ZX).
/// `gate rzx(theta) a, b { h b; cx a, b; u1(theta) b; cx a, b; h b; }`
operation rzx(theta : Angle, qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
    h(qubit1);
    cx(qubit0, qubit1);
    u1(theta, qubit1);
    cx(qubit0, qubit1);
    h(qubit1);
}

/// Controlled-S gate.
/// `gate cs a,b { h b; cp(pi/2) a,b; h b; }`
operation cs(qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
    Controlled s([qubit1], qubit0);
}

/// Controlled-S† gate.
/// csdg: gate csdg a,b { h b; cp(-pi/2) a,b; h b; }
operation csdg(qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
    Controlled Adjoint S([qubit1], qubit0);
}

/// The inverse single-qubit Sqrt(X) gate.
/// `gate sxdg a { rz(pi/2) a; h a; rz(pi/2); }`
operation sxdg(qubit : Qubit) : Unit is Adj + Ctl {
    Adjoint sx(qubit);
}

// Controlled-√X gate.
/// `gate csx a,b { h b; cu1(pi/2) a,b; h b; }`
operation csx(qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
    Controlled sx([qubit1], qubit0);
}

/// The simplified Toffoli gate, also referred to as Margolus gate.
/// `gate rccx a,b,c { u2(0,pi) c; u1(pi/4) c; cx b, c; u1(-pi/4) c; cx a, c; u1(pi/4) c; cx b, c; u1(-pi/4) c; u2(0,pi) c; }`
operation rccx(ctrl1 : Qubit, ctrl2 : Qubit, target : Qubit) : Unit is Adj + Ctl {
    u2(ZERO_ANGLE(), PI_ANGLE(), target);
    u1(PI_OVER_4(), target);
    cx(ctrl2, target);
    u1(NEG_PI_OVER_4(), target);
    cx(ctrl1, target);
    u1(PI_OVER_4(), target);
    cx(ctrl2, target);
    u1(NEG_PI_OVER_4(), target);
    u2(ZERO_ANGLE(), PI_ANGLE(), target);
}

/// c3sx/c3sqrtx: The 3-qubit controlled sqrt-X gate.
/// ```
/// gate c3sqrtx a,b,c,d {
///     h d;
///     cu1(pi/8) a,d;
///     h d; cx a,b;
///     h d;
///     cu1(-pi/8) b,d;
///     h d; cx a,b; h d;
///     cu1(pi/8) b,d;
///     h d;
///     cx b,c;
///     h d;
///     cu1(-pi/8) c,d;
///     h d;
///     cx a,c;
///     h d;
///     cu1(pi/8) c,d;
///     h d;
///     cx b,c;
///     h d;
///     cu1(-pi/8) c,d;
///     h d;
///     cx a,c;
///     h d;
///     cu1(pi/8) c,d;
///     h d;
/// }
/// ```
operation c3sqrtx(a : Qubit, b : Qubit, c : Qubit, target : Qubit) : Unit is Adj + Ctl {
    h(target);
    Controlled u1([a], (PI_OVER_8(), target));
    h(target);
    cx(a, b);
    h(target);
    Controlled u1([b], (NEG_PI_OVER_8(), target));
    h(target);
    cx(a, b);
    h(target);
    Controlled u1([b], (PI_OVER_8(), target));
    h(target);
    cx(b, c);
    h(target);
    Controlled u1([c], (NEG_PI_OVER_8(), target));
    h(target);
    cx(a, c);
    h(target);
    Controlled u1([c], (PI_OVER_8(), target));
    h(target);
    cx(b, c);
    h(target);
    Controlled u1([c], (NEG_PI_OVER_8(), target));
    h(target);
    cx(a, c);
    h(target);
    Controlled u1([c], (PI_OVER_8(), target));
    h(target);
}

/// The X gate controlled on 3 qubits.
/// ```
/// gate c3x a,b,c,d
/// {
///     h d;
///     p(pi/8) a;
///     p(pi/8) b;
///     p(pi/8) c;
///     p(pi/8) d;
///     cx a, b;
///     p(-pi/8) b;
///     cx a, b;
///     cx b, c;
///     p(-pi/8) c;
///     cx a, c;
///     p(pi/8) c;
///     cx b, c;
///     p(-pi/8) c;
///     cx a, c;
///     cx c, d;
///     p(-pi/8) d;
///     cx b, d;
///     p(pi/8) d;
///     cx c, d;
///     p(-pi/8) d;
///     cx a, d;
///     p(pi/8) d;
///     cx c, d;
///     p(-pi/8) d;
///     cx b, d;
///     p(pi/8) d;
///     cx c, d;
///     p(-pi/8) d;
///     cx a, d;
///     h d;
/// }
/// ```
operation c3x(a : Qubit, b : Qubit, c : Qubit, d : Qubit) : Unit is Adj + Ctl {
    h(d);
    p(PI_OVER_8(), a);
    p(PI_OVER_8(), b);
    p(PI_OVER_8(), c);
    p(PI_OVER_8(), d);
    cx(a, b);
    p(NEG_PI_OVER_8(), b);
    cx(a, b);
    cx(b, c);
    p(NEG_PI_OVER_8(), c);
    cx(a, c);
    p(PI_OVER_8(), c);
    cx(b, c);
    p(NEG_PI_OVER_8(), c);
    cx(a, c);
    cx(c, d);
    p(NEG_PI_OVER_8(), d);
    cx(b, d);
    p(PI_OVER_8(), d);
    cx(c, d);
    p(NEG_PI_OVER_8(), d);
    cx(a, d);
    p(PI_OVER_8(), d);
    cx(c, d);
    p(NEG_PI_OVER_8(), d);
    cx(b, d);
    p(PI_OVER_8(), d);
    cx(c, d);
    p(NEG_PI_OVER_8(), d);
    cx(a, d);
    h(d);
}

/// Simplified 3-controlled Toffoli gate.
/// ```
/// gate rc3x a,b,c,d{
///     u2(0,pi) d;
///     u1(pi/4) d;
///     cx c,d;
///     u1(-pi/4) d;
///     u2(0,pi) d;
///     cx a,d;
///     u1(pi/4) d;
///     cx b,d;
///     u1(-pi/4) d;
///     cx a,d;
///     u1(pi/4) d;
///     cx b,d;
///     u1(-pi/4) d;
///     u2(0,pi) d;
///     u1(pi/4) d;
///     cx c,d;
///     u1(-pi/4) d;
///     u2(0,pi) d;
/// }
/// ```
operation rc3x(a : Qubit, b : Qubit, c : Qubit, d : Qubit) : Unit is Adj + Ctl {
    u2(ZERO_ANGLE(), PI_ANGLE(), d);
    u1(PI_OVER_4(), d);
    cx(c, d);
    u1(NEG_PI_OVER_4(), d);
    u2(ZERO_ANGLE(), PI_ANGLE(), d);
    cx(a, d);
    u1(PI_OVER_4(), d);
    cx(b, d);
    u1(NEG_PI_OVER_4(), d);
    cx(a, d);
    u1(PI_OVER_4(), d);
    cx(b, d);
    u1(NEG_PI_OVER_4(), d);
    u2(ZERO_ANGLE(), PI_ANGLE(), d);
    u1(PI_OVER_4(), d);
    cx(c, d);
    u1(NEG_PI_OVER_4(), d);
    u2(ZERO_ANGLE(), PI_ANGLE(), d);
}

/// XX-YY gate.
/// ```
/// gate xx_minus_yy(theta, beta) a, b {
///     rz(-beta) b;
///     rz(-pi/2) a;
///     sx a;
///     rz(pi/2) a;
///     s b;
///     cx a, b;
///     ry(theta/2) a;
///     ry(-theta/2) b;
///     cx a, b;
///     sdg b;
///     rz(-pi/2) a;
///     sxdg a;
///     rz(pi/2) a;
///     rz(beta) b;
/// }
/// ```
operation xx_minus_yy(theta : Angle, beta : Angle, qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
    rz(NegAngle(beta), qubit1);
    rz(NEG_PI_OVER_2(), qubit0);
    sx(qubit0);
    rz(PI_OVER_2(), qubit0);
    s(qubit1);
    cx(qubit0, qubit1);
    ry(DivideAngleByInt(theta, 2), qubit0);
    ry(NegAngle(DivideAngleByInt(theta, 2)), qubit1);
    cx(qubit0, qubit1);
    sdg(qubit1);
    rz(NEG_PI_OVER_2(), qubit0);
    sxdg(qubit0);
    rz(PI_OVER_2(), qubit0);
    rz(beta, qubit1);
}

/// XX+YY gate.
/// ```
/// gate xx_plus_yy(theta, beta) a, b {
///     rz(beta) b;
///     rz(-pi/2) a;
///     sx a;
///     rz(pi/2) a;
///     s b;
///     cx a, b;
///     ry(theta/2) a;
///     ry(theta/2) b;
///     cx a, b;
///     sdg b;
///     rz(-pi/2) a;
///     sxdg a;
///     rz(pi/2) a;
///     rz(-beta) b;
/// }
/// ```
operation xx_plus_yy(theta : Angle, beta : Angle, qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
    rz(beta, qubit1);
    rz(NEG_PI_OVER_2(), qubit0);
    sx(qubit0);
    rz(PI_OVER_2(), qubit0);
    s(qubit1);
    cx(qubit0, qubit1);
    ry(DivideAngleByInt(theta, 2), qubit0);
    ry(DivideAngleByInt(theta, 2), qubit1);
    cx(qubit0, qubit1);
    sdg(qubit1);
    rz(NEG_PI_OVER_2(), qubit0);
    sxdg(qubit0);
    rz(PI_OVER_2(), qubit0);
    rz(NegAngle(beta), qubit1);
}

/// CCZ gate.
/// `gate ccz a,b,c { h c; ccx a,b,c; h c; }`
operation ccz(ctrl1 : Qubit, ctrl2 : Qubit, target : Qubit) : Unit is Adj + Ctl {
    h(target);
    ccx(ctrl1, ctrl2, target);
    h(target);
}

/// A resetting measurement operation that checks for qubit loss.
/// Returns 0 if the qubit measurement was `Zero`, 1 if it was `One`,
/// and 2 if the measurement indicated qubit loss.
operation mresetz_checked(q : Qubit) : Int {
    let (r, b) = Std.Measurement.MResetZChecked(q);
    if b {
        2
    } else {
        Std.OpenQASM.Convert.ResultAsInt(r)
    }
}

/// The ``BARRIER`` function is used to implement the `barrier` statement in QASM.
/// The `@SimulatableIntrinsic` attribute is used to mark the operation for QIR
/// generation.
/// Q# doesn't support barriers, so this is a no-op. We need to figure out what
/// barriers mean in the context of QIR in the future for better support.
@SimulatableIntrinsic()
operation __quantum__qis__barrier__body() : Unit {}
