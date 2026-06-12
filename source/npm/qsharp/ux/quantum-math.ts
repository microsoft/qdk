// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Basic classes for complex numbers, 2-vectors, and 2x2 matrices, plus the
// canonical single-qubit gate constants and computational-basis kets.
//
// IMPORTANT: this module must remain free of `three`, DOM, and any other
// browser-only or heavyweight imports. It is consumed by the state-viz
// state-compute worker (which is bundled into a Web Worker chunk) and by
// the Bloch sphere widget; pulling `three` in here would silently bloat
// the worker bundle by ~600 KB. Three.js-dependent code (Rotations,
// AppliedGate, PathEntry) lives in `./cplx.ts`, which re-exports
// everything below for back-compat.

const epsilon = 0.000001; // Tolerance when comparing numbers
export function compare(a: number, b: number, eps = epsilon): boolean {
  return Math.abs(a - b) < eps;
}

const fmt = new Intl.NumberFormat("en-US", { maximumFractionDigits: 4 });
export const numToStr = (n: number) => fmt.format(n);

// Some common numbers after various gates to display in LaTeX
// - 1 / sqrt(2) = 0.7071067811865476 = sqrt(2) / 2
// - 1 / (2 * sqrt(2)) = 0.3535533905932738 = sqrt(2) / 4
// - (1 + sqrt(2)) / (2 * sqrt(2)) = 0.8535533905932737 = cos^2(pi / 8)
// - (-1 + sqrt(2)) / (2 * sqrt(2)) = 0.14644660940672624 = sin^2(pi / 8)
function numToLaTeX(n: number, forceSign = false): string {
  let result = "";
  if (n < 0) {
    result += "- {";
  } else if (forceSign) {
    result += "+ {";
  } else {
    result += "{";
  }

  n = Math.abs(n);
  if (compare(n, 0)) {
    result += "0";
  } else if (compare(n, 1)) {
    result += "1";
  } else if (compare(n, 0.5)) {
    result += "1 \\over 2";
  } else if (compare(n, 0.25)) {
    result += "1 \\over 4";
  } else if (compare(n, 0.75)) {
    result += "3 \\over 4";
  } else if (compare(n, Math.SQRT1_2)) {
    // result += "\\sqrt{2} \\over 2";
    result += "1 \\over \\sqrt{2}";
  } else if (compare(n, 0.1464466094)) {
    result += "2 - \\sqrt{2} \\over 4";
    // result += "{-1 + \\sqrt{2}} \\over 2 \\sqrt{2}";
    // result += "\\sin^2{\\pi \\over 8}";
  } else if (compare(n, 0.3535533905)) {
    // result += "1 \\over {2 \\sqrt{2}}";
    result += "\\sqrt{2} \\over 4";
  } else if (compare(n, 0.8535533905)) {
    result += "{2 + \\sqrt{2}} \\over 4";
    // result += "{1 + \\sqrt{2}} \\over {2 \\sqrt{2}}";
    // result += "\\cos^2{\\pi \\over 8}";
  } else if (compare(n, 0.8660254037844386)) {
    result += "\\sqrt{3} \\over 2";
  } else {
    result += numToStr(n);
  }

  return result + "}";
}

export class Cplx {
  constructor(
    public re: number,
    public im: number,
  ) {}

  static zero = new Cplx(0, 0);
  static one = new Cplx(1, 0);
  static i = new Cplx(0, 1);
  static negOne = new Cplx(-1, 0);
  static negI = new Cplx(0, -1);
  static e_to_i_pi_over_2 = new Cplx(0, 1);
  static e_to_i_pi_over_4 = new Cplx(Math.SQRT1_2, Math.SQRT1_2);

  add(c: Cplx): Cplx {
    return new Cplx(this.re + c.re, this.im + c.im);
  }

  sub(c: Cplx): Cplx {
    return new Cplx(this.re - c.re, this.im - c.im);
  }

