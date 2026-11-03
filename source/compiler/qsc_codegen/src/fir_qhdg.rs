// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Lowering one FIR callable body into the QHDG graph.
//!
//! This producer lowers FIR after argument promotion. It shares the graph
//! vocabulary with the circuit and post-partial-evaluation RIR producers;
//! [`emitted_names`] exposes the names for vocabulary compatibility checks.
//!
//! What it lowers is one callable's body specialization rather than a whole
//! program, and that is the point rather than a limitation. A body optimized
//! here is optimized before partial evaluation inlines it and before a loop
//! around it is unrolled, so a redundancy removed once is removed from every
//! copy that would have been made. A body is therefore walked with its own
//! parameters still parameters, which is the case the other two producers never
//! meet.
//!
//! ## What a qubit operand is at FIR
//!
//! There is no qubit index at FIR. An index is minted by partial evaluation and
//! does not exist here, so nothing resolves the way it does below the Adaptive
//! profile. Two sources of a qubit are distinguished, and the distinction is
//! the whole soundness argument:
//!
//! * A qubit this body allocated through `__quantum__rt__qubit_allocate` is a
//!   [`Wire::Resource`]. The allocator returns a qubit that is not currently
//!   allocated, so two allocations in one body are two wires as a matter of
//!   what the runtime does, not as a matter of how the program was written.
//! * A qubit that arrived as a parameter is a [`Wire::Binding`]. Q# assigns an
//!   immutable binding once, so it holds one wire over the body, which is
//!   exactly the obligation that variant states. What it does not give is
//!   distinctness: two parameters may hold the same wire at some call site, and
//!   nothing at FIR can say they do not.
//!
//! Everything else owes [`Wire::Opaque`]: a `mutable` binding, because the
//! language permits a later `set`; an array element, because no alias analysis
//! exists at this level anywhere in this tree; and any operand form this
//! producer did not walk.
//!
//! The consequence is stated rather than hidden. An operation over one qubit
//! lowers from a binding, because comparing a binding to itself is what
//! [`Wire::Binding`] admits. An operation over two or more qubits lowers only
//! when every one of them resolved to an allocation, because threading two
//! possibly-aliasing bindings as two independent capability chains would let a
//! rewrite reorder across a wire it could not see.
//!
//! ## What it refuses
//!
//! Refusals are by name. Unsupported loop shapes, a controlled functor, a call
//! that produces a value, an expression form this producer does not model, and an
//! operand it could not resolve each return a distinct [`Error`] rather than
//! lowering to something weaker. A measurement is refused for a reason worth
//! stating: at FIR its result is a value bound to a local, not a write into a
//! declared result register, and the `classical.store:` name carries a register
//! index. Minting one here would be this producer inventing a fact the source
//! does not hold, which is the thing the shared vocabulary exists to prevent.

#[cfg(test)]
mod tests;

#[cfg(test)]
mod reach;

mod return_leg;

pub use return_leg::{RetireError, retire_into_fir};

use std::collections::{BTreeMap, BTreeSet};

use qsc_fir::fir::{
    BinOp, BlockId, CallableImpl, CallableKind, ExprId, ExprKind, Functor, Global, LocalVarId,
    Mutability, PackageId, PackageLookup, PackageStore, PackageStoreLookup, PatId, PatKind, Res,
    StmtId, StmtKind, StoreItemId, StringComponent, UnOp,
};
use qsc_fir::ty::{Prim, Ty};
use qsc_qhdg::vocabulary::{ALLOCATE, CARRY, RELEASE, RESET, THETA};
use qsc_qhdg::{
    Direction, EdgeKind, EffectDomain, EffectEvent, EffectMode, EffectResource, ExitKind, Graph,
    GraphBuilder, InterfaceBinding, InterfaceRole, NodeId, NodeOrigin, NodePayload,
    Operand as Wire, OperandWires, PortId, PortKind, RegionId, RegionKind, ResourceId,
    StableNodeId, Stage, Type, Usage, verify,
};

/// Why a callable body could not be lowered into a graph.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("item {item} in package {package} is not a callable")]
    NotACallable { package: String, item: String },
    #[error("callable '{name}' has no body specialization to walk")]
    NoBody { name: String },
    #[error("callable '{name}' has no graph form")]
    UnsupportedCallable { name: String },
    #[error("callable '{name}' is reached from inside its own body")]
    RecursiveCallable { name: String },
    #[error("a controlled call to '{name}' is outside what this producer lowers")]
    ControlledCall { name: String },
    #[error("call to '{name}' produces a value the graph has nothing to receive")]
    ValuedCall { name: String },
    #[error("a callee expression that is not a resolved item")]
    UnresolvedCallee,
    #[error("statement form '{kind}' is outside what this producer lowers")]
    UnsupportedStatement { kind: &'static str },
    #[error("expression form '{kind}' is outside what this producer lowers")]
    UnsupportedExpression { kind: &'static str },
    #[error("an argument does not match the shape of the parameter it binds")]
    ArgumentShape,
    #[error("a qubit operand resolved to no wire this producer can name")]
    UnresolvedQubit,
    #[error(
        "an operation names {count} qubits whose distinctness is not provable at FIR, so it cannot be threaded"
    )]
    AmbiguousIdentity { count: usize },
    #[error("an operation names the same qubit more than once")]
    RepeatedQubit,
    #[error("a qubit operand names no capability this body holds")]
    UnknownQubit,
    #[error("the graph could not be built: {reason}")]
    Build { reason: String },
    #[error("a loop body is outside what this producer lowers: {reason}")]
    UnsupportedLoop { reason: String },
    #[error("the lowered graph failed its own verification: {0}")]
    Verification(String),
}

/// Where a statement sits in the block a node was lowered from.
///
/// A block and an index rather than the node's `NodeOrigin`, which is a flat
/// counter over lowered calls and so cannot name a position in a block. This is
/// the address a FIR edit uses, because the execution graph is regenerated from
/// FIR rather than edited.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct StatementSite {
    pub block: BlockId,
    pub index: usize,
}

/// A lowered callable body and the statement each of its nodes came from.
///
/// Keyed by durable identity, so the table still answers after a rewrite: a
/// stable identity is never reissued, and a retired node's absence from the
/// graph is exactly the fact a return leg reads.
pub struct Lowered {
    pub graph: Graph,
    /// The package every [`StatementSite`] below addresses.
    ///
    /// A body reached through a call is attributed to the call statement that
    /// reached it, so every site names a block of the lowered callable's own
    /// package even when the walk crossed into another one.
    pub package: PackageId,
    pub sites: BTreeMap<StableNodeId, StatementSite>,
    /// Every node this lowering produced, so a return leg can tell a node it
    /// removed from one something else added.
    pub built: Vec<StableNodeId>,
    pub census: IdentityCensus,
    /// Statements containing evaluation that the graph does not represent and
    /// whose totality is unproved. Their quantum nodes may be optimized, but
    /// the entire attributed call must not be erased.
    pub retained_evaluation: BTreeSet<StatementSite>,
}

