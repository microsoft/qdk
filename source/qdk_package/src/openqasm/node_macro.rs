// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A declarative macro for generating semantic AST leaf node `#[pyclass]`es.
//!
//! Every semantic node is an owned, frozen value that holds already-built
//! children (`Py<PyAny>`) plus a few scalar fields. Rather than hand-writing the
//! `#[pyclass]`, getters, `children()` traversal, `__repr__`, initializer chain,
//! and `Send + Sync` assertions for every syntactic and semantic variant, the
//! [`qasm_node!`] macro generates all of that from a compact per-variant
//! description.
//!
//! # Field kinds
//!
//! Each field is declared with one of four kinds (note the required trailing
//! comma after every field). A field may carry a `///` doc comment, which
//! becomes the docstring of its generated Python getter:
//!
//! * `name: val <Type>,` is a scalar value (for example `String`, `u32`,
//!   `Option<String>`). Exposed via `#[pyo3(get)]`; never part of `children()`.
//! * `name: span,` and `name: optspan,` are secondary source positions, such as
//!   the span of a keyword or of one part of a compound construct, carrying
//!   `Span` and `Option<Span>` respectively. Both are exposed via `#[pyo3(get)]`
//!   but excluded from `children()`, `__eq__`, `__hash__`, and `__repr__`, so
//!   the same construct at two source offsets still compares and renders the
//!   same. Use `name: val Span,` instead when a position should participate,
//!   which no node currently wants.
//! * `name: node,` is a single child node (`Py<PyAny>`). Included in
//!   `children()`.
//! * `name: opt,` is an optional child node (`Option<Py<PyAny>>`). Included in
//!   `children()` when present.
//! * `name: list,` is a list of child nodes (`Vec<Py<PyAny>>`). Flattened into
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
/// Source positions never appear: `span` is inherited and omitted here, and a
/// `span`-kind field is deliberately kept out of the accumulator this reads.
/// This is an expression macro because `#[pymethods]` does not accept a macro
/// in item position.
macro_rules! qasm_eq_fields {
    ({ $($base:literal),* }, { $(($rk:ident, $rn:ident),)* }) => {
        &[
            $($base,)*
            $( $crate::openqasm::repr::attr_name(stringify!($rn)), )*
        ]
    };
}

