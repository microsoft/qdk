// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// **** Helper functions for rendering SVG elements ****

type StringMap = Record<string, string>;

export function createSvgElements(...tags: string[]): SVGElement[] {
  return tags.map((tag) =>
    document.createElementNS("http://www.w3.org/2000/svg", tag),
  );
}

export function setAttributes(el: SVGElement, attrs: StringMap) {
  for (const key in attrs) el.setAttribute(key, attrs[key]);
}

export function appendChildren(parent: Element, children: Element[]) {
  children.forEach((child) => parent.appendChild(child));
}

export function addChildWithClass(
  parent: HTMLElement,
  childTag: string,
  className: string,
): HTMLElement {
  const parentDoc = parent.ownerDocument;
  const child = parentDoc.createElement(childTag);
  child.classList.add(className);
  parent.appendChild(child);
  return child;
}
