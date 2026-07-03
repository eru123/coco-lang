// value.c — core tagged value construction, refcounting, and truthiness.

#include "coco_rt.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Abort with a diagnostic. Used for invariant violations and unsupported ops.
static void coco_die(const char *msg) {
    fprintf(stderr, "coco_rt: %s\n", msg);
    abort();
}

static void *xmalloc(size_t n) {
    void *p = malloc(n ? n : 1);
    if (!p) coco_die("out of memory");
    return p;
}

static void *xcalloc(size_t n, size_t sz) {
    void *p = calloc(n ? n : 1, sz);
    if (!p) coco_die("out of memory");
    return p;
}

static void *xrealloc(void *p, size_t n) {
    void *q = realloc(p, n ? n : 1);
    if (!q) coco_die("out of memory");
    return q;
}

// --- String heap objects ---------------------------------------------------

static coco_str *str_new(const char *data, int64_t len) {
    coco_str *s = xmalloc(sizeof(coco_str) + (size_t)len + 1);
    s->refcount = 1;
    s->len = len;
    if (len > 0 && data) memcpy(s->data, data, (size_t)len);
    s->data[len] = '\0';
    return s;
}

static void str_retain(coco_str *s) { if (s) s->refcount++; }
static void str_release(coco_str *s) {
    if (!s) return;
    if (--s->refcount <= 0) free(s);
}

// --- Bignum heap objects ---------------------------------------------------

static coco_bigint *bi_new(int64_t cap) {
    if (cap < 1) cap = 1;
    coco_bigint *bi = xmalloc(sizeof(coco_bigint));
    bi->refcount = 1;
    bi->neg = false;
    bi->len = 0;
    bi->cap = cap;
    bi->limbs = xcalloc((size_t)cap, sizeof(uint32_t));
    return bi;
}

static void bi_retain(coco_bigint *bi) { if (bi) bi->refcount++; }

static void bi_release(coco_bigint *bi) {
    if (!bi) return;
    if (--bi->refcount <= 0) {
        free(bi->limbs);
        free(bi);
    }
}

static void bi_reserve(coco_bigint *bi, int64_t cap) {
    if (bi->cap >= cap) return;
    int64_t nc = bi->cap;
    while (nc < cap) nc *= 2;
    bi->limbs = xrealloc(bi->limbs, (size_t)nc * sizeof(uint32_t));
    for (int64_t i = bi->cap; i < nc; i++) bi->limbs[i] = 0;
    bi->cap = nc;
}

// Normalize: drop high zero limbs. Zero is len==0, neg==false.
static void bi_normalize(coco_bigint *bi) {
    while (bi->len > 0 && bi->limbs[bi->len - 1] == 0) bi->len--;
    if (bi->len == 0) bi->neg = false;
}

// Build a bignum from an i64.
static coco_bigint *bi_from_i64(int64_t v) {
    uint64_t u;
    bool neg = false;
    if (v < 0) {
        neg = true;
        u = (uint64_t)(-(v + 1)) + 1; // handle INT64_MIN
    } else {
        u = (uint64_t)v;
    }
    coco_bigint *bi = bi_new(4);
    while (u != 0) {
        bi_reserve(bi, bi->len + 1);
        bi->limbs[bi->len++] = (uint32_t)(u & 0xFFFFFFFFu);
        u >>= 32;
    }
    bi->neg = neg && bi->len > 0;
    bi_normalize(bi);
    return bi;
}

// Convert bignum to f64 (may lose precision for large values).
static double bi_to_f64(const coco_bigint *bi) {
    double r = 0.0;
    // Horner from most significant limb.
    for (int64_t i = bi->len - 1; i >= 0; i--) {
        r = r * 4294967296.0 + (double)bi->limbs[i];
    }
    return bi->neg ? -r : r;
}

// --- List heap objects -----------------------------------------------------

static void list_retain(coco_list *l) { if (l) l->refcount++; }

static void list_release(coco_list *l) {
    if (!l) return;
    if (--l->refcount <= 0) {
        for (int64_t i = 0; i < l->len; i++) coco_release(l->items[i]);
        free(l->items);
        free(l);
    }
}

// --- Map heap objects ------------------------------------------------------

static void map_retain(coco_map *m) { if (m) m->refcount++; }

