/*! noble-ed25519 - MIT License (c) 2019 Paul Miller (paulmillr.com) */
const P = 2n ** 255n - 19n; // ed25519 is twisted edwards curve
const N = 2n ** 252n + 27742317777372353535851937790883648493n; // curve's (group) order
const Gx = 0x216936d3cd6e53fec0a4e231fdd6dc5c692cc7609525a7b2c9562d608f25d51an; // base point x
const Gy = 0x6666666666666666666666666666666666666666666666666666666666666658n; // base point y
const CURVE = {
    a: -1n, // where a=-1, d = -(121665/121666) == -(121665 * inv(121666)) mod P
    d: 37095705934669439343138083508754565189542113879843219016388785533085940283555n,
    p: P, n: N, h: 8, Gx, Gy // field prime, curve (group) order, cofactor
};
const err = (m = '') => { throw new Error(m); }; // error helper, messes-up stack trace
const str = (s) => typeof s === 'string'; // is string
const isu8 = (a) => (a instanceof Uint8Array ||
    (a != null && typeof a === 'object' && a.constructor.name === 'Uint8Array'));
const au8 = (a, l) => // is Uint8Array (of specific length)
 !isu8(a) || (typeof l === 'number' && l > 0 && a.length !== l) ?
    err('Uint8Array of valid length expected') : a;
