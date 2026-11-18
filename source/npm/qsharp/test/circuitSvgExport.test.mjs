// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// @ts-check

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { h, render } from "preact";
import { draw } from "../dist/ux/circuit-vis/index.js";
import { Circuit } from "../dist/ux/circuit.js";
const XLINK_NS = "http://www.w3.org/1999/xlink";
const exportCoverageCircuit = JSON.parse(
  readFileSync(new URL("./svg-export.qsc", import.meta.url), "utf8"),
);

test("renderer exports the full mounted SVG without changing its geometry or zoom", async () => {
  await withDom(async (document) => {
    const circuit = representativeCircuit();
    const before = JSON.stringify(circuit);
    const { renderer, container } = drawCircuit(document, circuit);
    renderer.userSetZoomLevel(25);
    const mounted = container.querySelector("svg.qviz");
    const markup = container.innerHTML;
    const options = {
      title: "Parameterized Bell circuit",
      description: "A Bell circuit with an Rx gate and measurement.",
    };
    const first = renderer.exportSvg(options);
    const parsed = parseSvg(first);

    assert.equal(first, renderer.exportSvg(options));
    assert.equal(JSON.stringify(circuit), before);
    assert.equal(container.innerHTML, markup);
    assert.deepEqual(geometry(parsed), geometry(mounted));
    assert.equal(parsed.getAttribute("width"), mounted.getAttribute("width"));
    assert.equal(parsed.getAttribute("height"), mounted.getAttribute("height"));
    assert.equal(
      parsed.getAttribute("viewBox"),
      mounted.getAttribute("viewBox"),
    );
    assert.equal(parsed.getAttribute("style"), null);
    assert.match(first, /^<\?xml version="1\.0" encoding="UTF-8"\?>\n/);
    assert.doesNotMatch(first, /<text\b|@font-face|data-qdk-text-fallback/);
    assert.deepEqual(labelTexts(parsed), labelTexts(mounted));
    for (const use of parsed.querySelectorAll("use")) {
      const reference = use.getAttributeNS(XLINK_NS, "href") ?? "";
      assert.match(reference, /^#qdk-glyph-(?:main|math)-[0-9a-f]+$/);
      assert.ok(
        parsed.querySelector(`defs > path[id="${reference.slice(1)}"]`),
      );
    }
    assert.doesNotMatch(first, /QDKCircuitMain|QDKCircuitMath|var\(--/);
    assert.doesNotMatch(first, /gate-control|\sdata-[A-Za-z0-9_-]+=/);
    assert.equal(parsed.getAttribute("xmlns"), "http://www.w3.org/2000/svg");
    assert.equal(parsed.getAttribute("role"), "img");
    assert.equal(
      parsed.getAttribute("aria-labelledby"),
      "qdk-circuit-title qdk-circuit-description",
    );
    assert.equal(parsed.querySelector("title")?.textContent, options.title);
    assert.equal(
      parsed.querySelector("desc")?.textContent,
      options.description,
    );
    assert.equal(parsed.querySelector(".qdk-circuit-export-background"), null);
    assert.ok(parsed.querySelector(".oplus"));
    assert.ok(parsed.querySelector(".gate-measure"));
    assert.match(
      parsed.querySelector(".arg-button")?.textContent ?? "",
      /π\/2/,
    );
    assert.ok(container.querySelector("[data-location]"));
  });
});

test("renderer export removes editor controls and source links only from the copy", async () => {
  await withDom(async (document) => {
    const { renderer, container } = drawCircuit(document, groupedCircuit());
    const svg = container.querySelector("svg.qviz");
    const namespace = "http://www.w3.org/2000/svg";
    const overlay = document.createElementNS(namespace, "g");
    overlay.classList.add("editor-overlay");
    overlay.appendChild(document.createElementNS(namespace, "rect"));
    svg.appendChild(overlay);
    const link = document.createElementNS(namespace, "a");
    link.setAttribute("href", "command:qsharp-vscode.open-source");
    const input = svg.firstElementChild;
    svg.prepend(link);
    link.appendChild(input);
    const before = container.innerHTML;

    const parsed = parseSvg(renderer.exportSvg());
    assert.equal(parsed.querySelector("a,.editor-overlay,.gate-control"), null);
    assert.deepEqual(geometry(parsed), geometry(svg));
    assert.equal(container.innerHTML, before);
  });
});

test("renderer export leaves out the circuit editor's add-qubit row", async () => {
  await withDom(async (document) => {
    const { renderer: viewer } = drawCircuit(document, representativeCircuit());
    const container = document.createElement("div");
    document.body.appendChild(container);
    const editor = draw(
      { version: 2, circuits: [representativeCircuit()] },
      container,
      { editor: { editCallback: () => {} } },
    );
    const mounted = container.querySelector("svg.qviz");
    const expected = parseSvg(viewer.exportSvg());
    const exported = parseSvg(editor.exportSvg());

    assert.ok(
      Number(mounted?.getAttribute("height")) >
        Number(expected.getAttribute("height")),
    );
    for (const name of ["width", "height", "viewBox"]) {
      assert.equal(
        exported.getAttribute(name),
        expected.getAttribute(name),
        name,
      );
    }
  });
});

test("standalone SWAP hitboxes do not become black rectangles in SVG viewers", async () => {
  await withDom(async (document) => {
    const circuit = representativeCircuit();
    circuit.componentGrid.push({
      components: [
        {
          kind: "unitary",
          gate: "SWAP",
          targets: [{ qubit: 0 }, { qubit: 1 }],
        },
      ],
    });
    const { renderer } = drawCircuit(document, circuit);
    const parsed = parseSvg(renderer.exportSvg());
    assert.ok(parsed.querySelector("rect.gate-swap"));
    assert.match(
      parsed.querySelector("style")?.textContent ?? "",
      /rect\.gate-swap\{fill:none;stroke:none\}/,
    );
  });
});

test("renderer export preserves all supported gate forms and classical markers", async () => {
  await withDom(async (document) => {
    for (const renderDepth of [0, 999999]) {
      const container = document.createElement("div");
      document.body.appendChild(container);
      const before = JSON.stringify(exportCoverageCircuit);
      const renderer = draw(exportCoverageCircuit, container, { renderDepth });
      const mounted = container.querySelector("svg.qviz");
      const markup = container.innerHTML;
      const parsed = parseSvg(renderer.exportSvg());

      assert.deepEqual(geometry(parsed), geometry(mounted));
      assert.equal(container.innerHTML, markup);
      assert.equal(JSON.stringify(exportCoverageCircuit), before);
      for (const selector of [
        ".gate-unitary",
        ".oplus",
        "rect.gate-swap",
        ".control-dot",
        ".anti-control-dot",
        ".gate-measure",
        ".gate-ket",
        ".register-classical",
        ".classically-controlled-btn",
        ".arg-button",
      ]) {
        assert.ok(parsed.querySelector(selector), selector);
      }
      assert.equal(
        parsed.querySelector(".gate-control,.editor-overlay,a"),
        null,
      );
      assert.ok(parsed.textContent?.includes("U12†"));
      assert.ok(parsed.textContent?.includes("θ2 + π/4"));
      assert.equal(parsed.querySelectorAll(".qs-qubit-label").length, 4);
      container.remove();
    }
  });
});

test("renderer export lowers subscripts by the viewer's offset for each font size", async () => {
  await withDom(async (document) => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const renderer = draw(exportCoverageCircuit, container, {
      renderDepth: 999999,
    });
    const mounted = Array.from(container.querySelectorAll("svg.qviz text"));
    const exported = Array.from(
      parseSvg(renderer.exportSvg()).querySelectorAll("g > desc"),
    );

    // Chromium's `baseline-shift: sub` offsets for 16, 14, and 12 px labels.
    for (const [label, offset] of [
      ["|ψ12⟩", 6],
      ["U12†", 5],
      ["θ2 + π/4", 4.5],
      ["c3", 5],
    ]) {
      const y = Number(
        mounted.find((text) => text.textContent === label)?.getAttribute("y"),
      );
      const glyphs = Array.from(
        exported
          .find((description) => description.textContent === label)
          ?.parentElement?.querySelectorAll("use") ?? [],
        (use) => {
          const [, scale, glyphY] =
            /^matrix\((\S+) 0 0 \S+ \S+ (\S+)\)$/.exec(
              use.getAttribute("transform") ?? "",
            ) ?? [];
          return { scale: Number(scale), y: Number(glyphY) };
        },
      );
      const subscriptScale = Math.min(...glyphs.map((glyph) => glyph.scale));
      const subscripts = glyphs.filter(
        (glyph) => glyph.scale === subscriptScale,
      );
      assert.ok(subscripts.length > 0, label);
      for (const glyph of subscripts) {
        // Each run is centered on its KaTeX_Main x-height (442 units).
        const shift = glyph.y - y - (442 * glyph.scale) / 2;
        assert.equal(Number(shift.toFixed(3)), offset, label);
      }
    }
  });
});

test("conditional ket labels draw U+3009 with the KaTeX bracket", async () => {
  await withDom(async (document) => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const renderer = draw(exportCoverageCircuit, container, {
      renderDepth: 999999,
    });
    const parsed = parseSvg(renderer.exportSvg());
    const label = Array.from(parsed.querySelectorAll("g > desc")).find(
      (description) => description.textContent === "if: c3 = |1\u3009",
    )?.parentElement;

    assert.equal(parsed.querySelector("text"), null);
    assert.equal(
      label?.lastElementChild?.getAttributeNS(XLINK_NS, "href"),
      "#qdk-glyph-main-27e9",
    );
    assert.ok(container.textContent?.includes("if: c3 = |1\u3009"));
  });
});

