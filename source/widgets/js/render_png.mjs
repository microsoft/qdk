#!/usr/bin/env node
// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Headless PNG renderer for MoleculeViewer using Playwright + 3Dmol.
// Reads JSON from stdin, writes PNG to stdout.
//
// Input format:
//   { "molecule_data": "...",          // XYZ format string
//     "cube_data": "...",              // optional: Gaussian cube file string
//     "iso_value": 0.02,              // optional: isovalue for orbital
//     "width": 640,                    // optional: image width
//     "height": 480,                   // optional: image height
//     "style": "Sphere"               // optional: Sphere|Stick|Line
//   }
//
// Requires: playwright (npm), Chromium browser installed via
//   npx playwright install chromium

import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));

// 3Dmol source is copied alongside this script at build time.
const threeDmolSrc = readFileSync(resolve(__dirname, "3Dmol-min.js"), "utf-8");

const input = readFileSync(0, "utf-8");
const props = JSON.parse(input);

const {
  molecule_data: moleculeData,
  cube_data: cubeData,
  iso_value: isoValue = 0.02,
  width = 640,
  height = 480,
  style = "Sphere",
} = props;

if (!moleculeData) {
  process.stderr.write("Error: molecule_data is required\n");
  process.exit(1);
}

// Build self-contained HTML
const html = `<!DOCTYPE html>
<html>
<head><style>
  * { margin: 0; padding: 0; }
  body { background: transparent; }
  #viewer { width: ${width}px; height: ${height}px; }
</style></head>
<body>
<div id="viewer"></div>
<script>${threeDmolSrc}</script>
<script>
(function() {
  var moleculeData = ${JSON.stringify(moleculeData)};
  var cubeData = ${JSON.stringify(cubeData || "")};
  var isoValue = ${isoValue};
  var viewStyle = ${JSON.stringify(style)};

  var viewer = $3Dmol.createViewer("viewer", {
    backgroundColor: "white"
  });

  viewer.addModel(moleculeData.trim(), "xyz", { assignBonds: true });

  if (viewStyle === "Sphere") {
    viewer.setStyle({}, { sphere: { scale: 0.3 }, stick: {} });
  } else if (viewStyle === "Stick") {
    viewer.setStyle({}, { stick: { radius: 0.2 } });
  } else if (viewStyle === "Line") {
    viewer.setStyle({}, { line: { linewidth: 5.0 } });
  }

  if (cubeData) {
    viewer.addVolumetricData(cubeData.trim(), "cube", {
      isoval: isoValue,
      opacity: 1,
      color: "#0072B2"
    });
    viewer.addVolumetricData(cubeData.trim(), "cube", {
      isoval: -1 * isoValue,
      opacity: 1,
      color: "#FFA500"
    });
  }

  viewer.zoomTo();
  viewer.render();

  // Signal that rendering is done
  window.__renderDone = true;
})();
</script>
</body>
</html>`;

// Launch Playwright and screenshot
async function render() {
  let chromium;
  try {
    ({ chromium } = await import("playwright"));
  } catch {
    process.stderr.write(
      "Error: playwright is not installed.\n" +
        "Install it with: npm install playwright\n" +
        "Then install a browser: npx playwright install chromium\n",
    );
    process.exit(1);
  }

  const browser = await chromium.launch({
    args: ["--no-sandbox", "--disable-gpu"],
  });
  try {
    const page = await browser.newPage({
      viewport: { width, height },
    });

    await page.setContent(html, { waitUntil: "domcontentloaded" });

    // Wait for 3Dmol to finish rendering
    await page.waitForFunction("window.__renderDone === true", {
      timeout: 15000,
    });

    // Give WebGL a moment to flush
    await page.waitForTimeout(500);

    const png = await page.screenshot({
      type: "png",
      omitBackground: true,
    });

    process.stdout.write(png);
  } finally {
    await browser.close();
  }
}

render().catch((err) => {
  process.stderr.write(`Render error: ${err.message}\n`);
  process.exit(1);
});
