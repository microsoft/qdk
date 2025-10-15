// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// @ts-check

import { JSDOM } from "jsdom";
import { createCanvas, Image, ImageData } from "canvas";
import { afterEach, beforeEach } from "node:test";

const documentTemplate = `<!doctype html><html>
      <head>
        <link rel="stylesheet" href="../../../ux/qsharp-ux.css">
        <link rel="stylesheet" href="../../../ux/qsharp-circuit.css">
      </head>
      <body>
      </body>
      </html>`;

export function withDom() {
  /** @type {JSDOM | null} */
  let jsdom = null;

  beforeEach(() => {
    jsdom = new JSDOM(documentTemplate, {
      pretendToBeVisual: true,
      resources: "usable",
    });

    const { window } = jsdom;

    globalThis.window = window;
    globalThis.document = window.document;
    globalThis.Node = window.Node;
    globalThis.HTMLElement = window.HTMLElement;
    globalThis.SVGElement = window.SVGElement;
    globalThis.XMLSerializer = window.XMLSerializer;
    globalThis.CustomEvent = window.CustomEvent;

    window.HTMLCanvasElement.prototype.getContext = function getContext(
      type,
      ...args
    ) {
      if (type === "2d") {
        // create a new canvas instance with the same dimensions
        const nodeCanvas = createCanvas(this.width, this.height);
        return nodeCanvas.getContext("2d", ...args);
      }
      return null;
    };

    // Optional: expose Image, ImageData for compatibility
    globalThis.Image = Image;
    globalThis.ImageData = ImageData;
  });

  afterEach(() => {
    jsdom?.window.close();

    // clean up globals
    delete globalThis.window;
    delete globalThis.document;
    delete globalThis.Node;
    delete globalThis.HTMLElement;
    delete globalThis.SVGElement;
    delete globalThis.XMLSerializer;
    delete globalThis.CustomEvent;
    delete globalThis.Image;
    delete globalThis.ImageData;

    jsdom = null;
  });
}
