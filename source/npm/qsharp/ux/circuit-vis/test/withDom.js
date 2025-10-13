// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// @ts-check

import { before, after } from "node:test";
import { JSDOM } from "jsdom";

export function withDom() {
  /** @type {JSDOM | null} */
  let jsdom = null;

  before(() => {
    jsdom = new JSDOM(
      `<!doctype html><html>
      <head>
        <link rel="stylesheet" href="../../../qsharp-ux.css">
        <link rel="stylesheet" href="../../../qsharp-circuit.css">
      </head>
      <body><div id="app"></div></body></html>`,
      {
        contentType: "text/html",
        pretendToBeVisual: true,
      },
    );
    const { window } = jsdom;

    // expose a minimal DOM to globals
    globalThis.window = window;
    globalThis.document = window.document;
    globalThis.Node = window.Node;
    globalThis.HTMLElement = window.HTMLElement;
    globalThis.SVGElement = window.SVGElement;
    globalThis.XMLSerializer = window.XMLSerializer;
  });

  after(() => {
    if (jsdom) {
      jsdom.window.close();
      jsdom = null;
    }
    delete globalThis.window;
    delete globalThis.document;
    delete globalThis.Node;
    delete globalThis.HTMLElement;
    delete globalThis.SVGElement;
    delete globalThis.XMLSerializer;
  });
}
