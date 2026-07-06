/* todo-c: the tri-language Todo spec (examples/TODO_SPEC.md) in C.
 * Freestanding wasm32 — no libc, static memory only.
 *
 * Build:
 *   clang --target=wasm32-unknown-unknown -O2 -nostdlib \
 *         -Wl,--no-entry -Wl,--export-memory -I../../sdk-c \
 *         -o todo-c.wasm todo.c
 */
#include "gwb.h"

#define ATOM_TEXT_DECORATION 1024u
#define MAX_TODOS 1024

typedef struct {
    u32 row, label, text, x;
    u8 done, alive;
} todo_t;

static todo_t todos[MAX_TODOS];
static u32 todo_count;   /* high-water mark (slots are tombstoned) */
static u32 live_count, done_count, created;

static u32 list_id, status_text, input_id, add_btn, hundred_btn;
static char input_value[256];

static u32 text_element(u32 tag, const char *text) {
    u32 el = gwb_new_id();
    gwb_create_element(el, tag);
    u32 t = gwb_new_id();
    gwb_create_text(t, text);
    gwb_append_child(el, t);
    return el;
}

static void add_todo(const char *text) {
    created++;
    char namebuf[64];
    if (!text[0]) {
        char *p = gwb_append_str(namebuf, "Item ");
        p = gwb_append_u32(p, created);
        *p = 0;
        text = namebuf;
    }
    if (todo_count >= MAX_TODOS) return;

    u32 row = gwb_new_id();
    gwb_create_element(row, GWB_DIV);
    gwb_set_style(row, GWB_STYLE_DISPLAY, "flex");
    gwb_set_style(row, GWB_STYLE_GAP, "8px");
    gwb_set_style(row, GWB_STYLE_MARGIN, "0 0 6px 0");

    u32 label = gwb_new_id();
    gwb_create_element(label, GWB_SPAN);
    gwb_set_style(label, GWB_STYLE_CURSOR, "pointer");
    u32 label_text = gwb_new_id();
    gwb_create_text(label_text, text);
    gwb_append_child(label, label_text);
    gwb_append_child(row, label);

    u32 x = text_element(GWB_BUTTON, "x");
    gwb_append_child(row, x);

    gwb_append_child(list_id, row);
    gwb_listen(label, GWB_EV_CLICK);
    gwb_listen(x, GWB_EV_CLICK);

    todo_t *t = &todos[todo_count++];
    t->row = row; t->label = label; t->text = label_text; t->x = x;
    t->done = 0; t->alive = 1;
    live_count++;
}

static void update_status(void) {
    char buf[64];
    char *p = gwb_append_u32(buf, live_count);
    p = gwb_append_str(p, " items, ");
    p = gwb_append_u32(p, done_count);
    p = gwb_append_str(p, " done");
    *p = 0;
    gwb_set_text(status_text, buf);
}

static todo_t *find_by_label(u32 id) {
    for (u32 i = 0; i < todo_count; i++)
        if (todos[i].alive && todos[i].label == id) return &todos[i];
    return 0;
}
static todo_t *find_by_x(u32 id) {
    for (u32 i = 0; i < todo_count; i++)
        if (todos[i].alive && todos[i].x == id) return &todos[i];
    return 0;
}

static u32 on_event(const gwb_event *e) {
    if (e->kind == GWB_EV_INPUT && e->listener == input_id) {
        u32 n = e->str_len < sizeof(input_value) - 1 ? e->str_len : sizeof(input_value) - 1;
        for (u32 i = 0; i < n; i++) input_value[i] = e->str[i];
        input_value[n] = 0;
    } else if (e->kind == GWB_EV_CLICK) {
        if (e->listener == add_btn) {
            add_todo(input_value);
            input_value[0] = 0;
            gwb_set_attr(input_id, GWB_ATTR_VALUE, "");
            update_status();
        } else if (e->listener == hundred_btn) {
            for (int i = 0; i < 100; i++) add_todo("");
            update_status();
            gwb_log(GWB_LOG_INFO, "+100 encoded (guest, no clock in freestanding C)");
        } else {
            todo_t *t = find_by_label(e->listener);
            if (t) {
                t->done = !t->done;
                if (t->done) {
                    done_count++;
                    gwb_set_style(t->label, GWB_STYLE_COLOR, "#9a9fa6");
                    gwb_set_style(t->label, ATOM_TEXT_DECORATION, "line-through");
                } else {
                    done_count--;
                    gwb_set_style(t->label, GWB_STYLE_COLOR, "#e8e8e8");
                    gwb_remove_style(t->label, ATOM_TEXT_DECORATION);
                }
                update_status();
            } else if ((t = find_by_x(e->listener)) != 0) {
                if (t->done) done_count--;
                t->alive = 0;
                live_count--;
                gwb_remove(t->row);
                update_status();
            }
        }
    }
    return 0;
}