/// How the qubit operands in one body resolved through the shared wire
/// abstraction.
///
/// Reported whether or not lowering succeeded, so a decline can be attributed
/// to identity rather than inferred later from an absence of benefit.
///
/// Counted once per operand of an intrinsic call, which is the only place
/// identity is used, plus once at every place a wire could not be named at all.
/// A binding site that resolved is not counted, because the same wire would
/// then be counted again at each call boundary it crossed and a body reached
/// through two calls would look twice as resolved as it is.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IdentityCensus {
    /// Operands resolved to an allocation this body made.
    pub resource: usize,
    /// Operands named by a binding the language assigns once.
    pub binding: usize,
    /// Operands the producer could put in neither form.
    pub opaque: usize,
}

/// Lowers one callable's body specialization into a graph, refusing to return
/// one that does not verify.
///
/// # Errors
///
/// Returns an error when the body holds a construct this producer does not
/// accept, when an operand's identity cannot be established, when the graph
/// cannot be assembled, or when the assembled graph fails the verification the
/// graph crate applies to its own programs.
pub fn callable_to_graph(store: &PackageStore, callable: StoreItemId) -> Result<Graph, Error> {
    lower_callable_with_census(store, callable)
        .0
        .map(|lowered| lowered.graph)
}

/// Lowers one callable's body and reports how its qubit operands resolved
/// either way.
///
/// The census is returned alongside a refusal as well as alongside a graph,
/// because the interesting number is how often identity declined, and a body
/// that refused is exactly where that number would otherwise be lost.
pub fn lower_callable_with_census(
    store: &PackageStore,
    callable: StoreItemId,
) -> (Result<Lowered, Error>, IdentityCensus) {
    let (built, census) = Lowering::new(store).run(callable);
    let lowered = built.and_then(|(graph, sites, retained_evaluation)| {
        let diagnostics = verify(&graph);
        if !diagnostics.is_empty() {
            let reported = diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code.as_str(), diagnostic.message))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(Error::Verification(reported));
        }
        let sites = sites
            .into_iter()
            .filter_map(|(node, site)| Some((graph.stable_id(node)?, site)))
            .collect();
        let built = graph
            .nodes()
            .filter_map(|node| graph.stable_id(node.id))
            .collect();
        Ok(Lowered {
            graph,
            package: callable.package,
            sites,
            built,
            census,
            retained_evaluation,
        })
    });
    (lowered, census)
}

/// A graph and the statement position each of its nodes was built at.
type Built = (
    Graph,
    BTreeMap<NodeId, StatementSite>,
    BTreeSet<StatementSite>,
);

/// The specialization a resolved call reaches, with what the walk needs from
/// its declaration.
struct Target {
    name: String,
    package: PackageId,
    /// The parameter pattern the caller's argument binds against.
    input: PatId,
    /// The block to walk, or `None` when the callable is an intrinsic.
    block: Option<BlockId>,
}

struct Lowering<'a> {
    store: &'a PackageStore,
    builder: GraphBuilder,
    region: RegionId,
    /// The region every node of the callable's own straight line sits in.
    ///
    /// Compared against `region` to tell whether the walk is currently inside a
    /// loop body, which is a question allocation has to ask.
    root: RegionId,
    /// The capability standing for each wire as the walk proceeds.
    live: BTreeMap<Wire, PortId>,
    /// Wires the region currently being built has not taken from outside yet.
    ///
    /// Empty outside a region. A wire's first use inside one is a crossing and
    /// binds the interface; every later use is an ordinary edge.
    crossings: BTreeSet<Wire>,
    /// The port each carried wire entered the current region through.
    ///
    /// Only the first iteration takes the wire from outside, so this is where
    /// the back edge is connected back to.
    carried_entry: BTreeMap<Wire, PortId>,
    /// A small number per wire, so a node origin and an effect resource can
    /// name it without leaking an arena id.
    ordinal: BTreeMap<Wire, u32>,
    /// The wire each local variable stands for, for the body being walked.
    frame: BTreeMap<LocalVarId, Wire>,
    /// Callables currently being walked, innermost last, so a body that reaches
    /// itself is refused rather than followed.
    call_stack: Vec<StoreItemId>,
    next_resource: u64,
    next_binding: u32,
    census: IdentityCensus,
    /// A running position, so every node carries a distinct origin.
    position: u32,
    /// The statement currently being lowered, and the nodes it produced.
    site: Option<StatementSite>,
    sites: BTreeMap<NodeId, StatementSite>,
    retained_evaluation: BTreeSet<StatementSite>,
}

impl<'a> Lowering<'a> {
    fn new(store: &'a PackageStore) -> Self {
        let mut builder = GraphBuilder::new();
        let region = builder.add_region(None, RegionKind::ModuleOmega);
        Self {
            store,
            builder,
            region,
            root: region,
            live: BTreeMap::new(),
            crossings: BTreeSet::new(),
            carried_entry: BTreeMap::new(),
            ordinal: BTreeMap::new(),
            frame: BTreeMap::new(),
            call_stack: Vec::new(),
            next_resource: 0,
            next_binding: 0,
            census: IdentityCensus::default(),
            position: 0,
            site: None,
            sites: BTreeMap::new(),
            retained_evaluation: BTreeSet::new(),
        }
    }

    fn run(mut self, callable: StoreItemId) -> (Result<Built, Error>, IdentityCensus) {
        let outcome = self.lower_callable(callable);
        let census = self.census;
        match outcome {
            Ok(()) => (
                Ok((self.builder.finish(), self.sites, self.retained_evaluation)),
                census,
            ),
            Err(error) => (Err(error), census),
        }
    }

    fn lower_callable(&mut self, callable: StoreItemId) -> Result<(), Error> {
        let Some(Global::Callable(decl)) = self.store.get_global(callable) else {
            return Err(Error::NotACallable {
                package: callable.package.to_string(),
                item: callable.item.to_string(),
            });
        };
        let name = decl.name.name.to_string();
        let input = decl.input;
        let CallableImpl::Spec(spec) = &decl.implementation else {
            return Err(Error::NoBody { name });
        };
        let block = spec.body.block;

        self.declare_parameters(callable.package, input)?;
        self.call_stack.push(callable);
        self.lower_block(callable.package, block)?;
        self.call_stack.pop();
        self.release_remaining()
    }

    /// Declares one qubit for each qubit parameter of the body being lowered.
    ///
    /// A parameter has no producing node inside the body, and a capability with
    /// no source is not a capability. The declaration is what gives the chain
    /// its head, on the same terms the RIR producer declares a program's width.
    fn declare_parameters(&mut self, package: PackageId, input: PatId) -> Result<(), Error> {
        for (local, ty) in self.bindings_of(package, input) {
            if !is_qubit(&ty) {
                continue;
            }
            let wire = Wire::Binding(self.next_binding);
            self.next_binding += 1;
            self.frame.insert(local, wire);
            self.declare_wire(wire)?;
        }
        Ok(())
    }

