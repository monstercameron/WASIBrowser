/* gwb.h — the low-level C binding for the GWB ABI (docs/ABI.md).
 * Guest-side, freestanding wasm32 (no libc, no wasi-sdk needed):
 *   clang --target=wasm32-unknown-unknown -O2 -nostdlib \
 *         -Wl,--no-entry -Wl,--export-memory -o app.wasm app.c
 * Binding #3; mirror of sdk/gwb (Go) and sdk-rust.
 */
#ifndef GWB_H
#define GWB_H

typedef unsigned char u8;
typedef unsigned short u16;
typedef unsigned int u32;
typedef unsigned long long u64;
typedef float f32;
typedef double f64;

#define GWB_ROOT 1u

/* Well-known atoms (mirror of renderer/src/abi.rs). */
enum {
    GWB_DIV = 1, GWB_SPAN = 2, GWB_P = 3, GWB_H1 = 4, GWB_H2 = 5, GWB_H3 = 6,
    GWB_BUTTON = 7, GWB_INPUT = 8,

    GWB_ATTR_CLASS = 100, GWB_ATTR_ID = 101, GWB_ATTR_STYLE = 102,
    GWB_ATTR_VALUE = 104, GWB_ATTR_TYPE = 105, GWB_ATTR_PLACEHOLDER = 106,

    GWB_STYLE_DISPLAY = 200, GWB_STYLE_COLOR = 201, GWB_STYLE_BACKGROUND = 202,
    GWB_STYLE_WIDTH = 203, GWB_STYLE_HEIGHT = 204, GWB_STYLE_MARGIN = 205,
    GWB_STYLE_PADDING = 206, GWB_STYLE_FONT_SIZE = 208, GWB_STYLE_GAP = 210,
    GWB_STYLE_BORDER_RADIUS = 214, GWB_STYLE_CURSOR = 215,
};

/* Event kinds. */
enum {
    GWB_EV_POINTER_DOWN = 1, GWB_EV_POINTER_UP = 2, GWB_EV_POINTER_MOVE = 3,
    GWB_EV_CLICK = 4, GWB_EV_DBLCLICK = 5, GWB_EV_POINTER_ENTER = 6,
    GWB_EV_POINTER_LEAVE = 7, GWB_EV_WHEEL = 8, GWB_EV_KEY_DOWN = 9,
    GWB_EV_KEY_UP = 10, GWB_EV_TEXT_INPUT = 11, GWB_EV_INPUT = 12,
    GWB_EV_FOCUS = 15, GWB_EV_BLUR = 16, GWB_EV_SCROLL = 17,
    GWB_EV_WINDOW_RESIZE = 18, GWB_EV_THEME_CHANGE = 19,
    GWB_EV_OBSERVED_LAYOUT = 32,
};

enum { GWB_LOG_DEBUG = 0, GWB_LOG_INFO = 1, GWB_LOG_WARN = 2, GWB_LOG_ERROR = 3 };

#define GWB_NO_STR 0xFFFFFFFFu

/* ---- imports ---- */
#define GWB_IMPORT(name) \
    __attribute__((import_module("gwb"), import_name(name)))

GWB_IMPORT("submit") extern u32 gwb_imp_submit(const u8 *ptr, u32 len);
GWB_IMPORT("event_region") extern void gwb_imp_event_region(const u8 *ptr, u32 len);
GWB_IMPORT("log") extern void gwb_imp_log(u32 level, const u8 *ptr, u32 len);
GWB_IMPORT("request_frame") extern void gwb_imp_request_frame(void);

#define GWB_EXPORT(name) __attribute__((export_name(name)))

/* ---- tiny freestanding runtime ---- */
static u32 gwb_strlen(const char *s) { u32 n = 0; while (s[n]) n++; return n; }

static void gwb_log(u32 level, const char *msg) {
    gwb_imp_log(level, (const u8 *)msg, gwb_strlen(msg));
}

/* ---- event region ---- */
#define GWB_EVENT_BUF_SIZE 8192
static u8 gwb_event_buf[GWB_EVENT_BUF_SIZE];

static void gwb_register_event_region(void) {
    gwb_imp_event_region(gwb_event_buf, GWB_EVENT_BUF_SIZE);
}

/* ---- id allocation ---- */
static u32 gwb_next_id = 1; /* 1 is ROOT */
static u32 gwb_new_id(void) { return ++gwb_next_id; }

/* ---- batch builder (static buffers) ---- */
#define GWB_OPS_CAP (64 * 1024)
#define GWB_HEAP_CAP (64 * 1024)
static u8 gwb_batch_buf[16 + GWB_OPS_CAP + GWB_HEAP_CAP];
static u8 gwb_heap_buf[GWB_HEAP_CAP];
static u32 gwb_ops_len, gwb_heap_len, gwb_op_count;

