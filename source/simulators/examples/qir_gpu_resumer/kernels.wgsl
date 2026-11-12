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
  round: u32,
  site_off: u32,
  meta_off: u32,
  site_mode: u32,
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

// ---------------------------------------------------------------------------
// Noise. A noise site's fault is drawn per shot from a hash of the shot's
// noise seed, the round and the call's batch index (so every thread and
// workgroup of a shot agrees, and a shot's draws depend only on its id),
// and picked from its table (entries (thr_hi, thr_lo, xbits, zbits) in ops):
// the first entry whose cumulative threshold exceeds the draw.

fn mix(v: u32) -> u32 {
  var x = v;
  x ^= x >> 16u;
  x *= 0x7feb352du;
  x ^= x >> 15u;
  x *= 0x846ca68bu;
  x ^= x >> 16u;
  return x;
}

fn below(hi: u32, lo: u32, e: vec4<f32>) -> bool {
  let th = ival(e.x);
  return hi < th || (hi == th && lo < ival(e.y));
}

// The fault (xbits, zbits over the targets) of the noise site whose header
// is h = (marker, site, table offset, table length), for the shot in `slot`.
fn draw(slot: u32, h: vec4<f32>) -> vec2<u32> {
  let at = p.meta_off + 2u * slot;
  var site = p.site_off + ival(h.y);
  if (p.site_mode == 1u) { site = shots[at + 1u] - p.site_off + ival(h.y); }
  let u = mix(shots[at] ^ mix((p.round * 0x9e3779b9u) ^ mix(site ^ 0x632be5abu)));
  let hi = u >> 16u;
  let lo = u & 0xffffu;
  let off = ival(h.z);
  let len = ival(h.w);
  // Most draws are fault-free: the last entry's threshold is the total.
  if (!below(hi, lo, ops[off + len - 1u])) { return vec2(0u, 0u); }
  for (var e = 0u; e < len; e++) {
    let entry = ops[off + e];
    if (below(hi, lo, entry)) { return vec2(ival(entry.z), ival(entry.w)); }
  }
  return vec2(0u, 0u);
}

// The fault as qubit masks (x, z): the targets' bits, the targets read
// from ops[at + 1..] (global qubits).
fn fault_masks(f: vec2<u32>, at: u32) -> vec2<u32> {
  var m = vec2(0u, 0u);
  if (f.x == 0u && f.y == 0u) { return m; }
  let count = ival(ops[at + 1u].x);
  for (var i = 0u; i < count; i++) {
    let w = i + 1u;
    let q = 1u << ival(ops[at + 1u + w / 4u][w % 4u]);
    if (((f.x >> i) & 1u) == 1u) { m.x |= q; }
    if (((f.y >> i) & 1u) == 1u) { m.y |= q; }
  }
  return m;
}

// The vec4 a noise site's code takes (header and targets).
fn noise_len(at: u32) -> u32 {
  return 1u + (ival(ops[at + 1u].x) + 4u) / 4u;
}

fn sign_of(i: u32, z: u32) -> f32 {
  return select(1.0, -1.0, (countOneBits(i & z) & 1u) == 1u);
}

// One noise site (ops[op_off..]) on each group shot: a thread per pair of
// amplitudes; shots without a fault leave at once.
@compute @workgroup_size(256)
fn pauli(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
  let t = flat(id, nw);
  let j = t >> (p.n - 1u);
  if (j >= p.shot_len) { return; }
  let k = t & ((1u << (p.n - 1u)) - 1u);
  let slot = shots[p.shot_off + j];
  let f = draw(slot, ops[p.op_off]);
  if (f.x == 0u && f.y == 0u) { return; }
  let m = fault_masks(f, p.op_off);
  let base = slot << p.n;
  if (m.x == 0u) {
    let i0 = insert0(k, 0u);
    let i1 = i0 | 1u;
    s[base | i0] = s[base | i0] * sign_of(i0, m.y);
    s[base | i1] = s[base | i1] * sign_of(i1, m.y);
  } else {
    let pivot = firstLeadingBit(m.x);
    let i0 = insert0(k, pivot);
    let i1 = i0 ^ m.x;
    let a = s[base | i0];
    let b = s[base | i1];
    s[base | i1] = a * sign_of(i0, m.y);
    s[base | i0] = b * sign_of(i1, m.y);
  }
}

