// coco_rt.h — Coco native runtime: tagged value model and adaptive arithmetic.
//
// This is the runtime half of Coco's *adaptive numeric tower*: every value is
// represented in the fastest tier whose result is still exact. The codegen
// emits either a direct native op (when operand types are known statically) or
// a call to one of the `coco_*` functions here (when types are dynamic), which
// dispatch on the value's tag at runtime and pick the appropriate tier.
//
// Value model: refcounted, tagged, heap-allocated. Refcounting (not tracing GC)
// matches the interpreter, where List/Map are Arc-backed and `gc_ref()` returns
// None. Map keys are strings only (matching the interpreter).

#ifndef COCO_RT_H
#define COCO_RT_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

// --- Tags -------------------------------------------------------------------
// tag 0=int(i64 fast path), 1=float(f64), 2=bigint, 3=string, 4=bool,
// 5=null, 6=list, 7=map.
typedef enum {
    COCO_INT = 0,
    COCO_FLOAT = 1,
    COCO_BIGINT = 2,
    COCO_STRING = 3,
    COCO_BOOL = 4,
    COCO_NULL = 5,
    COCO_LIST = 6,
    COCO_MAP = 7,
} coco_tag;

// Forward declarations of heap object types.
typedef struct coco_bigint coco_bigint;
typedef struct coco_str coco_str;
typedef struct coco_list coco_list;
typedef struct coco_map coco_map;
typedef struct coco_map_entry coco_map_entry;

// A Coco string: length-prefixed, refcounted, UTF-8 bytes (not NUL-terminated
// internally; a NUL is kept for C-interop convenience).
struct coco_str {
    int refcount;
    int64_t len;     // byte length, excluding the trailing NUL
    char data[];     // len bytes + a trailing '\0'
};

// A minimal sign-magnitude bignum: array of uint32 limbs, little-endian
// (limbs[0] is least significant), `neg` is the sign. Zero is len==0.
struct coco_bigint {
    int refcount;
    bool neg;
    int64_t len;     // number of used limbs
    int64_t cap;     // allocated limbs
    uint32_t *limbs; // limbs[0..len]
};

struct coco_list {
    int refcount;
    int64_t len;
    int64_t cap;
    struct coco_val **items; // each item is a coco_val* (borrowed ref)
};

struct coco_map_entry {
    char *key;            // owned NUL-terminated string
    int64_t key_len;
    struct coco_val *val; // borrowed ref
    int64_t hash;         // cached hash of key
};

struct coco_map {
    int refcount;
    int64_t len;          // number of live entries
    int64_t cap;          // bucket count (power of two)
    coco_map_entry *entries; // flat array, open addressing
};

// The tagged value. Allocated on the heap and refcounted. The codegen passes
// `coco_val*` around; ownership follows the "caller releases args, callee
// returns a fresh ref" convention used throughout.
typedef struct coco_val {
    coco_tag tag;
    int refcount;
    union {
        int64_t i;          // COCO_INT
        double f;           // COCO_FLOAT
        coco_bigint *bi;    // COCO_BIGINT (owned)
        coco_str *s;        // COCO_STRING (owned)
        bool b;             // COCO_BOOL
        coco_list *l;       // COCO_LIST (owned)
        coco_map *m;        // COCO_MAP (owned)
    } u;
} coco_val;

// --- Construction (each returns a fresh value with refcount 1) -------------
coco_val *coco_make_int(int64_t i);
coco_val *coco_make_float(double f);
coco_val *coco_make_bool(bool b);
coco_val *coco_make_null(void);
coco_val *coco_make_string(const char *data, int64_t len);
coco_val *coco_make_string_cstr(const char *cstr);

// --- Refcounting -----------------------------------------------------------
// coco_retain returns v; coco_release decrements and frees if it hits 0.
coco_val *coco_retain(coco_val *v);
void coco_release(coco_val *v);

// --- Truthiness (matches interpreter is_truthy) ---------------------------
// Bool by value; Null false; Int/Float nonzero; String/List/Map non-empty.
bool coco_is_truthy(const coco_val *v);

