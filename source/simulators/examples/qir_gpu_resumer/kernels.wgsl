// Spike (not for review): the kernels of the GPU resumer.
//
// The state of a chunk of shots is one array of complex f32 amplitudes, each
// shot's 2^n at `slot << n`. A pass runs on the shots of one group: the slots
// `shots[shot_off .. shot_off + shot_len]`. Every pass reads its parameters
// from one 256-byte slot of the uniform buffer (a dynamic offset); op lists
// (block, diagonal and two-qubit matrices) live in `ops`, integers there
// written as exact f32 values (never bit patterns: a small integer's bits are
// a denormal, which the driver may flush).
//
// Mesa's d3d12 driver hangs on whole-struct copies to or from storage, so
// fields are copied one by one.

struct P {
  m0: vec4<f32>,
  m1: vec4<f32>,
  n: u32,
  q0: u32,
  q1: u32,
  shot_off: u32,
  shot_len: u32,
  op_off: u32,
  op_len: u32,
  kind: u32,
  meas: u32,
  slots: u32,
  per_shot: u32,
  block: u32,
}

@group(0) @binding(0) var<storage, read_write> s: array<vec2<f32>>;
@group(0) @binding(1) var<uniform> p: P;
@group(0) @binding(2) var<storage, read> ops: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> shots: array<u32>;
@group(0) @binding(4) var<storage, read_write> rng: array<u32>;
@group(0) @binding(5) var<storage, read_write> partial: array<vec2<f32>>;
@group(0) @binding(6) var<storage, read_write> outcomes: array<u32>;
@group(0) @binding(7) var<storage, read_write> flags: array<vec2<f32>>;

const WG: u32 = 256u;
const NO_RESULT: u32 = 0xffffffffu;

fn cmul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
  return vec2(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}

// `k` with a zero bit inserted at `q`.
fn insert0(k: u32, q: u32) -> u32 {
  let low = k & ((1u << q) - 1u);
  return ((k >> q) << (q + 1u)) | low;
}

fn flat(id: vec3<u32>, n: vec3<u32>) -> u32 {
  return id.x + id.y * n.x * WG;
}

fn flat_group(wid: vec3<u32>, n: vec3<u32>) -> u32 {
  return wid.x + wid.y * n.x;
}

fn ival(x: f32) -> u32 {
  return u32(x + 0.5);
}

// |0...0> in every slot of the chunk.
@compute @workgroup_size(256)
fn init(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
  let t = flat(id, nw);
  if (t >= (p.slots << p.n)) { return; }
  let mask = (1u << p.n) - 1u;
  s[t] = select(vec2(0.0, 0.0), vec2(1.0, 0.0), (t & mask) == 0u);
}

// A 2x2 matrix (rows m0, m1) on qubit q0.
@compute @workgroup_size(256)
fn gate1(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
  let t = flat(id, nw);
  let j = t >> (p.n - 1u);
  if (j >= p.shot_len) { return; }
  let k = t & ((1u << (p.n - 1u)) - 1u);
  let base = shots[p.shot_off + j] << p.n;
  let i0 = base | insert0(k, p.q0);
  let i1 = i0 | (1u << p.q0);
  let a = s[i0];
  let b = s[i1];
  s[i0] = cmul(p.m0.xy, a) + cmul(p.m0.zw, b);
  s[i1] = cmul(p.m1.xy, a) + cmul(p.m1.zw, b);
}

// Row r of the 4x4 matrix at ops[off] (row-major, 2 entries per vec4)
// applied to (v0, v1, v2, v3).
fn row4(off: u32, r: u32, v0: vec2<f32>, v1: vec2<f32>, v2: vec2<f32>, v3: vec2<f32>) -> vec2<f32> {
  let a = ops[off + r * 2u];
  let b = ops[off + r * 2u + 1u];
  return cmul(a.xy, v0) + cmul(a.zw, v1) + cmul(b.xy, v2) + cmul(b.zw, v3);
}

// A 4x4 matrix (ops[op_off..+8]) on qubits q0 (most significant), q1.
@compute @workgroup_size(256)
fn gate2(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
  let t = flat(id, nw);
  let j = t >> (p.n - 2u);
  if (j >= p.shot_len) { return; }
  let k = t & ((1u << (p.n - 2u)) - 1u);
  let base = shots[p.shot_off + j] << p.n;
  let lo = min(p.q0, p.q1);
  let hi = max(p.q0, p.q1);
  let i0 = base | insert0(insert0(k, lo), hi);
  let i1 = i0 | (1u << p.q1);
  let i2 = i0 | (1u << p.q0);
  let i3 = i2 | (1u << p.q1);
  let v0 = s[i0];
  let v1 = s[i1];
  let v2 = s[i2];
  let v3 = s[i3];
  s[i0] = row4(p.op_off, 0u, v0, v1, v2, v3);
  s[i1] = row4(p.op_off, 1u, v0, v1, v2, v3);
  s[i2] = row4(p.op_off, 2u, v0, v1, v2, v3);
  s[i3] = row4(p.op_off, 3u, v0, v1, v2, v3);
}

