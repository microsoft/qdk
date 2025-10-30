// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { useRef, useEffect } from "preact/hooks";
import { createViewer, GLViewer } from "3dmol";

import "./style.css";

export function MoleculeViewer(props: { moleculeData: string }) {
  // Holds reference to the viewer div and 3Dmol viewer object.
  const viewerRef = useRef<HTMLDivElement>(null);
  const viewer = useRef<GLViewer | null>(null);

  console.log("MoleculeViewer rendered");

  // Runs after the DOM has been created. Create the 3Dmol viewer and adds the model.
  useEffect(() => {
    console.log("MoleculeViewer useEffect called");
    if (props.moleculeData && viewerRef.current) {
      const molViewer = viewer.current ?? createViewer(viewerRef.current);
      try {
        molViewer.clear(); // If the model is being replaced, clear the old one. Perhaps should get and update instead?
        molViewer.addModel(props.moleculeData.trim(), "xyz", {
          assignBonds: true,
        });
      } catch (error) {
        console.error("Error adding model:", error);
      }
      viewer.current = molViewer;
      updateViewerStyle("Sphere");
    }
  });

  function updateViewerStyle(style: string) {
    const currViewer = viewer.current;
    if (!currViewer) {
      return;
    }
    currViewer.setStyle({}, {});

    if (style === "Sphere") {
      currViewer.setStyle({}, { sphere: { scale: 0.3 }, stick: {} });
    } else if (style === "Stick") {
      currViewer.setStyle({}, { stick: { radius: 0.2 } });
    } else if (style === "Line") {
      currViewer.setStyle({}, { line: { linewidth: 5.0 } });
    }
    currViewer.zoomTo();
    currViewer.render();
  }

  return (
    <div id="viewer-container">
      <div
        id="viewer"
        ref={viewerRef}
        style="width: 640px; height: 480px;"
      ></div>

      <div id="view-dropdown-container">
        <label for="viewSelector">Visualization Style:</label>
        <br />
        <select
          id="viewSelector"
          onChange={(e) => {
            const style = (e.target as HTMLSelectElement).value;
            updateViewerStyle(style);
          }}
        >
          <option value="Sphere">Sphere</option>
          <option value="Stick">Stick</option>
          <option value="Line">Line</option>
        </select>
      </div>
    </div>
  );
}
