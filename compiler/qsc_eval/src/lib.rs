// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![warn(clippy::mod_module_files, clippy::pedantic, clippy::unwrap_used)]

#[cfg(test)]
mod tests;

pub mod val;

use std::collections::{hash_map::Entry, HashMap};

use qir_backend::Pauli;
use qsc_ast::ast::{
    self, Block, CallableDecl, Expr, ExprKind, Lit, NodeId, Package, Pat, PatKind, Span, Stmt,
    StmtKind,
};
use qsc_frontend::{symbol, Context};
use val::{ConversionError, Value};

#[derive(Debug)]
pub struct Error {
    pub span: Span,
    pub kind: ErrorKind,
}

#[derive(Debug)]
pub enum ErrorKind {
    EmptyExpr,
    Index(i64),
    OutOfRange(i64),
    Type(&'static str, &'static str),
    TupleArity(usize, usize),
    Unimplemented,
    UserFail(String),
}

impl Error {
    fn unimpl(span: Span) -> Result<Value, Error> {
        Err(Self {
            span,
            kind: ErrorKind::Unimplemented,
        })
    }
}

trait WithSpan {
    type Output;

    fn with_span(self, span: Span) -> Self::Output;
}

impl<T> WithSpan for Result<T, ConversionError> {
    type Output = Result<T, Error>;

    fn with_span(self, span: Span) -> Result<T, Error> {
        self.map_err(|e| Error {
            span,
            kind: ErrorKind::Type(e.expected, e.actual),
        })
    }
}

#[allow(dead_code)]
pub struct Evaluator<'a> {
    package: &'a Package,
    context: &'a Context,
    scopes: Vec<HashMap<symbol::Id, Value>>,
    globals: HashMap<symbol::Id, &'a CallableDecl>,
}

impl<'a> Evaluator<'a> {
    #[must_use]
    pub fn new(package: &'a Package, context: &'a Context) -> Self {
        Self {
            package,
            context,
            scopes: vec![],
            globals: HashMap::default(),
        }
    }

    /// Evaluates the entry expression from the current context.
    /// # Errors
    /// Returns the first error encountered during execution.
    pub fn run(&mut self) -> Result<Value, Error> {
        if let Some(expr) = &self.package.entry {
            self.eval_expr(expr)
        } else {
            Err(Error {
                span: Span { lo: 0, hi: 0 },
                kind: ErrorKind::EmptyExpr,
            })
        }
    }

    fn eval_expr(&mut self, expr: &Expr) -> Result<Value, Error> {
        match &expr.kind {
            ExprKind::Array(arr) => {
                let mut val_arr = vec![];
                for expr in arr {
                    val_arr.push(self.eval_expr(expr)?);
                }
                Ok(Value::Array(val_arr))
            }
            ExprKind::Block(block) => self.eval_block(block),
            ExprKind::Fail(msg) => Err(Error {
                span: expr.span,
                kind: ErrorKind::UserFail(self.eval_expr(msg)?.try_into().with_span(msg.span)?),
            }),
            ExprKind::If(cond, then, els) => {
                if self.eval_expr(cond)?.try_into().with_span(cond.span)? {
                    self.eval_block(then)
                } else if let Some(els) = els {
                    self.eval_expr(els)
                } else {
                    Ok(Value::Tuple(vec![]))
                }
            }
            ExprKind::Index(arr, index) => {
                let arr = self.eval_expr(arr)?.try_into_array().with_span(arr.span)?;
                let index_val: i64 = self.eval_expr(index)?.try_into().with_span(index.span)?;
                let i: usize = index_val.try_into().map_err(|_| Error {
                    span: index.span,
                    kind: ErrorKind::Index(index_val),
                })?;
                match arr.get(i) {
                    Some(v) => Ok(v.clone()),
                    None => Err(Error {
                        span: index.span,
                        kind: ErrorKind::OutOfRange(index_val),
                    }),
                }
            }
            ExprKind::Lit(lit) => Ok(lit_to_val(lit)),
            ExprKind::Paren(expr) => self.eval_expr(expr),
            ExprKind::Path(path) => Ok(self.resolve_binding(path.id)),
            ExprKind::Range(start, step, end) => self.eval_range(start, step, end),
            ExprKind::Tuple(tup) => {
                let mut val_tup = vec![];
                for expr in tup {
                    val_tup.push(self.eval_expr(expr)?);
                }
                Ok(Value::Tuple(val_tup))
            }
            ExprKind::ArrayRepeat(_, _)
            | ExprKind::Assign(_, _)
            | ExprKind::AssignOp(_, _, _)
            | ExprKind::AssignUpdate(_, _, _)
            | ExprKind::BinOp(_, _, _)
            | ExprKind::Call(_, _)
            | ExprKind::Conjugate(_, _)
            | ExprKind::Err
            | ExprKind::Field(_, _)
            | ExprKind::For(_, _, _)
            | ExprKind::Hole
            | ExprKind::Lambda(_, _, _)
            | ExprKind::Repeat(_, _, _)
            | ExprKind::Return(_)
            | ExprKind::TernOp(_, _, _, _)
            | ExprKind::UnOp(_, _)
            | ExprKind::While(_, _) => Error::unimpl(expr.span),
        }
    }