// A product of diagonal gates as a phase, in turns: the sum over terms
// (d, m, c) of c * popcount(k & (k >> d) & m). A term with d = 0 is a phase
// on each qubit of m; with d > 0, on each pair (q, q + d), q in m, both one.
// (The product's global phase is dropped.)
fn term_turns(t: vec4<f32>, k: u32) -> f32 {
  return t.z * f32(countOneBits(k & (k >> ival(t.x)) & ival(t.y)));
}

fn phase(turns: f32) -> vec2<f32> {
  let a = 6.2831853 * fract(turns);
  return vec2(cos(a), sin(a));
}

// op_len phase terms (ops[op_off..]) on every amplitude.
@compute @workgroup_size(256)
fn diag(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
  let t = flat(id, nw);
  let j = t >> p.n;
  if (j >= p.shot_len) { return; }
  let k = t & ((1u << p.n) - 1u);
  let i = (shots[p.shot_off + j] << p.n) | k;
  var turns = 0.0;
  for (var term = 0u; term < p.op_len; term++) {
    turns += term_turns(ops[p.op_off + term], k);
  }
  s[i] = cmul(phase(turns), s[i]);
}

var<workgroup> tile: array<vec2<f32>, 1024>;
var<workgroup> code: array<vec4<f32>, 128>;

// Row r of the 4x4 matrix at code[off] (row-major, 2 entries per vec4).
fn row4w(off: u32, r: u32, v0: vec2<f32>, v1: vec2<f32>, v2: vec2<f32>, v3: vec2<f32>) -> vec2<f32> {
  let a = code[off + r * 2u];
  let b = code[off + r * 2u + 1u];
  return cmul(a.xy, v0) + cmul(a.zw, v1) + cmul(b.xy, v2) + cmul(b.zw, v3);
}

// The position of the block's i-th qubit (ascending; 5 bits each in q0, q1).
fn tile_pos(i: u32) -> u32 {
  return select((p.q1 >> (5u * (i - 6u))) & 31u, (p.q0 >> (5u * i)) & 31u, i < 6u);
}

// The in-shot offset of tile entry e: its bits at the block's positions.
fn tile_offset(e: u32) -> u32 {
  var k = 0u;
  for (var i = 0u; i < p.block; i++) { k |= ((e >> i) & 1u) << tile_pos(i); }
  return k;
}

// op_len ops on the `block` qubits at tile_pos(0..block) (at most 10; the
// low qubits, or a gather block), 2^block amplitudes in workgroup memory;
// ops name the qubits by their index in the block. The ops are `kind` vec4 at ops[op_off..] (at most 128),
// copied to workgroup memory; each a header (kind, qa, qb, len), then kind
// 11: 2x2 rows (2 vec4); kind 12: 4x4 (8 vec4); kind 13: len phase terms.
@compute @workgroup_size(256)
fn block(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
  let w = flat_group(wid, nw);
  let b = p.block;
  let high = p.n - b;
  let j = w >> high;
  if (j >= p.shot_len) { return; }
  let size = 1u << b;
  // The workgroup's other qubits: its index with zeros at the block's.
  let contiguous = tile_pos(b - 1u) == b - 1u;
  var rest = w & ((1u << high) - 1u);
  if (contiguous) {
    rest = rest << b;
  } else {
    for (var i = 0u; i < b; i++) { rest = insert0(rest, tile_pos(i)); }
  }
  let base = (shots[p.shot_off + j] << p.n) | rest;
  if (lid.x < p.kind) { code[lid.x] = ops[p.op_off + lid.x]; }
  if (contiguous) {
    for (var e = lid.x; e < size; e += WG) { tile[e] = s[base + e]; }
  } else {
    for (var e = lid.x; e < size; e += WG) { tile[e] = s[base + tile_offset(e)]; }
  }
  workgroupBarrier();
  var off = 0u;
  for (var o = 0u; o < p.op_len; o++) {
    let h = code[off];
    let kind = ival(h.x);
    let qa = ival(h.y);
    let qb = ival(h.z);
    let len = ival(h.w);
    if (kind == 11u) {
      let r0 = code[off + 1u];
      let r1 = code[off + 2u];
      for (var e = lid.x; e < size / 2u; e += WG) {
        let i0 = insert0(e, qa);
        let i1 = i0 | (1u << qa);
        let a = tile[i0];
        let c = tile[i1];
        tile[i0] = cmul(r0.xy, a) + cmul(r0.zw, c);
        tile[i1] = cmul(r1.xy, a) + cmul(r1.zw, c);
      }
      off += 3u;
    } else if (kind == 12u) {
      let e = lid.x;
      if (e < size / 4u) {
        let lo = min(qa, qb);
        let hi = max(qa, qb);
        let i0 = insert0(insert0(e, lo), hi);
        let i1 = i0 | (1u << qb);
        let i2 = i0 | (1u << qa);
        let i3 = i2 | (1u << qb);
        let v0 = tile[i0];
        let v1 = tile[i1];
        let v2 = tile[i2];
        let v3 = tile[i3];
        tile[i0] = row4w(off + 1u, 0u, v0, v1, v2, v3);
        tile[i1] = row4w(off + 1u, 1u, v0, v1, v2, v3);
        tile[i2] = row4w(off + 1u, 2u, v0, v1, v2, v3);
        tile[i3] = row4w(off + 1u, 3u, v0, v1, v2, v3);
      }
      off += 9u;
    } else {
      for (var e = lid.x; e < size; e += WG) {
        var turns = 0.0;
        for (var term = 0u; term < len; term++) {
          turns += term_turns(code[off + 1u + term], e);
        }
        tile[e] = cmul(phase(turns), tile[e]);
      }
      off += 1u + len;
    }
    workgroupBarrier();
  }
  if (contiguous) {
    for (var e = lid.x; e < size; e += WG) { s[base + e] = tile[e]; }
  } else {
    for (var e = lid.x; e < size; e += WG) { s[base + tile_offset(e)] = tile[e]; }
  }
}