    /// Every name one pattern binds, with the type it binds at.
    fn bindings_of(&self, package: PackageId, pat: PatId) -> Vec<(LocalVarId, Ty)> {
        let pat = self.store.get(package).get_pat(pat);
        match &pat.kind {
            PatKind::Bind(ident) => vec![(ident.id, pat.ty.clone())],
            PatKind::Discard => Vec::new(),
            PatKind::Tuple(items) => items
                .iter()
                .flat_map(|item| self.bindings_of(package, *item))
                .collect(),
        }
    }

    /// Gives a wire its declaring node and its first live capability.
    fn declare_wire(&mut self, wire: Wire) -> Result<(), Error> {
        let ordinal =
            u32::try_from(self.ordinal.len()).expect("a body declares fewer than 2^32 qubits");
        self.ordinal.insert(wire, ordinal);
        let node = self.node(
            ALLOCATE,
            vec![qubit_effect(EffectMode::Allocate, false, ordinal)],
            NodeOrigin::Qubit(ordinal),
        )?;
        let port = self.port(node, Direction::Output, PortKind::Capability, Type::Qubit)?;
        self.live.insert(wire, port);
        Ok(())
    }

    /// Consumes every capability still live, so none dangles.
    fn release_remaining(&mut self) -> Result<(), Error> {
        for wire in self.live.keys().copied().collect::<Vec<_>>() {
            self.release_wire(wire)?;
        }
        Ok(())
    }

    fn release_wire(&mut self, wire: Wire) -> Result<(), Error> {
        let source = self.live.remove(&wire).ok_or(Error::UnknownQubit)?;
        let ordinal = *self.ordinal.get(&wire).ok_or(Error::UnknownQubit)?;
        let node = self.node(
            RELEASE,
            vec![qubit_effect(EffectMode::Release, false, ordinal)],
            NodeOrigin::Qubit(ordinal),
        )?;
        let input = self.port(node, Direction::Input, PortKind::Capability, Type::Qubit)?;
        self.connect(source, input, EdgeKind::AffineCapability)
    }

    fn lower_block(&mut self, package: PackageId, block: BlockId) -> Result<(), Error> {
        let stmts = self.store.get(package).get_block(block).stmts.clone();
        for (index, stmt) in stmts.iter().enumerate() {
            let outer = self.site;
            let own = StatementSite { block, index };
            // Attribution is keyed on the call frame, not on block nesting.
            //
            // A node built inside a callee body belongs to the call that
            // reached it, because two call sites would otherwise share one
            // site. A node built inside a nested block of the callable being
            // lowered belongs to that block, because a loop body is reached
            // from exactly one place and its statements have addresses of their
            // own.
            //
            // Keying on nesting instead would break the invariant `Lowered::package`
            // documents: a `while` inside a callee is a nested block too, and
            // giving it its own site would name a block of another package,
            // which the return leg resolves against this one. That removes a
            // statement from a shared callee body and changes every call site
            // of it, which is exactly what the fallback below prevents.
            self.site = if self.in_own_frame() {
                Some(own)
            } else {
                outer.or(Some(own))
            };
            let lowered = self.lower_stmt(package, *stmt);
            self.site = outer;
            lowered?;
        }
        Ok(())
    }

    /// Whether the walk is inside the lowered callable's own frame rather than
    /// inside a body it reached through a call.
    fn in_own_frame(&self) -> bool {
        self.call_stack.len() == 1
    }

    fn lower_stmt(&mut self, package: PackageId, stmt: StmtId) -> Result<(), Error> {
        match &self.store.get(package).get_stmt(stmt).kind {
            // A nested item declaration contributes nothing until it is called.
            StmtKind::Item(_) => Ok(()),
            StmtKind::Expr(expr) | StmtKind::Semi(expr) => self.lower_effect_expr(package, *expr),
            StmtKind::Local(mutability, pat, expr) => {
                self.lower_local(package, *mutability, *pat, *expr)
            }
        }
    }

    /// Lowers a binding, which is where a qubit enters the body.
    fn lower_local(
        &mut self,
        package: PackageId,
        mutability: Mutability,
        pat: PatId,
        expr: ExprId,
    ) -> Result<(), Error> {
        let bindings = self.bindings_of(package, pat);
        let qubit = bindings
            .iter()
            .find(|(_, ty)| is_qubit(ty))
            .map(|(local, _)| *local);
        let Some(local) = qubit else {
            // Not a qubit binding. Its right-hand side may still hold a call
            // this producer has to see, so it is walked rather than skipped.
            return self.lower_effect_expr(package, expr);
        };

        if mutability == Mutability::Mutable {
            // A `set` later in the body would give this name a second wire, so
            // the binding does not hold one over the span anything compares
            // across. Counted before it is refused, so the decline is
            // attributable to identity rather than to an absence of benefit.
            self.census.opaque += 1;
            return Err(Error::UnresolvedQubit);
        }

        if self.is_allocation(package, expr) {
            if self.region != self.root {
                // The allocation would sit inside the region and its release
                // outside it, because `release_remaining` runs at the end of
                // the callable. Refused where it is met rather than left to
                // fail verification, which would name nothing about the loop.
                return Err(Error::UnsupportedLoop {
                    reason: "it allocates a qubit the release would leave the body without"
                        .to_string(),
                });
            }
            let wire = Wire::Resource(ResourceId(self.next_resource));
            self.next_resource += 1;
            self.frame.insert(local, wire);
            return self.declare_wire(wire);
        }

        // `let r = q;` names the wire `q` already holds. Anything else is a
        // form this producer did not resolve, and an unresolved qubit binding
        // is refused rather than tracked as a wire nothing can compare.
        let wire = self.wire_of(package, expr);
        if wire == Wire::Opaque {
            self.census.opaque += 1;
            return Err(Error::UnresolvedQubit);
        }
        self.frame.insert(local, wire);
        Ok(())
    }

    /// Whether an expression is the runtime call that mints a fresh qubit.
    fn is_allocation(&self, package: PackageId, expr: ExprId) -> bool {
        let ExprKind::Call(callee, _) = &self.store.get(package).get_expr(expr).kind else {
            return false;
        };
        let Ok((item, _, controlled)) = resolve_callee(self.store, package, *callee) else {
            return false;
        };
        controlled == 0
            && matches!(
                self.store.get_global(item),
                Some(Global::Callable(decl)) if &*decl.name.name == QUBIT_ALLOCATE
            )
    }