  mul(c: Cplx | number): Cplx {
    if (typeof c === "number") {
      return new Cplx(this.re * c, this.im * c);
    } else {
      // FOIL: (a + bi)(c + di) = ac + adi + bci + bdi^2
      return new Cplx(
        this.re * c.re - this.im * c.im,
        this.re * c.im + this.im * c.re,
      );
    }
  }

  mag(): number {
    return Math.sqrt(this.re ** 2 + this.im ** 2);
  }

  isUnit(): boolean {
    return Math.abs(this.mag() - 1.0) < epsilon;
  }

  norm(): Cplx {
    if (this.isUnit()) {
      return new Cplx(this.re, this.im);
    } else {
      return this.mul(1 / this.mag());
    }
  }

  conj(): Cplx {
    return new Cplx(this.re, -this.im);
  }

  compare(c: Cplx, eps = epsilon): boolean {
    return compare(this.re, c.re, eps) && compare(this.im, c.im, eps);
  }

  toPolar() {
    const magnitude = Math.sqrt(this.re * this.re + this.im * this.im);
    const phase = Math.atan2(this.im, this.re);
    return { magnitude, phase };
  }

  static fromPolar(magnitude: number, phase: number) {
    const result = new Cplx(Math.cos(phase), Math.sin(phase)).mul(magnitude);
    return result;
  }

  static parse(input: string): Cplx | null {
    // Valid formats: "0", "1", "i", "-i", "-1.2+3i", "-2i", etc.

    // Removal all whitespace
    input = input.replace(/\s/g, "");

    // Handle special and common cases
    switch (input) {
      case "0":
        return new Cplx(0, 0);
      case "1":
        return new Cplx(1, 0);
      case "i":
        return new Cplx(0, 1);
      case "-1":
        return new Cplx(-1, 0);
      case "-i":
        return new Cplx(0, -1);
      default:
        break;
    }

    // Regular expressions to parse a complex number
    // - Optional leading '-' sign
    // - Numeric value for real part (optional if imaginary part is present)
    // - Optional numeric imaginary part with leading [+-] and trailing 'i'
    //   - If real part is not present, leading '+' on imaginary part is optional
    // - Numeric parts are an integer followed by an optional decimal part
    // - If decimal part is present, it must contain at least one digit
    // - Scientific notation is not supported

    // To ease parsing, look for the real and imaginary parts separately
    const rePart = /^[-]?(\d+)(\.\d+)?(?=$|[+-])/;
    const imPart = /(^|[+-])(\d+)(\.\d+)?(?=i$)/;

    const reMatch = input.match(rePart);
    const imMatch = input.match(imPart);
    if (!reMatch && !imMatch) {
      return null;
    }

    const reVal = parseFloat(reMatch?.[0] ?? "0");
    const imVal = parseFloat(imMatch?.[0] ?? "0");
    return new Cplx(reVal, imVal);
  }

  toString() {
    const reTo4 = numToStr(this.re);
    const imTo4 = compare(this.im, 1)
      ? ""
      : compare(this.im, -1)
        ? "-"
        : numToStr(this.im);

    if (compare(this.im, 0)) {
      return reTo4;
    } else if (compare(this.re, 0)) {
      return `${imTo4}${"i"}`;
    } else {
      return `${reTo4}${this.im > 0 ? "+" : ""}${imTo4}i`;
    }
  }

  toLaTeX() {
    if (this.compare(Cplx.zero)) {
      return "{0}";
    } else if (this.compare(Cplx.one)) {
      return "{1}";
    } else if (this.compare(Cplx.i)) {
      return "{i}";
    } else if (this.compare(Cplx.negOne)) {
      return "{-1}";
    } else if (this.compare(Cplx.negI)) {
      return "{-i}";
    } else if (this.compare(Cplx.e_to_i_pi_over_2)) {
      return "{e^{i {\\pi \\over 2}}}";
    } else if (this.compare(Cplx.e_to_i_pi_over_4)) {
      return "{e^{i {\\pi \\over 4}}}";
    } else if (compare(this.re, 0)) {
      return numToLaTeX(this.im) + "i";
    } else if (compare(this.im, 0)) {
      return numToLaTeX(this.re);
    } else {
      return `${numToLaTeX(this.re)} ${numToLaTeX(this.im, true)}i`;
    }
  }
}

