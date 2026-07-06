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
 *
 * Layer 2 — ergonomic authoring aliases (lowercase; components PascalCase):
 *   component(Name, props, PropsType) { ... return view(...); }
 *   Comp(Props(CompProps, .field = v))      direct component calls (no child())
 *   app(Root, { ... })                      exports
 *   div/p/h1/button/input/span/...          variadic tags; bare strings become text
 *   class(U(...)) + attrs go directly in    (no props() wrapper — removed)
 *   text("Count: %d", count)                reactive text (mini printf: %s %d %%)
 *   mapRange/map/mapKeyed/mapKeyedIf, bindI32(handler, i32)          lists
 *   useQuery(key, url) / refetchQuery(key)   non-blocking async data (host
 *     does the HTTP; completion re-renders — React Query, minus the queue)
 *   atomI32/atomStr + useAtom/setAtom        shared global state, no drilling
 *   memoI32/memoStr(key, deps, expr)         deps-cached derived values
 *   keyedId("key")                           stable guest id of a keyed node
 *
 * REACT PARITY MAP: useState=stateX · useEffect ✓ · useContext ✓ ·
 * useMemo=memoX · jotai-atoms ✓ · useRef=keyedId + C statics ·
 * useCallback=N/A (handlers are static fns) · useReducer=event body + switch ·
 * Suspense≈useQuery.loading · keys/reconciliation ✓ · portals/error
 * boundaries: not yet (traps are the error story).
 *   stateI32/stateBool/stateStr, set(name, v), previousI32           hooks
 *   event(name){...} / eventInput(name, e){...}, onClick/onInput     handlers
 *   eventKey/eventPointer/eventWheel/eventResize(name, e){...}       payloads
 *   onDblClick/onContextMenu/onKeyDown/onKeyUp/onFocus/onBlur/
 *   onPointer*(+ onMouseEnter/Leave, onHover pair)/onWheel/onScroll  events
 *   onLoad/onWindowResize/onThemeChange       window-level (root-subscribed)
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

enum { K_EMPTY, K_ELEM, K_TEXT, K_STYLE, K_ATTR, K_HANDLER, K_GROUP, K_UTIL, K_KEY };

