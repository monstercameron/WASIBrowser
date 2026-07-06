/* gwbc.h — C component framework on the GWB ABI ("JSX with the preprocessor").
 *
 * Sits on top of gwb.h (the LL binding). Gives freestanding C:
 *   COMPONENT / RETURN / USE / PROPS / PROP     components + composition
 *   STATE / STATE_STR / SET / PREVIOUS          name-keyed state registry
 *   EVENT / EVENT_INPUT                         handlers as render-declared blocks
 *   Div/P/H1/Button/Input/... variadic macros   bare strings auto-wrap via _Generic
 *   U(...) utility tokens (Tailwind-ish)        inline-style groups
 *   T("fmt", ...)                               reactive text (mini printf: %s %d %%)
 *   WHEN / IF / Empty                           conditional rendering
 *   GWB_APP(Root, PROPS(...))                   generates the wasm exports
 *
 * Model: immediate mode + full replace. Every event re-renders the whole tree
 * (two passes: handlers run, then a clean render) and replaces the mount's
 * children. The stress test showed 1600 ops apply in ~1ms, so this is fine
 * for demo-scale apps; reconciliation is a later framework layer.
 *
 * Known limitation: full replace recreates the focused <input>; the framework
 * re-focuses its successor (matched by handler id) but the caret resets to
 * position 0, so mid-render typing inserts at the front. Fix belongs to a
 * keyed reconciler or a host-side caret save/restore, not to this layer.
 */
#ifndef GWBC_H
#define GWBC_H

#include "gwb.h"

typedef signed int i32;
typedef u32 Handler;

/* ---------------------------------------------------------------- nodes */

typedef struct { i32 idx; } Node;

enum { K_EMPTY, K_ELEM, K_TEXT, K_STYLE, K_ATTR, K_HANDLER, K_GROUP };

typedef struct {
    u8 kind;
    u16 ev;          /* K_HANDLER: event kind */
    u32 a;           /* tag / prop atom / attr atom / handler id */
    const char *s;   /* text / style value / attr value */
    i32 first, last, next;
} GwbcNode;

#define GWBC_MAX_NODES 4096
static GwbcNode gc_nodes[GWBC_MAX_NODES];
static i32 gc_node_count;
static char gc_strpool[48 * 1024];
static u32 gc_strpool_len;

static Node gc_alloc(u8 kind) {
    if (gc_node_count >= GWBC_MAX_NODES) gc_node_count = GWBC_MAX_NODES - 1; /* clamp: last node overwritten */
    GwbcNode *n = &gc_nodes[gc_node_count];
    n->kind = kind; n->ev = 0; n->a = 0; n->s = 0;
    n->first = n->last = n->next = -1;
    return (Node){ gc_node_count++ };
}

static const char *gc_strdup(const char *s, u32 len) {
    if (gc_strpool_len + len + 1 > sizeof(gc_strpool)) return "";
    char *dst = gc_strpool + gc_strpool_len;
    for (u32 i = 0; i < len; i++) dst[i] = s[i];
    dst[len] = 0;
    gc_strpool_len += len + 1;
    return dst;
}

static Node Empty(void) { return gc_alloc(K_EMPTY); }

static Node gwbc_text(const char *s) {
    Node n = gc_alloc(K_TEXT);
    gc_nodes[n.idx].s = s;
    return n;
}
static Node gwbc_pass(Node n) { return n; }

static Node gc_style(u32 prop, const char *v) {
    Node n = gc_alloc(K_STYLE);
    gc_nodes[n.idx].a = prop; gc_nodes[n.idx].s = v;
    return n;
}
static Node gc_attr(u32 attr, const char *v) {
    Node n = gc_alloc(K_ATTR);
    gc_nodes[n.idx].a = attr; gc_nodes[n.idx].s = v;
    return n;
}
static void gc_append(Node parent, Node child) {
    GwbcNode *p = &gc_nodes[parent.idx];
    if (p->first < 0) p->first = child.idx; else gc_nodes[p->last].next = child.idx;
    p->last = child.idx;
}
static Node gc_group2(Node a, Node b) {
    Node g = gc_alloc(K_GROUP);
    gc_append(g, a); gc_append(g, b);
    return g;
}

static Node gwbc_element(u32 tag, const Node *items, u32 n) {
    Node el = gc_alloc(K_ELEM);
    gc_nodes[el.idx].a = tag;
    for (u32 i = 0; i < n; i++) gc_append(el, items[i]);
    return el;
}

/* ---------------------------------------------------------------- mini fmt */

