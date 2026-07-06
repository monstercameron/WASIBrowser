/* gwbc.h — GoWebComponents shorthand for C, on the GWB ABI.
 *
 * Declarative element builders, prop options, child normalization,
 * conditional nodes, mapped fragments, and event wrappers — the GWC (Go)
 * helper model translated to freestanding C, in two layers:
 *
 * Layer 1 — Go-mirror helpers (capitalized, like the Go package):
 *   Text(x) / Textf(fmt, ...) / Children(...)
 *   If(cond, node) / IfElse(c, a, b) / Unless(cond, node)
 *   Show(cond, node)        stays in the DOM, display:none when false
 *   Range(n, render_fn)     fn-based lists: static Node render(i32 i)
 *   Maybe(ptr, render_fn)   render if non-NULL
 *   Prevent(h) / Stop(h)    handler wrappers (preventDefault/stopPropagation)
 *   PropsOf(...)            prop grouping (additive)
 *
 * Layer 2 — ergonomic authoring aliases (lowercase; components PascalCase):
 *   component(Name, props, PropsType) { ... return view(...); }
 *   Comp(Props(CompProps, .field = v))      direct component calls (no child())
 *   app(Root, { ... })                      exports
 *   div/p/h1/button/input/span/...          variadic tags; bare strings become text
 *   props(...) / class(...) / css(...)      grouping (pure splats, zero runtime)
 *   text("Count: %d", count)                reactive text (mini printf: %s %d %%)
 *   mapRange(i, n, node) / map(it, arr, n, node) / mapKeyed(...)     expr lists
 *   stateI32/stateBool/stateStr, set(name, v), previousI32           hooks
 *   event(name){...} / eventInput(name, e){...}, onClick/onInput     handlers
 *   id("...") type/value/placeholder(...)   attributes
 *
 * Rules of thumb: inside the returned tree use If/Show/Range/map; outside it,
 * write plain C. Props are additive by construction (splats), so "merging"
 * is just passing more of them. Not yet ported from Go: Repeat/Join (need
 * node cloning), FlatMap/FilterMap/Switch/Cond, Debounce/Throttle.
 *
 * RENDER MODEL: each interaction runs the component function TWICE — a settle
 * pass (event bodies execute, set()s land) then the committed render. The DOM
 * is replaced once. Event bodies run exactly once; component-body side
 * effects run twice. Use renderCount() for committed-render counts.
 *
 * Model: immediate mode + full replace. Every event re-renders the whole tree
 * (two passes: handlers run, then a clean render) and replaces the mount's
 * children. Utility tokens dedupe into one generated <style> sheet: the first
 * use of a token emits a rule; after warm-up, re-renders ship class attrs only.
 *
 * FAILURE POLICY: capacity overruns TRAP (gwb.log + __builtin_trap) instead of
 * silently clamping. The host logs the guest fault; the last guest:log line
 * names the exhausted resource. Loud beats degenerate-but-passing.
 *
 * Known limitation: full replace recreates the focused <input>; the framework
 * re-focuses its successor (matched by handler id) but the caret resets to
 * position 0. Fix belongs to a keyed reconciler or host caret save/restore.
 *
 * ERROR CATALOG (macro soup produces bad diagnostics; translate here):
 *  - "controlling expression type ... not compatible with any generic
 *    association" inside div(...)/p(...) etc.
 *      => you passed something that isn't a Node or a string (e.g. a Handler,
 *         an int, a function name without ()). Wrap text in text(...),
 *         handlers in onClick(...)/onInput(...).
 *  - "too many arguments provided to function-like macro invocation" or
 *    GC_F33 undeclared
 *      => an element has more than 32 children/modifiers; split with frag().
 *  - "called object type 'Node' is not a function"
 *      => missing comma between two children, e.g. p(...) div(...).
 *  - weird parse errors on an ordinary identifier named div/p/type/value/...
 *      => you declared a FUNCTION with a DSL macro's name; the macros only
 *         fire on `name(` shapes, so variables are fine but functions clash.
 *  - wasm-ld "undefined symbol: strlen/memcpy/memset"
 *      => you forgot -fno-builtin (clang idiom-recognizes your loops into
 *         libc calls that don't exist). Use scripts/build-c.cmd.
 *  - guest trap right after a "gwbc: ... full" log line
 *      => a capacity below was exhausted; raise the cap.
 *  - garbage/empty text that "should" have a value
 *      => STRING LIFETIME: bare-string children and attr values are stored
 *         BY POINTER and read at emit time, after your component returned.
 *         Literals and state strings are safe; STACK BUFFERS ARE NOT.
 *         Route stack-composed strings through text()/Text()/strf() (arena
 *         copies), or pass numbers as i32 props and format in the child.
 */
