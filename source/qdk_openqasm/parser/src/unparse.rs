// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A source emitter (or "unparser") for the syntactic `OpenQASM` AST.
//!
//! Given a [`Program`] produced by [`crate::parser::parse`], [`unparse`] emits
//! canonical, syntactically valid `OpenQASM 3` source. Trivia (comments and the
//! original whitespace) are not preserved by the AST, so the output is a
//! normalized rendering rather than a byte-for-byte reproduction of the input.
//!
//! The emitter is precedence-aware: nested binary and unary expressions are
//! parenthesized only when required to preserve the parsed structure, mirroring
//! the precedence table used by [`crate::parser::expr`].

use crate::parser::ast::{
    AccessControl, Annotation, ArrayBaseTypeKind, ArrayReferenceType, ArrayType, BinOp, Block,
    DefParameter, DefParameterType, EnumerableSet, Expr, ExprKind, ExternParameter, ForStmt,
    GateModifierKind, GateOperand, GateOperandKind, IdentOrIndexedIdent, IfStmt, Index, IndexList,
    IndexListItem, LiteralKind, MeasureExpr, Pragma, Program, QuantumGateModifier, Range,
    ScalarType, ScalarTypeKind, Set, Stmt, StmtKind, SwitchStmt, TimeUnit, TypeDef, UnaryOp,
    ValueExpr, WhileLoop,
};

/// Emits valid `OpenQASM 3` source for a syntactic [`Program`].
///
/// The output is canonical: it reflects the parsed structure with normalized
/// whitespace and indentation, and does not preserve comments. Re-parsing the
/// emitted source yields an equivalent program.
#[must_use]
pub fn unparse(program: &Program) -> String {
    let mut out = String::new();
    if let Some(version) = &program.version {
        out.push_str("OPENQASM ");
        out.push_str(&version.to_string());
        out.push_str(";\n");
    }
    for stmt in &program.statements {
        out.push_str(&stmt_to_string(stmt, 0));
    }
    out
}

// ----------------------------------------------------------------------------
// Indentation helpers
// ----------------------------------------------------------------------------

fn pad(indent: usize) -> String {
    "    ".repeat(indent)
}

/// Wraps a single-line statement `core` with the leading indentation and a
/// trailing newline.
fn line(indent: usize, core: &str) -> String {
    format!("{}{core}\n", pad(indent))
}

// ----------------------------------------------------------------------------
// Statements
// ----------------------------------------------------------------------------

fn stmt_to_string(stmt: &Stmt, indent: usize) -> String {
    let mut out = String::new();
    for annotation in &stmt.annotations {
        out.push_str(&line(indent, &annotation_to_string(annotation)));
    }
    out.push_str(&stmt_kind_to_string(&stmt.kind, indent));
    out
}

