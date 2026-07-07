/* shop.c — Aurelia storefront (clothing & accessories) in reactified C.
 *
 * A full store on the GWB ABI: catalog grid + category filter, product detail,
 * cart, login, checkout, confirmation, and an admin orders view — every screen
 * driven by authenticated RPC to the Go server (docs/04-WEB-RPC.md). Reads via
 * useRpc (declarative, cached), mutations via gwbc_rpc (completion callbacks).
 *
 * Build:  clang --target=wasm32-unknown-unknown -O2 -nostdlib -fno-builtin \
 *           -Wl,--no-entry -Wl,--export-memory -I sdk-c -o shop.wasm shop.c
 * Run:    (dev server on :8787)  renderer web://shop.local --manifest-root ../manifests
 */
#include "gwbc.h"
#include "gwbjson.h"

enum { V_CATALOG, V_PRODUCT, V_CART, V_LOGIN, V_CHECKOUT, V_CONFIRM, V_ADMIN };

/* Cross-cutting state as atoms so event handlers AND RPC completion callbacks
 * (which run outside component scope) can both update it. */
atomI32(gView, V_CATALOG);
atomStr(gCategory, "all");
atomStr(gProductId, "");
atomI32(gLoggedIn, 0);
atomStr(gUserName, "");
atomStr(gUserRole, "");
atomStr(gAuthErr, "");
atomStr(gLastOrder, "");
atomI32(gCartCount, 0);

static void go(i32 view) { setAtom(gView, view); }

/* ---------------------------------------------------------------- model */
typedef struct {
    char id[48], name[72], category[24], image[12], blurb[200];
    i32 price, stock, featured;
} Product;
/* Parse helpers allocate their result arrays with the library's per-render
 * scratch allocator (renderArr) — no fixed caps, freed wholesale each render,
 * no arena plumbing in app code. Each returns the count + array ptr via `out`. */
static i32 js_arr_len(const char *arr) {
    i32 n = 0;
    for (const char *e = js_arr_first(arr); e; e = js_arr_next(e)) n++;
    return n;
}

static void read_product(const char *e, Product *p) {
    js_get_str(e, "id", p->id, sizeof p->id);
    js_get_str(e, "name", p->name, sizeof p->name);
    js_get_str(e, "category", p->category, sizeof p->category);
    js_get_str(e, "image", p->image, sizeof p->image);
    js_get_str(e, "blurb", p->blurb, sizeof p->blurb);
    p->price = js_get_int(e, "price");
    p->stock = js_get_int(e, "stock");
    p->featured = js_get_bool(e, "featured");
}
static i32 parse_products(const char *json, Product **out) {
    const char *arr = js_find(json, "products");
    i32 n = js_arr_len(arr), i = 0;
    Product *ps = renderArr(Product, n);
    for (const char *e = js_arr_first(arr); e && i < n; e = js_arr_next(e))
        read_product(e, &ps[i++]);
    *out = ps;
    return n;
}

typedef struct { char id[48], name[72], img[12], cat[24]; i32 qty, lineTotal; } CartLine;
static i32 parse_cart(const char *json, CartLine **out, i32 *subtotal) {
    *subtotal = js_get_int(json, "subtotal");
    const char *items = js_find(json, "items");
    i32 n = js_arr_len(items), i = 0;
    CartLine *ls = renderArr(CartLine, n);
    for (const char *e = js_arr_first(items); e && i < n; e = js_arr_next(e)) {
        CartLine *l = &ls[i++];
        const char *prod = js_find(e, "product");
        js_get_str(prod, "id", l->id, sizeof l->id);
        js_get_str(prod, "name", l->name, sizeof l->name);
        js_get_str(prod, "image", l->img, sizeof l->img);
        js_get_str(prod, "category", l->cat, sizeof l->cat);
        l->qty = js_get_int(e, "qty");
        l->lineTotal = js_get_int(e, "lineTotal");
    }
    *out = ls;
    return n;
}

