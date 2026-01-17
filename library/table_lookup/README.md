# table_lookup library

The `table_lookup` library defines various primitives useful to perform computation and uncomputation of table lookup. It also defines wrapper function
which uses one of the approaches depending on options.

## Lookup

`Lookup` is the main operation implementing various table lookup algorithms and options. Note, that most unlookup algorithms are measurement-based and return target register to zero state.

### Options for lookup

* `DoStdLookup` - Use lookup algorithm defined in the Q# standard library.
* `DoMCXLookup` - Use naive lookup algorithm via multicontrolled X gates. See [arXiv:1805.03662](https://arxiv.org/abs/1805.03662), Section A.
* `DoRecursiveSelectLookup` - Use select network algorithm via recursion. See [arXiv:2211.01133](https://arxiv.org/abs/2211.01133), Section 2.
* `DoPPLookup` - Use lookup algorithm via power products without address split. See [arXiv:2505.15917](https://arxiv.org/abs/2505.15917), Section A.4.
* `DoSplitPPLookup` - Use lookup algorithm via power products with address split. See [arXiv:2505.15917](https://arxiv.org/abs/2505.15917), Section A.4.

### Options for unlookup

* `DoStdUnlookup` - Use unlookup algorithm defined in the Q# standard library.
* `DoUnlookupViaLookup` - Perform unlookup via the same algorithm as lookup as it is self-adjoint.
* `DoMCXUnlookup` - Perform measurement-based unlookup with corrections via multicontrolled X gates. See [arXiv:2211.01133](https://arxiv.org/abs/2211.01133), Section 2.
* `DoPPUnlookup` - Perform measurement-based unlookup with corrections via power products without address split (Phase lookup). See [arXiv:2505.15917](https://arxiv.org/abs/2505.15917), Section A.3.
* `DoSplitPPUnlookup` - Perform measurement-based unlookup with corrections via power products with address split (Phase lookup). See [arXiv:2505.15917](https://arxiv.org/abs/2505.15917), Section A.3.

## Potential future work

* Add more control how uncomputation of AND gate is performed.
* Add resource estimation hints.
* If gate set includes multi-target gates, code can be optimized to use those.
* Implement delayed combined corrections.