const u8n = (data) => new Uint8Array(data); // creates Uint8Array
const toU8 = (a, len) => au8(str(a) ? h2b(a) : u8n(au8(a)), len); // norm(hex/u8a) to u8a
const mod = (a, b = P) => { let r = a % b; return r >= 0n ? r : b + r; }; // mod division
const isPoint = (p) => (p instanceof Point ? p : err('Point expected')); // is xyzt point
class Point {
    constructor(ex, ey, ez, et) {
        this.ex = ex;
        this.ey = ey;
        this.ez = ez;
        this.et = et;
    }
    static fromAffine(p) { return new Point(p.x, p.y, 1n, mod(p.x * p.y)); }
    static fromHex(hex, zip215 = false) {
        const { d } = CURVE;
        hex = toU8(hex, 32);
        const normed = hex.slice(); // copy the array to not mess it up
        const lastByte = hex[31];
        normed[31] = lastByte & ~0x80; // adjust first LE byte = last BE byte
        const y = b2n_LE(normed); // decode as little-endian, convert to num
        if (zip215 && !(0n <= y && y < 2n ** 256n))
            err('bad y coord 1'); // zip215=true  [1..2^256-1]
        if (!zip215 && !(0n <= y && y < P))
            err('bad y coord 2'); // zip215=false [1..P-1]
        const y2 = mod(y * y); // y²
        const u = mod(y2 - 1n); // u=y²-1
        const v = mod(d * y2 + 1n); // v=dy²+1
        let { isValid, value: x } = uvRatio(u, v); // (uv³)(uv⁷)^(p-5)/8; square root
        if (!isValid)
            err('bad y coordinate 3'); // not square root: bad point
        const isXOdd = (x & 1n) === 1n; // adjust sign of x coordinate
        const isLastByteOdd = (lastByte & 0x80) !== 0; // x_0, last bit
        if (!zip215 && x === 0n && isLastByteOdd)
            err('bad y coord 3'); // x=0 and x_0 = 1
        if (isLastByteOdd !== isXOdd)
            x = mod(-x);
        return new Point(x, y, 1n, mod(x * y)); // Z=1, T=xy
    }
    get x() { return this.toAffine().x; } // .x, .y will call expensive toAffine.
    get y() { return this.toAffine().y; } // Should be used with care.
    equals(other) {
        const { ex: X1, ey: Y1, ez: Z1 } = this;
        const { ex: X2, ey: Y2, ez: Z2 } = isPoint(other); // isPoint() checks class equality
        const X1Z2 = mod(X1 * Z2), X2Z1 = mod(X2 * Z1);
        const Y1Z2 = mod(Y1 * Z2), Y2Z1 = mod(Y2 * Z1);
        return X1Z2 === X2Z1 && Y1Z2 === Y2Z1;
    }
    is0() { return this.equals(I); }
    negate() {
        return new Point(mod(-this.ex), this.ey, this.ez, mod(-this.et));
    }
    double() {
        const { ex: X1, ey: Y1, ez: Z1 } = this; // Cost: 4M + 4S + 1*a + 6add + 1*2
        const { a } = CURVE; // https://hyperelliptic.org/EFD/g1p/auto-twisted-extended.html#doubling-dbl-2008-hwcd
        const A = mod(X1 * X1);
        const B = mod(Y1 * Y1);
        const C = mod(2n * mod(Z1 * Z1));
        const D = mod(a * A);
        const x1y1 = X1 + Y1;
        const E = mod(mod(x1y1 * x1y1) - A - B);
        const G = D + B;
        const F = G - C;
        const H = D - B;
        const X3 = mod(E * F);
        const Y3 = mod(G * H);
        const T3 = mod(E * H);
        const Z3 = mod(F * G);
        return new Point(X3, Y3, Z3, T3);
    }
    add(other) {
        const { ex: X1, ey: Y1, ez: Z1, et: T1 } = this; // Cost: 8M + 1*k + 8add + 1*2.
        const { ex: X2, ey: Y2, ez: Z2, et: T2 } = isPoint(other); // doesn't check if other on-curve
        const { a, d } = CURVE; // http://hyperelliptic.org/EFD/g1p/auto-twisted-extended-1.html#addition-add-2008-hwcd-3
        const A = mod(X1 * X2);
        const B = mod(Y1 * Y2);
        const C = mod(T1 * d * T2);
        const D = mod(Z1 * Z2);
        const E = mod((X1 + Y1) * (X2 + Y2) - A - B);
        const F = mod(D - C);
        const G = mod(D + C);
        const H = mod(B - a * A);
        const X3 = mod(E * F);
        const Y3 = mod(G * H);
        const T3 = mod(E * H);
        const Z3 = mod(F * G);
        return new Point(X3, Y3, Z3, T3);
    }
    mul(n, safe = true) {
        if (n === 0n)
            return safe === true ? err('cannot multiply by 0') : I;
        if (!(typeof n === 'bigint' && 0n < n && n < N))
            err('invalid scalar, must be < L');
        if (!safe && this.is0() || n === 1n)
            return this; // safe=true bans 0. safe=false allows 0.
        if (this.equals(G))
            return wNAF(n).p; // use wNAF precomputes for base points
        let p = I, f = G; // init result point & fake point
        for (let d = this; n > 0n; d = d.double(), n >>= 1n) { // double-and-add ladder
            if (n & 1n)
                p = p.add(d); // if bit is present, add to point
            else if (safe)
                f = f.add(d); // if not, add to fake for timing safety
        }
        return p;
    }
    multiply(scalar) { return this.mul(scalar); } // Aliases for compatibilty
    clearCofactor() { return this.mul(BigInt(CURVE.h), false); } // multiply by cofactor
    isSmallOrder() { return this.clearCofactor().is0(); } // check if P is small order
    isTorsionFree() {
        let p = this.mul(N / 2n, false).double(); // ensures the point is not "bad".
        if (N % 2n)
            p = p.add(this); // P^(N+1)             // P*N == (P*(N/2))*2+P
        return p.is0();
    }
    toAffine() {
        const { ex: x, ey: y, ez: z } = this; // (x, y, z, t) ∋ (x=x/z, y=y/z, t=xy)
        if (this.equals(I))
            return { x: 0n, y: 1n }; // fast-path for zero point
        const iz = invert(z); // z^-1: invert z
        if (mod(z * iz) !== 1n)
            err('invalid inverse'); // (z * z^-1) must be 1, otherwise bad math
        return { x: mod(x * iz), y: mod(y * iz) }; // x = x*z^-1; y = y*z^-1
    }
    toRawBytes() {
        const { x, y } = this.toAffine(); // convert to affine 2d point
        const b = n2b_32LE(y); // encode number to 32 bytes
        b[31] |= x & 1n ? 0x80 : 0; // store sign in first LE byte
        return b;
    }
    toHex() { return b2h(this.toRawBytes()); } // encode to hex string
}
Point.BASE = new Point(Gx, Gy, 1n, mod(Gx * Gy)); // Generator / Base point
Point.ZERO = new Point(0n, 1n, 1n, 0n); // Identity / Zero point
const { BASE: G, ZERO: I } = Point; // Generator, identity points
const padh = (num, pad) => num.toString(16).padStart(pad, '0');
const b2h = (b) => Array.from(b).map(e => padh(e, 2)).join(''); // bytes to hex
const h2b = (hex) => {
    const l = hex.length; // error if not string,
    if (!str(hex) || l % 2)
        err('hex invalid 1'); // or has odd length like 3, 5.
    const arr = u8n(l / 2); // create result array
    for (let i = 0; i < arr.length; i++) {
        const j = i * 2;
        const h = hex.slice(j, j + 2); // hexByte. slice is faster than substr
        const b = Number.parseInt(h, 16); // byte, created from string part
        if (Number.isNaN(b) || b < 0)
            err('hex invalid 2'); // byte must be valid 0 <= byte < 256
        arr[i] = b;
    }
    return arr;
};
const n2b_32LE = (num) => h2b(padh(num, 32 * 2)).reverse(); // number to bytes LE
const b2n_LE = (b) => BigInt('0x' + b2h(u8n(au8(b)).reverse())); // bytes LE to num
const concatB = (...arrs) => {
    const r = u8n(arrs.reduce((sum, a) => sum + au8(a).length, 0)); // create u8a of summed length
    let pad = 0; // walk through each array,
    arrs.forEach(a => { r.set(a, pad); pad += a.length; }); // ensure they have proper type
    return r;
};
const invert = (num, md = P) => {
    if (num === 0n || md <= 0n)
        err('no inverse n=' + num + ' mod=' + md); // no neg exponent for now
    let a = mod(num, md), b = md, x = 0n, y = 1n, u = 1n, v = 0n;
    while (a !== 0n) { // uses euclidean gcd algorithm
        const q = b / a, r = b % a; // not constant-time
        const m = x - u * q, n = y - v * q;
        b = a, a = r, x = u, y = v, u = m, v = n;
    }
    return b === 1n ? mod(x, md) : err('no inverse'); // b is gcd at this point
};
const pow2 = (x, power) => {
    let r = x;
    while (power-- > 0n) {
        r *= r;
        r %= P;
    }
    return r;
};
const pow_2_252_3 = (x) => {
    const x2 = (x * x) % P; // x^2,       bits 1
    const b2 = (x2 * x) % P; // x^3,       bits 11
    const b4 = (pow2(b2, 2n) * b2) % P; // x^(2^4-1), bits 1111
    const b5 = (pow2(b4, 1n) * x) % P; // x^(2^5-1), bits 11111
    const b10 = (pow2(b5, 5n) * b5) % P; // x^(2^10)
    const b20 = (pow2(b10, 10n) * b10) % P; // x^(2^20)
    const b40 = (pow2(b20, 20n) * b20) % P; // x^(2^40)
    const b80 = (pow2(b40, 40n) * b40) % P; // x^(2^80)
    const b160 = (pow2(b80, 80n) * b80) % P; // x^(2^160)
    const b240 = (pow2(b160, 80n) * b80) % P; // x^(2^240)
    const b250 = (pow2(b240, 10n) * b10) % P; // x^(2^250)
    const pow_p_5_8 = (pow2(b250, 2n) * x) % P; // < To pow to (p+3)/8, multiply it by x.
    return { pow_p_5_8, b2 };
};
const RM1 = 19681161376707505956807079304988542015446066515923890162744021073123829784752n; // √-1
const uvRatio = (u, v) => {
    const v3 = mod(v * v * v); // v³
    const v7 = mod(v3 * v3 * v); // v⁷
    const pow = pow_2_252_3(u * v7).pow_p_5_8; // (uv⁷)^(p-5)/8
    let x = mod(u * v3 * pow); // (uv³)(uv⁷)^(p-5)/8
    const vx2 = mod(v * x * x); // vx²
    const root1 = x; // First root candidate
    const root2 = mod(x * RM1); // Second root candidate; RM1 is √-1
    const useRoot1 = vx2 === u; // If vx² = u (mod p), x is a square root
    const useRoot2 = vx2 === mod(-u); // If vx² = -u, set x <-- x * 2^((p-1)/4)
    const noRoot = vx2 === mod(-u * RM1); // There is no valid root, vx² = -u√-1
    if (useRoot1)
        x = root1;
    if (useRoot2 || noRoot)
        x = root2; // We return root2 anyway, for const-time
    if ((mod(x) & 1n) === 1n)
        x = mod(-x); // edIsNegative
    return { isValid: useRoot1 || useRoot2, value: x };
};
const modL_LE = (hash) => mod(b2n_LE(hash), N); // modulo L; but little-endian
let _shaS;
const sha512a = (...m) => etc.sha512Async(...m); // Async SHA512
const sha512s = (...m) => // Sync SHA512, not set by default
 typeof _shaS === 'function' ? _shaS(...m) : err('etc.sha512Sync not set');