typedef struct { char id[32], sub[64]; i32 subtotal; } AdminOrder;
static i32 parse_orders(const char *json, AdminOrder **out, i32 *total) {
    *total = 0;
    const char *arr = js_find(json, "orders");
    i32 n = js_arr_len(arr), i = 0;
    AdminOrder *os = renderArr(AdminOrder, n);
    for (const char *e = js_arr_first(arr); e && i < n; e = js_arr_next(e)) {
        AdminOrder *o = &os[i++];
        js_get_str(e, "id", o->id, sizeof o->id);
        js_get_str(e, "sub", o->sub, sizeof o->sub);
        o->subtotal = js_get_int(e, "subtotal");
        *total += o->subtotal;
    }
    *out = os;
    return n;
}

/* gwbc strf is a mini-printf (%s %d %% only) — pad the cents manually. */
static const char *price_str(i32 cents) {
    i32 c = cents % 100;
    return strf("$%d.%d%d", cents / 100, c / 10, c % 10);
}

/* Product tiles use an elegant serif monogram on a category-tinted ground
 * instead of external images (the runtime is self-contained — no remote
 * assets). initial() = first letter; tint class keys the gradient. */
static const char *initial(const char *s) {
    static char b[2];
    b[0] = (s && s[0]) ? s[0] : '?';
    b[1] = 0;
    return b;
}

/* ---------------------------------------------------------------- RPC callbacks */
static char g_token[512];
static void onLoginDone(i32 ok, i32 ec, i32 status, const char *body, void *ud) {
    (void)ud; (void)status;
    if (ok) {
        char role[16], name[64];
        js_get_str(body, "token", g_token, sizeof g_token);
        js_get_str(body, "role", role, sizeof role);
        js_get_str(body, "name", name, sizeof name);
        gwb_session_set(g_token);
        setAtom(gLoggedIn, 1);
        setAtom(gUserRole, role);
        setAtom(gUserName, name);
        setAtom(gAuthErr, "");
        invalidate("cart");
        go(V_CATALOG);
        logf("[shop] logged in as %s (%s)", name, role);
    } else {
        setAtom(gAuthErr, ec == GWB_RPC_ERR_AUTHN ? "Invalid email or password."
                                                  : "Login failed. Please try again.");
    }
}
static void onCartChanged(i32 ok, i32 ec, i32 status, const char *body, void *ud) {
    (void)ec; (void)status; (void)ud;
    if (ok) { setAtom(gCartCount, js_get_int(body, "count")); invalidate("cart"); }
}
static void onOrderPlaced(i32 ok, i32 ec, i32 status, const char *body, void *ud) {
    (void)ec; (void)status; (void)ud;
    if (ok) {
        char id[32]; js_get_str(body, "id", id, sizeof id);
        setAtom(gLastOrder, id);
        setAtom(gCartCount, 0);
        invalidate("cart");
        go(V_CONFIRM);
        logf("[shop] order placed: %s", id);
    }
}