/// Expands to the public Python class documentation for a generated node.
///
/// Keep these strings byte-for-byte identical to the corresponding class
/// docstrings in `_native_syntax.pyi` and `_native_semantic.pyi`.
macro_rules! qasm_node_doc {
    (Identifier) => { "An identifier expression (a reference to a name)." };
    (IndexedIdentifier) => { "An indexed identifier (for example ``a[i]``) in an l-value position." };
    (HardwareQubit) => { "A hardware-qubit gate operand (for example ``$0``)." };
    (ErrorExpression) => {
        "A placeholder inserted when the parser recovers from an invalid expression.\n\nInspect the parse result's diagnostics for the error. The placeholder keeps\nthe recovered tree traversable and identifies the affected source span."
    };
    (UnaryExpression) => { "A unary operator expression." };
    (BinaryExpression) => { "A binary operator expression." };
    (IntegerLiteral) => { "An integer literal of arbitrary precision." };
    (FloatLiteral) => { "A floating-point literal." };
    (ImaginaryLiteral) => { "An imaginary literal, carrying its magnitude." };
    (BooleanLiteral) => { "A boolean literal." };
    (BitstringLiteral) => { "A bitstring literal, carrying its value and declared width." };
    (DurationLiteral) => { "A duration literal, carrying its magnitude and unit." };
    (ArrayLiteral) => { "An array literal, exposing its elements as children." };
    (StringLiteral) => { "A string literal." };
    (FunctionCall) => { "A function-call expression." };
    (Cast) => { "A type-cast expression." };
    (IndexExpression) => { "An index expression (for example ``a[i]``)." };
    (ParenExpression) => { "A parenthesized expression." };
    (DurationOf) => { "A ``durationof`` expression over a block of statements." };
    (Concatenation) => { "A concatenation r-value (for example ``a ++ b``)." };
    (QuantumMeasurement) => { "A measurement r-value (for example ``measure q``)." };
    (IntType) => { "A signed integer type, optionally sized." };
    (UintType) => { "An unsigned integer type, optionally sized." };
    (FloatType) => { "A floating-point type, optionally sized." };
    (AngleType) => { "An angle type, optionally sized." };
    (BitType) => { "A bit or bit-register type, optionally sized." };
    (ComplexType) => { "A complex type, optionally carrying its component float type." };
    (BoolType) => { "A boolean type." };
    (DurationType) => { "A duration type." };
    (StretchType) => { "A stretch type." };
    (QubitType) => { "A qubit parameter type, optionally sized." };
    (ErrorType) => { "A type that could not be parsed." };
    (ArrayType) => { "A sized array type." };
    (StaticArrayReferenceType) => { "An array reference with statically known dimensions." };
    (DynArrayReferenceType) => { "An array reference with a dynamic number of dimensions." };
    (QubitDeclaration) => { "A qubit declaration statement (for example ``qubit q;``)." };
    (AliasStatement) => { "An alias declaration statement (``let``)." };
    (ClassicalAssignment) => { "A classical assignment statement (``a = b;``)." };
    (CompoundAssignment) => { "A compound assignment statement (for example ``a += b;``)." };
    (QuantumBarrier) => { "A ``barrier`` statement." };
    (Box) => { "A ``box`` statement." };
    (BreakStatement) => { "A ``break`` statement." };
    (CompoundStatement) => { "A block of statements (``{ ... }``)." };
    (CalibrationStatement) => { "A ``cal`` calibration block." };
    (CalibrationGrammarDeclaration) => { "A ``defcalgrammar`` declaration." };
    (ClassicalDeclaration) => { "A classical variable declaration." };
    (ConstantDeclaration) => { "A ``const`` declaration." };
    (ContinueStatement) => { "A ``continue`` statement." };
    (SubroutineDefinition) => { "A ``def`` subroutine definition." };
    (CalibrationDefinition) => { "A ``defcal`` calibration definition." };
    (DelayInstruction) => { "A ``delay`` instruction." };
    (EndStatement) => { "An ``end`` statement." };
    (ExpressionStatement) => { "A statement that evaluates an expression." };
    (ExternDeclaration) => { "An ``extern`` declaration." };
    (ForInLoop) => { "A ``for`` loop over an iterable set." };
    (BranchingStatement) => { "An ``if`` / ``else`` branching statement." };
    (QuantumGate) => { "A quantum gate call." };
    (QuantumPhase) => { "A ``gphase`` statement." };
    (Include) => { "An ``include`` directive." };
    (IODeclaration) => { "An ``input`` / ``output`` declaration." };
    (QuantumMeasurementStatement) => { "A measurement statement (for example ``c = measure q;``)." };
    (Pragma) => {
        "A ``pragma`` directive.\n\n:attr:`command` contains all text after the keyword. :attr:`name` and\n:attr:`value` split that text into its leading dotted identifier and the\nremaining content for convenient inspection."
    };
    (QuantumGateDefinition) => {
        "A ``gate`` definition.\n\n``params`` and ``qubits`` hold :class:`Identifier` nodes. A parameter the\nparser recovered from is an :class:`ErrorExpression` at the span it would\nhave occupied."
    };
    (QuantumReset) => { "A ``reset`` statement." };
    (ReturnStatement) => { "A ``return`` statement." };
    (WhileLoop) => { "A ``while`` loop." };
    (ErrorStatement) => {
        "A placeholder inserted when the parser recovers from an invalid statement.\n\nInspect the parse result's diagnostics for the error. The placeholder keeps\nthe recovered tree traversable and identifies the affected source span."
    };
    (SemIntType) => { "A resolved signed integer type." };
    (SemUintType) => { "A resolved unsigned integer type." };
    (SemFloatType) => { "A resolved floating-point type." };
    (SemAngleType) => { "A resolved angle type." };
    (SemComplexType) => { "A resolved complex type." };
    (SemBitType) => { "A resolved single-bit type." };
    (SemBoolType) => { "A resolved boolean type." };
    (SemDurationType) => { "A resolved duration type." };
    (SemStretchType) => { "A resolved stretch type." };
    (SemQubitType) => { "A resolved single-qubit type." };
    (SemHardwareQubitType) => { "A resolved hardware qubit type, as written ``$0``." };
    (SemBitArrayType) => { "A resolved bit register type." };
    (SemQubitArrayType) => { "A resolved qubit register type." };
    (SemArrayType) => { "A resolved array type." };
    (SemStaticArrayReferenceType) => { "A resolved array reference whose dimension lengths are all known." };
    (SemDynArrayReferenceType) => { "A resolved array reference declared with ``#dim``, so only its rank is known." };
    (SemGateType) => { "A resolved gate type." };
    (SemFunctionType) => { "A resolved subroutine signature." };
    (SemRangeType) => { "The resolved type of a range expression." };
    (SemSetType) => { "The resolved type of a discrete set expression." };
    (SemVoidType) => { "The resolved type of an expression that yields no value." };
    (SemErrorType) => { "The resolved type analysis assigns when it could not determine one." };
    (SemErrExpr) => {
        "A placeholder for an expression semantic analysis could not resolve.\n\nInspect the analysis result's diagnostics for the cause. The placeholder\npreserves the source span so tools can continue traversing the recovered\ntree."
    };
    (SemResolvedIdent) => { "A reference to a resolved symbol." };
    (SemCapturedResolvedIdent) => { "A reference to a symbol captured from an enclosing scope." };
    (SemUnaryOpExpr) => { "A unary operator expression." };
    (SemBinaryOpExpr) => {
        "A binary operator expression."
    };
    (SemLiteral) => { "A literal expression." };
    (SemFunctionCall) => { "A call to a resolved function." };
    (SemBuiltinFunctionCall) => { "A call to a built-in function." };
    (SemCast) => { "A type cast expression." };
    (SemIndexedExpr) => { "An indexing expression." };
    (SemParen) => { "A parenthesized expression." };
    (SemMeasure) => { "A measurement expression." };
    (SemRuntimeSizeof) => { "A runtime ``sizeof`` expression." };
    (SemEvaluatedDurationof) => { "An evaluated ``durationof`` expression." };
    (SemConcat) => { "A concatenation expression." };
    (SemAliasDecl) => { "An alias declaration statement." };
    (SemAssign) => { "An assignment statement." };
    (SemBarrier) => { "A barrier statement." };
    (SemBox) => { "A box statement." };
    (SemBlock) => { "A block of statements." };
    (SemBreak) => { "A break statement." };
    (SemCalibration) => { "A calibration statement." };
    (SemCalibrationGrammar) => { "A calibration grammar statement." };
    (SemClassicalDecl) => { "A classical variable declaration statement." };
    (SemContinue) => { "A continue statement." };
    (SemDef) => { "A subroutine definition statement." };
    (SemDefCal) => { "A ``defcal`` statement." };
    (SemDelay) => { "A delay statement." };
    (SemEnd) => { "An end statement." };
    (SemExprStmt) => { "An expression statement." };
    (SemExternDecl) => { "An extern declaration statement." };
    (SemForLoop) => { "A ``for`` loop statement." };
    (SemGateCall) => { "A gate call statement." };
    (SemIfStmt) => { "An ``if`` statement." };
    (SemIndexedAssign) => { "An indexed assignment statement." };
    (SemInputDeclaration) => { "An input declaration statement." };
    (SemOutputDeclaration) => { "An output declaration statement." };
    (SemPragma) => {
        "A pragma statement.\n\n:attr:`command` contains all text after the keyword. :attr:`name` and\n:attr:`value` split that text into its leading dotted identifier and the\nremaining content for convenient inspection."
    };
    (SemGateDefinition) => { "A quantum gate definition statement." };
    (SemQubitDecl) => { "A qubit declaration statement." };
    (SemQubitArrayDecl) => { "A qubit array declaration statement." };
    (SemReset) => { "A reset statement." };
    (SemReturn) => { "A return statement." };
    (SemWhileLoop) => { "A ``while`` loop statement." };
    (SemErrStmt) => {
        "A placeholder for a statement semantic analysis could not resolve.\n\nInspect the analysis result's diagnostics for the cause. The placeholder\npreserves the source span so tools can continue traversing the recovered\ntree."
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

    // ---- munch: secondary source span ----
    // Deliberately does not extend `rf`, which is the single accumulator that
    // feeds `__repr__`, `__eq__`, and `__hash__`. That omission is the whole
    // point of this kind: a source position is reachable but never structural.
    (@munch $cat:ident, $name:ident,
        meta { $($meta:tt)* }, disp { $($disp:tt)* },
        sf { $($sf:tt)* }, param { $($param:tt)* }, ctor { $($ctor:tt)* },
        nodes { $($n:tt)* }, opts { $($o:tt)* }, lists { $($l:tt)* }, rf { $($rf:tt)* };
        $(#[$fmeta:meta])* $f:ident : span , $($rest:tt)*
    ) => {
        qasm_node!(@munch $cat, $name,
            meta { $($meta)* }, disp { $($disp)* },
            sf { $($sf)* $(#[$fmeta])* #[pyo3(get)] $f: Span, },
            param { $($param)* $f: Span, },
            ctor { $($ctor)* $f, },
            nodes { $($n)* }, opts { $($o)* }, lists { $($l)* },
            rf { $($rf)* };
            $($rest)*);
    };

    // ---- munch: optional secondary source span ----
    (@munch $cat:ident, $name:ident,
        meta { $($meta:tt)* }, disp { $($disp:tt)* },
        sf { $($sf:tt)* }, param { $($param:tt)* }, ctor { $($ctor:tt)* },
        nodes { $($n:tt)* }, opts { $($o:tt)* }, lists { $($l:tt)* }, rf { $($rf:tt)* };
        $(#[$fmeta:meta])* $f:ident : optspan , $($rest:tt)*
    ) => {
        qasm_node!(@munch $cat, $name,
            meta { $($meta)* }, disp { $($disp)* },
            sf { $($sf)* $(#[$fmeta])* #[pyo3(get)] $f: Option<Span>, },
            param { $($param)* $f: Option<Span>, },
            ctor { $($ctor)* $f, },
            nodes { $($n)* }, opts { $($o)* }, lists { $($l)* },
            rf { $($rf)* };
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
        #[doc = qasm_node_doc!($name)]
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
            #[allow(clippy::too_many_arguments)]
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
        #[doc = qasm_node_doc!($name)]
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
            #[allow(clippy::too_many_arguments)]
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
        #[doc = qasm_node_doc!($name)]
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
            #[allow(clippy::too_many_arguments)]
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
        #[doc = qasm_node_doc!($name)]
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
            #[allow(clippy::too_many_arguments)]
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
        #[doc = concat!("An analyzed OpenQASM `", $($disp)*, "` component.")]
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
            #[allow(clippy::too_many_arguments)]
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
            #[allow(clippy::too_many_arguments)]
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
        #[doc = concat!("A parsed OpenQASM `", $($disp)*, "` component.")]
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
            #[allow(clippy::too_many_arguments)]
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
            #[allow(clippy::too_many_arguments)]
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
        #[doc = qasm_node_doc!($name)]
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
            #[allow(clippy::too_many_arguments)]
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
        #[doc = qasm_node_doc!($name)]
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
            #[allow(clippy::too_many_arguments)]
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
