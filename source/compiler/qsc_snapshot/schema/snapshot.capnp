# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

@0xefc9678557ce0bce;

struct Document @0xc707b76219db13e2 {
  version @0 :UInt32;
  union {
    invalid @1 :Void;
    circuit @2 :CircuitDocument;
    store @3 :CircuitStore;
    graph @4 :GraphSnapshot;
  }
}

struct OptionalId32 @0x9af8d8707f881ca7 {
  union {
    none @0 :Void;
    some @1 :UInt32;
  }
}

struct OptionalId64 @0xbab20f4936704c40 {
  union {
    none @0 :Void;
    some @1 :UInt64;
  }
}

struct OptionalText @0xed863d8b158bb971 {
  union {
    none @0 :Void;
    some @1 :Text;
  }
}

struct Real64 @0xe34975b38fa97e42 {
  bits @0 :UInt64;
}

struct OptionalReal @0xd84818a875546576 {
  union {
    none @0 :Void;
    some @1 :Real64;
  }
}

struct Register @0xcee43e4cbeec8fd3 {
  name @0 :Text;
  start @1 :UInt32;
  size @2 :UInt32;
}

struct CircuitDocument @0x919d1acc49b84207 {
  store @0 :CircuitStore;
  quantumRegisters @1 :List(Register);
  classicalRegisters @2 :List(Register);
}

struct CircuitStore @0xc798bf99100958b9 {
  version @0 :UInt32;
  qubitCount @1 :UInt32;
  clbitCount @2 :UInt32;
  globalPhase @3 :Real64;
  parameters @4 :List(Text);
  variables @5 :List(Variable);
  expressions @6 :List(Expression);
  angles @7 :List(CircuitAngle);
  blocks @8 :List(Block);
  instructions @9 :List(Instruction);
}

struct Block @0x80f048f5bfa40cdf {
  instructions @0 :List(Instruction);
}

struct Instruction @0xe08fea74abac97fe {
  operation @0 :Operation;
  qubits @1 :List(UInt32);
  clbits @2 :List(UInt32);
  parameters @3 :OptionalParameters;
}

struct OptionalParameters @0xdfc3e1d769e75b37 {
  union {
    none @0 :Void;
    some @1 :List(ParameterValue);
  }
}

struct ParameterValue @0xcb867ab9a58c45b4 {
  union {
    invalid @0 :Void;
    literal @1 :Real64;
    parameter @2 :UInt32;
    expression @3 :UInt32;
  }
}

enum StandardGate @0xa599c8db272bf419 {
  invalid @0;
  i @1;
  x @2;
  y @3;
  z @4;
  h @5;
  s @6;
  sdg @7;
  sx @8;
  sxdg @9;
  t @10;
  tdg @11;
  cx @12;
  cy @13;
  cz @14;
  swap @15;
  ccx @16;
  cswap @17;
  rx @18;
  ry @19;
  rz @20;
  rxx @21;
  ryy @22;
  rzz @23;
  crx @24;
  cry @25;
  crz @26;
  measureX @27;
  measureY @28;
  measureZ @29;
  prepareX @30;
  prepareY @31;
  prepareZ @32;
}

struct Operation @0xebc41499e350b8d4 {
  union {
    invalid @0 :Void;
    standard @1 :StandardGate;
    custom @2 :CustomOperation;
    unitary @3 :UnitaryOperation;
    barrier @4 :UInt8;
    reset @5 :Void;
    delay @6 :Delay;
    control @7 :ControlFlow;
  }
}

struct CustomOperation @0x8c2dddf6148a4fc5 {
  name @0 :Text;
  qubitArity @1 :UInt8;
  parameterArity @2 :UInt8;
}

struct Complex64 @0xbaac676776e66d64 {
  realBits @0 :UInt64;
  imaginaryBits @1 :UInt64;
}

struct UnitaryOperation @0xec5ac3faaa19a827 {
  qubitArity @0 :UInt8;
  matrix @1 :List(Complex64);
}

enum TimeUnit @0xc4a4be594637c2df {
  invalid @0;
  dt @1;
  ns @2;
  us @3;
  ms @4;
  s @5;
}

struct Delay @0x8033817a0b4e6709 {
  duration @0 :Real64;
  unit @1 :TimeUnit;
}

enum VariableType @0xa6fffa1442c6bab4 {
  invalid @0;
  boolean @1;
  integer @2;
  real @3;
}