// The phase (in turns) a diagonal pass's code gives the amplitude that
// starts at k: phase terms evaluated at k moved by the X parts of the
// faults before them, and each fault's Z sign (half a turn).
fn diag_turns(k: u32, faults: ptr<function, array<vec2<u32>, 32>>) -> f32 {
  var turns = 0.0;
  var c = 0u;
  var odd = 0u;
  var f = 0u;
  var at = 0u;
  while (at < p.op_len) {
    let e = ops[p.op_off + at];
    if (e.x < 0.0) {
      let m = (*faults)[f];
      f++;
      odd ^= countOneBits((k ^ c) & m.y) & 1u;
      c ^= m.x;
      at += noise_len(p.op_off + at);
    } else {
      turns += term_turns(e, k ^ c);
      at++;
    }
  }
  return turns + 0.5 * f32(odd);
}

// A diagonal pass: op_len vec4 of code (ops[op_off..]): phase terms, and
// noise sites (marker -1) between them; q0 the count of terms. A thread
// per amplitude; with a fault's X part, the pair (k, k ^ X) by one thread.
// The faults of a diagonal pass's noise sites, by code position (hoisted).
var<workgroup> dmask: array<vec2<u32>, 256>;

var<workgroup> dxor: atomic<u32>;
var<workgroup> dany: atomic<u32>;

// diag_turns with the faults read from dmask by code position, the code
// (noise sites between phase terms) at ops[op_off + q0 ..][.. op_len - q0].
fn diag_turns_w(k: u32) -> f32 {
  var turns = 0.0;
  var c = 0u;
  var odd = 0u;
  var at = 0u;
  let len = p.op_len - p.q0;
  let code = p.op_off + p.q0;
  while (at < len) {
    let e = ops[code + at];
    if (e.x < 0.0) {
      let m = dmask[at];
      odd ^= countOneBits((k ^ c) & m.y) & 1u;
      c ^= m.x;
      at += noise_len(code + at);
    } else {
      turns += term_turns(e, k ^ c);
      at++;
    }
  }
  return turns + 0.5 * f32(odd);
}

// A diagonal pass with noise, its draws hoisted (p.meas == 1; n >= 8): the
// pass's phase without its noise (q0 terms at op_off), then its code. A
// workgroup's threads are one shot's: `x` is the XOR of the X parts of the
// shot's faults, `hit` whether it has any.
fn diag_hoisted(lid: u32, w: u32, slot: u32, x: u32, hit: bool) {
  let k = ((w << 8u) | lid) & ((1u << p.n) - 1u);
  let base = slot << p.n;
  if (!hit) {
    if (p.q0 == 0u) { return; }
    var turns = 0.0;
    for (var term = 0u; term < p.q0; term++) {
      turns += term_turns(ops[p.op_off + term], k);
    }
    s[base | k] = cmul(phase(turns), s[base | k]);
    return;
  }
  if (x == 0u) {
    s[base | k] = cmul(phase(diag_turns_w(k)), s[base | k]);
    return;
  }
  if (((k >> firstLeadingBit(x)) & 1u) == 1u) { return; }
  let k1 = k ^ x;
  let a = s[base | k];
  let b = s[base | k1];
  s[base | k1] = cmul(phase(diag_turns_w(k)), a);
  s[base | k] = cmul(phase(diag_turns_w(k1)), b);
}

