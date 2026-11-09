// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Opening a native library and looking up its functions.
//!
//! `libraries::open` opens each library the crate uses with
//! [`load_library`], from the first of its candidate paths that exists, and
//! fills the table of its functions, which `function_table!` declares. The
//! MPS engine, added in a later change, calls the libraries through those
//! tables. Looking a function up stores its address; it does not call it.
//! Opening returns its fault as a [`LibraryFault`]; filling a table returns
//! every missing function, so that the user fixes them all at once.

use crate::LibraryFault;
use libloading::{Library, os::unix as unix_library};
use std::path::{Path, PathBuf};

/// A library file the system loader opened, and the path it was opened
/// from. Dropping it closes the library.
pub(crate) struct OpenLibraryFile {
    path: PathBuf,
    library: Library,
}

impl OpenLibraryFile {
    /// The path the library was opened from.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

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

impl SymbolResolver for OpenLibraryFile {
    unsafe fn resolve<T: Copy>(&self, symbol: &'static str) -> Option<T> {
        // SAFETY: `T` is a function-pointer type (the caller's contract), so
        // `Option<T>` reads a zero address as `None`; `self.library`
        // outlives every copied function pointer.
        let resolved = unsafe { self.library.get::<Option<T>>(symbol.as_bytes()) }.ok()?;
        *resolved
    }
}

/// Opens the first of `paths`, a library's candidates in order, that exists.
///
/// An absolute path that does not exist is skipped. A bare file name is
/// searched for by the system loader, and skipped when the loader finds no
/// file of that name. Any other failure is the library's.
pub(crate) fn load_library(paths: Vec<PathBuf>) -> Result<OpenLibraryFile, LibraryFault> {
    for path in &paths {
        if path.is_absolute() && !path.exists() {
            continue;
        }

        // SAFETY: opening a library runs its initialisers (and, when it is
        // closed, its finalisers) as foreign code. The candidate is one of
        // the NVIDIA libraries `libraries` lists, by a fixed name or by an
        // absolute path the user set, and its initialisers are trusted as
        // any NVIDIA library's are. The `Library` is never closed while a
        // resolved pointer is live: it is kept in the `Arc<LoadedLibraries>`
        // that the pointers live in.
        let result = unsafe {
            unix_library::Library::open(
                Some(path.as_os_str()),
                unix_library::RTLD_NOW | unix_library::RTLD_LOCAL,
            )
        };
        match result {
            Ok(opened) => {
                return Ok(OpenLibraryFile {
                    path: path.clone(),
                    library: opened.into(),
                });
            }
            Err(error) => {
                let message = error.to_string();
                if path.is_absolute() || !no_file_of_that_name(path, &message) {
                    return Err(LibraryFault::LoadFailed {
                        path: path.clone(),
                        message,
                    });
                }
            }
        }
    }
    Err(LibraryFault::NotFound { attempted: paths })
}

/// Whether the system loader's `message`, on failing to open the bare file
/// name `candidate`, says that it found no file of that name.
///
/// glibc's loader starts its message with the name of the file it could not
/// open: the candidate's when there is no such file, a dependency's when the
/// candidate was found but needs a library that is missing. Only the first
/// lets discovery try the next candidate. Another loader's message (musl's,
/// or macOS's when the tests run there) is never read as "no such file", so
/// the file name is reported as a load failure rather than skipped.
pub(crate) fn no_file_of_that_name(candidate: &Path, message: &str) -> bool {
    candidate
        .to_str()
        .and_then(|name| message.strip_prefix(name))
        .and_then(|rest| rest.strip_prefix(": "))
        .is_some_and(|reason| reason.ends_with("No such file or directory"))
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
            $(pub(crate) $field: $crate::libraries::loader::function_table!(@field $need $type),)*
        }

        $(const _: $type = $declarations::$symbol;)*

        impl $crate::libraries::loader::FunctionTable for $table {
            fn resolve<R: $crate::libraries::loader::SymbolResolver>(
                resolver: &R,
            ) -> Result<Self, Vec<$crate::LibraryFault>> {
                let mut missing = Vec::new();
                $(let $field = $crate::libraries::loader::function_table!(
                    @lookup $need resolver, &mut missing, $symbol
                );)*
                if !missing.is_empty() {
                    return Err(missing);
                }
                Ok(Self {
                    $($field: $crate::libraries::loader::function_table!(@take $need $field),)*
                })
            }
        }

        impl $table {
            /// The rows, as `(field, symbol, required)`.
            #[cfg(test)]
            #[allow(dead_code, reason = "only the tests of the main tables read their rows")]
            pub(crate) const ROWS: &[(&str, &str, bool)] = &[
                $((
                    stringify!($field),
                    stringify!($symbol),
                    $crate::libraries::loader::function_table!(@required $need),
                ),)*
            ];
        }
    };
    (@field required $type:ty) => { $type };
    (@field optional $type:ty) => { Option<$type> };
    (@lookup required $resolver:ident, $missing:expr, $symbol:ident) => {
        $crate::libraries::loader::resolve_required($resolver, $missing, stringify!($symbol))
    };
    (@lookup optional $resolver:ident, $missing:expr, $symbol:ident) => {
        $crate::libraries::loader::resolve_optional($resolver, stringify!($symbol))
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