typedef struct {
    u8 kind;
    u8 variant;      /* K_UTIL: 0 = base, 1 = :hover */
    u16 ev;          /* K_HANDLER: event kind */
    u32 a;           /* tag / style-prop atom / attr atom / handler id */
    const char *s;   /* text / style value / attr value / K_UTIL css prop */
    const char *s2;  /* K_UTIL css value */
    u64 keyv;        /* K_KEY: identity key */
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

/* -------------------------------------------------- keyed identity registry
 * The minimal correctness reconciler: full replace by default, but elements
 * carrying a key REUSE their host node across renders (moved into the new
 * tree before the old one is cleared). Preserves the state that lives ON
 * host nodes: input focus + caret, scroll offsets, widget state, CSS
 * transitions. Not a diffing reconciler — attrs/classes are re-set, children
 * of a reused node are rebuilt. Limitation: keyed nodes nested inside other
 * keyed nodes are dropped by the parent's child-rebuild (do not nest keys).
 */
#define GWBC_MAX_KEYED 256

typedef struct {
    u64 key;
    u32 guestId;
    u8 live, seen, reused;
} GcKeyed;

static GcKeyed gc_keyed[GWBC_MAX_KEYED];
static u32 gc_keyed_count;
static u32 gc_reuse_count_last; /* diagnostics: nodes kept last commit */

static GcKeyed *gc_keyed_find(u64 key) {
    for (u32 i = 0; i < gc_keyed_count; i++)
        if (gc_keyed[i].live && gc_keyed[i].key == key) return &gc_keyed[i];
    return 0;
}
static void gc_keyed_register(u64 key, u32 guestId) {
    for (u32 i = 0; i < gc_keyed_count; i++) {
        GcKeyed *k = &gc_keyed[i];
        if (!k->live) {
            k->key = key; k->guestId = guestId;
            k->live = 1; k->seen = 1; k->reused = 0;
            return;
        }
    }
    if (gc_keyed_count >= GWBC_MAX_KEYED) gwbc_panic("gwbc: keyed registry full");
    GcKeyed *k = &gc_keyed[gc_keyed_count++];
    k->key = key; k->guestId = guestId;
    k->live = 1; k->seen = 1; k->reused = 0;
}
static u8 gc_id_was_reused(u32 guestId) {
    for (u32 i = 0; i < gc_keyed_count; i++)
        if (gc_keyed[i].live && gc_keyed[i].guestId == guestId && gc_keyed[i].reused)
            return 1;
    return 0;
}

static Node gc_key_node(u64 key) {
    Node n = gc_alloc(K_KEY);
    gc_nodes[n.idx].keyv = key;
    return n;
}

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
/* forward decl: setters flag the effect re-render loop */
static u8 gc_state_dirtied;

static void gwbc_set_i32(const char *name, i32 v) {
    GcState *s = gc_slot(name);
    if (s->v_i32 != v) { s->prev_i32 = s->v_i32; s->has_prev = 1; }
    s->v_i32 = v;
    gc_state_dirtied = 1;
}
static void gwbc_set_str(const char *name, const char *v) {
    GcState *s = gc_slot(name);
    u32 i = 0; while (v[i] && i < 127) { s->v_str[i] = v[i]; i++; } s->v_str[i] = 0;
    gc_state_dirtied = 1;
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

/* Window-level hook: listens on the mount root (guest id 1) no matter which
 * element the node sits in — the host targets root for load/resize/theme. */
static Node gc_on_root(Handler h, u16 kind) {
    Node n = gc_on(h, kind);
    gc_nodes[n.idx].variant = 1;
    return n;
}

/* GWC-style event wrappers: onClick(Prevent(save)) makes the host cancel
 * the default action; Stop halts propagation. Composable. */
static Handler Prevent(Handler h) { gc_handler_ret[h] |= 1; return h; }
static Handler Stop(Handler h) { gc_handler_ret[h] |= 2; return h; }

/* --------------------------------------------------------------- effects
 * useEffect: after-COMMIT side-effect hook. Render records it (commit pass
 * only — the settle pass ignores effects), the framework runs it after the
 * tree is applied. Keyed identity (like state), not call-order identity.
 *
 *   flow: render records -> commit applies tree -> cleanups -> runs
 *   deps: u64 fingerprints — deps0() mount-once, depsI32/depsPtr/depsStr,
 *         deps2(a, b) to combine. Change -> cleanup(old ctx) + run(new ctx).
 *   unmount: a key not seen during a commit render is cleaned up + retired.
 *   userdata: COPIED into effect-owned storage (never a live stack pointer):
 *     useEffectCtx("k", run, cleanup, ((Ctx){ .a = 1 }), depsI32(x));
 *   Effects may set() state: the commit loop re-renders (bounded) until
 *   settled. Division of labor: useQuery = server data; useEffect =
 *   subscriptions/timers/imperative sync; state = local; events = input.
 */
typedef u64 Deps;
typedef void (*EffectFn)(void *userdata);

static Deps gc_deps_mix(u64 h, u64 v) {
    h ^= v + 0x9E3779B97F4A7C15ull + (h << 6) + (h >> 2);
    return h;
}
static Deps deps0(void) { return 0x9E3779B97F4A7C15ull; }
static Deps depsI32(i32 v) { return gc_deps_mix(deps0(), (u64)(u32)v); }
static Deps depsPtr(const void *p) { return gc_deps_mix(deps0(), (u64)(unsigned long)p); }
static Deps depsStr(const char *s) {
    u64 h = 1469598103934665603ull;
    while (s && *s) h = (h ^ (u8)*s++) * 1099511628211ull;
    return h;
}
static Deps deps2(Deps a, Deps b) { return gc_deps_mix(a, b); }

#define GWBC_MAX_EFFECTS 32
#define GWBC_EFFECT_CTX 64

typedef struct {
    const char *key;
    Deps deps;
    EffectFn run, cleanup;
    u8 inited, seen, pendingRun, pendingCleanup;
    EffectFn oldCleanup;
    char ctx[GWBC_EFFECT_CTX];
    u32 ctxLen;
    char oldCtx[GWBC_EFFECT_CTX];
    u32 oldCtxLen;
} GcEffect;

static GcEffect gc_effects[GWBC_MAX_EFFECTS];
static u32 gc_effect_count;
static u8 gc_committing;      /* effects record only during the commit pass */
static u8 gc_state_dirtied;   /* set() ran (drives the effect re-render loop) */

static void gwbc_use_effect(const char *key, EffectFn run, EffectFn cleanup,
                            const void *ctx, u32 ctxLen, Deps deps) {
    if (!gc_committing) return;
    if (ctxLen > GWBC_EFFECT_CTX) gwbc_panic("gwbc: effect ctx too big (GWBC_EFFECT_CTX)");

    GcEffect *e = 0;
    for (u32 i = 0; i < gc_effect_count; i++)
        if (gc_streq(gc_effects[i].key, key)) { e = &gc_effects[i]; break; }
    if (!e) {
        if (gc_effect_count >= GWBC_MAX_EFFECTS) gwbc_panic("gwbc: effect registry full");
        e = &gc_effects[gc_effect_count++];
        e->key = key;
        e->inited = 0;
    }
    e->seen = 1;

    if (!e->inited) {
        e->inited = 1;
        e->deps = deps;
        e->run = run;
        e->cleanup = cleanup;
        e->ctxLen = ctxLen;
        for (u32 i = 0; i < ctxLen; i++) e->ctx[i] = ((const char *)ctx)[i];
        e->pendingRun = 1;
        return;
    }
    if (e->deps != deps) {
        /* cleanup must see the OLD context */
        e->pendingCleanup = e->cleanup != 0;
        e->oldCleanup = e->cleanup;
        e->oldCtxLen = e->ctxLen;
        for (u32 i = 0; i < e->ctxLen; i++) e->oldCtx[i] = e->ctx[i];

        e->deps = deps;
        e->run = run;
        e->cleanup = cleanup;
        e->ctxLen = ctxLen;
        for (u32 i = 0; i < ctxLen; i++) e->ctx[i] = ((const char *)ctx)[i];
        e->pendingRun = 1;
    }
}

#define useEffect(key, run, cleanup, deps) gwbc_use_effect(key, run, cleanup, 0, 0, deps)
/* ctx is an lvalue or compound literal; it is COPIED (stack-safe). */
#define useEffectCtx(key, run, cleanup, ctxVal, deps) \
    gwbc_use_effect(key, run, cleanup, &(ctxVal), sizeof(ctxVal), deps)

static void gc_run_effects(void) {
    /* 1) unmount: initialized effects whose key wasn't seen this commit */
    for (u32 i = 0; i < gc_effect_count; i++) {
        GcEffect *e = &gc_effects[i];
        if (e->inited && !e->seen) {
            if (e->cleanup) e->cleanup(e->ctxLen ? e->ctx : 0);
            e->inited = 0;
            e->pendingRun = 0;
            e->pendingCleanup = 0;
        }
    }
    /* 2) cleanups from deps changes (old context) */
    for (u32 i = 0; i < gc_effect_count; i++) {
        GcEffect *e = &gc_effects[i];
        if (e->pendingCleanup) {
            e->pendingCleanup = 0;
            if (e->oldCleanup) e->oldCleanup(e->oldCtxLen ? e->oldCtx : 0);
        }
    }
    /* 3) runs */
    for (u32 i = 0; i < gc_effect_count; i++) {
        GcEffect *e = &gc_effects[i];
        if (e->pendingRun) {
            e->pendingRun = 0;
            e->run(e->ctxLen ? e->ctx : 0);
        }
    }
}

/* ------------------------------------------------------------------ atoms
 * Shared global state with a typed handle (Jotai/GWC-UseAtom shaped).
 * Define once at file scope; read/write from ANY component — no prop
 * drilling. Backed by the same registry as state; writes re-render.
 *   atomI32(interactionCount, 0);            file scope
 *   i32 n = useAtom(interactionCount);       any component
 *   setAtom(interactionCount, n + 1);        any event/effect
 */
typedef struct { const char *key; i32 initial; } AtomI32;
typedef struct { const char *key; const char *initial; } AtomStr;
#define atomI32(name, initialVal) static const AtomI32 name = { #name, initialVal }
#define atomStr(name, initialVal) static const AtomStr name = { #name, initialVal }

static i32 gwbc_use_atom_i32(AtomI32 a) { return gwbc_use_i32(a.key, a.initial); }
static char *gwbc_use_atom_str(AtomStr a) { return gwbc_use_str(a.key, a.initial); }
static void gwbc_set_atom_i32(AtomI32 a, i32 v) { gwbc_set_i32(a.key, v); }
static void gwbc_set_atom_str(AtomStr a, const char *v) { gwbc_set_str(a.key, v); }

#define useAtom(a) _Generic((a), AtomI32: gwbc_use_atom_i32, AtomStr: gwbc_use_atom_str)(a)
#define setAtom(a, v) _Generic((a), AtomI32: gwbc_set_atom_i32, AtomStr: gwbc_set_atom_str)(a, v)

/* ------------------------------------------------------------------- memo
 * Deps-cached derived values (useMemo). The expression re-evaluates only
 * when the deps fingerprint changes; otherwise the cached value returns.
 *   const char *s = memoStr("summary", depsI32(total), strf("%d tasks", total));
 */
#define GWBC_MAX_MEMOS 16

typedef struct { const char *key; Deps deps; i32 value; u8 has; } GcMemoI32;
typedef struct { const char *key; Deps deps; char value[256]; u8 has; } GcMemoStr;
static GcMemoI32 gc_memos_i32[GWBC_MAX_MEMOS];
static u32 gc_memo_i32_count;
static GcMemoStr gc_memos_str[GWBC_MAX_MEMOS];
static u32 gc_memo_str_count;

static GcMemoI32 *gc_memo_i32_slot(const char *key) {
    for (u32 i = 0; i < gc_memo_i32_count; i++)
        if (gc_streq(gc_memos_i32[i].key, key)) return &gc_memos_i32[i];
    if (gc_memo_i32_count >= GWBC_MAX_MEMOS) gwbc_panic("gwbc: memo registry full");
    GcMemoI32 *m = &gc_memos_i32[gc_memo_i32_count++];
    m->key = key; m->has = 0;
    return m;
}
static GcMemoStr *gc_memo_str_slot(const char *key) {
    for (u32 i = 0; i < gc_memo_str_count; i++)
        if (gc_streq(gc_memos_str[i].key, key)) return &gc_memos_str[i];
    if (gc_memo_str_count >= GWBC_MAX_MEMOS) gwbc_panic("gwbc: memo registry full");
    GcMemoStr *m = &gc_memos_str[gc_memo_str_count++];
    m->key = key; m->has = 0;
    return m;
}

#define memoI32(keyStr, depsv, expr) (__extension__({ \
    GcMemoI32 *gc__m = gc_memo_i32_slot(keyStr); \
    Deps gc__d = (depsv); \
    if (!gc__m->has || gc__m->deps != gc__d) { \
        gc__m->has = 1; gc__m->deps = gc__d; gc__m->value = (expr); \
    } \
    gc__m->value; }))