/* ---------------------------------------------------------------- styles */
static const char *shopCss =
    ".shop{font-family:system-ui,-apple-system,'Segoe UI',sans-serif;color:#211d1a;"
    "background:#f7f4ef;min-height:100vh;-webkit-font-smoothing:antialiased}"
    ".wrap{max-width:1160px;margin:0 auto;padding:0 28px}"
    /* top bar */
    ".nav{position:sticky;top:0;z-index:20;background:rgba(247,244,239,.82);"
    "backdrop-filter:blur(14px);border-bottom:1px solid #e6ded2}"
    ".nav .wrap{display:flex;align-items:center;gap:10px;height:68px}"
    ".brand{font-weight:800;font-size:21px;letter-spacing:.02em;cursor:pointer;color:#211d1a}"
    ".brand .dot{color:#a24e2a}"
    ".spacer{flex:1}"
    ".navlink{font-size:14px;color:#6b625a;cursor:pointer;padding:8px 12px;border-radius:9px;"
    "transition:background .15s,color .15s;font-weight:500}"
    ".navlink:hover{background:#ede6db;color:#211d1a}"
    ".cartbtn{position:relative;font-size:14px;font-weight:600;cursor:pointer;padding:9px 18px;"
    "border-radius:11px;background:#211d1a;color:#fff;transition:background .15s}"
    ".cartbtn:hover{background:#3c352f}"
    ".badge{position:absolute;top:-7px;right:-7px;background:#a24e2a;color:#fff;font-size:11px;"
    "min-width:19px;height:19px;border-radius:10px;display:flex;align-items:center;"
    "justify-content:center;padding:0 5px;font-weight:700;box-shadow:0 0 0 2px #f7f4ef}"
    /* back row */
    ".back{display:inline-flex;align-items:center;gap:6px;font-size:13px;color:#6b625a;"
    "cursor:pointer;padding:8px 0;margin-top:18px;font-weight:500}"
    ".back:hover{color:#a24e2a}"
    ".crumb{font-size:13px;color:#a39a8f;margin:18px 0 0}"
    ".crumb b{color:#6b625a;font-weight:600}"
    /* hero */
    ".hero{padding:40px 0 6px}"
    ".eyebrow{font-size:11px;text-transform:uppercase;letter-spacing:.18em;color:#a24e2a;font-weight:700}"
    ".hero h1{font-family:Georgia,'Times New Roman',serif;font-size:46px;line-height:1.04;"
    "letter-spacing:-.01em;margin:12px 0 10px;font-weight:600;color:#211d1a}"
    ".hero p{color:#6b625a;font-size:16px;max-width:540px;line-height:1.6}"
    /* chips */
    ".chips{display:flex;gap:9px;flex-wrap:wrap;margin:26px 0 8px}"
    ".chip{font-size:13px;padding:9px 18px;border-radius:999px;border:1px solid #e0d7ca;"
    "background:#fdfbf8;color:#6b625a;cursor:pointer;transition:all .15s;font-weight:500}"
    ".chip:hover{border-color:#c9bdac;color:#211d1a}"
    ".chip.active{background:#211d1a;color:#fbf8f3;border-color:#211d1a}"
    /* grid + cards */
    ".grid{display:grid;grid-template-columns:repeat(3,1fr);gap:24px;padding:14px 0 56px}"
    ".card{background:#fdfbf8;border:1px solid #ebe3d7;border-radius:14px;overflow:hidden;"
    "cursor:pointer;transition:transform .18s ease,box-shadow .18s ease,border-color .18s;"
    "display:flex;flex-direction:column}"
    ".card:hover{transform:translateY(-4px);box-shadow:0 16px 34px rgba(33,29,26,.11);border-color:#e0d7ca}"
    /* monogram tile — the product 'image' */
    ".tile{height:230px;display:flex;align-items:center;justify-content:center;position:relative;overflow:hidden}"
    ".mono{font-family:Georgia,'Times New Roman',serif;font-size:96px;font-weight:600;"
    "line-height:1;opacity:.9;user-select:none}"
    ".tile .glyph{position:absolute;bottom:12px;right:14px;font-size:22px;opacity:.5}"
    ".m-tops{background:linear-gradient(140deg,#eef1e9,#dfe6d6)}.m-tops .mono{color:#59684a}"
    ".m-bottoms{background:linear-gradient(140deg,#eaedf3,#d8dceb)}.m-bottoms .mono{color:#485069}"
    ".m-outerwear{background:linear-gradient(140deg,#f0efe4,#e2dfcd)}.m-outerwear .mono{color:#6a6644}"
    ".m-footwear{background:linear-gradient(140deg,#f2ebe2,#e6d8c8)}.m-footwear .mono{color:#6b5138}"
    ".m-accessories{background:linear-gradient(140deg,#f4ece5,#ecdccf)}.m-accessories .mono{color:#8a563a}"
    ".cbody{padding:18px;display:flex;flex-direction:column;gap:5px;flex:1}"
    ".rowtop{display:flex;justify-content:space-between;align-items:baseline}"
    ".cat{font-size:10.5px;text-transform:uppercase;letter-spacing:.13em;color:#a39a8f;font-weight:600}"
    ".featured{font-size:10px;font-weight:700;color:#a24e2a;text-transform:uppercase;letter-spacing:.1em}"
    ".name{font-weight:600;font-size:15.5px;line-height:1.35;color:#211d1a}"
    ".crow{display:flex;align-items:center;justify-content:space-between;margin-top:auto;padding-top:12px}"
    ".price{font-weight:700;font-size:16px;color:#211d1a;font-variant-numeric:tabular-nums}"
    /* buttons */
    ".btn{font-size:14px;font-weight:600;padding:11px 20px;border-radius:11px;cursor:pointer;"
    "border:1px solid #e0d7ca;background:#fdfbf8;color:#211d1a;transition:all .15s}"
    ".btn:hover{background:#f2ebe1;border-color:#d3c7b7}"
    ".btn.primary{background:#211d1a;color:#fbf8f3;border-color:#211d1a}"
    ".btn.primary:hover{background:#3c352f}"
    ".btn.accent{background:#a24e2a;color:#fff;border-color:#a24e2a}"
    ".btn.accent:hover{background:#87401f}"
    ".btn.small{padding:8px 14px;font-size:13px;border-radius:9px}"
    ".btn.block{width:100%;display:flex;justify-content:center;margin-top:10px}"
    /* detail */
    ".detail{display:grid;grid-template-columns:5fr 6fr;gap:48px;padding:8px 0 64px;align-items:start}"
    ".art{min-height:480px;border-radius:18px;display:flex;align-items:center;justify-content:center;position:relative;overflow:hidden}"
    ".art .mono{font-size:200px}"
    ".art .glyph{position:absolute;bottom:24px;right:28px;font-size:40px;opacity:.5}"
    ".dcol{padding-top:8px}"
    ".detail .cat{margin-bottom:10px}"
    ".detail h1{font-family:Georgia,serif;font-size:36px;font-weight:600;letter-spacing:-.01em;margin:6px 0 4px;color:#211d1a}"
    ".dprice{font-size:24px;font-weight:700;color:#211d1a;margin:8px 0 18px;font-variant-numeric:tabular-nums}"
    ".lead{color:#6b625a;font-size:16px;line-height:1.7;margin:0 0 24px}"
    ".meta{display:flex;gap:14px;flex-wrap:wrap;margin:0 0 28px}"
    ".pill{font-size:12.5px;color:#6b625a;background:#efe8dd;border-radius:999px;padding:6px 13px}"
    /* cart / lists */
    ".panel{background:#fdfbf8;border:1px solid #ebe3d7;border-radius:14px;padding:6px 22px;margin:18px 0}"
    ".line{display:flex;align-items:center;gap:16px;padding:18px 0;border-bottom:1px solid #efe8dd}"
    ".line:last-child{border-bottom:none}"
    ".ico{width:56px;height:56px;border-radius:11px;display:flex;align-items:center;justify-content:center;flex-shrink:0}"
    ".ico .mono{font-size:26px}"
    ".grow{flex:1;min-width:0}"
    ".qty{display:flex;align-items:center;gap:12px}"
    ".qty button{width:30px;height:30px;border-radius:8px;border:1px solid #e0d7ca;background:#fdfbf8;"
    "cursor:pointer;font-size:17px;line-height:1;color:#6b625a}"
    ".qty button:hover{background:#f2ebe1;color:#211d1a}"
    ".qnum{min-width:20px;text-align:center;font-weight:600;font-variant-numeric:tabular-nums}"
    ".summary{display:flex;align-items:center;justify-content:space-between;padding:20px 2px 8px;"
    "font-size:21px;font-weight:700;color:#211d1a}"
    ".summary .price{font-size:21px}"
    ".muted{color:#87807a;font-size:14px}"
    /* forms */
    ".form{max-width:440px;margin:44px auto;background:#fdfbf8;border:1px solid #ebe3d7;"
    "border-radius:18px;padding:36px;box-shadow:0 10px 30px rgba(33,29,26,.05)}"
    ".form h2{font-family:Georgia,serif;font-size:27px;font-weight:600;letter-spacing:-.01em;margin:0 0 4px;color:#211d1a}"
    ".field{display:flex;flex-direction:column;gap:7px;margin:18px 0}"
    ".field label{font-size:13px;color:#6b625a;font-weight:600}"
    ".field input{padding:12px 15px;border:1px solid #ddd3c5;border-radius:10px;font-size:14px;"
    "background:#fffdfa;color:#211d1a;transition:border-color .15s,box-shadow .15s}"
    ".field input:focus{outline:none;border-color:#a24e2a;box-shadow:0 0 0 3px rgba(162,78,42,.13)}"
    ".err{color:#c0392b;font-size:13px;margin:8px 0;background:#fbeae7;padding:9px 12px;border-radius:9px}"
    ".hint{font-size:12.5px;color:#a39a8f;margin-top:18px;line-height:1.8;border-top:1px solid #efe8dd;padding-top:14px}"
    ".hint b{color:#6b625a}"
    /* confirm */
    ".confirm{max-width:500px;margin:72px auto;text-align:center}"
    ".confirm .seal{width:76px;height:76px;border-radius:50%;background:#eaf0e4;color:#59684a;"
    "font-size:38px;display:flex;align-items:center;justify-content:center;margin:0 auto 22px}"
    ".confirm h1{font-family:Georgia,serif;font-size:32px;font-weight:600;color:#211d1a;margin:0 0 10px}"
    ".ordertag{font-family:ui-monospace,'SF Mono',monospace;font-weight:700;color:#a24e2a;"
    "background:#f4ece5;padding:2px 8px;border-radius:6px}"
    ".empty{text-align:center;color:#87807a;padding:72px 0;font-size:15px}"
    ".empty .btn{margin-top:18px}"
    ".spin{display:inline-block;width:26px;height:26px;border:3px solid #e6ded2;"
    "border-top-color:#a24e2a;border-radius:50%;animation:sp .7s linear infinite}"
    "@keyframes sp{to{transform:rotate(360deg)}}"
    ".grid,.detail,.confirm,.form,.panel{animation:fade .35s ease}"
    "@keyframes fade{from{opacity:0;transform:translateY(6px)}to{opacity:1;transform:none}}";

