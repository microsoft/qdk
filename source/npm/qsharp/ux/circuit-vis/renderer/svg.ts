// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { svgNS } from "./constants.js";

export type SvgChild = SvgElement | string;

class SvgClassList {
  constructor(private readonly element: SvgElement) {}

  add(...tokens: string[]): void {
    const classes = new Set(
      (this.element.getAttribute("class") ?? "")
        .split(/\s+/)
        .filter((token) => token.length > 0),
    );
    for (const token of tokens) {
      if (token.length === 0 || /\s/.test(token)) {
        throw new Error(`Invalid SVG class name "${token}".`);
      }
      classes.add(token);
    }
    if (classes.size > 0) {
      this.element.setAttribute("class", Array.from(classes).join(" "));
    }
  }

  contains(token: string): boolean {
    return (this.element.getAttribute("class") ?? "")
      .split(/\s+/)
      .includes(token);
  }
}

/**
 * Minimal SVG element representation shared by interactive rendering and
 * standalone serialization. Circuit layout and formatters build this tree;
 * browser hosts convert it to DOM only when mounting the visualizer.
 */
export class SvgElement {
  readonly attributes = new Map<string, string>();
  readonly children: SvgChild[] = [];
  readonly classList = new SvgClassList(this);

  constructor(readonly tagName: string) {
    assertXmlName(tagName, "element");
  }

  setAttribute(name: string, value: string): void {
    assertXmlName(name, "attribute");
    assertValidXml(value, `attribute "${name}"`);
    this.attributes.set(name, value);
  }

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  appendChild(child: SvgElement): SvgElement {
    this.children.push(child);
    return child;
  }

  appendText(value: string): void {
    assertValidXml(value, "text");
    if (value.length > 0) {
      this.children.push(value);
    }
  }

  get textContent(): string {
    return this.children
      .map((child) => (typeof child === "string" ? child : child.textContent))
      .join("");
  }

  set textContent(value: string) {
    assertValidXml(value, "text");
    this.children.length = 0;
    this.appendText(value);
  }

  replaceChildren(...children: SvgChild[]): void {
    this.children.length = 0;
    for (const child of children) {
      if (typeof child === "string") {
        this.appendText(child);
      } else {
        this.appendChild(child);
      }
    }
  }

  querySelector(selector: string): SvgElement | null {
    const match = simpleSelector(selector);
    return findSvgElement(
      this,
      (element) => element !== this && match(element),
    );
  }
}

export function createSvgElement(
  tagName: string,
  attributes: Readonly<Record<string, string>> = {},
): SvgElement {
  const element = new SvgElement(tagName);
  for (const [name, value] of Object.entries(attributes)) {
    element.setAttribute(name, value);
  }
  return element;
}

export function findSvgElement(
  root: SvgElement,
  predicate: (element: SvgElement) => boolean,
): SvgElement | null {
  if (predicate(root)) {
    return root;
  }
  for (const child of root.children) {
    if (typeof child === "string") continue;
    const match = findSvgElement(child, predicate);
    if (match !== null) {
      return match;
    }
  }
  return null;
}

export function removeSvgElements(
  root: SvgElement,
  predicate: (element: SvgElement) => boolean,
): void {
  for (let index = root.children.length - 1; index >= 0; index--) {
    const child = root.children[index];
    if (typeof child === "string") continue;
    if (predicate(child)) {
      root.children.splice(index, 1);
    } else {
      removeSvgElements(child, predicate);
    }
  }
}

export function removeSvgAttributes(
  root: SvgElement,
  predicate: (name: string) => boolean,
): void {
  for (const name of root.attributes.keys()) {
    if (predicate(name)) {
      root.attributes.delete(name);
    }
  }
  for (const child of root.children) {
    if (typeof child !== "string") {
      removeSvgAttributes(child, predicate);
    }
  }
}

export function toDomSvgElement(
  source: SvgElement,
  ownerDocument: Document,
): SVGElement {
  const target = ownerDocument.createElementNS(svgNS, source.tagName);
  for (const [name, value] of source.attributes) {
    target.setAttribute(name, value);
  }
  for (const child of source.children) {
    target.appendChild(
      typeof child === "string"
        ? ownerDocument.createTextNode(child)
        : toDomSvgElement(child, ownerDocument),
    );
  }
  return target;
}

export function serializeSvgElement(source: SvgElement): string {
  const attributes = Array.from(
    source.attributes,
    ([name, value]) => ` ${name}="${escapeXmlAttribute(value)}"`,
  ).join("");
  const children = source.children
    .map((child) =>
      typeof child === "string"
        ? escapeXmlText(child)
        : serializeSvgElement(child),
    )
    .join("");
  return `<${source.tagName}${attributes}>${children}</${source.tagName}>`;
}

function simpleSelector(selector: string): (element: SvgElement) => boolean {
  const match = selector.match(
    /^(?<tag>[A-Za-z_][A-Za-z0-9_.:-]*)?(?:\.(?<class>[A-Za-z0-9_-]+))?$/,
  );
  if (match?.groups === undefined || selector.length === 0) {
    throw new Error(`Unsupported SVG selector "${selector}".`);
  }
  const tag = match.groups["tag"];
  const className = match.groups["class"];
  return (element) =>
    (tag === undefined || element.tagName === tag) &&
    (className === undefined || element.classList.contains(className));
}

function escapeXmlText(value: string): string {
  assertValidXml(value, "text");
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function escapeXmlAttribute(value: string): string {
  return escapeXmlText(value)
    .replaceAll('"', "&quot;")
    .replaceAll("\t", "&#x9;")
    .replaceAll("\n", "&#xA;")
    .replaceAll("\r", "&#xD;");
}

function assertXmlName(value: string, kind: string): void {
  if (!/^[A-Za-z_][A-Za-z0-9_.:-]*$/.test(value)) {
    throw new Error(`Invalid SVG ${kind} name "${value}".`);
  }
}

function assertValidXml(value: string, context: string): void {
  for (let index = 0; index < value.length; index++) {
    const codePoint = value.codePointAt(index);
    if (codePoint === undefined) break;
    const valid =
      codePoint === 0x9 ||
      codePoint === 0xa ||
      codePoint === 0xd ||
      (codePoint >= 0x20 && codePoint <= 0xd7ff) ||
      (codePoint >= 0xe000 && codePoint <= 0xfffd) ||
      (codePoint >= 0x10000 && codePoint <= 0x10ffff);
    if (!valid) {
      throw new Error(
        `Cannot serialize SVG ${context}: invalid XML character U+${codePoint
          .toString(16)
          .toUpperCase()
          .padStart(4, "0")}.`,
      );
    }
    if (codePoint > 0xffff) index++;
  }
}