// Convenience constructor accepting a number, an existing Cplx, or a textbook-
// style string like "1+2i" / "-i" / "3.5". String parsing delegates to
// `Cplx.parse`; it's deliberately lenient so the matrix/vector factories below
// can also accept string inputs split on commas.
export function cplx(x: number | string | Cplx): Cplx {
  if (typeof x === "number") {
    return new Cplx(x, 0);
  } else if (typeof x === "string") {
    const result = Cplx.parse(x);
    if (result) {
      return result;
    } else {
      throw Error("Invalid cplx string");
    }
  } else if (x instanceof Cplx) {
    return new Cplx(x.re, x.im);
  } else {
    throw Error("Invalid cplx parameter");
  }
}

export class Vec2 {
  constructor(
    public x: Cplx,
    public y: Cplx,
  ) {}
  add(v: Vec2): Vec2 {
    return new Vec2(this.x.add(v.x), this.y.add(v.y));
  }

  sub(v: Vec2): Vec2 {
    return new Vec2(this.x.sub(v.x), this.y.sub(v.y));
  }

  mul(v: number): Vec2 {
    return new Vec2(this.x.mul(v), this.y.mul(v));
  }

  innerProduct(v: Vec2): Cplx {
    return this.x.mul(v.x).add(this.y.mul(v.y));
  }

  outerProduct(v: Vec2): M2x2 {
    return new M2x2(
      this.x.mul(v.x),
      this.x.mul(v.y),
      this.y.mul(v.x),
      this.y.mul(v.y),
    );
  }

  remove0Phase(): Vec2 {
    const p0 = this.x.toPolar();
    const p1 = this.y.toPolar();

    return new Vec2(
      Cplx.fromPolar(p0.magnitude, 0),
      Cplx.fromPolar(p1.magnitude, p1.phase - p0.phase),
    );
  }

  compare(v: Vec2): boolean {
    return this.x.compare(v.x) && this.y.compare(v.y);
  }

  toLaTeX() {
    return `\\begin{bmatrix} ${this.x.toLaTeX()} \\\\ ${this.y.toLaTeX()} \\end{bmatrix}`;
  }

  toString() {
    return `[${this.x},${this.y}]`;
  }
}

// Convenience constructor for `Vec2` accepting either an existing Vec2, an
// array of two numeric/Cplx entries, or a comma-separated textbook string like
// `"1,i"`. The string form is mostly useful in tests and ad-hoc declarations
// where matching textbook notation matters more than micro-optimizing module
// init.
export function vec2(input: Vec2 | string | (number | Cplx)[]): Vec2 {
  if (input instanceof Vec2) {
    return new Vec2(input.x, input.y);
  } else if (Array.isArray(input) && input.length === 2) {
    return new Vec2(cplx(input[0]), cplx(input[1]));
  } else if (typeof input === "string") {
    const parts = input.split(",");
    if (parts.length === 2) {
      return new Vec2(cplx(parts[0]), cplx(parts[1]));
    }
  }
  throw Error("Invalid input parameter to vec2");
}

// A 2x2 matrix of complex numbers
export class M2x2 {
  constructor(
    public a: Cplx,
    public b: Cplx,
    public c: Cplx,
    public d: Cplx,
  ) {}

  add(m: M2x2): M2x2 {
    return new M2x2(
      this.a.add(m.a),
      this.b.add(m.b),
      this.c.add(m.c),
      this.d.add(m.d),
    );
  }

