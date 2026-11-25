// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The table of a library's functions, and how it is filled.
//!
//! The crate links no NVIDIA library. Each function it calls is looked up
//! by name, at run time, in the library file [`discover`](crate::discover)
//! found. A library's function table holds those functions, and every call
//! into the library goes through it.
//!
//! `function_table!` declares a library's table: a struct with one field per
//! function the crate uses, named for the function's C symbol. For example,
//! the row `required device::cudaSetDevice` becomes the field
//! `cudaSetDevice`, looked up as `"cudaSetDevice"`. Filling a table looks
//! each function up through a [`SymbolResolver`]. It stores the function's
//! address and does not call it. It returns every missing required
//! function, so that the user fixes them all at once.

use crate::LibraryFault;

/// Where a table's functions are looked up. It is a trait so that a table
/// can be filled, and tested, without a library file.
pub(crate) trait SymbolResolver {
    /// The function exported under `symbol`, or `None` when the library
    /// does not export it or exports it at address zero.
    ///
    /// # Safety
    /// `T` must be the function-pointer type of the function exported under
    /// `symbol`, and the library must outlive every use of the result.
    unsafe fn resolve<T: Copy>(&self, symbol: &'static str) -> Option<T>;
}

/// A table of one library's functions, which `function_table!` declares.
pub(crate) trait FunctionTable: Sized {
    /// Looks up every function of the table through `resolver`. When a
    /// required one is missing, it returns every missing required one, in
    /// the table's order.
    fn resolve<R: SymbolResolver>(resolver: &R) -> Result<Self, Vec<LibraryFault>>;
}

/// Declares a table of function pointers, one row per function:
/// `required family::symbol` or `optional family::symbol`. Here
/// `family::symbol` is the function's pointer type, declared in its
/// family's file under the function's C name.
///
/// It makes the field `symbol` (an `Option` when optional) and its lookup
/// by `stringify!(symbol)` ([`FunctionTable`]). It also makes
/// `const _: family::symbol = declarations::symbol`, which compiles only if
/// the type is the symbol's declared signature. Each missing required
/// symbol is a fault naming it. A missing optional one is `None`. As a
/// field is the symbol it is looked up by, no field can hold another
/// function. No table can hold a function twice: two fields of one name do
/// not compile.
macro_rules! function_table {
    (
        $(#[$attribute:meta])*
        struct $table:ident, declared in $declarations:ident {
            $($need:ident $family:ident::$symbol:ident,)*
        }
    ) => {
        $(#[$attribute])*
        #[allow(non_snake_case, reason = "each field is its function's C symbol")]
        pub(crate) struct $table {
            $(pub(crate) $symbol: $crate::libraries::function_table::function_table!(
                @field $need $family::$symbol
            ),)*
        }

        $(const _: $family::$symbol = $declarations::$symbol;)*

        impl $crate::libraries::function_table::FunctionTable for $table {
            #[allow(non_snake_case, reason = "each binding is its function's C symbol")]
            fn resolve<R: $crate::libraries::function_table::SymbolResolver>(
                resolver: &R,
            ) -> Result<Self, Vec<$crate::LibraryFault>> {
                #[allow(unused_mut, reason = "a table of only optional rows adds no fault")]
                let mut missing = Vec::new();
                $(let $symbol = $crate::libraries::function_table::function_table!(
                    @lookup $need resolver, &mut missing, $symbol
                );)*
                if !missing.is_empty() {
                    return Err(missing);
                }
                Ok(Self {
                    $($symbol: $crate::libraries::function_table::function_table!(@take $need $symbol),)*
                })
            }
        }

        impl $table {
            /// The rows, as `(symbol, required)`.
            #[cfg(test)]
            #[allow(dead_code, reason = "only the tests of some tables read their rows")]
            const ROWS: &[(&str, bool)] = &[
                $((
                    stringify!($symbol),
                    $crate::libraries::function_table::function_table!(@required $need),
                ),)*
            ];
        }
    };
    (@field required $type:ty) => { $type };
    (@field optional $type:ty) => { Option<$type> };
    (@lookup required $resolver:ident, $missing:expr, $symbol:ident) => {
        $crate::libraries::function_table::resolve_required($resolver, $missing, stringify!($symbol))
    };
    (@lookup optional $resolver:ident, $missing:expr, $symbol:ident) => {
        $crate::libraries::function_table::resolve_optional($resolver, stringify!($symbol))
    };
    (@take required $symbol:ident) => {
        $symbol.expect("no required function is missing: `resolve` returned otherwise")
    };
    (@take optional $symbol:ident) => { $symbol };
    (@required required) => { true };
    (@required optional) => { false };
}

pub(crate) use function_table;

/// A required row's lookup, for `function_table!`. It returns the function,
/// or `None` with the fault naming it added to `missing`.
pub(crate) fn resolve_required<R: SymbolResolver, T: Copy>(
    resolver: &R,
    missing: &mut Vec<LibraryFault>,
    symbol: &'static str,
) -> Option<T> {
    // SAFETY: `function_table!` gives `T` as the type checked against the
    // symbol's declaration; the library is retained by `LoadedLibraries`.
    let function = unsafe { resolver.resolve(symbol) };
    if function.is_none() {
        missing.push(LibraryFault::MissingRequiredSymbol { symbol });
    }
    function
}

/// An optional row's lookup, for `function_table!`: the function, if the
/// library exports it.
pub(crate) fn resolve_optional<R: SymbolResolver, T: Copy>(
    resolver: &R,
    symbol: &'static str,
) -> Option<T> {
    // SAFETY: as in `resolve_required`.
    unsafe { resolver.resolve(symbol) }
}

#[cfg(all(test, unix))]
mod tests;