#[allow(clippy::too_many_lines)]
fn stmt_kind_to_string(kind: &StmtKind, indent: usize) -> String {
    match kind {
        StmtKind::Alias(s) => line(
            indent,
            &format!(
                "let {} = {};",
                ident_or_indexed(&s.ident),
                join_exprs(&s.exprs, " ++ ")
            ),
        ),
        StmtKind::Assign(s) => line(
            indent,
            &format!("{} = {};", ident_or_indexed(&s.lhs), value_expr(&s.rhs)),
        ),
        StmtKind::AssignOp(s) => line(
            indent,
            &format!(
                "{} {}= {};",
                ident_or_indexed(&s.lhs),
                bin_op_symbol(s.op),
                value_expr(&s.rhs)
            ),
        ),
        StmtKind::Barrier(s) => {
            let operands = join_gate_operands(&s.qubits);
            if operands.is_empty() {
                line(indent, "barrier;")
            } else {
                line(indent, &format!("barrier {operands};"))
            }
        }
        StmtKind::Box(s) => box_stmt(s, indent),
        StmtKind::Break(_) => line(indent, "break;"),
        StmtKind::Block(b) => format!("{}{}\n", pad(indent), block(b, indent)),
        StmtKind::Cal(s) => line(indent, &s.content),
        StmtKind::CalibrationGrammar(s) => line(indent, &format!("defcalgrammar \"{}\";", s.name)),
        StmtKind::ClassicalDecl(s) => {
            let init = match &s.init_expr {
                Some(init) => format!(" = {}", value_expr(init)),
                None => String::new(),
            };
            line(
                indent,
                &format!("{} {}{init};", type_def(&s.ty), s.identifier.name),
            )
        }
        StmtKind::ConstDecl(s) => line(
            indent,
            &format!(
                "const {} {} = {};",
                type_def(&s.ty),
                s.identifier.name,
                value_expr(&s.init_expr)
            ),
        ),
        StmtKind::Continue(_) => line(indent, "continue;"),
        StmtKind::Def(s) => {
            let params = s
                .params
                .iter()
                .map(|p| def_parameter(p))
                .collect::<Vec<_>>()
                .join(", ");
            let ret = match &s.return_type {
                Some(ty) => format!(" -> {}", scalar_type(ty)),
                None => String::new(),
            };
            format!(
                "{}def {}({params}){ret} {}\n",
                pad(indent),
                s.name.name,
                block(&s.body, indent)
            )
        }
        StmtKind::DefCal(s) => line(indent, &s.content),
        StmtKind::Delay(s) => line(
            indent,
            &format!("delay[{}]{};", expr(&s.duration), space_operands(&s.qubits)),
        ),
        StmtKind::End(_) => line(indent, "end;"),
        StmtKind::ExprStmt(s) => {
            let rendered = expr(&s.expr);
            if rendered.is_empty() {
                String::new()
            } else {
                line(indent, &format!("{rendered};"))
            }
        }
        StmtKind::ExternDecl(s) => {
            let params = s
                .params
                .iter()
                .map(|p| extern_parameter(p))
                .collect::<Vec<_>>()
                .join(", ");
            let ret = match &s.return_type {
                Some(ty) => format!(" -> {}", scalar_type(ty)),
                None => String::new(),
            };
            line(indent, &format!("extern {}({params}){ret};", s.ident.name))
        }
        StmtKind::For(s) => for_stmt(s, indent),
        StmtKind::If(s) => if_stmt(s, indent),
        StmtKind::GateCall(s) => {
            let args = if s.args.is_empty() {
                String::new()
            } else {
                format!("({})", join_exprs(&s.args, ", "))
            };
            let duration = match &s.duration {
                Some(d) => format!("[{}]", expr(d)),
                None => String::new(),
            };
            line(
                indent,
                &format!(
                    "{}{}{args}{duration}{};",
                    modifiers(&s.modifiers),
                    s.name.name,
                    space_operands(&s.qubits)
                ),
            )
        }
        StmtKind::GPhase(s) => {
            let args = if s.args.is_empty() {
                String::new()
            } else {
                format!("({})", join_exprs(&s.args, ", "))
            };
            let duration = match &s.duration {
                Some(d) => format!("[{}]", expr(d)),
                None => String::new(),
            };
            line(
                indent,
                &format!(
                    "{}gphase{args}{duration}{};",
                    modifiers(&s.modifiers),
                    space_operands(&s.qubits)
                ),
            )
        }
        StmtKind::Include(s) => line(indent, &format!("include \"{}\";", s.filename)),
        StmtKind::IODeclaration(s) => line(
            indent,
            &format!("{} {} {};", s.io_identifier, type_def(&s.ty), s.ident.name),
        ),
        StmtKind::Measure(s) => {
            let target = match &s.target {
                Some(target) => format!(" -> {}", ident_or_indexed(target)),
                None => String::new(),
            };
            line(
                indent,
                &format!("{}{target};", measure_expr(&s.measurement)),
            )
        }
        StmtKind::Pragma(s) => line(indent, &pragma_to_string(s)),
        StmtKind::QuantumGateDefinition(s) => {
            let params = s
                .params
                .iter()
                .filter_map(|p| p.item_as_ref())
                .map(|id| id.name.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let params = if params.is_empty() {
                String::new()
            } else {
                format!("({params})")
            };
            let qubits = s
                .qubits
                .iter()
                .filter_map(|q| q.item_as_ref())
                .map(|id| id.name.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{}gate {}{params} {qubits} {}\n",
                pad(indent),
                s.ident.name,
                block(&s.body, indent)
            )
        }
        StmtKind::QuantumDecl(s) => {
            let size = size_designator(s.ty.size.as_ref());
            line(indent, &format!("qubit{size} {};", s.qubit.name))
        }
        StmtKind::Reset(s) => line(indent, &format!("reset {};", gate_operand(&s.operand))),
        StmtKind::Return(s) => match &s.expr {
            Some(value) => line(indent, &format!("return {};", value_expr(value))),
            None => line(indent, "return;"),
        },
        StmtKind::Switch(s) => switch_stmt(s, indent),
        StmtKind::WhileLoop(s) => while_stmt(s, indent),
        StmtKind::Err => String::new(),
    }
}

fn box_stmt(s: &crate::parser::ast::BoxStmt, indent: usize) -> String {
    let duration = match &s.duration {
        Some(d) => format!("[{}]", expr(d)),
        None => String::new(),
    };
    let mut inner = String::new();
    for stmt in &s.body {
        inner.push_str(&stmt_to_string(stmt, indent + 1));
    }
    format!("{0}box{duration} {{\n{inner}{0}}}\n", pad(indent))
}

fn for_stmt(s: &ForStmt, indent: usize) -> String {
    format!(
        "{}for {} {} in {} {}\n",
        pad(indent),
        scalar_type(&s.ty),
        s.ident.name,
        enumerable_set(&s.set_declaration),
        body(&s.body, indent)
    )
}

fn if_stmt(s: &IfStmt, indent: usize) -> String {
    let else_part = match &s.else_body {
        Some(else_body) => format!(" else {}", body(else_body, indent)),
        None => String::new(),
    };
    format!(
        "{}if ({}) {}{else_part}\n",
        pad(indent),
        expr(&s.condition),
        body(&s.if_body, indent)
    )
}

fn while_stmt(s: &WhileLoop, indent: usize) -> String {
    format!(
        "{}while ({}) {}\n",
        pad(indent),
        expr(&s.while_condition),
        body(&s.body, indent)
    )
}

fn switch_stmt(s: &SwitchStmt, indent: usize) -> String {
    let cases: String = s
        .cases
        .iter()
        .map(|case| {
            format!(
                "{}case {} {}\n",
                pad(indent + 1),
                join_exprs(&case.labels, ", "),
                block(&case.block, indent + 1)
            )
        })
        .collect::<Vec<_>>()
        .concat();
    let default = match &s.default {
        Some(default) => format!(
            "{}default {}\n",
            pad(indent + 1),
            block(default, indent + 1)
        ),
        None => String::new(),
    };
    format!(
        "{0}switch ({1}) {{\n{cases}{default}{0}}}\n",
        pad(indent),
        expr(&s.target)
    )
}

/// Renders a block `{ ... }`. The opening brace is on the current line; the
/// closing brace is indented to `indent`. No trailing newline is added.
fn block(b: &Block, indent: usize) -> String {
    let mut out = String::from("{\n");
    for stmt in &b.stmts {
        out.push_str(&stmt_to_string(stmt, indent + 1));
    }
    out.push_str(&pad(indent));
    out.push('}');
    out
}

/// Renders the body of a control-flow statement. A block body is emitted as-is;
/// a single statement is wrapped in a block so the output is always valid.
fn body(stmt: &Stmt, indent: usize) -> String {
    if let StmtKind::Block(b) = &*stmt.kind {
        block(b, indent)
    } else {
        format!("{{\n{}{}}}", stmt_to_string(stmt, indent + 1), pad(indent))
    }
}

fn annotation_to_string(annotation: &Annotation) -> String {
    let mut out = format!("@{}", annotation.identifier.as_string());
    if let Some(value) = &annotation.value {
        out.push(' ');
        out.push_str(value);
    }
    out
}

fn pragma_to_string(pragma: &Pragma) -> String {
    let mut out = String::from("pragma");
    if let Some(identifier) = &pragma.identifier {
        out.push(' ');
        out.push_str(&identifier.as_string());
    }
    if let Some(value) = &pragma.value {
        out.push(' ');
        out.push_str(value);
    }
    out
}

// ----------------------------------------------------------------------------
// Gate operands, modifiers, and measurements
// ----------------------------------------------------------------------------

fn modifiers(modifiers: &[Box<QuantumGateModifier>]) -> String {
    let mut out = String::new();
    for modifier in modifiers {
        out.push_str(&gate_modifier(&modifier.kind));
        out.push(' ');
    }
    out
}

fn gate_modifier(kind: &GateModifierKind) -> String {
    match kind {
        GateModifierKind::Inv => "inv @".to_string(),
        GateModifierKind::Pow(e) => format!("pow({}) @", expr(e)),
        GateModifierKind::Ctrl(None) => "ctrl @".to_string(),
        GateModifierKind::Ctrl(Some(e)) => format!("ctrl({}) @", expr(e)),
        GateModifierKind::NegCtrl(None) => "negctrl @".to_string(),
        GateModifierKind::NegCtrl(Some(e)) => format!("negctrl({}) @", expr(e)),
    }
}

fn measure_expr(measure: &MeasureExpr) -> String {
    format!("measure {}", gate_operand(&measure.operand))
}

fn gate_operand(operand: &GateOperand) -> String {
    match &operand.kind {
        GateOperandKind::IdentOrIndexedIdent(ident) => ident_or_indexed(ident),
        GateOperandKind::HardwareQubit(qubit) => format!("${}", qubit.name),
        GateOperandKind::Err => String::new(),
    }
}

fn join_gate_operands(operands: &[Box<GateOperand>]) -> String {
    operands
        .iter()
        .map(|operand| gate_operand(operand))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Renders a leading-space-prefixed gate operand list, or the empty string when
/// there are no operands.
fn space_operands(operands: &[Box<GateOperand>]) -> String {
    let list = join_gate_operands(operands);
    if list.is_empty() {
        String::new()
    } else {
        format!(" {list}")
    }
}

fn ident_or_indexed(ident: &IdentOrIndexedIdent) -> String {
    match ident {
        IdentOrIndexedIdent::Ident(ident) => ident.name.to_string(),
        IdentOrIndexedIdent::IndexedIdent(indexed) => {
            let mut out = indexed.ident.name.to_string();
            for index in &indexed.indices {
                out.push_str(&index_to_string(index));
            }
            out
        }
    }
}

// ----------------------------------------------------------------------------
// Value expressions (rhs of declarations, assignments, returns)
// ----------------------------------------------------------------------------

fn value_expr(value: &ValueExpr) -> String {
    match value {
        ValueExpr::Concat(concat) => join_exprs(&concat.operands, " ++ "),
        ValueExpr::Expr(e) => expr(e),
        ValueExpr::Measurement(measure) => measure_expr(measure),
    }
}

// ----------------------------------------------------------------------------
// Expressions (precedence-aware)
// ----------------------------------------------------------------------------

const UNARY_PREC: u8 = 11;
const ATOM_PREC: u8 = 15;

fn bin_op_precedence(op: BinOp) -> u8 {
    match op {
        BinOp::Exp => 12,
        BinOp::Mul | BinOp::Div | BinOp::Mod => 10,
        BinOp::Add | BinOp::Sub => 9,
        BinOp::Shl | BinOp::Shr => 8,
        BinOp::Gt | BinOp::Gte | BinOp::Lt | BinOp::Lte => 7,
        BinOp::Eq | BinOp::Neq => 6,
        BinOp::AndB => 5,
        BinOp::OrB => 4,
        BinOp::XorB => 3,
        BinOp::AndL => 2,
        BinOp::OrL => 1,
    }
}

fn bin_op_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Exp => "**",
        BinOp::Eq => "==",
        BinOp::Neq => "!=",
        BinOp::Gt => ">",
        BinOp::Gte => ">=",
        BinOp::Lt => "<",
        BinOp::Lte => "<=",
        BinOp::AndB => "&",
        BinOp::OrB => "|",
        BinOp::XorB => "^",
        BinOp::AndL => "&&",
        BinOp::OrL => "||",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
    }
}

