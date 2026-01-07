# table_lookup

The `table_lookup` library defines various primitives useful to perform computation and uncomputation of table lookup. It also defines wrapper function
which uses one of the approaches depending on options.

Options for lookup:

* `SelectViaStd` - Use select algorithm defined in the standard library.
* `SelectViaMCX` - Use basic select algorithm via multicontrolled X gates.
* `SelectViaRecursion` - Use select algorithm via recursion.
* `SelectViaPP` - Use select algorithm via power products without address split.
* `SelectViaSplitPP` - Use select algorithm via power products with address split.

Options for unlookup:

* `UnselectViaStd` - Use unselect algorithm defined in the standard library.
* `UnselectViaSelect` - Perform unselect via same algorithm as select as it is self-adjoint.
* `UnselectViaMCX` - Perform unselect via multicontrolled X gates.
* `UnselectViaPP` - Perform unselect via power products without address split (Phase lookup).
* `UnselectViaSplitPP` - Perform unselect via power products with address split (Phase lookup).

# Potential future work

* Add more control how uncomputation of AND gate is performed.
* Add resource estimation hints.
* If gate set includes multi-target gates, code can be optimized to use those.
* Implement delayed combined corrections.