#define memoStr(keyStr, depsv, expr) (__extension__({ \
    GcMemoStr *gc__m = gc_memo_str_slot(keyStr); \
    Deps gc__d = (depsv); \
    if (!gc__m->has || gc__m->deps != gc__d) { \
        gc__m->has = 1; gc__m->deps = gc__d; \
        const char *gc__s = (expr); \
        u32 gc__i = 0; \
        while (gc__s[gc__i] && gc__i < 255) { gc__m->value[gc__i] = gc__s[gc__i]; gc__i++; } \
        gc__m->value[gc__i] = 0; \
    } \
    (const char *)gc__m->value; }))

/* ------------------------------------------------------------ query cache
 * React-Query-shaped async: useQuery(key, url) never blocks — it returns the
 * cached state and starts a host-side fetch if idle/stale. Completion
 * arrives as a NET_RESULT event; the framework stores the body and
 * re-renders (full replace = every subscriber sees fresh state).
 * Continuation context lives HERE (persistent statics), never on the stack.
 */
enum { QUERY_IDLE, QUERY_LOADING, QUERY_SUCCESS, QUERY_ERROR };

/* Stale-while-revalidate: a refetch keeps serving the previous data (no
 * Loading flash); `fetching` says a request is in flight, `loading` only
 * when there is nothing to show yet. */