/* ---------------------------------------------------------------- catalog */
component0(CatalogView) {
    const char *cat = useAtom(gCategory);
    RpcResult q = useRpc(strf("catalog:%s", cat), "catalog", "shop.catalog.v1", "list",
                         strf("{\"Category\":\"%s\"}", cat), 0);
    Product *prods = 0;
    i32 nprods = 0;
    if (q.ok) nprods = parse_products(q.data, &prods);

    static const char *CID[6] = {"all", "tops", "bottoms", "outerwear", "footwear", "accessories"};
    static const char *CLBL[6] = {"All", "Tops", "Bottoms", "Outerwear", "Footwear", "Accessories"};

    eventI32(pick, i)  { setAtom(gCategory, CID[i]); }
    eventI32(open, i)  { setAtom(gProductId, prods[i].id); go(V_PRODUCT); }
    eventI32(add, i)   {
        if (!useAtom(gLoggedIn)) go(V_LOGIN);
        else gwbc_rpc("cart", "shop.cart.v1", "add",
                 strf("{\"ID\":\"%s\",\"Qty\":1}", prods[i].id), GWB_RPC_F_SESSION, onCartChanged, 0);
    }

    return div(
        div(cls("hero"),
            span(cls("eyebrow"), "New season"),
            h1("Considered clothing & everyday accessories"),
            p("Durable materials, honest construction, quietly good design. "
              "Free returns within 30 days.")),
        div(cls("chips"),
            map(c, CID, 6,
                button(cls(strEq(cat, *c) ? "chip active" : "chip"),
                    id(strf("chip-%s", *c)),
                    onClick(bindI32(pick, (i32)(c - CID))),
                    text("%s", CLBL[(i32)(c - CID)])))),
        IfElse(q.loading,
            div(cls("empty"), span(cls("spin")), p(cls("muted"), "Loading the collection...")),
        IfElse(q.err,
            div(cls("empty"), text("Couldn't load products (error %d).", (i32)q.errClass)),
            div(cls("grid"),
                map(p, prods, nprods,
                    div(cls("card"), id(strf("card-%s", p->id)),
                        onClick(bindI32(open, (i32)(p - prods))),
                        div(cls(strf("tile m-%s", p->category)),
                            span(cls("mono"), text("%s", initial(p->name))),
                            span(cls("glyph"), text("%s", p->image))),
                        div(cls("cbody"),
                            div(cls("rowtop"),
                                span(cls("cat"), text("%s", p->category)),
                                If(p->featured, span(cls("featured"), "Featured"))),
                            span(cls("name"), text("%s", p->name)),
                            div(cls("crow"),
                                span(cls("price"), text("%s", price_str(p->price))),
                                button(cls("btn accent small"), id(strf("add-%s", p->id)),
                                    onClick(bindI32(add, (i32)(p - prods))),
                                    "Add")))))))));
}

