// bignum.c — minimal sign-magnitude bignum arithmetic.
//
// This is the bignum tier of Coco's adaptive numeric tower: it is reached only
// when an i64 operation would overflow (correctness-gated escalation), so
// correctness matters more than speed. A full Karatsuba/FFT implementation is
// a later optimization; this is schoolbook arithmetic on uint32 limbs.

#include "coco_rt.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void die(const char *msg) { fprintf(stderr, "coco_rt: %s\n", msg); abort(); }

// Compare magnitudes |a| vs |b|. Returns -1/0/1.
static int mag_cmp(const coco_bigint *a, const coco_bigint *b) {
    if (a->len != b->len) return a->len < b->len ? -1 : 1;
    for (int64_t i = a->len - 1; i >= 0; i--) {
        if (a->limbs[i] != b->limbs[i]) return a->limbs[i] < b->limbs[i] ? -1 : 1;
    }
    return 0;
}

// |a| + |b| -> fresh bignum (always non-negative).
static coco_bigint *mag_add(const coco_bigint *a, const coco_bigint *b) {
    int64_t n = (a->len > b->len ? a->len : b->len) + 1;
    coco_bigint *r = coco_bi_new(n);
    coco_bi_reserve(r, n);
    uint64_t carry = 0;
    for (int64_t i = 0; i < n; i++) {
        uint64_t av = (i < a->len) ? a->limbs[i] : 0;
        uint64_t bv = (i < b->len) ? b->limbs[i] : 0;
        uint64_t s = av + bv + carry;
        r->limbs[i] = (uint32_t)(s & 0xFFFFFFFFu);
        carry = s >> 32;
        r->len = i + 1;
    }
    r->neg = false;
    coco_bi_normalize(r);
    return r;
}

// |a| - |b|, requires |a| >= |b|. Result non-negative.
static coco_bigint *mag_sub(const coco_bigint *a, const coco_bigint *b) {
    coco_bigint *r = coco_bi_new(a->len);
    coco_bi_reserve(r, a->len);
    int64_t borrow = 0;
    for (int64_t i = 0; i < a->len; i++) {
        int64_t av = a->limbs[i];
        int64_t bv = (i < b->len) ? (int64_t)b->limbs[i] : 0;
        int64_t d = av - bv - borrow;
        if (d < 0) { d += ((int64_t)1 << 32); borrow = 1; } else { borrow = 0; }
        r->limbs[i] = (uint32_t)d;
        r->len = i + 1;
    }
    r->neg = false;
    coco_bi_normalize(r);
    return r;
}

// Signed add via magnitude add/sub depending on signs.
coco_bigint *coco_bi_add(const coco_bigint *a, const coco_bigint *b) {
    if (a->len == 0) { coco_bigint *r = coco_bi_new(b->len); coco_bi_reserve(r, b->len); memcpy(r->limbs, b->limbs, (size_t)b->len * 4); r->len = b->len; r->neg = b->neg; coco_bi_normalize(r); return r; }
    if (b->len == 0) { coco_bigint *r = coco_bi_new(a->len); coco_bi_reserve(r, a->len); memcpy(r->limbs, a->limbs, (size_t)a->len * 4); r->len = a->len; r->neg = a->neg; coco_bi_normalize(r); return r; }
    if (a->neg == b->neg) {
        coco_bigint *r = mag_add(a, b);
        r->neg = a->neg;
        coco_bi_normalize(r);
        return r;
    }
    // Different signs: subtract smaller magnitude from larger.
    int c = mag_cmp(a, b);
    if (c == 0) { return coco_bi_new(1); } // result is zero
    if (c > 0) {
        coco_bigint *r = mag_sub(a, b);
        r->neg = a->neg;
        coco_bi_normalize(r);
        return r;
    } else {
        coco_bigint *r = mag_sub(b, a);
        r->neg = b->neg;
        coco_bi_normalize(r);
        return r;
    }
}

coco_bigint *coco_bi_sub(const coco_bigint *a, const coco_bigint *b) {
    // a - b = a + (-b). Make a negated copy of b.
    coco_bigint nb;
    nb = *b;
    nb.neg = !b->neg;
    if (b->len == 0) nb.neg = false;
    return coco_bi_add(a, &nb);
}