@compute @workgroup_size(256)
fn diag(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
  if (p.meas == 1u) {
    let w = flat_group(wid, nw);
    let j = w >> (p.n - 8u);
    if (j >= p.shot_len) { return; }
    let slot = shots[p.shot_off + j];
    if (lid.x == 0u) {
      atomicStore(&dxor, 0u);
      atomicStore(&dany, 0u);
    }
    workgroupBarrier();
    if (lid.x < p.op_len - p.q0) {
      let at = p.op_off + p.q0 + lid.x;
      let e = ops[at];
      if (e.x < 0.0) {
        let m = fault_masks(draw(slot, e), at);
        dmask[lid.x] = m;
        if (m.x != 0u || m.y != 0u) {
          atomicXor(&dxor, m.x);
          atomicOr(&dany, 1u);
        }
      }
    }
    workgroupBarrier();
    diag_hoisted(lid.x, w, slot, atomicLoad(&dxor), atomicLoad(&dany) != 0u);
    return;
  }
  let t = flat(id, nw);
  let j = t >> p.n;
  if (j >= p.shot_len) { return; }
  let k = t & ((1u << p.n) - 1u);
  let slot = shots[p.shot_off + j];
  let base = slot << p.n;
  if (p.q0 == p.op_len) {
    // Phase terms only.
    var turns = 0.0;
    for (var term = 0u; term < p.op_len; term++) {
      turns += term_turns(ops[p.op_off + term], k);
    }
    s[base | k] = cmul(phase(turns), s[base | k]);
    return;
  }
  var faults: array<vec2<u32>, 32>;
  var x = 0u;
  var hit = false;
  var f = 0u;
  var at = 0u;
  while (at < p.op_len) {
    let e = ops[p.op_off + at];
    if (e.x < 0.0) {
      let m = fault_masks(draw(slot, e), p.op_off + at);
      faults[f] = m;
      f++;
      x ^= m.x;
      hit = hit || m.x != 0u || m.y != 0u;
      at += noise_len(p.op_off + at);
    } else {
      at++;
    }
  }
  if (!hit && p.q0 == 0u) { return; }
  if (x == 0u) {
    s[base | k] = cmul(phase(diag_turns(k, &faults)), s[base | k]);
    return;
  }
  if (((k >> firstLeadingBit(x)) & 1u) == 1u) { return; }
  let k1 = k ^ x;
  let a = s[base | k];
  let b = s[base | k1];
  s[base | k1] = cmul(phase(diag_turns(k, &faults)), a);
  s[base | k] = cmul(phase(diag_turns(k1, &faults)), b);
}

var<workgroup> tile: array<vec2<f32>, 1024>;
var<workgroup> code: array<vec4<f32>, 256>;
// The faults (block-local masks) of a block's noise sites, by op index,
// and which sites are fault-free (bits), when the draws are hoisted.
var<workgroup> nmask: array<vec2<u32>, 128>;
var<workgroup> nfree: array<u32, 128>;
var<workgroup> nskip: vec4<u32>;
var<workgroup> nany: u32;

// The vec4 the block op at code[off] takes.
fn op_size(off: u32) -> u32 {
  let h = code[off];
  let kind = ival(h.x);
  if (kind == 11u) { return 3u; }
  if (kind == 12u) { return 9u; }
  if (kind == 14u) { return 1u + (ival(code[off + 1u].x) + 4u) / 4u; }
  return 1u + ival(h.w);
}

// Whether bits 0..n of `skip` are all set.
fn all_set(skip: vec4<u32>, n: u32) -> bool {
  var ok = true;
  for (var i = 0u; i < 4u; i++) {
    let lo = i * 32u;
    if (n > lo) {
      let count = min(32u, n - lo);
      let want = select((1u << count) - 1u, 0xffffffffu, count == 32u);
      ok = ok && (skip[i] & want) == want;
    }
  }
  return ok;
}