/* ---------------------------------------------------------------- product detail */
component0(ProductView) {
    const char *id = useAtom(gProductId);
    RpcResult q = useRpc(strf("product:%s", id), "catalog", "shop.catalog.v1", "get",
                         strf("{\"ID\":\"%s\"}", id), 0);
    Product pr = {0};
    if (q.ok) read_product(q.data, &pr);

    event(back)   { go(V_CATALOG); }
    event(addOne) {
        if (!useAtom(gLoggedIn)) go(V_LOGIN);
        else gwbc_rpc("cart", "shop.cart.v1", "add",
                 strf("{\"ID\":\"%s\",\"Qty\":1}", useAtom(gProductId)), GWB_RPC_F_SESSION,
                 onCartChanged, 0);
    }
    return IfElse(q.loading,
        div(cls("empty"), span(cls("spin"))),
        div(
            span(cls("back"), id("detail-back"), onClick(back), "\xE2\x86\x90  Back to shop"),
            div(cls("detail"),
                div(cls(strf("art m-%s", pr.category)),
                    span(cls("mono"), text("%s", initial(pr.name))),
                    span(cls("glyph"), text("%s", pr.image))),
                div(cls("dcol"),
                    div(cls("cat"), text("%s", pr.category)),
                    h1(text("%s", pr.name)),
                    div(cls("dprice"), text("%s", price_str(pr.price))),
                    p(cls("lead"), text("%s", pr.blurb)),
                    div(cls("meta"),
                        span(cls("pill"), text("%d in stock", pr.stock)),
                        span(cls("pill"), "Free shipping over $150"),
                        span(cls("pill"), "30-day returns")),
                    button(cls("btn accent block"), id("detail-add"), onClick(addOne),
                        text("Add to cart - %s", price_str(pr.price)))))));
}

