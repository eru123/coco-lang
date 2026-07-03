// arith.c — adaptive numeric tower: arithmetic that dispatches on operand tags
// and picks the fastest tier that keeps the result exact.
//
//   int + int (no overflow)  -> native i64 op        (Tier 0, fastest)
//   int + int (overflow)     -> bignum escalation    (Tier 1, exact)
//   float involved           -> f64                  (Tier 2)
//   bigint involved          -> bignum               (Tier 1)
//   string + string (+)      -> concatenation        (Tier 3)
//   type mismatch            -> abort with diagnostic
//
// The codegen emits direct native ops for statically-typed operands and calls
// these for dynamically-typed ones; both paths honor the same tiering.

#include "coco_rt.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void die(const char *msg) { fprintf(stderr, "coco_rt: %s\n", msg); abort(); }

// Is the value an integer type (int or bigint)?
static bool is_int(const coco_val *v) { return v->tag == COCO_INT || v->tag == COCO_BIGINT; }

// Promote an int/bigint to a fresh bignum.
static coco_bigint *to_bi(const coco_val *v) {
    if (v->tag == COCO_INT) return coco_bi_from_i64(v->u.i);
    if (v->tag == COCO_BIGINT) { coco_bi_retain(v->u.bi); return v->u.bi; }
    die("to_bi: not an integer");
    return NULL;
}

// Try an i64 add with overflow check; returns true and sets *out if no overflow.
static bool i64_add(int64_t a, int64_t b, int64_t *out) {
#if defined(__GNUC__) || defined(__clang__)
    return !__builtin_add_overflow(a, b, out);
#else
    *out = (uint64_t)a + (uint64_t)b;
    if (((a ^ *out) & (b ^ *out)) < 0) return false; // overflow
    return true;
#endif
}
static bool i64_sub(int64_t a, int64_t b, int64_t *out) {
#if defined(__GNUC__) || defined(__clang__)
    return !__builtin_sub_overflow(a, b, out);
#else
    *out = (uint64_t)a - (uint64_t)b;
    if (((a ^ b) & (a ^ *out)) < 0) return false;
    return true;
#endif
}
static bool i64_mul(int64_t a, int64_t b, int64_t *out) {
#if defined(__GNUC__) || defined(__clang__)
    return !__builtin_mul_overflow(a, b, out);
#else
    *out = (int64_t)((uint64_t)a * (uint64_t)b);
    if (a != 0 && *out / a != b) return false;
    return true;
#endif
}

// Integer add: i64 fast path, overflow -> bignum.
static coco_val *int_add(const coco_val *a, const coco_val *b) {
    if (a->tag == COCO_INT && b->tag == COCO_INT) {
        int64_t r;
        if (i64_add(a->u.i, b->u.i, &r)) return coco_make_int(r);
        // Overflow: escalate to bignum (exact).
        coco_bigint *ba = coco_bi_from_i64(a->u.i);
        coco_bigint *bb = coco_bi_from_i64(b->u.i);
        coco_bigint *br = coco_bi_add(ba, bb);
        coco_bi_release(ba); coco_bi_release(bb);
        return coco_make_bigint(br);
    }
    // At least one is bigint.
    coco_bigint *ba = to_bi(a);
    coco_bigint *bb = to_bi(b);
    coco_bigint *br = coco_bi_add(ba, bb);
    coco_bi_release(ba); coco_bi_release(bb);
    return coco_make_bigint(br);
}

static coco_val *int_sub(const coco_val *a, const coco_val *b) {
    if (a->tag == COCO_INT && b->tag == COCO_INT) {
        int64_t r;
        if (i64_sub(a->u.i, b->u.i, &r)) return coco_make_int(r);
        coco_bigint *ba = coco_bi_from_i64(a->u.i);
        coco_bigint *bb = coco_bi_from_i64(b->u.i);
        coco_bigint *br = coco_bi_sub(ba, bb);
        coco_bi_release(ba); coco_bi_release(bb);
        return coco_make_bigint(br);
    }
    coco_bigint *ba = to_bi(a);
    coco_bigint *bb = to_bi(b);
    coco_bigint *br = coco_bi_sub(ba, bb);
    coco_bi_release(ba); coco_bi_release(bb);
    return coco_make_bigint(br);
}

