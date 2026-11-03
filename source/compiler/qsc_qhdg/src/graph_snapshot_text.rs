// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Typed S-expression grammar over the same domain records as the binary codec.

use crate::{
    Angle, AngleOp, Authority, Borrow, BorrowKind, CleanupState, Direction, Edge, EdgeKind,
    EffectDomain, EffectEvent, EffectMode, EffectResource, Epoch, ExitKind, FirOriginKind,
    InterfaceBinding, InterfaceRole, NodeId, NodeOrigin, NodePayload, NodeValue, OperationIdentity,
    OwnerPath, OwnershipState, PathSelector, PortId, PortKind, Real, Region, RegionId,
    RegionInterface, RegionKind, ResourceId, StableNodeId, Stage, SymbolId, Type, Usage,
    graph::{Graph, Node, Port},
    graph_snapshot::{Payload, SnapshotError, check_metadata, hex, unique_map, unique_set},
    identity::NodeIdentity,
};
use qsc_snapshot::{
    Budget, Error, Kind, depth,
    text::{self, Record, Value, Writer},
};
use std::{collections::BTreeSet, sync::Arc};

trait Text: Sized {
    fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error>;
    fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error>;
}
fn field<T: Text>(out: &mut Writer, name: &str, value: &T, nesting: usize) -> Result<(), Error> {
    out.field(name, |out| value.write(out, nesting))
}
fn read<T: Text>(
    record: &mut Record<'_>,
    name: &str,
    budget: &mut Budget,
    nesting: usize,
) -> Result<T, Error> {
    T::read(record.field(name)?, budget, nesting)
}
fn single<'a>(name: &str, values: &[&'a Value]) -> Result<&'a Value, Error> {
    if let [value] = values {
        Ok(value)
    } else {
        Err(Error::malformed(format!("{name} expects one value")))
    }
}
fn unit(values: &[&Value]) -> Result<(), Error> {
    if values.is_empty() {
        Ok(())
    } else {
        Err(Error::malformed("unit variant has a value"))
    }
}
macro_rules! atom {
    ($ty:ty, $parse:ident) => {
        impl Text for $ty {
            fn write(&self, out: &mut Writer, _: usize) -> Result<(), Error> {
                out.atom(self);
                Ok(())
            }
            fn read(value: &Value, _: &mut Budget, _: usize) -> Result<Self, Error> {
                text::$parse(value)
            }
        }
    };
}
atom!(u32, u32);
atom!(u64, u64);
atom!(i64, i64);
impl Text for bool {
    fn write(&self, out: &mut Writer, _: usize) -> Result<(), Error> {
        out.atom(if *self { "true" } else { "false" });
        Ok(())
    }
    fn read(value: &Value, _: &mut Budget, _: usize) -> Result<Self, Error> {
        text::boolean(value)
    }
}
impl Text for usize {
    fn write(&self, out: &mut Writer, _: usize) -> Result<(), Error> {
        out.atom(u64::try_from(*self).map_err(Error::malformed)?);
        Ok(())
    }
    fn read(value: &Value, _: &mut Budget, _: usize) -> Result<Self, Error> {
        usize::try_from(text::u64(value)?).map_err(Error::malformed)
    }
}
impl Text for String {
    fn write(&self, out: &mut Writer, _: usize) -> Result<(), Error> {
        out.string(self)
    }
    fn read(value: &Value, _: &mut Budget, _: usize) -> Result<Self, Error> {
        text::string(value)
    }
}
impl Text for Real {
    fn write(&self, out: &mut Writer, _: usize) -> Result<(), Error> {
        out.real(self.get().to_bits())
    }
    fn read(value: &Value, _: &mut Budget, _: usize) -> Result<Self, Error> {
        Ok(Self::new(f64::from_bits(text::real(value)?)))
    }
}
impl<T: Text> Text for Vec<T> {
    fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error> {
        qsc_snapshot::length(self.len())?;
        out.open("list")?;
        for item in self {
            item.write(out, nesting)?;
        }
        out.close()
    }
    fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error> {
        text::sequence(value, budget)?
            .into_iter()
            .map(|value| T::read(value, budget, nesting))
            .collect()
    }
}
impl<T: Text + Ord> Text for BTreeSet<T> {
    fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error> {
        qsc_snapshot::length(self.len())?;
        out.open("list")?;
        for item in self {
            item.write(out, nesting)?;
        }
        out.close()
    }
    fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error> {
        unique_set(Vec::<T>::read(value, budget, nesting)?)
    }
}
impl<T: Text> Text for Option<T> {
    fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error> {
        match self {
            Some(value) => {
                out.open("some")?;
                value.write(out, nesting)?;
            }
            None => out.open("none")?,
        }
        out.close()
    }
    fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error> {
        let (tag, values) = text::variant(value)?;
        match tag {
            "none" => {
                unit(&values)?;
                Ok(None)
            }
            "some" => Ok(Some(T::read(single(tag, &values)?, budget, nesting)?)),
            _ => Err(Error::malformed("invalid optional value")),
        }
    }
}
macro_rules! wrapper {
    ($ty:ident,$inner:ty) => {
        impl Text for $ty {
            fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error> {
                self.0.write(out, nesting)
            }
            fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error> {
                Ok(Self(<$inner>::read(value, budget, nesting)?))
            }
        }
    };
}
wrapper!(NodeId, u32);
wrapper!(RegionId, u32);
wrapper!(PortId, u32);
wrapper!(StableNodeId, u64);
wrapper!(ResourceId, u64);
wrapper!(Epoch, u64);
wrapper!(OperationIdentity, String);
macro_rules! enumeration {
    ($ty:ident, {$($variant:ident=>$name:literal),+ $(,)?})=>{
        impl Text for $ty {
            fn write(&self,out:&mut Writer,_:usize)->Result<(),Error>{out.atom(match self {$($ty::$variant=>$name),+});Ok(())}
            fn read(value:&Value,_:&mut Budget,_:usize)->Result<Self,Error>{match text::symbol(value)? {$($name=>Ok($ty::$variant),)+_=>Err(Error::malformed(concat!("invalid ",stringify!($ty))))}}
        }
    };
}
enumeration!(RegionKind,{ModuleOmega=>"moduleOmega",Lambda=>"lambda",Gamma=>"gamma",Theta=>"theta",Recursion=>"recursion",Handler=>"handler",Stage=>"stage",ResidualCfg=>"residualCfg"});
enumeration!(ExitKind,{Return=>"return",Abort=>"abort",Branch=>"branch",Backedge=>"backedge"});
enumeration!(InterfaceRole,{Argument=>"argument",Result=>"result",Exit=>"exit",Capture=>"capture",Carried=>"carried"});
enumeration!(Direction,{Input=>"input",Output=>"output"});
enumeration!(PortKind,{Value=>"value",Capability=>"capability",HardEffect=>"hardEffect",ControlExit=>"controlExit"});
enumeration!(Stage,{Generation=>"generation",Residual=>"residual"});
enumeration!(Usage,{Copyable=>"copyable",Affine=>"affine",Linear=>"linear"});
enumeration!(EdgeKind,{CopyableValue=>"copyableValue",AffineCapability=>"affineCapability",HardEffect=>"hardEffect",ControlExit=>"controlExit"});
enumeration!(EffectDomain,{Memory=>"memory",Quantum=>"quantum",Result=>"result",Allocation=>"allocation",Output=>"output",Abort=>"abort",Host=>"host",Noise=>"noise",Unknown=>"unknown"});
enumeration!(EffectMode,{Read=>"read",Write=>"write",Irreversible=>"irreversible",Allocate=>"allocate",Release=>"release",Emit=>"emit",Abort=>"abort",Unknown=>"unknown"});
enumeration!(AngleOp,{Add=>"add",Sub=>"sub",Mul=>"mul",Div=>"div"});
enumeration!(FirOriginKind,{Package=>"package",Item=>"item",Block=>"block",Expression=>"expression",Pattern=>"pattern",Statement=>"statement"});
enumeration!(BorrowKind,{Shared=>"shared",Exclusive=>"exclusive"});
macro_rules! record {
    ($ty:ident,$name:literal, {$($field:ident=>$wire:literal),+ $(,)?})=>{
        impl Text for $ty {
            fn write(&self,out:&mut Writer,nesting:usize)->Result<(),Error>{
                out.open($name)?;$(field(out,$wire,&self.$field,nesting)?;)+out.close()
            }
            fn read(value:&Value,budget:&mut Budget,nesting:usize)->Result<Self,Error>{
                let mut record=Record::new(value,$name)?;
                let result=Self {$($field:read(&mut record,$wire,budget,nesting)?),+};record.finish()?;Ok(result)
            }
        }
    };
}
record!(Region,"region",{id=>"id",parent=>"parent",kind=>"kind",owner=>"owner",interface=>"interface"});
record!(RegionInterface,"regionInterface",{arguments=>"arguments",results=>"results",exits=>"exits",bindings=>"bindings"});
record!(InterfaceBinding,"interfaceBinding",{outer=>"outer",inner=>"inner",role=>"role"});
record!(Node,"node",{id=>"id",region=>"region",operation=>"operation",inputs=>"inputs",outputs=>"outputs",effects=>"effects",symbol=>"symbol",payload=>"payload"});
record!(SymbolId,"symbol",{package=>"package",item=>"item"});
record!(Port,"port",{id=>"id",node=>"node",region=>"region",direction=>"direction",kind=>"kind",ty=>"type",stage=>"stage",usage=>"usage",resource=>"resource"});
record!(Edge,"edge",{source=>"source",target=>"target",kind=>"kind"});
record!(EffectEvent,"effect",{domain=>"domain",mode=>"mode",resource=>"resource",observable=>"observable"});
record!(OwnerPath,"ownerPath",{root=>"root",selectors=>"selectors"});
record!(Authority,"authority",{path=>"path",resources=>"resources",epoch=>"epoch",cleanup=>"cleanup"});
record!(Borrow,"borrow",{id=>"id",kind=>"kind",path=>"path",resources=>"resources",epoch=>"epoch"});
impl Text for (ExitKind, PortId) {
    fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error> {
        out.open("regionExit")?;
        field(out, "kind", &self.0, nesting)?;
        field(out, "port", &self.1, nesting)?;
        out.close()
    }
    fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error> {
        let mut r = Record::new(value, "regionExit")?;
        let result = (
            read(&mut r, "kind", budget, nesting)?,
            read(&mut r, "port", budget, nesting)?,
        );
        r.finish()?;
        Ok(result)
    }
}
impl Text for EffectResource {
    fn write(&self, out: &mut Writer, _: usize) -> Result<(), Error> {
        match self {
            Self::Named(name) => {
                out.open("named")?;
                out.string(name)?;
            }
            Self::Unknown => out.open("unknown")?,
        }
        out.close()
    }
    fn read(value: &Value, _: &mut Budget, _: usize) -> Result<Self, Error> {
        let (tag, values) = text::variant(value)?;
        match tag {
            "named" => Ok(Self::Named(text::string(single(tag, &values)?)?)),
            "unknown" => {
                unit(&values)?;
                Ok(Self::Unknown)
            }
            _ => Err(Error::malformed("invalid effect resource")),
        }
    }
}
impl Text for Type {
    fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error> {
        depth(nesting)?;
        match self {
            Self::Unit => out.open("unit")?,
            Self::Bool => out.open("boolean")?,
            Self::Int => out.open("integer")?,
            Self::Double => out.open("double")?,
            Self::String => out.open("string")?,
            Self::Result => out.open("result")?,
            Self::Qubit => out.open("qubit")?,
            Self::Unknown => out.open("unknown")?,
            Self::Tuple(items) => {
                out.open("tuple")?;
                items.write(out, nesting + 1)?;
            }
            Self::Array(item) => {
                out.open("array")?;
                item.write(out, nesting + 1)?;
            }
            Self::Location(item) => {
                out.open("location")?;
                item.write(out, nesting + 1)?;
            }
        }
        out.close()
    }
    fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error> {
        depth(nesting)?;
        let (tag, values) = text::variant(value)?;
        match tag {
            "tuple" => Ok(Self::Tuple(Vec::<Type>::read(
                single(tag, &values)?,
                budget,
                nesting + 1,
            )?)),
            "array" | "location" => {
                budget.charge(1)?;
                let inner = Box::new(Self::read(single(tag, &values)?, budget, nesting + 1)?);
                Ok(if tag == "array" {
                    Self::Array(inner)
                } else {
                    Self::Location(inner)
                })
            }
            _ => {
                unit(&values)?;
                match tag {
                    "unit" => Ok(Self::Unit),
                    "boolean" => Ok(Self::Bool),
                    "integer" => Ok(Self::Int),
                    "double" => Ok(Self::Double),
                    "string" => Ok(Self::String),
                    "result" => Ok(Self::Result),
                    "qubit" => Ok(Self::Qubit),
                    "unknown" => Ok(Self::Unknown),
                    _ => Err(Error::malformed("invalid type")),
                }
            }
        }
    }
}
impl Text for Angle {
    fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error> {
        depth(nesting)?;
        match self {
            Self::Literal { value } => {
                out.open("literal")?;
                value.write(out, nesting)?;
            }
            Self::Parameter { name } => {
                out.open("parameter")?;
                out.string(name)?;
            }
            Self::Neg { operand } => {
                out.open("neg")?;
                operand.write(out, nesting + 1)?;
            }
            Self::Binary { op, left, right } => {
                out.open("binary")?;
                out.open("binaryGraphAngle")?;
                field(out, "op", op, nesting)?;
                out.field("left", |out| left.write(out, nesting + 1))?;
                out.field("right", |out| right.write(out, nesting + 1))?;
                out.close()?;
            }
        }
        out.close()
    }
    fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error> {
        depth(nesting)?;
        budget.charge(1)?;
        let (tag, values) = text::variant(value)?;
        let value = single(tag, &values)?;
        match tag {
            "literal" => Ok(Self::Literal {
                value: Real::read(value, budget, nesting)?,
            }),
            "parameter" => Ok(Self::Parameter {
                name: text::string(value)?,
            }),
            "neg" => Ok(Self::Neg {
                operand: Box::new(Self::read(value, budget, nesting + 1)?),
            }),
            "binary" => {
                let mut r = Record::new(value, "binaryGraphAngle")?;
                let result = Self::Binary {
                    op: read(&mut r, "op", budget, nesting)?,
                    left: Box::new(read(&mut r, "left", budget, nesting + 1)?),
                    right: Box::new(read(&mut r, "right", budget, nesting + 1)?),
                };
                r.finish()?;
                Ok(result)
            }
            _ => Err(Error::malformed("invalid angle")),
        }
    }
}
impl Text for NodeValue {
    fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error> {
        match self {
            Self::Number(value) => {
                out.open("number")?;
                value.write(out, nesting)?;
            }
            Self::ClassicalBit(value) => {
                out.open("classicalBit")?;
                out.atom(value);
            }
            Self::Angle(value) => {
                out.open("angle")?;
                value.write(out, nesting)?;
            }
        }
        out.close()
    }
    fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error> {
        let (tag, values) = text::variant(value)?;
        let value = single(tag, &values)?;
        match tag {
            "number" => Ok(Self::Number(Real::read(value, budget, nesting)?)),
            "classicalBit" => Ok(Self::ClassicalBit(text::u32(value)?)),
            "angle" => Ok(Self::Angle(Angle::read(value, budget, nesting)?)),
            _ => Err(Error::malformed("invalid node value")),
        }
    }
}
impl Text for NodeOrigin {
    fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error> {
        match self {
            Self::Instruction(id) => {
                out.open("instruction")?;
                out.atom(id);
            }
            Self::Qubit(id) => {
                out.open("qubit")?;
                out.atom(id);
            }
            Self::Clbit(id) => {
                out.open("clbit")?;
                out.atom(id);
            }
            Self::GlobalPhase => out.open("globalPhase")?,
            Self::Fir {
                package,
                kind,
                local_id,
            } => {
                out.open("fir")?;
                out.open("firOrigin")?;
                field(out, "package", package, nesting)?;
                field(out, "kind", kind, nesting)?;
                field(out, "localId", local_id, nesting)?;
                out.close()?;
            }
        }
        out.close()
    }
    fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error> {
        let (tag, values) = text::variant(value)?;
        if tag == "globalPhase" {
            unit(&values)?;
            return Ok(Self::GlobalPhase);
        }
        let value = single(tag, &values)?;
        match tag {
            "instruction" => Ok(Self::Instruction(text::u32(value)?)),
            "qubit" => Ok(Self::Qubit(text::u32(value)?)),
            "clbit" => Ok(Self::Clbit(text::u32(value)?)),
            "fir" => {
                let mut r = Record::new(value, "firOrigin")?;
                let result = Self::Fir {
                    package: read(&mut r, "package", budget, nesting)?,
                    kind: read(&mut r, "kind", budget, nesting)?,
                    local_id: read(&mut r, "localId", budget, nesting)?,
                };
                r.finish()?;
                Ok(result)
            }
            _ => Err(Error::malformed("invalid node origin")),
        }
    }
}
fn write_nullable<T: Text>(
    out: &mut Writer,
    value: Option<&T>,
    nesting: usize,
) -> Result<(), Error> {
    if let Some(value) = value {
        value.write(out, nesting)
    } else {
        out.open("none")?;
        out.close()
    }
}
fn read_nullable<T: Text>(
    value: &Value,
    budget: &mut Budget,
    nesting: usize,
) -> Result<Option<T>, Error> {
    let (tag, values) = text::variant(value)?;
    if tag == "none" {
        unit(&values)?;
        Ok(None)
    } else {
        Ok(Some(T::read(value, budget, nesting)?))
    }
}
impl Text for NodePayload {
    fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error> {
        out.open("nodePayload")?;
        out.field("value", |out| {
            write_nullable(out, self.value.as_ref(), nesting)
        })?;
        out.field("origin", |out| {
            write_nullable(out, self.origin.as_ref(), nesting)
        })?;
        out.close()
    }
    fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error> {
        let mut r = Record::new(value, "nodePayload")?;
        let result = Self {
            value: read_nullable(r.field("value")?, budget, nesting)?,
            origin: read_nullable(r.field("origin")?, budget, nesting)?,
        };
        r.finish()?;
        Ok(result)
    }
}
impl Text for PathSelector {
    fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error> {
        match self {
            Self::Field(name) => {
                out.open("field")?;
                out.string(name)?;
            }
            Self::Index(index) => {
                out.open("index")?;
                out.atom(index);
            }
            Self::DynamicIndex { symbol, predicate } => {
                out.open("dynamic")?;
                out.open("dynamicIndex")?;
                field(out, "symbol", symbol, nesting)?;
                field(out, "predicate", predicate, nesting)?;
                out.close()?;
            }
            Self::Slice { start, end, step } => {
                out.open("slice")?;
                out.open("slice")?;
                field(out, "start", start, nesting)?;
                field(out, "end", end, nesting)?;
                field(out, "step", step, nesting)?;
                out.close()?;
            }
        }
        out.close()
    }
    fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error> {
        let (tag, values) = text::variant(value)?;
        let value = single(tag, &values)?;
        match tag {
            "field" => Ok(Self::Field(text::string(value)?)),
            "index" => Ok(Self::Index(text::i64(value)?)),
            "dynamic" => {
                let mut r = Record::new(value, "dynamicIndex")?;
                let result = Self::DynamicIndex {
                    symbol: read(&mut r, "symbol", budget, nesting)?,
                    predicate: read(&mut r, "predicate", budget, nesting)?,
                };
                r.finish()?;
                Ok(result)
            }
            "slice" => {
                let mut r = Record::new(value, "slice")?;
                let result = Self::Slice {
                    start: read(&mut r, "start", budget, nesting)?,
                    end: read(&mut r, "end", budget, nesting)?,
                    step: read(&mut r, "step", budget, nesting)?,
                };
                r.finish()?;
                Ok(result)
            }
            _ => Err(Error::malformed("invalid path selector")),
        }
    }
}
impl Text for CleanupState {
    fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error> {
        match self {
            Self::CleanZ => out.open("cleanZ")?,
            Self::LiveUnknown => out.open("liveUnknown")?,
            Self::Escaped => out.open("escaped")?,
            Self::Released => out.open("released")?,
            Self::Restored { delta } => {
                out.open("restored")?;
                out.atom(delta);
            }
            Self::PendingRestore { delta, history } => {
                out.open("pendingRestore")?;
                out.open("pendingRestore")?;
                field(out, "delta", delta, nesting)?;
                field(out, "history", history, nesting)?;
                out.close()?;
            }
            Self::PendingFragment {
                delta,
                fragment_count,
                history,
            } => {
                out.open("pendingFragment")?;
                out.open("pendingFragment")?;
                field(out, "delta", delta, nesting)?;
                field(out, "fragmentCount", fragment_count, nesting)?;
                field(out, "history", history, nesting)?;
                out.close()?;
            }
        }
        out.close()
    }
    fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error> {
        let (tag, values) = text::variant(value)?;
        match tag {
            "restored" => Ok(Self::Restored {
                delta: text::u64(single(tag, &values)?)?,
            }),
            "pendingRestore" => {
                let mut r = Record::new(single(tag, &values)?, "pendingRestore")?;
                let result = Self::PendingRestore {
                    delta: read(&mut r, "delta", budget, nesting)?,
                    history: read(&mut r, "history", budget, nesting)?,
                };
                r.finish()?;
                Ok(result)
            }
            "pendingFragment" => {
                let mut r = Record::new(single(tag, &values)?, "pendingFragment")?;
                let result = Self::PendingFragment {
                    delta: read(&mut r, "delta", budget, nesting)?,
                    fragment_count: read(&mut r, "fragmentCount", budget, nesting)?,
                    history: read(&mut r, "history", budget, nesting)?,
                };
                r.finish()?;
                Ok(result)
            }
            _ => {
                unit(&values)?;
                match tag {
                    "cleanZ" => Ok(Self::CleanZ),
                    "liveUnknown" => Ok(Self::LiveUnknown),
                    "escaped" => Ok(Self::Escaped),
                    "released" => Ok(Self::Released),
                    _ => Err(Error::malformed("invalid cleanup")),
                }
            }
        }
    }
}