struct Variable @0xf6d8e127e18d8e8d {
  name @0 :Text;
  type @1 :VariableType;
}

struct Literal @0xe8438bdbe7abd523 {
  union {
    invalid @0 :Void;
    boolean @1 :Bool;
    integer @2 :Int64;
    real @3 :Real64;
  }
}

enum BinaryOp @0xaeb4c751f0de2a06 {
  invalid @0;
  eq @1;
  ne @2;
  lt @3;
  le @4;
  gt @5;
  ge @6;
  and @7;
  or @8;
  add @9;
  sub @10;
  mul @11;
}

struct BinaryExpression @0xec97e77c28a64879 {
  op @0 :BinaryOp;
  left @1 :UInt32;
  right @2 :UInt32;
}

struct Expression @0x930b46dea3773f5c {
  union {
    invalid @0 :Void;
    literal @1 :Literal;
    variable @2 :UInt32;
    clbit @3 :UInt32;
    not @4 :UInt32;
    binary @5 :BinaryExpression;
  }
}

enum AngleOp @0xfa650f0f41dcf353 {
  invalid @0;
  add @1;
  sub @2;
  mul @3;
  div @4;
}

struct BinaryCircuitAngle @0xd27428311664d5e5 {
  op @0 :AngleOp;
  left @1 :UInt32;
  right @2 :UInt32;
}

struct CircuitAngle @0xfc9a629f5e52f01e {
  union {
    invalid @0 :Void;
    literal @1 :Real64;
    parameter @2 :UInt32;
    neg @3 :UInt32;
    binary @4 :BinaryCircuitAngle;
  }
}

struct ControlFlow @0xab69453fe3096e38 {
  union {
    invalid @0 :Void;
    ifElse @1 :IfElse;
    while @2 :While;
    for @3 :For;
    switch @4 :Switch;
    box @5 :Box;
    moment @6 :UInt32;
    break @7 :Void;
    continue @8 :Void;
  }
}

struct IfElse @0xb0ef051f8fbf48be {
  condition @0 :UInt32;
  thenBlock @1 :UInt32;
  elseBlock @2 :OptionalId32;
}

struct While @0xcc9cb94f7dfe6085 {
  condition @0 :UInt32;
  body @1 :UInt32;
}

struct LoopIndex @0xb38cc79409220839 {
  union {
    none @0 :Void;
    parameter @1 :UInt32;
    variable @2 :UInt32;
  }
}

struct For @0x91f4aae96a09a5c2 {
  index @0 :LoopIndex;
  values @1 :List(Literal);
  body @2 :UInt32;
}

struct Case @0x8f9bf620f1ee4148 {
  values @0 :List(Literal);
  body @1 :UInt32;
}

struct Switch @0xf42b13f2b7a38099 {
  selector @0 :UInt32;
  cases @1 :List(Case);
  default @2 :OptionalId32;
}

struct Box @0xeb27f39abe3f1b89 {
  body @0 :UInt32;
  duration @1 :OptionalReal;
  label @2 :OptionalText;
}

struct GraphSnapshot @0xe60f92dd12eb4ce6 {
  payload @0 :GraphPayload;
  digest @1 :Data;
}

struct GraphPayload @0x9118831662dcdfce {
  version @0 :UInt32;
  registryVersion @1 :UInt32;
  requiredFeatures @2 :List(Text);
  optionalFeatures @3 :List(Text);
  checkpoint @4 :GraphCheckpoint;
}

struct GraphCheckpoint @0xf9a26530a893fa26 {
  regions @0 :List(Region);
  nodes @1 :List(Node);
  ports @2 :List(Port);
  edges @3 :List(Edge);
  demands @4 :List(UInt32);
  identity @5 :IdentityState;
  ownership @6 :OwnershipState;
  revision @7 :UInt64;
  nextRegion @8 :UInt32;
  nextPort @9 :UInt32;
}

enum RegionKind @0xec9c6bba2428f675 {
  invalid @0;
  moduleOmega @1;
  lambda @2;
  gamma @3;
  theta @4;
  recursion @5;
  handler @6;
  stage @7;
  residualCfg @8;
}

enum ExitKind @0xeb5e2f9c59a234cb {
  invalid @0;
  return @1;
  abort @2;
  branch @3;
  backedge @4;
}