static coco_val *int_mul(const coco_val *a, const coco_val *b) {
    if (a->tag == COCO_INT && b->tag == COCO_INT) {
        int64_t r;
        if (i64_mul(a->u.i, b->u.i, &r)) return coco_make_int(r);
        coco_bigint *ba = coco_bi_from_i64(a->u.i);
        coco_bigint *bb = coco_bi_from_i64(b->u.i);
        coco_bigint *br = coco_bi_mul(ba, bb);
        coco_bi_release(ba); coco_bi_release(bb);
        return coco_make_bigint(br);
    }
    coco_bigint *ba = to_bi(a);
    coco_bigint *bb = to_bi(b);
    coco_bigint *br = coco_bi_mul(ba, bb);
    coco_bi_release(ba); coco_bi_release(bb);
    return coco_make_bigint(br);
}

static coco_val *int_div(const coco_val *a, const coco_val *b) {
    if (b->tag == COCO_INT ? b->u.i == 0 : (b->tag == COCO_BIGINT && b->u.bi->len == 0))
        die("division by zero");
    if (a->tag == COCO_INT && b->tag == COCO_INT) {
        // INT64_MIN / -1 overflows; escalate.
        if (a->u.i == INT64_MIN && b->u.i == -1) {
            coco_bigint *ba = coco_bi_from_i64(a->u.i);
            coco_bigint *bb = coco_bi_from_i64(b->u.i);
            coco_bigint *br = coco_bi_div(ba, bb);
            coco_bi_release(ba); coco_bi_release(bb);
            return coco_make_bigint(br);
        }
        return coco_make_int(a->u.i / b->u.i);
    }
    coco_bigint *ba = to_bi(a);
    coco_bigint *bb = to_bi(b);
    coco_bigint *br = coco_bi_div(ba, bb);
    coco_bi_release(ba); coco_bi_release(bb);
    return coco_make_bigint(br);
}

static coco_val *int_mod(const coco_val *a, const coco_val *b) {
    if (b->tag == COCO_INT ? b->u.i == 0 : (b->tag == COCO_BIGINT && b->u.bi->len == 0))
        die("modulo by zero");
    if (a->tag == COCO_INT && b->tag == COCO_INT) {
        // C % is truncated toward zero; remainder takes sign of dividend,
        // matching the interpreter's int modulo.
        return coco_make_int(a->u.i % b->u.i);
    }
    coco_bigint *ba = to_bi(a);
    coco_bigint *bb = to_bi(b);
    coco_bigint *br = coco_bi_mod(ba, bb);
    coco_bi_release(ba); coco_bi_release(bb);
    return coco_make_bigint(br);
}

// --- Public dispatch -------------------------------------------------------

coco_val *coco_add(coco_val *a, coco_val *b) {
    // String concatenation tier.
    if (a->tag == COCO_STRING && b->tag == COCO_STRING)
        return coco_str_concat(a, b);
    // Float tier: any float operand -> f64 result.
    if (a->tag == COCO_FLOAT || b->tag == COCO_FLOAT) {
        double av = (a->tag == COCO_FLOAT) ? a->u.f : coco_int_to_f64(a);
        double bv = (b->tag == COCO_FLOAT) ? b->u.f : coco_int_to_f64(b);
        return coco_make_float(av + bv);
    }
    // Integer tier (i64 fast path + bignum escalation).
    if (is_int(a) && is_int(b)) return int_add(a, b);
    die("coco_add: unsupported operand types");
    return NULL;
}

coco_val *coco_sub(coco_val *a, coco_val *b) {
    if (a->tag == COCO_FLOAT || b->tag == COCO_FLOAT) {
        double av = (a->tag == COCO_FLOAT) ? a->u.f : coco_int_to_f64(a);
        double bv = (b->tag == COCO_FLOAT) ? b->u.f : coco_int_to_f64(b);
        return coco_make_float(av - bv);
    }
    if (is_int(a) && is_int(b)) return int_sub(a, b);
    die("coco_sub: unsupported operand types");
    return NULL;
}

coco_val *coco_mul(coco_val *a, coco_val *b) {
    if (a->tag == COCO_FLOAT || b->tag == COCO_FLOAT) {
        double av = (a->tag == COCO_FLOAT) ? a->u.f : coco_int_to_f64(a);
        double bv = (b->tag == COCO_FLOAT) ? b->u.f : coco_int_to_f64(b);
        return coco_make_float(av * bv);
    }
    if (is_int(a) && is_int(b)) return int_mul(a, b);
    die("coco_mul: unsupported operand types");
    return NULL;
}