static char *gc_fmt_i32(char *dst, i32 v) {
    if (v < 0) { *dst++ = '-'; v = -v; }
    return gwb_append_u32(dst, (u32)v);
}
static void fmt_i32(char *dst, i32 v) { *gc_fmt_i32(dst, v) = 0; }

static const char *gc_vfmt(const char *fmt, __builtin_va_list ap) {
    char buf[512];
    char *p = buf, *end = buf + sizeof(buf) - 1;
    for (const char *f = fmt; *f && p < end; f++) {
        if (*f != '%') { *p++ = *f; continue; }
        f++;
        if (*f == 'd') { char tmp[16]; fmt_i32(tmp, __builtin_va_arg(ap, i32)); for (char *t = tmp; *t && p < end;) *p++ = *t++; }
        else if (*f == 's') { const char *s = __builtin_va_arg(ap, const char *); while (*s && p < end) *p++ = *s++; }
        else if (*f == '%') { *p++ = '%'; }
        else { *p++ = '%'; if (p < end) *p++ = *f; }
    }
    *p = 0;
    return gc_strdup(buf, (u32)(p - buf));
}

static Node Textf(const char *fmt, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, fmt);
    const char *s = gc_vfmt(fmt, ap);
    __builtin_va_end(ap);
    return gwbc_text(s);
}

/* rem(quarter-steps): Tailwind scale, n * 0.25rem, rendered "1.25rem". */
static const char *gc_rem(i32 quarters) {
    char buf[24];
    char *p = gc_fmt_i32(buf, quarters / 4);
    i32 frac = (quarters % 4) * 25;
    if (frac) { *p++ = '.'; if (frac == 25) { *p++ = '2'; *p++ = '5'; } else if (frac == 50) { *p++ = '5'; } else { *p++ = '7'; *p++ = '5'; } }
    *p++ = 'r'; *p++ = 'e'; *p++ = 'm';
    return gc_strdup(buf, (u32)(p - buf));
}

/* ---------------------------------------------------------------- state */

typedef struct { u8 ok; i32 value; } Previous_i32;

typedef struct {
    const char *name;
    u8 kind; /* 0 = i32, 1 = str */
    i32 v_i32, prev_i32;
    u8 has_prev;
    char v_str[128];
} GcState;

static GcState gc_state[64];
static u32 gc_state_count;

static u8 gc_streq(const char *a, const char *b) {
    while (*a && *a == *b) { a++; b++; }
    return *a == *b;
}
static GcState *gc_slot(const char *name) {
    for (u32 i = 0; i < gc_state_count; i++)
        if (gc_streq(gc_state[i].name, name)) return &gc_state[i];
    if (gc_state_count >= 64) return &gc_state[63];
    GcState *s = &gc_state[gc_state_count++];
    s->name = name; s->kind = 0; s->v_i32 = 0; s->prev_i32 = 0; s->has_prev = 0; s->v_str[0] = 0;
    return s;
}
static u8 gc_slot_new(const char *name) {
    for (u32 i = 0; i < gc_state_count; i++)
        if (gc_streq(gc_state[i].name, name)) return 0;
    return 1;
}

static i32 gwbc_use_i32(const char *name, i32 initial) {
    u8 fresh = gc_slot_new(name);
    GcState *s = gc_slot(name);
    if (fresh) s->v_i32 = initial;
    return s->v_i32;
}
static char *gwbc_use_str(const char *name, const char *initial) {
    u8 fresh = gc_slot_new(name);
    GcState *s = gc_slot(name);
    if (fresh) { u32 i = 0; while (initial[i] && i < 127) { s->v_str[i] = initial[i]; i++; } s->v_str[i] = 0; s->kind = 1; }
    return s->v_str;
}
static void gwbc_set_i32(const char *name, i32 v) {
    GcState *s = gc_slot(name);
    if (s->v_i32 != v) { s->prev_i32 = s->v_i32; s->has_prev = 1; }
    s->v_i32 = v;
}
static void gwbc_set_str(const char *name, const char *v) {
    GcState *s = gc_slot(name);
    u32 i = 0; while (v[i] && i < 127) { s->v_str[i] = v[i]; i++; } s->v_str[i] = 0;
}
static Previous_i32 gwbc_use_previous_i32(const char *name, i32 current) {
    (void)current;
    GcState *s = gc_slot(name);
    return (Previous_i32){ s->has_prev, s->prev_i32 };
}

/* ---------------------------------------------------------------- events */

static const char *gc_handler_names[64];
static u32 gc_handler_count;
static u32 gc_active_handler = 0xFFFFFFFF;
static const char *gc_event_value = "";