fn unary_op_symbol(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::NotB => "~",
        UnaryOp::NotL => "!",
    }
}

fn expr_precedence(kind: &ExprKind) -> u8 {
    match kind {
        ExprKind::BinaryOp(b) => bin_op_precedence(b.op),
        ExprKind::UnaryOp(_) => UNARY_PREC,
        _ => ATOM_PREC,
    }
}

fn parenthesize(rendered: String, needs_parens: bool) -> String {
    if needs_parens {
        format!("({rendered})")
    } else {
        rendered
    }
}

fn expr(e: &Expr) -> String {
    match &*e.kind {
        ExprKind::Err => String::new(),
        ExprKind::Ident(ident) => ident.name.to_string(),
        ExprKind::UnaryOp(u) => {
            let operand_prec = expr_precedence(&u.expr.kind);
            let needs_parens =
                operand_prec < UNARY_PREC || matches!(&*u.expr.kind, ExprKind::UnaryOp(_));
            format!(
                "{}{}",
                unary_op_symbol(u.op),
                parenthesize(expr(&u.expr), needs_parens)
            )
        }
        ExprKind::BinaryOp(b) => {
            let prec = bin_op_precedence(b.op);
            let right_assoc = matches!(b.op, BinOp::Exp);
            let lhs_prec = expr_precedence(&b.lhs.kind);
            let rhs_prec = expr_precedence(&b.rhs.kind);
            let lhs_parens = lhs_prec < prec || (lhs_prec == prec && right_assoc);
            let rhs_parens = rhs_prec < prec || (rhs_prec == prec && !right_assoc);
            format!(
                "{} {} {}",
                parenthesize(expr(&b.lhs), lhs_parens),
                bin_op_symbol(b.op),
                parenthesize(expr(&b.rhs), rhs_parens)
            )
        }
        ExprKind::Lit(lit) => literal(&lit.kind),
        ExprKind::FunctionCall(call) => {
            format!("{}({})", call.name.name, join_exprs(&call.args, ", "))
        }
        ExprKind::Cast(cast) => format!("{}({})", type_def(&cast.ty), expr(&cast.arg)),
        ExprKind::IndexExpr(index) => {
            format!(
                "{}{}",
                expr(&index.collection),
                index_to_string(&index.index)
            )
        }
        ExprKind::Paren(inner) => format!("({})", expr(inner)),
        ExprKind::DurationOf(duration) => {
            format!("durationof({})", block(&duration.scope, 0))
        }
    }
}