typedef struct {
    i32 loading;    /* in flight AND no data yet (first load) */
    i32 fetching;   /* in flight (first load OR background refetch) */
    i32 ok;         /* status == QUERY_SUCCESS (data valid, maybe refetching) */
    i32 err;        /* status == QUERY_ERROR */
    u16 httpStatus; /* 0 = transport error */
    const char *data; /* response body ("" until first success), persistent */
} QueryResult;

#define GWBC_MAX_QUERIES 16
#ifndef GWBC_QUERY_POOL_SIZE
#define GWBC_QUERY_POOL_SIZE (64 * 1024)
#endif

typedef struct {
    const char *key;
    u8 status;
    u8 inFlight;
    u8 stale;
    u32 reqId;
    u16 httpStatus;
    u32 dataOff; /* into gc_qpool; 0xFFFFFFFF = none */
} GcQuery;

static GcQuery gc_queries[GWBC_MAX_QUERIES];
static u32 gc_query_count;
static char gc_qpool[GWBC_QUERY_POOL_SIZE];
static u32 gc_qpool_len;

static GcQuery *gc_query_slot(const char *key) {
    for (u32 i = 0; i < gc_query_count; i++)
        if (gc_streq(gc_queries[i].key, key)) return &gc_queries[i];
    if (gc_query_count >= GWBC_MAX_QUERIES) gwbc_panic("gwbc: query cache full");
    GcQuery *q = &gc_queries[gc_query_count++];
    q->key = key; q->status = QUERY_IDLE; q->reqId = 0;
    q->inFlight = 0; q->stale = 0;
    q->httpStatus = 0; q->dataOff = 0xFFFFFFFFu;
    return q;
}

static QueryResult useQuery(const char *key, const char *url) {
    GcQuery *q = gc_query_slot(key);
    if (!q->inFlight && (q->status == QUERY_IDLE || q->stale)) {
        q->reqId = gwb_fetch(url);
        q->inFlight = 1;
        q->stale = 0;
        if (q->status == QUERY_IDLE) q->status = QUERY_LOADING;
        /* refetch of resolved data: status stays SUCCESS/ERROR — the old
         * data keeps rendering (no Loading flash) */
    }
    QueryResult r = {0};
    r.loading = q->status == QUERY_LOADING;
    r.fetching = q->inFlight;
    r.ok = q->status == QUERY_SUCCESS;
    r.err = q->status == QUERY_ERROR;
    r.httpStatus = q->httpStatus;
    r.data = q->dataOff == 0xFFFFFFFFu ? "" : gc_qpool + q->dataOff;
    return r;
}

/* Mark a query stale: the next render starts a background refetch while
 * the previous data keeps rendering. (Old bodies stay in the append-only
 * pool — demo-grade; a real pool would compact.) */
static void refetchQuery(const char *key) {
    gc_query_slot(key)->stale = 1;
}

static void gc_query_complete(const gwb_event *e) {
    for (u32 i = 0; i < gc_query_count; i++) {
        GcQuery *q = &gc_queries[i];
        if (q->reqId != e->target || !q->inFlight) continue;
        q->inFlight = 0;
        if (gc_qpool_len + e->str_len + 1 > GWBC_QUERY_POOL_SIZE)
            gwbc_panic("gwbc: query pool full (GWBC_QUERY_POOL_SIZE)");
        q->dataOff = gc_qpool_len;
        for (u32 j = 0; j < e->str_len; j++) gc_qpool[gc_qpool_len++] = e->str[j];
        gc_qpool[gc_qpool_len++] = 0;
        q->httpStatus = e->netStatus;
        q->status = e->netOk ? QUERY_SUCCESS : QUERY_ERROR;
        return;
    }
}

/* node-id -> handler map, rebuilt each render */
static u32 gc_hn_node[256]; static u32 gc_hn_handler[256]; static u16 gc_hn_kind[256];
static u32 gc_hn_count;

