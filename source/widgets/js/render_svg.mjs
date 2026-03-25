#!/usr/bin/env node
// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Server-side renderer for Q# visualisation components.
// Reads JSON from stdin, writes SVG/HTML to stdout.
//
// Input format:
//   { "component": "ChordDiagram" | "Histogram" | "Circuit" | "OrbitalEntanglement",
//     "props": { ... } }
//
// This file is bundled by esbuild into a self-contained script so that it
// works wherever Node.js is available — no sibling module imports needed.

import { readFileSync } from "node:fs";
import { orbitalEntanglementToSvg } from "../../npm/qsharp/ux/orbitalEntanglement.tsx";
import { histogramToSvg } from "../../npm/qsharp/ux/histogram.tsx";
import { circuitToSvg } from "../../npm/qsharp/ux/circuitToSvg.ts";

const input = readFileSync(0, "utf-8"); // stdin
const { component, props } = JSON.parse(input);

let output = "";

switch (component) {
  // ---- OrbitalEntanglement (chord diagram) ----
  case "ChordDiagram":
  case "OrbitalEntanglement": {
    output = orbitalEntanglementToSvg(props);
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

  // ---- Circuit (pure string SVG, no DOM needed) ----
  case "Circuit": {
    const circuitData =
      typeof props.circuit === "string"
        ? JSON.parse(props.circuit)
        : props.circuit;
    output = circuitToSvg(circuitData, {
      gatesPerRow: props.gates_per_row ?? 0,
      darkMode: props.dark_mode ?? false,
      renderDepth: props.render_depth ?? 0,
    });
    break;
  }

  default:
    process.stderr.write(`Unknown component: ${component}\n`);
    process.exit(1);
}

process.stdout.write(output);
