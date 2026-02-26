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
import { OrbitalEntanglement } from "../../npm/qsharp/ux/orbitalEntanglement.tsx";
import { Histogram } from "../../npm/qsharp/ux/histogram.tsx";
import { draw as drawCircuit } from "../../npm/qsharp/ux/circuit-vis/index.ts";
import { toCircuitGroup } from "../../npm/qsharp/src/data-structures/legacyCircuitUpdate.ts";

// ---------- Embedded CSS for standalone SVGs ----------
// The interactive widgets inherit CSS from the page's theme variables.
// For standalone SVG export we resolve those variables to concrete values.

const HISTOGRAM_CSS_LIGHT = `
  .bar { fill: #8ab8ff; }
  .bar-label { font-size: 3pt; fill: #000; text-anchor: end; pointer-events: none; }
  .bar-label-ket { font-family: Consolas, "Menlo", monospace; font-variant-ligatures: none; }
  .histo-label { font-size: 3.5pt; fill: #222; }
  .hover-text { font-size: 3.5pt; fill: #222; text-anchor: middle; }
`;

const HISTOGRAM_CSS_DARK = `
  .bar { fill: #4aa3ff; }
  .bar-label { font-size: 3pt; fill: #fff; text-anchor: end; pointer-events: none; }
  .bar-label-ket { font-family: Consolas, "Menlo", monospace; font-variant-ligatures: none; }
  .histo-label { font-size: 3.5pt; fill: #eee; }
  .hover-text { font-size: 3.5pt; fill: #eee; text-anchor: middle; }
`;

const CIRCUIT_CSS_LIGHT = `
  line, circle, rect { stroke: #202020; stroke-width: 1; }
  text { fill: #202020; dominant-baseline: middle; text-anchor: middle;
         user-select: none; pointer-events: none; }
  .qs-maintext { font-family: "KaTeX_Main", sans-serif; font-style: normal; }
  .qs-mathtext { font-family: "KaTeX_Math", serif; }
  .gate .qs-group-label { fill: #202020; text-anchor: start; }
  .gate-unitary { fill: #333333; }
  .gate text { fill: #ffffff; }
  .control-line, .control-dot { fill: #202020; }
  .oplus > line, .oplus > circle { fill: #ececf0; stroke: #202020; stroke-width: 2; }
  .gate-measure { fill: #007acc; }
  .qs-line-measure, .arc-measure { stroke: #ffffff; fill: none; stroke-width: 1; }
  .gate-ket { fill: #007acc; }
  text.ket-text { fill: #ffffff; stroke: none; }
  rect.gate-swap { fill: transparent; stroke: transparent; }
  .register-classical { stroke-width: 0.5; }
  .qubit-wire { stroke: #202020; }
  .qs-qubit-label { fill: #202020; }
  .gate-collapse circle, .gate-expand circle { fill: white; stroke-width: 2px; stroke: black; }
  .gate-collapse path, .gate-expand path { stroke-width: 4px; stroke: black; }
  .classical-container { stroke-dasharray: 8, 8; fill-opacity: 0; }
  .classically-controlled-btn circle { fill: #ececf0; stroke-width: 1; }
  .classically-controlled-btn text { dominant-baseline: middle; text-anchor: middle;
    stroke: none; font-family: "KaTeX_Main", sans-serif; fill: #202020; }
`;