coco_val *coco_div(coco_val *a, coco_val *b) {
    if (a->tag == COCO_FLOAT || b->tag == COCO_FLOAT) {
        double av = (a->tag == COCO_FLOAT) ? a->u.f : coco_int_to_f64(a);
        double bv = (b->tag == COCO_FLOAT) ? b->u.f : coco_int_to_f64(b);
        return coco_make_float(av / bv); // IEEE inf/nan for div-by-zero
    }
    if (is_int(a) && is_int(b)) return int_div(a, b);
    die("coco_div: unsupported operand types");
    return NULL;
}

coco_val *coco_mod(coco_val *a, coco_val *b) {
    if (a->tag == COCO_FLOAT || b->tag == COCO_FLOAT) {
        double av = (a->tag == COCO_FLOAT) ? a->u.f : coco_int_to_f64(a);
        double bv = (b->tag == COCO_FLOAT) ? b->u.f : coco_int_to_f64(b);
        return coco_make_float(av - bv * (double)((int64_t)(av / bv)));
    }
    if (is_int(a) && is_int(b)) return int_mod(a, b);
    die("coco_mod: unsupported operand types");
    return NULL;
}

// --- Comparisons -----------------------------------------------------------
// Equality is type-strict per the interpreter: 1 != 1.0, 1 != "1".

bool coco_eq(const coco_val *a, const coco_val *b) {
    if (a == b) return true;
    // Integer types compare by value across int/bigint.
    if (is_int(a) && is_int(b)) {
        coco_bigint *ba = to_bi(a);
        coco_bigint *bb = to_bi(b);
        bool eq = coco_bi_cmp(ba, bb) == 0;
        coco_bi_release(ba); coco_bi_release(bb);
        return eq;
    }
    if (a->tag != b->tag) return false;
    switch (a->tag) {
        case COCO_FLOAT:  return a->u.f == b->u.f;
        case COCO_BOOL:   return a->u.b == b->u.b;
        case COCO_NULL:   return true;
        case COCO_STRING: return a->u.s->len == b->u.s->len &&
                                  memcmp(a->u.s->data, b->u.s->data, (size_t)a->u.s->len) == 0;
        case COCO_LIST:   die("coco_eq: list equality not implemented"); break;
        case COCO_MAP:    die("coco_eq: map equality not implemented"); break;
        default: return false;
    }
}

int coco_cmp(const coco_val *a, const coco_val *b) {
    // Integer comparison across int/bigint.
    if (is_int(a) && is_int(b)) {
        coco_bigint *ba = to_bi(a);
        coco_bigint *bb = to_bi(b);
        int c = coco_bi_cmp(ba, bb);
        coco_bi_release(ba); coco_bi_release(bb);
        return c < 0 ? -1 : (c > 0 ? 1 : 0);
    }
    if (a->tag == COCO_FLOAT && b->tag == COCO_FLOAT) {
        if (a->u.f < b->u.f) return -1;
        if (a->u.f > b->u.f) return 1;
        return 0;
    }
    if (a->tag == COCO_STRING && b->tag == COCO_STRING) {
        int64_t n = a->u.s->len < b->u.s->len ? a->u.s->len : b->u.s->len;
        int c = memcmp(a->u.s->data, b->u.s->data, (size_t)n);
        if (c != 0) return c < 0 ? -1 : 1;
        if (a->u.s->len < b->u.s->len) return -1;
        if (a->u.s->len > b->u.s->len) return 1;
        return 0;
    }
    die("coco_cmp: unsupported operand types");
    return 0;
}

// --- Unary -----------------------------------------------------------------

coco_val *coco_neg(const coco_val *a) {
    if (a->tag == COCO_INT) {
        if (a->u.i == INT64_MIN) {
            // -INT64_MIN overflows -> bignum.
            coco_bigint *bi = coco_bi_from_i64(a->u.i);
            bi->neg = !bi->neg;
            if (bi->len == 0) bi->neg = false;
            return coco_make_bigint(bi);
        }
        return coco_make_int(-a->u.i);
    }
    if (a->tag == COCO_FLOAT) return coco_make_float(-a->u.f);
    if (a->tag == COCO_BIGINT) {
        coco_bigint *bi = to_bi(a);
        bi->neg = !bi->neg;
        if (bi->len == 0) bi->neg = false;
        return coco_make_bigint(bi);
    }
    die("coco_neg: unsupported operand type");
    return NULL;
}

coco_val *coco_not(const coco_val *a) {
    return coco_make_bool(!coco_is_truthy(a));
}