test("measurement basis labels export as white KaTeX outlines", async () => {
  await withDom(async (document) => {
    const { renderer } = drawCircuit(document, {
      qubits: [
        { id: 0, numResults: 1 },
        { id: 1, numResults: 1 },
      ],
      componentGrid: [
        {
          components: [
            {
              kind: "measurement",
              gate: "MX",
              qubits: [{ qubit: 0 }],
              results: [{ qubit: 0, result: 0 }],
            },
            {
              kind: "measurement",
              gate: "MZ",
              qubits: [{ qubit: 1 }],
              results: [{ qubit: 1, result: 0 }],
            },
          ],
        },
      ],
    });
    const parsed = parseSvg(renderer.exportSvg());

    assert.equal(parsed.querySelector("text"), null);
    assert.deepEqual(
      Array.from(parsed.querySelectorAll("g.measurement-basis"), (label) => [
        label.querySelector("desc")?.textContent,
        label.getAttribute("fill"),
        label.querySelector("use")?.getAttributeNS(XLINK_NS, "href"),
      ]),
      [
        ["X", "#fff", "#qdk-glyph-main-58"],
        ["Z", "#fff", "#qdk-glyph-main-5a"],
      ],
    );
  });
});

test("renderer export keeps labels with unsupported characters as complete text", async () => {
  await withDom(async (document) => {
    const circuit = singleQubitCircuit("Opération");
    circuit.componentGrid.push(singleQubitCircuit("H").componentGrid[0]);
    const { renderer } = drawCircuit(document, circuit);
    const exported = renderer.exportSvg();
    const parsed = parseSvg(exported);

    assert.deepEqual(
      Array.from(parsed.querySelectorAll("text"), (text) => text.textContent),
      ["Opération"],
    );
    assert.equal(parsed.getAttribute("data-qdk-text-fallback"), "U+00E9");
    assert.match(exported, /@font-face\{font-family:"KaTeX_Main";/);
    assert.match(exported, /@font-face\{font-family:"KaTeX_Math";/);
    assert.ok(labelTexts(parsed).includes("H"));
    assert.ok(parsed.querySelector("use"));
  });
});

test("renderer export collapses label white space like browsers", async () => {
  await withDom(async (document) => {
    const { renderer } = drawCircuit(
      document,
      singleQubitCircuit(" loop: 0..\r\n\tn - 1 "),
    );
    const parsed = parseSvg(renderer.exportSvg());
    const label = Array.from(parsed.querySelectorAll("g > desc")).find(
      (description) => description.textContent === "loop: 0.. n - 1",
    );

    assert.equal(parsed.querySelector("text"), null);
    assert.equal(
      label?.parentElement?.querySelectorAll("use").length,
      "loop:0..n-1".length,
    );
  });
});

test("renderer export rejects invalid dimensions instead of producing a broken SVG", async () => {
  await withDom(async (document) => {
    for (const dimension of ["width", "height"]) {
      for (const value of ["-1", "NaN", "Infinity"]) {
        const { renderer, container } = drawCircuit(
          document,
          representativeCircuit(),
        );
        container.querySelector("svg.qviz").setAttribute(dimension, value);
        assert.throws(
          () => renderer.exportSvg(),
          new RegExp(`invalid ${dimension}`),
        );
        container.remove();
      }
    }
  });
});

test("renderer export escapes labels and accessible titles", async () => {
  await withDom(async (document) => {
    const { renderer } = drawCircuit(document, singleQubitCircuit("<A&B>"));
    const exported = renderer.exportSvg({ title: "A & B" });
    assert.match(exported, /&lt;/);
    assert.match(exported, /&amp;/);
    const parsed = parseSvg(exported);
    assert.equal(parsed.textContent?.includes("<A&B>"), true);
    assert.equal(parsed.querySelector("title")?.textContent, "A & B");
  });
});

test("renderer export rejects invalid XML characters instead of saving a broken SVG", async () => {
  await withDom(async (document) => {
    for (const label of ["bad\u0000label", "bad\uD800label"]) {
      const { renderer } = drawCircuit(document, singleQubitCircuit(label));
      assert.throws(() => renderer.exportSvg(), /invalid XML character/);
    }

    const { renderer } = drawCircuit(document, singleQubitCircuit("H"));
    assert.throws(
      () => renderer.exportSvg({ title: "bad\u0000title" }),
      /invalid XML character/,
    );
  });
});

test("renderer export preserves the current expansion state", async () => {
  await withDom(async (document) => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const renderer = draw(
      {
        version: 2,
        circuits: [groupedCircuit()],
      },
      container,
    );

    assert.equal(
      parseSvg(renderer.exportSvg()).textContent?.includes("ChildH"),
      false,
    );
    const expand = container.querySelector(".gate-expand");
    assert.ok(expand);
    expand.dispatchEvent(new document.defaultView.Event("click"));

    assert.equal(
      parseSvg(renderer.exportSvg()).textContent?.includes("ChildH"),
      true,
    );
  });
});

test("Circuit exports through its host callback", async () => {
  await withDom(async (document) => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    /** @type {{ contents: string; suggestedName: string } | undefined} */
    let saved;
    render(
      h(Circuit, {
        title: "Bell pair",
        circuit: representativeCircuit(),
        onExportSvg: (contents, suggestedName) => {
          saved = { contents, suggestedName };
        },
      }),
      host,
    );

    await waitFor(() => host.querySelector("button")?.disabled === false);
    const button = host.querySelector("button");
    assert.equal(button?.textContent, "Export as SVG");
    button?.click();
    await waitFor(() => saved !== undefined);

    assert.equal(saved?.suggestedName, "bell-pair.svg");
    assert.equal(
      parseSvg(saved?.contents ?? "").querySelector("title")?.textContent,
      "Bell pair",
    );
    assert.equal(host.querySelector(".qs-circuit-export-notice"), null);
    render(null, host);
  });
});

