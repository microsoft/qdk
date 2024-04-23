// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { useEffect, useRef } from "preact/hooks";
import markdownit from "markdown-it";
import { IDocFile } from "qsharp-lang";

export function getNamespaces(
  documentation: Map<string, string> | undefined,
): string[] {
  if (documentation) {
    return Array.from(documentation.keys());
  }
  return new Array<string>();
}

// Takes array of documents (containing data for each item in the standard library)
// and creates a documentation map, which maps from a namespace
// to the combined HTML-formatted documentation for all items in that namespace.
export function processDocumentFiles(
  docFiles: IDocFile[],
): Map<string, string> {
  const md = markdownit();
  const contentByNamespace = new Map<string, string>();
  const regex = new RegExp("^qsharp.namespace: Microsoft.Quantum.(.+)$", "m");

  for (const doc of docFiles) {
    const match = regex.exec(doc.metadata); // Parse namespace out of metadata
    if (match == null) {
      continue; // Skip items with non-parsable metadata
    }
    // The next line contains "Zero-width space" unicode character
    // to allow line breaks before the period.
    const newNamespace = "… " + match[1].replace(".", "​.");
    const newContent = md.render(doc.contents);

    if (contentByNamespace.has(newNamespace)) {
      const existingContent = contentByNamespace.get(newNamespace)!;
      contentByNamespace.set(
        newNamespace,
        existingContent + "\n<br>\n<br>\n" + newContent,
      );
    } else {
      contentByNamespace.set(newNamespace, newContent);
    }
  }
  return contentByNamespace;
}

export function DocumentationDisplay(props: {
  currentNamespace: string;
  documentation: Map<string, string> | undefined;
}) {
  const docsDiv = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!docsDiv.current || !props.documentation) return;
    docsDiv.current.innerHTML = props.documentation.get(
      props.currentNamespace,
    )!;
    MathJax.typeset();
  }, [props.currentNamespace]);

  return <div ref={docsDiv}></div>;
}