const hash2extK = (hashed) => {
    const head = hashed.slice(0, 32); // slice creates a copy, unlike subarray
    head[0] &= 248; // Clamp bits: 0b1111_1000,
    head[31] &= 127; // 0b0111_1111,
    head[31] |= 64; // 0b0100_0000
    const prefix = hashed.slice(32, 64); // private key "prefix"
    const scalar = modL_LE(head); // modular division over curve order
    const point = G.mul(scalar); // public key point
    const pointBytes = point.toRawBytes(); // point serialized to Uint8Array
    return { head, prefix, scalar, point, pointBytes };
};
// RFC8032 5.1.5; getPublicKey async, sync. Hash priv key and extract point.
const getExtendedPublicKeyAsync = (priv) => sha512a(toU8(priv, 32)).then(hash2extK);
const getExtendedPublicKey = (priv) => hash2extK(sha512s(toU8(priv, 32)));
const getPublicKeyAsync = (priv) => getExtendedPublicKeyAsync(priv).then(p => p.pointBytes);
const getPublicKey = (priv) => getExtendedPublicKey(priv).pointBytes;
function hashFinish(asynchronous, res) {
    if (asynchronous)
        return sha512a(res.hashable).then(res.finish);
    return res.finish(sha512s(res.hashable));
}
const _sign = (e, rBytes, msg) => {
    const { pointBytes: P, scalar: s } = e;
    const r = modL_LE(rBytes); // r was created outside, reduce it modulo L
    const R = G.mul(r).toRawBytes(); // R = [r]B
    const hashable = concatB(R, P, msg); // dom2(F, C) || R || A || PH(M)
    const finish = (hashed) => {
        const S = mod(r + modL_LE(hashed) * s, N); // S = (r + k * s) mod L; 0 <= s < l
        return au8(concatB(R, n2b_32LE(S)), 64); // 64-byte sig: 32b R.x + 32b LE(S)
    };
    return { hashable, finish };
};
const signAsync = async (msg, privKey) => {
    const m = toU8(msg); // RFC8032 5.1.6: sign msg with key async
    const e = await getExtendedPublicKeyAsync(privKey); // pub,prfx
    const rBytes = await sha512a(e.prefix, m); // r = SHA512(dom2(F, C) || prefix || PH(M))
    return hashFinish(true, _sign(e, rBytes, m)); // gen R, k, S, then 64-byte signature
};
const sign = (msg, privKey) => {
    const m = toU8(msg); // RFC8032 5.1.6: sign msg with key sync
    const e = getExtendedPublicKey(privKey); // pub,prfx
    const rBytes = sha512s(e.prefix, m); // r = SHA512(dom2(F, C) || prefix || PH(M))
    return hashFinish(false, _sign(e, rBytes, m)); // gen R, k, S, then 64-byte signature
};
const dvo = { zip215: true };
const _verify = (sig, msg, pub, opts = dvo) => {
    msg = toU8(msg); // Message hex str/Bytes
    sig = toU8(sig, 64); // Signature hex str/Bytes, must be 64 bytes
    const { zip215 } = opts; // switch between zip215 and rfc8032 verif
    let A, R, s, SB, hashable = new Uint8Array();
    try {
        A = Point.fromHex(pub, zip215); // public key A decoded
        R = Point.fromHex(sig.slice(0, 32), zip215); // 0 <= R < 2^256: ZIP215 R can be >= P
        s = b2n_LE(sig.slice(32, 64)); // Decode second half as an integer S
        SB = G.mul(s, false); // in the range 0 <= s < L
        hashable = concatB(R.toRawBytes(), A.toRawBytes(), msg); // dom2(F, C) || R || A || PH(M)
    }
    catch (error) { }
    const finish = (hashed) => {
        if (SB == null)
            return false; // false if try-catch catched an error
        if (!zip215 && A.isSmallOrder())
            return false; // false for SBS: Strongly Binding Signature
        const k = modL_LE(hashed); // decode in little-endian, modulo L
        const RkA = R.add(A.mul(k, false)); // [8]R + [8][k]A'
        return RkA.add(SB.negate()).clearCofactor().is0(); // [8][S]B = [8]R + [8][k]A'
    };
    return { hashable, finish };
};
// RFC8032 5.1.7: verification async, sync
const verifyAsync = async (s, m, p, opts = dvo) => hashFinish(true, _verify(s, m, p, opts));
const verify = (s, m, p, opts = dvo) => hashFinish(false, _verify(s, m, p, opts));
const cr = () => // We support: 1) browsers 2) node.js 19+
 typeof globalThis === 'object' && 'crypto' in globalThis ? globalThis.crypto : undefined;