#ifndef GWBC_H
#define GWBC_H

#include "gwb.h"

typedef signed int i32;
typedef u32 Handler;

/* ---------------------------------------------------------------- panic */

static void gwbc_panic(const char *msg) {
    gwb_log(GWB_LOG_ERROR, msg);
    __builtin_trap();
}

/* ---------------------------------------------------------------- nodes */

typedef struct { i32 idx; } Node;
/* Children-as-props: a Children field is a Node; build with Children(...).
 * The arena reserves index 0 as a permanent empty node, so a zero-initialized
 * Node/Children field (omitted in a Props literal) renders as nothing. */
typedef Node Children;

enum { K_EMPTY, K_ELEM, K_TEXT, K_STYLE, K_ATTR, K_HANDLER, K_GROUP, K_UTIL };

typedef struct {
    u8 kind;
    u8 variant;      /* K_UTIL: 0 = base, 1 = :hover */
    u16 ev;          /* K_HANDLER: event kind */
    u32 a;           /* tag / style-prop atom / attr atom / handler id */
    const char *s;   /* text / style value / attr value / K_UTIL css prop */
    const char *s2;  /* K_UTIL css value */
    i32 first, last, next;
} GwbcNode;

#define GWBC_MAX_NODES 4096
static GwbcNode gc_nodes[GWBC_MAX_NODES];
static i32 gc_node_count;
static char gc_strpool[48 * 1024];
static u32 gc_strpool_len;

static Node gc_alloc(u8 kind) {
    if (gc_node_count >= GWBC_MAX_NODES) gwbc_panic("gwbc: node arena full (GWBC_MAX_NODES)");
    GwbcNode *n = &gc_nodes[gc_node_count];
    n->kind = kind; n->variant = 0; n->ev = 0; n->a = 0; n->s = 0; n->s2 = 0;
    n->first = n->last = n->next = -1;
    return (Node){ gc_node_count++ };
}

