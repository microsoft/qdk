// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { App } from "@modelcontextprotocol/ext-apps";
import {
  applyDocumentTheme,
  applyHostStyleVariables,
  applyHostFonts,
} from "@modelcontextprotocol/ext-apps";
import { draw, type CircuitGroup } from "qdk-circuit-vis/index.js";
import "qdk-ux-css";
import "qdk-circuit-css";
import "qdk-theme-css";

const container = document.getElementById("circuit-container")!;
const status = document.getElementById("status")!;

const app = new App({ name: "qdk-circuit", version: "0.0.0" });

app.ontoolresult = (result) => {
  const circuitGroup = result.structuredContent as CircuitGroup | undefined;
  if (!circuitGroup) {
    status.textContent = "No circuit data received.";
    return;
  }

  // Clear previous content and render
  container.innerHTML = "";
  draw(circuitGroup, container);
};

app.onhostcontextchanged = (ctx) => {
  if (ctx.theme) applyDocumentTheme(ctx.theme);
  if (ctx.styles?.variables) applyHostStyleVariables(ctx.styles.variables);
  if (ctx.styles?.css?.fonts) applyHostFonts(ctx.styles.css.fonts);
  if (ctx.safeAreaInsets) {
    const { top, right, bottom, left } = ctx.safeAreaInsets;
    document.body.style.padding = `${top}px ${right}px ${bottom}px ${left}px`;
  }
};

app.onteardown = async () => {
  container.innerHTML = "";
  return {};
};

app.connect();