fn join_exprs(exprs: &[Box<Expr>], separator: &str) -> String {
    exprs
        .iter()
        .map(|e| expr(e))
        .collect::<Vec<_>>()
        .join(separator)
}

fn literal(kind: &LiteralKind) -> String {
    match kind {
        LiteralKind::Array(exprs) => format!("{{{}}}", join_exprs(exprs, ", ")),
        LiteralKind::Bitstring(value, width) => {
            let width = *width as usize;
            format!("\"{:0>width$}\"", value.to_str_radix(2))
        }
        LiteralKind::Bool(b) => b.to_string(),
        LiteralKind::Duration(value, unit) => {
            format!("{}{}", format_float(*value), time_unit(*unit))
        }
        LiteralKind::Float(value) => format_float(*value),
        LiteralKind::Imaginary(value) => format!("{}im", format_float(*value)),
        LiteralKind::Int(value) => value.to_string(),
        LiteralKind::BigInt(value) => value.to_string(),
        LiteralKind::String(value) => format!("\"{value}\""),
    }
}

fn format_float(value: f64) -> String {
    if value.is_nan() || value.is_infinite() {
        return value.to_string();
    }
    let rendered = value.to_string();
    if rendered.contains(['.', 'e', 'E']) {
        rendered
    } else {
        format!("{rendered}.0")
    }
}

