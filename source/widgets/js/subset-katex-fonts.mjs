// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { readFile, writeFile } from "node:fs/promises";

const cssPath = new URL(
  "../src/qsharp_widgets/static/bloch.css",
  import.meta.url,
);

// Chromium loads only these faces for the complete set of expressions emitted
// by the Bloch sphere. Keep this list descriptor-specific: KaTeX uses the same
// family name for multiple styles and weights.
const requiredFaces = new Set([
  "KaTeX_Main|normal|400",
  "KaTeX_Math|italic|400",
  "KaTeX_Size3|normal|400",
  "KaTeX_Size4|normal|400",
]);

const css = await readFile(cssPath, "utf8");
const fontFacePattern = /@font-face\{[^}]*\}/g;
const foundRequiredFaces = new Set();
let totalFaces = 0;
let removedFaces = 0;

const subsetCss = css.replace(fontFacePattern, (fontFace) => {
  totalFaces += 1;

  const family = readDescriptor(fontFace, "font-family");
  const style = readDescriptor(fontFace, "font-style") ?? "normal";
  const weight = readDescriptor(fontFace, "font-weight") ?? "400";
  const key = `${family}|${style}|${weight}`;

  if (requiredFaces.has(key)) {
    foundRequiredFaces.add(key);
    return fontFace;
  }

  removedFaces += 1;
  return "";
});

const missingFaces = [...requiredFaces].filter(
  (face) => !foundRequiredFaces.has(face),
);
if (missingFaces.length > 0) {
  throw new Error(
    `KaTeX CSS is missing required Bloch font faces: ${missingFaces.join(", ")}`,
  );
}
if (totalFaces === 0 || removedFaces === 0) {
  throw new Error(
    `Expected bundled KaTeX font faces to subset; found ${totalFaces}, removed ${removedFaces}`,
  );
}

await writeFile(cssPath, subsetCss);
console.log(
  `Subset KaTeX fonts in bloch.css: kept ${foundRequiredFaces.size}, removed ${removedFaces}`,
);

function readDescriptor(fontFace, descriptor) {
  const match = fontFace.match(new RegExp(`${descriptor}:([^;}]+)`));
  return match?.[1];
}