static void gwb_put32(u8 *p, u32 v) {
    p[0] = (u8)v; p[1] = (u8)(v >> 8); p[2] = (u8)(v >> 16); p[3] = (u8)(v >> 24);
}
static u32 gwb_get32(const u8 *p) {
    return (u32)p[0] | ((u32)p[1] << 8) | ((u32)p[2] << 16) | ((u32)p[3] << 24);
}
static u16 gwb_get16(const u8 *p) { return (u16)((u32)p[0] | ((u32)p[1] << 8)); }
static f32 gwb_getf32(const u8 *p) { u32 v = gwb_get32(p); f32 f; __builtin_memcpy(&f, &v, 4); return f; }

static void gwb_op(u8 code, u16 a, u32 b, u32 c, u32 d) {
    u8 *r = gwb_batch_buf + 16 + gwb_ops_len;
    if (gwb_ops_len + 16 > GWB_OPS_CAP) return; /* full: drop (demo-grade) */
    r[0] = code; r[1] = 0;
    r[2] = (u8)a; r[3] = (u8)(a >> 8);
    gwb_put32(r + 4, b); gwb_put32(r + 8, c); gwb_put32(r + 12, d);
    gwb_ops_len += 16;
    gwb_op_count++;
}

static u32 gwb_str(const char *s) {
    u32 len = gwb_strlen(s);
    u32 off = gwb_heap_len;
    if (gwb_heap_len + 4 + len + 3 > GWB_HEAP_CAP) return GWB_NO_STR;
    gwb_put32(gwb_heap_buf + gwb_heap_len, len);
    gwb_heap_len += 4;
    for (u32 i = 0; i < len; i++) gwb_heap_buf[gwb_heap_len++] = (u8)s[i];
    while (gwb_heap_len % 4) gwb_heap_buf[gwb_heap_len++] = 0;
    return off;
}

static void gwb_create_element(u32 id, u32 tag) { gwb_op(1, 0, id, tag, GWB_NO_STR); }
static void gwb_create_text(u32 id, const char *t) { u32 s = gwb_str(t); gwb_op(2, 0, id, 0, s); }
static void gwb_set_attr(u32 id, u32 name, const char *v) { u32 s = gwb_str(v); gwb_op(3, 0, id, name, s); }
static void gwb_remove_attr(u32 id, u32 name) { gwb_op(4, 0, id, name, GWB_NO_STR); }
static void gwb_set_text(u32 id, const char *t) { u32 s = gwb_str(t); gwb_op(5, 0, id, 0, s); }
static void gwb_set_style(u32 id, u32 prop, const char *v) { u32 s = gwb_str(v); gwb_op(6, 0, id, prop, s); }
static void gwb_remove_style(u32 id, u32 prop) { gwb_op(7, 0, id, prop, GWB_NO_STR); }
static void gwb_append_child(u32 parent, u32 child) { gwb_op(8, 0, parent, child, GWB_NO_STR); }
static void gwb_insert_before(u32 parent, u32 child, u32 before) { gwb_op(9, 0, parent, child, before); }
static void gwb_remove(u32 id) { gwb_op(10, 0, id, 0, GWB_NO_STR); }
static void gwb_replace_with(u32 old, u32 new_) { gwb_op(11, 0, old, new_, GWB_NO_STR); }
static void gwb_clear(u32 id) { gwb_op(12, 0, id, 0, GWB_NO_STR); }
static void gwb_set_inner_html(u32 id, const char *h) { u32 s = gwb_str(h); gwb_op(13, 0, id, 0, s); }
static void gwb_define_atom(u32 atom, const char *n) { u32 s = gwb_str(n); gwb_op(14, 0, atom, 0, s); }
static void gwb_listen(u32 id, u16 kind) { gwb_op(15, kind, id, 0, GWB_NO_STR); }
static void gwb_unlisten(u32 id, u16 kind) { gwb_op(16, kind, id, 0, GWB_NO_STR); }
static void gwb_observe(u32 id, u32 what) { gwb_op(17, 0, id, what, GWB_NO_STR); }
static void gwb_unobserve(u32 id, u32 what) { gwb_op(18, 0, id, what, GWB_NO_STR); }
static void gwb_focus(u32 id) { gwb_op(19, 0, id, 0, GWB_NO_STR); }

