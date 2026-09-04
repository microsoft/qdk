// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { RendererToExtensionMessage } from "./schema.js";

export type PostAction = (message: RendererToExtensionMessage) => boolean;

export type RenderContext = {
  postAction: PostAction;
  addDisposable(dispose: () => void): void;
};

export function appendTextElement(
  parent: HTMLElement,
  tagName: keyof HTMLElementTagNameMap,
  className: string,
  text: string,
): HTMLElement {
  const element = document.createElement(tagName);
  element.className = className;
  element.textContent = text;
  parent.appendChild(element);
  return element;
}

/**
 * Like {@link appendTextElement}, but renders `backtick` spans as inline code.
 *
 * Course prose is authored in Markdown, so prompts and answer options routinely
 * name APIs as `circuit(...)`. Rendering those as literal backticks looks like
 * a bug. This deliberately is NOT a Markdown parser: it only splits on
 * backticks and builds `<code>` nodes with `textContent`, so author text is
 * never interpreted as HTML.
 */
export function appendRichTextElement(
  parent: HTMLElement,
  tagName: keyof HTMLElementTagNameMap,
  className: string,
  text: string,
): HTMLElement {
  const element = document.createElement(tagName);
  element.className = className;
  appendInlineText(element, text);
  parent.appendChild(element);
  return element;
}

function appendInlineText(parent: HTMLElement, text: string): void {
  const parts = text.split("`");

  // An even number of parts means an unbalanced backtick. Treat the whole
  // string as plain text rather than guessing where the code span ends.
  if (parts.length % 2 === 0) {
    parent.appendChild(document.createTextNode(text));
    return;
  }

  parts.forEach((part, index) => {
    if (part.length === 0) {
      return;
    }
    if (index % 2 === 1) {
      const code = document.createElement("code");
      code.textContent = part;
      parent.appendChild(code);
    } else {
      parent.appendChild(document.createTextNode(part));
    }
  });
}

/**
 * Build a Copilot action button.
 *
 * The sparkle mark and the `--vscode-button-*` colors are deliberate: this is
 * the same affordance as the "Ask for a Hint" item in the cell status bar, so
 * it should read as the same control even though it lives in cell output.
 */
export function createActionButton(label: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "qdk-learning-action";
  button.appendChild(createSparkleIcon());
  button.appendChild(document.createTextNode(label));
  return button;
}

function createSparkleIcon(): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 16 16");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("focusable", "false");

  const path = document.createElementNS(SVG_NS, "path");
  path.setAttribute("d", SPARKLE_PATH);
  svg.appendChild(path);
  return svg;
}

const SVG_NS = "http://www.w3.org/2000/svg";

/** A large four-point star with a smaller companion, matching the codicon. */
const SPARKLE_PATH =
  "M9.5 1.5 11 5.2 14.7 6.7 11 8.2 9.5 11.9 8 8.2 4.3 6.7 8 5.2 Z " +
  "M4 9.6 4.8 11.5 6.7 12.3 4.8 13.1 4 15 3.2 13.1 1.3 12.3 3.2 11.5 Z";

/**
 * Announce a change to assistive technology.
 *
 * Answering a question or switching orbitals updates the view in place, which
 * a screen reader would otherwise miss.
 */
export function setLiveRegion(element: HTMLElement) {
  element.setAttribute("role", "status");
  element.setAttribute("aria-live", "polite");
}