test("Circuit reports labels that remain text after export", async () => {
  await withDom(async (document) => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    render(
      h(Circuit, {
        circuit: singleQubitCircuit("Opération"),
        onExportSvg: () => {},
      }),
      host,
    );

    await waitFor(() => host.querySelector("button")?.disabled === false);
    host.querySelector("button")?.click();
    await waitFor(
      () => host.querySelector(".qs-circuit-export-notice") !== null,
    );

    assert.match(
      host.querySelector(".qs-circuit-export-notice")?.textContent ?? "",
      /\(U\+00E9\)/,
    );
    render(null, host);
  });
});

test("Circuit bounds host export filenames for long titles", async () => {
  await withDom(async (document) => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    let suggestedName;
    render(
      h(Circuit, {
        title: "a".repeat(200),
        circuit: singleQubitCircuit("H"),
        onExportSvg: (_contents, name) => {
          suggestedName = name;
        },
      }),
      host,
    );

    await waitFor(() => host.querySelector("button")?.disabled === false);
    host.querySelector("button")?.click();
    await waitFor(() => suggestedName !== undefined);

    assert.equal(suggestedName?.length, 128);
    assert.equal(suggestedName?.endsWith(".svg"), true);
    render(null, host);
  });
});

test("Circuit avoids Windows reserved device names in export filenames", async () => {
  await withDom(async (document) => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    for (const [title, expectedName] of [
      ["CON", "quantum-circuit.svg"],
      ["PRN", "quantum-circuit.svg"],
      ["AUX", "quantum-circuit.svg"],
      ["NUL", "quantum-circuit.svg"],
      ["COM1", "quantum-circuit.svg"],
      ["LPT9", "quantum-circuit.svg"],
      ["Con.details", "quantum-circuit.svg"],
      ["Control", "control.svg"],
      ["COM10", "com10.svg"],
    ]) {
      let suggestedName;
      render(
        h(Circuit, {
          title,
          circuit: singleQubitCircuit("H"),
          onExportSvg: (_contents, name) => {
            suggestedName = name;
          },
        }),
        host,
      );

      await waitFor(() => host.querySelector("button")?.disabled === false);
      host.querySelector("button")?.click();
      await waitFor(() => suggestedName !== undefined);

      assert.equal(suggestedName, expectedName, title);
      render(null, host);
    }
  });
});

