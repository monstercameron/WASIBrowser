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
#define MAX_PRODUCTS 48
static Product g_prods[MAX_PRODUCTS];
static i32 g_prodCount;

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
static void parse_products(const char *json) {
    const char *arr = js_find(json, "products");
    g_prodCount = 0;
    for (const char *e = js_arr_first(arr); e && g_prodCount < MAX_PRODUCTS; e = js_arr_next(e))
        read_product(e, &g_prods[g_prodCount++]);
}

typedef struct { char id[48], name[72], img[12]; i32 qty, lineTotal; } CartLine;
static CartLine g_cart[32];
static i32 g_cartN, g_cartSubtotal;
static void parse_cart(const char *json) {
    g_cartN = 0;
    g_cartSubtotal = js_get_int(json, "subtotal");
    const char *items = js_find(json, "items");
    for (const char *e = js_arr_first(items); e && g_cartN < 32; e = js_arr_next(e)) {
        CartLine *l = &g_cart[g_cartN++];
        const char *prod = js_find(e, "product");
        js_get_str(prod, "id", l->id, sizeof l->id);
        js_get_str(prod, "name", l->name, sizeof l->name);
        js_get_str(prod, "image", l->img, sizeof l->img);
        l->qty = js_get_int(e, "qty");
        l->lineTotal = js_get_int(e, "lineTotal");
    }
}

typedef struct { char id[32], sub[64]; i32 subtotal; } AdminOrder;
static AdminOrder g_ord[48];
static i32 g_ordN, g_ordTotal;
static void parse_orders(const char *json) {
    g_ordN = 0; g_ordTotal = 0;
    const char *arr = js_find(json, "orders");
    for (const char *e = js_arr_first(arr); e && g_ordN < 48; e = js_arr_next(e)) {
        AdminOrder *o = &g_ord[g_ordN++];
        js_get_str(e, "id", o->id, sizeof o->id);
        js_get_str(e, "sub", o->sub, sizeof o->sub);
        o->subtotal = js_get_int(e, "subtotal");
        g_ordTotal += o->subtotal;
    }
}

