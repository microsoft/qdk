// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A declarative macro for generating semantic AST leaf node `#[pyclass]`es.
//!
//! Every semantic node is an owned, frozen value that holds already-built
//! children (`Py<PyAny>`) plus a few scalar fields. Rather than hand-writing the
//! `#[pyclass]`, getters, `children()` traversal, `__repr__`, initializer chain,
//! and `Send + Sync` assertions for each of the ~46 variants, the [`qasm_node!`]
//! macro generates all of that from a compact per-variant description.
//!
//! # Field kinds
//!
//! Each field is declared with one of four kinds (note the required trailing
//! comma after every field). A field may carry a `///` doc comment, which
//! becomes the docstring of its generated Python getter:
//!
//! * `name: val <Type>,` — a scalar value (for example `String`, `u32`,
//!   `Option<String>`). Exposed via `#[pyo3(get)]`; never part of `children()`.
//! * `name: node,` — a single child node (`Py<PyAny>`). Included in
//!   `children()`.
//! * `name: opt,` — an optional child node (`Option<Py<PyAny>>`). Included in
//!   `children()` when present.
//! * `name: list,` — a list of child nodes (`Vec<Py<PyAny>>`). Flattened into
//!   `children()`.
//!
//! Statement nodes additionally report their inherited `annotations` from
//! `children()`, ahead of their own fields, so that a generic traversal reaches
//! every node that participates in structural equality.
//!
//! # Categories
//!
//! The leading `@expr` / `@stmt` / `@sexpr` / `@sstmt` token selects the
//! inheritance chain:
//!
//! * `@expr Name = "PyName" { .. }` extends [`super::semantic::SemExpr`] (so it
//!   inherits `ty`, `const_value`, and `symbol`). Its `init` takes those three
//!   values ahead of the node's own fields. The required `= "PyName"` gives the
//!   class its clean, un-prefixed Python name in the `qdk._native._semantic`
//!   submodule (the Rust identifier keeps its `Sem` prefix).
//! * `@stmt Name = "PyName" { .. }` extends [`super::semantic::SemStmt`] (so it
//!   inherits `annotations`). Its `init` takes `annotations` ahead of the
//!   node's own fields, and `= "PyName"` names it the same way as `@expr`.
//! * `@sexpr Name { .. }` is the *syntactic* counterpart: it extends
//!   [`super::nodes::Expression`] directly (the parser tree carries no resolved
//!   type or symbol information), so its `init` takes only `span` and the node's
//!   own fields.
//! * `@sstmt Name { .. }` extends [`super::nodes::Statement`] directly; its
//!   `init` takes only `span` and the node's own fields.
//! * `@aux Name = "PyName" { .. }` and `@saux Name { .. }` create semantic
//!   and syntactic auxiliary nodes rooted directly at [`super::nodes::QASMNode`].
//! * `@stype Name { .. }` creates a syntactic type node extending
//!   [`super::nodes::ClassicalType`], so callers can dispatch with
//!   `isinstance(node, ClassicalType)`.
//!
//! The expression and statement chains reference `SemExpr` / `SemType` /
//! `SemSymbol` / `sem_expr_base` / `sem_stmt_base`, which must be in scope at
//! the invocation site; the syntactic chains reference `Expression` /
//! `Statement` / `syntax_expr_base` / `syntax_stmt_base`. All chains ultimately
//! root at `QASMNode`, matching the reference `openqasm3` node hierarchy so
//! callers can dispatch with `isinstance`.

/// Renders one field of a generated node for its `__repr__`.
///
/// A scalar, a single child, and an optional child all render through the
/// node's own getter, so a `repr` cannot disagree with the accessor beside it.
/// A list renders only its length, so a `repr` never walks a whole tree.
macro_rules! qasm_repr_field {
    (list, $node:expr, $f:ident) => {
        format!(
            "{}={}",
            $crate::openqasm::repr::py_label(stringify!($f)),
            $crate::openqasm::repr::py_attr_len($node, stringify!($f))
        )
    };
    ($kind:ident, $node:expr, $f:ident) => {
        format!(
            "{}={}",
            $crate::openqasm::repr::py_label(stringify!($f)),
            $crate::openqasm::repr::py_attr($node, stringify!($f))
        )
    };
}