static const char *gc_strdup(const char *s, u32 len) {
    if (gc_strpool_len + len + 1 > sizeof(gc_strpool)) gwbc_panic("gwbc: render string pool full");
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

/* escape hatch: inline style by well-known/dynamic atom */
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
static Node gc_u(const char *prop, const char *value) {
    Node n = gc_alloc(K_UTIL);
    gc_nodes[n.idx].s = prop; gc_nodes[n.idx].s2 = value;
    return n;
}
static void gc_append(Node parent, Node child) {
    /* Node index 0 is the shared zero-value empty sentinel. It must NEVER be
     * linked: appending it to several parents would mutate its `next` pointer
     * and cross-link the tree into cycles (learned via stack-overflow trap). */
    if (child.idx == 0) return;
    GwbcNode *p = &gc_nodes[parent.idx];
    if (p->first < 0) p->first = child.idx; else gc_nodes[p->last].next = child.idx;
    p->last = child.idx;
}
static Node gc_group2(Node a, Node b) {
    Node g = gc_alloc(K_GROUP);
    gc_append(g, a); gc_append(g, b);
    return g;
}
static void gc_set_variant(Node n, u8 variant) {
    GwbcNode *nd = &gc_nodes[n.idx];
    if (nd->kind == K_UTIL) nd->variant = variant;
    else if (nd->kind == K_GROUP)
        for (i32 c = nd->first; c >= 0; c = gc_nodes[c].next)
            gc_set_variant((Node){ c }, variant);
}
static Node gc_hover(Node n) { gc_set_variant(n, 1); return n; }

static Node gwbc_element(u32 tag, const Node *items, u32 n) {
    Node el = gc_alloc(K_ELEM);
    gc_nodes[el.idx].a = tag;
    for (u32 i = 0; i < n; i++) gc_append(el, items[i]);
    return el;
}
static Node gwbc_fragment(const Node *items, u32 n) {
    Node g = gc_alloc(K_GROUP);
    for (u32 i = 0; i < n; i++) gc_append(g, items[i]);
    return g;
}

/* ---------------------------------------------------------------- mini fmt */

static char *gc_fmt_i32(char *dst, i32 v) {
    if (v < 0) { *dst++ = '-'; v = -v; }
    return gwb_append_u32(dst, (u32)v);
}
static void fmtI32(char *dst, i32 v) { *gc_fmt_i32(dst, v) = 0; }

static const char *gc_vfmt(const char *fmt, __builtin_va_list ap) {
    char buf[512];
    char *p = buf, *end = buf + sizeof(buf) - 1;
    for (const char *f = fmt; *f && p < end; f++) {
        if (*f != '%') { *p++ = *f; continue; }
        f++;
        if (*f == 'd') { char tmp[16]; fmtI32(tmp, __builtin_va_arg(ap, i32)); for (char *t = tmp; *t && p < end;) *p++ = *t++; }
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

typedef struct { u8 ok; i32 value; } PreviousI32;

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
    if (gc_state_count >= 64) gwbc_panic("gwbc: state registry full (64 slots)");
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
static PreviousI32 gwbc_use_previous_i32(const char *name, i32 current) {
    (void)current;
    GcState *s = gc_slot(name);
    return (PreviousI32){ s->has_prev, s->prev_i32 };
}

/* ---------------------------------------------------------------- events */

static const char *gc_handler_names[64];
static u32 gc_handler_ret[64]; /* Prevent/Stop flags returned to the host */
static u32 gc_handler_count;
static u32 gc_active_handler = 0xFFFFFFFF;
static const char *gc_event_value = "";

typedef struct { const char *value; } InputEvent;

static Handler gwbc_handler(const char *name) {
    for (u32 i = 0; i < gc_handler_count; i++)
        if (gc_streq(gc_handler_names[i], name)) return i;
    if (gc_handler_count >= 64) gwbc_panic("gwbc: handler registry full (64)");
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

/* GWC-style event wrappers: onClick(Prevent(save)) makes the host cancel
 * the default action; Stop halts propagation. Composable. */
static Handler Prevent(Handler h) { gc_handler_ret[h] |= 1; return h; }
static Handler Stop(Handler h) { gc_handler_ret[h] |= 2; return h; }

/* node-id -> handler map, rebuilt each render */
static u32 gc_hn_node[256]; static u32 gc_hn_handler[256]; static u16 gc_hn_kind[256];
static u32 gc_hn_count;

/* Payload-bound handlers (withI32): per-render table of (base, payload)
 * pairs addressed by synthetic handler ids with the top bit set. */
static u32 gc_hp_base[192]; static i32 gc_hp_payload[192];
static u32 gc_hp_count;
static i32 gc_event_payload;

/* withI32(onToggleTask, task->id): bind an i32 payload to a handler for this
 * render. The receiving event reads it via eventI32(name, arg). */
static Handler withI32(Handler h, i32 payload) {
    if (gc_hp_count >= 192) gwbc_panic("gwbc: payload handler table full (192)");
    gc_hp_base[gc_hp_count] = h;
    gc_hp_payload[gc_hp_count] = payload;
    return 0x80000000u | gc_hp_count++;
}
static u32 gc_resolve_handler(u32 raw, i32 *payload) {
    if (raw & 0x80000000u) {
        u32 i = raw & 0x7FFFFFFFu;
        *payload = gc_hp_payload[i];
        return gc_hp_base[i];
    }
    *payload = 0;
    return raw;
}

/* ---------------------------------------------------------------- utility classes */

/* Persistent token -> class registry. First use of a token mints ".uN{...}"
 * into the generated stylesheet; re-renders then ship only class attrs. */
#define GC_MAX_CLASSES 192
static const char *gc_cl_prop[GC_MAX_CLASSES];
static const char *gc_cl_val[GC_MAX_CLASSES];
static u8 gc_cl_variant[GC_MAX_CLASSES];
static u32 gc_cl_count;
static u8 gc_css_dirty;
static char gc_perm[16 * 1024]; /* persistent strings (class values outlive the render arena) */
static u32 gc_perm_len;

static const char *gc_permdup(const char *s) {
    u32 len = gwb_strlen(s);
    if (gc_perm_len + len + 1 > sizeof(gc_perm)) gwbc_panic("gwbc: class string pool full");
    char *dst = gc_perm + gc_perm_len;
    for (u32 i = 0; i <= len; i++) dst[i] = s[i];
    gc_perm_len += len + 1;
    return dst;
}

static u32 gc_class_for(const char *prop, const char *val, u8 variant) {
    for (u32 i = 0; i < gc_cl_count; i++)
        if (gc_cl_variant[i] == variant && gc_streq(gc_cl_prop[i], prop) && gc_streq(gc_cl_val[i], val))
            return i;
    if (gc_cl_count >= GC_MAX_CLASSES) gwbc_panic("gwbc: class registry full (GC_MAX_CLASSES)");
    gc_cl_prop[gc_cl_count] = gc_permdup(prop);
    gc_cl_val[gc_cl_count] = gc_permdup(val);
    gc_cl_variant[gc_cl_count] = variant;
    gc_css_dirty = 1;
    return gc_cl_count++;
}

static u32 gc_style_text_node; /* text node inside the generated <style> */

static void gc_emit_stylesheet(void) {
    static char css[24 * 1024];
    u32 len = 0;
    for (u32 i = 0; i < gc_cl_count; i++) {
        char head[32];
        char *p = gwb_append_str(head, ".u");
        p = gwb_append_u32(p, i);
        if (gc_cl_variant[i] == 1) p = gwb_append_str(p, ":hover");
        *p = 0;
        const char *parts[6] = { head, "{", gc_cl_prop[i], ":", gc_cl_val[i], "}" };
        for (u32 j = 0; j < 6; j++) {
            const char *s = parts[j];
            while (*s) {
                if (len + 2 >= sizeof(css)) gwbc_panic("gwbc: stylesheet buffer full");
                css[len++] = *s++;
            }
        }
    }
    css[len] = 0;
    gwb_set_text(gc_style_text_node, css);
    gc_css_dirty = 0;
}

/* ---------------------------------------------------------------- emit */

static u32 gc_container; /* persistent mount div; children replaced per render */

static void gc_emit(i32 idx, u32 parent);

/* Apply one modifier/child to an element; utility tokens accumulate class
 * names into classbuf instead of emitting per-node style ops. */
static void gc_apply(i32 idx, u32 elem, char *classbuf, u32 *classlen) {
    GwbcNode *n = &gc_nodes[idx];
    switch (n->kind) {
    case K_UTIL: {
        u32 cls = gc_class_for(n->s, n->s2, n->variant);
        char tmp[16];
        char *p = gwb_append_str(tmp, *classlen ? " u" : "u");
        p = gwb_append_u32(p, cls);
        *p = 0;
        for (char *t = tmp; *t && *classlen < 250; t++) classbuf[(*classlen)++] = *t;
        classbuf[*classlen] = 0;
        break;
    }
    case K_STYLE: gwb_set_style(elem, n->a, n->s); break;
    case K_ATTR: gwb_set_attr(elem, n->a, n->s); break;
    case K_HANDLER:
        gwb_listen(elem, n->ev);
        if (gc_hn_count >= 256) gwbc_panic("gwbc: handler-node map full (256)");
        gc_hn_node[gc_hn_count] = elem;
        gc_hn_handler[gc_hn_count] = n->a;
        gc_hn_kind[gc_hn_count] = n->ev;
        gc_hn_count++;
        break;
    case K_GROUP:
        for (i32 c = n->first; c >= 0; c = gc_nodes[c].next) gc_apply(c, elem, classbuf, classlen);
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
    if (n->kind == K_GROUP) { /* fragment / EACH at child position */
        for (i32 c = n->first; c >= 0; c = gc_nodes[c].next) gc_emit(c, parent);
        return;
    }
    if (n->kind != K_ELEM) return;
    u32 id = gwb_new_id();
    gwb_create_element(id, n->a);
    char classbuf[256];
    u32 classlen = 0;
    classbuf[0] = 0;
    for (i32 c = n->first; c >= 0; c = gc_nodes[c].next) gc_apply(c, id, classbuf, &classlen);
    if (classlen) gwb_set_attr(id, GWB_ATTR_CLASS, classbuf);
    gwb_append_child(parent, id);
}

/* ---------------------------------------------------------------- render loop */

static Node gwb__root(void); /* generated by GWB_APP */

/* Reset per-render storage. Arena index 0 becomes a permanent K_EMPTY node
 * so zero-valued Node/Children fields render as nothing. */
static void gc_arena_reset(void) {
    gc_node_count = 0;
    gc_strpool_len = 0;
    gc_hn_count = 0;
    gc_hp_count = 0;
    (void)gc_alloc(K_EMPTY);
}

static i32 gc_commit_count;
/* Committed renders (one per interaction + the initial mount). NOTE: the
 * component FUNCTION runs twice per interaction — a settle pass that executes
 * event bodies, then the committed render. Keep side effects (counters, logs,
 * business mutations) inside event bodies, which run exactly once; use
 * renderCount() instead of hand-rolled statics in component bodies. */
static i32 renderCount(void) { return gc_commit_count; }

static void gc_render_clean(void) {
    gc_arena_reset();
    gc_commit_count++;
    Node tree = gwb__root();
    gwb_clear(gc_container);
    gc_emit(tree.idx, gc_container);
    if (gc_css_dirty) gc_emit_stylesheet();
}

static void gwbc_render(void) {
    if (gc_active_handler != 0xFFFFFFFF) {
        /* Which event kind fired the active handler? Read from the PREVIOUS
         * render's tables before the reset wipes them (resolving payload-
         * bound synthetic ids down to their base handler). */
        u32 refocus = gc_active_handler;
        u16 refocus_kind = 0;
        for (u32 i = 0; i < gc_hn_count; i++) {
            i32 ignored;
            if (gc_resolve_handler(gc_hn_handler[i], &ignored) == gc_active_handler)
                refocus_kind = gc_hn_kind[i];
        }

        /* Pass 1: run EVENT bodies so SETs settle. Tree discarded. */
        gc_arena_reset();
        (void)gwb__root();
        gc_active_handler = 0xFFFFFFFF;
        gc_event_value = "";

        /* Pass 2: clean render with settled state. */
        gc_render_clean();
        /* Full replace destroyed the focused input; re-focus its successor. */
        if (refocus_kind == GWB_EV_INPUT) {
            for (u32 i = 0; i < gc_hn_count; i++) {
                i32 ignored;
                if (gc_resolve_handler(gc_hn_handler[i], &ignored) == refocus
                    && gc_hn_kind[i] == GWB_EV_INPUT)
                    gwb_focus(gc_hn_node[i]);
            }
        }
    } else {
        gc_render_clean();
    }
    gwb_flush();
}

static u32 gc_on_event(const gwb_event *e) {
    for (u32 i = 0; i < gc_hn_count; i++) {
        if (gc_hn_node[i] == e->listener && gc_hn_kind[i] == e->kind) {
            /* Resolve payload-bound ids against THIS render's table before
             * gwbc_render resets it. */
            i32 payload;
            u32 base = gc_resolve_handler(gc_hn_handler[i], &payload);
            gc_active_handler = base;
            gc_event_payload = payload;
            gc_event_value = e->str;
            gwbc_render();
            return gc_handler_ret[base]; /* Prevent/Stop flags */
        }
    }
    return 0;
}

static void gwbc_boot(void) {
    gwb_register_event_region();
    gwb_define_atom(1030, "style"); /* the <style> element tag */
    /* generated stylesheet lives beside the app container */
    u32 style_el = gwb_new_id();
    gwb_create_element(style_el, 1030);
    gc_style_text_node = gwb_new_id();
    gwb_create_text(gc_style_text_node, "");
    gwb_append_child(style_el, gc_style_text_node);
    gwb_append_child(GWB_ROOT, style_el);

    gc_container = gwb_new_id();
    gwb_create_element(gc_container, GWB_DIV);
    gwb_append_child(GWB_ROOT, gc_container);
    gwbc_render();
}

/* ---------------------------------------------------------------- variadic map */

#define GC_NARG(...) GC_NARG_(__VA_ARGS__, \
    32, 31, 30, 29, 28, 27, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17, \
    16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1)
#define GC_NARG_(_1, _2, _3, _4, _5, _6, _7, _8, _9, _10, _11, _12, _13, _14, _15, _16, \
    _17, _18, _19, _20, _21, _22, _23, _24, _25, _26, _27, _28, _29, _30, _31, _32, N, ...) N
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
#define GC_F17(F, a, ...) F(a), GC_F16(F, __VA_ARGS__)
#define GC_F18(F, a, ...) F(a), GC_F17(F, __VA_ARGS__)
#define GC_F19(F, a, ...) F(a), GC_F18(F, __VA_ARGS__)
#define GC_F20(F, a, ...) F(a), GC_F19(F, __VA_ARGS__)
#define GC_F21(F, a, ...) F(a), GC_F20(F, __VA_ARGS__)
#define GC_F22(F, a, ...) F(a), GC_F21(F, __VA_ARGS__)
#define GC_F23(F, a, ...) F(a), GC_F22(F, __VA_ARGS__)
#define GC_F24(F, a, ...) F(a), GC_F23(F, __VA_ARGS__)
#define GC_F25(F, a, ...) F(a), GC_F24(F, __VA_ARGS__)
#define GC_F26(F, a, ...) F(a), GC_F25(F, __VA_ARGS__)
#define GC_F27(F, a, ...) F(a), GC_F26(F, __VA_ARGS__)
#define GC_F28(F, a, ...) F(a), GC_F27(F, __VA_ARGS__)
#define GC_F29(F, a, ...) F(a), GC_F28(F, __VA_ARGS__)
#define GC_F30(F, a, ...) F(a), GC_F29(F, __VA_ARGS__)
#define GC_F31(F, a, ...) F(a), GC_F30(F, __VA_ARGS__)
#define GC_F32(F, a, ...) F(a), GC_F31(F, __VA_ARGS__)
#define GC_MAP(F, ...) GC_CAT(GC_F, GC_NARG(__VA_ARGS__))(F, __VA_ARGS__)

/* bare strings become text nodes; Nodes pass through */
#define GC_N(x) _Generic((x), \
    char *: gwbc_text, \
    const char *: gwbc_text, \
    Node: gwbc_pass)(x)

#define GWB_ELM(tag, ...) \
    gwbc_element(tag, (const Node[]){ GC_MAP(GC_N, __VA_ARGS__) }, GC_NARG(__VA_ARGS__))

/* ------------------------------------------------------------------ the DSL
 *
 * Lowercase-only public API (React convention: lowercase = host elements,
 * PascalCase = your components; utility tokens stay PascalCase constants).
 * Function-like macros only expand at `name(` call shapes, so `div`, `p`,
 * `type`, `value` etc. cannot corrupt ordinary identifiers — but avoid
 * defining your own functions with these names in app code.
 */

/* -- elements (host tags) -- */
#define main(...) GWB_ELM(23, __VA_ARGS__)
#define div(...) GWB_ELM(GWB_DIV, __VA_ARGS__)
#define span(...) GWB_ELM(GWB_SPAN, __VA_ARGS__)
#define p(...) GWB_ELM(GWB_P, __VA_ARGS__)
#define h1(...) GWB_ELM(GWB_H1, __VA_ARGS__)
#define h2(...) GWB_ELM(GWB_H2, __VA_ARGS__)
#define h3(...) GWB_ELM(GWB_H3, __VA_ARGS__)
#define button(...) GWB_ELM(GWB_BUTTON, __VA_ARGS__)
#define input(...) GWB_ELM(GWB_INPUT, __VA_ARGS__)
#define a(...) GWB_ELM(9, __VA_ARGS__)
#define ul(...) GWB_ELM(11, __VA_ARGS__)
#define li(...) GWB_ELM(13, __VA_ARGS__)
#define section(...) GWB_ELM(20, __VA_ARGS__)
#define header(...) GWB_ELM(21, __VA_ARGS__)
#define footer(...) GWB_ELM(22, __VA_ARGS__)
#define pre(...) GWB_ELM(26, __VA_ARGS__)
#define strong(...) GWB_ELM(28, __VA_ARGS__)
#define em(...) GWB_ELM(29, __VA_ARGS__)

/* -- composition -- */
#define frag(...) \
    gwbc_fragment((const Node[]){ GC_MAP(GC_N, __VA_ARGS__) }, GC_NARG(__VA_ARGS__))
#define view(...) frag(__VA_ARGS__)
#define empty() Empty()

#define component(Name, props, PropsType) static Node Name(PropsType props)
#define component0(Name) static Node Name(void)

/* Components are plain functions returning Node — call them directly, like
 * the Go package. Props is just a compound-literal convenience:
 *   CounterPanel(Props(CounterPanelProps, .name = name, .count = count))
 */
#define Props(Type, ...) ((Type){ __VA_ARGS__ })

/* -- conditionals (Go-mirror capitals; C keywords own the lowercase) -- */
#define If(cond, node) ((cond) ? (node) : Empty())
#define IfElse(cond, then_node, else_node) ((cond) ? (then_node) : (else_node))
#define Unless(cond, node) ((cond) ? Empty() : (node))
/* Show keeps the node in the DOM and toggles visibility (GWC semantics);
 * non-element nodes fall back to If behavior. */
static Node gwbc_show(i32 cond, Node node) {
    if (cond) return node;
    if (gc_nodes[node.idx].kind == K_ELEM) {
        gc_append(node, gc_u("display", "none"));
        return node;
    }
    return Empty();
}
#define Show(cond, node) gwbc_show((i32)(cond) != 0, (node))
#define Maybe(ptr, render_fn) ((ptr) ? (render_fn)(ptr) : Empty())

/* -- fn-based lists (GWC Range; prefer over inline closures C doesn't have) -- */
typedef Node (*RenderIndexFn)(i32 i);
static Node gwbc_range(i32 count, RenderIndexFn render) {
    Node f = gc_alloc(K_GROUP);
    for (i32 i = 0; i < count; i++) gc_append(f, render(i));
    return f;
}
#define Range(count, render_fn) gwbc_range((i32)(count), (render_fn))

/* -- lists (statement expressions; clang/gcc) -- */
#define mapRange(var, count, node_expr) (__extension__({ \
    Node gc__f = gc_alloc(K_GROUP); \
    for (i32 var = 0; var < (i32)(count); var++) gc_append(gc__f, (node_expr)); \
    gc__f; }))