    /// Lowers an expression evaluated for its effect.
    ///
    /// Three answers, in order. A call and a block are *walked*, because both
    /// can hold a gate further down and a graph that skipped one would be a
    /// graph missing an operation the program performs. A form the judgement
    /// proves classical is *cleared*: it contributes no node, which is right
    /// because the graph does not model classical content and does not need to.
    /// Everything else is refused by name.
    ///
    /// Cleared and skipped are not the same thing, and keeping them apart is
    /// the whole design. Skipping an unrecognized form would drop whatever it
    /// contains from the graph without saying so, which is the silent-loss
    /// failure this producer is written to avoid. Clearing is a proof that
    /// there was nothing to drop.
    fn lower_effect_expr(&mut self, package: PackageId, expr: ExprId) -> Result<(), Error> {
        match &self.store.get(package).get_expr(expr).kind {
            ExprKind::Call(callee, arg) => {
                let (callee, arg) = (*callee, *arg);
                return self.lower_valued_or_effect_call(package, expr, callee, arg);
            }
            // Walked statement by statement rather than cleared as a unit. This
            // is how a desugared loop is reached at all: `loop_unification`
            // wraps every `for` and `repeat` in a block holding a classical
            // prelude and the `while` itself.
            ExprKind::Block(block) => {
                let block = *block;
                return self.lower_block(package, block);
            }
            ExprKind::While(cond, body) => {
                let (cond, body) = (*cond, *body);
                return self.lower_loop(package, cond, body);
            }
            _ => {}
        }
        let kind = &self.store.get(package).get_expr(expr).kind;
        if models_effect_expr(kind) {
            return Ok(());
        }
        let kind = expr_kind_name(kind);
        if is_effect_free(self.store, package, expr) {
            self.retain_evaluation(package, expr);
            return Ok(());
        }
        Err(Error::UnsupportedExpression { kind })
    }

    fn retain_evaluation(&mut self, package: PackageId, expr: ExprId) {
        let mut proof = EffectFreedom::new(self.store);
        proof.require_total = true;
        if !proof.expr(package, expr)
            && let Some(site) = self.site
        {
            self.retained_evaluation.insert(site);
        }
    }

    /// Lowers a call, clearing it when the graph would have to receive a value
    /// it can prove is classical.
    ///
    /// A call whose value the graph would hold is refused, and that boundary
    /// stands: it is the reach ceiling this work does not raise. What is
    /// cleared here is the other thing that shares its shape, a call such as
    /// `Length(values)` whose callee performs no quantum operation at all. Its
    /// value never enters the graph either, because nothing was added.
    fn lower_valued_or_effect_call(
        &mut self,
        package: PackageId,
        expr: ExprId,
        callee: ExprId,
        arg: ExprId,
    ) -> Result<(), Error> {
        self.retain_evaluation(package, arg);
        match self.lower_call(package, callee, arg) {
            Err(Error::ValuedCall { name }) => {
                if is_effect_free(self.store, package, expr) {
                    self.retain_evaluation(package, expr);
                    Ok(())
                } else {
                    Err(Error::ValuedCall { name })
                }
            }
            other => other,
        }
    }

    /// Lowers a `while` into a theta node that owns the region its body
    /// repeats in.
    ///
    /// FIR supplies what the post-partial-evaluation producer has to
    /// reconstruct. There is no block graph to find a header in, no join to
    /// search for, and no back edge to exclude from a reachability query: a
    /// `while` is a condition and a body block, already nested. Every `for` and
    /// every `repeat` arrives here too, because `loop_unification` desugars
    /// both into a `while` on HIR before FIR is lowered.
    ///
    /// The trip condition never enters the graph. A theta's ports are
    /// capability-typed and carry qubits, so the condition is not a value the
    /// graph holds. It is still put to the effect-freedom judgement, because a
    /// condition holding a call would otherwise be walked past in silence.
    fn lower_loop(&mut self, package: PackageId, cond: ExprId, body: BlockId) -> Result<(), Error> {
        self.retain_evaluation(package, cond);
        if !is_effect_free(self.store, package, cond) {
            return Err(Error::UnsupportedLoop {
                reason: "its condition is not provably free of quantum effect".to_string(),
            });
        }

        // Every wire live at the loop is carried. A wire the body never touches
        // is handed through by a carry rather than left outside, which costs
        // one node and an adjacency across the loop, and keeps the rest of the
        // callable optimizable instead of declining all of it.
        let carried: Vec<Wire> = self.live.keys().copied().collect();

        // A port the body binds against must sit in the region the loop node is
        // in. For a nested loop that means taking each carried wire through the
        // enclosing region's interface first, because an entering port of the
        // region above is not something an edge may leave.
        let owed: Vec<Wire> = carried
            .iter()
            .copied()
            .filter(|wire| self.crossings.contains(wire))
            .collect();
        for wire in owed {
            self.carry(wire)?;
        }

        let position = self.next_position();
        let theta = self.node(THETA, Vec::new(), NodeOrigin::Instruction(position))?;

        let mut entering = Vec::with_capacity(carried.len());
        for wire in &carried {
            let port = self.port(theta, Direction::Input, PortKind::Capability, Type::Qubit)?;
            let source = self.live[wire];
            self.connect(source, port, EdgeKind::AffineCapability)?;
            entering.push(port);
        }
        let mut leaving = Vec::with_capacity(carried.len());
        for _ in &carried {
            leaving.push(self.port(theta, Direction::Output, PortKind::Capability, Type::Qubit)?);
        }

        self.lower_loop_body(package, body, theta, &carried, &entering, &leaving)?;

        for (index, wire) in carried.iter().enumerate() {
            self.live.insert(*wire, leaving[index]);
        }
        Ok(())
    }

    /// Opens the body's region, walks it, and restores the enclosing region's
    /// state on the error path as well as the ordinary one.
    fn lower_loop_body(
        &mut self,
        package: PackageId,
        body: BlockId,
        theta: NodeId,
        carried: &[Wire],
        entering: &[PortId],
        leaving: &[PortId],
    ) -> Result<(), Error> {
        let parent = self.region;
        let region = self.builder.add_region(Some(parent), RegionKind::Theta);
        // A body no node owns is a region nothing enters, which the reader
        // refuses by name.
        self.builder
            .declare_region_owner(region, theta)
            .map_err(build_error)?;
        self.region = region;
        let enclosing_crossings =
            std::mem::replace(&mut self.crossings, carried.iter().copied().collect());
        let enclosing_entry = std::mem::take(&mut self.carried_entry);
        let enclosing_live = self.live.clone();
        for (index, wire) in carried.iter().enumerate() {
            self.live.insert(*wire, entering[index]);
        }

        let lowered = self.close_loop_body(package, body, region, carried, leaving);

        if lowered.is_err() {
            self.live = enclosing_live;
        }
        self.carried_entry = enclosing_entry;
        self.crossings = enclosing_crossings;
        self.region = parent;
        lowered
    }

    fn close_loop_body(
        &mut self,
        package: PackageId,
        body: BlockId,
        region: RegionId,
        carried: &[Wire],
        leaving: &[PortId],
    ) -> Result<(), Error> {
        self.lower_block(package, body)?;
        for wire in carried {
            if self.crossings.contains(wire) {
                self.carry(*wire)?;
            }
        }
        for (index, wire) in carried.iter().enumerate() {
            let produced = self.live[wire];
            self.bind(region, leaving[index], produced, InterfaceRole::Result)?;
            let taken =
                self.carried_entry
                    .get(wire)
                    .copied()
                    .ok_or_else(|| Error::UnsupportedLoop {
                        reason: "its body never took a wire it carries".to_string(),
                    })?;
            // Declared on both ends. The wire leaving one iteration is the wire
            // entering the next, and an edge that says so without the region
            // saying so is an undeclared cycle, which is a different thing.
            self.builder
                .declare_exit(region, ExitKind::Backedge, produced)
                .map_err(build_error)?;
            self.connect(produced, taken, EdgeKind::AffineCapability)?;
        }
        Ok(())
    }

