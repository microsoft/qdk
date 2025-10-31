// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { useRef, useEffect, useState } from "preact/hooks";
import { createViewer, GLViewer } from "3dmol";

import "./style.css";

export function MoleculeViewer(props: {
  moleculeData: string;
  cubeData: { [key: string]: string };
}) {
  // Holds reference to the viewer div and 3Dmol viewer object.
  const viewerRef = useRef<HTMLDivElement>(null);
  const viewer = useRef<GLViewer | null>(null);
  const activeCubeData = useRef([] as any[]);

  const [viewStyle, setViewStyle] = useState("Sphere");
  const [isoval, setIsoval] = useState(0.02);
  const [cubeKey, setCubeKey] = useState(Object.keys(props.cubeData)[0] || "");

  console.log(
    "MoleculeViewer rendered. cube keys:",
    Object.keys(props.cubeData),
  );

  // Runs after the DOM has been created. Create the 3Dmol viewer and adds the model.
  useEffect(() => {
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
      viewer.current.zoomTo();
    }
  }, [props.moleculeData]);

  useEffect(() => {
    console.log("Updating viewer with style:", viewStyle, "isoval:", isoval);
    const currViewer = viewer.current;
    if (!currViewer) {
      return;
    }

    if (cubeKey && props.cubeData[cubeKey]) {
      activeCubeData.current.forEach((voldata) => {
        currViewer.removeShape(voldata);
      });
      activeCubeData.current = [];
      const cubeData = props.cubeData[cubeKey];
      activeCubeData.current.push(
        currViewer.addVolumetricData(cubeData.trim(), "cube", {
          isoval,
          opacity: 1,
          color: "red",
        }),
      );
      activeCubeData.current.push(
        currViewer.addVolumetricData(cubeData.trim(), "cube", {
          isoval: -1 * isoval,
          opacity: 1,
          color: "green",
        }),
      );
    }

    if (viewStyle === "Sphere") {
      currViewer.setStyle({}, { sphere: { scale: 0.3 }, stick: {} });
    } else if (viewStyle === "Stick") {
      currViewer.setStyle({}, { stick: { radius: 0.2 } });
    } else if (viewStyle === "Line") {
      currViewer.setStyle({}, { line: { linewidth: 5.0 } });
    }
    currViewer.render();

    // Sometimes keys are added later. If that's the case, change the cubeKey to the first available.
    if (!cubeKey && Object.keys(props.cubeData).length > 0) {
      setCubeKey(Object.keys(props.cubeData)[0]);
    }
  }, [viewStyle, isoval, cubeKey, props.moleculeData, props.cubeData]);

  return (
    <div id="viewer-container">
      <div
        id="viewer"
        ref={viewerRef}
        style="width: 640px; height: 480px;"
      ></div>

      <div id="view-dropdown-container" class="view-option">
        <label for="viewSelector">Visualization Style:</label>
        <select
          id="viewSelector"
          onChange={(e) => {
            const style = (e.target as HTMLSelectElement).value;
            setViewStyle(style);
          }}
        >
          <option value="Sphere">Sphere</option>
          <option value="Stick">Stick</option>
          <option value="Line">Line</option>
        </select>
      </div>
      {cubeKey ? (
        <>
          <div id="cube-dropdown-container" class="view-option">
            <label for="cubeSelector">Cube selection:</label>
            <select
              id="cubeSelector"
              onChange={(e) => {
                const key = (e.target as HTMLSelectElement).value;
                setCubeKey(key);
              }}
            >
              {Object.keys(props.cubeData).map((key) => (
                <option value={key} selected={key === cubeKey}>
                  {key}
                </option>
              ))}
            </select>
          </div>

          <div id="isoval-slider-container" class="view-option">
            <label for="isovalSlider">Adjust isovalue:</label>
            <input
              type="range"
              id="isovalSlider"
              min="0.005"
              max="0.1"
              step="0.005"
              value={isoval}
              onInput={(e) => {
                const new_isoval = parseFloat(
                  (e.target as HTMLInputElement).value,
                );
                setIsoval(new_isoval);
              }}
            />
          </div>
        </>
      ) : null}
    </div>
  );
}
