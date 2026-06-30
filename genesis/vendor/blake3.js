// blake3.js — a compact, dependency-free BLAKE3 (hash mode, 256-bit output) for
// the browser. Faithful port of the official reference (reference_impl.py) so the
// content-addressed message id computed here EQUALS agentbbs-core's
// `blake3::hash(MessageBody::signing_bytes()).to_hex()` on the server — giving
// genesis id == server id (ADR: JS↔Rust id parity; fixes dedup + edit/delete by id).
//
// Verified against rustc `blake3` for "", "abc", 1024-byte, and 2050-byte inputs.

const IV = new Uint32Array([
  0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
  0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
]);
const MSG = [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8];
const CHUNK_START = 1, CHUNK_END = 2, PARENT = 4, ROOT = 8;
const CHUNK_LEN = 1024;

const rotr = (x, n) => ((x >>> n) | (x << (32 - n))) >>> 0;

function g(s, a, b, c, d, mx, my) {
  s[a] = (s[a] + s[b] + mx) >>> 0; s[d] = rotr(s[d] ^ s[a], 16);
  s[c] = (s[c] + s[d]) >>> 0;      s[b] = rotr(s[b] ^ s[c], 12);
  s[a] = (s[a] + s[b] + my) >>> 0; s[d] = rotr(s[d] ^ s[a], 8);
  s[c] = (s[c] + s[d]) >>> 0;      s[b] = rotr(s[b] ^ s[c], 7);
}

// Returns the full 16-word compression state (post-feedforward).
function compress(cv, block, counterLo, counterHi, blockLen, flags) {
  const s = new Uint32Array(16);
  s[0] = cv[0]; s[1] = cv[1]; s[2] = cv[2]; s[3] = cv[3];
  s[4] = cv[4]; s[5] = cv[5]; s[6] = cv[6]; s[7] = cv[7];
  s[8] = IV[0]; s[9] = IV[1]; s[10] = IV[2]; s[11] = IV[3];
  s[12] = counterLo >>> 0; s[13] = counterHi >>> 0; s[14] = blockLen >>> 0; s[15] = flags >>> 0;
  let m = block.slice(0, 16);
  for (let r = 0; r < 7; r++) {
    g(s, 0, 4, 8, 12, m[0], m[1]);
    g(s, 1, 5, 9, 13, m[2], m[3]);
    g(s, 2, 6, 10, 14, m[4], m[5]);
    g(s, 3, 7, 11, 15, m[6], m[7]);
    g(s, 0, 5, 10, 15, m[8], m[9]);
    g(s, 1, 6, 11, 12, m[10], m[11]);
    g(s, 2, 7, 8, 13, m[12], m[13]);
    g(s, 3, 4, 9, 14, m[14], m[15]);
    if (r < 6) { const pm = new Uint32Array(16); for (let i = 0; i < 16; i++) pm[i] = m[MSG[i]]; m = pm; }
  }
  for (let i = 0; i < 8; i++) { s[i] = (s[i] ^ s[i + 8]) >>> 0; s[i + 8] = (s[i + 8] ^ cv[i]) >>> 0; }
  return s;
}

// 64 bytes at `off` → 16 little-endian u32 (zero-padded past the end).
function blockWords(bytes, off) {
  const w = new Uint32Array(16);
  for (let i = 0; i < 16; i++) {
    const j = off + i * 4;
    w[i] = (((bytes[j] || 0)) | ((bytes[j + 1] || 0) << 8) | ((bytes[j + 2] || 0) << 16) | ((bytes[j + 3] || 0) << 24)) >>> 0;
  }
  return w;
}

const first8 = (s) => s.slice(0, 8);

// Merge a finished chunk CV into the subtree stack (reference algorithm).
function parentCv(left, right, flags) {
  const block = new Uint32Array(16);
  block.set(left, 0); block.set(right, 8);
  return first8(compress(IV, block, 0, 0, 64, PARENT | flags));
}

export function blake3(bytes) {
  const n = bytes.length;
  const cvStack = [];
  let chunkCounter = 0;
  let off = 0;
  // Process all complete chunks except the final one (which becomes the root Output).
  while (n - off > CHUNK_LEN) {
    // one full 1024-byte chunk
    let cv = IV;
    for (let b = 0; b < 16; b++) {
      const flags = (b === 0 ? CHUNK_START : 0) | (b === 15 ? CHUNK_END : 0);
      cv = first8(compress(cv, blockWords(bytes, off + b * 64), chunkCounter & 0xffffffff, Math.floor(chunkCounter / 4294967296), 64, flags));
    }
    // add to stack, merging on even counts
    let total = chunkCounter + 1;
    let newCv = cv;
    while ((total & 1) === 0) { newCv = parentCv(cvStack.pop(), newCv, 0); total >>>= 1; }
    cvStack.push(newCv);
    chunkCounter += 1;
    off += CHUNK_LEN;
  }
  // Final chunk: compress all but the last block as CVs, defer the last block as Output.
  const remaining = n - off;
  const numBlocks = Math.max(1, Math.ceil(remaining / 64));
  let cv = IV;
  for (let b = 0; b < numBlocks - 1; b++) {
    const flags = (b === 0 ? CHUNK_START : 0);
    cv = first8(compress(cv, blockWords(bytes, off + b * 64), chunkCounter & 0xffffffff, Math.floor(chunkCounter / 4294967296), 64, flags));
  }
  // The deferred last (partial) block.
  const lastB = numBlocks - 1;
  const lastLen = remaining === 0 ? 0 : (remaining - lastB * 64);
  const startFlag = lastB === 0 ? CHUNK_START : 0;
  let outInputCv = cv;
  let outBlock = blockWords(bytes, off + lastB * 64);
  let outBlockLen = lastLen;
  let outFlags = startFlag | CHUNK_END;
  // The deferred final-chunk Output carries its own chunk counter for its
  // chaining_value; parent nodes (and the root output) use counter 0.
  let outCntLo = chunkCounter & 0xffffffff, outCntHi = Math.floor(chunkCounter / 4294967296);
  // Merge up the stack: the chunk Output becomes the right child each level.
  for (let i = cvStack.length - 1; i >= 0; i--) {
    const childCv = first8(compress(outInputCv, outBlock, outCntLo, outCntHi, outBlockLen, outFlags));
    const block = new Uint32Array(16);
    block.set(cvStack[i], 0); block.set(childCv, 8);
    outInputCv = IV; outBlock = block; outBlockLen = 64; outFlags = PARENT;
    outCntLo = 0; outCntHi = 0;
  }
  // Root output: 32 bytes, output_block_counter = 0.
  const root = compress(outInputCv, outBlock, 0, 0, outBlockLen, outFlags | ROOT);
  const out = new Uint8Array(32);
  for (let i = 0; i < 8; i++) {
    out[i * 4] = root[i] & 0xff; out[i * 4 + 1] = (root[i] >>> 8) & 0xff;
    out[i * 4 + 2] = (root[i] >>> 16) & 0xff; out[i * 4 + 3] = (root[i] >>> 24) & 0xff;
  }
  return out;
}

export function blake3hex(bytes) {
  const out = blake3(bytes);
  let s = '';
  for (let i = 0; i < out.length; i++) s += out[i].toString(16).padStart(2, '0');
  return s;
}
