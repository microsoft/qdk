#!/usr/bin/env node
// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Server-side renderer for Q# visualisation components.
// Reads JSON from stdin, writes SVG/HTML to stdout.
//
// Input format:
//   { "component": "OrbitalEntanglement" | "Histogram" | "Circuit",
//     "props": { ... } }
//
// This file is bundled by esbuild into a self-contained script so that it
// works wherever Node.js is available — no sibling module imports needed.

import { readFileSync } from "node:fs";
import { h } from "preact";
import renderToString from "preact-render-to-string";
import { ChordDiagram, OrbitalEntanglement } from "../../npm/qsharp/ux/orbitalEntanglement.tsx";
import { Histogram } from "../../npm/qsharp/ux/histogram.tsx";
import { draw as drawCircuit } from "../../npm/qsharp/ux/circuit-vis/index.ts";
import { toCircuitGroup } from "../../npm/qsharp/src/data-structures/legacyCircuitUpdate.ts";
import { installSvgDomShim } from "./svgDomShim.mjs";

// ---------- Canonical CSS for standalone SVGs ----------
// Import the same CSS files used by the interactive widgets.  In a
// standalone SVG the :root selector matches the <svg> element, so
// CSS var() fallback chains resolve to their concrete light-mode values
// (e.g. var(--vscode-editor-foreground, var(--jp-widgets-color, #202020))
// → #202020) because none of the host-specific custom properties exist.
import themeCss from "../../npm/qsharp/ux/qdk-theme.css";
import uxCss from "../../npm/qsharp/ux/qsharp-ux.css";
import circuitCss from "../../npm/qsharp/ux/qsharp-circuit.css";

const input = readFileSync(0, "utf-8"); // stdin
const { component, props } = JSON.parse(input);

let output = "";

switch (component) {
  // ---- ChordDiagram (generic chord diagram, pure Preact SVG) ----
  case "ChordDiagram": {
    const vnode = h(ChordDiagram, { ...props, static: true });
    output = renderToString(vnode);
    break;
  }

  // ---- OrbitalEntanglement (pure Preact SVG) ----
  case "OrbitalEntanglement": {
    const vnode = h(OrbitalEntanglement, { ...props, static: true });
    output = renderToString(vnode);
    break;
  }

  // ---- Histogram (pure Preact SVG) ----
  case "Histogram": {
    // The TS component expects `data` as a Map, but JSON gives us an object.
    const histProps = {
      ...props,
      data: new Map(Object.entries(props.data)),
      onFilter: () => {},
    };
    const vnode = h(Histogram, histProps);
    let html = renderToString(vnode);

    // Strip any <h4>...</h4> shots header that precedes the SVG
    html = html.replace(/^<h4[^>]*>[\s\S]*?<\/h4>/, "");

    // Remove interactive elements that shouldn't appear in static export:
    //   - settings icon:  <g class="menu-icon" ...>...</g>
    //   - info icon:      <g class="menu-icon" ...>...</g>
    //   - dropdown menu:  <g id="menu" ...>...</g>
    //   - help-info pane: <g style="display: none;">...</g> (last one)
    html = html.replace(/<g\s+class="menu-icon"[^>]*>[\s\S]*?<\/g>/g, "");
    html = html.replace(/<g\s+id="menu"[^>]*>[\s\S]*?<\/g>/g, "");
    // The help-info is a <g style="display: none;"> wrapping nested elements –
    // match from the last <g style="display: none;"> to the closing </svg>
    html = html.replace(
      /<g\s+style="display:\s*none;"[^>]*>[\s\S]*?<\/g>\s*(?=<\/svg>)/,
      "",
    );

    // Add xmlns for standalone SVG and set a reasonable default render size
    html = html.replace(
      /<svg\s+class="histogram"/,
      '<svg xmlns="http://www.w3.org/2000/svg" class="histogram" width="600"',
    );

    // Inject canonical CSS so the SVG renders correctly standalone
    html = html.replace(
      /(<svg\s[^>]*>)/,
      `$1<defs><style>${themeCss}\n${uxCss}</style></defs>`,
    );

    output = html;
    break;
  }

  // ---- Circuit (imperative DOM via qviz) ----
  case "Circuit": {
    // qviz.draw() uses the DOM API internally.  Install a minimal SVG DOM
    // shim so that it works without jsdom or any external dependency.
    const restore = installSvgDomShim();

    try {
      const container = document.createElement("div");
      const circuitData =
        typeof props.circuit === "string"
          ? JSON.parse(props.circuit)
          : props.circuit;

      // Normalise any legacy/raw format into a proper CircuitGroup
      const result = toCircuitGroup(circuitData);
      if (!result.ok) {
        process.stderr.write(`Circuit conversion error: ${result.error}\n`);
        process.exit(1);
      }

      drawCircuit(result.circuitGroup, container);

      // The rendered SVG is the first child with class "qviz"
      const svg = container.querySelector("svg.qviz");
      if (svg) {
        // Ensure the xmlns is present for standalone SVG files
        svg.setAttribute("xmlns", "http://www.w3.org/2000/svg");
        // Add qs-circuit class so the nested circuit CSS selectors match
        const existing = svg.getAttribute("class") || "";
        svg.setAttribute("class", `qs-circuit ${existing}`.trim());
        // Inject canonical CSS so the SVG renders correctly standalone
        const styleEl = document.createElementNS(
          "http://www.w3.org/2000/svg",
          "style",
        );
        styleEl.textContent = `${themeCss}\n${uxCss}\n${circuitCss}`;
        const defs = document.createElementNS(
          "http://www.w3.org/2000/svg",
          "defs",
        );
        defs.appendChild(styleEl);
        svg.insertBefore(defs, svg.firstChild);
        output = svg.outerHTML;
      } else {
        // Fallback: return the full container HTML
        output = container.innerHTML;
      }
    } finally {
      restore();
    }
    break;
  }

  default:
    process.stderr.write(`Unknown component: ${component}\n`);
    process.exit(1);
}

process.stdout.write(output);