/* Payload-bound handlers (withI32): per-render table of (base, payload)
 * pairs addressed by synthetic handler ids with the top bit set. */
static u32 gc_hp_base[192]; static i32 gc_hp_payload[192];
static u32 gc_hp_count;
static i32 gc_event_payload;

/* bindI32(onToggleTask, task->id): EAGERLY bind an i32 payload to a handler
 * for this render. Lightweight: one slot in a per-render table, no allocation,
 * no cleanup. The receiving event reads it via eventI32(name, arg). */
static Handler bindI32(Handler h, i32 payload) {
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

/* Base CSS injected ahead of the utility rules (plugins set this — e.g.
 * gwbc-tw.h's Preflight). Scope selectors to #mount. */
static const char *gc_base_css;

static void gc_emit_stylesheet(void) {
    static char css[24 * 1024];
    u32 len = 0;
    if (gc_base_css) {
        const char *s = gc_base_css;
        while (*s) {
            if (len + 2 >= sizeof(css)) gwbc_panic("gwbc: stylesheet buffer full");
            css[len++] = *s++;
        }
    }
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
    case K_HANDLER: {
        /* variant 1 = window-level hook (onLoad/onWindowResize/onThemeChange):
         * the host delivers those to the mount root, not the element the node
         * happens to sit in. */
        u32 target = n->variant ? GWB_ROOT : elem;
        gwb_listen(target, n->ev);
        if (gc_hn_count >= 256) gwbc_panic("gwbc: handler-node map full (256)");
        gc_hn_node[gc_hn_count] = target;
        gc_hn_handler[gc_hn_count] = n->a;
        gc_hn_kind[gc_hn_count] = n->ev;
        gc_hn_count++;
        break;
    }
    case K_GROUP:
        for (i32 c = n->first; c >= 0; c = gc_nodes[c].next) gc_apply(c, elem, classbuf, classlen);
        break;
    case K_KEY: break; /* consumed by gc_emit's key scan */
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

    /* Keyed identity: a direct K_KEY child marks this element as keyed. */
    u64 key = 0;
    for (i32 c = n->first; c >= 0; c = gc_nodes[c].next) {
        if (gc_nodes[c].kind == K_KEY) { key = gc_nodes[c].keyv; break; }
    }

    u32 id;
    GcKeyed *slot = key ? gc_keyed_find(key) : 0;
    if (slot) {
        /* Reuse the surviving host node: move it into the new tree (blitz
         * re-parents on append), rebuild its children, re-set attrs/classes.
         * Focus/caret/scroll/widget state riding the node survives. */
        id = slot->guestId;
        slot->seen = 1;
        slot->reused = 1;
        gc_reuse_count_last++;
        gwb_clear(id);
    } else {
        id = gwb_new_id();
        gwb_create_element(id, n->a);
        if (key) gc_keyed_register(key, id);
    }
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
    for (u32 i = 0; i < gc_effect_count; i++) gc_effects[i].seen = 0;
    for (u32 i = 0; i < gc_keyed_count; i++) {
        gc_keyed[i].seen = 0;
        gc_keyed[i].reused = 0;
    }
    gc_reuse_count_last = 0;

    gc_committing = 1;
    Node tree = gwb__root();
    gc_committing = 0;

    /* Emit into a fresh wrapper FIRST: keyed reuse moves surviving nodes out
     * of the old tree while it is still alive. Then drop the remnants and
     * attach the new wrapper. */
    u32 wrap = gwb_new_id();
    gwb_create_element(wrap, GWB_DIV);
    gc_emit(tree.idx, wrap);
    gwb_clear(gc_container);
    gwb_append_child(gc_container, wrap);

    /* Keys not seen this commit died with the old tree. */
    for (u32 i = 0; i < gc_keyed_count; i++)
        if (gc_keyed[i].live && !gc_keyed[i].seen) gc_keyed[i].live = 0;

    if (gc_css_dirty) gc_emit_stylesheet();
}

/* Commit + effects loop: effects run after the tree is applied; if an
 * effect set() state, re-render (bounded) until settled. */
static void gc_commit_cycle(void) {
    for (i32 gc__it = 0; gc__it < 3; gc__it++) {
        gc_render_clean();
        gwb_flush();
        gc_state_dirtied = 0;
        gc_run_effects();
        if (!gc_state_dirtied) break;
    }
    gwb_flush(); /* ops emitted directly by effects */
    if (gc_reuse_count_last) {
        char buf[64];
        char *p = gwb_append_str(buf, "gwbc: kept ");
        p = gwb_append_u32(p, gc_reuse_count_last);
        p = gwb_append_str(p, " keyed node(s)");
        *p = 0;
        gwb_log(GWB_LOG_DEBUG, buf);
    }
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

        /* Pass 2: clean render with settled state, then effects. */
        gc_commit_cycle();
        /* Re-focus the replaced input's successor — but NOT if the node was
         * keyed and reused: it never lost focus, and re-focusing would stomp
         * the (now genuinely preserved) caret position. */
        if (refocus_kind == GWB_EV_INPUT) {
            for (u32 i = 0; i < gc_hn_count; i++) {
                i32 ignored;
                if (gc_resolve_handler(gc_hn_handler[i], &ignored) == refocus
                    && gc_hn_kind[i] == GWB_EV_INPUT
                    && !gc_id_was_reused(gc_hn_node[i]))
                    gwb_focus(gc_hn_node[i]);
            }
        }
    } else {
        gc_commit_cycle();
    }
    gwb_flush();
}