/* gwbc strf is a mini-printf (%s %d %% only) — pad the cents manually. */
static const char *price_str(i32 cents) {
    i32 c = cents % 100;
    return strf("$%d.%d%d", cents / 100, c / 10, c % 10);
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
    ".shop{font-family:system-ui,-apple-system,'Segoe UI',sans-serif;color:#1c1917;"
    "background:#faf9f7;min-height:100vh}"
    ".wrap{max-width:1120px;margin:0 auto;padding:0 24px}"
    ".nav{position:sticky;top:0;z-index:10;background:rgba(250,249,247,.86);"
    "backdrop-filter:blur(10px);border-bottom:1px solid #e7e2da}"
    ".nav .wrap{display:flex;align-items:center;gap:8px;height:64px}"
    ".brand{font-weight:800;font-size:20px;letter-spacing:-.02em;cursor:pointer}"
    ".brand .dot{color:#b45309}"
    ".spacer{flex:1}"
    ".navlink{font-size:14px;color:#57534e;cursor:pointer;padding:6px 10px;border-radius:8px}"
    ".navlink:hover{background:#f0ece5;color:#1c1917}"
    ".cartbtn{position:relative;font-size:14px;font-weight:600;cursor:pointer;"
    "padding:8px 14px;border-radius:10px;background:#1c1917;color:#fff}"
    ".badge{position:absolute;top:-6px;right:-6px;background:#b45309;color:#fff;font-size:11px;"
    "min-width:18px;height:18px;border-radius:9px;display:flex;align-items:center;"
    "justify-content:center;padding:0 4px;font-weight:700}"
    ".hero{padding:44px 0 8px}"
    ".eyebrow{font-size:12px;text-transform:uppercase;letter-spacing:.14em;color:#b45309;font-weight:700}"
    ".hero h1{font-size:40px;line-height:1.06;letter-spacing:-.03em;margin:10px 0 8px;font-weight:800}"
    ".hero p{color:#57534e;font-size:16px;max-width:560px}"
    ".chips{display:flex;gap:8px;flex-wrap:wrap;margin:22px 0}"
    ".chip{font-size:13px;padding:8px 16px;border-radius:999px;border:1px solid #e7e2da;"
    "background:#fff;color:#57534e;cursor:pointer;transition:all .15s}"
    ".chip:hover{border-color:#cfc7ba}"
    ".chip.active{background:#1c1917;color:#fff;border-color:#1c1917}"
    ".grid{display:grid;grid-template-columns:repeat(3,1fr);gap:20px;padding:8px 0 48px}"
    ".card{background:#fff;border:1px solid #ece7df;border-radius:16px;overflow:hidden;"
    "cursor:pointer;transition:transform .15s,box-shadow .15s;display:flex;flex-direction:column}"
    ".card:hover{transform:translateY(-3px);box-shadow:0 12px 28px rgba(28,25,23,.10)}"
    ".thumb{font-size:64px;height:176px;display:flex;align-items:center;justify-content:center;"
    "background:linear-gradient(135deg,#f5f1ea,#efe8dd)}"
    ".cbody{padding:16px;display:flex;flex-direction:column;gap:5px;flex:1}"
    ".cat{font-size:11px;text-transform:uppercase;letter-spacing:.1em;color:#a8a29e}"
    ".name{font-weight:600;font-size:15px;line-height:1.3}"
    ".crow{display:flex;align-items:center;justify-content:space-between;margin-top:auto;padding-top:10px}"
    ".price{font-weight:700;font-size:16px}"
    ".featured{font-size:10px;font-weight:700;color:#b45309;text-transform:uppercase;letter-spacing:.08em}"
    ".btn{font-size:14px;font-weight:600;padding:10px 18px;border-radius:10px;cursor:pointer;"
    "border:1px solid #e7e2da;background:#fff;color:#1c1917;transition:all .15s}"
    ".btn:hover{background:#f5f1ea}"
    ".btn.primary{background:#1c1917;color:#fff;border-color:#1c1917}"
    ".btn.primary:hover{background:#3a3531}"
    ".btn.accent{background:#b45309;color:#fff;border-color:#b45309}"
    ".btn.accent:hover{background:#92400e}"
    ".btn.block{width:100%;display:flex;justify-content:center;margin-top:8px}"
    ".detail{display:grid;grid-template-columns:1fr 1fr;gap:40px;padding:24px 0 56px}"
    ".art{font-size:140px;min-height:420px;display:flex;align-items:center;justify-content:center;"
    "background:linear-gradient(135deg,#f5f1ea,#efe8dd);border-radius:20px}"
    ".detail h1{font-size:32px;letter-spacing:-.02em;margin:6px 0}"
    ".lead{color:#57534e;font-size:16px;line-height:1.6;margin:14px 0}"
    ".meta{display:flex;gap:20px;color:#78716c;font-size:13px;margin:12px 0 24px}"
    ".panel{background:#fff;border:1px solid #ece7df;border-radius:16px;padding:4px 20px;margin:20px 0}"
    ".line{display:flex;align-items:center;gap:16px;padding:18px 0;border-bottom:1px solid #f0ece5}"
    ".ico{font-size:36px;width:60px;height:60px;display:flex;align-items:center;justify-content:center;"
    "background:#f5f1ea;border-radius:12px}"
    ".grow{flex:1}"
    ".qty{display:flex;align-items:center;gap:10px}"
    ".qty button{width:30px;height:30px;border-radius:8px;border:1px solid #e7e2da;background:#fff;"
    "cursor:pointer;font-size:16px;line-height:1}"
    ".summary{display:flex;align-items:center;justify-content:space-between;padding:18px 0 6px;"
    "font-size:20px;font-weight:700}"
    ".muted{color:#78716c;font-size:14px}"
    ".form{max-width:420px;margin:40px auto;background:#fff;border:1px solid #ece7df;"
    "border-radius:18px;padding:32px}"
    ".form h2{font-size:24px;letter-spacing:-.02em;margin:0 0 4px}"
    ".field{display:flex;flex-direction:column;gap:6px;margin:16px 0}"
    ".field label{font-size:13px;color:#57534e;font-weight:500}"
    ".field input{padding:11px 14px;border:1px solid #ddd6cc;border-radius:10px;font-size:14px;background:#fdfcfa}"
    ".field input:focus{outline:none;border-color:#b45309;box-shadow:0 0 0 3px rgba(180,83,9,.12)}"
    ".err{color:#dc2626;font-size:13px;margin:6px 0}"
    ".hint{font-size:12px;color:#a8a29e;margin-top:16px;line-height:1.7}"
    ".confirm{max-width:480px;margin:60px auto;text-align:center}"
    ".big{font-size:56px}"
    ".ordertag{font-family:ui-monospace,monospace;font-weight:700;color:#b45309}"
    ".empty{text-align:center;color:#78716c;padding:60px 0}"
    ".spin{display:inline-block;width:22px;height:22px;border:3px solid #e7e2da;"
    "border-top-color:#b45309;border-radius:50%;animation:sp .7s linear infinite}"
    "@keyframes sp{to{transform:rotate(360deg)}}";

/* ---------------------------------------------------------------- catalog */
component0(CatalogView) {
    const char *cat = useAtom(gCategory);
    RpcResult q = useRpc(strf("catalog:%s", cat), "catalog", "shop.catalog.v1", "list",
                         strf("{\"Category\":\"%s\"}", cat), 0);
    if (q.ok) parse_products(q.data);

    static const char *CID[6] = {"all", "tops", "bottoms", "outerwear", "footwear", "accessories"};
    static const char *CLBL[6] = {"All", "Tops", "Bottoms", "Outerwear", "Footwear", "Accessories"};

    eventI32(pick, i)  { setAtom(gCategory, CID[i]); }
    eventI32(open, i)  { setAtom(gProductId, g_prods[i].id); go(V_PRODUCT); }
    eventI32(add, i)   {
        if (!useAtom(gLoggedIn)) go(V_LOGIN);
        else gwbc_rpc("cart", "shop.cart.v1", "add",
                 strf("{\"ID\":\"%s\",\"Qty\":1}", g_prods[i].id), GWB_RPC_F_SESSION, onCartChanged, 0);
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
                    onClick(bindI32(pick, (i32)(c - CID))),
                    text("%s", CLBL[(i32)(c - CID)])))),
        IfElse(q.loading,
            div(cls("empty"), span(cls("spin")), p(cls("muted"), "Loading the collection...")),
        IfElse(q.err,
            div(cls("empty"), text("Couldn't load products (error %d).", (i32)q.errClass)),
            div(cls("grid"),
                map(p, g_prods, g_prodCount,
                    div(cls("card"), id(strf("card-%s", p->id)),
                        onClick(bindI32(open, (i32)(p - g_prods))),
                        div(cls("thumb"), text("%s", p->image)),
                        div(cls("cbody"),
                            span(cls("cat"), text("%s", p->category)),
                            span(cls("name"), text("%s", p->name)),
                            If(p->featured, span(cls("featured"), "Featured")),
                            div(cls("crow"),
                                span(cls("price"), text("%s", price_str(p->price))),
                                button(cls("btn accent"), id(strf("add-%s", p->id)),
                                    onClick(bindI32(add, (i32)(p - g_prods))),
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
        div(cls("detail"),
            div(cls("art"), text("%s", pr.image)),
            div(
                span(cls("navlink"), onClick(back), "< Back to shop"),
                div(cls("cat"), text("%s", pr.category)),
                h1(text("%s", pr.name)),
                div(cls("summary"), text("%s", price_str(pr.price))),
                p(cls("lead"), text("%s", pr.blurb)),
                div(cls("meta"),
                    span(text("%d in stock", pr.stock)),
                    span("Free shipping over $150"),
                    span("30-day returns")),
                button(cls("btn accent block"), id("detail-add"), onClick(addOne),
                    text("Add to cart - %s", price_str(pr.price))))));
}

/* ---------------------------------------------------------------- cart */
component0(CartView) {
    RpcResult q = useRpc("cart", "cart", "shop.cart.v1", "get", "{}", GWB_RPC_F_SESSION);
    if (q.ok) parse_cart(q.data);

    event(shop)      { go(V_CATALOG); }
    event(checkout)  { go(V_CHECKOUT); }
    eventI32(inc, i) { gwbc_rpc("cart","shop.cart.v1","add",
        strf("{\"ID\":\"%s\",\"Qty\":1}", g_cart[i].id), GWB_RPC_F_SESSION, onCartChanged, 0); }
    eventI32(dec, i) { gwbc_rpc("cart","shop.cart.v1","setQty",
        strf("{\"ID\":\"%s\",\"Qty\":%d}", g_cart[i].id, g_cart[i].qty - 1), GWB_RPC_F_SESSION, onCartChanged, 0); }
    eventI32(del, i) { gwbc_rpc("cart","shop.cart.v1","remove",
        strf("{\"ID\":\"%s\"}", g_cart[i].id), GWB_RPC_F_SESSION, onCartChanged, 0); }

    return div(
        h1(cls("hero"), "Your cart"),
        IfElse(q.loading, div(cls("empty"), span(cls("spin"))),
        IfElse(g_cartN == 0,
            div(cls("empty"),
                p("Your cart is empty."),
                button(cls("btn primary"), onClick(shop), "Browse the collection")),
            div(
                div(cls("panel"),
                    map(l, g_cart, g_cartN,
                        div(cls("line"),
                            div(cls("ico"), text("%s", l->img)),
                            div(cls("grow"),
                                div(cls("name"), text("%s", l->name)),
                                div(cls("muted"), text("%s each", price_str(l->lineTotal / (l->qty ? l->qty : 1))))),
                            div(cls("qty"),
                                button(onClick(bindI32(dec, (i32)(l - g_cart))), "-"),
                                span(text("%d", l->qty)),
                                button(onClick(bindI32(inc, (i32)(l - g_cart))), "+")),
                            span(cls("price"), text("%s", price_str(l->lineTotal))),
                            span(cls("navlink"), onClick(bindI32(del, (i32)(l - g_cart))), "Remove")))),
                div(cls("summary"), span("Subtotal"), span(text("%s", price_str(g_cartSubtotal)))),
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
        div(cls("big"), "\xE2\x9C\x85"),
        h1("Order confirmed"),
        p(cls("muted"), "Thank you! Your order ",
            span(cls("ordertag"), text("%s", useAtom(gLastOrder))),
            " is confirmed and on its way."),
        button(cls("btn primary"), onClick(cont), "Continue shopping"));
}

/* ---------------------------------------------------------------- admin */
component0(AdminView) {
    RpcResult q = useRpc("admin:orders", "admin", "shop.admin.v1", "orders", "{}", GWB_RPC_F_SESSION);
    if (q.ok) parse_orders(q.data);
    return div(
        h1(cls("hero"), "Admin - Orders"),
        IfElse(q.err && q.errClass == GWB_RPC_ERR_AUTHZ,
            div(cls("empty"), "Admin access required."),
        IfElse(q.loading, div(cls("empty"), span(cls("spin"))),
        IfElse(g_ordN == 0, div(cls("empty"), "No orders yet."),
            div(
                div(cls("panel"),
                    map(o, g_ord, g_ordN,
                        div(cls("line"),
                            div(cls("grow"),
                                div(cls("name"), text("%s", o->id)),
                                div(cls("muted"), text("%s", o->sub))),
                            span(cls("price"), text("%s", price_str(o->subtotal)))))),
                div(cls("summary"), span("Gross"), span(text("%s", price_str(g_ordTotal)))))))));
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