test("Circuit downloads in browser hosts without a save callback", async () => {
  await withDom(async (document, window) => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    let downloadedName;
    let revokedUrl;
    window.URL.createObjectURL = () => "blob:qdk-circuit";
    window.URL.revokeObjectURL = (url) => {
      revokedUrl = url;
    };
    const originalClick = window.HTMLAnchorElement.prototype.click;
    window.HTMLAnchorElement.prototype.click = function () {
      downloadedName = this.download;
    };

    try {
      render(
        h(Circuit, {
          title: "Browser circuit",
          circuit: singleQubitCircuit("H"),
        }),
        host,
      );
      await waitFor(() => host.querySelector("button")?.disabled === false);
      host.querySelector("button")?.click();
      await waitFor(() => downloadedName !== undefined);
      await waitFor(() => revokedUrl !== undefined);

      assert.equal(downloadedName, "browser-circuit.svg");
      assert.equal(revokedUrl, "blob:qdk-circuit");
    } finally {
      window.HTMLAnchorElement.prototype.click = originalClick;
      render(null, host);
    }
  });
});

function representativeCircuit() {
  return {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "H",
            targets: [{ qubit: 0 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            controls: [{ qubit: 0 }],
            targets: [{ qubit: 1 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "Rx",
            args: ["π/2"],
            targets: [{ qubit: 1 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
    ],
  };
}

function groupedCircuit() {
  return {
    qubits: [{ id: 0 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Group",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "ChildH",
                    targets: [{ qubit: 0 }],
                  },
                ],
              },
            ],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 0 }],
          },
        ],
      },
    ],
  };
}