#define map(item, array, len, node_expr) (__extension__({ \
    Node gc__f = gc_alloc(K_GROUP); \
    for (u32 gc__i = 0; gc__i < (u32)(len); gc__i++) { \
        __auto_type item = &(array)[gc__i]; \
        gc_append(gc__f, (node_expr)); \
    } \
    gc__f; }))
/* Keys are accepted (and type-checked) now, used by the future reconciler;
 * today this renders like map(). Keep keys stable and unique. */
#define mapKeyed(item, array, len, key_expr, node_expr) (__extension__({ \
    Node gc__f = gc_alloc(K_GROUP); \
    for (u32 gc__i = 0; gc__i < (u32)(len); gc__i++) { \
        __auto_type item = &(array)[gc__i]; \
        (void)(key_expr); \
        gc_append(gc__f, (node_expr)); \
    } \
    gc__f; }))

/* -- hooks -- */
#define stateI32(name, initial) i32 name = gwbc_use_i32(#name, (initial))
#define stateBool(name, initial) i32 name = gwbc_use_i32(#name, (initial) ? 1 : 0)
#define stateStr(name, initial) char *name = gwbc_use_str(#name, (initial))
/* Enum state is i32-backed; set(name, v) works via the default branch. */
#define stateEnum(Type, name, initial) Type name = (Type)gwbc_use_i32(#name, (i32)(initial))
/* Struct state: persistent storage, initialized once, accessed by POINTER.
 * Mutate it directly (business functions take the pointer); the framework
 * re-renders after every event anyway. There is deliberately no set() for
 * structs — _Generic cannot dispatch arbitrary user types (the open-generics
 * wall), and copy-then-set would only fake immutability C doesn't have. */