    /// Hands a wire through a body that never touched it.
    fn carry(&mut self, wire: Wire) -> Result<(), Error> {
        let position = self.next_position();
        let node = self.node(CARRY, Vec::new(), NodeOrigin::Instruction(position))?;
        let input = self.port(node, Direction::Input, PortKind::Capability, Type::Qubit)?;
        self.take(wire, input)?;
        let output = self.port(node, Direction::Output, PortKind::Capability, Type::Qubit)?;
        self.live.insert(wire, output);
        Ok(())
    }

    /// Consumes the capability standing for a wire, declaring the region
    /// crossing first when this is the body's first use of it.
    fn take(&mut self, wire: Wire, input: PortId) -> Result<(), Error> {
        let source = self.live.get(&wire).copied().ok_or(Error::UnknownQubit)?;
        if self.crossings.remove(&wire) {
            self.bind(self.region, source, input, InterfaceRole::Carried)?;
            // Only the first iteration takes the wire from outside, and the one
            // edge this port admits is the back edge that brings it back. The
            // binding is what says where it came from before that.
            self.carried_entry.insert(wire, input);
            return Ok(());
        }
        self.connect(source, input, EdgeKind::AffineCapability)
    }

    fn bind(
        &mut self,
        region: RegionId,
        outer: PortId,
        inner: PortId,
        role: InterfaceRole,
    ) -> Result<(), Error> {
        self.builder
            .bind_interface(region, InterfaceBinding { outer, inner, role })
            .map_err(build_error)
    }

    fn lower_call(&mut self, package: PackageId, callee: ExprId, arg: ExprId) -> Result<(), Error> {
        let (item, adjoint, controlled) = resolve_callee(self.store, package, callee)?;
        let target = target_of(self.store, item, adjoint, controlled)?;
        let name = target.name.clone();

        match target.block {
            None => self.lower_intrinsic(&name, package, target.package, target.input, arg),
            Some(block) => {
                if self.call_stack.contains(&item) {
                    return Err(Error::RecursiveCallable { name });
                }
                let frame = self.bind_arguments(target.package, target.input, package, arg)?;
                let outer = std::mem::replace(&mut self.frame, frame);
                self.call_stack.push(item);
                let lowered = self.lower_block(target.package, block);
                self.call_stack.pop();
                self.frame = outer;
                lowered
            }
        }
    }

    /// Binds a callee's parameters to the wires the caller passed.
    ///
    /// Substituting at the call site is what turns a parameter back into the
    /// caller's own operand, and is why a body reached from two call sites is
    /// walked twice rather than once.
    fn bind_arguments(
        &mut self,
        declared_in: PackageId,
        pat: PatId,
        called_from: PackageId,
        arg: ExprId,
    ) -> Result<BTreeMap<LocalVarId, Wire>, Error> {
        let mut frame = BTreeMap::new();
        self.bind_pattern(declared_in, pat, called_from, arg, &mut frame)?;
        Ok(frame)
    }

    fn bind_pattern(
        &mut self,
        declared_in: PackageId,
        pat: PatId,
        called_from: PackageId,
        arg: ExprId,
        frame: &mut BTreeMap<LocalVarId, Wire>,
    ) -> Result<(), Error> {
        let pattern = self.store.get(declared_in).get_pat(pat);
        let ty = pattern.ty.clone();
        match pattern.kind.clone() {
            PatKind::Tuple(items) => {
                let ExprKind::Tuple(args) = &self.store.get(called_from).get_expr(arg).kind else {
                    return Err(Error::ArgumentShape);
                };
                if args.len() != items.len() {
                    return Err(Error::ArgumentShape);
                }
                for (item, value) in items.iter().zip(args.clone()) {
                    self.bind_pattern(declared_in, *item, called_from, value, frame)?;
                }
                Ok(())
            }
            PatKind::Bind(ident) if is_qubit(&ty) => {
                let wire = self.wire_of(called_from, arg);
                if wire == Wire::Opaque {
                    self.census.opaque += 1;
                    return Err(Error::UnresolvedQubit);
                }
                frame.insert(ident.id, wire);
                Ok(())
            }
            // A classical argument holds nothing the graph carries, but the
            // expression that computes it may, so it is checked rather than
            // ignored.
            PatKind::Bind(_) | PatKind::Discard => self.lower_effect_expr(called_from, arg),
        }
    }

    /// What this producer knows about the wire one operand names.
    ///
    /// An `ExprKind::Index` reaches the last arm with everything else this
    /// producer did not walk. No alias analysis exists at this level anywhere
    /// in this tree, so two index expressions cannot be told apart and neither
    /// can be told apart from a name.
    fn wire_of(&self, package: PackageId, expr: ExprId) -> Wire {
        match &self.store.get(package).get_expr(expr).kind {
            ExprKind::Var(Res::Local(local), _) => {
                self.frame.get(local).copied().unwrap_or(Wire::Opaque)
            }
            _ => Wire::Opaque,
        }
    }

    fn count(&mut self, wire: Wire) {
        match wire {
            Wire::Resource(_) => self.census.resource += 1,
            Wire::Binding(_) => self.census.binding += 1,
            Wire::Opaque => self.census.opaque += 1,
        }
    }

    /// Lowers a call to an intrinsic, which is where a vocabulary name is
    /// emitted.
    fn lower_intrinsic(
        &mut self,
        name: &str,
        called_from: PackageId,
        declared_in: PackageId,
        input: PatId,
        arg: ExprId,
    ) -> Result<(), Error> {
        let operand_types = self.bindings_of(declared_in, input);
        let arguments = self.arguments_of(called_from, arg, operand_types.len())?;
        let wires = self.qubit_wires(called_from, &operand_types, &arguments)?;

        if name == QUBIT_RELEASE {
            let [wire] = wires[..] else {
                return Err(Error::UnresolvedQubit);
            };
            return self.release_wire(wire);
        }

        let operation = operation_name(name).ok_or_else(|| Error::UnsupportedCallable {
            name: name.to_string(),
        })?;
        let position = self.next_position();
        // A reset destroys information, so it is observable and the retention
        // rule must not treat it as removable.
        let observable = operation == RESET;
        let effects = if observable {
            wires
                .iter()
                .map(|wire| qubit_effect(EffectMode::Irreversible, true, self.ordinal_of(*wire)))
                .collect()
        } else {
            vec![effect(EffectMode::Write, false)]
        };
        let node = self.node(operation, effects, NodeOrigin::Instruction(position))?;
        self.thread(node, &wires)?;
        if observable {
            self.builder.demand(node);
        }
        Ok(())
    }

