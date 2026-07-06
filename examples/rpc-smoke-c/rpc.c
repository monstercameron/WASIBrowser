/* rpc-smoke: the smallest end-to-end proof of the RPC interconnect.
 *
 * On page load it fires gwb_rpc_call("echo", ...) and renders the reply when
 * the GWB_EV_RPC_RESULT event arrives. Proves rpc_call -> host -> Go server ->
 * RPC_RESULT -> guest. Build: scripts/build-c.cmd examples/rpc-smoke-c/rpc.c
 * renderer/rpc-smoke.wasm ; run with the dev server on 127.0.0.1:8787.
 *
 *   clang --target=wasm32-unknown-unknown -O2 -nostdlib -fno-builtin \
 *     -Wl,--no-entry -Wl,--export-memory -I sdk-c -o rpc-smoke.wasm rpc.c
 */
#include "gwb.h"

enum { ROOT = 1 };
static u32 status_id, body_id;
static u32 pending_req;

static void build(void) {
    /* atoms already cover div/h1; use them directly */
    u32 title = gwb_new_id();
    gwb_create_element(title, 4 /* h1 */);
    gwb_create_text(gwb_new_id(), "RPC smoke test"); /* text node id unused after append */
    /* simpler: set text via SetText on the element */
    gwb_set_text(title, "RPC smoke test");
    gwb_set_style(title, 208 /*font-size*/, "22px");
    gwb_append_child(ROOT, title);

    status_id = gwb_new_id();
    gwb_create_element(status_id, 1 /* div */);
    gwb_set_text(status_id, "calling echo service...");
    gwb_set_style(status_id, 201 /*color*/, "#b45309");
    gwb_append_child(ROOT, status_id);

    body_id = gwb_new_id();
    gwb_create_element(body_id, 26 /* pre */);
    gwb_set_text(body_id, "");
    gwb_set_style(body_id, 202 /*background*/, "#f3f4f6");
    gwb_set_style(body_id, 206 /*padding*/, "12px");
    gwb_set_style(body_id, 214 /*border-radius*/, "8px");
    gwb_append_child(ROOT, body_id);
    gwb_flush();
}

static u32 on_event(const gwb_event *e) {
    if (e->kind == GWB_EV_PAGE_LOAD) {
        pending_req = gwb_rpc_call("echo", "gwb.echo.v1", "echo",
                                   "{\"hello\":\"from wasm\",\"n\":42}", 0);
        if (pending_req == 0) {
            gwb_set_text(status_id, "rpc_call FAILED (undeclared service / buffer)");
            gwb_set_style(status_id, 201, "#dc2626");
            gwb_flush();
        } else {
            gwb_log(GWB_LOG_INFO, "rpc-smoke: fired echo call");
        }
        return 0;
    }
    if (e->kind == GWB_EV_RPC_RESULT && e->rpcReqId == pending_req) {
        if (e->rpcOk) {
            gwb_set_text(status_id, "RPC OK — echo replied:");
            gwb_set_style(status_id, 201, "#059669");
            gwb_set_text(body_id, e->str);
        } else {
            gwb_set_text(status_id, "RPC FAILED");
            gwb_set_style(status_id, 201, "#dc2626");
            gwb_set_text(body_id, e->str);
        }
        gwb_flush();
        gwb_log(GWB_LOG_INFO, "rpc-smoke: got result");
    }
    return 0;
}

GWB_EXPORT("gwb_abi_version") u32 gwb_abi_version(void) { return (1u << 16) | 0u; }

GWB_EXPORT("gwb_start")
void gwb_start(f32 w, f32 h, f32 scale, u32 flags) {
    (void)w; (void)h; (void)scale; (void)flags;
    gwb_register_event_region();
    gwb_listen(ROOT, GWB_EV_PAGE_LOAD);
    build();
}

GWB_EXPORT("gwb_events")
u32 gwb_events(u32 count) { return gwb_decode_events(count, on_event); }
