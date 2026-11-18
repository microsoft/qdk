// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The table of a library's functions, and how it is filled.
//!
//! `function_table!` declares a library's table: one row per function the
//! crate uses, each checked when the crate compiles against the function's
//! declaration in the library's bindings. Filling a table looks each
//! function up through a [`SymbolResolver`]; it stores the function's
//! address and does not call it. It returns every missing required
//! function, so that the user fixes them all at once. The MPS engine calls
//! the libraries through these tables, in
//! `engine::native_calls::loaded_libraries`.

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
    /// Looks up every function of the table through `resolver`; when a
    /// required one is missing, every missing required one, in the table's
    /// order.
    fn resolve<R: SymbolResolver>(resolver: &R) -> Result<Self, Vec<LibraryFault>>;
}

/// Declares a table of function pointers, one row per function:
/// `required field: Type = symbol` or `optional field: Type = symbol`.
///
/// It makes the field (an `Option` when optional), the lookup of the field
/// by `stringify!(symbol)` ([`FunctionTable`]), and `const _: Type =
/// declarations::symbol`, which compiles only if `Type` is the symbol's
/// declared signature. Each missing required symbol is a fault naming it; a
/// missing optional one is `None`.
macro_rules! function_table {
    (
        $(#[$attribute:meta])*
        struct $table:ident, declared in $declarations:ident {
            $($need:ident $field:ident: $type:ty = $symbol:ident,)*
        }
    ) => {
        $(#[$attribute])*
        pub(crate) struct $table {
            $(pub(crate) $field: $crate::libraries::function_table::function_table!(@field $need $type),)*
        }

        $(const _: $type = $declarations::$symbol;)*

        impl $crate::libraries::function_table::FunctionTable for $table {
            fn resolve<R: $crate::libraries::function_table::SymbolResolver>(
                resolver: &R,
            ) -> Result<Self, Vec<$crate::LibraryFault>> {
                #[allow(unused_mut, reason = "a table of only optional rows adds no fault")]
                let mut missing = Vec::new();
                $(let $field = $crate::libraries::function_table::function_table!(
                    @lookup $need resolver, &mut missing, $symbol
                );)*
                if !missing.is_empty() {
                    return Err(missing);
                }
                Ok(Self {
                    $($field: $crate::libraries::function_table::function_table!(@take $need $field),)*
                })
            }
        }

        impl $table {
            /// The rows, as `(field, symbol, required)`.
            #[cfg(test)]
            pub(crate) const ROWS: &[(&str, &str, bool)] = &[
                $((
                    stringify!($field),
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
    (@take required $field:ident) => {
        $field.expect("no required function is missing: `resolve` returned otherwise")
    };
    (@take optional $field:ident) => { $field };
    (@required required) => { true };
    (@required optional) => { false };
}

pub(crate) use function_table;

/// A required row's lookup, for `function_table!`: the function, or `None`
/// with the fault naming it added to `missing`.
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