/* ---------------------------------------------------------------- cart */
component0(CartView) {
    RpcResult q = useRpc("cart", "cart", "shop.cart.v1", "get", "{}", GWB_RPC_F_SESSION);
    CartLine *cart = 0;
    i32 ncart = 0, subtotal = 0;
    if (q.ok) ncart = parse_cart(q.data, &cart, &subtotal);

    event(shop)      { go(V_CATALOG); }
    event(checkout)  { go(V_CHECKOUT); }
    eventI32(inc, i) { gwbc_rpc("cart","shop.cart.v1","add",
        strf("{\"ID\":\"%s\",\"Qty\":1}", cart[i].id), GWB_RPC_F_SESSION, onCartChanged, 0); }
    eventI32(dec, i) { gwbc_rpc("cart","shop.cart.v1","setQty",
        strf("{\"ID\":\"%s\",\"Qty\":%d}", cart[i].id, cart[i].qty - 1), GWB_RPC_F_SESSION, onCartChanged, 0); }
    eventI32(del, i) { gwbc_rpc("cart","shop.cart.v1","remove",
        strf("{\"ID\":\"%s\"}", cart[i].id), GWB_RPC_F_SESSION, onCartChanged, 0); }

    return div(
        span(cls("back"), onClick(shop), "\xE2\x86\x90  Continue shopping"),
        h1(cls("hero"), "Your cart"),
        IfElse(q.loading, div(cls("empty"), span(cls("spin"))),
        IfElse(ncart == 0,
            div(cls("empty"),
                p("Your cart is empty."),
                button(cls("btn primary"), onClick(shop), "Browse the collection")),
            div(
                div(cls("panel"),
                    map(l, cart, ncart,
                        div(cls("line"),
                            div(cls(strf("ico m-%s", l->cat)),
                                span(cls("mono"), text("%s", initial(l->name)))),
                            div(cls("grow"),
                                div(cls("name"), text("%s", l->name)),
                                div(cls("muted"), text("%s each", price_str(l->lineTotal / (l->qty ? l->qty : 1))))),
                            div(cls("qty"),
                                button(onClick(bindI32(dec, (i32)(l - cart))), "-"),
                                span(cls("qnum"), text("%d", l->qty)),
                                button(onClick(bindI32(inc, (i32)(l - cart))), "+")),
                            span(cls("price"), text("%s", price_str(l->lineTotal))),
                            span(cls("navlink"), onClick(bindI32(del, (i32)(l - cart))), "Remove")))),
                div(cls("summary"), span("Subtotal"), span(cls("price"), text("%s", price_str(subtotal)))),
                p(cls("muted"), "Shipping & taxes calculated at checkout."),
                button(cls("btn accent block"), id("go-checkout"), onClick(checkout),
                    "Proceed to checkout")))));
}