typedef struct { const char *value; } InputEvent;

static Handler gwbc_handler(const char *name) {
    for (u32 i = 0; i < gc_handler_count; i++)
        if (gc_streq(gc_handler_names[i], name)) return i;
    if (gc_handler_count >= 64) return 63;
    gc_handler_names[gc_handler_count] = name;
    return gc_handler_count++;
}
static u8 gwbc_handler_active(Handler h) { return h == gc_active_handler; }
static const char *gwbc_input_value(Handler h) {
    return h == gc_active_handler ? gc_event_value : "";
}

static Node gc_on(Handler h, u16 kind) {
    Node n = gc_alloc(K_HANDLER);
    gc_nodes[n.idx].a = h; gc_nodes[n.idx].ev = kind;
    return n;
}

/* node-id -> handler map, rebuilt each render */
static u32 gc_hn_node[256]; static u32 gc_hn_handler[256]; static u16 gc_hn_kind[256];
static u32 gc_hn_count;

/* ---------------------------------------------------------------- emit */

static u32 gc_container; /* persistent mount div; children replaced per render */

static void gc_emit(i32 idx, u32 parent);

static void gc_apply(i32 idx, u32 elem) {
    GwbcNode *n = &gc_nodes[idx];
    switch (n->kind) {
    case K_STYLE: gwb_set_style(elem, n->a, n->s); break;
    case K_ATTR: gwb_set_attr(elem, n->a, n->s); break;
    case K_HANDLER:
        gwb_listen(elem, n->ev);
        if (gc_hn_count < 256) {
            gc_hn_node[gc_hn_count] = elem;
            gc_hn_handler[gc_hn_count] = n->a;
            gc_hn_kind[gc_hn_count] = n->ev;
            gc_hn_count++;
        }
        break;
    case K_GROUP:
        for (i32 c = n->first; c >= 0; c = gc_nodes[c].next) gc_apply(c, elem);
        break;
    case K_EMPTY: break;
    default: gc_emit(idx, elem); break; /* K_ELEM, K_TEXT */
    }
}

static void gc_emit(i32 idx, u32 parent) {
    GwbcNode *n = &gc_nodes[idx];
    if (n->kind == K_TEXT) {
        u32 id = gwb_new_id();
        gwb_create_text(id, n->s);
        gwb_append_child(parent, id);
        return;
    }
    if (n->kind != K_ELEM) return;
    u32 id = gwb_new_id();
    gwb_create_element(id, n->a);
    for (i32 c = n->first; c >= 0; c = gc_nodes[c].next) gc_apply(c, id);
    gwb_append_child(parent, id);
}

/* ---------------------------------------------------------------- render loop */

static Node gwb__root(void); /* generated by GWB_APP */

static void gwbc_render(void) {
    /* Pass 1 (only when an event is active): run EVENT bodies so SETs land. */
    if (gc_active_handler != 0xFFFFFFFF) {
        gc_node_count = 0; gc_strpool_len = 0;
        (void)gwb__root();
        u32 refocus = gc_active_handler;
        u16 refocus_kind = 0;
        for (u32 i = 0; i < gc_hn_count; i++)
            if (gc_hn_handler[i] == gc_active_handler) refocus_kind = gc_hn_kind[i];
        gc_active_handler = 0xFFFFFFFF;
        gc_event_value = "";

        /* Pass 2: clean render with settled state. */
        gc_node_count = 0; gc_strpool_len = 0; gc_hn_count = 0;
        Node tree = gwb__root();
        gwb_clear(gc_container);
        gc_emit(tree.idx, gc_container);
        /* Full replace destroyed the focused input; re-focus its successor. */
        if (refocus_kind == GWB_EV_INPUT) {
            for (u32 i = 0; i < gc_hn_count; i++)
                if (gc_hn_handler[i] == refocus && gc_hn_kind[i] == GWB_EV_INPUT)
                    gwb_focus(gc_hn_node[i]);
        }
    } else {
        gc_node_count = 0; gc_strpool_len = 0; gc_hn_count = 0;
        Node tree = gwb__root();
        gwb_clear(gc_container);
        gc_emit(tree.idx, gc_container);
    }
    gwb_flush();
}

static u32 gc_on_event(const gwb_event *e) {
    for (u32 i = 0; i < gc_hn_count; i++) {
        if (gc_hn_node[i] == e->listener && gc_hn_kind[i] == e->kind) {
            gc_active_handler = gc_hn_handler[i];
            gc_event_value = e->str;
            gwbc_render();
            return 0;
        }
    }
    return 0;
}