fn time_unit(unit: TimeUnit) -> &'static str {
    match unit {
        TimeUnit::Dt => "dt",
        TimeUnit::Ns => "ns",
        TimeUnit::Us => "us",
        TimeUnit::Ms => "ms",
        TimeUnit::S => "s",
    }
}

// ----------------------------------------------------------------------------
// Indices, ranges, and sets
// ----------------------------------------------------------------------------

fn index_to_string(index: &Index) -> String {
    match index {
        Index::IndexSet(set) => format!("[{}]", set_to_string(set)),
        Index::IndexList(list) => format!("[{}]", index_list(list)),
    }
}

fn index_list(list: &IndexList) -> String {
    list.values
        .iter()
        .map(|item| index_list_item(item))
        .collect::<Vec<_>>()
        .join(", ")
}

fn index_list_item(item: &IndexListItem) -> String {
    match item {
        IndexListItem::RangeDefinition(range) => range_to_string(range),
        IndexListItem::Expr(e) => expr(e),
        IndexListItem::Err => String::new(),
    }
}

fn range_to_string(range: &Range) -> String {
    let start = range.start.as_ref().map(expr).unwrap_or_default();
    let end = range.end.as_ref().map(expr).unwrap_or_default();
    match &range.step {
        Some(step) => format!("{start}:{}:{end}", expr(step)),
        None => format!("{start}:{end}"),
    }
}

