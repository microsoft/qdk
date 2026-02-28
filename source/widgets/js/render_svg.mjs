#!/usr/bin/env node
// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Server-side renderer for Q# visualisation components.
// Reads JSON from stdin, writes SVG/HTML to stdout.
//
// Input format:
//   { "component": "ChordDiagram" | "Histogram",
//     "props": { ... } }
//
// This file is bundled by esbuild into a self-contained script so that it
// works wherever Node.js is available — no sibling module imports needed.

import { readFileSync } from "node:fs";
import { chordDiagramToSvg } from "../../npm/qsharp/ux/orbitalEntanglement.tsx";
import { histogramToSvg } from "../../npm/qsharp/ux/histogram.tsx";

const input = readFileSync(0, "utf-8"); // stdin
const { component, props } = JSON.parse(input);

let output = "";

switch (component) {
  // ---- ChordDiagram (generic chord diagram, pure Preact SVG) ----
  case "ChordDiagram": {
    output = chordDiagramToSvg(props);
    break;
  }

  // ---- Histogram (pure Preact SVG) ----
  case "Histogram": {
    // The TS component expects `data` as a Map, but JSON gives us an object.
    const histProps = {
      ...props,
      data: new Map(Object.entries(props.data)),
    };
    output = histogramToSvg(histProps);
    break;
  }

  default:
    process.stderr.write(`Unknown component: ${component}\n`);
    process.exit(1);
}

process.stdout.write(output);
