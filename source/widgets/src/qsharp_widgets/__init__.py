# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import importlib.metadata
import pathlib
import time
from typing import Literal

import anywidget
import traitlets

try:
    __version__ = importlib.metadata.version("qsharp_widgets")
except importlib.metadata.PackageNotFoundError:
    __version__ = "unknown"


class SpaceChart(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("SpaceChart").tag(sync=True)
    estimates = traitlets.Dict().tag(sync=True)
    index = traitlets.Integer().tag(sync=True)

    def __init__(self, estimates, index=None):
        """
        This function generates a chart for the qubit utilization of the estimates.

        Parameters:
        - estimates: data for the chart.
        - index (optional): the index of the estimate to be displayed. In case of a single point estimate, the parameter is ignored. In case of the frontier estimate, indexes correspond to points on frontier from the shortest runtime to the longest one. If not provided, the shortest runtime estimate is displayed.
        """
        super().__init__(estimates=estimates, index=0 if index is None else index)


class EstimatesOverview(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("EstimatesOverview").tag(sync=True)
    estimates = traitlets.Dict().tag(sync=True)
    colors = traitlets.List().tag(sync=True)
    runNames = traitlets.List().tag(sync=True)

    def __init__(self, estimates, colors=None, runNames=None):
        """
        This function generates a summary results table with a qubit-time diagram.

        Parameters:
        - estimates: data for the table and the diagram.
        - colors (optional): the list of colors which could be provided in the hex form or by name. If the length of the list does not match the number of the estimates, the colors parameter will be ignored and replaced with defaults.
        - runNames (optional): the list of the run names. If the length of the list does not match the number of the estimates, the runNames parameter will be ignored and replaced with defaults.

        Returns:
        None
        """
        super().__init__(
            estimates=estimates,
            colors=[] if colors is None else colors,
            runNames=[] if runNames is None else runNames,
        )


class EstimatesPanel(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("EstimatesPanel").tag(sync=True)
    estimates = traitlets.Dict().tag(sync=True)
    colors = traitlets.List().tag(sync=True)
    runNames = traitlets.List().tag(sync=True)

    def __init__(self, estimates, colors=None, runNames=None):
        """
        This function generates the whole estimates panel with the summary results table, the space-time chart, the space chart and the details report.

        Parameters:
        - estimates: data for all the tables and diagrams.
        - colors (optional): the list of colors which could be provided in the hex form or by name. If the length of the list does not match the number of the estimates, the colors parameter will be ignored and replaced with defaults.
        - runNames (optional): the list of the run names. If the length of the list does not match the number of the estimates, the runNames parameter will be ignored and replaced with defaults.

        Returns:
        None
        """
        super().__init__(
            estimates=estimates,
            colors=[] if colors is None else colors,
            runNames=[] if runNames is None else runNames,
        )


class EstimateDetails(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("EstimateDetails").tag(sync=True)
    estimates = traitlets.Dict().tag(sync=True)
    index = traitlets.Integer().tag(sync=True)

    def __init__(self, estimates, index=None):
        """
        This function generates a report for the qubit utilization of the estimates.

        Parameters:
        - estimates: data for the report.
        - index (optional): the index of the estimate to be displayed. In case of a single point estimate, the parameter is ignored. In case of the frontier estimate, indexes correspond to points on frontier from the shortest runtime to the longest one. If not provided, the shortest runtime estimate is displayed.
        """
        super().__init__(estimates=estimates, index=0 if index is None else index)


class Histogram(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("Histogram").tag(sync=True)
    buckets = traitlets.Dict().tag(sync=True)
    shot_count = traitlets.Integer().tag(sync=True)
    shot_header = traitlets.Bool(True).tag(sync=True)
    labels = traitlets.Unicode("raw").tag(sync=True)
    items = traitlets.Unicode("all").tag(sync=True)
    sort = traitlets.Unicode("a-to-z").tag(sync=True)

    def _update_ui(self):
        self.buckets = self._new_buckets.copy()
        self.shot_count = self._new_count
        self._last_message = time.time()

    def _add_result(self, result):
        result_str = str(result["result"])
        old_value = self._new_buckets.get(result_str, 0)
        self._new_buckets.update({result_str: old_value + 1})
        self._new_count += 1

        # Only update the UI max 10 times per second
        if time.time() - self._last_message >= 0.1:
            self._update_ui()

    def __init__(
        self,
        results=None,
        *,
        shot_header=True,
        bar_values=None,
        labels: Literal["raw", "kets", "none"] = "raw",
        items: Literal["all", "top-10", "top-25"] = "all",
        sort: Literal["a-to-z", "high-to-low", "low-to-high"] = "a-to-z",
    ):
        # Set up initial values before calling super().__init__()
        self._new_buckets = {}
        self._new_count = 0
        self._last_message = time.time()

        # Calculate initial traitlet values
        initial_shot_header = shot_header
        initial_buckets = {}
        initial_shot_count = 0

        # If provided a list of results, count the buckets and update.
        # Need to distinguish between the case where we're provided a list of results
        # or a list of ShotResults
        if results is not None:
            for result in results:
                if isinstance(result, dict) and "result" in result:
                    self._add_result(result)
                else:
                    # Convert the raw result to a ShotResult for the call
                    self._add_result({"result": result, "events": []})

            initial_buckets = self._new_buckets.copy()
            initial_shot_count = self._new_count
        elif bar_values is not None:
            initial_buckets = bar_values
            initial_shot_count = 0
            initial_shot_header = False

        # Pass all initial values to super().__init__()
        super().__init__(
            shot_header=initial_shot_header,
            buckets=initial_buckets,
            shot_count=initial_shot_count,
            labels=labels,
            items=items,
            sort=sort,
        )

    def run(self, entry_expr, shots):
        import qsharp

        self._new_buckets = {}
        self._new_count = 0

        # Note: For now, we don't care about saving the results, just counting
        # up the results for each bucket. If/when we add output details and
        # navigation, then we'll need to save the results. However, we pass
        # 'save_results=True' to avoid printing to the console.
        qsharp.run(entry_expr, shots, on_result=self._add_result, save_events=True)

        # Update the UI one last time to make sure we show the final results
        self._update_ui()

    def _build_svg_props(self, dark_mode=False):
        """Build the props dict for SVG rendering."""
        return {
            "data": dict(self.buckets),
            "shotCount": self.shot_count,
            "filter": "",
            "labels": self.labels,
            "items": self.items,
            "sort": self.sort,
            "darkMode": bool(dark_mode),
        }

    def export_svg(self, path=None, dark_mode=False):
        """Render the histogram to a standalone SVG.

        When the widget is displayed in a notebook the SVG is rendered
        in-browser by the same ``histogramToSvg`` function used by
        the Node.js SSR script.  When the widget is not connected the
        function falls back to spawning Node.js.

        The traitlets (including interactive state like labels/items/sort
        which the front-end syncs back) are read at call time, so exports
        always reflect the latest user changes.

        Parameters
        ----------
        path : str or Path, optional
            When given the SVG is written to this file and the path is
            returned.  Otherwise the SVG markup string is returned.
        dark_mode : bool
            When ``True`` the exported SVG uses light text on a dark
            background; when ``False`` (default) dark text on a light
            background.

        Returns
        -------
        str
            SVG markup (when *path* is ``None``) or the file path.
        """
        svg = self._export_svg_via_widget(dark_mode)
        if svg is None:
            props = self._build_svg_props(dark_mode)
            svg = _render_component_node("Histogram", props)

        if path is not None:
            from pathlib import Path as _P

            _P(path).write_text(svg, encoding="utf-8")
            return str(path)
        return svg

    def _export_svg_via_widget(self, dark_mode=False):
        """Try to render SVG in-browser via the live widget front-end.

        Sends a custom message asking the JS side to call
        ``histogramToSvg()`` and waits for the response.  Returns
        ``None`` if the widget is not connected.
        """
        import threading

        result = [None]
        event = threading.Event()

        def _on_msg(_, content, buffers):
            if isinstance(content, dict) and content.get("type") == "svg_result":
                result[0] = content.get("svg")
                event.set()

        try:
            self.on_msg(_on_msg)
            self.send({"type": "export_svg", "dark_mode": bool(dark_mode)})
            if event.wait(timeout=5):
                return result[0]
        except Exception:
            pass
        finally:
            try:
                self.on_msg(_on_msg, remove=True)
            except Exception:
                pass
        return None


class Circuit(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("Circuit").tag(sync=True)
    circuit_json = traitlets.Unicode().tag(sync=True)

    def __init__(self, circuit):
        super().__init__(circuit_json=circuit.json())
        self.layout.overflow = "visible scroll"

    def export_svg(self, path=None, dark_mode=False, gates_per_row=0, render_depth=0):
        """Render the circuit to a standalone SVG.

        Parameters
        ----------
        path : str or Path, optional
            When given the SVG is written to this file and the path is
            returned.  Otherwise the SVG markup string is returned.
        dark_mode : bool
            When ``True`` the exported SVG uses light-on-dark colours.
        gates_per_row : int
            Maximum gate columns per row before wrapping.  ``0`` (default)
            means no wrapping.
        render_depth : int
            How many levels of grouped operations to expand.
            ``0`` (default) shows groups as collapsed boxes.
            ``1`` expands one level, showing children inline.
            Use a large number (e.g. 99) to fully expand.

        Returns
        -------
        str
            SVG markup (when *path* is ``None``) or the file path.
        """
        props = {
            "circuit": self.circuit_json,
            "dark_mode": bool(dark_mode),
            "gates_per_row": int(gates_per_row),
            "render_depth": int(render_depth),
        }
        svg = _render_component_node("Circuit", props)

        if path is not None:
            from pathlib import Path as _P

            _P(path).write_text(svg, encoding="utf-8")
            return str(path)
        return svg


class Atoms(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("Atoms").tag(sync=True)
    machine_layout = traitlets.Dict().tag(sync=True)
    trace_data = traitlets.Dict().tag(sync=True)

    def __init__(self, machine_layout, trace_data):
        super().__init__(machine_layout=machine_layout, trace_data=trace_data)


class ChordDiagram(anywidget.AnyWidget):
    """General-purpose chord diagram widget.

    Displays per-node scalar values as coloured arcs and pairwise weights
    as chords.  ``OrbitalEntanglement`` is a convenience subclass that
    maps orbital-specific terminology onto these general parameters.

    The component renders self-contained SVG with inline styles, so the
    same ``export_svg()`` code path (server-side ``renderToString``)
    works identically whether or not a live DOM is available.  Interactive
    state (e.g. the grouping toggle) is synced back to the ``options``
    traitlet by the JS front-end so exports always reflect the latest
    user changes.
    """

    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("ChordDiagram").tag(sync=True)
    node_values = traitlets.List().tag(sync=True)
    pairwise_weights = traitlets.List().tag(sync=True)
    labels = traitlets.List().tag(sync=True)
    selected_indices = traitlets.List(allow_none=True, default_value=None).tag(
        sync=True
    )
    options = traitlets.Dict().tag(sync=True)

    def __init__(
        self,
        node_values,
        pairwise_weights,
        *,
        labels=None,
        selected_indices=None,
        group_selected=False,
        **options,
    ):
        """Create a chord diagram.

        Parameters
        ----------
        node_values : list[float]
            Per-node scalar values (length *N*).  Drives arc colour.
        pairwise_weights : list[list[float]]
            N×N symmetric weight matrix.  Drives chord colour / width.
        labels : list[str], optional
            Node labels.  Defaults to ``["0", "1", …]``.
        selected_indices : list[int], optional
            Node indices to highlight.
        group_selected : bool, optional
            When ``True``, reorder arcs so that selected nodes sit
            adjacent on the ring.  Defaults to ``False``.
        **options
            Forwarded to the JS component as visual knobs
            (``gap_deg``, ``radius``, ``arc_width``, ``line_scale``,
            ``edge_threshold``, ``node_vmax``, ``edge_vmax``,
            ``node_colormap``, ``edge_colormap``,
            ``node_colorbar_label``, ``edge_colorbar_label``,
            ``node_hover_prefix``, ``edge_hover_prefix``,
            ``title``, ``width``, ``height``, ``selection_color``,
            ``selection_linewidth``).
        """
        n = len(node_values)
        if labels is None:
            labels = [str(i) for i in range(n)]

        opts = dict(options)
        opts["group_selected"] = bool(group_selected)

        super().__init__(
            node_values=list(node_values),
            pairwise_weights=[list(row) for row in pairwise_weights],
            labels=list(labels),
            selected_indices=list(selected_indices) if selected_indices else None,
            options=opts,
        )

    def _build_svg_props(self, dark_mode=False):
        """Build the camelCase props dict for SVG rendering."""
        props: dict = {
            "nodeValues": list(self.node_values),
            "pairwiseWeights": [list(row) for row in self.pairwise_weights],
            "labels": list(self.labels),
            "darkMode": bool(dark_mode),
        }
        if self.selected_indices:
            props["selectedIndices"] = list(self.selected_indices)
        for key, val in (self.options or {}).items():
            props[_snake_to_camel(key)] = val
        return props

    def export_svg(self, path=None, dark_mode=False):
        """Render the diagram to a standalone SVG.

        When the widget is displayed in a notebook the SVG is rendered
        in-browser by the same ``chordDiagramToSvg`` function used by
        the Node.js SSR script — one rendering path everywhere.  When
        the widget is not connected (e.g. a plain Python script) the
        function falls back to spawning Node.js.

        The ``options`` traitlet (including interactive state like the
        grouping toggle) is read at call time, so exports always
        reflect the latest user changes.

        Parameters
        ----------
        path : str or Path, optional
            When given the SVG is written to this file and the path is
            returned.  Otherwise the SVG markup string is returned.
        dark_mode : bool
            When ``True`` the exported SVG uses light text on a dark
            background; when ``False`` (default) dark text on a
            transparent background.

        Returns
        -------
        str
            SVG markup (when *path* is ``None``) or the file path.
        """
        svg = self._export_svg_via_widget(dark_mode)
        if svg is None:
            props = self._build_svg_props(dark_mode)
            svg = _render_component_node("ChordDiagram", props)

        if path is not None:
            from pathlib import Path as _P

            _P(path).write_text(svg, encoding="utf-8")
            return str(path)
        return svg

    def _export_svg_via_widget(self, dark_mode=False):
        """Try to render SVG in-browser via the live widget front-end.

        Sends a custom message asking the JS side to call
        ``chordDiagramToSvg()`` and waits for the response.  Returns
        ``None`` if the widget is not connected.
        """
        import threading

        result = [None]
        event = threading.Event()

        def _on_msg(_, content, buffers):
            if isinstance(content, dict) and content.get("type") == "svg_result":
                result[0] = content.get("svg")
                event.set()

        try:
            self.on_msg(_on_msg)
            self.send({"type": "export_svg", "dark_mode": bool(dark_mode)})
            # Wait up to 5 seconds for the front-end to respond
            if event.wait(timeout=5):
                return result[0]
        except Exception:
            pass
        finally:
            try:
                self.on_msg(_on_msg, remove=True)
            except Exception:
                pass
        return None


class OrbitalEntanglement(ChordDiagram):
    """Orbital entanglement chord diagram.

    Convenience subclass of ``ChordDiagram`` that accepts
    orbital-specific terminology (``s1_entropies``,
    ``mutual_information``) and supplies sensible defaults for quantum
    chemistry visualisation (colorbar labels, scale maxima, hover
    prefixes).
    """

    def __init__(
        self,
        wavefunction=None,
        *,
        s1_entropies=None,
        mutual_information=None,
        labels=None,
        selected_indices=None,
        group_selected=False,
        mi_threshold=None,
        s1_vmax=None,
        mi_vmax=None,
        title="Orbital Entanglement",
        **options,
    ):
        """Create an orbital entanglement diagram.

        Can be constructed either from a ``Wavefunction`` object or from
        raw entropy / mutual-information arrays.

        Parameters
        ----------
        wavefunction : optional
            A ``Wavefunction`` with single-orbital entropies and mutual
            information.  When provided, *s1_entropies* and
            *mutual_information* are extracted automatically.
        s1_entropies : list[float], optional
            Single-orbital entropies (length *N*).  Required when
            *wavefunction* is not given.
        mutual_information : list[list[float]], optional
            N×N mutual-information matrix.  Required when *wavefunction*
            is not given.
        labels : list[str], optional
            Orbital labels.  Defaults to ``["0", "1", …]``.
        selected_indices : list[int], optional
            Orbital indices to highlight.
        group_selected : bool, optional
            When ``True``, reorder arcs so that selected orbitals sit
            adjacent on the ring.  Defaults to ``False``.
        mi_threshold : float, optional
            Minimum mutual information to draw a chord.
        s1_vmax : float, optional
            Clamp for the single-orbital entropy colour scale.
            Defaults to ``ln(4)``.
        mi_vmax : float, optional
            Clamp for the mutual-information colour scale.
            Defaults to ``ln(16)``.
        title : str, optional
            Diagram title.  Defaults to ``"Orbital Entanglement"``.
        **options
            Additional visual knobs forwarded to the JS component.
        """
        import math

        if wavefunction is not None:
            import numpy as np

            s1_entropies = np.asarray(
                wavefunction.get_single_orbital_entropies()
            ).tolist()
            mutual_information = np.asarray(
                wavefunction.get_mutual_information()
            ).tolist()
            n = len(s1_entropies)
            if labels is None:
                try:
                    orbitals = wavefunction.get_orbitals()
                    if orbitals.has_active_space():
                        active_indices = orbitals.get_active_space_indices()[0]
                        labels = [str(idx) for idx in active_indices]
                    else:
                        labels = [str(i) for i in range(n)]
                except (AttributeError, TypeError, IndexError):
                    labels = [str(i) for i in range(n)]
        elif s1_entropies is None or mutual_information is None:
            raise ValueError(
                "Either 'wavefunction' or both 's1_entropies' and "
                "'mutual_information' must be provided."
            )

        # Map OE-specific params to generic ChordDiagram options
        if mi_threshold is not None:
            options.setdefault("edge_threshold", mi_threshold)
        options.setdefault("node_vmax", s1_vmax if s1_vmax is not None else math.log(4))
        options.setdefault(
            "edge_vmax", mi_vmax if mi_vmax is not None else math.log(16)
        )
        options.setdefault("node_colorbar_label", "Single-orbital entropy")
        options.setdefault("edge_colorbar_label", "Mutual information")
        options.setdefault("node_hover_prefix", "S\u2081=")
        options.setdefault("edge_hover_prefix", "MI=")
        options.setdefault("title", title)

        super().__init__(
            node_values=s1_entropies,
            pairwise_weights=mutual_information,
            labels=labels,
            selected_indices=selected_indices,
            group_selected=group_selected,
            **options,
        )


# ---------------------------------------------------------------------------
# Server-side SVG rendering via Node.js (same components as the widget)
# ---------------------------------------------------------------------------

# Path to the Node SSR helper script bundled alongside the widget JS.
_RENDER_SVG_SCRIPT = pathlib.Path(__file__).parent / "static" / "render_svg.mjs"

# Path to the headless PNG renderer (Playwright + 3Dmol).
_RENDER_PNG_SCRIPT = pathlib.Path(__file__).parent / "static" / "render_png.mjs"


def _snake_to_camel(name: str) -> str:
    """Convert ``snake_case`` to ``camelCase``."""
    parts = name.split("_")
    return parts[0] + "".join(p.capitalize() for p in parts[1:])


def _render_component_node(component: str, props: dict) -> str:
    """Render a component server-side via Node.js.

    Parameters
    ----------
    component : str
        Component name (``"ChordDiagram"``, ``"Histogram"``,
        ``"Circuit"``).
    props : dict
        Props dict that will be JSON-serialised and passed to the JS
        component.

    Returns
    -------
    str
        The rendered SVG / HTML markup.
    """
    import json
    import shutil
    import subprocess

    node = shutil.which("node")
    if node is None:
        raise RuntimeError(
            "Node.js is required for server-side SVG rendering but "
            "'node' was not found on the PATH."
        )

    payload = json.dumps({"component": component, "props": props})

    result = subprocess.run(
        [node, str(_RENDER_SVG_SCRIPT)],
        input=payload,
        capture_output=True,
        text=True,
        timeout=30,
    )

    if result.returncode != 0:
        raise RuntimeError(
            f"Node SSR render failed (exit {result.returncode}):\n" f"{result.stderr}"
        )

    return result.stdout


class MoleculeViewer(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("MoleculeViewer").tag(sync=True)
    molecule_data = traitlets.Unicode().tag(sync=True)
    cube_data = traitlets.Dict().tag(sync=True)
    isoval = traitlets.Float(0.02).tag(sync=True)

    def __init__(self, molecule_data, cube_data={}, isoval=0.02):
        """
        This function generates a 3D molecule viewer for the provided molecular data in XYZ format.

        Parameters:
        - molecule_data: string containing the molecular data in XYZ format.
        """
        super().__init__(
            molecule_data=molecule_data, cube_data=cube_data, isoval=isoval
        )

    def export_png(
        self,
        path=None,
        width=640,
        height=480,
        style="Sphere",
        cube_label=None,
        iso_value=None,
    ):
        """Render the molecule to a PNG image using headless Chromium.

        Uses Playwright to launch a headless browser with 3Dmol — the
        same library as the interactive widget — so the output is
        pixel-identical.  Requires ``playwright`` (npm) and a Chromium
        browser (``npx playwright install chromium``).

        Parameters
        ----------
        path : str or Path, optional
            When given the PNG is written to this file and the path is
            returned.  Otherwise the raw PNG bytes are returned.
        width : int
            Image width in pixels.
        height : int
            Image height in pixels.
        style : str
            Visualisation style: ``"Sphere"`` (default), ``"Stick"``,
            or ``"Line"``.
        cube_label : str, optional
            Key into ``cube_data`` dict selecting which orbital to
            render.  When ``None`` and exactly one cube file is
            available it is used automatically.
        iso_value : float, optional
            Isovalue threshold for orbital rendering.  Defaults to the
            widget's ``isoval`` traitlet.

        Returns
        -------
        bytes or str
            PNG bytes (when *path* is ``None``) or the file path.
        """
        import json
        import shutil
        import subprocess

        node = shutil.which("node")
        if node is None:
            raise RuntimeError(
                "Node.js is required for PNG rendering but "
                "'node' was not found on the PATH."
            )

        props = {
            "molecule_data": self.molecule_data,
            "width": int(width),
            "height": int(height),
            "style": style,
        }

        # Resolve cube data
        cube_str = None
        if cube_label is not None:
            cube_str = self.cube_data.get(cube_label)
        elif len(self.cube_data) == 1:
            cube_str = next(iter(self.cube_data.values()))

        if cube_str is not None:
            props["cube_data"] = cube_str
            props["iso_value"] = float(
                iso_value if iso_value is not None else self.isoval
            )

        payload = json.dumps(props)

        result = subprocess.run(
            [node, str(_RENDER_PNG_SCRIPT)],
            input=payload,
            capture_output=True,
            text=False,
            timeout=30,
        )

        if result.returncode != 0:
            stderr = result.stderr.decode("utf-8", errors="replace")
            if "playwright" in stderr.lower() and "install" in stderr.lower():
                raise RuntimeError(
                    "PNG rendering requires a Chromium browser managed by "
                    "Playwright.  Install it once with:\n\n"
                    "    npx playwright install chromium\n\n"
                    "Then retry export_png()."
                )
            if "playwright is not installed" in stderr.lower():
                raise RuntimeError(
                    "PNG rendering requires the 'playwright' npm package "
                    "and a Chromium browser.\n\n"
                    "Install them with:\n"
                    "    npm install playwright\n"
                    "    npx playwright install chromium\n\n"
                    "Then retry export_png()."
                )
            raise RuntimeError(
                f"PNG render failed (exit {result.returncode}):\n{stderr}"
            )

        png_bytes = result.stdout

        if path is not None:
            from pathlib import Path as _P

            _P(path).write_bytes(png_bytes)
            return str(path)
        return png_bytes