fn set_to_string(set: &Set) -> String {
    format!("{{{}}}", join_exprs(&set.values, ", "))
}

fn enumerable_set(set: &EnumerableSet) -> String {
    match set {
        EnumerableSet::Set(set) => set_to_string(set),
        EnumerableSet::Range(range) => format!("[{}]", range_to_string(range)),
        EnumerableSet::Expr(e) => expr(e),
    }
}

// ----------------------------------------------------------------------------
// Types
// ----------------------------------------------------------------------------

fn size_designator(size: Option<&Expr>) -> String {
    match size {
        Some(size) => format!("[{}]", expr(size)),
        None => String::new(),
    }
}

fn type_def(ty: &TypeDef) -> String {
    match ty {
        TypeDef::Scalar(scalar) => scalar_type(scalar),
        TypeDef::Array(array) => array_type(array),
        TypeDef::ArrayReference(array) => array_reference_type(array),
    }
}

fn scalar_type(ty: &ScalarType) -> String {
    match &ty.kind {
        ScalarTypeKind::Bit(bit) => format!("bit{}", size_designator(bit.size.as_ref())),
        ScalarTypeKind::Int(int) => format!("int{}", size_designator(int.size.as_ref())),
        ScalarTypeKind::UInt(uint) => format!("uint{}", size_designator(uint.size.as_ref())),
        ScalarTypeKind::Float(float) => format!("float{}", size_designator(float.size.as_ref())),
        ScalarTypeKind::Complex(complex) => match &complex.base_size {
            Some(base) => format!("complex[float{}]", size_designator(base.size.as_ref())),
            None => "complex".to_string(),
        },
        ScalarTypeKind::Angle(angle) => format!("angle{}", size_designator(angle.size.as_ref())),
        ScalarTypeKind::BoolType => "bool".to_string(),
        ScalarTypeKind::Duration => "duration".to_string(),
        ScalarTypeKind::Stretch => "stretch".to_string(),
        ScalarTypeKind::Err => String::new(),
    }
}