static void gwbc_boot(void) {
    gwb_register_event_region();
    /* dynamic atoms used by utilities */
    gwb_define_atom(1025, "padding-left");
    gwb_define_atom(1026, "padding-right");
    gwb_define_atom(1027, "padding-top");
    gwb_define_atom(1028, "padding-bottom");
    gwb_define_atom(1029, "max-width");
    gc_container = gwb_new_id();
    gwb_create_element(gc_container, GWB_DIV);
    gwb_append_child(GWB_ROOT, gc_container);
    gwbc_render();
}

/* ---------------------------------------------------------------- variadic map */

#define GC_NARG(...) GC_NARG_(__VA_ARGS__, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1)
#define GC_NARG_(_1, _2, _3, _4, _5, _6, _7, _8, _9, _10, _11, _12, _13, _14, _15, _16, N, ...) N
#define GC_CAT(a, b) GC_CAT_(a, b)
#define GC_CAT_(a, b) a##b

#define GC_F1(F, a) F(a)
#define GC_F2(F, a, ...) F(a), GC_F1(F, __VA_ARGS__)
#define GC_F3(F, a, ...) F(a), GC_F2(F, __VA_ARGS__)
#define GC_F4(F, a, ...) F(a), GC_F3(F, __VA_ARGS__)
#define GC_F5(F, a, ...) F(a), GC_F4(F, __VA_ARGS__)
#define GC_F6(F, a, ...) F(a), GC_F5(F, __VA_ARGS__)
#define GC_F7(F, a, ...) F(a), GC_F6(F, __VA_ARGS__)
#define GC_F8(F, a, ...) F(a), GC_F7(F, __VA_ARGS__)
#define GC_F9(F, a, ...) F(a), GC_F8(F, __VA_ARGS__)
#define GC_F10(F, a, ...) F(a), GC_F9(F, __VA_ARGS__)
#define GC_F11(F, a, ...) F(a), GC_F10(F, __VA_ARGS__)
#define GC_F12(F, a, ...) F(a), GC_F11(F, __VA_ARGS__)
#define GC_F13(F, a, ...) F(a), GC_F12(F, __VA_ARGS__)
#define GC_F14(F, a, ...) F(a), GC_F13(F, __VA_ARGS__)
#define GC_F15(F, a, ...) F(a), GC_F14(F, __VA_ARGS__)
#define GC_F16(F, a, ...) F(a), GC_F15(F, __VA_ARGS__)
#define GC_MAP(F, ...) GC_CAT(GC_F, GC_NARG(__VA_ARGS__))(F, __VA_ARGS__)

/* bare strings become text nodes; Nodes pass through */
#define GC_N(x) _Generic((x), \
    char *: gwbc_text, \
    const char *: gwbc_text, \
    Node: gwbc_pass)(x)

#define GWB_ELM(tag, ...) \
    gwbc_element(tag, (const Node[]){ GC_MAP(GC_N, __VA_ARGS__) }, GC_NARG(__VA_ARGS__))

/* ---------------------------------------------------------------- the DSL */

#define Main(...) GWB_ELM(23, __VA_ARGS__) /* atom 23 = main */
#define Div(...) GWB_ELM(GWB_DIV, __VA_ARGS__)
#define Span(...) GWB_ELM(GWB_SPAN, __VA_ARGS__)
#define P(...) GWB_ELM(GWB_P, __VA_ARGS__)
#define H1(...) GWB_ELM(GWB_H1, __VA_ARGS__)
#define H2(...) GWB_ELM(GWB_H2, __VA_ARGS__)
#define H3(...) GWB_ELM(GWB_H3, __VA_ARGS__)
#define Button(...) GWB_ELM(GWB_BUTTON, __VA_ARGS__)
#define Input(...) GWB_ELM(GWB_INPUT, __VA_ARGS__)

#define COMPONENT(name, Props, props) static Node name(Props props)
#define RETURN(node) return (node)
#define USE(component, props) component(props)
#define PROPS(Type, ...) ((Type){ __VA_ARGS__ })
#define PROP(name) props.name
#define WHEN(cond, node) ((cond) ? (node) : Empty())
#define IF(cond) if (cond)

