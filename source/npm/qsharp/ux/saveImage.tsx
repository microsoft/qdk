// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Writes resource estimator output to PNG file

import {
  getImageSize,
  createImage,
  nodeToDataURI,
  toArray,
} from "./saveImageUtil.js";

async function cloneSingleNode<T extends HTMLElement>(
  node: T,
): Promise<HTMLElement> {
  return node.cloneNode(false) as T;
}

async function cloneChildren<T extends HTMLElement>(
  nativeNode: T,
  clonedNode: T,
): Promise<T> {
  const children = toArray<T>((nativeNode.shadowRoot ?? nativeNode).childNodes);

  // Depth-first traversal of DOM objects
  await children.reduce(
    (deferred, child) =>
      deferred
        .then(() => cloneNode(child))
        .then((clonedChild: HTMLElement | null) => {
          if (clonedChild) {
            clonedNode.appendChild(clonedChild);
          }
        }),
    Promise.resolve(),
  );

  return clonedNode;
}

function cloneCSSStyle<T extends HTMLElement>(nativeNode: T, clonedNode: T) {
  const targetStyle = clonedNode.style;
  if (!targetStyle) {
    return;
  }

  const sourceStyle = window.getComputedStyle(nativeNode);
  toArray<string>(sourceStyle).forEach((name) => {
    const value = sourceStyle.getPropertyValue(name);
    targetStyle.setProperty(name, value, sourceStyle.getPropertyPriority(name));
  });
}

function decorate<T extends HTMLElement>(nativeNode: T, clonedNode: T): T {
  cloneCSSStyle(nativeNode, clonedNode);
  return clonedNode;
}

async function cloneNode<T extends HTMLElement>(node: T): Promise<T | null> {
  return Promise.resolve(node)
    .then((clonedNode) => cloneSingleNode(clonedNode) as Promise<T>)
    .then((clonedNode) => cloneChildren(node, clonedNode))
    .then((clonedNode) => decorate(node, clonedNode));
}

async function saveToPng<T extends HTMLElement>(
  node: T,
  backgroundColor: string,
): Promise<string> {
  const { width, height } = getImageSize(node);
  const clonedNode = (await cloneNode(node)) as HTMLElement;
  const uri = await nodeToDataURI(clonedNode, width, height);
  const img = await createImage(uri);

  const ratio = window.devicePixelRatio || 1;
  const canvas = document.createElement("canvas");
  canvas.width = width * ratio;
  canvas.height = height * ratio;

  const context = canvas.getContext("2d")!;
  context.fillStyle = backgroundColor;
  context.fillRect(0, 0, canvas.width, canvas.height);

  context.drawImage(img, 0, 0, canvas.width, canvas.height);
  return canvas.toDataURL();
}

export async function saveToImage<T extends HTMLElement>(
  element: T,
  filename = "image.png",
) {
  const backgroundColor =
    getComputedStyle(element).getPropertyValue("--main-background");
  const data = await saveToPng(element, backgroundColor);
  const link = document.createElement("a");
  if (typeof link.download === "string") {
    link.href = data;
    link.download = filename;

    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
  } else {
    window.open(data);
  }
}