/* Full record of the event being handled — the typed payload views
 * (eventPointer/eventKey/eventWheel/eventResize) read from here. Valid only
 * while the handler's settle pass runs. */
static gwb_event gc_event_info;

static u32 gc_on_event(const gwb_event *e) {
    if (e->kind == GWB_EV_NET_RESULT) {
        gc_query_complete(e);
        gwbc_render(); /* plain re-render: all useQuery readers see new state */
        return 0;
    }
    for (u32 i = 0; i < gc_hn_count; i++) {
        if (gc_hn_node[i] == e->listener && gc_hn_kind[i] == e->kind) {
            /* Resolve payload-bound ids against THIS render's table before
             * gwbc_render resets it. */
            i32 payload;
            u32 base = gc_resolve_handler(gc_hn_handler[i], &payload);
            gc_active_handler = base;
            gc_event_payload = payload;
            gc_event_value = e->str;
            gc_event_info = *e;
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

/* -- elements (host tags) --
 *
 * EVERY renderable HTML tag, lowercase (React host-element convention;
 * components stay PascalCase). Atom ids mirror
 * renderer/src/abi.rs well_known_atoms() / docs/ABI.md.
 * Deliberately absent: document-level + scripting tags (html/head/meta/
 * script/iframe/object/embed) — guests own #mount only, and no JS exists.
 * CAUTION: short names (a, b, i, q, s, u, p, var, time, data, main, ...) are
 * function-like macros — they only expand at a `name(` call shape, so plain
 * identifiers are safe, but NEVER define app functions with these names
 * (see the error catalog entry on DSL-named functions). */
#define main(...) GWB_ELM(23, __VA_ARGS__)
#define div(...) GWB_ELM(GWB_DIV, __VA_ARGS__)
#define span(...) GWB_ELM(GWB_SPAN, __VA_ARGS__)
#define p(...) GWB_ELM(GWB_P, __VA_ARGS__)
#define h1(...) GWB_ELM(GWB_H1, __VA_ARGS__)
#define h2(...) GWB_ELM(GWB_H2, __VA_ARGS__)
#define h3(...) GWB_ELM(GWB_H3, __VA_ARGS__)
#define h4(...) GWB_ELM(33, __VA_ARGS__)
#define h5(...) GWB_ELM(34, __VA_ARGS__)
#define h6(...) GWB_ELM(35, __VA_ARGS__)
#define button(...) GWB_ELM(GWB_BUTTON, __VA_ARGS__)
#define input(...) GWB_ELM(GWB_INPUT, __VA_ARGS__)
#define a(...) GWB_ELM(9, __VA_ARGS__)
#define img(...) GWB_ELM(10, __VA_ARGS__)
#define ul(...) GWB_ELM(11, __VA_ARGS__)
#define ol(...) GWB_ELM(12, __VA_ARGS__)
#define li(...) GWB_ELM(13, __VA_ARGS__)
#define table(...) GWB_ELM(14, __VA_ARGS__)
#define tr(...) GWB_ELM(15, __VA_ARGS__)
#define td(...) GWB_ELM(16, __VA_ARGS__)
#define th(...) GWB_ELM(17, __VA_ARGS__)
#define form(...) GWB_ELM(18, __VA_ARGS__)
#define label(...) GWB_ELM(19, __VA_ARGS__)
#define section(...) GWB_ELM(20, __VA_ARGS__)
#define header(...) GWB_ELM(21, __VA_ARGS__)
#define footer(...) GWB_ELM(22, __VA_ARGS__)
#define nav(...) GWB_ELM(24, __VA_ARGS__)
#define article(...) GWB_ELM(25, __VA_ARGS__)
#define pre(...) GWB_ELM(26, __VA_ARGS__)
#define code(...) GWB_ELM(27, __VA_ARGS__)
#define strong(...) GWB_ELM(28, __VA_ARGS__)
#define em(...) GWB_ELM(29, __VA_ARGS__)
#define textarea(...) GWB_ELM(30, __VA_ARGS__)
#define select(...) GWB_ELM(31, __VA_ARGS__)
#define option(...) GWB_ELM(32, __VA_ARGS__)
#define aside(...) GWB_ELM(36, __VA_ARGS__)
#define address(...) GWB_ELM(37, __VA_ARGS__)
#define blockquote(...) GWB_ELM(38, __VA_ARGS__)
#define hr(...) GWB_ELM(39, __VA_ARGS__)
#define br(...) GWB_ELM(40, __VA_ARGS__)
#define figure(...) GWB_ELM(41, __VA_ARGS__)
#define figcaption(...) GWB_ELM(42, __VA_ARGS__)
#define small(...) GWB_ELM(43, __VA_ARGS__)
#define mark(...) GWB_ELM(44, __VA_ARGS__)
#define kbd(...) GWB_ELM(45, __VA_ARGS__)
#define samp(...) GWB_ELM(46, __VA_ARGS__)
#define var(...) GWB_ELM(47, __VA_ARGS__)
#define cite(...) GWB_ELM(48, __VA_ARGS__)
#define abbr(...) GWB_ELM(49, __VA_ARGS__)
#define q(...) GWB_ELM(50, __VA_ARGS__)
#define sub(...) GWB_ELM(51, __VA_ARGS__)
#define sup(...) GWB_ELM(52, __VA_ARGS__)
#define time(...) GWB_ELM(53, __VA_ARGS__)
#define data(...) GWB_ELM(54, __VA_ARGS__)
#define b(...) GWB_ELM(55, __VA_ARGS__)
#define i(...) GWB_ELM(56, __VA_ARGS__)
#define u(...) GWB_ELM(57, __VA_ARGS__)
#define s(...) GWB_ELM(58, __VA_ARGS__)
#define wbr(...) GWB_ELM(59, __VA_ARGS__)
#define dfn(...) GWB_ELM(60, __VA_ARGS__)
#define del(...) GWB_ELM(61, __VA_ARGS__)
#define ins(...) GWB_ELM(62, __VA_ARGS__)
#define dl(...) GWB_ELM(63, __VA_ARGS__)
#define dt(...) GWB_ELM(64, __VA_ARGS__)
#define dd(...) GWB_ELM(65, __VA_ARGS__)
#define menu(...) GWB_ELM(66, __VA_ARGS__)
#define caption(...) GWB_ELM(67, __VA_ARGS__)
#define thead(...) GWB_ELM(68, __VA_ARGS__)
#define tbody(...) GWB_ELM(69, __VA_ARGS__)
#define tfoot(...) GWB_ELM(70, __VA_ARGS__)
#define colgroup(...) GWB_ELM(71, __VA_ARGS__)
#define col(...) GWB_ELM(72, __VA_ARGS__)
#define optgroup(...) GWB_ELM(73, __VA_ARGS__)
#define fieldset(...) GWB_ELM(74, __VA_ARGS__)
#define legend(...) GWB_ELM(75, __VA_ARGS__)
#define datalist(...) GWB_ELM(76, __VA_ARGS__)
#define output(...) GWB_ELM(77, __VA_ARGS__)
#define progress(...) GWB_ELM(78, __VA_ARGS__)
#define meter(...) GWB_ELM(79, __VA_ARGS__)
#define details(...) GWB_ELM(80, __VA_ARGS__)
#define summary(...) GWB_ELM(81, __VA_ARGS__)
#define dialog(...) GWB_ELM(82, __VA_ARGS__)
#define video(...) GWB_ELM(83, __VA_ARGS__)
#define audio(...) GWB_ELM(84, __VA_ARGS__)
#define source(...) GWB_ELM(85, __VA_ARGS__)
#define canvas(...) GWB_ELM(86, __VA_ARGS__)
#define picture(...) GWB_ELM(87, __VA_ARGS__)
#define track(...) GWB_ELM(88, __VA_ARGS__)

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
/* Keyed lists: each item's root element carries an identity key (the
 * call-site line salts it, so different lists may reuse the same ids).
 * Same key across renders => the HOST NODE is preserved (moved, not
 * recreated): focus, caret, scroll and widget state survive. */
#define GC_KEYSALT ((u64)(__LINE__) * 2654435761ull)
#define mapKeyed(item, array, len, key_expr, node_expr) (__extension__({ \
    Node gc__f = gc_alloc(K_GROUP); \
    for (u32 gc__i = 0; gc__i < (u32)(len); gc__i++) { \
        __auto_type item = &(array)[gc__i]; \
        Node gc__n = (node_expr); \
        if (gc_nodes[gc__n.idx].kind == K_ELEM) \
            gc_append(gc__n, gc_key_node(gc_deps_mix(GC_KEYSALT, (u64)(u32)(key_expr)))); \
        gc_append(gc__f, gc__n); \
    } \
    gc__f; }))