const CIRCUIT_CSS_DARK = `
  line, circle, rect { stroke: #d4d4d4; stroke-width: 1; }
  text { fill: #d4d4d4; dominant-baseline: middle; text-anchor: middle;
         user-select: none; pointer-events: none; }
  .qs-maintext { font-family: "KaTeX_Main", sans-serif; font-style: normal; }
  .qs-mathtext { font-family: "KaTeX_Math", serif; }
  .gate .qs-group-label { fill: #d4d4d4; text-anchor: start; }
  .gate-unitary { fill: #3c3c3c; }
  .gate text { fill: #ffffff; }
  .control-line, .control-dot { fill: #d4d4d4; }
  .oplus > line, .oplus > circle { fill: #1e1e1e; stroke: #d4d4d4; stroke-width: 2; }
  .gate-measure { fill: #0e639c; }
  .qs-line-measure, .arc-measure { stroke: #ffffff; fill: none; stroke-width: 1; }
  .gate-ket { fill: #0e639c; }
  text.ket-text { fill: #ffffff; stroke: none; }
  rect.gate-swap { fill: transparent; stroke: transparent; }
  .register-classical { stroke-width: 0.5; }
  .qubit-wire { stroke: #d4d4d4; }
  .qs-qubit-label { fill: #d4d4d4; }
  .gate-collapse circle, .gate-expand circle { fill: #1e1e1e; stroke-width: 2px; stroke: #d4d4d4; }
  .gate-collapse path, .gate-expand path { stroke-width: 4px; stroke: #d4d4d4; }
  .classical-container { stroke-dasharray: 8, 8; fill-opacity: 0; }
  .classically-controlled-btn circle { fill: #1e1e1e; stroke-width: 1; }
  .classically-controlled-btn text { dominant-baseline: middle; text-anchor: middle;
    stroke: none; font-family: "KaTeX_Main", sans-serif; fill: #d4d4d4; }
`;

/** Inject a <defs><style> block as the first child of an SVG string. */
function injectSvgStyle(svgString, css) {
  const styleBlock = `<defs><style>${css}</style></defs>`;
  // Insert right after the opening <svg ...> tag
  return svgString.replace(/>/, `>${styleBlock}`);
}

const input = readFileSync(0, "utf-8"); // stdin
const { component, props } = JSON.parse(input);

let output = "";

switch (component) {
  // ---- OrbitalEntanglement (pure Preact SVG) ----
  case "OrbitalEntanglement": {
    const vnode = h(OrbitalEntanglement, props);
    output = renderToString(vnode);
    break;
  }

  // ---- Histogram (pure Preact SVG) ----
  case "Histogram": {
    // The TS component expects `data` as a Map, but JSON gives us an object.
    const dark = !!props.darkMode;
    const histProps = {
      ...props,
      data: new Map(Object.entries(props.data)),
      onFilter: () => {},
    };
    delete histProps.darkMode;
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
    html = html.replace(/<g\s+style="display:\s*none;"[^>]*>[\s\S]*?<\/g>\s*(?=<\/svg>)/, "");

    // Add xmlns for standalone SVG and set a reasonable default render size
    html = html.replace(
      /<svg\s+class="histogram"/,
      '<svg xmlns="http://www.w3.org/2000/svg" class="histogram" width="600"'
    );

    // Inject resolved CSS styles
    const css = dark ? HISTOGRAM_CSS_DARK : HISTOGRAM_CSS_LIGHT;
    html = html.replace(
      /(<svg\s[^>]*>)/,
      `$1<defs><style>${css}</style></defs>`
    );

    output = html;
    break;
  }

  // ---- Circuit (imperative DOM via qviz) ----
  case "Circuit": {
    // qviz.draw() needs a real DOM.  Use jsdom.
    const { JSDOM } = await import("jsdom");
    const dom = new JSDOM("<!DOCTYPE html><html><body></body></html>");
    const win = dom.window;

    // Patch globals so qviz helpers can call
    // document.createElementNS, document.createElement, getComputedStyle, etc.
    globalThis.document = win.document;
    globalThis.window = win;
    globalThis.getComputedStyle = win.getComputedStyle;
    globalThis.DOMPoint = win.DOMPoint;
    globalThis.performance = win.performance;
    globalThis.requestAnimationFrame = (cb) => setTimeout(cb, 0);

    const container = win.document.createElement("div");
    const circuitData = typeof props.circuit === "string"
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
      // Inject embedded CSS so the SVG renders correctly standalone
      const dark = !!props.darkMode;
      const css = dark ? CIRCUIT_CSS_DARK : CIRCUIT_CSS_LIGHT;
      const styleEl = dom.window.document.createElementNS(
        "http://www.w3.org/2000/svg", "style"
      );
      styleEl.textContent = css;
      const defs = dom.window.document.createElementNS(
        "http://www.w3.org/2000/svg", "defs"
      );
      defs.appendChild(styleEl);
      svg.insertBefore(defs, svg.firstChild);
      output = svg.outerHTML;
    } else {
      // Fallback: return the full container HTML
      output = container.innerHTML;
    }
    break;
  }

  default:
    process.stderr.write(`Unknown component: ${component}\n`);
    process.exit(1);
}

process.stdout.write(output);