fn array_base_type(base: &ArrayBaseTypeKind) -> String {
    match base {
        ArrayBaseTypeKind::Int(int) => format!("int{}", size_designator(int.size.as_ref())),
        ArrayBaseTypeKind::UInt(uint) => format!("uint{}", size_designator(uint.size.as_ref())),
        ArrayBaseTypeKind::Float(float) => format!("float{}", size_designator(float.size.as_ref())),
        ArrayBaseTypeKind::Complex(complex) => match &complex.base_size {
            Some(base) => format!("complex[float{}]", size_designator(base.size.as_ref())),
            None => "complex".to_string(),
        },
        ArrayBaseTypeKind::Angle(angle) => format!("angle{}", size_designator(angle.size.as_ref())),
        ArrayBaseTypeKind::BoolType => "bool".to_string(),
        ArrayBaseTypeKind::Duration => "duration".to_string(),
    }
}

fn array_type(array: &ArrayType) -> String {
    format!(
        "array[{}, {}]",
        array_base_type(&array.base_type),
        join_exprs(&array.dimensions, ", ")
    )
}

fn access_control(access: &AccessControl) -> &'static str {
    match access {
        AccessControl::ReadOnly => "readonly",
        AccessControl::Mutable => "mutable",
    }
}

fn array_reference_type(array: &ArrayReferenceType) -> String {
    match array {
        ArrayReferenceType::Static(ty) => format!(
            "{} array[{}, {}]",
            access_control(&ty.mutability),
            array_base_type(&ty.base_type),
            join_exprs(&ty.dimensions, ", ")
        ),
        ArrayReferenceType::Dyn(ty) => format!(
            "{} array[{}, #dim = {}]",
            access_control(&ty.mutability),
            array_base_type(&ty.base_type),
            expr(&ty.dimensions)
        ),
    }
}

fn def_parameter(param: &DefParameter) -> String {
    let ty = match &*param.ty {
        DefParameterType::ArrayReference(array) => array_reference_type(array),
        DefParameterType::Qubit(qubit) => format!("qubit{}", size_designator(qubit.size.as_ref())),
        DefParameterType::Scalar(scalar) => scalar_type(scalar),
    };
    format!("{ty} {}", param.ident.name)
}

fn extern_parameter(param: &ExternParameter) -> String {
    match param {
        ExternParameter::Scalar(scalar, _) => scalar_type(scalar),
        ExternParameter::ArrayReference(array, _) => array_reference_type(array),
    }
}