// Schoolbook multiply.
coco_bigint *coco_bi_mul(const coco_bigint *a, const coco_bigint *b) {
    if (a->len == 0 || b->len == 0) return coco_bi_new(1);
    int64_t n = a->len + b->len;
    coco_bigint *r = coco_bi_new(n);
    coco_bi_reserve(r, n);
    for (int64_t i = 0; i < n; i++) r->limbs[i] = 0;
    r->len = n;
    for (int64_t i = 0; i < a->len; i++) {
        uint64_t carry = 0;
        uint64_t av = a->limbs[i];
        for (int64_t j = 0; j < b->len; j++) {
            uint64_t cur = (uint64_t)r->limbs[i + j] + av * (uint64_t)b->limbs[j] + carry;
            r->limbs[i + j] = (uint32_t)(cur & 0xFFFFFFFFu);
            carry = cur >> 32;
        }
        r->limbs[i + b->len] += (uint32_t)carry;
    }
    r->neg = (a->neg != b->neg);
    coco_bi_normalize(r);
    return r;
}

int coco_bi_cmp(const coco_bigint *a, const coco_bigint *b) {
    if (a->len == 0 && b->len == 0) return 0;
    if (a->len == 0) return b->neg ? 1 : -1;
    if (b->len == 0) return a->neg ? -1 : 1;
    if (a->neg != b->neg) return a->neg ? -1 : 1;
    int c = mag_cmp(a, b);
    return a->neg ? -c : c;
}

// Long division: |a| / |b| -> quotient, remainder via out params. Both fresh.
// Truncation toward zero (remainder takes sign of dividend).
static void mag_divmod(const coco_bigint *a, const coco_bigint *b,
                       coco_bigint **q_out, coco_bigint **r_out) {
    if (b->len == 0) die("division by zero");
    if (mag_cmp(a, b) < 0) {
        // a < b: quotient 0, remainder a.
        coco_bigint *q = coco_bi_new(1);
        coco_bigint *r = coco_bi_new(a->len);
        coco_bi_reserve(r, a->len);
        memcpy(r->limbs, a->limbs, (size_t)a->len * 4);
        r->len = a->len;
        coco_bi_normalize(r);
        *q_out = q; *r_out = r;
        return;
    }
    // Bit-by-bit long division (simple, correct; not fast).
    int64_t nbits = a->len * 32;
    coco_bigint *q = coco_bi_new(a->len);
    coco_bi_reserve(q, a->len);
    q->len = a->len;
    for (int64_t i = 0; i < q->len; i++) q->limbs[i] = 0;
    coco_bigint *rem = coco_bi_new(a->len);
    coco_bi_reserve(rem, a->len);
    rem->len = 0;
    for (int64_t bit = nbits - 1; bit >= 0; bit--) {
        // rem = rem << 1
        coco_bi_reserve(rem, rem->len + 1);
        uint32_t carry = 0;
        for (int64_t i = 0; i < rem->len; i++) {
            uint64_t v = ((uint64_t)rem->limbs[i] << 1) | carry;
            rem->limbs[i] = (uint32_t)(v & 0xFFFFFFFFu);
            carry = (uint32_t)(v >> 32);
        }
        if (carry) { rem->limbs[rem->len++] = carry; }
        // bring in bit `bit` of a
        uint32_t abit = (a->limbs[bit / 32] >> (bit % 32)) & 1u;
        if (abit) {
            if (rem->len == 0) { rem->len = 1; rem->limbs[0] = 1; }
            else { rem->limbs[0] |= 1u; }
        }
        coco_bi_normalize(rem);
        if (mag_cmp(rem, b) >= 0) {
            // rem -= b (magnitudes, rem >= b)
            coco_bigint *nr = mag_sub(rem, b);
            coco_bi_release(rem);
            rem = nr;
            // set quotient bit
            q->limbs[bit / 32] |= (1u << (bit % 32));
        }
    }
    coco_bi_normalize(q);
    coco_bi_normalize(rem);
    *q_out = q; *r_out = rem;
}

coco_bigint *coco_bi_div(const coco_bigint *a, const coco_bigint *b) {
    coco_bigint *q, *r;
    mag_divmod(a, b, &q, &r);
    q->neg = (a->neg != b->neg) && q->len != 0;
    coco_bi_release(r);
    return q;
}

coco_bigint *coco_bi_mod(const coco_bigint *a, const coco_bigint *b) {
    coco_bigint *q, *r;
    mag_divmod(a, b, &q, &r);
    r->neg = a->neg && r->len != 0; // remainder takes sign of dividend
    coco_bi_release(q);
    return r;
}