  sub(m: M2x2): M2x2 {
    return new M2x2(
      this.a.sub(m.a),
      this.b.sub(m.b),
      this.c.sub(m.c),
      this.d.sub(m.d),
    );
  }

  mul(m: M2x2 | Cplx | number): M2x2 {
    if (typeof m === "number") m = cplx(m);
    if (m instanceof Cplx) {
      return new M2x2(
        this.a.mul(m),
        this.b.mul(m),
        this.c.mul(m),
        this.d.mul(m),
      );
    } else {
      return new M2x2(
        this.a.mul(m.a).add(this.b.mul(m.c)),
        this.a.mul(m.b).add(this.b.mul(m.d)),
        this.c.mul(m.a).add(this.d.mul(m.c)),
        this.c.mul(m.b).add(this.d.mul(m.d)),
      );
    }
  }

  mulVec2(v: Vec2): Vec2 {
    return new Vec2(
      this.a.mul(v.x).add(this.b.mul(v.y)),
      this.c.mul(v.x).add(this.d.mul(v.y)),
    );
  }

  compare(m: M2x2): boolean {
    return (
      this.a.compare(m.a) &&
      this.b.compare(m.b) &&
      this.c.compare(m.c) &&
      this.d.compare(m.d)
    );
  }

  det(): Cplx {
    return this.a.mul(this.d).sub(this.b.mul(this.c));
  }

  transpose(): M2x2 {
    return new M2x2(this.a, this.c, this.b, this.d);
  }

  adjoint(): M2x2 {
    return new M2x2(this.a.conj(), this.c.conj(), this.b.conj(), this.d.conj());
  }

  isDiagonal(eps = epsilon): boolean {
    return this.b.compare(Cplx.zero, eps) && this.c.compare(Cplx.zero, eps);
  }

  phase(): number {
    const pa = this.a.toPolar();
    const pd = this.d.toPolar();
    let phase = pd.phase - pa.phase;
    if (phase < 0) phase += Math.PI * 2;
    return phase;
  }

  toString() {
    return `[
  ${this.a.toString()}, ${this.b.toString()}
  ${this.c.toString()}, ${this.d.toString()}
]`;
  }

  toShortString() {
    return `[[${this.a.toString()},${this.b.toString()}],[${this.c.toString()},${this.d.toString()}]]`;
  }
}

// Convenience constructor for `M2x2` accepting either an existing M2x2, an
// array of four numeric/Cplx entries in row-major order, or a comma-separated
// textbook string like `"0, 1, 1, 0"`. Same rationale as `vec2(string)`: kept
// for tests and ad-hoc one-offs where matching the matrix's visual layout
// matters more than module-init speed.
export function m2x2(v: M2x2 | (number | Cplx)[] | string): M2x2 {
  if (v instanceof M2x2) {
    return new M2x2(v.a, v.b, v.c, v.d);
  } else if (Array.isArray(v) && v.length === 4) {
    return new M2x2(cplx(v[0]), cplx(v[1]), cplx(v[2]), cplx(v[3]));
  } else if (typeof v === "string") {
    const parts = v.split(",");
    if (parts.length === 4) {
      return new M2x2(
        cplx(parts[0]),
        cplx(parts[1]),
        cplx(parts[2]),
        cplx(parts[3]),
      );
    }
  }
  throw Error("Invalid input paramter to m2x2");
}

// I = [[1, 0], [0, 1]]
export const Ident = new M2x2(Cplx.one, Cplx.zero, Cplx.zero, Cplx.one);

// X = [[0, 1], [1, 0]]
export const PauliX = new M2x2(Cplx.zero, Cplx.one, Cplx.one, Cplx.zero);

// Y = [[0, -i], [i, 0]]
export const PauliY = new M2x2(Cplx.zero, Cplx.negI, Cplx.i, Cplx.zero);

