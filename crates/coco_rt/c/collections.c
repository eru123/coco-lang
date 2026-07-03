// collections.c — lists, maps, strings, and builtins (print/len/tostring/range).

#include "coco_rt.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void die(const char *msg) { fprintf(stderr, "coco_rt: %s\n", msg); abort(); }
static void *xmalloc(size_t n) { void *p = malloc(n ? n : 1); if (!p) die("oom"); return p; }
static void *xrealloc(void *p, size_t n) { void *q = realloc(p, n ? n : 1); if (!q) die("oom"); return q; }

// --- Strings ---------------------------------------------------------------

int64_t coco_str_len(const coco_val *s) {
    if (s->tag != COCO_STRING) die("coco_str_len: not a string");
    return s->u.s->len;
}

const char *coco_str_data(const coco_val *s) {
    if (s->tag != COCO_STRING) die("coco_str_data: not a string");
    return s->u.s->data;
}

coco_val *coco_str_concat(const coco_val *a, const coco_val *b) {
    if (a->tag != COCO_STRING || b->tag != COCO_STRING)
        die("coco_str_concat: not strings");
    int64_t n = a->u.s->len + b->u.s->len;
    char *buf = xmalloc((size_t)n + 1);
    memcpy(buf, a->u.s->data, (size_t)a->u.s->len);
    memcpy(buf + a->u.s->len, b->u.s->data, (size_t)b->u.s->len);
    buf[n] = '\0';
    coco_val *r = coco_make_string(buf, n);
    free(buf);
    return r;
}

// --- Lists -----------------------------------------------------------------

coco_val *coco_list_new(int64_t cap) {
    coco_val *v = xmalloc(sizeof(coco_val));
    v->tag = COCO_LIST;
    v->refcount = 1;
    coco_list *l = xmalloc(sizeof(coco_list));
    l->refcount = 1;
    l->len = 0;
    l->cap = cap > 0 ? cap : 4;
    l->items = xmalloc((size_t)l->cap * sizeof(coco_val *));
    v->u.l = l;
    return v;
}

int64_t coco_list_len(const coco_val *list) {
    if (list->tag != COCO_LIST) die("coco_list_len: not a list");
    return list->u.l->len;
}

void coco_list_push(coco_val *list, coco_val *item) {
    if (list->tag != COCO_LIST) die("coco_list_push: not a list");
    coco_list *l = list->u.l;
    if (l->len >= l->cap) {
        l->cap *= 2;
        l->items = xrealloc(l->items, (size_t)l->cap * sizeof(coco_val *));
    }
    l->items[l->len++] = coco_retain(item);
}

coco_val *coco_list_get(const coco_val *list, int64_t idx) {
    if (list->tag != COCO_LIST) die("coco_list_get: not a list");
    coco_list *l = list->u.l;
    // Negative indices wrap from the end (matches interpreter).
    int64_t i = idx;
    if (i < 0) i += l->len;
    if (i < 0 || i >= l->len) die("coco_list_get: index out of bounds");
    return coco_retain(l->items[i]);
}

// --- Maps (string keys, open addressing) -----------------------------------

static int64_t hash_key(const char *key, int64_t len) {
    // FNV-1a 64-bit.
    uint64_t h = 1469598103934665603ULL;
    for (int64_t i = 0; i < len; i++) {
        h ^= (uint8_t)key[i];
        h *= 1099511628211ULL;
    }
    return (int64_t)h;
}

coco_val *coco_map_new(void) {
    coco_val *v = xmalloc(sizeof(coco_val));
    v->tag = COCO_MAP;
    v->refcount = 1;
    coco_map *m = xmalloc(sizeof(coco_map));
    m->refcount = 1;
    m->len = 0;
    m->cap = 8;
    m->entries = xmalloc((size_t)m->cap * sizeof(coco_map_entry));
    for (int64_t i = 0; i < m->cap; i++) { m->entries[i].key = NULL; }
    v->u.m = m;
    return v;
}

int64_t coco_map_len(const coco_val *map) {
    if (map->tag != COCO_MAP) die("coco_map_len: not a map");
    return map->u.m->len;
}

static int64_t bucket_probe(const coco_map *m, const char *key, int64_t klen, int64_t h, bool find_empty) {
    int64_t mask = m->cap - 1;
    int64_t i = (int64_t)((uint64_t)h) & mask;
    for (int64_t step = 0; step < m->cap; step++) {
        coco_map_entry *e = &m->entries[i];
        if (!e->key) return find_empty ? i : -1;
        if (e->hash == h && e->key_len == klen && memcmp(e->key, key, (size_t)klen) == 0)
            return i;
        i = (i + 1) & mask;
    }
    return -1;
}

