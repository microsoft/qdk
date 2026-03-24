// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Normalizes "pi" (any case) to "π" and trims whitespace for
// consistent storage and display.
export function normalizeAngleExpression(expr: string): string {
  return expr.trim().replace(/pi/gi, "π");
}

// Evaluate a simple arithmetic expression supporting numbers, + - * /, parentheses, and π.
// Returns `undefined` for invalid inputs.
export function evaluateAngleExpression(expr: string): number | undefined {
  if (!expr) return undefined;
  const src = normalizeAngleExpression(expr);
  if (!src) return undefined;

  // Tokenizer
  type Tok = { type: "num" | "pi" | "op" | "lpar" | "rpar"; value?: string };
  const toks: Tok[] = [];
  let i = 0;
  while (i < src.length) {
    const ch = src[i];
    if (ch === " " || ch === "\t" || ch === "\n") {
      i++;
      continue;
    }
    if (ch === "(" || ch === ")") {
      toks.push({ type: ch === "(" ? "lpar" : "rpar" });
      i++;
      continue;
    }
    if (ch === "+" || ch === "-" || ch === "*" || ch === "/") {
      toks.push({ type: "op", value: ch });
      i++;
      continue;
    }
    if (ch === "π") {
      toks.push({ type: "pi" });
      i++;
      continue;
    }
    // number: digits with optional decimal part; no leading dot
    if (ch === "." || /\d/.test(ch)) {
      let j = i + 1;
      while (j < src.length && /[0-9.]/.test(src[j])) j++;
      const numStr = src.slice(i, j);
      const valid = /^(?:\d+(?:\.\d*)?)$/.test(numStr);
      if (!valid) return undefined;
      toks.push({ type: "num", value: numStr });
      i = j;
      continue;
    }
    // Unknown character
    return undefined;
  }

  // Recursive descent parser
  let k = 0;
  const peek = () => toks[k];
  const consume = () => toks[k++];

  const parseExpr = (): number | undefined => {
    let lhs = parseTerm();
    if (lhs === undefined) return undefined;
    while (
      peek() &&
      peek().type === "op" &&
      (peek().value === "+" || peek().value === "-")
    ) {
      const op = consume().value!;
      const rhs = parseTerm();
      if (rhs === undefined) return undefined;
      lhs = op === "+" ? lhs + rhs : lhs - rhs;
    }
    return lhs;
  };

  const parseTerm = (): number | undefined => {
    let lhs = parseFactor();
    if (lhs === undefined) return undefined;
    while (
      peek() &&
      peek().type === "op" &&
      (peek().value === "*" || peek().value === "/")
    ) {
      const op = consume().value!;
      const rhs = parseFactor();
      if (rhs === undefined) return undefined;
      lhs = op === "*" ? lhs * rhs : lhs / rhs;
    }
    return lhs;
  };

  const parseFactor = (): number | undefined => {
    let sign = 1;
    if (
      peek() &&
      peek().type === "op" &&
      (peek().value === "+" || peek().value === "-")
    ) {
      sign = consume().value! === "-" ? -1 : 1;
    }

    const t = peek();
    if (!t) return undefined;
    if (t.type === "num") {
      consume();
      return sign * parseFloat(t.value!);
    }
    if (t.type === "pi") {
      consume();
      return sign * Math.PI;
    }
    if (t.type === "lpar") {
      consume();
      const v = parseExpr();
      if (peek() && peek().type === "rpar") consume();
      else return undefined;
      if (v === undefined) return undefined;
      return sign * v;
    }
    return undefined;
  };

  const result = parseExpr();
  if (result === undefined || k !== toks.length || !isFinite(result))
    return undefined;
  return result;
}

export function isValidAngleExpression(expr: string): boolean {
  return evaluateAngleExpression(expr) !== undefined;
}