    fn eval_range(
        &mut self,
        start: &Option<Box<Expr>>,
        step: &Option<Box<Expr>>,
        end: &Option<Box<Expr>>,
    ) -> Result<Value, Error> {
        Ok(Value::Range(
            start
                .as_ref()
                .map(|expr| self.eval_expr(expr)?.try_into().with_span(expr.span))
                .transpose()?,
            step.as_ref()
                .map(|expr| self.eval_expr(expr)?.try_into().with_span(expr.span))
                .transpose()?,
            end.as_ref()
                .map(|expr| self.eval_expr(expr)?.try_into().with_span(expr.span))
                .transpose()?,
        ))
    }

    fn eval_block(&mut self, block: &Block) -> Result<Value, Error> {
        self.scopes.push(HashMap::default());
        let result = if let Some((last, most)) = block.stmts.split_last() {
            for stmt in most {
                let _ = self.eval_stmt(stmt)?;
            }
            self.eval_stmt(last)
        } else {
            Ok(Value::Tuple(vec![]))
        };
        let _ = self.scopes.pop();
        result
    }

    fn eval_stmt(&mut self, stmt: &Stmt) -> Result<Value, Error> {
        match &stmt.kind {
            StmtKind::Expr(expr) => self.eval_expr(expr),
            StmtKind::Let(pat, expr) => {
                let val = self.eval_expr(expr)?;
                self.bind_value(pat, val, expr.span)?;
                Ok(Value::Tuple(vec![]))
            }
            StmtKind::Semi(expr) => {
                let _ = self.eval_expr(expr)?;
                Ok(Value::Tuple(vec![]))
            }
            StmtKind::Borrow(_, _, _) | StmtKind::Mutable(_, _) | StmtKind::Use(_, _, _) => {
                Error::unimpl(stmt.span)
            }
        }
    }

    fn bind_value(&mut self, pat: &Pat, val: Value, span: Span) -> Result<(), Error> {
        match &pat.kind {
            PatKind::Bind(variable, _) => {
                let id = self.context.symbols().get(variable.id).unwrap_or_else(|| {
                    panic!(
                        "Symbol resolution error: no symbol ID for {:?}",
                        variable.id
                    );
                });
                let scope = self.scopes.last_mut().expect("Binding requires a scope.");
                match scope.entry(id) {
                    Entry::Vacant(entry) => entry.insert(val),
                    Entry::Occupied(_) => panic!("{id:?} is already bound"),
                };
                Ok(())
            }
            PatKind::Discard(_) => Ok(()),
            PatKind::Elided => panic!("Elided pattern not valid syntax in binding"),
            PatKind::Paren(pat) => self.bind_value(pat, val, span),
            PatKind::Tuple(tup) => {
                let val_tup = val.try_into_tuple().with_span(span)?;
                if val_tup.len() == tup.len() {
                    for (pat, val) in tup.iter().zip(val_tup.into_iter()) {
                        self.bind_value(pat, val, span)?;
                    }
                    Ok(())
                } else {
                    Err(Error {
                        span: pat.span,
                        kind: ErrorKind::TupleArity(tup.len(), val_tup.len()),
                    })
                }
            }
        }
    }

    fn resolve_binding(&self, id: NodeId) -> Value {
        let id = self.context.symbols().get(id).unwrap_or_else(|| {
            panic!("Symbol resolution error: {id:?} not found in symbol table.");
        });
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(&id))
            .unwrap_or_else(|| panic!("Symbol resolution error: {id:?} is not bound."))
            .clone()
    }
}

fn lit_to_val(lit: &Lit) -> Value {
    match lit {
        Lit::BigInt(v) => Value::BigInt(v.clone()),
        Lit::Bool(v) => Value::Bool(*v),
        Lit::Double(v) => Value::Double(*v),
        Lit::Int(v) => Value::Int(*v),
        Lit::Pauli(v) => Value::Pauli(match v {
            ast::Pauli::I => Pauli::I,
            ast::Pauli::X => Pauli::X,
            ast::Pauli::Y => Pauli::Y,
            ast::Pauli::Z => Pauli::Z,
        }),
        Lit::Result(v) => Value::Result(match v {
            ast::Result::Zero => false,
            ast::Result::One => true,
        }),
        Lit::String(v) => Value::String(v.clone()),
    }
}