    /// The argument expressions one call passes, in parameter order.
    fn arguments_of(
        &self,
        package: PackageId,
        arg: ExprId,
        arity: usize,
    ) -> Result<Vec<ExprId>, Error> {
        if arity == 1 {
            return Ok(vec![arg]);
        }
        let ExprKind::Tuple(args) = &self.store.get(package).get_expr(arg).kind else {
            return Err(Error::ArgumentShape);
        };
        if args.len() != arity {
            return Err(Error::ArgumentShape);
        }
        Ok(args.clone())
    }

    /// The wires an intrinsic call names, in operand order, with the identity
    /// question answered through the shared abstraction.
    ///
    /// One qubit may be a binding, because comparing a binding to itself is
    /// what the variant admits. Two or more must every one be an allocation
    /// this body made: `resolved` is all or nothing, and it succeeds only when
    /// distinctness is a fact rather than the absence of a proof.
    fn qubit_wires(
        &mut self,
        package: PackageId,
        parameters: &[(LocalVarId, Ty)],
        arguments: &[ExprId],
    ) -> Result<Vec<Wire>, Error> {
        let mut wires = Vec::new();
        for ((_, ty), arg) in parameters.iter().zip(arguments) {
            if !is_qubit(ty) {
                continue;
            }
            let wire = self.wire_of(package, *arg);
            self.count(wire);
            wires.push(wire);
        }
        match wires.len() {
            1 if wires[0] == Wire::Opaque => Err(Error::UnresolvedQubit),
            0 | 1 => Ok(wires),
            count => {
                let resolved = OperandWires::new(wires.iter().copied())
                    .resolved()
                    .ok_or(Error::AmbiguousIdentity { count })?;
                if resolved.len() != wires.len() {
                    return Err(Error::RepeatedQubit);
                }
                Ok(wires)
            }
        }
    }

    /// Consumes and replaces the capability for each wire an operation names.
    fn thread(&mut self, node: NodeId, wires: &[Wire]) -> Result<(), Error> {
        for wire in wires {
            let input = self.port(node, Direction::Input, PortKind::Capability, Type::Qubit)?;
            self.take(*wire, input)?;
            let output = self.port(node, Direction::Output, PortKind::Capability, Type::Qubit)?;
            self.live.insert(*wire, output);
        }
        Ok(())
    }

    fn ordinal_of(&self, wire: Wire) -> u32 {
        self.ordinal.get(&wire).copied().unwrap_or_default()
    }

    fn next_position(&mut self) -> u32 {
        let position = self.position;
        self.position += 1;
        position
    }

    fn node(
        &mut self,
        operation: impl Into<String>,
        effects: Vec<EffectEvent>,
        origin: NodeOrigin,
    ) -> Result<NodeId, Error> {
        let node = self
            .builder
            .add_node(self.region, operation, effects)
            .map_err(build_error)?;
        self.builder
            .declare_payload(
                node,
                NodePayload {
                    value: None,
                    origin: Some(origin),
                },
            )
            .map_err(build_error)?;
        if let Some(site) = self.site {
            self.sites.insert(node, site);
        }
        Ok(node)
    }

    fn port(
        &mut self,
        node: NodeId,
        direction: Direction,
        kind: PortKind,
        ty: Type,
    ) -> Result<PortId, Error> {
        let usage = match kind {
            PortKind::Capability => Usage::Linear,
            PortKind::Value => Usage::Copyable,
            _ => Usage::Affine,
        };
        self.builder
            .add_port(node, direction, kind, ty, Stage::Residual, usage)
            .map_err(build_error)
    }

    fn connect(&mut self, source: PortId, target: PortId, kind: EdgeKind) -> Result<(), Error> {
        self.builder
            .connect(qsc_qhdg::Edge {
                source,
                target,
                kind,
            })
            .map_err(build_error)
    }
}

fn build_error(error: impl std::fmt::Debug) -> Error {
    Error::Build {
        reason: format!("{error:?}"),
    }
}

/// Resolves a callee through adjoint and controlled functor wrappers.
///
/// A resolved callable does not carry the functor its call site applied, so
/// the wrappers are unwrapped here and reported: an adjoint selects a
/// different specialization and a controlled one is refused, rather than
/// either quietly becoming the callable it wraps.
///
/// Free rather than a method so the reach survey asks this question through the
/// same code the producer asks it through.
fn resolve_callee(
    store: &PackageStore,
    package: PackageId,
    callee: ExprId,
) -> Result<(StoreItemId, bool, usize), Error> {
    let mut current = callee;
    let mut adjoint = false;
    let mut controlled = 0usize;
    loop {
        match &store.get(package).get_expr(current).kind {
            ExprKind::Var(Res::Item(item), _) => {
                return Ok((
                    StoreItemId {
                        package: item.package,
                        item: item.item,
                    },
                    adjoint,
                    controlled,
                ));
            }
            ExprKind::UnOp(UnOp::Functor(Functor::Adj), inner) => {
                // Two adjoints are the callable itself.
                adjoint = !adjoint;
                current = *inner;
            }
            ExprKind::UnOp(UnOp::Functor(Functor::Ctl), inner) => {
                controlled += 1;
                current = *inner;
            }
            _ => return Err(Error::UnresolvedCallee),
        }
    }
}

/// The specialization a resolved call reaches.
fn target_of(
    store: &PackageStore,
    item: StoreItemId,
    adjoint: bool,
    controlled: usize,
) -> Result<Target, Error> {
    let Some(Global::Callable(decl)) = store.get_global(item) else {
        return Err(Error::NotACallable {
            package: item.package.to_string(),
            item: item.item.to_string(),
        });
    };
    let name = decl.name.name.to_string();
    if controlled > 0 {
        return Err(Error::ControlledCall { name });
    }
    // A returned value would have to be threaded to whatever reads it, and
    // the only thing this graph holds is a capability.
    if !is_unit(&decl.output) {
        return Err(Error::ValuedCall { name });
    }
    let block = match &decl.implementation {
        CallableImpl::Intrinsic => {
            if adjoint {
                // An intrinsic has no adjoint specialization to select, so
                // the functor would be dropped rather than applied.
                return Err(Error::UnsupportedCallable { name });
            }
            None
        }
        CallableImpl::Spec(spec) => {
            let selected = if adjoint {
                spec.adj
                    .as_ref()
                    .ok_or(Error::UnsupportedCallable { name: name.clone() })?
            } else {
                &spec.body
            };
            Some(selected.block)
        }
        // Its simulation override is not the operation the target runs.
        CallableImpl::SimulatableIntrinsic(_) => {
            return Err(Error::UnsupportedCallable { name });
        }
    };
    Ok(Target {
        name,
        package: item.package,
        input: decl.input,
        block,
    })
}

/// The effect-expression forms this producer models.
///
/// One definition, consulted by `lower_effect_expr` when it lowers and by the
/// reach survey when it counts. A form that is not here is refused by name, and
/// adding one to this list without giving it a lowering is how a form would
/// start being skipped silently.
fn models_effect_expr(kind: &ExprKind) -> bool {
    match kind {
        ExprKind::Call(..) | ExprKind::Lit(_) | ExprKind::Var(..) | ExprKind::Hole => true,
        ExprKind::Tuple(items) => items.is_empty(),
        _ => false,
    }
}