impl Text for NodeIdentity {
    fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error> {
        let (live, generations, retired, free, next_slot, next_stable) = self.snapshot_parts();
        out.open("identityState")?;
        out.field("live", |out| {
            out.open("list")?;
            for (node, stable) in live {
                out.open("liveIdentity")?;
                field(out, "node", node, nesting)?;
                field(out, "stable", stable, nesting)?;
                out.close()?;
            }
            out.close()
        })?;
        out.field("generations", |out| {
            out.open("list")?;
            for (node, generation) in generations {
                out.open("generation")?;
                field(out, "node", node, nesting)?;
                field(out, "generation", generation, nesting)?;
                out.close()?;
            }
            out.close()
        })?;
        field(out, "retired", retired, nesting)?;
        out.field("free", |out| {
            out.open("list")?;
            for node in free {
                node.write(out, nesting)?;
            }
            out.close()
        })?;
        field(out, "nextSlot", &next_slot, nesting)?;
        field(out, "nextStable", &next_stable, nesting)?;
        out.close()
    }
    fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error> {
        let mut r = Record::new(value, "identityState")?;
        let mut live = Vec::new();
        for value in text::sequence(r.field("live")?, budget)? {
            let mut entry = Record::new(value, "liveIdentity")?;
            live.push((
                read::<NodeId>(&mut entry, "node", budget, nesting)?,
                read::<StableNodeId>(&mut entry, "stable", budget, nesting)?,
            ));
            entry.finish()?;
        }
        let mut generations = Vec::new();
        for value in text::sequence(r.field("generations")?, budget)? {
            let mut entry = Record::new(value, "generation")?;
            generations.push((
                read::<NodeId>(&mut entry, "node", budget, nesting)?,
                read::<u32>(&mut entry, "generation", budget, nesting)?,
            ));
            entry.finish()?;
        }
        let result = Self::from_snapshot(
            unique_map(live, |entry| entry.0)?
                .into_iter()
                .map(|(node, (_, stable))| (node, stable))
                .collect(),
            unique_map(generations, |entry| entry.0)?
                .into_iter()
                .map(|(node, (_, generation))| (node, generation))
                .collect(),
            read(&mut r, "retired", budget, nesting)?,
            read(&mut r, "free", budget, nesting)?,
            read(&mut r, "nextSlot", budget, nesting)?,
            read(&mut r, "nextStable", budget, nesting)?,
        )?;
        r.finish()?;
        Ok(result)
    }
}
impl Text for OwnershipState {
    fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error> {
        out.open("ownershipState")?;
        field(out, "authorities", &self.authorities, nesting)?;
        field(out, "borrows", &self.borrows, nesting)?;
        field(out, "nextBorrow", &self.snapshot_next_borrow(), nesting)?;
        out.close()
    }
    fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error> {
        let mut r = Record::new(value, "ownershipState")?;
        let result = Self::from_snapshot(
            read(&mut r, "authorities", budget, nesting)?,
            read(&mut r, "borrows", budget, nesting)?,
            read(&mut r, "nextBorrow", budget, nesting)?,
        )?;
        r.finish()?;
        Ok(result)
    }
}
fn write_records<'a, T: Text + 'a>(
    out: &mut Writer,
    values: impl Iterator<Item = &'a T>,
) -> Result<(), Error> {
    out.open("list")?;
    for value in values {
        value.write(out, 0)?;
    }
    out.close()
}
impl Text for Graph {
    fn write(&self, out: &mut Writer, nesting: usize) -> Result<(), Error> {
        out.open("graphCheckpoint")?;
        out.field("regions", |out| write_records(out, self.regions.values()))?;
        out.field("nodes", |out| write_records(out, self.nodes.values()))?;
        out.field("ports", |out| write_records(out, self.ports.values()))?;
        field(out, "edges", &self.edges, nesting)?;
        field(out, "demands", &self.demanded_nodes, nesting)?;
        field(out, "identity", &self.identity, nesting)?;
        field(out, "ownership", &self.ownership, nesting)?;
        field(out, "revision", &self.revision, nesting)?;
        field(out, "nextRegion", &self.next_region, nesting)?;
        field(out, "nextPort", &self.next_port, nesting)?;
        out.close()
    }
    fn read(value: &Value, budget: &mut Budget, nesting: usize) -> Result<Self, Error> {
        let mut r = Record::new(value, "graphCheckpoint")?;
        let result = Self {
            cache_identity: Arc::new(()),
            regions: unique_map(
                read::<Vec<Region>>(&mut r, "regions", budget, nesting)?,
                |region| region.id,
            )?,
            nodes: unique_map(
                read::<Vec<Node>>(&mut r, "nodes", budget, nesting)?,
                |node| node.id,
            )?,
            ports: unique_map(
                read::<Vec<Port>>(&mut r, "ports", budget, nesting)?,
                |port| port.id,
            )?,
            edges: read(&mut r, "edges", budget, nesting)?,
            demanded_nodes: read(&mut r, "demands", budget, nesting)?,
            identity: read(&mut r, "identity", budget, nesting)?,
            ownership: read(&mut r, "ownership", budget, nesting)?,
            revision: read(&mut r, "revision", budget, nesting)?,
            next_region: read(&mut r, "nextRegion", budget, nesting)?,
            next_port: read(&mut r, "nextPort", budget, nesting)?,
        };
        r.finish()?;
        Ok(result)
    }
}
pub(crate) fn encode(graph: &Graph, digest: &[u8; 32]) -> Result<String, Error> {
    let mut out = Writer::new(Kind::Graph)?;
    out.open("graph")?;
    out.field("payload", |out| {
        out.open("graphPayload")?;
        field(
            out,
            "version",
            &super::graph_snapshot::GRAPH_SNAPSHOT_SCHEMA_VERSION,
            0,
        )?;
        field(
            out,
            "registryVersion",
            &super::graph_snapshot::CONTRACT_REGISTRY_VERSION,
            0,
        )?;
        field(out, "requiredFeatures", &BTreeSet::<String>::new(), 0)?;
        field(out, "optionalFeatures", &BTreeSet::<String>::new(), 0)?;
        field(out, "checkpoint", graph, 0)?;
        out.close()
    })?;
    out.field("digest", |out| out.string(&hex(digest)))?;
    out.close()?;
    out.finish()
}
pub(crate) fn decode(bytes: &[u8]) -> Result<(Payload, [u8; 32]), SnapshotError> {
    let parsed = text::parse(Kind::Graph, bytes)?;
    let mut budget = parsed.budget();
    let mut root = parsed.record("graph")?;
    let mut r = Record::new(root.field("payload")?, "graphPayload")?;
    let version = read(&mut r, "version", &mut budget, 0)?;
    let registry_version = read(&mut r, "registryVersion", &mut budget, 0)?;
    let required_features = read(&mut r, "requiredFeatures", &mut budget, 0)?;
    check_metadata(version, registry_version, &required_features)?;
    let payload = Payload {
        version,
        registry_version,
        required_features,
        optional_features: read(&mut r, "optionalFeatures", &mut budget, 0)?,
        graph: read(&mut r, "checkpoint", &mut budget, 0)?,
    };
    r.finish()?;
    let digest = text::string(root.field("digest")?)?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(SnapshotError::Malformed);
    }
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digest[index * 2..index * 2 + 2], 16)
            .map_err(|_| SnapshotError::Malformed)?;
    }
    root.finish()?;
    Ok((payload, bytes))
}