/* Filter + map in one: renders node_expr only for items where cond_expr
 * holds. Keeps call sites from nesting If inside mapKeyed. */
#define mapKeyedIf(item, array, len, key_expr, cond_expr, node_expr) (__extension__({ \
    Node gc__f = gc_alloc(K_GROUP); \
    for (u32 gc__i = 0; gc__i < (u32)(len); gc__i++) { \
        __auto_type item = &(array)[gc__i]; \
        if (cond_expr) { \
            Node gc__n = (node_expr); \
            if (gc_nodes[gc__n.idx].kind == K_ELEM) \
                gc_append(gc__n, gc_key_node(gc_deps_mix(GC_KEYSALT, (u64)(u32)(key_expr)))); \
            gc_append(gc__f, gc__n); \
        } \
    } \
    gc__f; }))

/* Explicit identity for a single stateful element (inputs, scroll areas):
 *   input(keyed("draft"), value(v), onInput(h), ...) */
#define keyed(strKey) gc_key_node(depsStr(strKey))

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

/* -- typed event payload views --
 * Scoped structs over the delivering event's record; use with the matching
 * on* attachment. All fields are copies except KeyEvent.key (valid for the
 * handler body only).
 *   eventKey(draftKey, k) { if (strEq(k.key, "Enter")) ...; }   onKeyDown
 *   eventPointer(rowEnter, pt) { ... pt.x, pt.y ... }           pointer family
 *   eventWheel(spin, w) { if (w.dy < 0) ...; }                  onWheel
 *   eventResize(loaded, v) { ... v.w, v.h, v.scale ... }        onLoad/onWindowResize */