/// Whether evaluating an expression can reach no quantum operation.
///
/// Sound in one direction only, and that direction is chosen. A `true` is a
/// proof that the expression allocates nothing, measures nothing, and applies
/// no gate. A `false` is either the opposite proof or an admission that this
/// judgement could not tell, and the two are deliberately one answer, because
/// both must refuse. A wrong `true` would let the producer drop a gate from the
/// graph and leave it in the FIR, which is the silent-loss failure the whole
/// refusal posture exists to prevent. A wrong `false` costs only reach.
///
/// Clearing is not receiving. An expression this judgement clears contributes
/// no node to the graph and the graph never holds its value; a call whose value
/// the graph would need is refused rather than cleared.
struct EffectFreedom<'a> {
    store: &'a PackageStore,
    /// Callables currently being proved, innermost last, so a body that reaches
    /// itself answers "cannot prove" rather than not answering at all.
    proving: Vec<StoreItemId>,
    require_total: bool,
}

impl<'a> EffectFreedom<'a> {
    fn new(store: &'a PackageStore) -> Self {
        Self {
            store,
            proving: Vec::new(),
            require_total: false,
        }
    }

    fn expr(&mut self, package: PackageId, expr: ExprId) -> bool {
        match self.store.get(package).get_expr(expr).kind.clone() {
            ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(..) => true,
            ExprKind::Array(items) | ExprKind::ArrayLit(items) | ExprKind::Tuple(items) => {
                items.into_iter().all(|item| self.expr(package, item))
            }
            // No quantum effect does not make evaluation removable. These
            // forms can fail even when their children are effect-free.
            ExprKind::ArrayRepeat(..)
            | ExprKind::Index(..)
            | ExprKind::AssignIndex(..)
            | ExprKind::UpdateIndex(..)
            | ExprKind::BinOp(BinOp::Div | BinOp::Mod | BinOp::Exp | BinOp::Shl | BinOp::Shr, ..)
            | ExprKind::AssignOp(
                BinOp::Div | BinOp::Mod | BinOp::Exp | BinOp::Shl | BinOp::Shr,
                ..,
            ) if self.require_total => false,
            ExprKind::ArrayRepeat(left, right)
            | ExprKind::Index(left, right)
            | ExprKind::Assign(left, right)
            | ExprKind::AssignOp(_, left, right)
            | ExprKind::AssignField(left, _, right)
            | ExprKind::BinOp(_, left, right)
            | ExprKind::UpdateField(left, _, right) => {
                self.expr(package, left) && self.expr(package, right)
            }
            ExprKind::AssignIndex(first, second, third)
            | ExprKind::UpdateIndex(first, second, third) => {
                self.expr(package, first) && self.expr(package, second) && self.expr(package, third)
            }
            ExprKind::Field(inner, _) | ExprKind::UnOp(_, inner) => self.expr(package, inner),
            ExprKind::Range(start, step, end) => [start, step, end]
                .into_iter()
                .flatten()
                .all(|part| self.expr(package, part)),
            ExprKind::String(components) => components.iter().all(|component| match component {
                StringComponent::Expr(inner) => self.expr(package, *inner),
                StringComponent::Lit(_) => true,
            }),
            ExprKind::Struct(_, copy, fields) => {
                copy.is_none_or(|copy| self.expr(package, copy))
                    && fields.iter().all(|field| self.expr(package, field.value))
            }
            ExprKind::Block(block) => self.block(package, block),
            ExprKind::Call(callee, arg) => self.call(package, callee, arg),
            // Control flow and divergence are not classical content this
            // judgement clears, whatever they hold. `If` and `While` belong to
            // the producer's own lowering; `Fail` and `Return` leave the
            // straight line the graph models, and clearing one would let a
            // rewrite reorder across a point the program may never pass.
            ExprKind::Closure(..)
            | ExprKind::Fail(..)
            | ExprKind::If(..)
            | ExprKind::Parallel(..)
            | ExprKind::Return(..)
            | ExprKind::While(..) => false,
        }
    }

    fn block(&mut self, package: PackageId, block: BlockId) -> bool {
        let stmts = self.store.get(package).get_block(block).stmts.clone();
        stmts.into_iter().all(|stmt| {
            match self.store.get(package).get_stmt(stmt).kind {
                // A nested item declaration evaluates nothing until it is
                // called, and the call is what this judgement would then see.
                StmtKind::Item(_) => true,
                StmtKind::Expr(expr) | StmtKind::Semi(expr) | StmtKind::Local(_, _, expr) => {
                    self.expr(package, expr)
                }
            }
        })
    }

    fn call(&mut self, package: PackageId, callee: ExprId, arg: ExprId) -> bool {
        if !self.expr(package, arg) {
            return false;
        }
        let Ok((item, adjoint, controlled)) = resolve_callee(self.store, package, callee) else {
            return false;
        };
        if controlled > 0 {
            return false;
        }
        if self.proving.contains(&item) {
            return false;
        }
        let Some(Global::Callable(decl)) = self.store.get_global(item) else {
            return false;
        };
        match &decl.implementation {
            // No body to walk, so the declaration is the only evidence there
            // is, and the declaration says less than it looks like it does. Q#
            // forbids a function from calling an operation or allocating a
            // qubit, which rules out applying a gate; it does not rule out
            // reading quantum state. `DumpMachine`, `DumpRegister`, and
            // `DumpMatrix` are declared functions that observe, and clearing
            // one lets a rewrite cancel a pair straddling the point the program
            // is observed at. So an intrinsic clears only when it is named as
            // classical, and an intrinsic operation never clears, which is what
            // keeps `use q = Qubit[n]` refused.
            CallableImpl::Intrinsic => {
                decl.kind == CallableKind::Function
                    && !adjoint
                    && classical_intrinsic(&decl.name.name)
                    && (!self.require_total || &*decl.name.name != "DoubleAsStringWithPrecision")
            }
            // Its simulation override is not the operation the target runs, so
            // there is nothing here to prove anything about.
            CallableImpl::SimulatableIntrinsic(_) => false,
            CallableImpl::Spec(spec) => {
                let selected = if adjoint {
                    spec.adj.as_ref()
                } else {
                    Some(&spec.body)
                };
                let Some(selected) = selected else {
                    return false;
                };
                let block = selected.block;
                self.proving.push(item);
                let cleared = self.block(item.package, block);
                self.proving.pop();
                cleared
            }
        }
    }
}

/// Whether evaluating `expr` can reach no quantum operation. This does not
/// establish totality; whole-call retirement separately retains evaluation.
///
/// See [`EffectFreedom`] for which direction this is sound in and why.
fn is_effect_free(store: &PackageStore, package: PackageId, expr: ExprId) -> bool {
    EffectFreedom::new(store).expr(package, expr)
}