// A block noise site's fault as block-local masks (x, z).
fn block_masks(f: vec2<u32>, off: u32) -> vec2<u32> {
  var m = vec2(0u, 0u);
  let count = ival(code[off + 1u].x);
  for (var i = 0u; i < count; i++) {
    let wi = i + 1u;
    let q = 1u << ival(code[off + 1u + wi / 4u][wi % 4u]);
    if (((f.x >> i) & 1u) == 1u) { m.x |= q; }
    if (((f.y >> i) & 1u) == 1u) { m.y |= q; }
  }
  return m;
}

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
// 11: 2x2 rows (2 vec4); kind 12: 4x4 (8 vec4); kind 13: len phase terms;
// kind 14: a noise site; kind 16: len phase terms standing for the next qa
// ops (a run of phases and sites) when none of its sites has a fault.
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
  workgroupBarrier();
  // Hoisted draws (p.meas == 1): thread o draws op o's site, if a site
  // (nfree: 1 fault-free, 2 a fault); then thread 0 marks the ops to skip:
  // fault-free sites, and either a kind-16 op (a run's phase without its
  // noise) or the run it stands for, by whether a site of the run has a
  // fault.
  var skip = vec4(0u, 0u, 0u, 0u);
  // The code run: by default all of it; for a fault-free shot, the
  // noiseless variant (p.per_shot - 1 ops at p.m0.x), if there is one.
  var start = 0u;
  var count = p.op_len;
  if (p.meas == 1u) {
    if (lid.x < p.op_len) {
      var at = 0u;
      for (var o = 0u; o < lid.x; o++) { at += op_size(at); }
      let h = code[at];
      var m = vec2(0u, 0u);
      var free = 0u;
      if (ival(h.x) == 14u) {
        m = block_masks(draw(shots[p.shot_off + j], h), at);
        free = select(2u, 1u, m.x == 0u && m.y == 0u);
      }
      nmask[lid.x] = m;
      nfree[lid.x] = free;
    }
    workgroupBarrier();
    if (p.per_shot > 0u) {
      if (lid.x == 0u) {
        var any = 0u;
        for (var o = 0u; o < p.op_len; o++) { any |= nfree[o] & 2u; }
        nany = any;
      }
      if (workgroupUniformLoad(&nany) == 0u) {
        start = ival(p.m0.x);
        count = p.per_shot - 1u;
        if (count == 0u) { return; }
      }
    }
  }
  if (p.meas == 1u && count == p.op_len && start == 0u) {
    if (lid.x == 0u) {
      var sk = vec4(0u, 0u, 0u, 0u);
      var at = 0u;
      for (var o = 0u; o < p.op_len; o++) {
        let h = code[at];
        let kind = ival(h.x);
        if (kind == 14u && nfree[o] == 1u) {
          sk[o >> 5u] |= 1u << (o & 31u);
        }
        if (kind == 16u) {
          let r = ival(h.y);
          var faulty = false;
          for (var i = o + 1u; i <= o + r; i++) { faulty = faulty || nfree[i] == 2u; }
          if (faulty) {
            sk[o >> 5u] |= 1u << (o & 31u);
          } else {
            for (var i = o + 1u; i <= o + r; i++) { sk[i >> 5u] |= 1u << (i & 31u); }
          }
        }
        at += op_size(at);
      }
      nskip = sk;
    }
    skip = workgroupUniformLoad(&nskip);
    // Every op skipped (a fault-free shot's noise-only piece): the tile is
    // left as it is.
    if (all_set(skip, p.op_len)) { return; }
  }
  if (contiguous) {
    for (var e = lid.x; e < size; e += WG) { tile[e] = s[base + e]; }
  } else {
    for (var e = lid.x; e < size; e += WG) { tile[e] = s[base + tile_offset(e)]; }
  }
  workgroupBarrier();
  var off = start;
  for (var o = 0u; o < count; o++) {
    if (((skip[o >> 5u] >> (o & 31u)) & 1u) == 1u) {
      // A fault-free site, or a run or its fast phase: no sweep, no barrier.
      off += op_size(off);
      continue;
    }
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
    } else if (kind == 14u) {
      // A noise site; its targets (block-local) follow the header.
      var m = vec2(0u, 0u);
      if (p.meas == 1u) {
        m = nmask[o];
      } else {
        m = block_masks(draw(shots[p.shot_off + j], h), off);
      }
      let count = ival(code[off + 1u].x);
      if (m.x != 0u || m.y != 0u) {
        let mx = m.x;
        let mz = m.y;
        if (mx == 0u) {
          for (var e = lid.x; e < size; e += WG) {
            tile[e] = tile[e] * sign_of(e, mz);
          }
        } else {
          let pivot = firstLeadingBit(mx);
          for (var e = lid.x; e < size / 2u; e += WG) {
            let i0 = insert0(e, pivot);
            let i1 = i0 ^ mx;
            let a = tile[i0];
            let c = tile[i1];
            tile[i1] = a * sign_of(i0, mz);
            tile[i0] = c * sign_of(i1, mz);
          }
        }
      }
      off += 1u + (count + 4u) / 4u;
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