static u32 gwb_flush(void) {
    if (gwb_op_count == 0) return 0;
    gwb_batch_buf[0] = 'G'; gwb_batch_buf[1] = 'W'; gwb_batch_buf[2] = 'B'; gwb_batch_buf[3] = '1';
    gwb_put32(gwb_batch_buf + 4, gwb_op_count);
    gwb_put32(gwb_batch_buf + 8, 16 + gwb_ops_len);
    gwb_put32(gwb_batch_buf + 12, gwb_heap_len);
    for (u32 i = 0; i < gwb_heap_len; i++) gwb_batch_buf[16 + gwb_ops_len + i] = gwb_heap_buf[i];
    u32 status = gwb_imp_submit(gwb_batch_buf, 16 + gwb_ops_len + gwb_heap_len);
    gwb_ops_len = gwb_heap_len = gwb_op_count = 0;
    return status;
}

/* ---- event decode ---- */
typedef struct {
    u16 kind, flags;
    u32 target, listener;
    f32 x, y, w, h, dx, dy, scale;
    u16 buttons, mods;
    u8 pressed, dark;
    const char *str; /* NUL-terminated view into the event region */
    u32 str_len;
} gwb_event;

/* Decode records, calling handler(e) per record; ORs the return flags. */
typedef u32 (*gwb_event_fn)(const gwb_event *e);

static u32 gwb_decode_events(u32 count, gwb_event_fn handler) {
    u32 ret = 0, off = 0;
    static char strbuf[1024];
    for (u32 i = 0; i < count; i++) {
        if (off + 40 > GWB_EVENT_BUF_SIZE) break;
        const u8 *r = gwb_event_buf + off;
        gwb_event e = {0};
        e.kind = gwb_get16(r); e.flags = gwb_get16(r + 2);
        e.target = gwb_get32(r + 4); e.listener = gwb_get32(r + 8);
        switch (e.kind) {
        case GWB_EV_POINTER_DOWN: case GWB_EV_POINTER_UP: case GWB_EV_POINTER_MOVE:
        case GWB_EV_CLICK: case GWB_EV_DBLCLICK:
        case GWB_EV_POINTER_ENTER: case GWB_EV_POINTER_LEAVE:
            e.x = gwb_getf32(r + 20); e.y = gwb_getf32(r + 24);
            e.buttons = gwb_get16(r + 28); e.mods = gwb_get16(r + 30);
            break;
        case GWB_EV_WHEEL:
            e.dx = gwb_getf32(r + 20); e.dy = gwb_getf32(r + 24);
            e.mods = gwb_get16(r + 28);
            break;
        case GWB_EV_KEY_DOWN: case GWB_EV_KEY_UP: case GWB_EV_TEXT_INPUT:
            e.mods = gwb_get16(r + 22); e.pressed = r[24];
            break;
        case GWB_EV_SCROLL:
            e.x = gwb_getf32(r + 20); e.y = gwb_getf32(r + 24);
            break;
        case GWB_EV_WINDOW_RESIZE:
            e.w = gwb_getf32(r + 20); e.h = gwb_getf32(r + 24);
            e.scale = gwb_getf32(r + 28);
            break;
        case GWB_EV_THEME_CHANGE: e.dark = (u8)gwb_get32(r + 20); break;
        case GWB_EV_OBSERVED_LAYOUT:
            e.x = gwb_getf32(r + 20); e.y = gwb_getf32(r + 24);
            e.w = gwb_getf32(r + 28); e.h = gwb_getf32(r + 32);
            break;
        default: break;
        }
        u32 str_len = gwb_get32(r + 36);
        u32 next = off + 40;
        e.str = ""; e.str_len = 0;
        if (str_len > 0 && next + str_len <= GWB_EVENT_BUF_SIZE) {
            u32 n = str_len < sizeof(strbuf) - 1 ? str_len : sizeof(strbuf) - 1;
            for (u32 j = 0; j < n; j++) strbuf[j] = (char)gwb_event_buf[next + j];
            strbuf[n] = 0;
            e.str = strbuf; e.str_len = n;
            next += str_len;
            next = (next + 3) & ~3u;
        }
        off = next;
        ret |= handler(&e);
    }
    return ret;
}

/* ---- string helpers (no libc) ---- */
static char *gwb_append_str(char *dst, const char *s) {
    while (*s) *dst++ = *s++;
    return dst;
}
static char *gwb_append_u32(char *dst, u32 v) {
    char tmp[10];
    int n = 0;
    if (v == 0) tmp[n++] = '0';
    while (v) { tmp[n++] = (char)('0' + v % 10); v /= 10; }
    while (n) *dst++ = tmp[--n];
    return dst;
}

#endif /* GWB_H */