// --- Adaptive arithmetic ---------------------------------------------------
// Each dispatches on operand tags and picks the fastest exact tier:
//   int+int (no overflow) -> native i64 op
//   int+int (overflow)    -> bignum escalation (exact)
//   float involved        -> f64
//   string+string (+)     -> concatenation
//   bigint involved       -> bignum
// Mismatched/unsupported combinations abort with a diagnostic.
coco_val *coco_add(coco_val *a, coco_val *b);
coco_val *coco_sub(coco_val *a, coco_val *b);
coco_val *coco_mul(coco_val *a, coco_val *b);
coco_val *coco_div(coco_val *a, coco_val *b);
coco_val *coco_mod(coco_val *a, coco_val *b);

// --- Comparisons (type-strict equality per interpreter: 1 != 1.0) ---------
bool coco_eq(const coco_val *a, const coco_val *b);
int coco_cmp(const coco_val *a, const coco_val *b); // -1/0/1, aborts on mismatch

// --- Unary -----------------------------------------------------------------
coco_val *coco_neg(const coco_val *a);   // int/float/bigint
coco_val *coco_not(const coco_val *a);   // logical not -> bool

// --- Lists -----------------------------------------------------------------
coco_val *coco_list_new(int64_t cap);
coco_val *coco_list_get(const coco_val *list, int64_t idx); // negative wraps
void coco_list_push(coco_val *list, coco_val *item);
int64_t coco_list_len(const coco_val *list);

// --- Maps (string keys) ----------------------------------------------------
coco_val *coco_map_new(void);
coco_val *coco_map_get(const coco_val *map, const char *key, int64_t key_len);
void coco_map_set(coco_val *map, const char *key, int64_t key_len, coco_val *val);
int64_t coco_map_len(const coco_val *map);

// --- Strings ---------------------------------------------------------------
int64_t coco_str_len(const coco_val *s);
coco_val *coco_str_concat(const coco_val *a, const coco_val *b);
const char *coco_str_data(const coco_val *s);

// --- Builtins --------------------------------------------------------------
// print writes the value's string form to stdout, returns null.
coco_val *coco_print(coco_val *v);
// len returns the length (list/map/string) as an int.
coco_val *coco_len(coco_val *v);
// toString returns the string form of any value.
coco_val *coco_tostring(const coco_val *v);
// range(a, b) returns a list [a, a+1, ..., b-1] (exclusive). b<=a -> empty.
coco_val *coco_range(int64_t a, int64_t b);

// --- Helpers used by codegen ----------------------------------------------
// coco_int_to_f64: promote an int/bigint to f64 (may lose precision for big).
double coco_int_to_f64(const coco_val *a);

// --- Internal bignum API (for arith.c / bignum.c) -------------------------
// These build the bignum tier of the adaptive tower. They operate on
// coco_bigint directly; the value-level coco_add etc. wrap them.
coco_bigint *coco_bi_from_i64(int64_t v);
double coco_bi_to_f64(const coco_bigint *bi);
void coco_bi_retain(coco_bigint *bi);
void coco_bi_release(coco_bigint *bi);
coco_bigint *coco_bi_new(int64_t cap);
void coco_bi_reserve(coco_bigint *bi, int64_t cap);
void coco_bi_normalize(coco_bigint *bi);

// Bignum arithmetic (each returns a fresh bignum with refcount 1).
coco_bigint *coco_bi_add(const coco_bigint *a, const coco_bigint *b);
coco_bigint *coco_bi_sub(const coco_bigint *a, const coco_bigint *b);
coco_bigint *coco_bi_mul(const coco_bigint *a, const coco_bigint *b);
int coco_bi_cmp(const coco_bigint *a, const coco_bigint *b); // -1/0/1
// Quotient (truncated toward zero) and remainder (sign of dividend).
coco_bigint *coco_bi_div(const coco_bigint *a, const coco_bigint *b);
coco_bigint *coco_bi_mod(const coco_bigint *a, const coco_bigint *b);

// Wrap a bignum into a tagged value (takes ownership of bi's ref).
coco_val *coco_make_bigint(coco_bigint *bi);

#endif // COCO_RT_H