var<workgroup> acc2: array<vec2<f32>, 256>;

// Per group shot j and chunk c: the sums of |amp|^2 with qubit q0 = 0 and 1
// over 4096 pairs, into partial[j * per_shot + c].
@compute @workgroup_size(256)
fn prob(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
  let w = flat_group(wid, nw);
  let j = w / p.per_shot;
  if (j >= p.shot_len) { return; }
  let c = w % p.per_shot;
  let half = 1u << (p.n - 1u);
  let base = shots[p.shot_off + j] << p.n;
  var sum = vec2(0.0, 0.0);
  for (var it = 0u; it < 16u; it++) {
    let e = c * 4096u + it * WG + lid.x;
    if (e < half) {
      let i0 = base | insert0(e, p.q0);
      let a = s[i0];
      let b = s[i0 | (1u << p.q0)];
      sum += vec2(dot(a, a), dot(b, b));
    }
  }
  acc2[lid.x] = sum;
  workgroupBarrier();
  for (var stride = 128u; stride > 0u; stride >>= 1u) {
    if (lid.x < stride) { acc2[lid.x] += acc2[lid.x + stride]; }
    workgroupBarrier();
  }
  if (lid.x == 0u) { partial[w] = acc2[0]; }
}

fn pcg(slot: u32) -> u32 {
  let state = rng[slot] * 747796405u + 2891336453u;
  rng[slot] = state;
  let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
  return (word >> 22u) ^ word;
}

// One thread per group shot: draws the outcome from the shot's own generator,
// records it (unless meas is NO_RESULT, a reset), and leaves (outcome,
// 1/sqrt(P(outcome))) in flags[slot] for collapse.
@compute @workgroup_size(256)
fn sample(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
  let j = flat(id, nw);
  if (j >= p.shot_len) { return; }
  let slot = shots[p.shot_off + j];
  var t = vec2(0.0, 0.0);
  for (var c = 0u; c < p.per_shot; c++) { t += partial[j * p.per_shot + c]; }
  let u = f32(pcg(slot) >> 8u) * (1.0 / 16777216.0);
  let one = u * (t.x + t.y) < t.y;
  flags[slot] = vec2(select(0.0, 1.0, one), inverseSqrt(select(t.x, t.y, one)));
  if (p.meas != NO_RESULT) {
    outcomes[p.meas * p.slots + slot] = select(0u, 1u, one);
  }
}

// Collapses qubit q0 of each group shot to its drawn outcome, renormalized;
// kind 0 (mz) keeps the outcome's branch, kind 1 (mresetz, reset) moves it
// to |0>.
@compute @workgroup_size(256)
fn collapse(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
  let t = flat(id, nw);
  let j = t >> (p.n - 1u);
  if (j >= p.shot_len) { return; }
  let k = t & ((1u << (p.n - 1u)) - 1u);
  let slot = shots[p.shot_off + j];
  let f = flags[slot];
  let one = f.x > 0.5;
  let i0 = (slot << p.n) | insert0(k, p.q0);
  let i1 = i0 | (1u << p.q0);
  let zero = vec2(0.0, 0.0);
  let a = s[i0];
  let b = s[i1];
  if (p.kind == 0u) {
    s[i0] = select(a * f.y, zero, one);
    s[i1] = select(zero, b * f.y, one);
  } else {
    s[i0] = select(a, b, one) * f.y;
    s[i1] = zero;
  }
}