static void map_release(coco_map *m) {
    if (!m) return;
    if (--m->refcount <= 0) {
        for (int64_t i = 0; i < m->cap; i++) {
            coco_map_entry *e = &m->entries[i];
            if (e->key) {
                free(e->key);
                coco_release(e->val);
            }
        }
        free(m->entries);
        free(m);
    }
}

// --- Value construction ----------------------------------------------------

coco_val *coco_make_int(int64_t i) {
    coco_val *v = xmalloc(sizeof(coco_val));
    v->tag = COCO_INT;
    v->refcount = 1;
    v->u.i = i;
    return v;
}

coco_val *coco_make_float(double f) {
    coco_val *v = xmalloc(sizeof(coco_val));
    v->tag = COCO_FLOAT;
    v->refcount = 1;
    v->u.f = f;
    return v;
}

coco_val *coco_make_bool(bool b) {
    coco_val *v = xmalloc(sizeof(coco_val));
    v->tag = COCO_BOOL;
    v->refcount = 1;
    v->u.b = b;
    return v;
}

coco_val *coco_make_null(void) {
    coco_val *v = xmalloc(sizeof(coco_val));
    v->tag = COCO_NULL;
    v->refcount = 1;
    v->u.i = 0;
    return v;
}

coco_val *coco_make_string(const char *data, int64_t len) {
    coco_val *v = xmalloc(sizeof(coco_val));
    v->tag = COCO_STRING;
    v->refcount = 1;
    v->u.s = str_new(data, len);
    return v;
}

coco_val *coco_make_string_cstr(const char *cstr) {
    return coco_make_string(cstr, cstr ? (int64_t)strlen(cstr) : 0);
}

// --- Refcounting -----------------------------------------------------------

coco_val *coco_retain(coco_val *v) {
    if (v) v->refcount++;
    return v;
}

void coco_release(coco_val *v) {
    if (!v) return;
    if (--v->refcount > 0) return;
    switch (v->tag) {
        case COCO_BIGINT: bi_release(v->u.bi); break;
        case COCO_STRING: str_release(v->u.s); break;
        case COCO_LIST:   list_release(v->u.l); break;
        case COCO_MAP:    map_release(v->u.m); break;
        default: break; // int/float/bool/null hold no heap object
    }
    free(v);
}

// --- Truthiness (matches interpreter is_truthy) ---------------------------

bool coco_is_truthy(const coco_val *v) {
    if (!v) return false;
    switch (v->tag) {
        case COCO_BOOL:   return v->u.b;
        case COCO_NULL:   return false;
        case COCO_INT:    return v->u.i != 0;
        case COCO_FLOAT:  return v->u.f != 0.0;
        case COCO_STRING: return v->u.s && v->u.s->len > 0;
        case COCO_LIST:   return v->u.l && v->u.l->len > 0;
        case COCO_MAP:    return v->u.m && v->u.m->len > 0;
        case COCO_BIGINT: return !(v->u.bi->len == 0);
    }
    return false;
}

// --- int -> f64 promotion --------------------------------------------------

double coco_int_to_f64(const coco_val *a) {
    if (a->tag == COCO_INT) return (double)a->u.i;
    if (a->tag == COCO_BIGINT) return bi_to_f64(a->u.bi);
    coco_die("coco_int_to_f64: not an integer");
    return 0.0;
}

// Wrap a bignum into a tagged value (takes ownership of bi's ref).
coco_val *coco_make_bigint(coco_bigint *bi) {
    coco_val *v = xmalloc(sizeof(coco_val));
    v->tag = COCO_BIGINT;
    v->refcount = 1;
    v->u.bi = bi;
    return v;
}

// Expose the internal constructors to other translation units.
coco_bigint *coco_bi_from_i64(int64_t v) { return bi_from_i64(v); }
double coco_bi_to_f64(const coco_bigint *bi) { return bi_to_f64(bi); }
void coco_bi_retain(coco_bigint *bi) { bi_retain(bi); }
void coco_bi_release(coco_bigint *bi) { bi_release(bi); }
coco_bigint *coco_bi_new(int64_t cap) { return bi_new(cap); }
void coco_bi_reserve(coco_bigint *bi, int64_t cap) { bi_reserve(bi, cap); }
void coco_bi_normalize(coco_bigint *bi) { bi_normalize(bi); }