#define STATE(type, name, initial) type name = gwbc_use_##type(#name, initial)
#define gwbc_use_i32_alias gwbc_use_i32
#define STATE_STR(name, initial) char *name = gwbc_use_str(#name, initial)
#define SET(name, value) _Generic((value), \
    char *: gwbc_set_str, const char *: gwbc_set_str, default: gwbc_set_i32)(#name, value)
/* Stringizes the TRACKED state's variable name — PREVIOUS(i32, prev, count)
 * reads the "count" slot's history, not a fresh "prev" slot. */
#define PREVIOUS(type, name, value) Previous_##type name = gwbc_use_previous_##type(#value, value)

#define EVENT(name) \
    Handler name = gwbc_handler(#name); \
    if (gwbc_handler_active(name))
#define EVENT_INPUT(name, e) \
    Handler name = gwbc_handler(#name); \
    InputEvent e = { gwbc_input_value(name) }; \
    if (gwbc_handler_active(name))

#define T(...) Textf(__VA_ARGS__)
#define U(...) __VA_ARGS__ /* utility tokens splat into the element's args */
#define CSS(...) __VA_ARGS__

/* attributes + handlers */
#define Type(v) gc_attr(GWB_ATTR_TYPE, v)
#define Value(v) gc_attr(GWB_ATTR_VALUE, v)
#define Placeholder(v) gc_attr(GWB_ATTR_PLACEHOLDER, v)
#define OnClick(h) gc_on((h), GWB_EV_CLICK)
#define OnInput(h) gc_on((h), GWB_EV_INPUT)

/* utility tokens (Tailwind-ish, inline styles) */
#define Block gc_style(GWB_STYLE_DISPLAY, "block")
#define Flex gc_style(GWB_STYLE_DISPLAY, "flex")
#define FlexCol gc_style(211, "column") /* flex-direction */
#define ItemsCenter gc_style(212, "center")
#define Gap(n) gc_style(GWB_STYLE_GAP, gc_rem(n))
#define Pad(n) gc_style(GWB_STYLE_PADDING, gc_rem(n))
#define Px(n) gc_group2(gc_style(1025, gc_rem(n)), gc_style(1026, gc_rem(n)))
#define Py(n) gc_group2(gc_style(1027, gc_rem(n)), gc_style(1028, gc_rem(n)))
#define WFull gc_style(GWB_STYLE_WIDTH, "100%")
#define MaxW(n) gc_style(1029, gc_rem(n))
#define Rounded gc_style(GWB_STYLE_BORDER_RADIUS, "0.25rem")
#define RoundedLg gc_style(GWB_STYLE_BORDER_RADIUS, "0.5rem")
#define RoundedXl gc_style(GWB_STYLE_BORDER_RADIUS, "0.75rem")
#define TextXs gc_style(GWB_STYLE_FONT_SIZE, "0.75rem")
#define TextSm gc_style(GWB_STYLE_FONT_SIZE, "0.875rem")
#define TextLg gc_style(GWB_STYLE_FONT_SIZE, "1.125rem")
#define Text4xl gc_style(GWB_STYLE_FONT_SIZE, "2.25rem")
#define FontSemibold gc_style(209, "600") /* font-weight */
#define FontBold gc_style(209, "700")
#define Cursor(v) gc_style(GWB_STYLE_CURSOR, v)
#define Bg(hex) gc_style(GWB_STYLE_BACKGROUND, hex)
#define Fg(hex) gc_style(GWB_STYLE_COLOR, hex)
#define Border1(hex) gc_style(207, hex) /* border shorthand value like "1px solid #..." */

/* slate/amber palette shortcuts */
#define BgWhite Bg("#ffffff")
#define BgSlate100 Bg("#f1f5f9")
#define BgSlate700 Bg("#334155")
#define BgSlate900 Bg("#0f172a")
#define FgWhite Fg("#ffffff")
#define FgSlate500 Fg("#64748b")
#define FgSlate600 Fg("#475569")
#define FgSlate900 Fg("#0f172a")
#define FgAmber500 Fg("#f59e0b")
#define BorderSlate200 Border1("1px solid #e2e8f0")
#define BorderSlate300 Border1("1px solid #cbd5e1")

/* ---------------------------------------------------------------- app entry */

#define GWB_APP(Component, props) \
    static Node gwb__root(void) { return Component(props); } \
    GWB_EXPORT("gwb_abi_version") u32 gwbc_abi_version_(void) { return 1u << 16; } \
    GWB_EXPORT("gwb_start") void gwbc_start_(f32 w, f32 h, f32 s, u32 f) { \
        (void)w; (void)h; (void)s; (void)f; \
        gwbc_boot(); \
    } \
    GWB_EXPORT("gwb_events") u32 gwbc_events_(u32 count) { \
        return gwb_decode_events(count, gc_on_event); \
    } \
    GWB_EXPORT("gwb_frame") void gwbc_frame_(f32 dt) { (void)dt; }

#endif /* GWBC_H */