// Z = [[1, 0], [0, -1]]
export const PauliZ = new M2x2(Cplx.one, Cplx.zero, Cplx.zero, Cplx.negOne);

// H = 1/sqrt(2) * [[1, 1], [1, -1]]
export const Hadamard = new M2x2(
  new Cplx(Math.SQRT1_2, 0),
  new Cplx(Math.SQRT1_2, 0),
  new Cplx(Math.SQRT1_2, 0),
  new Cplx(-Math.SQRT1_2, 0),
);

// S = [[1, 0], [0, i]]
export const SGate = new M2x2(Cplx.one, Cplx.zero, Cplx.zero, Cplx.i);

// T = [[1, 0], [0, e^{i pi/4}]]
export const TGate = new M2x2(
  Cplx.one,
  Cplx.zero,
  Cplx.zero,
  Cplx.e_to_i_pi_over_4,
);

// Square root of X (sometimes written V). Half-rotation around the Bloch X
// axis: (1+i)/2 on the diagonal, (1-i)/2 off-diagonal.
//   SX = 1/2 * [[1+i, 1-i], [1-i, 1+i]]
export const SXGate = new M2x2(
  new Cplx(0.5, 0.5),
  new Cplx(0.5, -0.5),
  new Cplx(0.5, -0.5),
  new Cplx(0.5, 0.5),
);

// Single-qubit rotation gates parameterized by angle theta.
// Rx(θ) = [[cos(θ/2),  -i sin(θ/2)], [-i sin(θ/2), cos(θ/2)]]
export function rotationX(theta: number): M2x2 {
  const c = Math.cos(theta / 2);
  const s = Math.sin(theta / 2);
  return new M2x2(
    new Cplx(c, 0),
    new Cplx(0, -s),
    new Cplx(0, -s),
    new Cplx(c, 0),
  );
}

// Ry(θ) = [[cos(θ/2), -sin(θ/2)], [sin(θ/2), cos(θ/2)]]
export function rotationY(theta: number): M2x2 {
  const c = Math.cos(theta / 2);
  const s = Math.sin(theta / 2);
  return new M2x2(
    new Cplx(c, 0),
    new Cplx(-s, 0),
    new Cplx(s, 0),
    new Cplx(c, 0),
  );
}

// Rz(θ) = [[e^{-iθ/2}, 0], [0, e^{iθ/2}]]
export function rotationZ(theta: number): M2x2 {
  const eNeg = new Cplx(Math.cos(-theta / 2), Math.sin(-theta / 2));
  const ePos = new Cplx(Math.cos(theta / 2), Math.sin(theta / 2));
  return new M2x2(eNeg, Cplx.zero, Cplx.zero, ePos);
}

// --- Computational-basis kets ---
// |0> = [1, 0],  |1> = [0, 1]
export const Ket0 = new Vec2(Cplx.one, Cplx.zero);
export const Ket1 = new Vec2(Cplx.zero, Cplx.one);

// |+>  = 1/sqrt(2) * [1,  1]
// |->  = 1/sqrt(2) * [1, -1]
// |+i> = 1/sqrt(2) * [1,  i]
// |-i> = 1/sqrt(2) * [1, -i]
const inv_sqrt_2 = new Cplx(Math.SQRT1_2, 0);
const neg_inv_sqrt_2 = new Cplx(-Math.SQRT1_2, 0);
const i_inv_sqrt_2 = new Cplx(0, Math.SQRT1_2);
const neg_i_inv_sqrt_2 = new Cplx(0, -Math.SQRT1_2);

export const KetPlus = new Vec2(inv_sqrt_2, inv_sqrt_2);
export const KetMinus = new Vec2(inv_sqrt_2, neg_inv_sqrt_2);
export const KetPlusI = new Vec2(inv_sqrt_2, i_inv_sqrt_2);
export const KetMinusI = new Vec2(inv_sqrt_2, neg_i_inv_sqrt_2);