const etc = {
    bytesToHex: b2h, hexToBytes: h2b, concatBytes: concatB,
    mod, invert,
    randomBytes: (len = 32) => {
        const crypto = cr(); // Can be shimmed in node.js <= 18 to prevent error:
        // import { webcrypto } from 'node:crypto';
        // if (!globalThis.crypto) globalThis.crypto = webcrypto;
        if (!crypto || !crypto.getRandomValues)
            err('crypto.getRandomValues must be defined');
        return crypto.getRandomValues(u8n(len));
    },
    sha512Async: async (...messages) => {
        const crypto = cr();
        if (!crypto || !crypto.subtle)
            err('crypto.subtle or etc.sha512Async must be defined');
        const m = concatB(...messages);
        return u8n(await crypto.subtle.digest('SHA-512', m.buffer));
    },
    sha512Sync: undefined, // Actual logic below
};
Object.defineProperties(etc, { sha512Sync: {
        configurable: false, get() { return _shaS; }, set(f) { if (!_shaS)
            _shaS = f; },
    } });
const utils = {
    getExtendedPublicKeyAsync, getExtendedPublicKey,
    randomPrivateKey: () => etc.randomBytes(32),
    precompute(w = 8, p = G) { p.multiply(3n); w; return p; }, // no-op
};
const W = 8; // Precomputes-related code. W = window size
const precompute = () => {
    const points = []; // 10x sign(), 2x verify(). To achieve this,
    const windows = 256 / W + 1; // app needs to spend 40ms+ to calculate
    let p = G, b = p; // a lot of points related to base point G.
    for (let w = 0; w < windows; w++) { // Points are stored in array and used
        b = p; // any time Gx multiplication is done.
        points.push(b); // They consume 16-32 MiB of RAM.
        for (let i = 1; i < 2 ** (W - 1); i++) {
            b = b.add(p);
            points.push(b);
        }
        p = b.double(); // Precomputes don't speed-up getSharedKey,
    } // which multiplies user point by scalar,
    return points; // when precomputes are using base point
};
let Gpows = undefined; // precomputes for base point G
const wNAF = (n) => {
    // Compared to other point mult methods,
    const comp = Gpows || (Gpows = precompute()); // stores 2x less points using subtraction
    const neg = (cnd, p) => { let n = p.negate(); return cnd ? n : p; }; // negate
    let p = I, f = G; // f must be G, or could become I in the end
    const windows = 1 + 256 / W; // W=8 17 windows
    const wsize = 2 ** (W - 1); // W=8 128 window size
    const mask = BigInt(2 ** W - 1); // W=8 will create mask 0b11111111
    const maxNum = 2 ** W; // W=8 256
    const shiftBy = BigInt(W); // W=8 8
    for (let w = 0; w < windows; w++) {
        const off = w * wsize;
        let wbits = Number(n & mask); // extract W bits.
        n >>= shiftBy; // shift number by W bits.
        if (wbits > wsize) {
            wbits -= maxNum;
            n += 1n;
        } // split if bits > max: +224 => 256-32
        const off1 = off, off2 = off + Math.abs(wbits) - 1; // offsets, evaluate both
        const cnd1 = w % 2 !== 0, cnd2 = wbits < 0; // conditions, evaluate both
        if (wbits === 0) {
            f = f.add(neg(cnd1, comp[off1])); // bits are 0: add garbage to fake point
        }
        else { //          ^ can't add off2, off2 = I
            p = p.add(neg(cnd2, comp[off2])); // bits are 1: add to result point
        }
    }
    return { p, f }; // return both real and fake points for JIT
}; // !! you can disable precomputes by commenting-out call of the wNAF() inside Point#mul()
export { getPublicKey, getPublicKeyAsync, sign, verify, // Remove the export to easily use in REPL
signAsync, verifyAsync, CURVE, etc, utils, Point as ExtendedPoint }; // envs like browser console
