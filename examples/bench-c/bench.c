/* bench-c: DOM-write benchmark guest on the raw GWB LL ABI (gwb.h).
 * No component framework — this measures the boundary itself.
 *
 * Workloads (mirrored in bench/js/bench.html for Chromium):
 *   #b-create1k   mount 1000 rows: div.row > (span "Item N") + (button "x")
 *   #b-create5k   mount 5000 rows (same shape)
 *   #b-update-all set every span's text to "Item N updated"
 *   #b-update-one set span[count/2]'s text
 *   #b-class-all  set class attr on every row
 *   #b-clear      remove all rows (subtree clear)
 *
 * Timing comes from host instrumentation (freestanding C has no clock):
 *   guest_call_us = whole guest execution incl. batch encode
 *   decode_us     = host-side batch decode/validate
 *   apply_us      = DocumentMutator application
 *
 * Build: scripts\build-c.cmd examples\bench-c\bench.c renderer\bench-c.wasm
 */
/* 5k rows * ~9 ops = ~45k ops = ~720 KB of batch: size the LL buffers up. */
#define GWB_OPS_CAP (1024 * 1024)
#define GWB_HEAP_CAP (512 * 1024)
#include "gwb.h"

#define MAX_ROWS 5000

static u32 rows[MAX_ROWS];
static u32 spans[MAX_ROWS]; /* the span's text node ids */
static u32 row_count;
static u32 list_id, status_id;

static u32 btn_create1k, btn_create5k, btn_update_all, btn_update_one,
    btn_class_all, btn_clear;

static char numbuf[64];
static const char *fmt_item(const char *prefix, u32 n, const char *suffix) {
    char *p = numbuf;
    while (*prefix) *p++ = *prefix++;
    p = gwb_append_u32(p, n);
    while (*suffix) *p++ = *suffix++;
    *p = 0;
    return numbuf;
}

static void set_status(const char *s) { gwb_set_text(status_id, s); }

static void clear_rows(void) {
    gwb_clear(list_id);
    row_count = 0;
}

static void create_rows(u32 n) {
    clear_rows();
    for (u32 i = 0; i < n && i < MAX_ROWS; i++) {
        u32 row = gwb_new_id();
        gwb_create_element(row, GWB_DIV);
        gwb_set_attr(row, GWB_ATTR_CLASS, "row");

        u32 span = gwb_new_id();
        gwb_create_element(span, GWB_SPAN);
        u32 txt = gwb_new_id();
        gwb_create_text(txt, fmt_item("Item ", i + 1, ""));
        gwb_append_child(span, txt);
        gwb_append_child(row, span);

        u32 btn = gwb_new_id();
        gwb_create_element(btn, GWB_BUTTON);
        u32 btxt = gwb_new_id();
        gwb_create_text(btxt, "x");
        gwb_append_child(btn, btxt);
        gwb_append_child(row, btn);

        gwb_append_child(list_id, row);
        rows[i] = row;
        spans[i] = txt;
        row_count = i + 1;
    }
    set_status(fmt_item("created ", n, " rows"));
}

static u32 mkbutton(const char *domId, const char *label) {
    u32 b = gwb_new_id();
    gwb_create_element(b, GWB_BUTTON);
    gwb_set_attr(b, GWB_ATTR_ID, domId);
    u32 t = gwb_new_id();
    gwb_create_text(t, label);
    gwb_append_child(b, t);
    gwb_append_child(GWB_ROOT, b);
    gwb_listen(b, GWB_EV_CLICK);
    return b;
}

GWB_EXPORT("gwb_abi_version") u32 gwb_abi_version(void) { return 1u << 16; }

GWB_EXPORT("gwb_start") void gwb_start(f32 w, f32 h, f32 s, u32 f) {
    (void)w; (void)h; (void)s; (void)f;
    gwb_register_event_region();

    btn_create1k = mkbutton("b-create1k", "create 1k");
    btn_create5k = mkbutton("b-create5k", "create 5k");
    btn_update_all = mkbutton("b-update-all", "update all");
    btn_update_one = mkbutton("b-update-one", "update one");
    btn_class_all = mkbutton("b-class-all", "class all");
    btn_clear = mkbutton("b-clear", "clear");

    u32 status = gwb_new_id();
    gwb_create_element(status, GWB_P);
    status_id = gwb_new_id();
    gwb_create_text(status_id, "ready");
    gwb_append_child(status, status_id);
    gwb_append_child(GWB_ROOT, status);

    list_id = gwb_new_id();
    gwb_create_element(list_id, GWB_DIV);
    gwb_append_child(GWB_ROOT, list_id);

    gwb_flush();
}

static u32 on_event(const gwb_event *e) {
    if (e->kind != GWB_EV_CLICK) return 0;

    if (e->listener == btn_create1k) {
        create_rows(1000);
    } else if (e->listener == btn_create5k) {
        create_rows(5000);
    } else if (e->listener == btn_update_all) {
        for (u32 i = 0; i < row_count; i++) {
            gwb_set_text(spans[i], fmt_item("Item ", i + 1, " updated"));
        }
        set_status("updated all");
    } else if (e->listener == btn_update_one) {
        if (row_count > 0) {
            u32 mid = row_count / 2;
            gwb_set_text(spans[mid], fmt_item("Item ", mid + 1, " touched"));
        }
        set_status("updated one");
    } else if (e->listener == btn_class_all) {
        for (u32 i = 0; i < row_count; i++) {
            gwb_set_attr(rows[i], GWB_ATTR_CLASS, "row toggled");
        }
        set_status("classed all");
    } else if (e->listener == btn_clear) {
        clear_rows();
        set_status("cleared");
    }
    return 0;
}

GWB_EXPORT("gwb_events") u32 gwb_events(u32 count) {
    u32 ret = gwb_decode_events(count, on_event);
    gwb_flush();
    return ret;
}

GWB_EXPORT("gwb_frame") void gwb_frame(f32 dt) { (void)dt; }
