# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import importlib.metadata
import pathlib
import time

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
        super().__init__()
        self.estimates = estimates
        self.index = 0 if index is None else index


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
        super().__init__()
        self.estimates = estimates
        self.colors = [] if colors is None else colors
        self.runNames = [] if runNames is None else runNames


class EstimatesPanel(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("EstimatesPanel").tag(sync=True)
    estimates = traitlets.Dict().tag(sync=True)
    colors = traitlets.List().tag(sync=True)
    runNames = traitlets.List().tag(sync=True)

    def __init__(self, estimates, colors=None, runNames=None):
        """
        This function generates the whole estimates panel with the summary results table, ther space-time chart, the space chart and the details report.

        Parameters:
        - estimates: data for all the tables and diagrams.
        - colors (optional): the list of colors which could be provided in the hex form or by name. If the length of the list does not match the number of the estimates, the colors parameter will be ignored and replaced with defaults.
        - runNames (optional): the list of the run names. If the length of the list does not match the number of the estimates, the runNames parameter will be ignored and replaced with defaults.

        Returns:
        None
        """
        super().__init__()
        self.estimates = estimates
        self.colors = [] if colors is None else colors
        self.runNames = [] if runNames is None else runNames


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
        super().__init__()
        self.estimates = estimates
        self.index = 0 if index is None else index


class Histogram(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("Histogram").tag(sync=True)
    buckets = traitlets.Dict().tag(sync=True)
    shot_count = traitlets.Integer().tag(sync=True)

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

    def __init__(self, results=None):
        super().__init__()

        self._new_buckets = {}
        self._new_count = 0
        self._last_message = time.time()

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

            self._update_ui()

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
        super().__init__()
        self.circuit_json = circuit.json()
        self.layout.overflow = "visible scroll"


class Atoms(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("Atoms").tag(sync=True)
    machine_layout = traitlets.Dict().tag(sync=True)
    trace_data = traitlets.Dict().tag(sync=True)

    def __init__(self, machine_layout, trace_data):
        super().__init__()
        self.machine_layout = machine_layout
        self.trace_data = trace_data