/// Whether an intrinsic function computes rather than observes.
///
/// Named rather than inferred, on the same posture as [`operation_name`]: an
/// intrinsic added to the library is refused until it is proved classical.
/// Every name here is pure computation over its arguments. The intrinsic
/// functions deliberately absent read or write state a rewrite would reorder
/// across, and each would be a wrong `true`: the three `Dump` functions observe
/// quantum state, `Message` writes output, `ConfigurePauliNoise` and
/// `ConfigureQubitLoss` change how later operations behave, the estimation and
/// configuration intrinsics read runtime state, and `__quantum__rt__read_loss`
/// reads a measurement outcome's loss.
fn classical_intrinsic(name: &str) -> bool {
    matches!(
        name,
        "ArcCos"
            | "ArcSin"
            | "ArcTan"
            | "ArcTan2"
            | "Cos"
            | "Cosh"
            | "DoubleAsStringWithPrecision"
            | "IntAsBigInt"
            | "IntAsDouble"
            | "Length"
            | "Log"
            | "Sin"
            | "Sinh"
            | "Sqrt"
            | "Tan"
            | "Tanh"
            | "Truncate"
    )
}

const fn effect(mode: EffectMode, observable: bool) -> EffectEvent {
    EffectEvent {
        domain: EffectDomain::Quantum,
        mode,
        resource: EffectResource::Unknown,
        observable,
    }
}

fn qubit_effect(mode: EffectMode, observable: bool, qubit: u32) -> EffectEvent {
    EffectEvent {
        domain: EffectDomain::Quantum,
        mode,
        resource: EffectResource::Named(format!("qubit:{qubit}")),
        observable,
    }
}

fn is_qubit(ty: &Ty) -> bool {
    matches!(ty, Ty::Prim(Prim::Qubit))
}

fn is_unit(ty: &Ty) -> bool {
    matches!(ty, Ty::Tuple(items) if items.is_empty())
}

/// The name of an expression form, for a refusal that says what it refused.
fn expr_kind_name(kind: &ExprKind) -> &'static str {
    match kind {
        ExprKind::Array(..) | ExprKind::ArrayLit(..) => "array",
        ExprKind::ArrayRepeat(..) => "array repeat",
        ExprKind::Assign(..)
        | ExprKind::AssignOp(..)
        | ExprKind::AssignField(..)
        | ExprKind::AssignIndex(..) => "assignment",
        ExprKind::BinOp(..) => "binary operator",
        ExprKind::Block(..) => "block",
        ExprKind::Call(..) => "call",
        ExprKind::Closure(..) => "closure",
        ExprKind::Fail(..) => "fail",
        ExprKind::Field(..) => "field access",
        ExprKind::Hole => "hole",
        ExprKind::If(..) => "conditional",
        ExprKind::Index(..) => "index",
        ExprKind::Parallel(..) => "parallel",
        ExprKind::Lit(..) => "literal",
        ExprKind::Range(..) => "range",
        ExprKind::Return(..) => "return",
        ExprKind::Struct(..) => "struct",
        ExprKind::String(..) => "string",
        ExprKind::UpdateIndex(..) => "index update",
        ExprKind::Tuple(..) => "tuple",
        ExprKind::UnOp(..) => "unary operator",
        ExprKind::UpdateField(..) => "field update",
        ExprKind::Var(..) => "variable",
        ExprKind::While(..) => "loop",
    }
}

/// The runtime intrinsic that mints a qubit.
const QUBIT_ALLOCATE: &str = "__quantum__rt__qubit_allocate";

/// The runtime intrinsic that gives one back.
const QUBIT_RELEASE: &str = "__quantum__rt__qubit_release";

/// The vocabulary name for one intrinsic, from the one place that defines the
/// names.
///
/// Exhaustive over the intrinsics this producer accepts rather than defaulting,
/// so an intrinsic added to the library has to be given a graph name
/// deliberately instead of arriving as something it is not.
///
/// Three FIR-visible intrinsics are deliberately absent. `__quantum__qis__m__body`
/// and `__quantum__qis__mresetz__body` return a `Result` that is bound to a
/// local here rather than written into a declared register, and the vocabulary's
/// store name carries a register index this producer would have to invent.
/// `__quantum__qis__barrier__body` names no qubit, so it would enter the graph
/// holding no capability and ordering nothing.
fn operation_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "__quantum__qis__ccx__body" => "quantum.ccx",
        "__quantum__qis__cx__body" => "quantum.cx",
        "__quantum__qis__cy__body" => "quantum.cy",
        "__quantum__qis__cz__body" => "quantum.cz",
        "__quantum__qis__h__body" => "quantum.h",
        "__quantum__qis__rx__body" => "quantum.rx",
        "__quantum__qis__rxx__body" => "quantum.rxx",
        "__quantum__qis__ry__body" => "quantum.ry",
        "__quantum__qis__ryy__body" => "quantum.ryy",
        "__quantum__qis__rz__body" => "quantum.rz",
        "__quantum__qis__rzz__body" => "quantum.rzz",
        "__quantum__qis__s__adj" => "quantum.s_adj",
        "__quantum__qis__s__body" => "quantum.s",
        "__quantum__qis__swap__body" => "quantum.swap",
        "__quantum__qis__sx__body" => "quantum.sx",
        "__quantum__qis__t__adj" => "quantum.t_adj",
        "__quantum__qis__t__body" => "quantum.t",
        "__quantum__qis__x__body" => "quantum.x",
        "__quantum__qis__y__body" => "quantum.y",
        "__quantum__qis__z__body" => "quantum.z",
        "__quantum__qis__reset__body" => RESET,
        QUBIT_ALLOCATE => ALLOCATE,
        QUBIT_RELEASE => RELEASE,
        _ => return None,
    })
}

/// Every operation name this producer can emit.
///
/// Declared so the shared vocabulary can be asked to accept it, which is the
/// check that this producer added no name of its own. The two structural names
/// are here for the same reason the gate names are: a theta and the carry that
/// hands a wire through one are operations a reader has to recognize, and
/// neither was invented here.
pub fn emitted_names() -> impl Iterator<Item = &'static str> {
    const INTRINSICS: [&str; 23] = [
        "__quantum__qis__ccx__body",
        "__quantum__qis__cx__body",
        "__quantum__qis__cy__body",
        "__quantum__qis__cz__body",
        "__quantum__qis__h__body",
        "__quantum__qis__rx__body",
        "__quantum__qis__rxx__body",
        "__quantum__qis__ry__body",
        "__quantum__qis__ryy__body",
        "__quantum__qis__rz__body",
        "__quantum__qis__rzz__body",
        "__quantum__qis__s__adj",
        "__quantum__qis__s__body",
        "__quantum__qis__swap__body",
        "__quantum__qis__sx__body",
        "__quantum__qis__t__adj",
        "__quantum__qis__t__body",
        "__quantum__qis__x__body",
        "__quantum__qis__y__body",
        "__quantum__qis__z__body",
        "__quantum__qis__reset__body",
        QUBIT_ALLOCATE,
        QUBIT_RELEASE,
    ];
    INTRINSICS
        .into_iter()
        .filter_map(operation_name)
        .chain([THETA, CARRY])
}