/// Expands to the participating attribute list for structural equality.
///
/// The base's attributes come first, then the class's own in declaration order.
/// Source positions never appear: `span` is inherited and omitted here, and no
/// generated class declares a `Span`-typed field. This is an expression macro
/// because `#[pymethods]` does not accept a macro in item position.
macro_rules! qasm_eq_fields {
    ({ $($base:literal),* }, { $(($rk:ident, $rn:ident),)* }) => {
        &[
            $($base,)*
            $( $crate::openqasm::repr::attr_name(stringify!($rn)), )*
        ]
    };
}

/// Generates a semantic AST leaf `#[pyclass]` and its accessors.
///
/// See the [module documentation](self) for the field-kind and category
/// grammar.
macro_rules! qasm_node {
    // ---- category entry points ----
    (@expr $name:ident = $pyname:literal { $($fields:tt)* }) => {
        qasm_node!(@munch expr, $name,
            meta { name = $pyname, module = "qdk.openqasm.semantic" }, disp { $pyname },
            sf {}, param {}, ctor {}, nodes {}, opts {}, lists {}, rf {};
            $($fields)*);
    };
    (@stmt $name:ident = $pyname:literal { $($fields:tt)* }) => {
        qasm_node!(@munch stmt, $name,
            meta { name = $pyname, module = "qdk.openqasm.semantic" }, disp { $pyname },
            sf {}, param {}, ctor {}, nodes {}, opts {}, lists {}, rf {};
            $($fields)*);
    };
    (@sexpr $name:ident { $($fields:tt)* }) => {
        qasm_node!(@munch sexpr, $name,
            meta { module = "qdk.openqasm.parser" }, disp { stringify!($name) },
            sf {}, param {}, ctor {}, nodes {}, opts {}, lists {}, rf {};
            $($fields)*);
    };
    (@sstmt $name:ident { $($fields:tt)* }) => {
        qasm_node!(@munch sstmt, $name,
            meta { module = "qdk.openqasm.parser" }, disp { stringify!($name) },
            sf {}, param {}, ctor {}, nodes {}, opts {}, lists {}, rf {};
            $($fields)*);
    };
    (@aux $name:ident = $pyname:literal { $($fields:tt)* }) => {
        qasm_node!(@munch aux, $name,
            meta { name = $pyname, module = "qdk.openqasm.semantic" }, disp { $pyname },
            sf {}, param {}, ctor {}, nodes {}, opts {}, lists {}, rf {};
            $($fields)*);
    };
    (@aux $name:ident = $pyname:literal, doc = $doc:literal { $($fields:tt)* }) => {
        qasm_node!(@munch auxdoc, $name,
            meta { name = $pyname, module = "qdk.openqasm.semantic" }, disp { $pyname ; $doc },
            sf {}, param {}, ctor {}, nodes {}, opts {}, lists {}, rf {};
            $($fields)*);
    };
    (@saux $name:ident { $($fields:tt)* }) => {
        qasm_node!(@munch saux, $name,
            meta { module = "qdk.openqasm.parser" }, disp { stringify!($name) },
            sf {}, param {}, ctor {}, nodes {}, opts {}, lists {}, rf {};
            $($fields)*);
    };
    (@saux $name:ident, doc = $doc:literal { $($fields:tt)* }) => {
        qasm_node!(@munch sauxdoc, $name,
            meta { module = "qdk.openqasm.parser" }, disp { stringify!($name) ; $doc },
            sf {}, param {}, ctor {}, nodes {}, opts {}, lists {}, rf {};
            $($fields)*);
    };
    (@stype $name:ident { $($fields:tt)* }) => {
        qasm_node!(@munch stype, $name,
            meta { module = "qdk.openqasm.parser" }, disp { stringify!($name) },
            sf {}, param {}, ctor {}, nodes {}, opts {}, lists {}, rf {};
            $($fields)*);
    };
    (@semtype $name:ident = $pyname:literal { $($fields:tt)* }) => {
        qasm_node!(@munch semtype, $name,
            meta { name = $pyname, module = "qdk.openqasm.semantic" }, disp { $pyname },
            sf {}, param {}, ctor {}, nodes {}, opts {}, lists {}, rf {};
            $($fields)*);
    };

    // ---- munch: scalar value field ----
    (@munch $cat:ident, $name:ident,
        meta { $($meta:tt)* }, disp { $($disp:tt)* },
        sf { $($sf:tt)* }, param { $($param:tt)* }, ctor { $($ctor:tt)* },
        nodes { $($n:tt)* }, opts { $($o:tt)* }, lists { $($l:tt)* }, rf { $($rf:tt)* };
        $(#[$fmeta:meta])* $f:ident : val $ty:ty , $($rest:tt)*
    ) => {
        qasm_node!(@munch $cat, $name,
            meta { $($meta)* }, disp { $($disp)* },
            sf { $($sf)* $(#[$fmeta])* #[pyo3(get)] $f: $ty, },
            param { $($param)* $f: $ty, },
            ctor { $($ctor)* $f, },
            nodes { $($n)* }, opts { $($o)* }, lists { $($l)* },
            rf { $($rf)* (val, $f), };
            $($rest)*);
    };

    // ---- munch: single child ----
    (@munch $cat:ident, $name:ident,
        meta { $($meta:tt)* }, disp { $($disp:tt)* },
        sf { $($sf:tt)* }, param { $($param:tt)* }, ctor { $($ctor:tt)* },
        nodes { $($n:tt)* }, opts { $($o:tt)* }, lists { $($l:tt)* }, rf { $($rf:tt)* };
        $(#[$fmeta:meta])* $f:ident : node , $($rest:tt)*
    ) => {
        qasm_node!(@munch $cat, $name,
            meta { $($meta)* }, disp { $($disp)* },
            sf { $($sf)* $(#[$fmeta])* #[pyo3(get)] $f: Py<PyAny>, },
            param { $($param)* $f: Py<PyAny>, },
            ctor { $($ctor)* $f, },
            nodes { $($n)* $f, }, opts { $($o)* }, lists { $($l)* },
            rf { $($rf)* (node, $f), };
            $($rest)*);
    };

    // ---- munch: optional child ----
    (@munch $cat:ident, $name:ident,
        meta { $($meta:tt)* }, disp { $($disp:tt)* },
        sf { $($sf:tt)* }, param { $($param:tt)* }, ctor { $($ctor:tt)* },
        nodes { $($n:tt)* }, opts { $($o:tt)* }, lists { $($l:tt)* }, rf { $($rf:tt)* };
        $(#[$fmeta:meta])* $f:ident : opt , $($rest:tt)*
    ) => {
        qasm_node!(@munch $cat, $name,
            meta { $($meta)* }, disp { $($disp)* },
            sf { $($sf)* $(#[$fmeta])* #[pyo3(get)] $f: Option<Py<PyAny>>, },
            param { $($param)* $f: Option<Py<PyAny>>, },
            ctor { $($ctor)* $f, },
            nodes { $($n)* }, opts { $($o)* $f, }, lists { $($l)* },
            rf { $($rf)* (opt, $f), };
            $($rest)*);
    };

    // ---- munch: child list ----
    (@munch $cat:ident, $name:ident,
        meta { $($meta:tt)* }, disp { $($disp:tt)* },
        sf { $($sf:tt)* }, param { $($param:tt)* }, ctor { $($ctor:tt)* },
        nodes { $($n:tt)* }, opts { $($o:tt)* }, lists { $($l:tt)* }, rf { $($rf:tt)* };
        $(#[$fmeta:meta])* $f:ident : list , $($rest:tt)*
    ) => {
        qasm_node!(@munch $cat, $name,
            meta { $($meta)* }, disp { $($disp)* },
            sf { $($sf)* $(#[$fmeta])* #[pyo3(get)] $f: Vec<Py<PyAny>>, },
            param { $($param)* $f: Vec<Py<PyAny>>, },
            ctor { $($ctor)* $f, },
            nodes { $($n)* }, opts { $($o)* }, lists { $($l)* $f, },
            rf { $($rf)* (list, $f), };
            $($rest)*);
    };

    // ---- terminal: all fields consumed ----
    (@munch $cat:ident, $name:ident,
        meta { $($meta:tt)* }, disp { $($disp:tt)* },
        sf { $($sf:tt)* }, param { $($param:tt)* }, ctor { $($ctor:tt)* },
        nodes { $($n:tt)* }, opts { $($o:tt)* }, lists { $($l:tt)* }, rf { $($rf:tt)* };
    ) => {
        qasm_node!(@emit $cat, $name,
            meta { $($meta)* }, disp { $($disp)* },
            { $($sf)* }, { $($param)* }, { $($ctor)* },
            { $($n)* }, { $($o)* }, { $($l)* }, { $($rf)* });
    };

    // ---- emit: expression node ----
    (@emit expr, $name:ident,
        meta { $($meta:tt)* }, disp { $($disp:tt)* },
        { $($sf:tt)* }, { $($param:tt)* }, { $($ctor:tt)* },
        { $($n:ident,)* }, { $($o:ident,)* }, { $($l:ident,)* }, { $(($rk:ident, $rn:ident),)* }
    ) => {
        #[doc = concat!("A read-only OpenQASM `", $($disp)*, "` node.")]
        #[pyclass(extends = SemExpr, frozen, $($meta)*)]
        pub(crate) struct $name {
            $($sf)*
        }

        #[pymethods]
        impl $name {
            /// The node's child nodes.
            #[allow(unused_mut, clippy::vec_init_then_push)]
            fn children(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
                let mut out: Vec<Py<PyAny>> = Vec::new();
                $( out.push(self.$n.clone_ref(py)); )*
                $( if let Some(child) = &self.$o { out.push(child.clone_ref(py)); } )*
                $( for child in &self.$l { out.push(child.clone_ref(py)); } )*
                let _ = py;
                out
            }

            #[allow(unused_mut, unused_variables)]
            fn __repr__(slf: &Bound<'_, Self>) -> String {
                let node = slf.as_any();
                let mut fields: Vec<String> = Vec::new();
                $( fields.push(qasm_repr_field!($rk, node, $rn)); )*
                format!("{}({})", $($disp)*, fields.join(", "))
            }

            fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
                $crate::openqasm::eq::structural_eq(slf.as_any(), other, qasm_eq_fields!({ "ty", "const_value", "symbol" }, { $(($rk, $rn),)* }))
            }

            fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
                $crate::openqasm::eq::structural_hash(slf.as_any(), qasm_eq_fields!({ "ty", "const_value", "symbol" }, { $(($rk, $rn),)* }))
            }
        }

        impl $name {
            pub(crate) fn init(
                span: Span,
                ty: Py<PyAny>,
                const_value: Option<Py<PyAny>>,
                symbol: Option<Py<SemSymbol>>,
                $($param)*
            ) -> PyClassInitializer<Self> {
                sem_expr_base(span, ty, const_value, symbol).add_subclass($name { $($ctor)* })
            }
        }

        const _: fn() = || {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<$name>();
        };
    };

    // ---- emit: statement node ----
    (@emit stmt, $name:ident,
        meta { $($meta:tt)* }, disp { $($disp:tt)* },
        { $($sf:tt)* }, { $($param:tt)* }, { $($ctor:tt)* },
        { $($n:ident,)* }, { $($o:ident,)* }, { $($l:ident,)* }, { $(($rk:ident, $rn:ident),)* }
    ) => {
        #[doc = concat!("A read-only OpenQASM `", $($disp)*, "` node.")]
        #[pyclass(extends = SemStmt, frozen, $($meta)*)]
        pub(crate) struct $name {
            $($sf)*
        }

        #[pymethods]
        impl $name {
            /// The node's child nodes, annotations first, then the node's own children.
            #[allow(unused_mut, clippy::vec_init_then_push, clippy::needless_pass_by_value)]
            fn children(slf: PyRef<'_, Self>) -> Vec<Py<PyAny>> {
                let py = slf.py();
                let mut out: Vec<Py<PyAny>> = Vec::new();
                for annotation in &slf.as_super().as_super().annotations {
                    out.push(annotation.clone_ref(py).into_any());
                }
                $( out.push(slf.$n.clone_ref(py)); )*
                $( if let Some(child) = &slf.$o { out.push(child.clone_ref(py)); } )*
                $( for child in &slf.$l { out.push(child.clone_ref(py)); } )*
                out
            }

            #[allow(unused_mut, unused_variables)]
            fn __repr__(slf: &Bound<'_, Self>) -> String {
                let node = slf.as_any();
                let mut fields: Vec<String> = Vec::new();
                $( fields.push(qasm_repr_field!($rk, node, $rn)); )*
                format!("{}({})", $($disp)*, fields.join(", "))
            }

            fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
                $crate::openqasm::eq::structural_eq(slf.as_any(), other, qasm_eq_fields!({ "annotations" }, { $(($rk, $rn),)* }))
            }

            fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
                $crate::openqasm::eq::structural_hash(slf.as_any(), qasm_eq_fields!({ "annotations" }, { $(($rk, $rn),)* }))
            }
        }

        impl $name {
            pub(crate) fn init(
                span: Span,
                annotations: Vec<Py<Annotation>>,
                $($param)*
            ) -> PyClassInitializer<Self> {
                sem_stmt_base(span, annotations).add_subclass($name { $($ctor)* })
            }
        }

        const _: fn() = || {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<$name>();
        };
    };

    // ---- emit: syntactic expression node ----
    (@emit sexpr, $name:ident,
        meta { $($meta:tt)* }, disp { $($disp:tt)* },
        { $($sf:tt)* }, { $($param:tt)* }, { $($ctor:tt)* },
        { $($n:ident,)* }, { $($o:ident,)* }, { $($l:ident,)* }, { $(($rk:ident, $rn:ident),)* }
    ) => {
        #[doc = concat!("A read-only OpenQASM `", $($disp)*, "` node.")]
        #[pyclass(extends = Expression, frozen, $($meta)*)]
        pub(crate) struct $name {
            $($sf)*
        }

        #[pymethods]
        impl $name {
            /// The node's child nodes.
            #[allow(unused_mut, clippy::vec_init_then_push)]
            fn children(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
                let mut out: Vec<Py<PyAny>> = Vec::new();
                $( out.push(self.$n.clone_ref(py)); )*
                $( if let Some(child) = &self.$o { out.push(child.clone_ref(py)); } )*
                $( for child in &self.$l { out.push(child.clone_ref(py)); } )*
                let _ = py;
                out
            }

            #[allow(unused_mut, unused_variables)]
            fn __repr__(slf: &Bound<'_, Self>) -> String {
                let node = slf.as_any();
                let mut fields: Vec<String> = Vec::new();
                $( fields.push(qasm_repr_field!($rk, node, $rn)); )*
                format!("{}({})", $($disp)*, fields.join(", "))
            }

            fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
                $crate::openqasm::eq::structural_eq(slf.as_any(), other, qasm_eq_fields!({}, { $(($rk, $rn),)* }))
            }

            fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
                $crate::openqasm::eq::structural_hash(slf.as_any(), qasm_eq_fields!({}, { $(($rk, $rn),)* }))
            }
        }

        impl $name {
            pub(crate) fn init(
                span: Span,
                $($param)*
            ) -> PyClassInitializer<Self> {
                syntax_expr_base(span).add_subclass($name { $($ctor)* })
            }
        }

        const _: fn() = || {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<$name>();
        };
    };

    // ---- emit: syntactic statement node ----
    (@emit sstmt, $name:ident,
        meta { $($meta:tt)* }, disp { $($disp:tt)* },
        { $($sf:tt)* }, { $($param:tt)* }, { $($ctor:tt)* },
        { $($n:ident,)* }, { $($o:ident,)* }, { $($l:ident,)* }, { $(($rk:ident, $rn:ident),)* }
    ) => {
        #[doc = concat!("A read-only OpenQASM `", $($disp)*, "` node.")]
        #[pyclass(extends = Statement, frozen, $($meta)*)]
        pub(crate) struct $name {
            $($sf)*
        }

        #[pymethods]
        impl $name {
            /// The node's child nodes, annotations first, then the node's own children.
            #[allow(unused_mut, clippy::vec_init_then_push, clippy::needless_pass_by_value)]
            fn children(slf: PyRef<'_, Self>) -> Vec<Py<PyAny>> {
                let py = slf.py();
                let mut out: Vec<Py<PyAny>> = Vec::new();
                for annotation in &slf.as_super().annotations {
                    out.push(annotation.clone_ref(py).into_any());
                }
                $( out.push(slf.$n.clone_ref(py)); )*
                $( if let Some(child) = &slf.$o { out.push(child.clone_ref(py)); } )*
                $( for child in &slf.$l { out.push(child.clone_ref(py)); } )*
                out
            }

            #[allow(unused_mut, unused_variables)]
            fn __repr__(slf: &Bound<'_, Self>) -> String {
                let node = slf.as_any();
                let mut fields: Vec<String> = Vec::new();
                $( fields.push(qasm_repr_field!($rk, node, $rn)); )*
                format!("{}({})", $($disp)*, fields.join(", "))
            }

            fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
                $crate::openqasm::eq::structural_eq(slf.as_any(), other, qasm_eq_fields!({ "annotations" }, { $(($rk, $rn),)* }))
            }

            fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
                $crate::openqasm::eq::structural_hash(slf.as_any(), qasm_eq_fields!({ "annotations" }, { $(($rk, $rn),)* }))
            }
        }

        impl $name {
            pub(crate) fn init(
                span: Span,
                annotations: Vec<Py<Annotation>>,
                $($param)*
            ) -> PyClassInitializer<Self> {
                syntax_stmt_base(span, annotations).add_subclass($name { $($ctor)* })
            }
        }

        const _: fn() = || {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<$name>();
        };
    };

    // ---- emit: semantic auxiliary node ----
    (@emit aux, $name:ident,
        meta { $($meta:tt)* }, disp { $($disp:tt)* },
        { $($sf:tt)* }, { $($param:tt)* }, { $($ctor:tt)* },
        { $($n:ident,)* }, { $($o:ident,)* }, { $($l:ident,)* }, { $(($rk:ident, $rn:ident),)* }
    ) => {
        #[doc = concat!("A read-only OpenQASM `", $($disp)*, "` node.")]
        #[pyclass(extends = QASMNode, frozen, $($meta)*)]
        pub(crate) struct $name {
            $($sf)*
        }

        #[pymethods]
        impl $name {
            /// The node's child nodes.
            #[allow(unused_mut, clippy::vec_init_then_push)]
            fn children(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
                let mut out: Vec<Py<PyAny>> = Vec::new();
                $( out.push(self.$n.clone_ref(py)); )*
                $( if let Some(child) = &self.$o { out.push(child.clone_ref(py)); } )*
                $( for child in &self.$l { out.push(child.clone_ref(py)); } )*
                let _ = py;
                out
            }

            #[allow(unused_mut, unused_variables)]
            fn __repr__(slf: &Bound<'_, Self>) -> String {
                let node = slf.as_any();
                let mut fields: Vec<String> = Vec::new();
                $( fields.push(qasm_repr_field!($rk, node, $rn)); )*
                format!("{}({})", $($disp)*, fields.join(", "))
            }

            fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
                $crate::openqasm::eq::structural_eq(slf.as_any(), other, qasm_eq_fields!({}, { $(($rk, $rn),)* }))
            }

            fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
                $crate::openqasm::eq::structural_hash(slf.as_any(), qasm_eq_fields!({}, { $(($rk, $rn),)* }))
            }
        }

        impl $name {
            pub(crate) fn init(span: Span, $($param)*) -> PyClassInitializer<Self> {
                PyClassInitializer::from(QASMNode { span }).add_subclass($name { $($ctor)* })
            }
        }

        const _: fn() = || {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<$name>();
        };
    };

    // ---- emit: documented semantic auxiliary node ----
    (@emit auxdoc, $name:ident,
        meta { $($meta:tt)* }, disp { $pyname:literal ; $doc:literal },
        { $($sf:tt)* }, { $($param:tt)* }, { $($ctor:tt)* },
        { $($n:ident,)* }, { $($o:ident,)* }, { $($l:ident,)* }, { $(($rk:ident, $rn:ident),)* }
    ) => {
        #[doc = $doc]
        #[pyclass(extends = QASMNode, frozen, $($meta)*)]
        pub(crate) struct $name {
            $($sf)*
        }

        #[pymethods]
        impl $name {
            /// The node's child nodes.
            #[allow(unused_mut, clippy::vec_init_then_push)]
            fn children(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
                let mut out: Vec<Py<PyAny>> = Vec::new();
                $( out.push(self.$n.clone_ref(py)); )*
                $( if let Some(child) = &self.$o { out.push(child.clone_ref(py)); } )*
                $( for child in &self.$l { out.push(child.clone_ref(py)); } )*
                let _ = py;
                out
            }

            #[allow(unused_mut, unused_variables)]
            fn __repr__(slf: &Bound<'_, Self>) -> String {
                let node = slf.as_any();
                let mut fields: Vec<String> = Vec::new();
                $( fields.push(qasm_repr_field!($rk, node, $rn)); )*
                format!("{}({})", $pyname, fields.join(", "))
            }

            fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
                $crate::openqasm::eq::structural_eq(slf.as_any(), other, qasm_eq_fields!({}, { $(($rk, $rn),)* }))
            }

            fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
                $crate::openqasm::eq::structural_hash(slf.as_any(), qasm_eq_fields!({}, { $(($rk, $rn),)* }))
            }
        }

        impl $name {
            pub(crate) fn init(span: Span, $($param)*) -> PyClassInitializer<Self> {
                PyClassInitializer::from(QASMNode { span }).add_subclass($name { $($ctor)* })
            }
        }

        const _: fn() = || {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<$name>();
        };
    };

    // ---- emit: syntactic auxiliary node ----
    (@emit saux, $name:ident,
        meta { $($meta:tt)* }, disp { $($disp:tt)* },
        { $($sf:tt)* }, { $($param:tt)* }, { $($ctor:tt)* },
        { $($n:ident,)* }, { $($o:ident,)* }, { $($l:ident,)* }, { $(($rk:ident, $rn:ident),)* }
    ) => {
        #[doc = concat!("A read-only OpenQASM `", $($disp)*, "` node.")]
        #[pyclass(extends = QASMNode, frozen, $($meta)*)]
        pub(crate) struct $name {
            $($sf)*
        }

        #[pymethods]
        impl $name {
            /// The node's child nodes.
            #[allow(unused_mut, clippy::vec_init_then_push)]
            fn children(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
                let mut out: Vec<Py<PyAny>> = Vec::new();
                $( out.push(self.$n.clone_ref(py)); )*
                $( if let Some(child) = &self.$o { out.push(child.clone_ref(py)); } )*
                $( for child in &self.$l { out.push(child.clone_ref(py)); } )*
                let _ = py;
                out
            }

            #[allow(unused_mut, unused_variables)]
            fn __repr__(slf: &Bound<'_, Self>) -> String {
                let node = slf.as_any();
                let mut fields: Vec<String> = Vec::new();
                $( fields.push(qasm_repr_field!($rk, node, $rn)); )*
                format!("{}({})", $($disp)*, fields.join(", "))
            }

            fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
                $crate::openqasm::eq::structural_eq(slf.as_any(), other, qasm_eq_fields!({}, { $(($rk, $rn),)* }))
            }

            fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
                $crate::openqasm::eq::structural_hash(slf.as_any(), qasm_eq_fields!({}, { $(($rk, $rn),)* }))
            }
        }

        impl $name {
            pub(crate) fn init(span: Span, $($param)*) -> PyClassInitializer<Self> {
                PyClassInitializer::from(QASMNode { span }).add_subclass($name { $($ctor)* })
            }
        }

        const _: fn() = || {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<$name>();
        };
    };

    // ---- emit: documented syntactic auxiliary node ----
    (@emit sauxdoc, $name:ident,
        meta { $($meta:tt)* }, disp { $pyname:expr ; $doc:literal },
        { $($sf:tt)* }, { $($param:tt)* }, { $($ctor:tt)* },
        { $($n:ident,)* }, { $($o:ident,)* }, { $($l:ident,)* }, { $(($rk:ident, $rn:ident),)* }
    ) => {
        #[doc = $doc]
        #[pyclass(extends = QASMNode, frozen, $($meta)*)]
        pub(crate) struct $name {
            $($sf)*
        }

        #[pymethods]
        impl $name {
            /// The node's child nodes.
            #[allow(unused_mut, clippy::vec_init_then_push)]
            fn children(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
                let mut out: Vec<Py<PyAny>> = Vec::new();
                $( out.push(self.$n.clone_ref(py)); )*
                $( if let Some(child) = &self.$o { out.push(child.clone_ref(py)); } )*
                $( for child in &self.$l { out.push(child.clone_ref(py)); } )*
                let _ = py;
                out
            }

            #[allow(unused_mut, unused_variables)]
            fn __repr__(slf: &Bound<'_, Self>) -> String {
                let node = slf.as_any();
                let mut fields: Vec<String> = Vec::new();
                $( fields.push(qasm_repr_field!($rk, node, $rn)); )*
                format!("{}({})", $pyname, fields.join(", "))
            }

            fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
                $crate::openqasm::eq::structural_eq(slf.as_any(), other, qasm_eq_fields!({}, { $(($rk, $rn),)* }))
            }

            fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
                $crate::openqasm::eq::structural_hash(slf.as_any(), qasm_eq_fields!({}, { $(($rk, $rn),)* }))
            }
        }

        impl $name {
            pub(crate) fn init(span: Span, $($param)*) -> PyClassInitializer<Self> {
                PyClassInitializer::from(QASMNode { span }).add_subclass($name { $($ctor)* })
            }
        }

        const _: fn() = || {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<$name>();
        };
    };

    // ---- emit: syntactic type node ----
    (@emit stype, $name:ident,
        meta { $($meta:tt)* }, disp { $($disp:tt)* },
        { $($sf:tt)* }, { $($param:tt)* }, { $($ctor:tt)* },
        { $($n:ident,)* }, { $($o:ident,)* }, { $($l:ident,)* }, { $(($rk:ident, $rn:ident),)* }
    ) => {
        #[doc = concat!("A read-only OpenQASM `", $($disp)*, "` type node.")]
        #[pyclass(extends = ClassicalType, frozen, $($meta)*)]
        pub(crate) struct $name {
            $($sf)*
        }

        #[pymethods]
        impl $name {
            /// The node's child nodes.
            #[allow(unused_mut, clippy::vec_init_then_push)]
            fn children(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
                let mut out: Vec<Py<PyAny>> = Vec::new();
                $( out.push(self.$n.clone_ref(py)); )*
                $( if let Some(child) = &self.$o { out.push(child.clone_ref(py)); } )*
                $( for child in &self.$l { out.push(child.clone_ref(py)); } )*
                let _ = py;
                out
            }

            #[allow(unused_mut, unused_variables)]
            fn __repr__(slf: &Bound<'_, Self>) -> String {
                let node = slf.as_any();
                let mut fields: Vec<String> = Vec::new();
                $( fields.push(qasm_repr_field!($rk, node, $rn)); )*
                format!("{}({})", $($disp)*, fields.join(", "))
            }

            fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
                $crate::openqasm::eq::structural_eq(slf.as_any(), other, qasm_eq_fields!({}, { $(($rk, $rn),)* }))
            }

            fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
                $crate::openqasm::eq::structural_hash(slf.as_any(), qasm_eq_fields!({}, { $(($rk, $rn),)* }))
            }
        }

        impl $name {
            pub(crate) fn init(span: Span, $($param)*) -> PyClassInitializer<Self> {
                syntax_type_base(span).add_subclass($name { $($ctor)* })
            }
        }

        const _: fn() = || {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<$name>();
        };
    };

    // ---- emit: semantic type node ----
    //
    // A resolved type has no source position, so this category takes no span and
    // roots at `SemType` rather than `QASMNode`.
    (@emit semtype, $name:ident,
        meta { $($meta:tt)* }, disp { $($disp:tt)* },
        { $($sf:tt)* }, { $($param:tt)* }, { $($ctor:tt)* },
        { $($n:ident,)* }, { $($o:ident,)* }, { $($l:ident,)* }, { $(($rk:ident, $rn:ident),)* }
    ) => {
        #[doc = concat!("A read-only OpenQASM `", $($disp)*, "` resolved type.")]
        #[pyclass(extends = SemType, frozen, $($meta)*)]
        pub(crate) struct $name {
            $($sf)*
        }

        #[pymethods]
        impl $name {
            /// The type's child types.
            #[allow(unused_mut, clippy::vec_init_then_push)]
            fn children(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
                let mut out: Vec<Py<PyAny>> = Vec::new();
                $( out.push(self.$n.clone_ref(py)); )*
                $( if let Some(child) = &self.$o { out.push(child.clone_ref(py)); } )*
                $( for child in &self.$l { out.push(child.clone_ref(py)); } )*
                let _ = py;
                out
            }

            #[allow(unused_mut, unused_variables)]
            fn __repr__(slf: &Bound<'_, Self>) -> String {
                let node = slf.as_any();
                let mut fields: Vec<String> = Vec::new();
                $( fields.push(qasm_repr_field!($rk, node, $rn)); )*
                format!("{}({})", $($disp)*, fields.join(", "))
            }

            fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
                $crate::openqasm::eq::structural_eq(slf.as_any(), other, qasm_eq_fields!({ "name", "is_const" }, { $(($rk, $rn),)* }))
            }

            fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
                $crate::openqasm::eq::structural_hash(slf.as_any(), qasm_eq_fields!({ "name", "is_const" }, { $(($rk, $rn),)* }))
            }
        }

        impl $name {
            pub(crate) fn init(name: String, is_const: bool, $($param)*)
                -> PyClassInitializer<Self>
            {
                sem_type_base(name, is_const).add_subclass($name { $($ctor)* })
            }
        }

        const _: fn() = || {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<$name>();
        };
    };
}