enum InterfaceRole @0xe59ed7b4288ac757 {
  invalid @0;
  argument @1;
  result @2;
  exit @3;
  capture @4;
  carried @5;
}

struct Region @0xb32bc906111d055b {
  id @0 :UInt32;
  parent @1 :OptionalId32;
  kind @2 :RegionKind;
  owner @3 :OptionalId32;
  interface @4 :RegionInterface;
}

struct RegionInterface @0x9a37e5a52997b0c8 {
  arguments @0 :List(UInt32);
  results @1 :List(UInt32);
  exits @2 :List(RegionExit);
  bindings @3 :List(InterfaceBinding);
}

struct RegionExit @0xbac451c343320619 {
  kind @0 :ExitKind;
  port @1 :UInt32;
}

struct InterfaceBinding @0x89faed01ea19f168 {
  outer @0 :UInt32;
  inner @1 :UInt32;
  role @2 :InterfaceRole;
}

struct Node @0xfc761dcc3f08f437 {
  id @0 :UInt32;
  region @1 :UInt32;
  operation @2 :Text;
  inputs @3 :List(UInt32);
  outputs @4 :List(UInt32);
  effects @5 :List(Effect);
  symbol @6 :OptionalSymbol;
  payload @7 :NodePayload;
}

struct Symbol @0x92e1e4d6a7022e84 {
  package @0 :Text;
  item @1 :Text;
}

struct OptionalSymbol @0x9abe9f0f0c03759b {
  union {
    none @0 :Void;
    some @1 :Symbol;
  }
}

enum Direction @0x80b10e0a996b46a4 {
  invalid @0;
  input @1;
  output @2;
}

enum PortKind @0xb63f369e4f1bcdd7 {
  invalid @0;
  value @1;
  capability @2;
  hardEffect @3;
  controlExit @4;
}

enum Stage @0xcb9002bda655a91f {
  invalid @0;
  generation @1;
  residual @2;
}

enum Usage @0xb30722add9e520e5 {
  invalid @0;
  copyable @1;
  affine @2;
  linear @3;
}

struct Port @0x93b973a2495fa1d2 {
  id @0 :UInt32;
  node @1 :UInt32;
  region @2 :UInt32;
  direction @3 :Direction;
  kind @4 :PortKind;
  type @5 :Type;
  stage @6 :Stage;
  usage @7 :Usage;
  resource @8 :OptionalId64;
}

struct Type @0xd257a6255545f3b5 {
  union {
    invalid @0 :Void;
    unit @1 :Void;
    boolean @2 :Void;
    integer @3 :Void;
    double @4 :Void;
    string @5 :Void;
    result @6 :Void;
    qubit @7 :Void;
    tuple @8 :List(Type);
    array @9 :Type;
    location @10 :Type;
    unknown @11 :Void;
  }
}

enum EdgeKind @0xa75b4344824e7f97 {
  invalid @0;
  copyableValue @1;
  affineCapability @2;
  hardEffect @3;
  controlExit @4;
}

struct Edge @0x8a1dd0e231072b58 {
  source @0 :UInt32;
  target @1 :UInt32;
  kind @2 :EdgeKind;
}

enum EffectDomain @0xf9108daa464c48a6 {
  invalid @0;
  memory @1;
  quantum @2;
  result @3;
  allocation @4;
  output @5;
  abort @6;
  host @7;
  noise @8;
  unknown @9;
}

enum EffectMode @0xc6aeccacb69329ff {
  invalid @0;
  read @1;
  write @2;
  irreversible @3;
  allocate @4;
  release @5;
  emit @6;
  abort @7;
  unknown @8;
}

struct EffectResource @0xd5f6f01893c13eb5 {
  union {
    unknown @0 :Void;
    named @1 :Text;
  }
}

struct Effect @0x8853f8cde2830f88 {
  domain @0 :EffectDomain;
  mode @1 :EffectMode;
  resource @2 :EffectResource;
  observable @3 :Bool;
}

struct NodePayload @0xd74a9bb1efbc281b {
  value @0 :NodeValue;
  origin @1 :NodeOrigin;
}

struct NodeValue @0xb9bf4f807c1a0bfe {
  union {
    none @0 :Void;
    number @1 :Real64;
    classicalBit @2 :UInt32;
    angle @3 :GraphAngle;
  }
}