/* ---- exports ---- */

GWB_EXPORT("gwb_abi_version") u32 gwb_abi_version(void) { return 1u << 16; }

GWB_EXPORT("gwb_start") void gwb_start(f32 w, f32 h, f32 scale, u32 flags) {
    (void)w; (void)h; (void)scale; (void)flags;
    gwb_register_event_region();

    gwb_define_atom(ATOM_TEXT_DECORATION, "text-decoration");

    u32 card = gwb_new_id();
    gwb_create_element(card, GWB_DIV);
    gwb_set_style(card, GWB_STYLE_PADDING, "20px");
    gwb_set_style(card, GWB_STYLE_BACKGROUND, "#26282c");
    gwb_set_style(card, GWB_STYLE_BORDER_RADIUS, "10px");
    gwb_set_style(card, GWB_STYLE_WIDTH, "420px");
    gwb_append_child(GWB_ROOT, card);

    u32 heading = text_element(GWB_H2, "Todos \xe2\x80\x94 C");
    gwb_set_style(heading, GWB_STYLE_MARGIN, "0 0 12px 0");
    gwb_append_child(card, heading);

    u32 row = gwb_new_id();
    gwb_create_element(row, GWB_DIV);
    gwb_set_style(row, GWB_STYLE_DISPLAY, "flex");
    gwb_set_style(row, GWB_STYLE_GAP, "8px");
    gwb_set_style(row, GWB_STYLE_MARGIN, "0 0 14px 0");
    gwb_append_child(card, row);

    input_id = gwb_new_id();
    gwb_create_element(input_id, GWB_INPUT);
    gwb_set_attr(input_id, GWB_ATTR_TYPE, "text");
    gwb_set_attr(input_id, GWB_ATTR_PLACEHOLDER, "What needs doing?");
    gwb_set_style(input_id, GWB_STYLE_WIDTH, "240px");
    gwb_append_child(row, input_id);
    gwb_listen(input_id, GWB_EV_INPUT);

    add_btn = text_element(GWB_BUTTON, "Add");
    gwb_append_child(row, add_btn);
    gwb_listen(add_btn, GWB_EV_CLICK);

    hundred_btn = text_element(GWB_BUTTON, "+100");
    gwb_append_child(row, hundred_btn);
    gwb_listen(hundred_btn, GWB_EV_CLICK);

    list_id = gwb_new_id();
    gwb_create_element(list_id, GWB_DIV);
    gwb_append_child(card, list_id);

    u32 status = gwb_new_id();
    gwb_create_element(status, GWB_P);
    status_text = gwb_new_id();
    gwb_create_text(status_text, "0 items, 0 done");
    gwb_append_child(status, status_text);
    gwb_set_style(status, GWB_STYLE_MARGIN, "10px 0 0 0");
    gwb_set_style(status, GWB_STYLE_FONT_SIZE, "12px");
    gwb_set_style(status, GWB_STYLE_COLOR, "#9a9fa6");
    gwb_append_child(card, status);

    gwb_flush();
}

GWB_EXPORT("gwb_events") u32 gwb_events(u32 count) {
    u32 ret = gwb_decode_events(count, on_event);
    gwb_flush();
    return ret;
}

GWB_EXPORT("gwb_frame") void gwb_frame(f32 dt_ms) { (void)dt_ms; }