/* ---------------------------------------------------------------- login */
component0(LoginView) {
    stateStr(email, "");
    stateStr(password, "");
    eventInput(setEmail, e)    { set(email, e.value); }
    eventInput(setPassword, e) { set(password, e.value); }
    event(submit) {
        setAtom(gAuthErr, "");
        gwbc_rpc("auth", "shop.auth.v1", "login",
                 strf("{\"Email\":\"%s\",\"Password\":\"%s\"}", email, password), 0, onLoginDone, 0);
    }
    const char *err = useAtom(gAuthErr);
    return div(cls("form"),
        h2("Welcome back"),
        p(cls("muted"), "Sign in to shop your cart and orders."),
        If(err[0], p(cls("err"), text("%s", err))),
        div(cls("field"), label("Email"),
            input(id("email"), type("email"), value(email), onInput(setEmail),
                placeholder("you@example.com"))),
        div(cls("field"), label("Password"),
            input(id("password"), type("password"), value(password), onInput(setPassword),
                placeholder("your password"))),
        button(cls("btn accent block"), id("login-submit"), onClick(submit), "Sign in"),
        div(cls("hint"),
            div("Demo accounts:"),
            div("shopper@aurelia.dev / shop1234"),
            div("admin@aurelia.dev / admin1234 (admin)")));
}

/* ---------------------------------------------------------------- checkout */
component0(CheckoutView) {
    stateStr(name, "");
    stateStr(addr, "");
    stateStr(city, "");
    stateStr(zip, "");
    eventInput(sn, e) { set(name, e.value); }
    eventInput(sa, e) { set(addr, e.value); }
    eventInput(sc, e) { set(city, e.value); }
    eventInput(sz, e) { set(zip, e.value); }
    event(place) {
        gwbc_rpc("orders", "shop.orders.v1", "place",
                 strf("{\"Shipping\":{\"Name\":\"%s\",\"Address\":\"%s\",\"City\":\"%s\",\"Zip\":\"%s\"}}",
                      name, addr, city, zip),
                 GWB_RPC_F_SESSION, onOrderPlaced, 0);
    }
    event(back) { go(V_CART); }
    return div(cls("form"),
        h2("Checkout"),
        p(cls("muted"), "Where should we send it?"),
        div(cls("field"), label("Full name"),
            input(id("ship-name"), value(name), onInput(sn), placeholder("Ada Lovelace"))),
        div(cls("field"), label("Address"),
            input(id("ship-addr"), value(addr), onInput(sa), placeholder("1 Analytical Way"))),
        div(cls("field"), label("City"),
            input(id("ship-city"), value(city), onInput(sc), placeholder("Miami"))),
        div(cls("field"), label("ZIP"),
            input(id("ship-zip"), value(zip), onInput(sz), placeholder("33101"))),
        button(cls("btn accent block"), id("place-order"), onClick(place), "Place order"),
        button(cls("btn block"), onClick(back), "Back to cart"));
}