#define stateStruct(Type, name, initFn) \
    static Type name##_state; \
    static u8 name##_inited; \
    if (!name##_inited) { name##_inited = 1; initFn(&name##_state); } \
    Type *name = &name##_state
#define set(name, val) _Generic((val), \
    char *: gwbc_set_str, const char *: gwbc_set_str, default: gwbc_set_i32)(#name, val)
/* Stringizes the TRACKED state's variable name — previousI32(prev, count)
 * reads the "count" slot's history, not a fresh "prev" slot. */
#define previousI32(name, val) PreviousI32 name = gwbc_use_previous_i32(#val, val)

#define event(name) \
    Handler name = gwbc_handler(#name); \
    if (gwbc_handler_active(name))
/* eventInput/eventI32 scope their payload variable inside a run-once for
 * statement, so multiple handlers may reuse the same variable name. */
#define eventInput(name, e) \
    Handler name = gwbc_handler(#name); \
    if (gwbc_handler_active(name)) \
        for (InputEvent e = { gwbc_input_value(name) }; e.value; e.value = 0)
/* Payload-receiving handler: pair with withI32(name, someI32) at bind sites.
 *   eventI32(toggleTask, taskId) { TaskStore_Toggle(store, taskId); } */
#define eventI32(name, arg) \
    Handler name = gwbc_handler(#name); \
    if (gwbc_handler_active(name)) \
        for (i32 arg = gc_event_payload, gc__go = 1; gc__go; gc__go = 0)

/* -- context (immediate-mode: a value stack scoped by provider()) --
 * context(ThemeContext, ThemeValue);            file scope
 * provider(ThemeContext, theme, ...children)    in-tree (statement expr:
 *   children are built AFTER the push — eager C evaluation is defeated)
 * ThemeValue t = useContext(ThemeContext);      any depth below */
#define context(Name, Type) \
    static Type Name##_ctx_stack[8]; \
    static i32 Name##_ctx_depth
#define provider(Name, val, ...) (__extension__({ \
    if (Name##_ctx_depth >= 8) gwbc_panic("gwbc: context depth (8)"); \
    Name##_ctx_stack[Name##_ctx_depth++] = (val); \
    Node gc__n = frag(__VA_ARGS__); \
    Name##_ctx_depth--; \
    gc__n; }))
#define useContext(Name) \
    (Name##_ctx_stack[Name##_ctx_depth > 0 ? Name##_ctx_depth - 1 : 0])

/* -- text + children -- */
static Node gwbc_text_i32(i32 v) {
    char buf[16];
    fmtI32(buf, v);
    return gwbc_text(gc_strdup(buf, gwb_strlen(buf)));
}
/* Text(any-ish): strings and i32 normalize to text nodes (GWC Text(any)). */
#define Text(x) _Generic((x), \
    char *: gwbc_text, const char *: gwbc_text, default: gwbc_text_i32)(x)
#define text(...) Textf(__VA_ARGS__)
#define Children(...) frag(__VA_ARGS__)

/* -- prop grouping (pure splats: additive by construction, zero runtime) -- */
#define props(...) __VA_ARGS__
#define PropsOf(...) __VA_ARGS__
#define class(...) __VA_ARGS__
#define css(...) __VA_ARGS__
#define U(...) __VA_ARGS__ /* utility token group, class(U(Flex, Gap(2))) */

/* propGroup reifies prop options as a single Node VALUE — for passing prop
 * bundles through Props fields (a splat can't cross a struct boundary):
 *   .rootExtra = propGroup(BgSlate50, Pad(6))    ...applied additively */
#define propGroup(...) \
    gwbc_fragment((const Node[]){ __VA_ARGS__ }, GC_NARG(__VA_ARGS__))
/* Conditional class/prop group: classIf(task->done, Opacity60, LineThrough) */
#define classIf(cond, ...) ((cond) ? propGroup(__VA_ARGS__) : (Node){ 0 })

/* -- attributes + handlers -- */
#define id(v) gc_attr(GWB_ATTR_ID, v)
#define testId(v) gc_attr(GWB_ATTR_ID, v)
#define type(v) gc_attr(GWB_ATTR_TYPE, v)
#define value(v) gc_attr(GWB_ATTR_VALUE, v)
#define placeholder(v) gc_attr(GWB_ATTR_PLACEHOLDER, v)
#define onClick(h) gc_on((h), GWB_EV_CLICK)
#define onInput(h) gc_on((h), GWB_EV_INPUT)
/* Presence attribute: disabled(cond) emits disabled="" only when true.
 * Blitz honors it natively (disabled elements don't hit-test as clicks). */
#define disabled(cond) ((cond) ? gc_attr(GWB_ATTR_DISABLED, "") : (Node){ 0 })

/* -- small helpers -- */
static i32 minI32(i32 a, i32 b) { return a < b ? a : b; }
static i32 maxI32(i32 a, i32 b) { return a > b ? a : b; }
static u8 strEq(const char *a, const char *b) { return gc_streq(a ? a : "", b ? b : ""); }
/* logf: formatted line to the in-window console + system log. Put these in
 * EVENT BODIES (run once per interaction), not component bodies (run twice). */
#define logf(...) gwb_log(GWB_LOG_INFO, strf(__VA_ARGS__))
#define logWarnf(...) gwb_log(GWB_LOG_WARN, strf(__VA_ARGS__))

/* strf: arena-formatted string (valid through this render) — for attribute
 * values like id(strf("task-%d", t->id)). For TEXT children prefer text(). */
static const char *strf(const char *fmt, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, fmt);
    const char *s = gc_vfmt(fmt, ap);
    __builtin_va_end(ap);
    return s;
}

/* utility tokens (Tailwind-ish) — compiled to classes + one <style> sheet */
#define Hover(token) gc_hover(token)
#define Block gc_u("display", "block")
#define Flex gc_u("display", "flex")
#define FlexCol gc_u("flex-direction", "column")
#define ItemsCenter gc_u("align-items", "center")
#define JustifyBetween gc_u("justify-content", "space-between")
#define Gap(n) gc_u("gap", gc_rem(n))
#define Pad(n) gc_u("padding", gc_rem(n))
#define Px(n) gc_group2(gc_u("padding-left", gc_rem(n)), gc_u("padding-right", gc_rem(n)))
#define Py(n) gc_group2(gc_u("padding-top", gc_rem(n)), gc_u("padding-bottom", gc_rem(n)))
#define WFull gc_u("width", "100%")
#define MaxW(n) gc_u("max-width", gc_rem(n))
#define Rounded gc_u("border-radius", "0.25rem")
#define RoundedLg gc_u("border-radius", "0.5rem")
#define RoundedXl gc_u("border-radius", "0.75rem")
#define TextXs gc_u("font-size", "0.75rem")
#define TextSm gc_u("font-size", "0.875rem")
#define TextLg gc_u("font-size", "1.125rem")
#define Text4xl gc_u("font-size", "2.25rem")
#define FontSemibold gc_u("font-weight", "600")
#define FontBold gc_u("font-weight", "700")
#define Cursor(v) gc_u("cursor", v)
#define Bg(hex) gc_u("background", hex)
#define Fg(hex) gc_u("color", hex)
#define Border1(v) gc_u("border", v)
#define Grid gc_u("display", "grid")
#define GridCols2 gc_u("grid-template-columns", "repeat(2, minmax(0, 1fr))")
#define GridCols4 gc_u("grid-template-columns", "repeat(4, minmax(0, 1fr))")
#define MxAuto gc_group2(gc_u("margin-left", "auto"), gc_u("margin-right", "auto"))
#define Opacity60 gc_u("opacity", "0.6")
#define LineThrough gc_u("text-decoration", "line-through")

/* slate/amber/red palette shortcuts */
#define BgWhite Bg("#ffffff")
#define BgSlate50 Bg("#f8fafc")
#define BgSlate100 Bg("#f1f5f9")
#define BgSlate700 Bg("#334155")
#define BgSlate900 Bg("#0f172a")
#define BgRed600 Bg("#dc2626")
#define BgRed700 Bg("#b91c1c")
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

/* app(StarterApp, { .title = "...", .initial_count = 0 }) */
#define app(Component, ...) GWB_APP(Component, ((Component##Props)__VA_ARGS__))

#endif /* GWBC_H */