function singleQubitCircuit(gate) {
  return {
    qubits: [{ id: 0 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate,
            targets: [{ qubit: 0 }],
          },
        ],
      },
    ],
  };
}

function parseSvg(contents) {
  return new JSDOM(contents, {
    contentType: "image/svg+xml",
  }).window.document.documentElement;
}

function drawCircuit(document, circuit) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const renderer = draw({ version: 2, circuits: [circuit] }, container);
  return { renderer, container };
}

function geometry(svg) {
  return Array.from(svg.querySelectorAll("*"))
    .filter(
      (element) =>
        ["line", "rect", "circle", "path"].includes(element.localName) &&
        !element.closest(".gate-control,.editor-overlay,defs"),
    )
    .map((element) => ({
      tag: element.localName,
      attributes: Array.from(element.attributes)
        .filter((attribute) => !attribute.name.startsWith("data-"))
        .map((attribute) => [attribute.name, attribute.value]),
    }));
}

// Label text from live text or from the descriptions of outlined labels.
function labelTexts(svg) {
  return Array.from(svg.querySelectorAll("text, g > desc"))
    .filter((element) => !element.closest(".gate-control,.editor-overlay"))
    .map((element) => element.textContent.replace(/[\t\n\r ]+/g, " ").trim());
}

async function withDom(callback) {
  const jsdom = new JSDOM("<!doctype html><html><body></body></html>", {
    pretendToBeVisual: true,
  });
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    Node: globalThis.Node,
    HTMLElement: globalThis.HTMLElement,
    SVGElement: globalThis.SVGElement,
    XMLSerializer: globalThis.XMLSerializer,
  };
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.Node = jsdom.window.Node;
  globalThis.HTMLElement = jsdom.window.HTMLElement;
  globalThis.SVGElement = jsdom.window.SVGElement;
  globalThis.XMLSerializer = jsdom.window.XMLSerializer;
  try {
    await callback(jsdom.window.document, jsdom.window);
  } finally {
    for (const [name, value] of Object.entries(previous)) {
      if (value === undefined) {
        delete globalThis[name];
      } else {
        globalThis[name] = value;
      }
    }
    jsdom.window.close();
  }
}

async function waitFor(predicate) {
  for (let attempt = 0; attempt < 100; attempt++) {
    if (predicate()) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error("Timed out waiting for the circuit UI.");
}