/* ---------------------------------------------------------------- confirm */
component0(ConfirmView) {
    event(cont) { go(V_CATALOG); }
    return div(cls("confirm"),
        div(cls("seal"), "\xE2\x9C\x93"),
        h1("Order confirmed"),
        p(cls("muted"), "Thank you! Your order ",
            span(cls("ordertag"), text("%s", useAtom(gLastOrder))),
            " is confirmed and on its way."),
        button(cls("btn primary"), onClick(cont), "Continue shopping"));
}

/* ---------------------------------------------------------------- admin */
component0(AdminView) {
    RpcResult q = useRpc("admin:orders", "admin", "shop.admin.v1", "orders", "{}", GWB_RPC_F_SESSION);
    AdminOrder *ords = 0;
    i32 nords = 0, total = 0;
    if (q.ok) nords = parse_orders(q.data, &ords, &total);
    return div(
        h1(cls("hero"), "Admin - Orders"),
        IfElse(q.err && q.errClass == GWB_RPC_ERR_AUTHZ,
            div(cls("empty"), "Admin access required."),
        IfElse(q.loading, div(cls("empty"), span(cls("spin"))),
        IfElse(nords == 0, div(cls("empty"), "No orders yet."),
            div(
                div(cls("panel"),
                    map(o, ords, nords,
                        div(cls("line"),
                            div(cls("grow"),
                                div(cls("name"), text("%s", o->id)),
                                div(cls("muted"), text("%s", o->sub))),
                            span(cls("price"), text("%s", price_str(o->subtotal)))))),
                div(cls("summary"), span("Gross"), span(text("%s", price_str(total)))))))));
}

/* ---------------------------------------------------------------- shell */
component0(Nav) {
    event(home)   { go(V_CATALOG); }
    event(cart)   { go(useAtom(gLoggedIn) ? V_CART : V_LOGIN); }
    event(login)  { go(V_LOGIN); }
    event(admin)  { go(V_ADMIN); }
    event(logout) { gwb_session_clear(); setAtom(gLoggedIn, 0); setAtom(gUserName, "");
                    setAtom(gUserRole, ""); setAtom(gCartCount, 0); invalidate("cart"); go(V_CATALOG); }
    i32 count = useAtom(gCartCount);
    return div(cls("nav"),
        div(cls("wrap"),
            span(cls("brand"), onClick(home), "AURELIA", span(cls("dot"), ".")),
            span(cls("spacer")),
            If(strEq(useAtom(gUserRole), "admin"),
                span(cls("navlink"), onClick(admin), "Admin")),
            IfElse(useAtom(gLoggedIn),
                span(cls("navlink"), id("nav-logout"), onClick(logout),
                    text("Hi, %s - Sign out", useAtom(gUserName))),
                span(cls("navlink"), id("nav-login"), onClick(login), "Sign in")),
            span(cls("cartbtn"), id("nav-cart"), onClick(cart), "Cart",
                If(count > 0, span(cls("badge"), text("%d", count))))));
}

typedef struct { i32 _unused; } ShopAppProps;
component(ShopApp, props, ShopAppProps) {
    (void)props;
    appCss(shopCss);
    i32 v = useAtom(gView);
    return div(cls("shop"),
        Nav(),
        div(cls("wrap"),
            IfElse(v == V_CATALOG, CatalogView(),
            IfElse(v == V_PRODUCT, ProductView(),
            IfElse(v == V_CART, CartView(),
            IfElse(v == V_LOGIN, LoginView(),
            IfElse(v == V_CHECKOUT, CheckoutView(),
            IfElse(v == V_CONFIRM, ConfirmView(),
            AdminView()))))))));
}

app(ShopApp, {0});