typedef struct { f32 x, y; i32 buttons, mods; } PointerEvent;
typedef struct { const char *key; i32 mods, pressed; } KeyEvent;
typedef struct { f32 dx, dy; i32 mods; } WheelEvent;
typedef struct { f32 w, h, scale; } ResizeEvent;

#define eventPointer(name, e) \
    Handler name = gwbc_handler(#name); \
    if (gwbc_handler_active(name)) \
        for (PointerEvent e = { gc_event_info.x, gc_event_info.y, \
                gc_event_info.buttons, gc_event_info.mods }, \
             *gc__p = &e; gc__p; gc__p = 0)
#define eventKey(name, e) \
    Handler name = gwbc_handler(#name); \
    if (gwbc_handler_active(name)) \
        for (KeyEvent e = { gc_event_value, gc_event_info.mods, \
                gc_event_info.pressed }, \
             *gc__p = &e; gc__p; gc__p = 0)
#define eventWheel(name, e) \
    Handler name = gwbc_handler(#name); \
    if (gwbc_handler_active(name)) \
        for (WheelEvent e = { gc_event_info.dx, gc_event_info.dy, \
                gc_event_info.mods }, \
             *gc__p = &e; gc__p; gc__p = 0)
#define eventResize(name, e) \
    Handler name = gwbc_handler(#name); \
    if (gwbc_handler_active(name)) \
        for (ResizeEvent e = { gc_event_info.w, gc_event_info.h, \
                gc_event_info.scale }, \
             *gc__p = &e; gc__p; gc__p = 0)

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

/* -- prop grouping (pure splats: additive by construction, zero runtime).
 * Attributes/handlers/classes go DIRECTLY into elements:
 *   div(class(U(Flex, Gap(2))), id("x"), onClick(h), children...)
 * (props()/PropsOf() wrappers existed and were removed: pure ceremony.) */
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
/* The full browser interaction-event surface. Every kind the ABI forwards
 * has an on* attachment; payloads via the typed event views
 * (eventPointer/eventKey/eventWheel/eventResize) or eventInput for value. */
#define onClick(h) gc_on((h), GWB_EV_CLICK)
#define onDblClick(h) gc_on((h), GWB_EV_DBLCLICK)
#define onContextMenu(h) gc_on((h), GWB_EV_CONTEXT_MENU)
#define onInput(h) gc_on((h), GWB_EV_INPUT)
#define onTextInput(h) gc_on((h), GWB_EV_TEXT_INPUT)
#define onKeyDown(h) gc_on((h), GWB_EV_KEY_DOWN)
#define onKeyUp(h) gc_on((h), GWB_EV_KEY_UP)
#define onFocus(h) gc_on((h), GWB_EV_FOCUS)
#define onBlur(h) gc_on((h), GWB_EV_BLUR)
#define onPointerDown(h) gc_on((h), GWB_EV_POINTER_DOWN)
#define onPointerUp(h) gc_on((h), GWB_EV_POINTER_UP)
#define onPointerMove(h) gc_on((h), GWB_EV_POINTER_MOVE)
#define onPointerEnter(h) gc_on((h), GWB_EV_POINTER_ENTER)
#define onPointerLeave(h) gc_on((h), GWB_EV_POINTER_LEAVE)
#define onPointerCancel(h) gc_on((h), GWB_EV_POINTER_CANCEL)
/* React-familiar aliases + a paired hover convenience */
#define onMouseEnter(h) onPointerEnter(h)
#define onMouseLeave(h) onPointerLeave(h)
#define onHover(enterH, leaveH) gc_group2(onPointerEnter(enterH), onPointerLeave(leaveH))
#define onWheel(h) gc_on((h), GWB_EV_WHEEL)
#define onScroll(h) gc_on((h), GWB_EV_SCROLL)
/* Window-level hooks — attach anywhere in the tree; they subscribe the mount
 * root. onLoad fires ONCE, right after the initial tree is applied. */
#define onLoad(h) gc_on_root((h), GWB_EV_PAGE_LOAD)
#define onWindowResize(h) gc_on_root((h), GWB_EV_WINDOW_RESIZE)
#define onThemeChange(h) gc_on_root((h), GWB_EV_THEME_CHANGE)
/* Presence attribute: disabled(cond) emits disabled="" only when true.
 * Blitz honors it natively (disabled elements don't hit-test as clicks). */
#define disabled(cond) ((cond) ? gc_attr(GWB_ATTR_DISABLED, "") : (Node){ 0 })

/* -- refs: the DOM-ref equivalent. A keyed element's guest id is stable
 * across renders; use it for imperative LL ops (gwb_focus etc.) from
 * event/effect bodies. Returns 0 if the key has not rendered yet. -- */
static u32 keyedId(const char *strKey) {
    GcKeyed *k = gc_keyed_find(depsStr(strKey));
    return k ? k->guestId : 0;
}

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