static void map_grow(coco_map *m) {
    int64_t oldcap = m->cap;
    coco_map_entry *old = m->entries;
    m->cap = oldcap * 2;
    m->entries = xmalloc((size_t)m->cap * sizeof(coco_map_entry));
    for (int64_t i = 0; i < m->cap; i++) m->entries[i].key = NULL;
    m->len = 0;
    for (int64_t i = 0; i < oldcap; i++) {
        if (old[i].key) {
            int64_t b = bucket_probe(m, old[i].key, old[i].key_len, old[i].hash, true);
            m->entries[b] = old[i];
            m->len++;
        }
    }
    free(old);
}

void coco_map_set(coco_val *map, const char *key, int64_t klen, coco_val *val) {
    if (map->tag != COCO_MAP) die("coco_map_set: not a map");
    coco_map *m = map->u.m;
    int64_t h = hash_key(key, klen);
    int64_t b = bucket_probe(m, key, klen, h, false);
    if (b >= 0) {
        // Replace existing.
        coco_release(m->entries[b].val);
        m->entries[b].val = coco_retain(val);
        return;
    }
    // Insert new.
    if ((m->len + 1) * 2 >= m->cap) map_grow(m);
    b = bucket_probe(m, key, klen, h, true);
    coco_map_entry *e = &m->entries[b];
    e->key = xmalloc((size_t)klen + 1);
    memcpy(e->key, key, (size_t)klen);
    e->key[klen] = '\0';
    e->key_len = klen;
    e->hash = h;
    e->val = coco_retain(val);
    m->len++;
}

coco_val *coco_map_get(const coco_val *map, const char *key, int64_t klen) {
    if (map->tag != COCO_MAP) die("coco_map_get: not a map");
    const coco_map *m = map->u.m;
    int64_t h = hash_key(key, klen);
    int64_t b = bucket_probe(m, key, klen, h, false);
    if (b < 0) return coco_make_null();
    return coco_retain(m->entries[b].val);
}

// --- Builtins --------------------------------------------------------------

// Stringify a value into a malloc'd buffer. Returns the buffer and sets *len.
static char *val_to_string(const coco_val *v, int64_t *len);

static char *int_to_string(int64_t i, int64_t *len) {
    char buf[32];
    int n = snprintf(buf, sizeof(buf), "%lld", (long long)i);
    char *out = xmalloc((size_t)n + 1);
    memcpy(out, buf, (size_t)n + 1);
    *len = n;
    return out;
}

static char *float_to_string(double f, int64_t *len) {
    char buf[64];
    // Use %g for compact representation, like many scripting languages.
    int n = snprintf(buf, sizeof(buf), "%g", f);
    char *out = xmalloc((size_t)n + 1);
    memcpy(out, buf, (size_t)n + 1);
    *len = n;
    return out;
}

static char *bigint_to_string(const coco_bigint *bi, int64_t *len) {
    // Convert via repeated divmod by 10^9. (Correctness over speed.)
    if (bi->len == 0) { char *out = xmalloc(2); out[0]='0'; out[1]='\0'; *len=1; return out; }
    // Work on a mutable copy of the magnitude.
    coco_bigint *cur = coco_bi_new(bi->len);
    coco_bi_reserve(cur, bi->len);
    memcpy(cur->limbs, bi->limbs, (size_t)bi->len * 4);
    cur->len = bi->len;
    cur->neg = false;
    coco_bi_normalize(cur);
    // Divisor 10^9 fits in a limb (32 bits).
    coco_bigint *d = coco_bi_from_i64(1000000000LL);
    // Collect chunks in reverse.
    char chunks[1024][10];
    int nchunks = 0;
    while (cur->len > 0) {
        coco_bigint *q, *r;
        // Use the internal mag_divmod via coco_bi_div/mod (public, on bigints).
        // We need remainder; re-derive by: r = cur - q*d. Simpler: inline.
        // Implement using coco_bi_div and coco_bi_mod.
        q = coco_bi_div(cur, d);
        r = coco_bi_mod(cur, d);
        int64_t chunk = 0;
        for (int64_t i = r->len - 1; i >= 0; i--) chunk = chunk * 4294967296LL + r->limbs[i];
        snprintf(chunks[nchunks], sizeof(chunks[nchunks]), "%09lld", (long long)chunk);
        nchunks++;
        coco_bi_release(r);
        coco_bi_release(cur);
        cur = q;
    }
    coco_bi_release(cur);
    coco_bi_release(d);
    // Assemble: first chunk has no leading-zero padding.
    char first[16];
    // The last-pushed chunk is the most significant; nchunks-1 is MSB.
    snprintf(first, sizeof(first), "%s", chunks[nchunks - 1]);
    // Trim leading zeros from the MSB chunk.
    char *p = first;
    while (*p == '0' && *(p + 1)) p++;
    int64_t total = (int64_t)strlen(p);
    for (int i = 0; i < nchunks - 1; i++) total += 9;
    if (bi->neg) total += 1;
    char *out = xmalloc((size_t)total + 1);
    int64_t pos = 0;
    if (bi->neg) out[pos++] = '-';
    memcpy(out + pos, p, strlen(p)); pos += strlen(p);
    for (int i = nchunks - 2; i >= 0; i--) {
        memcpy(out + pos, chunks[i], 9); pos += 9;
    }
    out[pos] = '\0';
    *len = pos;
    return out;
}

