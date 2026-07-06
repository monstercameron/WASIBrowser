/* gwbjson.h — a tiny freestanding JSON reader for GWB guests.
 *
 * Enough to consume RPC responses (docs/04-WEB-RPC.md): object field lookup
 * (string / int / bool), and array element iteration. Not a validator — it
 * trusts well-formed input from a known service. No allocation; string
 * extraction copies into a caller buffer (with basic \" \\ \/ \n \t unescaping).
 */
#ifndef GWBJSON_H
#define GWBJSON_H
#include "gwb.h" /* u32, i32 */

static u32 js_strlen(const char *s) { u32 n = 0; while (s[n]) n++; return n; }

static const char *js_ws(const char *p) {
    while (*p == ' ' || *p == '\n' || *p == '\t' || *p == '\r') p++;
    return p;
}

/* Skip exactly one JSON value; return the pointer just past it. */
static const char *js_skip(const char *p) {
    p = js_ws(p);
    if (*p == '"') {
        p++;
        while (*p && *p != '"') { if (*p == '\\' && p[1]) p++; p++; }
        if (*p) p++;
        return p;
    }
    if (*p == '{' || *p == '[') {
        char open = *p, close = (open == '{') ? '}' : ']';
        int depth = 0;
        for (; *p; p++) {
            if (*p == '"') {
                p++;
                while (*p && *p != '"') { if (*p == '\\' && p[1]) p++; p++; }
                if (!*p) return p;
            } else if (*p == open) {
                depth++;
            } else if (*p == close) {
                depth--;
                if (depth == 0) return p + 1;
            }
        }
        return p;
    }
    while (*p && *p != ',' && *p != '}' && *p != ']' &&
           *p != ' ' && *p != '\n' && *p != '\t' && *p != '\r') p++;
    return p;
}

/* Find the value for `key` inside the object at `obj` (points at or before '{').
 * Returns a pointer to the value (whitespace-skipped), or 0. */
static const char *js_find(const char *obj, const char *key) {
    if (!obj) return 0;
    const char *p = js_ws(obj);
    if (*p != '{') return 0;
    p++;
    u32 klen = js_strlen(key);
    while (1) {
        p = js_ws(p);
        if (*p == '}' || !*p) return 0;
        if (*p != '"') return 0;
        p++;
        const char *ks = p;
        while (*p && *p != '"') { if (*p == '\\' && p[1]) p++; p++; }
        u32 kl = (u32)(p - ks);
        if (*p) p++;
        p = js_ws(p);
        if (*p != ':') return 0;
        p++;
        p = js_ws(p);
        int match = (kl == klen);
        if (match) for (u32 i = 0; i < klen; i++) if (key[i] != ks[i]) { match = 0; break; }
        if (match) return p;
        p = js_skip(p);
        p = js_ws(p);
        if (*p == ',') p++;
        else return 0;
    }
}

/* Copy a string value at `v` into out[cap], unescaping. Returns out. */
static const char *js_str_at(const char *v, char *out, u32 cap) {
    u32 n = 0;
    if (!v || *v != '"') { if (cap) out[0] = 0; return out; }
    v++;
    while (*v && *v != '"' && n + 1 < cap) {
        char c = *v++;
        if (c == '\\' && *v) {
            char e = *v++;
            c = (e == 'n') ? '\n' : (e == 't') ? '\t' : e; /* ", \, /, others literal */
        }
        out[n++] = c;
    }
    out[n] = 0;
    return out;
}

/* Object field getters. */
static const char *js_get_str(const char *obj, const char *key, char *out, u32 cap) {
    return js_str_at(js_find(obj, key), out, cap);
}

static i32 js_get_int(const char *obj, const char *key) {
    const char *v = js_find(obj, key);
    if (!v) return 0;
    int neg = 0;
    if (*v == '-') { neg = 1; v++; }
    i32 n = 0;
    while (*v >= '0' && *v <= '9') { n = n * 10 + (*v - '0'); v++; }
    return neg ? -n : n;
}

static int js_get_bool(const char *obj, const char *key) {
    const char *v = js_find(obj, key);
    return v && *v == 't';
}

/* Array iteration: first element inside the array at `arr` (points at '['),
 * or 0 if empty; js_arr_next advances past the current element. */
static const char *js_arr_first(const char *arr) {
    if (!arr) return 0;
    const char *p = js_ws(arr);
    if (*p != '[') return 0;
    p = js_ws(p + 1);
    return (*p == ']') ? 0 : p;
}
static const char *js_arr_next(const char *elem) {
    const char *p = js_ws(js_skip(elem));
    if (*p == ',') return js_ws(p + 1);
    return 0;
}

#endif /* GWBJSON_H */
