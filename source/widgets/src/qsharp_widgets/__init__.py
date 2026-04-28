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


class Circuit(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("Circuit").tag(sync=True)
    circuit_json = traitlets.Unicode().tag(sync=True)

    def __init__(self, circuit):
        super().__init__(circuit_json=circuit.json())
        self.layout.overflow = "visible scroll"


class Atoms(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("Atoms").tag(sync=True)
    machine_layout = traitlets.Dict().tag(sync=True)
    trace_data = traitlets.Dict().tag(sync=True)

    def __init__(self, machine_layout, trace_data):
        super().__init__(machine_layout=machine_layout, trace_data=trace_data)


class Entanglement(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("Entanglement").tag(sync=True)
    s1_entropies = traitlets.List().tag(sync=True)
    mutual_information = traitlets.List().tag(sync=True)
    labels = traitlets.List().tag(sync=True)
    selected_indices = traitlets.List(allow_none=True, default_value=None).tag(sync=True)
    groups = traitlets.Dict(allow_none=True, default_value=None).tag(sync=True)
    options = traitlets.Dict().tag(sync=True)

    def __init__(
        self,
        wavefunction=None,
        *,
        s1_entropies=None,
        mutual_information=None,
        labels=None,
        selected_indices=None,
        groups=None,
        **options,
    ):
        """
        Displays an entanglement chord diagram.

        Can be constructed either from a ``Wavefunction`` object or from raw
        entropy / mutual-information arrays.

        Parameters
        ----------
        wavefunction : optional
            A ``qdk.chemistry.Wavefunction`` instance (from the
            ``qdk-chemistry`` package) with single-orbital entropies and
            mutual information.  When provided, *s1_entropies* and
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
            Orbital indices to highlight (single group, legacy API).
        groups : dict[str, list[int]], optional
            Named groups of orbital indices.  Each group is rendered with
            a distinct outline colour and, when grouped, its members are
            placed adjacent on the ring.  Takes precedence over
            *selected_indices* for grouping when both are provided.
        **options
            Visual knobs forwarded to the JS component.  All are
            optional; snake_case names are converted to camelCase
            automatically.

            ``gap_deg`` : float
                Gap in degrees between adjacent arcs.  Default ``3``.
            ``radius`` : float
                Outer radius of the ring in SVG user units.  Default ``1``.
            ``arc_width`` : float
                Radial thickness of each arc as a fraction of the
                radius.  Default ``0.08``.
            ``line_scale`` : float or None
                Chord width multiplier.  ``None`` (default) auto-scales
                to the data range.
            ``mi_threshold`` : float
                Minimum mutual-information value to draw a chord.
                Default ``0`` (draw all).
            ``s1_vmax`` : float or None
                Clamp for the single-orbital-entropy colour scale.
                Default ``ln(4)``.
            ``mi_vmax`` : float or None
                Clamp for the mutual-information colour scale.
                Default ``ln(16)``.
            ``title`` : str or None
                Title shown above the diagram.
                Default ``"Entanglement"``.
            ``width`` : int
                SVG viewport width in pixels.  Default ``600``.
            ``height`` : int
                SVG viewport height in pixels.  Default ``660``.
            ``selection_color`` : str
                CSS colour for the highlight outline around selected
                arcs.  Default auto-detected from background luminance.
            ``selection_linewidth`` : float
                Stroke width of the selection outline.  Default ``1.2``.
            ``group_colors`` : list[str]
                Override outline colours for each group (cycles if
                fewer colours than groups).
            ``group_selected`` : bool
                When ``True``, reorder arcs so that members of each
                group sit adjacent on the ring.  Default ``False``.
        """
        if wavefunction is not None:
            try:
                from qdk.chemistry import Wavefunction as _Wavefunction
            except ImportError:
                raise ImportError(
                    "The 'qdk-chemistry' package is required when passing a "
                    "wavefunction object.  Install it with:  pip install qdk-chemistry"
                ) from None
            if not isinstance(wavefunction, _Wavefunction):
                raise TypeError(
                    f"Expected a qdk.chemistry.Wavefunction instance, "
                    f"got {type(wavefunction).__qualname__}"
                )

            raw_s1 = wavefunction.get_single_orbital_entropies()
            raw_mi = wavefunction.get_mutual_information()
            # Accept numpy arrays or plain lists; normalise to plain lists.
            s1_entropies = (
                raw_s1.tolist() if hasattr(raw_s1, "tolist") else list(raw_s1)
            )
            mutual_information = (
                raw_mi.tolist() if hasattr(raw_mi, "tolist") else [list(row) for row in raw_mi]
            )
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

        if labels is None:
            labels = [str(i) for i in range(len(s1_entropies))]

        super().__init__(
            s1_entropies=s1_entropies,
            mutual_information=mutual_information,
            labels=labels,
            selected_indices=selected_indices,
            groups=groups,
            options=options,
        )


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
        - cube_data (optional): a dictionary where keys are cube names and values are dictionaries with the following structure:
          - "data": string containing the cube data in .cube file format.
          - "info": (optional) a dictionary containing any metadata you want to display with the cube data.
        """
        super().__init__(
            molecule_data=molecule_data, cube_data=cube_data, isoval=isoval
        )