struct GraphAngle @0x987593d66410fad9 {
  union {
    invalid @0 :Void;
    literal @1 :Real64;
    parameter @2 :Text;
    neg @3 :GraphAngle;
    binary @4 :BinaryGraphAngle;
  }
}

struct BinaryGraphAngle @0xc6c7f4a534543952 {
  op @0 :AngleOp;
  left @1 :GraphAngle;
  right @2 :GraphAngle;
}

enum FirOriginKind @0xc963025e50c098d6 {
  invalid @0;
  package @1;
  item @2;
  block @3;
  expression @4;
  pattern @5;
  statement @6;
}

struct FirOrigin @0xa8f8f3e252ef8e77 {
  package @0 :UInt32;
  kind @1 :FirOriginKind;
  localId @2 :UInt32;
}

struct NodeOrigin @0xcbdc1f43b3df6b16 {
  union {
    none @0 :Void;
    instruction @1 :UInt32;
    qubit @2 :UInt32;
    clbit @3 :UInt32;
    globalPhase @4 :Void;
    fir @5 :FirOrigin;
  }
}

struct LiveIdentity @0x9a02ed819a9173bc {
  node @0 :UInt32;
  stable @1 :UInt64;
}

struct Generation @0xb42f1fb530e4e89a {
  node @0 :UInt32;
  generation @1 :UInt32;
}

struct IdentityState @0xe66549f16e3f627c {
  live @0 :List(LiveIdentity);
  generations @1 :List(Generation);
  retired @2 :List(UInt64);
  free @3 :List(UInt32);
  nextSlot @4 :UInt32;
  nextStable @5 :UInt64;
}

struct OwnerPath @0xb4908c84ae8ef829 {
  root @0 :UInt64;
  selectors @1 :List(PathSelector);
}

struct PathSelector @0x88b94cc858667bc4 {
  union {
    invalid @0 :Void;
    field @1 :Text;
    index @2 :Int64;
    dynamic @3 :DynamicIndex;
    slice @4 :Slice;
  }
}

struct DynamicIndex @0xa2dcef1fa8621d33 {
  symbol @0 :Text;
  predicate @1 :Text;
}

struct Slice @0x8cde537b5f59e7bb {
  start @0 :Int64;
  end @1 :Int64;
  step @2 :Int64;
}

struct CleanupState @0x98ef4ea1d3d2b2df {
  union {
    invalid @0 :Void;
    cleanZ @1 :Void;
    pendingRestore @2 :PendingRestore;
    pendingFragment @3 :PendingFragment;
    restored @4 :UInt64;
    liveUnknown @5 :Void;
    escaped @6 :Void;
    released @7 :Void;
  }
}

struct PendingRestore @0xbcfe976188e6cba8 {
  delta @0 :UInt64;
  history @1 :List(Text);
}

struct PendingFragment @0xa5bc6ca0b0a34256 {
  delta @0 :UInt64;
  fragmentCount @1 :UInt64;
  history @2 :List(Text);
}

enum BorrowKind @0xf5a944ab597961d2 {
  invalid @0;
  shared @1;
  exclusive @2;
}

struct Authority @0x93f5baa210527bf8 {
  path @0 :OwnerPath;
  resources @1 :List(UInt64);
  epoch @2 :UInt64;
  cleanup @3 :CleanupState;
}

struct Borrow @0xa15464936638eded {
  id @0 :UInt64;
  kind @1 :BorrowKind;
  path @2 :OwnerPath;
  resources @3 :List(UInt64);
  epoch @4 :UInt64;
}

struct OwnershipState @0x94bb65066f322f5b {
  authorities @0 :List(Authority);
  borrows @1 :List(Borrow);
  nextBorrow @2 :UInt64;
}

struct RegionFingerprint @0xbef4f49122aa0716 {
  nodes @0 :List(MatchedNode);
  edges @1 :List(Edge);
}

struct MatchedNode @0xc16e00dbbf807014 {
  stable @0 :UInt64;
  union {
    absent @1 :Void;
    present @2 :MatchedContent;
  }
}

struct MatchedContent @0x8743512f2884da51 {
  operation @0 :Text;
  effects @1 :List(Effect);
  payload @2 :NodePayload;
  symbol @3 :OptionalSymbol;
  ports @4 :List(MatchedPort);
}

struct MatchedPort @0xc056884990516ece {
  id @0 :UInt32;
  union {
    absent @1 :Void;
    present @2 :Port;
  }
}
