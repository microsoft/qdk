# Q# widgets

The Q# widgets are Jupyter Widgets built using the AnyWidget package <https://anywidget.dev/>

Build with `./build.py --widgets`.

Install the built wheel via

```bash
pip install --force-reinstall ./target/wheels/qsharp_widgets-0.0.0-py2.py3-none-any.whl
```

For development, perform an 'editable' install with `pip install -e ./widgets` in
the venv used for testing. Any changes made to the package are then immediately reflected.

If developing the web code (JS and CSS), then in the test environment also install
`pip install watchfiles`, and in the `./widgets` directory run `npm run dev` to
build in watch mode. This will use AnyWidget's hot module reloading to automatically
update the Python package as changes are made. (See <https://anywidget.dev/blog/anywidget-02/>).

With the above done, use the Q# widgets in you Python test environment via `import qsharp_widgets`.

## Usage

In a notebook, generate the estimates for a program and display the widgets with
code such as that shown below:

```python
# Cell-1 : Import the modules and generate some estimates
from qdk import qsharp
from qdk.widgets import SpaceChart, EstimateDetails

with open("sample.qs", "r") as f:
    contents = f.read()
qsharp.eval(contents)
result1 = qsharp.estimate("Sample.Main()")

# Cell-2 : Display the details in table form
EstimateDetails(result1)

# Cell-3 : Display the space chart
SpaceChart(result1)

# Cell-4 : Use the logical counts to get estimates for a different qubit
result2 = qsharp.physical_estimates_from_logical_counts(
    result1.get("logicalCounts"),
    {
        "qubitParams": { "name": "qubit_maj_ns_e6" },
        "qecScheme": { "name": "floquet_code" }
    }
)
SpaceChart(result2)
```