static char *val_to_string(const coco_val *v, int64_t *len) {
    switch (v->tag) {
        case COCO_INT:    return int_to_string(v->u.i, len);
        case COCO_FLOAT:  return float_to_string(v->u.f, len);
        case COCO_BOOL:   { const char *s = v->u.b ? "true" : "false"; int64_t n = v->u.b ? 4 : 5; char *o = xmalloc(n+1); memcpy(o, s, n+1); *len = n; return o; }
        case COCO_NULL:   { char *o = xmalloc(5); memcpy(o, "null", 5); *len = 4; return o; }
        case COCO_STRING: { int64_t n = v->u.s->len; char *o = xmalloc(n+1); memcpy(o, v->u.s->data, n); o[n]='\0'; *len = n; return o; }
        case COCO_BIGINT: return bigint_to_string(v->u.bi, len);
        case COCO_LIST: {
            // Approximate form: [a, b, c] (each element via val_to_string).
            int64_t cap = 2, pos = 0;
            char *out = xmalloc(cap);
            out[pos++] = '[';
            for (int64_t i = 0; i < v->u.l->len; i++) {
                if (i > 0) { if (pos + 2 >= cap) { cap *= 2; out = xrealloc(out, cap); } out[pos++] = ','; out[pos++] = ' '; }
                int64_t el; char *es = val_to_string(v->u.l->items[i], &el);
                while (pos + el + 1 >= cap) { cap *= 2; out = xrealloc(out, cap); }
                memcpy(out + pos, es, el); pos += el; free(es);
            }
            while (pos + 1 >= cap) { cap *= 2; out = xrealloc(out, cap); }
            out[pos++] = ']'; out[pos] = '\0'; *len = pos; return out;
        }
        case COCO_MAP: {
            int64_t cap = 2, pos = 0;
            char *out = xmalloc(cap);
            out[pos++] = '{';
            bool first = true;
            for (int64_t i = 0; i < v->u.m->cap; i++) {
                coco_map_entry *e = &v->u.m->entries[i];
                if (!e->key) continue;
                if (!first) { if (pos + 2 >= cap) { cap *= 2; out = xrealloc(out, cap); } out[pos++] = ','; out[pos++] = ' '; }
                first = false;
                while (pos + e->key_len + 2 >= cap) { cap *= 2; out = xrealloc(out, cap); }
                memcpy(out + pos, e->key, e->key_len); pos += e->key_len;
                out[pos++] = ':'; out[pos++] = ' ';
                int64_t vl; char *vs = val_to_string(e->val, &vl);
                while (pos + vl + 1 >= cap) { cap *= 2; out = xrealloc(out, cap); }
                memcpy(out + pos, vs, vl); pos += vl; free(vs);
            }
            while (pos + 1 >= cap) { cap *= 2; out = xrealloc(out, cap); }
            out[pos++] = '}'; out[pos] = '\0'; *len = pos; return out;
        }
    }
    char *o = xmalloc(8); memcpy(o, "<val>", 6); *len = 5; return o;
}

coco_val *coco_tostring(const coco_val *v) {
    int64_t len;
    char *s = val_to_string(v, &len);
    coco_val *r = coco_make_string(s, len);
    free(s);
    return r;
}

coco_val *coco_print(coco_val *v) {
    int64_t len;
    char *s = val_to_string(v, &len);
    fwrite(s, 1, (size_t)len, stdout);
    fputc('\n', stdout);
    free(s);
    return coco_make_null();
}

coco_val *coco_len(coco_val *v) {
    switch (v->tag) {
        case COCO_STRING: return coco_make_int(v->u.s->len);
        case COCO_LIST:   return coco_make_int(v->u.l->len);
        case COCO_MAP:    return coco_make_int(v->u.m->len);
        default: die("coco_len: value has no length"); return NULL;
    }
}

coco_val *coco_range(int64_t a, int64_t b) {
    coco_val *list = coco_list_new(b > a ? b - a : 0);
    for (int64_t i = a; i < b; i++) {
        coco_list_push(list, coco_make_int(i));
    }
    return list;
}