// ----------------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::unparse;
    use crate::parser::parse;

    /// Parses `source`, asserts there are no parse errors, and returns the
    /// emitted source.
    fn emit(source: &str) -> String {
        let (program, errors) = parse(source);
        assert!(
            errors.is_empty(),
            "unexpected parse errors for source:\n{source}\nerrors: {errors:?}"
        );
        unparse(&program)
    }

    /// Asserts that `source` round-trips: the emitted source re-parses without
    /// errors, and a second emission is identical to the first (stable).
    fn assert_round_trip(source: &str) {
        let emitted = emit(source);
        let (program2, errors2) = parse(&emitted);
        assert!(
            errors2.is_empty(),
            "re-parsing emitted source produced errors:\n{emitted}\nerrors: {errors2:?}"
        );
        let emitted2 = unparse(&program2);
        assert_eq!(
            emitted, emitted2,
            "emission is not stable:\nfirst:\n{emitted}\nsecond:\n{emitted2}"
        );
    }

    #[test]
    fn version_header() {
        let emitted = emit("OPENQASM 3.0;");
        assert_eq!(emitted, "OPENQASM 3.0;\n");
        assert_round_trip("OPENQASM 3.0;");
    }

    #[test]
    fn qubit_and_bit_declarations() {
        assert_round_trip(
            "OPENQASM 3.0;
            qubit q;
            qubit[4] qs;
            bit b;
            bit[4] bs;",
        );
    }

    #[test]
    fn include_statement() {
        assert_round_trip("include \"stdgates.inc\";");
    }

    #[test]
    fn gate_definition_and_call() {
        assert_round_trip(
            "gate my_gate(theta, phi) a, b {
                rx(theta) a;
                ry(phi) b;
                cx a, b;
            }
            qubit[2] q;
            my_gate(0.5, 1.5) q[0], q[1];",
        );
    }

    #[test]
    fn gate_call_with_modifiers() {
        assert_round_trip(
            "qubit[3] q;
            ctrl @ x q[0], q[1];
            inv @ pow(2) @ s q[2];
            negctrl(2) @ x q[0], q[1], q[2];",
        );
    }

    #[test]
    fn measurements_and_reset() {
        assert_round_trip(
            "qubit[2] q;
            bit[2] c;
            reset q[0];
            c[0] = measure q[0];
            measure q[1] -> c[1];",
        );
    }

    #[test]
    fn barrier_and_gphase() {
        assert_round_trip(
            "qubit[2] q;
            barrier q[0], q[1];
            barrier;
            gphase(0.5);",
        );
    }

    #[test]
    fn classical_arithmetic_precedence() {
        let emitted = emit(
            "int a = 1;
            int b = 2;
            int c = 3;
            int d = a + b * c;
            int e = (a + b) * c;
            int f = a ** b ** c;
            int g = (a ** b) ** c;
            int h = -a + b;
            int i = -(a + b);",
        );
        assert!(emitted.contains("a + b * c;"), "got:\n{emitted}");
        assert!(emitted.contains("(a + b) * c;"), "got:\n{emitted}");
        assert!(emitted.contains("a ** b ** c;"), "got:\n{emitted}");
        assert!(emitted.contains("(a ** b) ** c;"), "got:\n{emitted}");
        assert!(emitted.contains("-a + b;"), "got:\n{emitted}");
        assert!(emitted.contains("-(a + b);"), "got:\n{emitted}");
        assert_round_trip(
            "int a = 1;
            int b = 2;
            int c = 3;
            int d = a + b * c;
            int e = (a + b) * c;
            int f = a ** b ** c;
            int g = (a ** b) ** c;",
        );
    }

    #[test]
    fn if_else_statement() {
        assert_round_trip(
            "int a = 1;
            qubit q;
            if (a == 1) {
                x q;
            } else {
                y q;
            }",
        );
    }

    #[test]
    fn for_and_while_loops() {
        assert_round_trip(
            "qubit q;
            for int i in [0:10] {
                x q;
            }
            for int j in {1, 3, 5} {
                h q;
            }
            int k = 0;
            while (k < 10) {
                k += 1;
            }",
        );
    }

    #[test]
    fn casts_and_indexing() {
        assert_round_trip(
            "int[32] a = 5;
            float[64] b = float[64](a);
            array[int[32], 3] arr;
            int c = arr[0] + arr[1];
            uint d = uint(b);",
        );
    }

    #[test]
    fn const_and_io_declarations() {
        assert_round_trip(
            "const int n = 4;
            input float theta;
            output bit result;",
        );
    }

    #[test]
    fn def_with_return() {
        assert_round_trip(
            "def add(int a, int b) -> int {
                return a + b;
            }",
        );
    }

    #[test]
    fn switch_statement() {
        assert_round_trip(
            "int a = 1;
            qubit q;
            switch (a) {
                case 1, 2 {
                    x q;
                }
                case 3 {
                    y q;
                }
                default {
                    z q;
                }
            }",
        );
    }

    #[test]
    fn alias_and_concat() {
        assert_round_trip(
            "qubit[4] q;
            let a = q[0] ++ q[1];",
        );
    }

    #[test]
    fn literals() {
        assert_round_trip(
            "float f = 3.5;
            int i = 42;
            bool b = true;
            bit[4] bs = \"1010\";
            duration d = 10ns;
            array[int[32], 3] arr = {1, 2, 3};",
        );
    }

    #[test]
    fn box_and_delay() {
        assert_round_trip(
            "qubit[2] q;
            box {
                x q[0];
                delay[10ns] q[1];
            }",
        );
    }
}
